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

pub mod envelope;
pub mod schema;
pub mod shard;
pub mod state_machine;
pub mod store;

pub use schema::{apply_migrations, MigrationError};
pub use state_machine::{
    transition, transition_reservation, ReservationTransitionError, StateTransitionError,
};
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

/// Reservation state (RFC-0960 §2.3).
///
/// Distinct from `AskState` (RFC-0959 receipt lifecycle) and from
/// `SettlementReceipt`'s state machine (RFC-0959 v1.0 Minted→Settled→
/// Consumed). A Reservation is the pre-auth escrow that binds a capability
/// to an intended operation; it carries the audit-window state machine
/// Reserved→Executing→Settled→Auditable→Released and the dispute branch
/// Frozen→Dispute→Rollback/Uphold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReservationState {
    /// Pre-auth holds the amount; capability bound.
    Reserved,
    /// Provider is executing the requested operation.
    Executing,
    /// Proof attached; awaiting audit window.
    Settled,
    /// Inside dispute window.
    Auditable,
    /// Terminal; transfers applied; reservation closed.
    Released,
    /// Deadline passed before settlement arrived.
    Expired,
    /// Explicit cancel by capability holder.
    Cancelled,
    /// Dispute filed; transfers not applied.
    Frozen,
}

impl ReservationState {
    /// SQL representation.
    #[must_use]
    pub const fn as_sql(&self) -> &'static str {
        match self {
            Self::Reserved => "Reserved",
            Self::Executing => "Executing",
            Self::Settled => "Settled",
            Self::Auditable => "Auditable",
            Self::Released => "Released",
            Self::Expired => "Expired",
            Self::Cancelled => "Cancelled",
            Self::Frozen => "Frozen",
        }
    }
}

impl std::fmt::Display for ReservationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_sql())
    }
}

/// Reservation record (RFC-0960 §2.3).
///
/// Step 6 of the 11-step exercise instantiates one of these. Replaces the
/// prior `blake3::hash(ask_id || b"escrow/v1")` placeholder, which was the
/// R1-F1 defect flagged in RFC-0960's R1 self-review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    /// BLAKE3(canonical_ser(reservation_unsigned)) — content-addressed.
    pub reservation_id: [u8; 32],
    /// Vault that authorizes this reservation (RFC-0960 §2.1).
    pub vault_id: [u8; 32],
    /// Capability bound to this reservation (RFC-0957 macaroon).
    pub capability_id: [u8; 32],
    /// Ask being pre-authed against (RFC-0959 ask_id).
    pub ask_id: [u8; 32],
    /// Resource axis being reserved (e.g. "input_tokens_per_1k").
    pub resource_axis: String,
    /// Amount in micro-units (OCTO_W micro-precision per RFC-0959).
    pub amount_micro: u128,
    /// Hard deadline for settlement to arrive.
    pub expires_at_unix: u64,
    /// Audit window duration (RFC-0960 §6); 0 = instant release.
    pub audit_window_secs: u64,
    /// Current state in the audit-window state machine.
    pub state: ReservationState,
    /// Optional link to a SettlementReceipt once settlement lands.
    pub settlement_ref: Option<[u8; 32]>,
    /// Unix timestamp at which the reservation was minted.
    pub created_at_unix: u64,
}

impl Reservation {
    /// Mint a new reservation in `Reserved` state.
    ///
    /// `reservation_id` is derived from the canonical inputs so two nodes
    /// constructing the same reservation independently produce the same id
    /// (RFC-0126 deterministic encoding).
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn mint(
        vault_id: [u8; 32],
        capability_id: [u8; 32],
        ask_id: [u8; 32],
        resource_axis: String,
        amount_micro: u128,
        expires_at_unix: u64,
        audit_window_secs: u64,
        created_at_unix: u64,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"reservation/v1");
        hasher.update(&vault_id);
        hasher.update(&capability_id);
        hasher.update(&ask_id);
        hasher.update(resource_axis.as_bytes());
        hasher.update(&amount_micro.to_le_bytes());
        hasher.update(&expires_at_unix.to_le_bytes());
        hasher.update(&audit_window_secs.to_le_bytes());
        hasher.update(&created_at_unix.to_le_bytes());
        let reservation_id = *hasher.finalize().as_bytes();

        Self {
            reservation_id,
            vault_id,
            capability_id,
            ask_id,
            resource_axis,
            amount_micro,
            expires_at_unix,
            audit_window_secs,
            state: ReservationState::Reserved,
            settlement_ref: None,
            created_at_unix,
        }
    }
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
    #[error("reservation not found: {0}")]
    ReservationNotFound(String),
    #[error("reservation expired: {0}")]
    ReservationExpired(String),
    #[error("invalid reservation transition: {from} → {to}")]
    InvalidReservationTransition {
        from: ReservationState,
        to: ReservationState,
    },
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
