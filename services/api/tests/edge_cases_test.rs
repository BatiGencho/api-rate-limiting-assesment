mod common;

use common::*;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

/// Test extremely large transaction data
#[tokio::test]
async fn test_extremely_large_payload() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();

    // Create 10MB payload (should likely be rejected)
    let large_data = "x".repeat(10 * 1024 * 1024);
    let transaction_data = json!({
        "type": "stress_test",
        "large_field": large_data
    });

    let response = client
        .submit_transaction(&account_id, transaction_data, None)
        .await
        .expect("Failed to send request");

    // Should either reject with 413 (Payload Too Large) or 400 (Bad Request) or handle gracefully
    match response.status() {
        StatusCode::OK => {
            println!("✅ System handled 10MB payload successfully");
        }
        StatusCode::PAYLOAD_TOO_LARGE => {
            println!("✅ System correctly rejected large payload with 413");
        }
        StatusCode::BAD_REQUEST => {
            println!("✅ System rejected large payload with 400");
        }
        status => {
            println!("⚠️ Unexpected status for large payload: {}", status);
        }
    }
}

/// Test malformed JSON with various edge cases
#[tokio::test]
async fn test_malformed_json_edge_cases() {
    TestEnvironment::validate_test_environment().await;

    let client = reqwest::Client::new();
    let url = format!("{}/v1/transactions/submit", API_BASE_URL);

    let extremely_nested = format!(
        "{{{}}}",
        "\"a\":{".repeat(1000) + "\"b\":1" + &"}".repeat(1000)
    );
    let very_long_string = format!(
        "{{\"account_id\":\"{}\",\"data\":\"test\"}}",
        "x".repeat(100000)
    );

    let malformed_payloads: Vec<(&str, String)> = vec![
        ("empty_body", "".to_string()),
        ("only_whitespace", "   \n\t  ".to_string()),
        ("incomplete_json", "{\"account_id\":".to_string()),
        (
            "invalid_unicode",
            "{\"account_id\":\"test\",\"data\":\"\\uDC00\"}".to_string(),
        ),
        ("extremely_nested", extremely_nested),
        (
            "null_bytes",
            "{\"account_id\":\"test\\u0000\",\"data\":null}".to_string(),
        ),
        ("very_long_string", very_long_string),
    ];

    for (test_name, payload) in malformed_payloads {
        println!("Testing malformed JSON: {}", test_name);

        let response = client
            .post(&url)
            .header("content-type", "application/json")
            .body(payload.clone())
            .send()
            .await
            .expect("Failed to send request");

        // Should return 400 Bad Request or 422 Unprocessable Entity for malformed JSON
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "Malformed JSON '{}' should return 400 Bad Request or 422 Unprocessable Entity, got {}",
            test_name,
            response.status()
        );
    }
}

/// Test special characters and encoding in account IDs
#[tokio::test]
async fn test_special_character_account_ids() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let transaction_data = TestData::sample_transaction_data();

    let special_account_ids = vec![
        "test-account-123",          // Hyphens
        "test_account_123",          // Underscores
        "test.account.123",          // Dots
        "test@account.com",          // Email format
        "test/account/123",          // Slashes
        "test account 123",          // Spaces
        "test\taccount\n123",        // Whitespace chars
        "тест-аккаунт-123",         // Cyrillic
        "测试账户123",                // Chinese
        "🚀rocket🚀",               // Emojis
        "account_with_very_long_name_that_exceeds_normal_limits_but_might_still_be_valid_depending_on_implementation",
        "",                          // Empty string
        " ",                         // Single space
        "null",                      // String "null"
        "undefined",                 // String "undefined"
    ];

    for account_id in special_account_ids {
        println!("Testing account ID: '{}'", account_id);

        let response = client
            .submit_transaction(account_id, transaction_data.clone(), None)
            .await
            .expect("Failed to send request");

        // Should either accept or reject with appropriate error
        match response.status() {
            StatusCode::OK => {
                println!("  ✅ Accepted special account ID");
            }
            StatusCode::BAD_REQUEST => {
                println!("  ⚠️ Rejected special account ID with 400");
            }
            status => {
                println!("  ❓ Unexpected status for special account ID: {}", status);
            }
        }
    }
}

/// Test extreme priority values
#[tokio::test]
async fn test_extreme_priority_values() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    let extreme_priorities = vec![
        i32::MIN, // Minimum i32
        i32::MAX, // Maximum i32
        -1000000, // Large negative
        1000000,  // Large positive
        0,        // Zero
        -1,       // Negative one
    ];

    for priority in extreme_priorities {
        println!("Testing priority: {}", priority);

        let response = client
            .submit_transaction(&account_id, transaction_data.clone(), Some(priority))
            .await
            .expect("Failed to send request");

        // System should handle extreme priorities gracefully
        match response.status() {
            StatusCode::OK => {
                println!("  ✅ Handled extreme priority successfully");

                // Verify response structure
                let body: Value = response.json().await.expect("Failed to parse JSON");
                assert!(body.get("transaction_id").is_some());
                assert!(body.get("queue_position").is_some());
            }
            StatusCode::BAD_REQUEST => {
                println!("  ⚠️ Rejected extreme priority with 400");
            }
            status => {
                println!("  ❓ Unexpected status for extreme priority: {}", status);
            }
        }
    }
}

/// Test deeply nested transaction data
#[tokio::test]
async fn test_deeply_nested_transaction_data() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();

    // Create deeply nested JSON (100 levels deep)
    let mut nested_data = json!("deep_value");
    for i in 0..100 {
        nested_data = json!({
            format!("level_{}", i): nested_data
        });
    }

    let response = client
        .submit_transaction(&account_id, nested_data, None)
        .await
        .expect("Failed to send request");

    // Should handle deep nesting gracefully
    match response.status() {
        StatusCode::OK => {
            println!("✅ Handled deeply nested data successfully");
        }
        StatusCode::BAD_REQUEST => {
            println!("⚠️ Rejected deeply nested data (may have depth limits)");
        }
        status => {
            println!("❓ Unexpected status for deeply nested data: {}", status);
        }
    }
}

/// Test concurrent requests from same account with same data
#[tokio::test]
async fn test_duplicate_concurrent_requests() {
    TestEnvironment::validate_test_environment().await;

    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit 10 identical concurrent requests
    let mut handles = Vec::new();
    for _ in 0..10 {
        let client = TestClient::new();
        let account_id = account_id.clone();
        let data = transaction_data.clone();

        let handle =
            tokio::spawn(async move { client.submit_transaction(&account_id, data, None).await });

        handles.push(handle);
    }

    // Collect results
    let mut successes = 0;
    let mut rate_limits = 0;
    let mut errors = 0;
    let mut transaction_ids = std::collections::HashSet::new();

    for handle in handles {
        let result = handle.await.expect("Task failed");
        match result {
            Ok(response) => match response.status() {
                StatusCode::OK => {
                    successes += 1;
                    let body: Value = response.json().await.expect("Failed to parse JSON");
                    let tx_id = body["transaction_id"].as_str().unwrap();
                    transaction_ids.insert(tx_id.to_string());
                }
                StatusCode::TOO_MANY_REQUESTS => rate_limits += 1,
                _ => errors += 1,
            },
            Err(_) => errors += 1,
        }
    }

    println!(
        "Duplicate concurrent requests: {} success, {} rate limited, {} errors",
        successes, rate_limits, errors
    );

    // All successful requests should have unique transaction IDs
    assert_eq!(
        transaction_ids.len(),
        successes,
        "All successful requests should have unique transaction IDs"
    );

    // Should have some successful submissions
    assert!(
        successes > 0,
        "Should have at least some successful submissions"
    );
}

/// Test request timeout handling
#[tokio::test]
async fn test_client_timeout_handling() {
    TestEnvironment::validate_test_environment().await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(100)) // Very short timeout
        .build()
        .expect("Failed to create client");

    let url = format!("{}/v1/transactions/submit", API_BASE_URL);
    let request_data = json!({
        "account_id": TestData::unique_account_id(),
        "transaction_data": TestData::sample_transaction_data()
    });

    let result = client.post(&url).json(&request_data).send().await;

    // Should either succeed quickly or timeout
    match result {
        Ok(response) => {
            println!(
                "✅ Request completed within 100ms timeout: {}",
                response.status()
            );
        }
        Err(e) if e.is_timeout() => {
            println!("⚠️ Request timed out as expected with short timeout");
        }
        Err(e) => {
            println!("❓ Unexpected error: {}", e);
        }
    }
}

/// Test handling of null and undefined values
#[tokio::test]
async fn test_null_undefined_values() {
    TestEnvironment::validate_test_environment().await;

    let client = reqwest::Client::new();
    let url = format!("{}/v1/transactions/submit", API_BASE_URL);

    let test_payloads = vec![
        (
            "null_transaction_data",
            json!({
                "account_id": "test_account",
                "transaction_data": null
            }),
        ),
        (
            "null_account_id",
            json!({
                "account_id": null,
                "transaction_data": {"type": "test"}
            }),
        ),
        (
            "null_priority",
            json!({
                "account_id": "test_account",
                "transaction_data": {"type": "test"},
                "priority": null
            }),
        ),
        ("missing_fields", json!({})),
        (
            "extra_fields",
            json!({
                "account_id": "test_account",
                "transaction_data": {"type": "test"},
                "extra_field": "should_be_ignored",
                "another_extra": 123
            }),
        ),
    ];

    for (test_name, payload) in test_payloads {
        println!("Testing payload: {}", test_name);

        let response = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .expect("Failed to send request");

        // Should handle null/missing values appropriately
        match response.status() {
            StatusCode::OK => {
                println!("  ✅ Accepted payload with null/missing values");
            }
            StatusCode::BAD_REQUEST => {
                println!("  ⚠️ Rejected payload with null/missing values (expected)");
            }
            status => {
                println!("  ❓ Unexpected status for null/missing values: {}", status);
            }
        }
    }
}

/// Test system behavior when database is under stress
#[tokio::test]
#[ignore] // Only run when testing database limits
async fn test_database_stress_handling() {
    TestEnvironment::validate_test_environment().await;

    println!("Testing database stress handling...");

    let _client = TestClient::new();
    let base_data = TestData::sample_transaction_data();

    // Rapid fire many requests to stress database connections
    let mut handles = Vec::new();
    for i in 0..1000 {
        let client = TestClient::new();
        let account_id = format!("db_stress_{}", i % 10);
        let mut data = base_data.clone();
        data["stress_id"] = json!(i);

        let handle =
            tokio::spawn(async move { client.submit_transaction(&account_id, data, None).await });

        handles.push(handle);

        // No delay - stress the system
    }

    let mut successes = 0;
    let mut failures = 0;

    for handle in handles {
        match handle.await {
            Ok(Ok(response)) if response.status() == StatusCode::OK => successes += 1,
            _ => failures += 1,
        }
    }

    println!(
        "Database stress test: {} successes, {} failures",
        successes, failures
    );

    // System should handle at least some requests even under stress
    assert!(
        successes > 0,
        "Should handle at least some requests under database stress"
    );

    // Failure rate should not be too high
    let failure_rate = failures as f64 / (successes + failures) as f64;
    assert!(
        failure_rate < 0.5,
        "Failure rate should be less than 50% under stress, got {:.1}%",
        failure_rate * 100.0
    );
}

/// Test Redis connection failure scenarios
#[tokio::test]
#[ignore] // Only run when simulating Redis failures
async fn test_redis_failure_resilience() {
    // Note: This test would require stopping Redis mid-test
    // It's marked as ignored since it requires manual intervention

    println!("Testing Redis failure resilience...");
    println!("Note: This test requires manually stopping Redis during execution");

    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit some requests before Redis failure
    for i in 0..5 {
        let mut data = transaction_data.clone();
        data["pre_failure"] = json!(i);

        let result = client.submit_transaction(&account_id, data, None).await;
        match result {
            Ok(response) => println!("Pre-failure request {}: {}", i, response.status()),
            Err(e) => println!("Pre-failure request {} failed: {}", i, e),
        }

        sleep(Duration::from_secs(1)).await;
    }

    println!("\n⚠️  Now stop Redis with: docker-compose stop redis");
    println!("Press Enter to continue testing...");

    // Wait for manual intervention
    sleep(Duration::from_secs(10)).await;

    // Test behavior during Redis failure
    for i in 0..5 {
        let mut data = transaction_data.clone();
        data["during_failure"] = json!(i);

        let result = client.submit_transaction(&account_id, data, None).await;
        match result {
            Ok(response) => println!("During-failure request {}: {}", i, response.status()),
            Err(e) => println!("During-failure request {} failed: {}", i, e),
        }

        sleep(Duration::from_secs(1)).await;
    }

    println!("✅ Redis failure resilience test completed");
    println!("   Remember to restart Redis: docker-compose start redis");
}
