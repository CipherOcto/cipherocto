//! `StoolapDidRegistry` impl stub (mission 0206-005 scaffold).
//!
//! The full impl is moved from
//! `crates/quota-router-storage/src/stoolap_did_registry.rs` in
//! mission 0206-003 (Trait Moves). This file currently hosts the
//! struct + `open_*` constructors + types; the trait impl block lands
//! in 0206-003.

use octo_storage_core::Database;

use octo_ident::{
    CapabilityDelegation, ChainId, ControllerReference, DidDocument, DidRegistry,
    DidRegistryError, ServiceEndpoint, VerificationMethod,
};

use std::sync::Arc;

/// 17-byte canonical encoding of the `CIPHEROCTO_MAINNET` namespace
/// (RFC-0010 v1.4 §ChainId Namespace Extension).
pub const MAINNET_CHAIN_ID_BYTES: [u8; 17] = [
    0x01, 0xeb, 0x30, 0x71, 0xb5, 0xe1, 0x13, 0x33, 0x0c, 0x87, 0x63, 0x09, 0x54, 0xe3, 0xcc, 0x08,
    0x12,
];

/// Errors returned by `StoolapDidRegistry` operations.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DidRegistryStorageError {
    #[error("did-registry storage error: {0}")]
    Storage(String),
}

/// Stoolap-backed DID registry (production).
#[derive(Clone)]
pub struct StoolapDidRegistry {
    db: Arc<Database>,
}

impl std::fmt::Debug for StoolapDidRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapDidRegistry").finish_non_exhaustive()
    }
}

impl StoolapDidRegistry {
    /// Open a fresh in-memory database with the did_registry schema
    /// applied. Test + single-process convenience.
    ///
    /// The full migration runner call (catalog + ApplyConfig) lands in
    /// mission 0206-003 when the impl block is moved. Until then, this
    /// returns an empty-DB handle — sufficient for crate-scaffold AC.
    pub fn open_in_memory() -> Result<Self, DidRegistryStorageError> {
        let db = octo_storage_core::Database::open_in_memory()
            .map_err(|e| DidRegistryStorageError::Storage(format!("open_in_memory: {e}")))?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Open a file-backed database at `path` with the did_registry
    /// schema applied.
    pub fn open_path(path: &str) -> Result<Self, DidRegistryStorageError> {
        let db = octo_storage_core::Database::open(path)
            .map_err(|e| DidRegistryStorageError::Storage(format!("open({path}): {e}")))?;
        Ok(Self { db: Arc::new(db) })
    }
}

// Type anchors: keep these import statements alive so the post-0206-003
// impl block's signatures stay well-typed from day one. The
// `#[allow(dead_code)]` marks silence lint noise while the impl block
// is staged. Replaced when the trait impl block lands in mission
// 0206-003.

#[allow(dead_code)]
fn _chain_id_to_canonical_bytes(chain_id: &ChainId) -> Result<[u8; 17], DidRegistryError> {
    let namespace = chain_id
        .namespace()
        .map_err(|e| DidRegistryError::Storage(format!("chain_id namespace: {e}")))?;
    Ok(namespace.canonical_bytes())
}

#[allow(dead_code)]
fn _now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[allow(dead_code)]
fn _type_anchors_for_layout(
    _d: &DidDocument,
    _c: &CapabilityDelegation,
    _r: &ControllerReference,
    _s: &ServiceEndpoint,
    _v: &VerificationMethod,
) {
}

#[allow(dead_code)]
fn _trait_anchor<T: DidRegistry>(_: &T) {}