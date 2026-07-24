//! Dispute resolution — task market dispute creation + evidence.
//!
//! Placeholder; full implementation lands in Task 6.3.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisputeReason {
    ResultMismatch,
    ProviderTimeout,
    ProviderError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub hash: [u8; 32],
    pub description: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DisputeError {
    #[error("dispute already exists for escrow {0:?}")]
    AlreadyExists([u8; 32]),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dispute {
    pub escrow_id: [u8; 32],
    pub raised_by: String,
    pub reason: DisputeReason,
    pub evidence: Option<Evidence>,
}
