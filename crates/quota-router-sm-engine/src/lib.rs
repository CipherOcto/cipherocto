//! CipherOcto quota-router settlement matching engine.
//!
//! Owns the `asks` + `consumed_receipt_index` schema + state machine
//! (Mint → Settled → Consumed). Stoolap (CipherOcto fork) is the embedded
//! SQL substrate per [[stoolap-general-purpose-db]] Path B — cipherocto
//! hosts the schema and orchestrates settlement; stoolap is just a
//! general-purpose DB engine.
//!
//! ## Mode gate (RFC-0917 invariant)
//!
//! Settlement operations are accessible via both the HTTP proxy and the
//! Python SDK in ALL modes. The mode gate controls HOW data is stored
//! (in-process embedded stoolap vs remote stub for tests), not whether
//! the surface is available.

#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]

use serde::{Deserialize, Serialize};

pub mod schema;
pub mod state_machine;
pub mod store;

pub use schema::{apply_migrations, MigrationError};
pub use state_machine::{transition, StateTransitionError};
pub use store::{SettlementStore, StoolapStore, StorageError};

/// Ask state (RFC-0959 v1.0 §State Machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AskState {
    /// Initial state after mint_with_zk() succeeds. ask_id, holder, axes recorded.
    Minted,
    /// Receipt settled; settlement_hash locked; ready for consumption.
    Settled,
    /// Terminal; receipt_index row written; settlement complete.
    Consumed,
}

impl AskState {
    /// SQL representation (matches migrations/001_create_asks.sql CHECK constraint).
    #[must_use]
    pub const fn as_sql(&self) -> &'static str {
        match self {
            Self::Minted => "Minted",
            Self::Settled => "Settled",
            Self::Consumed => "Consumed",
        }
    }

    /// Parse from SQL string.
    #[must_use]
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "Minted" => Some(Self::Minted),
            "Settled" => Some(Self::Settled),
            "Consumed" => Some(Self::Consumed),
            _ => None,
        }
    }
}

impl std::fmt::Display for AskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_sql())
    }
}

/// Ask record (RFC-0959 v1.0 §Data Structures).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    pub ask_id: [u8; 32],
    pub holder_did: String,
    pub axes_consumed: Vec<(String, u64)>,
    pub cap_root_hash: [u8; 32],
    pub invocation_hash: [u8; 32],
    pub current_unix_time: u64,
    pub output_hash: Option<[u8; 32]>,
}

/// Receipt record (RFC-0959 v1.0 §Data Structures).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub receipt_id: [u8; 32],
    pub ask_id: [u8; 32],
    pub settlement_hash: [u8; 32],
    pub router_id: String,
    pub router_sig: Vec<u8>,
    pub timestamp_unix: u64,
}

/// Settlement engine error (RPC-level; covers all settlement operations).
#[derive(Debug, thiserror::Error)]
pub enum SettlementError {
    #[error("ask not found: {0}")]
    AskNotFound(String),
    #[error("receipt already consumed: {0}")]
    AlreadyConsumed(String),
    #[error("invalid state transition: {from} → {to}")]
    InvalidTransition { from: AskState, to: AskState },
    #[error("settlement hash mismatch: expected {expected}, got {got}")]
    SettlementHashMismatch { expected: String, got: String },
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_state_sql_roundtrip() {
        for state in [AskState::Minted, AskState::Settled, AskState::Consumed] {
            assert_eq!(AskState::from_sql(state.as_sql()), Some(state));
        }
    }

    #[test]
    fn ask_state_unknown_sql_is_none() {
        assert_eq!(AskState::from_sql("Unknown"), None);
    }
}
