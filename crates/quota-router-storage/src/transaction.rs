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

use crate::bearer_capsule_stub::BearerCapsule;
use crate::holder_record::HolderRecord;
use crate::holder_registry::RegistryError;

/// Atomic transaction boundary.
pub struct Transaction {
    /// Underlying Stoolap handle. Concrete type lives in `stoolap_holder_registry.rs`;
    /// we keep it opaque here so the trait surface is stable.
    _inner: std::marker::PhantomData<()>,
}

impl Transaction {
    /// Insert a single `HolderRecord` into the registry.
    pub fn insert_holder_record(&self, _record: HolderRecord) -> Result<(), RegistryError> {
        // Concrete impl in `stoolap_holder_registry.rs::StoolapTransaction`.
        Err(RegistryError::Storage(
            "Transaction::insert_holder_record must be invoked via StoolapTransaction".into(),
        ))
    }

    /// Insert a paired (Bearer, Capability) record atomically.
    /// Both inserts succeed or neither does.
    /// **Mission 0957-c scope:** stub. Owned by 0969-b (RFC-0969 §Algorithms:mint_dual)
    /// per the co-author contract in the 0957-c mission notes.
    pub fn insert_dual(
        &self,
        _bearer: HolderRecord,
        _capability: HolderRecord,
    ) -> Result<(), RegistryError> {
        Err(RegistryError::Storage(
            "Transaction::insert_dual is owned by 0969-b (RFC-0969 §Algorithms:mint_dual) \
             — co-author contract per 0957-c mission notes"
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
            &BearerCapsule {
                bearer_capsule_hash: [0x42; 32],
                encrypted_capsule: vec![],
                seller_signature: [0x55; 64],
            },
            &[0x77; 32],
            &octo_ident::test_helpers::sample_did(137),
            [0x33; 32],
            1_700_000_000_000,
        );
        let err = tx.insert_holder_record(rec).unwrap_err();
        assert!(matches!(err, RegistryError::Storage(_)));
    }
}
