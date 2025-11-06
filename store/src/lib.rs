pub mod models;
pub mod user;
pub mod solana;
pub mod public_key;

use sqlx::PgPool;

pub struct Store {
    pub pool: PgPool,
}

impl Store {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
