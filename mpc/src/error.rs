use crate::serialization;
use solana_client::client_error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("deserialization failed for field `{field_name}`: {error}")]
    DeserializationFailed {
        error: serialization::Error,
        field_name: &'static str,
    },

    #[error("mismatch between messages")]
    MismatchMessages,

    #[error("invalid signature")]
    InvalidSignature,

    #[error("keypair is not in the list of pubkeys")]
    KeyPairIsNotInKeys,

    #[error("database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Solana client error: {0}")]
    SolanaClientError(#[from] client_error::ClientError),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("session not found")]
    SessionNotFound,

    #[error("key not found for user")]
    KeyNotFound,

    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

