use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Quote {
    pub id: Uuid,
    pub user_id: Uuid,
    pub quote_response: Value,
    pub created_at: DateTime<Utc>,
}
