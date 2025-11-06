use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct PublicKey {
    pub end_user_pubkey: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

