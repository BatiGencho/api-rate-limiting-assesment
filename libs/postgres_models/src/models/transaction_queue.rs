use crate::schema::transaction_queue;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Selectable)]
#[diesel(table_name = transaction_queue)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TransactionQueue {
    pub id: Uuid,
    pub account_id: String,
    pub transaction_data: serde_json::Value,
    pub status: String,
    pub priority: i32,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub processed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = transaction_queue)]
pub struct NewTransactionQueue {
    pub id: Uuid,
    pub account_id: String,
    pub transaction_data: serde_json::Value,
    pub status: String,
    pub priority: i32,
    pub retry_count: i32,
    pub max_retries: i32,
    pub scheduled_at: Option<DateTime<Utc>>,
}

impl NewTransactionQueue {
    pub fn new(id: Uuid, account_id: String, transaction_data: serde_json::Value) -> Self {
        Self {
            id,
            account_id,
            transaction_data,
            status: "pending".to_string(),
            priority: 0,
            retry_count: 0,
            max_retries: 3,
            scheduled_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Retry,
}

impl TransactionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Retry => "retry",
        }
    }
}
