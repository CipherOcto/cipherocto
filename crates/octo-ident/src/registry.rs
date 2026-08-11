//! `DidRegistry` trait — DID document storage substrate (RFC-0010 v1.3).
//!
//! Maps a canonical 32-byte DID hash (`RawDid::hash`, NOT the `WireDid`
//! typed wrapper — see [[cipherocto-design-principles]] §Stable
//! Abstractions Principle) to a `DidDocument` (public_key + revoked flag).
//!
//! ## Why raw byte slices (not typed `WireDid`)
//!
//! This trait lives in `octo-ident` (Layer B). The `StoolapDidRegistry`
//! impl lives in `quota-router-storage` (Layer B-adjacent). Having the
//! trait accept raw byte slices (instead of typed `WireDid`) breaks what
//! would otherwise be a cyclic crate dependency: `quota-router-storage`
//! cannot depend on `octo-ident` (which owns `WireDid`) without
//! `octo-identity-resolver-node` (which depends on both) losing its
//! trait-object dispatchability. Same pattern as `StoolapSpendLedger`
//! (`crates/quota-router-storage/src/stoolap_spend_ledger.rs`).
//!
//! ## Layer discipline
//!
//! Per [[cipherocto-design-principles]] §Layer A/B/C/D/E:
//! - `octo-ident` (Layer B) — trait + `InMemoryDidRegistry` test impl
//! - `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
//! - `octo-identity-resolver-node` (Layer C) — consumer only; NO dep on
//!   `quota-router-storage` (registry injected via `Arc<dyn DidRegistry>`)
//!
//! ## Out of scope (deferred to future RFCs)
//!
//! - Cross-instance write coordination (F7) — RFC-0862 amendment (separate mission)
//! - `ResolverBackend` typed view (F6) — RFC-0871 §Future Work
//! - Multi-chain DID resolution (F2) — RFC-0010 §Future Work
//! - Rich DID Documents (service endpoints, controller refs, capability delegation)

#![forbid(unsafe_code)]

use thiserror::Error;

/// DID document — the canonical storage record for a registered DID.
///
/// Per RFC-0010 v1.3 §Storage Extension §Data Structures, the document
/// is the minimum surface: a 32-byte public key + a `revoked` flag.
/// Rich DID Documents (service endpoints, controller refs, capability
/// delegation) are explicitly OUT of scope per RFC-0010 v1.3 and ship
/// in a future amendment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct DidDocument {
    /// 32-byte Ed25519 public key bound to the DID.
    pub public_key: [u8; 32],
    /// True when the DID has been revoked; resolve() returns `Ok(None)`
    /// for revoked DIDs (fail-closed semantics per RFC-0010 v1.3 §Storage
    /// Extension §Compatibility).
    pub revoked: bool,
}

/// Errors returned by `DidRegistry` operations.
///
/// All implementations (in-memory + production) MUST return only these
/// variants; storage-backend-specific error strings are tunneled via
/// `Storage(String)`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DidRegistryError {
    /// `register` called for a DID that was previously revoked.
    /// Registration after revocation is rejected (fail-closed per
    /// RFC-0010 v1.3 §Compatibility).
    #[error("cannot re-register revoked DID")]
    AlreadyRevoked,

    /// `revoke` called for a DID that has no record (not registered).
    #[error("unknown DID: not registered")]
    UnknownDid,

    /// Underlying storage failure (e.g. Stoolap error).
    #[error("storage error: {0}")]
    Storage(String),
}

/// Trait for DID document storage.
///
/// All methods are `&self` + `Send + Sync` (the trait is dyn-compatible
/// via `Arc<dyn DidRegistry>` injection at consumer boundaries).
///
/// The `canonical_hash` parameter is the decoded 32-byte `RawDid::hash`
/// — NOT the wire form `WireDid`. Consumers convert at the boundary
/// (`WireDid` → `RawDid::hash` via `CanonicalCodec::wire_to_raw`).
pub trait DidRegistry: Send + Sync + 'static {
    /// Register or upsert a `DidDocument` for `canonical_hash`.
    ///
    /// Upsert semantics: if a record already exists (and is NOT revoked),
    /// the existing `DidDocument` is replaced with the new one. If the
    /// record exists but IS revoked, returns `Err(AlreadyRevoked)`
    /// (fail-closed: re-registration post-revocation requires explicit
    /// amendment — out of scope for v1.3).
    ///
    /// # Errors
    /// - `DidRegistryError::AlreadyRevoked` if existing record is revoked.
    /// - `DidRegistryError::Storage` on underlying storage failure.
    fn register(&self, canonical_hash: &[u8; 32], doc: DidDocument)
        -> Result<(), DidRegistryError>;

    /// Resolve a `canonical_hash` to its `DidDocument`.
    ///
    /// Returns `Ok(None)` for unknown DIDs (not registered) AND for
    /// revoked DIDs (fail-closed: revoked DIDs are indistinguishable
    /// from unregistered DIDs to consumers).
    ///
    /// # Errors
    /// - `DidRegistryError::Storage` on underlying storage failure.
    fn resolve(&self, canonical_hash: &[u8; 32]) -> Result<Option<DidDocument>, DidRegistryError>;

    /// Mark a `canonical_hash` as revoked.
    ///
    /// Idempotent: revoking an already-revoked DID is a no-op (returns
    /// `Ok(())`). Revoking an unknown DID returns `Err(UnknownDid)`.
    ///
    /// # Errors
    /// - `DidRegistryError::UnknownDid` if no record exists.
    /// - `DidRegistryError::Storage` on underlying storage failure.
    fn revoke(&self, canonical_hash: &[u8; 32]) -> Result<(), DidRegistryError>;

    /// List all active (non-revoked) DID documents.
    ///
    /// Returns documents sorted by `canonical_hash` ascending for
    /// deterministic iteration (RFC-0010 v1.3 §Determinism Requirements).
    /// Revoked documents are filtered out — the trait surfaces only
    /// the active view.
    ///
    /// # Errors
    /// - `DidRegistryError::Storage` on underlying storage failure.
    fn list(&self) -> Result<Vec<DidDocument>, DidRegistryError>;
}
