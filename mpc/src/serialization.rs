use multi_party_eddsa::protocols::musig2::{PrivatePartialNonces, PublicPartialNonces};
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Signature};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AggMessage1 {
    pub sender: Pubkey,
    pub public_nonces: PublicPartialNonces,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SecretAggStepOne {
    pub private_nonces: PrivatePartialNonces,
    pub public_nonces: PublicPartialNonces,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq)]
pub struct PartialSignature(pub Signature);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid point: {0}")]
    InvalidPoint(curv::elliptic::curves::NotOnCurve),
    #[error("invalid scalar: {0}")]
    InvalidScalar(curv::elliptic::curves::WrongOrder),
}