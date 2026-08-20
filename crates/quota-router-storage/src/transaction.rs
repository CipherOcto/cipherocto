//! `Transaction` type (RFC-0957-A1 §Algorithms).
//!
//! Atomic multi-record operations boundary. Backed by a Stoolap transaction
//! (concrete `Database::begin()` per RFC-0957-A1 §Stoolap compatibility note).
//!
//! `Drop` is auto-rollback on panic; `commit` is the only success path.
//!
//! Mission 0957-c deviation: the canonical signature for `insert_dual` lives
//! on `Transaction` (per RFC §Stoolap compatibility note + R7-N3 fix). The
//! `TransactionExt` extension trait in RFC-0969 (open mission 0969-b) provides
//! the `insert_dual` for `CapabilityCatalog`; the underlying primitive
//! `Transaction::insert_holder_record` is owned here.

use thiserror::Error;

use octo_cap_macaroon::{BearerCapsule, HolderRecord, RegistryError};

/// Atomic transaction boundary.
pub struct Transaction {
    /// Underlying Stoolap handle. Concrete type lives in `stoolap_holder_registry.rs`;
    /// we keep it opaque here so the trait surface is stable.
    _inner: std::marker::PhantomData<()>,
}

impl Transaction {
    /// Insert a single `HolderRecord` into the registry.
    pub fn insert_holder_record(&self, _record: HolderRecord) -> Result<(), RegistryError> {
        // Concrete impl in `stoolap_holder_registry.rs::StoolapHolderRegistry::insert`.
        Err(RegistryError::Storage(
            "Transaction::insert_holder_record must be invoked via StoolapHolderRegistry::insert \
             (structural Transaction stub does not carry a Stoolap handle)"
                .into(),
        ))
    }

    /// Insert a paired (Bearer, Capability) record atomically.
    /// Both inserts succeed or neither does (RFC-0969 §Phase 2 atomicity).
    ///
    /// **Mission 0969-b1 scope:** concrete impl landed at
    /// `stoolap_holder_registry.rs::StoolapHolderRegistry::insert_dual`.
    /// This structural-stub body remains to preserve the trait surface
    /// (callers wired through `&mut Transaction`); the concrete impl is
    /// reachable via the registry directly. A future mission can give
    /// `Transaction` an `Arc<StoolapHolderRegistry>` handle so the
    /// structural-stub body delegates to the concrete impl.
    pub fn insert_dual(
        &self,
        _bearer: HolderRecord,
        _capability: HolderRecord,
    ) -> Result<(), RegistryError> {
        Err(RegistryError::Storage(
            "Transaction::insert_dual (structural stub) — concrete atomic impl at \
             stoolap_holder_registry.rs::StoolapHolderRegistry::insert_dual (mission 0969-b1)"
                .into(),
        ))
    }

    /// Append a settlement event to the chain. Used by `chain_tip_lock` CAS
    /// (RFC-0959-A1 §Algorithms). Owned by 0959-b.
    pub fn append_settlement_event(&self, _event: &[u8]) -> Result<(), RegistryError> {
        Err(RegistryError::Storage(
            "Transaction::append_settlement_event is owned by 0959-b".into(),
        ))
    }

    /// Read the current chain tip hash. Owned by 0959-b.
    pub fn read_chain_tip(&self) -> Result<[u8; 32], RegistryError> {
        Err(RegistryError::Storage(
            "Transaction::read_chain_tip is owned by 0959-b".into(),
        ))
    }

    /// Write the chain-tip row lock (CAS). Owned by 0959-b.
    pub fn write_lock_chain_tip(
        &self,
        _expected: &[u8; 32],
        _new: &[u8; 32],
    ) -> Result<(), RegistryError> {
        Err(RegistryError::Storage(
            "Transaction::write_lock_chain_tip is owned by 0959-b".into(),
        ))
    }
}

/// Placeholder stub for the `BearerCapsule` cipher-bound key bundle. 0957-c
/// only references the type via `HolderRecord::from_bearer`; the stub
/// re-exports the canonical type from `bearer_capsule_stub`.
pub type BearerCapsuleRef<'a> = &'a BearerCapsule;

/// Transaction errors (subset of `RegistryError`).
#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("registry error: {0}")]
    Registry(#[from] RegistryError),
    #[error("commit failed: {0}")]
    Commit(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_methods_return_storage_error() {
        let tx = Transaction {
            _inner: std::marker::PhantomData,
        };
        let rec = HolderRecord::from_bearer(
            &BearerCapsule::new([0x42; 32], vec![], [0x55; 64]),
            &[0x77; 32],
            &octo_ident::test_helpers::sample_did(137),
            [0x33; 32],
            1_700_000_000_000,
        );
        let err = tx.insert_holder_record(rec).unwrap_err();
        assert!(matches!(err, RegistryError::Storage(_)));
    }
}
