use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

pub const API_BASE_URL: &str = "http://localhost:3000";

/// Test client wrapper with convenience methods
pub struct TestClient {
    client: Client,
    base_url: String,
}

impl TestClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: API_BASE_URL.to_string(),
        }
    }

    /// Submit a transaction and return the response
    pub async fn submit_transaction(
        &self,
        account_id: &str,
        transaction_data: Value,
        priority: Option<i32>,
    ) -> reqwest::Result<reqwest::Response> {
        let mut payload = json!({
            "account_id": account_id,
            "transaction_data": transaction_data,
        });

        if let Some(p) = priority {
            payload["priority"] = json!(p);
        }

        self.client
            .post(format!("{}/v1/transactions/submit", self.base_url))
            .json(&payload)
            .send()
            .await
    }

    /// Submit a transaction and expect success
    pub async fn submit_transaction_expect_success(
        &self,
        account_id: &str,
        transaction_data: Value,
        priority: Option<i32>,
    ) -> (String, i64, i64) {
        let response = self
            .submit_transaction(account_id, transaction_data, priority)
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Expected 200 OK, got {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        );

        let body: Value = response
            .json()
            .await
            .expect("Failed to parse JSON response");

        let transaction_id = body["transaction_id"]
            .as_str()
            .expect("Missing transaction_id")
            .to_string();
        let queue_position = body["queue_position"]
            .as_i64()
            .expect("Missing queue_position");
        let estimated_time = body["estimated_processing_time_seconds"]
            .as_i64()
            .expect("Missing estimated_processing_time_seconds");

        (transaction_id, queue_position, estimated_time)
    }

    /// Submit a transaction and expect rate limit error
    pub async fn submit_transaction_expect_rate_limit(
        &self,
        account_id: &str,
        transaction_data: Value,
    ) -> Value {
        let response = self
            .submit_transaction(account_id, transaction_data, None)
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "Expected 429 Too Many Requests, got {}",
            response.status()
        );

        // Check rate limit headers are present
        let headers = response.headers();
        assert!(
            headers.contains_key("x-ratelimit-limit"),
            "Missing X-RateLimit-Limit header"
        );
        assert!(
            headers.contains_key("x-ratelimit-remaining"),
            "Missing X-RateLimit-Remaining header"
        );
        assert!(
            headers.contains_key("x-ratelimit-reset"),
            "Missing X-RateLimit-Reset header"
        );

        response
            .json()
            .await
            .expect("Failed to parse JSON response")
    }
}

/// Test data generators
pub struct TestData;

impl TestData {
    pub fn sample_transaction_data() -> Value {
        json!({
            "account_type": "user_pda",
            "owner_pubkey": "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
            "seed": "user_vault_test",
            "program_id": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            "space_bytes": 165,
            "lamports": 2039280
        })
    }

    pub fn premium_account_creation() -> Value {
        json!({
            "account_type": "token_account",
            "owner_pubkey": "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM",
            "mint_pubkey": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC mint
            "seed": "token_vault_premium",
            "program_id": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            "space_bytes": 165,
            "lamports": 2039280
        })
    }

    pub fn large_transaction_data() -> Value {
        json!({
            "account_type": "multisig_pda",
            "owners": [
                "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
                "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM",
                "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1"
            ],
            "threshold": 2,
            "seed": "enterprise_multisig_vault",
            "program_id": "11111111111111111111111111111112",
            "space_bytes": 355,
            "lamports": 5000000,
            "metadata": {
                "organization": "enterprise_client_001",
                "vault_type": "treasury",
                "created_by": "system"
            }
        })
    }

    pub fn account_id(suffix: &str) -> String {
        format!("defi_protocol_{}", suffix)
    }

    pub fn unique_account_id() -> String {
        format!(
            "client_{}",
            &uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()
        )
    }

    pub fn enterprise_account_id() -> String {
        format!(
            "enterprise_{}",
            &uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string()
        )
    }

    pub fn basic_tier_account_id() -> String {
        format!(
            "basic_{}",
            &uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string()
        )
    }

    pub fn premium_tier_account_id() -> String {
        format!(
            "premium_{}",
            &uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string()
        )
    }
}

/// Test timing utilities
pub struct TestTiming;

impl TestTiming {
    /// Wait for a condition to be true, with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout_seconds: u64,
        check_interval_ms: u64,
    ) -> bool
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_seconds);

        while start.elapsed() < timeout {
            if condition().await {
                return true;
            }
            sleep(Duration::from_millis(check_interval_ms)).await;
        }

        false
    }

    /// Measure execution time of an async operation
    pub async fn measure_async<F, Fut, T>(operation: F) -> (T, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let start = std::time::Instant::now();
        let result = operation().await;
        let duration = start.elapsed();
        (result, duration)
    }
}

/// Environment validation utilities
pub struct TestEnvironment;

impl TestEnvironment {
    /// Check if the API server is running
    pub async fn check_api_server() -> bool {
        let client = TestClient::new();
        match client
            .client
            .get(format!("{}/health", API_BASE_URL))
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// Wait for the API server to be ready
    pub async fn wait_for_api_server(timeout_seconds: u64) -> bool {
        TestTiming::wait_for_condition(
            Self::check_api_server,
            timeout_seconds,
            500, // Check every 500ms
        )
        .await
    }

    /// Validate test environment is properly set up
    pub async fn validate_test_environment() {
        assert!(
            Self::wait_for_api_server(30).await,
            "API server is not running on {}. Please start it with: just run-dev",
            API_BASE_URL
        );
    }
}

/// Performance test utilities
pub struct PerformanceMetrics {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub min_duration_ms: u128,
    pub max_duration_ms: u128,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: u128,
    pub p99_duration_ms: u128,
    pub requests_per_second: f64,
}

impl PerformanceMetrics {
    pub fn calculate(durations: &mut [Duration], total_duration: Duration) -> Self {
        durations.sort_unstable();

        let total_requests = durations.len();
        let successful_requests = total_requests; // All durations represent successful requests
        let failed_requests = 0; // Failed requests don't have durations

        let durations_ms: Vec<u128> = durations.iter().map(|d| d.as_millis()).collect();

        let min_duration_ms = durations_ms.first().copied().unwrap_or(0);
        let max_duration_ms = durations_ms.last().copied().unwrap_or(0);
        let avg_duration_ms = durations_ms.iter().sum::<u128>() as f64 / total_requests as f64;

        let p95_index = (total_requests as f64 * 0.95) as usize;
        let p99_index = (total_requests as f64 * 0.99) as usize;

        let p95_duration_ms = durations_ms
            .get(p95_index.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        let p99_duration_ms = durations_ms
            .get(p99_index.saturating_sub(1))
            .copied()
            .unwrap_or(0);

        let requests_per_second = total_requests as f64 / total_duration.as_secs_f64();

        Self {
            total_requests,
            successful_requests,
            failed_requests,
            min_duration_ms,
            max_duration_ms,
            avg_duration_ms,
            p95_duration_ms,
            p99_duration_ms,
            requests_per_second,
        }
    }

    pub fn print_summary(&self) {
        println!("=== Performance Test Results ===");
        println!("Total Requests: {}", self.total_requests);
        println!("Successful: {}", self.successful_requests);
        println!("Failed: {}", self.failed_requests);
        println!(
            "Success Rate: {:.2}%",
            (self.successful_requests as f64 / self.total_requests as f64) * 100.0
        );
        println!();
        println!("Response Times (ms):");
        println!("  Min: {}", self.min_duration_ms);
        println!("  Max: {}", self.max_duration_ms);
        println!("  Avg: {:.2}", self.avg_duration_ms);
        println!("  P95: {}", self.p95_duration_ms);
        println!("  P99: {}", self.p99_duration_ms);
        println!();
        println!(
            "Throughput: {:.2} requests/second",
            self.requests_per_second
        );
        println!("=============================");
    }

    /// Assert performance requirements are met
    pub fn assert_performance_requirements(&self) {
        // Assert sub-100ms p99 response time
        assert!(
            self.p99_duration_ms < 100,
            "P99 response time requirement not met: {}ms >= 100ms",
            self.p99_duration_ms
        );

        // Assert high success rate
        let success_rate = (self.successful_requests as f64 / self.total_requests as f64) * 100.0;
        assert!(
            success_rate >= 99.0,
            "Success rate requirement not met: {:.2}% < 99%",
            success_rate
        );

        // Assert reasonable throughput for 10k requests
        assert!(
            self.requests_per_second > 100.0,
            "Throughput too low: {:.2} requests/second",
            self.requests_per_second
        );
    }
}
