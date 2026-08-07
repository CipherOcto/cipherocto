//! `HolderRegistry` trait (RFC-0957-A1 §Algorithms).
//!
//! Abstract catalog with 6 methods: `lookup`, `lookup_by_ask`, `lookup_active`,
//! `insert`, `revoke`, `sync_peers`. Reference impl: `StoolapHolderRegistry`.

use thiserror::Error;

use crate::clock::Clock;
use crate::holder_kind::HolderKind;
use crate::holder_record::HolderRecord;

/// Registry errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    /// PK collision on `insert`.
    #[error("holder record already exists (cap_root_hash PK collision)")]
    AlreadyExists,
    /// `(ask_id, kind)` UNIQUE collision.
    #[error("holder record with (ask_id, kind) already exists")]
    DuplicateAskBinding,
    /// Underlying storage failure.
    #[error("storage error: {0}")]
    Storage(String),
    /// Record not found.
    #[error("holder record not found for cap_root_hash")]
    NotFound,
    /// Clock was not provided where required.
    #[error("clock required but not provided")]
    ClockRequired,
}

/// Per RFC-0957-A1 §Algorithms. Authoritative trait.
pub trait HolderRegistry: Send + Sync {
    /// Look up a record by `cap_root_hash` PK.
    fn lookup(&self, cap_root_hash: &[u8; 32]) -> Result<Option<HolderRecord>, RegistryError>;

    /// Look up a record by `(ask_id, kind)`. UNIQUE constraint guarantees ≤ 1 row.
    fn lookup_by_ask(
        &self,
        ask_id: &[u8; 32],
        kind: HolderKind,
    ) -> Result<Option<HolderRecord>, RegistryError>;

    /// Look up a record and verify it is currently ACTIVE (not revoked, not expired).
    /// Returns `Ok(None)` if the record is missing OR revoked OR expired.
    fn lookup_active(
        &self,
        cap_root_hash: &[u8; 32],
        clock: &dyn Clock,
    ) -> Result<Option<HolderRecord>, RegistryError>;

    /// Insert a new record. Fails with `RegistryError::AlreadyExists` on PK collision.
    fn insert(&self, record: HolderRecord) -> Result<(), RegistryError>;

    /// Atomic dual-record insert (RFC-0969 §Phase 2 atomicity invariant).
    /// Inserts both records in a single Stoolap transaction; either both
    /// persist (on commit) or neither does (on rollback).
    ///
    /// Default impl falls back to non-atomic `insert` calls — implementations
    /// that can open a Stoolap transaction SHOULD override with the atomic
    /// path (see `StoolapHolderRegistry::insert_dual`).
    fn insert_dual(
        &self,
        bearer: HolderRecord,
        capability: HolderRecord,
    ) -> Result<(), RegistryError> {
        self.insert(bearer)?;
        self.insert(capability)
    }

    /// Revoke a record. Sets `revoked_at_millis_unix = Some(current_millis_unix)`.
    /// Idempotent: revoking an already-revoked record is a no-op.
    fn revoke(&self, cap_root_hash: &[u8; 32], clock: &dyn Clock) -> Result<(), RegistryError>;

    /// Sync registry state with the configured peer set (RFC-0862).
    fn sync_peers(&self) -> Result<(), RegistryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_error_display_does_not_leak_pii() {
        // Error variants must NOT include raw credential material.
        let e = RegistryError::Storage("connection reset".into());
        assert!(!format!("{e}").contains("cap_root_hash"));
    }
}
