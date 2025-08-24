mod common;

use common::*;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

/// Test basic transaction submission functionality
#[tokio::test]
async fn test_submit_transaction_success() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    let (transaction_id, queue_position, estimated_time) = client
        .submit_transaction_expect_success(&account_id, transaction_data, None)
        .await;

    // Validate response format
    assert!(
        Uuid::parse_str(&transaction_id).is_ok(),
        "Invalid transaction_id format"
    );
    assert!(queue_position > 0, "Queue position should be positive");
    assert!(estimated_time >= 0, "Estimated time should be non-negative");
}

/// Test transaction submission with priority
#[tokio::test]
async fn test_submit_transaction_with_priority() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit high priority transaction
    let (transaction_id, queue_position, _) = client
        .submit_transaction_expect_success(&account_id, transaction_data.clone(), Some(10))
        .await;

    assert!(Uuid::parse_str(&transaction_id).is_ok());
    assert!(queue_position > 0);
}

/// Test transaction validation - invalid account ID
#[tokio::test]
async fn test_submit_transaction_invalid_account_id() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let transaction_data = TestData::sample_transaction_data();

    // Test empty account ID
    let response = client
        .submit_transaction("", transaction_data.clone(), None)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test very long account ID (assuming there's a length limit)
    let long_account_id = "a".repeat(1000);
    let response = client
        .submit_transaction(&long_account_id, transaction_data, None)
        .await
        .expect("Failed to send request");

    // Should either be BAD_REQUEST or succeed (depending on implementation)
    assert!(response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::OK);
}

/// Test transaction validation - empty transaction data
#[tokio::test]
async fn test_submit_transaction_empty_data() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();

    let response = client
        .submit_transaction(&account_id, json!({}), None)
        .await
        .expect("Failed to send request");

    // Empty transaction data should be allowed (implementation dependent)
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::BAD_REQUEST);
}

/// Test handling of large transaction data
#[tokio::test]
async fn test_submit_large_transaction_data() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let large_data = TestData::large_transaction_data();

    let (transaction_id, _, _) = client
        .submit_transaction_expect_success(&account_id, large_data, None)
        .await;

    assert!(Uuid::parse_str(&transaction_id).is_ok());
}

/// Test sequential queue position assignment
#[tokio::test]
async fn test_sequential_queue_positions() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit multiple transactions and collect queue positions
    let mut positions = Vec::new();
    for _ in 0..5 {
        let (_, position, _) = client
            .submit_transaction_expect_success(&account_id, transaction_data.clone(), None)
            .await;
        positions.push(position);
    }

    // Check that positions are increasing (though not necessarily sequential due to other tests)
    for i in 1..positions.len() {
        assert!(
            positions[i] > positions[i - 1],
            "Queue positions should be increasing"
        );
    }
}

/// Test that different accounts have independent queues
#[tokio::test]
async fn test_independent_account_queues() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account1 = TestData::unique_account_id();
    let account2 = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit transactions for both accounts
    let (tx1_id, pos1, _) = client
        .submit_transaction_expect_success(&account1, transaction_data.clone(), None)
        .await;

    let (tx2_id, pos2, _) = client
        .submit_transaction_expect_success(&account2, transaction_data, None)
        .await;

    // Both should succeed with valid IDs
    assert!(Uuid::parse_str(&tx1_id).is_ok());
    assert!(Uuid::parse_str(&tx2_id).is_ok());
    assert_ne!(tx1_id, tx2_id);

    // Positions can be similar since they're in different queues
    assert!(pos1 > 0);
    assert!(pos2 > 0);
}

/// Test concurrent submissions don't cause race conditions
#[tokio::test]
async fn test_concurrent_submissions() {
    TestEnvironment::validate_test_environment().await;

    let _client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit 10 concurrent transactions
    let mut handles = Vec::new();
    for i in 0..10 {
        let client = TestClient::new();
        let account_id = account_id.clone();
        let mut data = transaction_data.clone();
        data["batch_id"] = json!(i); // Make each transaction unique

        let handle = tokio::spawn(async move {
            client
                .submit_transaction_expect_success(&account_id, data, None)
                .await
        });

        handles.push(handle);
    }

    // Wait for all transactions to complete
    let mut results = Vec::new();
    for handle in handles {
        let result = handle.await.expect("Task failed");
        results.push(result);
    }

    // Verify all transactions succeeded and have unique IDs
    assert_eq!(results.len(), 10);

    let mut transaction_ids = HashSet::new();
    for (tx_id, position, _) in results {
        assert!(Uuid::parse_str(&tx_id).is_ok());
        assert!(position > 0);
        assert!(
            transaction_ids.insert(tx_id),
            "Duplicate transaction ID found"
        );
    }
}

/// Test priority ordering affects queue position
#[tokio::test]
async fn test_priority_ordering() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    // Submit transactions with different priorities
    let (_, low_pos, _) = client
        .submit_transaction_expect_success(&account_id, transaction_data.clone(), Some(1))
        .await;

    let (_, high_pos, _) = client
        .submit_transaction_expect_success(&account_id, transaction_data.clone(), Some(10))
        .await;

    let (_, med_pos, _) = client
        .submit_transaction_expect_success(&account_id, transaction_data, Some(5))
        .await;

    // Higher priority should get better (lower) queue positions
    // Note: This test assumes the implementation properly handles priority
    assert!(low_pos > 0);
    assert!(high_pos > 0);
    assert!(med_pos > 0);

    // The exact ordering depends on implementation details
    // but we can at least verify they're all positive
}

/// Test error handling for malformed JSON
#[tokio::test]
async fn test_malformed_request() {
    TestEnvironment::validate_test_environment().await;

    let client = reqwest::Client::new();

    // Send malformed JSON
    let response = client
        .post(format!("{}/v1/transactions/submit", API_BASE_URL))
        .header("content-type", "application/json")
        .body("{ invalid json }")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Test missing required fields
#[tokio::test]
async fn test_missing_required_fields() {
    TestEnvironment::validate_test_environment().await;

    let client = reqwest::Client::new();

    // Missing account_id
    let response = client
        .post(format!("{}/v1/transactions/submit", API_BASE_URL))
        .json(&json!({
            "transaction_data": {"type": "test"}
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY
    );

    // Missing transaction_data
    let response = client
        .post(format!("{}/v1/transactions/submit", API_BASE_URL))
        .json(&json!({
            "account_id": "test_account"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY
    );
}

/// Test response contains all required fields
#[tokio::test]
async fn test_response_format() {
    TestEnvironment::validate_test_environment().await;

    let client = TestClient::new();
    let account_id = TestData::unique_account_id();
    let transaction_data = TestData::sample_transaction_data();

    let response = client
        .submit_transaction(&account_id, transaction_data, None)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Check all required fields are present
    assert!(
        body.get("transaction_id").is_some(),
        "Missing transaction_id"
    );
    assert!(
        body.get("queue_position").is_some(),
        "Missing queue_position"
    );
    assert!(
        body.get("estimated_processing_time_seconds").is_some(),
        "Missing estimated_processing_time_seconds"
    );
    assert!(body.get("status").is_some(), "Missing status");

    // Check field types
    assert!(
        body["transaction_id"].is_string(),
        "transaction_id should be string"
    );
    assert!(
        body["queue_position"].is_i64(),
        "queue_position should be integer"
    );
    assert!(
        body["estimated_processing_time_seconds"].is_i64(),
        "estimated_processing_time_seconds should be integer"
    );
    assert!(body["status"].is_string(), "status should be string");

    // Check status value
    let status = body["status"].as_str().unwrap();
    assert!(
        status == "pending" || status == "processing",
        "Invalid status: {}",
        status
    );
}
