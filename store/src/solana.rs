use crate::models::asset::Asset;
use crate::models::balance::Balance;
use crate::models::quote::Quote;
use crate::Store;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug)]
pub enum QuoteError {
    DatabaseError(String),
}

impl std::fmt::Display for QuoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuoteError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for QuoteError {}

impl Store {
    pub async fn create_quote(
        &self,
        user_id: Uuid,
        quote_response: Value,
    ) -> Result<Quote, QuoteError> {
        let quote = sqlx::query_as!(
            Quote,
            r#"
            INSERT INTO quotes (user_id, quote_response)
            VALUES ($1, $2)
            RETURNING id, user_id, quote_response, created_at
            "#,
            user_id,
            quote_response
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| QuoteError::DatabaseError(e.to_string()))?;

        Ok(quote)
    }

    pub async fn get_quote(&self, quote_id: Uuid) -> Result<Option<Quote>, QuoteError> {
        let quote = sqlx::query_as!(
            Quote,
            r#"
            SELECT id, user_id, quote_response, created_at
            FROM quotes
            WHERE id = $1
            "#,
            quote_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| QuoteError::DatabaseError(e.to_string()))?;

        Ok(quote)
    }

    pub async fn get_sol_balance(&self, user_id: Uuid) -> Result<Option<Balance>, QuoteError> {
        let sol_mint_address = "So11111111111111111111111111111111111111112";
        let balance = sqlx::query_as!(
            Balance,
            r#"
            SELECT b.*
            FROM balances b
            JOIN assets a ON b.asset_id = a.id
            WHERE b.user_id = $1 AND a.mint_address = $2
            "#,
            user_id,
            sol_mint_address
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| QuoteError::DatabaseError(e.to_string()))?;

        Ok(balance)
    }

    pub async fn get_token_balances(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(Balance, Asset)>, QuoteError> {
        let balances = sqlx::query_as!(
            Balance,
            r#"
            SELECT b.*
            FROM balances b
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| QuoteError::DatabaseError(e.to_string()))?;
        
        let assets = sqlx::query_as!(
            Asset,
            r#"
            SELECT a.*
            FROM assets a
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| QuoteError::DatabaseError(e.to_string()))?;

        let mut result = Vec::new();
        for balance in balances {
            if balance.user_id == user_id {
                for asset in &assets {
                    if balance.asset_id == asset.id {
                        result.push((balance.clone(), asset.clone()));
                    }
                }
            }
        }

        Ok(result)
    }

    pub async fn upsert_asset(
        &self,
        mint_address: &str,
        decimals: i32,
        name: &str,
        symbol: &str,
    ) -> Result<Asset, QuoteError> {
        let asset = sqlx::query_as!(
            Asset,
            r#"
            INSERT INTO assets (mint_address, decimals, name, symbol)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (mint_address) DO UPDATE SET name = $3, symbol = $4
            RETURNING id, mint_address, decimals, name, symbol, logo_url, created_at, updated_at
            "#,
            mint_address,
            decimals,
            name,
            symbol
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| QuoteError::DatabaseError(e.to_string()))?;
        Ok(asset)
    }

    pub async fn upsert_balance(
        &self,
        user_id: Uuid,
        asset_id: Uuid,
        amount: i64,
    ) -> Result<Balance, QuoteError> {
        let balance = sqlx::query_as!(
            Balance,
            r#"
            INSERT INTO balances (user_id, asset_id, amount)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, asset_id) DO UPDATE SET amount = $3, updated_at = NOW()
            RETURNING id, amount, created_at, updated_at, user_id, asset_id
            "#,
            user_id,
            asset_id,
            amount
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| QuoteError::DatabaseError(e.to_string()))?;
        Ok(balance)
    }
}
