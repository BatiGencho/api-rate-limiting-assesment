use deadpool_redis::{
    redis::{pipe, AsyncCommands},
    Config, Pool, Runtime,
};

pub type RedisPool = Pool;
pub type RedisConnection = deadpool_redis::Connection;

const TS_EPS_SCALE: f64 = 1e15;

#[derive(Debug, thiserror::Error)]
pub enum RedisError {
    #[error("Redis pool error: {0}")]
    Pool(#[from] deadpool_redis::PoolError),

    #[error("Redis error: {0}")]
    Redis(#[from] deadpool_redis::redis::RedisError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub async fn create_pool(redis_url: &str) -> Result<RedisPool, RedisError> {
    let cfg = Config::from_url(redis_url);
    let pool = cfg
        .builder()
        .map_err(|e| RedisError::Config(e.to_string()))?
        .max_size(200)
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(|e| RedisError::Config(e.to_string()))?;
    Ok(pool)
}

pub struct RateLimiter {
    pool: RedisPool,
}

impl RateLimiter {
    pub fn new(pool: RedisPool) -> Self {
        Self { pool }
    }

    pub async fn check_rate_limit_pipe(
        &self,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> Result<RateLimitResult, RedisError> {
        let mut conn = self.pool.get().await?;
        // Use Redis TIME to stay consistent
        let (sec, usec): (i64, i64) = deadpool_redis::redis::cmd("TIME")
            .query_async(&mut *conn)
            .await?;
        let now_ms = sec * 1000 + (usec / 1000);
        let window_start_ns = (now_ms - (window_seconds as i64 * 1000)) * 1_000_000;
        let current_ns = sec * 1_000_000_000 + usec * 1000;
        let rate_limit_key = format!("rate_limit:{}", key);

        let mut p = pipe();
        p.atomic()
            .cmd("ZREMRANGEBYSCORE")
            .arg(&rate_limit_key)
            .arg(0.0)
            .arg(window_start_ns)
            .ignore()
            .cmd("ZADD")
            .arg(&rate_limit_key)
            .arg(current_ns)
            .arg(current_ns.to_string())
            .ignore()
            .cmd("ZCOUNT")
            .arg(&rate_limit_key)
            .arg(window_start_ns)
            .arg(current_ns)
            .cmd("EXPIRE")
            .arg(&rate_limit_key)
            .arg(window_seconds as i64)
            .ignore();

        let (count,): (i64,) = p.query_async(&mut *conn).await?;

        let allowed = count <= max_requests as i64;
        let remaining = if allowed {
            (max_requests as i64 - count).max(0) as u32
        } else {
            0
        };
        Ok(RateLimitResult {
            allowed,
            remaining,
            reset_at: (sec as u64) + window_seconds,
        })
    }

    pub async fn check_rate_limit(
        &self,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> Result<RateLimitResult, RedisError> {
        let mut conn = self.pool.get().await?;
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let window_start = current_time - (window_seconds * 1000);
        let window_start_nanos = (window_start * 1_000_000) as f64;
        let rate_limit_key = format!("rate_limit:{}", key);

        // Remove old entries from sorted set
        let _: i32 = deadpool_redis::redis::cmd("ZREMRANGEBYSCORE")
            .arg(&rate_limit_key)
            .arg(0.0)
            .arg(window_start_nanos)
            .query_async(&mut *conn)
            .await?;

        // Add new request first with unique score to handle concurrent requests
        // Use nanoseconds instead of milliseconds for better uniqueness
        let current_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as f64;
        let _: i32 = conn
            .zadd(&rate_limit_key, current_nanos, current_nanos)
            .await?;

        // Count current requests in window (including the one we just added)
        // Use the current nanos time that we just added to ensure consistency
        let count: i32 = conn
            .zcount(&rate_limit_key, window_start_nanos, current_nanos)
            .await?;

        if count > max_requests as i32 {
            return Ok(RateLimitResult {
                allowed: false,
                remaining: 0,
                reset_at: (current_time + (window_seconds * 1000)) / 1000,
            });
        }
        let _: bool = conn.expire(&rate_limit_key, window_seconds as i64).await?;

        Ok(RateLimitResult {
            allowed: true,
            remaining: (max_requests as i32 - count).max(0) as u32,
            reset_at: (current_time + (window_seconds * 1000)) / 1000,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_at: u64,
}

pub struct QueueManager {
    pool: RedisPool,
}

impl QueueManager {
    pub fn new(pool: RedisPool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(&self, queue_name: &str, data: &str) -> Result<i64, RedisError> {
        let mut conn = self.pool.get().await?;
        let position: i64 = conn.rpush(queue_name, data).await?;
        Ok(position)
    }

    /// Enqueue with priority - higher priority number = processed first
    /// Returns the queue *size* after insert (1-based, unique across submissions)
    pub async fn enqueue_with_priority_pipe(
        &self,
        queue_name: &str,
        data: &str,
        priority: i32,
    ) -> Result<i64, RedisError> {
        let mut conn = self.pool.get().await?;
        let priority_queue_name = format!("{}_priority", queue_name);

        // Tie-breaker (FIFO within same priority)
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as f64;

        // Score calculation: higher priority = lower score (processed first)
        // Timestamp scaled to avoid affecting priority ordering.
        let score = (1000 - priority) as f64 + (timestamp / TS_EPS_SCALE);

        // Pipeline: ZADD + ZRANK in one round-trip, wrapped in MULTI/EXEC
        let mut p = pipe();
        p.atomic()
            .zadd(&priority_queue_name, data, score)
            .zrank(&priority_queue_name, data);

        let (_zadd_result, rank): (i32, Option<i64>) = p.query_async(&mut *conn).await?;
        Ok(rank.map(|r| r + 1).unwrap_or(1))
    }

    /// Enqueue with priority - higher priority number = processed first
    pub async fn enqueue_with_priority(
        &self,
        queue_name: &str,
        data: &str,
        priority: i32,
    ) -> Result<i64, RedisError> {
        let mut conn = self.pool.get().await?;
        let priority_queue_name = format!("{}_priority", queue_name);

        // Tie-breaker (FIFO within same priority)
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as f64;

        // Score calculation: higher priority = lower score (processed first)
        // Timestamp scaled to avoid affecting priority ordering.
        let score = (1000 - priority) as f64 + (timestamp / TS_EPS_SCALE);

        // Non-atimic version:
        // With AsyncCommands::zadd the order is (key, member, score), not (key, score, member).
        let _zadd_result: i32 = conn.zadd(&priority_queue_name, data, score).await?;

        // Same connection for rank → 1-based position
        Self::get_priority_position_with_conn(&mut conn, &priority_queue_name, data).await
    }

    /// Get position in priority queue (1-indexed)
    async fn get_priority_position_with_conn(
        conn: &mut RedisConnection,
        priority_queue_name: &str,
        data: &str,
    ) -> Result<i64, RedisError> {
        let rank: Option<i64> = conn.zrank(priority_queue_name, data).await?;
        Ok(rank.map(|r| r + 1).unwrap_or(1))
    }

    /// Get position in priority queue (1-indexed).
    /// Keeps the old public API, but this one opens a new connection.
    /// Prefer the `_with_conn` variant inside methods that already have a connection.
    pub async fn get_priority_position(
        &self,
        priority_queue_name: &str,
        data: &str,
    ) -> Result<i64, RedisError> {
        let mut conn = self.pool.get().await?;
        Self::get_priority_position_with_conn(&mut conn, priority_queue_name, data).await
    }

    pub async fn priority_queue_length(&self, queue_name: &str) -> Result<i64, RedisError> {
        let mut conn = self.pool.get().await?;
        let priority_queue_name = format!("{}_priority", queue_name);
        let length: i64 = conn.zcard(&priority_queue_name).await?;
        Ok(length)
    }

    pub async fn dequeue_by_priority(
        &self,
        queue_name: &str,
    ) -> Result<Option<String>, RedisError> {
        let mut conn = self.pool.get().await?;
        let priority_queue_name = format!("{}_priority", queue_name);

        // NOTE: redis-rs typically returns Vec<(member, score)>; if your version returns Vec<String>, keep it.
        // If needed, switch to: let result: Vec<(String, f64)> = conn.zpopmin(&priority_queue_name, 1).await?;
        let result: Vec<String> = conn.zpopmin(&priority_queue_name, 1).await?;

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result[0].clone()))
        }
    }

    pub async fn get_priority_queue_order(
        &self,
        queue_name: &str,
    ) -> Result<Vec<String>, RedisError> {
        let mut conn = self.pool.get().await?;
        let priority_queue_name = format!("{}_priority", queue_name);
        // Score ascending: highest priority first
        let items: Vec<String> = conn.zrange(&priority_queue_name, 0, -1).await?;
        Ok(items)
    }

    pub async fn dequeue(&self, queue_name: &str) -> Result<Option<String>, RedisError> {
        let mut conn = self.pool.get().await?;
        let result: Option<String> = conn.lpop(queue_name, None).await?;
        Ok(result)
    }

    pub async fn queue_length(&self, queue_name: &str) -> Result<i64, RedisError> {
        let mut conn = self.pool.get().await?;
        let length: i64 = conn.llen(queue_name).await?;
        Ok(length)
    }

    pub async fn get_queue_position(
        &self,
        queue_name: &str,
        data: &str,
    ) -> Result<Option<i64>, RedisError> {
        let mut conn = self.pool.get().await?;
        let items: Vec<String> = conn.lrange(queue_name, 0, -1).await?;

        for (index, item) in items.iter().enumerate() {
            if item == data {
                return Ok(Some(index as i64 + 1));
            }
        }

        Ok(None)
    }
}
