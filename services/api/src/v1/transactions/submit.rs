use crate::{
    errors::{AppError, AppResult},
    extractors::ReadOnlyDatabaseConnection,
    lib::AppState,
    response::ResponseWithHeaders,
};
use axum::http::{HeaderMap, HeaderValue};
use axum::{extract::State, http::StatusCode, Json};
use diesel::query_dsl::methods::FilterDsl;
use diesel::query_dsl::methods::SelectDsl;
use diesel::{ExpressionMethods, OptionalExtension};
use diesel_async::RunQueryDsl;
use postgres_models::models::{NewTransactionQueue, TransactionQueue};
use postgres_models::schema::rate_limits::dsl as rate_limits_dsl;
use postgres_models::schema::transaction_queue;
use redis_cache::{QueueManager, RateLimitResult, RateLimiter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ACCOUNT_ID_MAX_CHARS: usize = 255;
const JSON_MAX_BYTES: usize = 1024 * 1024; // 1MB
const PRIORITY_MIN: i32 = -1000;
const PRIORITY_MAX: i32 = 1000;

const DEFAULT_MAX_REQUESTS: u32 = 100;
const DEFAULT_WINDOW_SECONDS: u64 = 60;

pub const RL_BUCKET_SUBMIT_TX: &str = "api:v1:transactions:submit";
pub const PRIORITY_QUEUE_BASE: &str = "transactions";

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitTransactionRequest {
    pub account_id: String,
    pub transaction_data: serde_json::Value,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct SubmitTransactionResponse {
    pub transaction_id: Uuid,
    pub queue_position: i64,
    pub estimated_processing_time_seconds: i64,
    pub status: String,
}

fn construct_rate_limit_headers(limit: u32, rate_limit_result: &RateLimitResult) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        "X-RateLimit-Limit",
        HeaderValue::from_str(&limit.to_string()).unwrap(),
    );
    h.insert(
        "X-RateLimit-Remaining",
        HeaderValue::from_str(&rate_limit_result.remaining.to_string()).unwrap(),
    );
    h.insert(
        "X-RateLimit-Reset",
        HeaderValue::from_str(&rate_limit_result.reset_at.to_string()).unwrap(),
    );
    h
}

#[allow(clippy::result_large_err)]
fn validate_submit_request(request: &SubmitTransactionRequest) -> AppResult<()> {
    let account_id = request.account_id.trim();
    let account_id_char_count = account_id.chars().count();
    if account_id_char_count == 0 || account_id_char_count > ACCOUNT_ID_MAX_CHARS {
        return Err(AppError::bad_request(format!(
            "Invalid account_id: must be 1-{ACCOUNT_ID_MAX_CHARS} characters"
        )));
    }

    if request.transaction_data.is_null() {
        return Err(AppError::bad_request("transaction_data cannot be null"));
    }

    match serde_json::to_vec(&request.transaction_data) {
        Ok(buf) => {
            if buf.len() > JSON_MAX_BYTES {
                return Err(AppError::payload_too_large(format!(
                    "transaction_data is too large (max {} bytes).",
                    JSON_MAX_BYTES
                )));
            }
        }
        Err(_) => {
            return Err(AppError::bad_request(
                "transaction_data contains unsupported or non-serializable values.",
            ));
        }
    }

    if let Some(p) = request.priority {
        if !(PRIORITY_MIN..=PRIORITY_MAX).contains(&p) {
            return Err(AppError::bad_request(format!(
                "Invalid priority: must be between -{PRIORITY_MIN} and {PRIORITY_MAX}.",
            )));
        }
    }

    Ok(())
}

async fn get_rate_limit_props<C: diesel_async::AsyncConnection<Backend = diesel::pg::Pg>>(
    db: &mut C,
    account_id: &str,
    limit_type: &str,
) -> Result<(u32, u64), AppError> {
    let row: Option<(i32, i32)> = rate_limits_dsl::rate_limits
        .filter(rate_limits_dsl::account_id.eq(account_id))
        .filter(rate_limits_dsl::limit_type.eq(limit_type))
        .select((
            rate_limits_dsl::max_requests,
            rate_limits_dsl::window_seconds,
        ))
        .first::<(i32, i32)>(db)
        .await
        .optional()
        .map_err(|e| {
            tracing::error!(
                error = ?e,
                account_id = %account_id,
                limit_type = %limit_type,
                "DB error reading rate_limits"
            );
            AppError::internal_server_error("reading db failed")
        })?;

    match row {
        Some((mr, ws)) => Ok((mr.max(1) as u32, ws.max(1) as u64)),
        None => Ok((DEFAULT_MAX_REQUESTS, DEFAULT_WINDOW_SECONDS)),
    }
}

async fn insert_transaction<C>(
    db: &mut C,
    request: &SubmitTransactionRequest,
    transaction_id: Uuid,
) -> Result<TransactionQueue, AppError>
where
    C: diesel_async::AsyncConnection<Backend = diesel::pg::Pg>,
{
    let new_transaction = NewTransactionQueue::new(
        transaction_id,
        request.account_id.clone(),
        request.transaction_data.clone(),
    );
    let transaction = diesel::insert_into(transaction_queue::table)
        .values(&new_transaction)
        .get_result::<TransactionQueue>(db)
        .await
        .map_err(|e| {
            tracing::error!(error=?e, "DB insert into transaction_queue table failed");
            AppError::internal_server_error("db insertion failed")
        })?;
    Ok(transaction)
}

async fn enqueue_and_position(
    redis_pool: deadpool_redis::Pool,
    tx_id: Uuid,
    priority: i32,
) -> Result<i64, AppError> {
    let qm = QueueManager::new(redis_pool);
    qm.enqueue_with_priority_pipe(PRIORITY_QUEUE_BASE, &tx_id.to_string(), priority)
        .await
        .map_err(|e| {
            tracing::error!(
                error = ?e,
                tx_id = %tx_id,
                queue = %PRIORITY_QUEUE_BASE,
                "Redis enqueue_with_priority failed"
            );
            AppError::internal_server_error("tx enqueing failed")
        })
}

async fn apply_rate_limiting<C: diesel_async::AsyncConnection<Backend = diesel::pg::Pg>>(
    redis_pool: deadpool_redis::Pool,
    db_conn: &mut C,
    request: &SubmitTransactionRequest,
) -> Result<(RateLimitResult, HeaderMap), AppError> {
    let rate_limiter = RateLimiter::new(redis_pool);
    let (max_requests, window_seconds) =
        get_rate_limit_props(db_conn, &request.account_id, RL_BUCKET_SUBMIT_TX).await?;
    let redis_key = format!("rl:{}:{}", request.account_id, RL_BUCKET_SUBMIT_TX);
    let rate_limit_result = rate_limiter
        .check_rate_limit_pipe(&redis_key, max_requests, window_seconds)
        .await?;
    let rate_limit_headers = construct_rate_limit_headers(max_requests, &rate_limit_result);
    if !rate_limit_result.allowed {
        return Err(AppError::too_many_requests("Rate limit exceeded")
            .with_headers(rate_limit_headers.clone()));
    }
    Ok((rate_limit_result, rate_limit_headers))
}

/// Submit a transaction to the queue
///
/// This is the main endpoint that candidates need to implement.
/// It should handle high-performance transaction queuing with proper
/// rate limiting, validation, and queue management.
///
/// Expected Performance: <100ms p99 latency, 10k+ concurrent requests
///
/// TODO: Implement the following steps in order:
///
/// Step 1: INPUT VALIDATION (Security Critical)
/// - Validate account_id: non-empty, reasonable length (< 255 chars)
/// - Validate transaction_data: not null, reasonable size (< 1MB)
/// - Validate priority: if provided, should be reasonable range (-1000 to 1000)
/// - Return 400 Bad Request for invalid input with descriptive errors
///
/// Step 2: RATE LIMITING (Performance Critical)
/// - Get rate limiter from state: &state.redis_pool
/// - Use libs/redis_cache/src/rate_limiter.rs::RateLimiter::check_rate_limit()
/// - Check account-specific limits from account_rate_limits table
/// - Return 429 Too Many Requests if exceeded
/// - MUST include rate limit headers in ALL responses:
///   - X-RateLimit-Limit: requests per minute allowed
///   - X-RateLimit-Remaining: requests remaining in current window
///   - X-RateLimit-Reset: timestamp when window resets
///
/// Step 3: DATABASE PERSISTENCE (Reliability Critical)
/// - Create NewTransactionQueue using libs/postgres_models/src/models.rs
/// - Generate UUID for transaction_id using Uuid::new_v4()
/// - Set created_at to current UTC timestamp
/// - Set status to "pending"
/// - Insert into transaction_queue table using diesel
/// - Handle database errors gracefully (return 500 Internal Server Error)
///
/// Step 4: QUEUE MANAGEMENT (Business Logic Critical)
/// - Use libs/redis_cache/src/queue_manager.rs::QueueManager
/// - Add transaction to Redis queue with priority
/// - Get current queue position considering priority ordering
/// - Higher priority numbers should be processed first
/// - Use Redis sorted sets for efficient priority queue
///
/// Step 5: RESPONSE CALCULATION
/// - Calculate estimated_processing_time_seconds:
///   - Base time: 30 seconds per transaction
///   - Multiply by queue position ahead of current transaction
///   - Cap at reasonable maximum (e.g., 3600 seconds)
/// - Return proper JSON response with all fields
///
/// Step 6: ERROR HANDLING
/// - All database errors should return 500 with generic message
/// - All Redis errors should return 500 with generic message  
/// - Invalid input should return 400 with specific validation errors
/// - Rate limiting should return 429 with retry information
/// - Log all errors for debugging but don't expose internals to client
///
/// PERFORMANCE REQUIREMENTS:
/// - This endpoint MUST handle 10,000+ concurrent requests
/// - p99 latency MUST be under 100ms
/// - Success rate MUST be >99% under normal load
/// - Use connection pooling efficiently (don't hold connections unnecessarily)
/// - Use prepared statements for database operations
///
/// SECURITY REQUIREMENTS:
/// - NO authentication required (this is intentional for the exercise)
/// - Validate ALL input thoroughly
/// - Prevent JSON injection attacks
/// - Don't expose internal error details
/// - Log security-relevant events
pub async fn handler(
    State(state): State<AppState>,
    ReadOnlyDatabaseConnection(mut db_read_conn): ReadOnlyDatabaseConnection,
    Json(request): Json<SubmitTransactionRequest>,
) -> AppResult<ResponseWithHeaders<Json<SubmitTransactionResponse>>> {
    // 1. Valdate the request
    validate_submit_request(&request)?;

    // 2. Rate limiting with READ-ONLY connection (fast)
    let (_, rate_limit_headers) =
        apply_rate_limiting(state.redis_pool.clone(), &mut db_read_conn, &request).await?;

    // Relase the connection immediately after use
    drop(db_read_conn);

    // 3. Generate UUID and queue position (fast)
    let transaction_id = Uuid::new_v4();
    let priority = request.priority.unwrap_or(0);
    let queue_position =
        enqueue_and_position(state.redis_pool.clone(), transaction_id, priority).await?;

    // 4. Background write (non-blocking)
    let db_pool = state.db_pool.clone();
    let request_clone = request.clone();
    tokio::spawn(async move {
        if let Ok(mut write_conn) = db_pool.get_owned().await {
            if let Err(e) =
                insert_transaction(&mut write_conn, &request_clone, transaction_id).await
            {
                tracing::error!("Background DB insert failed for {}: {}", transaction_id, e);
            }
        } else {
            tracing::error!(
                "Failed to get DB connection for background insert: {}",
                transaction_id
            );
        }
    });

    // 5: Success WITH rate-limit headers
    let resp = SubmitTransactionResponse {
        transaction_id,
        queue_position,
        estimated_processing_time_seconds: (queue_position.saturating_sub(1) * 30).min(3600),
        status: "pending".to_string(),
    };

    Ok(ResponseWithHeaders::new(
        StatusCode::OK,
        rate_limit_headers,
        Json(resp),
    ))
}
