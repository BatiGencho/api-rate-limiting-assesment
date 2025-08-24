use crate::schema::rate_limits;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Selectable)]
#[diesel(table_name = rate_limits)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RateLimit {
    pub id: Uuid,
    pub account_id: String,
    pub limit_type: String,
    pub max_requests: i32,
    pub window_seconds: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = rate_limits)]
pub struct NewRateLimit {
    pub id: Uuid,
    pub account_id: String,
    pub limit_type: String,
    pub max_requests: i32,
    pub window_seconds: i32,
}

impl NewRateLimit {
    pub fn new(
        account_id: String,
        limit_type: String,
        max_requests: i32,
        window_seconds: i32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            account_id,
            limit_type,
            max_requests,
            window_seconds,
        }
    }
}
