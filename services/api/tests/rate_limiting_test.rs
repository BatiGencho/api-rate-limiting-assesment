mod common;

use common::*;
use reqwest::StatusCode;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

/// Test basic rate limiting functionality
#[tokio::test]
async fn test_basic_rate_limiting() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit transactions up to the rate limit (default 100 per minute for premium tier)
    let mut successful_submissions = 0;
    let mut rate_limited = false;

    for _i in 0..110 {
        let response = client
            .submit_transaction(&account_id, transaction_data.clone(), None)
            .await
            .expect("Failed to send request");

        if response.status() == StatusCode::OK {
            successful_submissions += 1;
        } else if response.status() == StatusCode::TOO_MANY_REQUESTS {
            // Verify rate limit headers are present
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

            rate_limited = true;
            break;
        } else {
            panic!("Unexpected status code: {}", response.status());
        }
    }

    assert!(
        successful_submissions > 0,
        "Should have at least some successful submissions"
    );
    assert!(rate_limited, "Should eventually hit rate limit");
    assert!(
        successful_submissions <= 100,
        "Should not exceed default rate limit of 100 (premium tier)"
    );
}

/// Test rate limiting enforcement with exact limit
#[tokio::test]
async fn test_exact_rate_limit_enforcement() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit exactly 100 transactions (default premium tier limit)
    for i in 0..100 {
        let response = client
            .submit_transaction(&account_id, transaction_data.clone(), None)
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Transaction {} should succeed within rate limit",
            i + 1
        );
    }

    // The 101st transaction should be rate limited
    let error_response = client
        .submit_transaction_expect_rate_limit(&account_id, transaction_data)
        .await;

    // Verify error response format
    assert!(
        error_response.get("error").is_some(),
        "Missing error field in rate limit response"
    );
}

/// Test rate limiting is per-account (different accounts have independent limits)
#[tokio::test]
async fn test_per_account_rate_limiting() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account1 = TestData::unique_account_id();
    let account2 = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Exhaust rate limit for account1 (100 requests for premium tier)
    for _ in 0..100 {
        let response = client
            .submit_transaction(&account1, transaction_data.clone(), None)
            .await
            .expect("Failed to send request");
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Verify account1 is rate limited
    let response = client
        .submit_transaction(&account1, transaction_data.clone(), None)
        .await
        .expect("Failed to send request");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    // Verify account2 can still submit (independent rate limit)
    let response = client
        .submit_transaction(&account2, transaction_data, None)
        .await
        .expect("Failed to send request");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Account2 should not be affected by account1's rate limit"
    );
}

/// Test rate limit headers are correctly returned
#[tokio::test]
async fn test_rate_limit_headers() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit first transaction and check headers
    let response = client
        .submit_transaction(&account_id, transaction_data.clone(), None)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let headers = response.headers();

    // Check rate limit headers are present
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

    // Check header values are reasonable
    let limit = headers
        .get("x-ratelimit-limit")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i32>()
        .unwrap();
    let remaining = headers
        .get("x-ratelimit-remaining")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i32>()
        .unwrap();
    let reset = headers
        .get("x-ratelimit-reset")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();

    assert!(limit > 0, "Rate limit should be positive");
    assert!(remaining >= 0, "Remaining should be non-negative");
    assert!(
        remaining < limit,
        "Remaining should be less than limit after one request"
    );
    assert!(reset > 0, "Reset timestamp should be positive");
}

/// Test rate limit recovery after time window
/// Note: This test is marked as ignored because it takes time to complete
#[tokio::test]
#[ignore]
async fn test_rate_limit_recovery() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Exhaust rate limit
    // NOTE: since DEFAULT_MAX_REQUESTS: u32 = 100, we send 100 requests
    for _ in 0..100 {
        let response = client
            .submit_transaction(&account_id, transaction_data.clone(), None)
            .await
            .expect("Failed to send request");
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Verify rate limited
    let response = client
        .submit_transaction(&account_id, transaction_data.clone(), None)
        .await
        .expect("Failed to send request");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    // Wait for rate limit window to reset (1 minute + buffer)
    println!("Waiting for rate limit to reset...");
    sleep(Duration::from_secs(65)).await;

    // Should be able to submit again
    let response = client
        .submit_transaction(&account_id, transaction_data, None)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Should be able to submit after rate limit window resets"
    );
}

/// Test concurrent requests don't bypass rate limiting
#[tokio::test]
async fn test_concurrent_rate_limiting() {
    TestEnvironment::validate_test_environment().await;

    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit 120 concurrent requests (should hit rate limit with 100/min premium tier)
    let mut handles = Vec::new();
    for i in 0..120 {
        let client = TestClient::new();
        let account_id = account_id.clone();
        let mut data = transaction_data.clone();
        data["concurrent_id"] = json!(i);

        let handle = tokio::spawn(async move {
            let response = client
                .submit_transaction(&account_id, data, None)
                .await
                .expect("Failed to send request");
            response.status()
        });

        handles.push(handle);
    }

    // Collect results
    let mut success_count = 0;
    let mut rate_limited_count = 0;

    for handle in handles {
        let status = handle.await.expect("Task failed");
        match status {
            StatusCode::OK => success_count += 1,
            StatusCode::TOO_MANY_REQUESTS => rate_limited_count += 1,
            _ => panic!("Unexpected status: {}", status),
        }
    }

    println!(
        "Concurrent test results: {} successful, {} rate limited",
        success_count, rate_limited_count
    );

    // Should have some successful and some rate limited
    assert!(success_count > 0, "Should have some successful requests");
    assert!(
        rate_limited_count > 0,
        "Should have some rate limited requests"
    );
    assert!(
        success_count <= 102,
        "Should not significantly exceed premium tier rate limit (100/min) even with concurrency - got {} successful requests",
        success_count
    );
    assert_eq!(
        success_count + rate_limited_count,
        120,
        "All requests should be accounted for"
    );
}

/// Test rate limiting with different priorities (priorities don't bypass rate limits)
#[tokio::test]
async fn test_rate_limiting_with_priority() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Exhaust rate limit with high priority transactions (premium tier: 100 per minute)
    for _ in 0..100 {
        let response = client
            .submit_transaction(&account_id, transaction_data.clone(), Some(10))
            .await
            .expect("Failed to send request");
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Even high priority should be rate limited
    let response = client
        .submit_transaction(&account_id, transaction_data, Some(10))
        .await
        .expect("Failed to send request");
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "High priority transactions should still be subject to rate limits"
    );
}

/// Test rate limit error response format
#[tokio::test]
async fn test_rate_limit_error_format() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Exhaust rate limit (premium tier: 100 per minute)
    for _ in 0..100 {
        client
            .submit_transaction(&account_id, transaction_data.clone(), None)
            .await
            .expect("Failed to send request");
    }

    // Get rate limit error
    let error_response = client
        .submit_transaction_expect_rate_limit(&account_id, transaction_data)
        .await;

    // Verify error response structure
    assert!(error_response.get("error").is_some(), "Missing error field");

    let error = &error_response["error"];
    if error.is_string() {
        let error_msg = error.as_str().unwrap();
        assert!(
            error_msg.contains("rate limit") || error_msg.contains("too many"),
            "Error message should mention rate limiting: {}",
            error_msg
        );
    }
}
