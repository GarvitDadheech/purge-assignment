use crate::models::public_key::PublicKey;
use crate::Store;

#[derive(Debug)]
pub enum PublicKeyError {
    DatabaseError(String),
}

impl std::fmt::Display for PublicKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublicKeyError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for PublicKeyError {}

impl Store {
    pub async fn add_public_key(&self, pubkey: &str) -> Result<PublicKey, PublicKeyError> {
        let key = sqlx::query_as!(
            PublicKey,
            r#"
            INSERT INTO public_keys (end_user_pubkey, is_active)
            VALUES ($1, true)
            ON CONFLICT (end_user_pubkey) DO UPDATE SET is_active = true
            RETURNING end_user_pubkey, is_active, created_at
            "#,
            pubkey
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| PublicKeyError::DatabaseError(e.to_string()))?;
        Ok(key)
    }

    pub async fn get_all_public_keys(&self) -> Result<Vec<PublicKey>, PublicKeyError> {
        let keys = sqlx::query_as!(
            PublicKey,
            r#"
            SELECT end_user_pubkey, is_active, created_at FROM public_keys
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PublicKeyError::DatabaseError(e.to_string()))?;
        Ok(keys)
    }
}

