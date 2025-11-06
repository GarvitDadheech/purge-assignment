use crate::error::Error;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use crate::serialization::SecretAggStepOne;

#[derive(Debug, FromRow)]
pub struct MpcKey {
    pub end_user_pubkey: String,
    pub node_id: i32,
    pub public_key: String,
    pub private_key: String, // Encrypted at rest
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct MpcSigningSession {
    pub session_id: Uuid,
    pub end_user_pubkey: String,
    pub secret_state_1: Option<Vec<u8>>,
    pub secret_state_2: Option<Vec<u8>>,
    pub partial_sig_2: Option<String>,
    pub agg_message_2: Option<String>,
    pub to_address: String,
    pub amount: f64,
    pub memo: Option<String>,
    pub transaction: Option<String>,
}

#[derive(Clone)]
pub struct MpcStore {
    pool: PgPool,
}

impl MpcStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn store_key(&self, key: &MpcKey) -> Result<(), Error> {
        sqlx::query!(
            r#"
            INSERT INTO mpc_keys (end_user_pubkey, node_id, public_key, private_key)
            VALUES ($1, $2, $3, $4)
            "#,
            key.end_user_pubkey,
            key.node_id,
            key.public_key,
            key.private_key // TODO: Encrypt before storing
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_key(&self, end_user_pubkey: &str, node_id: i32) -> Result<MpcKey, Error> {
        let key = sqlx::query_as!(
            MpcKey,
            r#"
            SELECT end_user_pubkey, node_id, public_key, private_key FROM mpc_keys
            WHERE end_user_pubkey = $1 AND node_id = $2
            "#,
            end_user_pubkey,
            node_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(key)
    }

    pub async fn get_keys_for_user(&self, end_user_pubkey: &str) -> Result<Vec<MpcKey>, Error> {
        let keys = sqlx::query_as!(
            MpcKey,
            r#"
            SELECT end_user_pubkey, node_id, public_key, private_key FROM mpc_keys
            WHERE end_user_pubkey = $1
            ORDER BY node_id
            "#,
            end_user_pubkey
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(keys)
    }

    pub async fn create_session(
        &self,
        end_user_pubkey: &str,
        secret_state_1: &SecretAggStepOne,
        to_address: &str,
        amount: f64,
        memo: Option<String>,
        transaction: Option<String>,
    ) -> Result<Uuid, Error> {
        let session_id = Uuid::new_v4();
        let secret_state_1_bytes = serde_json::to_vec(secret_state_1).unwrap();
        let expires_at = Utc::now() + Duration::minutes(5);

        sqlx::query!(
            r#"
            INSERT INTO mpc_signing_sessions 
            (session_id, end_user_pubkey, secret_state_1, to_address, amount, memo, expires_at, transaction)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            session_id,
            end_user_pubkey,
            secret_state_1_bytes,
            to_address,
            amount,
            memo,
            expires_at,
            transaction
        )
        .execute(&self.pool)
        .await?;

        Ok(session_id)
    }

    pub async fn update_session_with_step2_data(
        &self,
        session_id: Uuid,
        secret_state_2: &SecretAggStepOne,
        partial_sig_2: &str,
        agg_message_2: &str,
    ) -> Result<(), Error> {
        let secret_state_2_bytes = serde_json::to_vec(secret_state_2).unwrap();
        sqlx::query!(
            r#"
            UPDATE mpc_signing_sessions
            SET secret_state_2 = $1, partial_sig_2 = $2, agg_message_2 = $3
            WHERE session_id = $4
            "#,
            secret_state_2_bytes,
            partial_sig_2,
            agg_message_2,
            session_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_session(&self, session_id: Uuid) -> Result<MpcSigningSession, Error> {
        let session = sqlx::query_as!(
            MpcSigningSession,
            r#"
            SELECT 
                session_id, end_user_pubkey, secret_state_1, secret_state_2,
                partial_sig_2, agg_message_2, to_address, amount, memo, transaction
            FROM mpc_signing_sessions
            WHERE session_id = $1 AND expires_at > NOW()
            "#,
            session_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::RowNotFound = e {
                Error::SessionNotFound
            } else {
                Error::DatabaseError(e)
            }
        })?;
        Ok(session)
    }
}
