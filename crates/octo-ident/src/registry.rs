//! `DidRegistry` trait — DID document storage substrate (RFC-0010 v1.3 + v1.5).
//!
//! Maps a canonical 32-byte DID hash (`RawDid::hash`, NOT the `WireDid`
//! typed wrapper — see [[cipherocto-design-principles]] §Stable
//! Abstractions Principle) to a `DidDocument` (public_key + revoked flag
//! + v1.5 rich surface).
//!
//! ## Multi-chain namespacing (RFC-0010 v1.4 + mission 0010-f2-registry-namespacing)
//!
//! The trait gains two ADDITIVE methods (`register_in_chain` +
//! `resolve_in_chain`) with default impls that forward to the
//! single-chain `register` / `resolve`. This preserves Layer B
//! back-compat per [[cipherocto-design-principles]] §Stable
//! Abstractions Principle — existing impls (`InMemoryDidRegistry`)
//! continue to compile unchanged. Production storage
//! (`StoolapDidRegistry` in `quota-router-storage`) overrides
//! both methods with chain-aware SQL that filters on the
//! `chain_id` BLOB column.
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
//! ## RFC-0010 v1.5 (additive on v1.3)
//!
//! v1.5 extends `DidDocument` with the W3C DID Core 1.0 surface:
//! service endpoints, controller refs, verification methods, capability
//! delegations. v1.3 docs (public_key + revoked only) remain backward
//! compatible because all new fields default to empty `Vec`s.
//!
//! ## Out of scope (deferred to future RFCs)
//!
//! - Cross-instance write coordination (F7) — RFC-0862 amendment (separate mission)
//! - `ResolverBackend` typed view — landed in `crate::resolver_backend`
//!   (mission 0871b-cross-node-forwarding AC-1); consumer is
//!   `octo-identity-resolver-node` Layer C.
//! - Multi-chain DID resolution (F2) — RFC-0010 §Future Work
//! - `StoolapDidRegistry` schema migration v009/v010 (rich-document JSON columns) — separate mission
//! - `canonical_hash` over rich fields — currently hashes only the stable
//!   `(public_key, revoked)` tuple so adding service endpoints doesn't
//!   shift the DID identity (W3C DID Core 1.0 invariant: identity ≠
//!   document content).

#![forbid(unsafe_code)]

use thiserror::Error;

use crate::rich_document::{
    CapabilityDelegation, ControllerReference, ServiceEndpoint, VerificationMethod,
};

/// DID document — the canonical storage record for a registered DID.
///
/// Per RFC-0010 v1.5 §Data Structures, the document carries:
/// - the MVP `public_key` + `revoked` flag (v1.3)
/// - `service_endpoints` (typed `Vec<ServiceEndpoint>`)
/// - `controllers` (typed `Vec<ControllerReference>`)
/// - `verification_methods` (typed `Vec<VerificationMethod>`)
/// - `capability_delegations` (typed `Vec<CapabilityDelegation>`)
///
/// `Copy` + `Hash` were DROPPED in v1.5 because `Vec` is neither. All
/// construction sites that previously wrote `DidDocument { public_key, revoked }`
/// continue to work because new fields default to empty `Vec`s.
///
/// ## Canonical identity
///
/// Per W3C DID Core 1.0 invariant "DID identity ≠ DID document content",
/// `canonical_hash` (in `octo-ident::write_coordinator`) hashes only the
/// stable `(public_key, revoked)` tuple. Adding service endpoints or
/// verification methods does NOT shift the DID identity — those are
/// document updates, not DID regenerations.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
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
    /// v1.5: typed service endpoints for resolver discovery.
    /// Bounded by [`MAX_SERVICE_ENDPOINTS`](crate::rich_document::MAX_SERVICE_ENDPOINTS).
    pub service_endpoints: Vec<ServiceEndpoint>,
    /// v1.5: typed controller references for hierarchical delegation.
    /// Bounded by [`MAX_CONTROLLERS`](crate::rich_document::MAX_CONTROLLERS).
    pub controllers: Vec<ControllerReference>,
    /// v1.5: typed verification methods for multi-key DID.
    /// Bounded by [`MAX_VERIFICATION_METHODS`](crate::rich_document::MAX_VERIFICATION_METHODS).
    pub verification_methods: Vec<VerificationMethod>,
    /// v1.5: typed capability delegation hashes.
    /// Bounded by [`MAX_CAPABILITY_DELEGATIONS`](crate::rich_document::MAX_CAPABILITY_DELEGATIONS).
    pub capability_delegations: Vec<CapabilityDelegation>,
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

    /// Register a DID on an explicit chain namespace (RFC-0010 v1.4).
    ///
    /// Default impl forwards to `register` (single-chain mode) for
    /// back-compat with existing `DidRegistry` impls. Production
    /// storage overrides this with chain-aware SQL.
    ///
    /// # Errors
    /// - `DidRegistryError::AlreadyRevoked` if existing record on
    ///   the same chain is revoked.
    /// - `DidRegistryError::Storage` on underlying storage failure.
    fn register_in_chain(
        &self,
        _chain_id: &crate::chain::ChainId,
        canonical_hash: &[u8; 32],
        doc: DidDocument,
    ) -> Result<(), DidRegistryError> {
        self.register(canonical_hash, doc)
    }

    /// Resolve a DID on an explicit chain namespace (RFC-0010 v1.4).
    ///
    /// Default impl forwards to `resolve` (single-chain mode) for
    /// back-compat with existing `DidRegistry` impls. Production
    /// storage overrides this with chain-aware SQL.
    ///
    /// Returns `Ok(None)` for unknown DIDs (not registered on the
    /// given chain) AND for revoked DIDs.
    ///
    /// # Errors
    /// - `DidRegistryError::Storage` on underlying storage failure.
    fn resolve_in_chain(
        &self,
        _chain_id: &crate::chain::ChainId,
        canonical_hash: &[u8; 32],
    ) -> Result<Option<DidDocument>, DidRegistryError> {
        self.resolve(canonical_hash)
    }
}
