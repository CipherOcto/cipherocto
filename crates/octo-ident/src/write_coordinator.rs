//! `DidWriteCoordinator` trait — cross-instance DID write routing substrate
//! (RFC-0862 v1.3 §DidWriteCoordinator).
//!
//! This trait defines the contract for coordinating DID writes
//! (`register` / `revoke`) across instances of `StoolapDidRegistry`.
//! Production HA / sharded deployments inject an
//! `Arc<dyn DidWriteCoordinator>` at the registry construction boundary
//! so the per-instance mutex + writer-election pattern (mission
//! `0871e-f7-cross-instance-did-coordination`) can mediate cross-instance
//! atomicity without `StoolapDidRegistry` gaining a hard dep on the
//! coordinator crate.
//!
//! ## Why in `octo-ident` (Layer B), not `quota-router-storage`
//!
//! Per [[cipherocto-design-principles]] §Layer discipline:
//! - `octo-ident` (Layer B) — substrate trait + error enum + `ChainId`
//! - `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
//!   consumes the coordinator via `Arc<dyn DidWriteCoordinator>`
//! - `octo-sync` (Layer B-substrate) — concrete coordinator impl
//!   (deferred to RFC-0862 v1.4 amendment)
//!
//! The trait surface is sealed: external crates cannot add methods;
//! only the substrate crate can. Same pattern as `octo-protocol`'s
//! `PayloadKindId` UUID allocation.
//!
//! ## Default impls (RFC-0862 v1.3 §DidWriteCoordinator)
//!
//! - `submit_register` — validates `canonical_hash(document) ==
//!   canonical_did_hash` then calls `submit_register_validated`. Per
//!   RFC-0862 v1.3 R11 H2 + R13 M5 the `canonical_hash` function
//!   currently lives inline in this crate; future `octo_sync::did`
//!   module will re-export it for cross-crate reuse.
//! - `submit_register_validated` — required; coordinator impl owns the
//!   writer-election / WAL / cross-instance dispatch.
//! - `submit_revoke` — required; same shape as
//!   `submit_register_validated` minus the document payload.
//! - `submit_register_local_fallback` — deprecated; returns
//!   `WriterUnavailable` by default. Per RFC-0862 v1.3 R12 the local
//!   LWW fallback is gated behind a future amendment (F12 / F13); the
//!   extension hook is preserved via the trait method shape.
//!
//! ## Out of scope (deferred)
//!
//! - Concrete `WriterElection`-backed coordinator impl (RFC-0862 v1.4
//!   amendment; mission `0871e-f7-cross-instance-did-coordination`).
//! - LWW local fallback (RFC-0862 v1.3 F12 + F13).
//! - `ChainId` typed-namespace + federation semantics (RFC-0010 v1.4
//!   amendment; mission `0010-f2-multi-chain-did-resolution`).

#![forbid(unsafe_code)]

use async_trait::async_trait;
use thiserror::Error;

use crate::chain::ChainId;
use crate::registry::DidDocument;

/// Errors returned by `DidWriteCoordinator` operations.
///
/// Per RFC-0862 v1.3 R12 M19 the error enum lives in `octo-ident`
/// (substrate-layer) so consumer crates can map to their own error
/// domains without coupling to a coordinator-impl crate.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DidWriteCoordinatorError {
    /// The writer-election substrate has no currently-elected leader
    /// (e.g., during failover window, partition, or bootstrap).
    /// Default `submit_register_local_fallback` returns this to enforce
    /// fail-closed semantics per RFC-0862 v1.3 R12.
    #[error("writer unavailable")]
    WriterUnavailable,

    /// `submit_register` was called with a `canonical_did_hash` that
    /// does not match `canonical_hash(document)` — i.e., the caller
    /// claimed a DID name that the document does not actually bind to.
    /// This is a programming error, NOT a transient failure.
    #[error("hash/document mismatch")]
    HashDocumentMismatch,

    /// `submit_register` / `submit_revoke` was called with a `chain_id`
    /// that does not match the coordinator's deployment-bound chain.
    /// Indicates a deployment misconfiguration or a misrouted request.
    #[error("chain_id mismatch")]
    ChainIdMismatch,

    /// Coordinator's underlying WAL detected corruption during the
    /// write. The write was NOT applied; the DID state is unchanged.
    #[error("WAL corruption detected: {0}")]
    WalCorruption(String),
}

/// Sealed trait pattern: only the substrate crate (`octo-sync`) can
/// implement `DidWriteCoordinator`.
///
/// Per RFC-0862 v1.3 R12 + RFC-0862 v1.4 §Concrete Impl Extension:
/// the substrate crate `octo-sync` provides the concrete
/// `RaftLikeDidWriteCoordinator` impl. The seal is `pub` (not `pub(crate)`
/// or private) so the substrate crate can `impl DidWriteCoordinatorSealed`
/// at its own crate boundary. Downstream crates (consumers of the
/// resolved `Arc<dyn DidWriteCoordinator>`) cannot add new impls
/// because they have no reason to import `octo_ident::sealed` for
/// production use — the trait is gated to the substrate crate via
/// the layer model (per [[cipherocto-design-principles]] §Layer
/// direction + §No parallel abstractions).
pub mod sealed {
    /// Sealed marker for `DidWriteCoordinator` impls. Only the
    /// substrate crate (`octo-sync`) implements this trait. See
    /// `octo_ident::write_coordinator` module docs for the layer
    /// model rationale.
    pub trait DidWriteCoordinatorSealed {}
}

/// Cross-instance DID write coordination contract (RFC-0862 v1.3
/// §DidWriteCoordinator).
///
/// All implementations are `Send + Sync` (the trait is dyn-compatible
/// via `Arc<dyn DidWriteCoordinator>` injection at the
/// `StoolapDidRegistry` construction boundary).
#[async_trait]
pub trait DidWriteCoordinator: sealed::DidWriteCoordinatorSealed + Send + Sync {
    /// Submit a `register` request to the coordinator.
    ///
    /// Default impl validates `canonical_hash(document) ==
    /// canonical_did_hash` then delegates to
    /// `submit_register_validated`. The validation guards against
    /// caller mis-binding a DID name to a document that does not
    /// hash to that name — a programming-error class of defect.
    ///
    /// # Errors
    /// - `HashDocumentMismatch` if the caller-supplied
    ///   `canonical_did_hash` does not match
    ///   `canonical_hash(document)`.
    /// - Coordinator-impl-specific errors from
    ///   `submit_register_validated` (typically `WriterUnavailable`,
    ///   `ChainIdMismatch`, or `WalCorruption`).
    async fn submit_register(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
        document: &DidDocument,
    ) -> Result<(), DidWriteCoordinatorError> {
        let computed = canonical_hash(document);
        if canonical_did_hash != &computed {
            return Err(DidWriteCoordinatorError::HashDocumentMismatch);
        }
        self.submit_register_validated(canonical_did_hash, chain_id, document)
            .await
    }

    /// Coordinator-owned register logic. Called by `submit_register`
    /// after the canonical-hash check passes.
    ///
    /// Implementations route the write through the writer-election
    /// substrate (Option B per
    /// [[drain-coordinator-approach-2026-08-10]]) and replicate via
    /// `octo_sync::DatabaseSyncAdapter` so all instances see the
    /// write before the function returns success.
    async fn submit_register_validated(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
        document: &DidDocument,
    ) -> Result<(), DidWriteCoordinatorError>;

    /// Submit a `revoke` request to the coordinator.
    ///
    /// No canonical-hash validation (revoke takes only the DID hash +
    /// chain, not a document). Implementations route through the
    /// same writer-election substrate as `submit_register_validated`.
    ///
    /// # Errors
    /// - `WriterUnavailable` if no leader is currently elected.
    /// - `ChainIdMismatch` if the chain does not match the
    ///   coordinator's deployment binding.
    /// - `WalCorruption` on underlying storage failure.
    async fn submit_revoke(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
    ) -> Result<(), DidWriteCoordinatorError>;

    /// Deprecated LWW local-fallback extension hook.
    ///
    /// Per RFC-0862 v1.3 R12 this method is preserved as a trait
    /// extension point for the future F12 (HLC) + F13 (LWW counter)
    /// amendments. The default impl returns `WriterUnavailable` to
    /// enforce fail-closed semantics; a future LWW impl will
    /// override this to perform an optimistic local write with
    /// HLC-stamped reconciliation during the failover window.
    #[deprecated(
        since = "1.3.0",
        note = "LWW substrate pending F12/F13 amendment; fail-closed default"
    )]
    async fn submit_register_local_fallback(
        &self,
        _canonical_did_hash: &[u8; 32],
        _chain_id: &ChainId,
        _document: &DidDocument,
    ) -> Result<(), DidWriteCoordinatorError> {
        Err(DidWriteCoordinatorError::WriterUnavailable)
    }
}

/// Canonical-hash derivation for a `DidDocument`.
///
/// Per RFC-0862 v1.3 R13 M5 the canonical hash binds the DID name to
/// the document's `public_key` via the same binding-domain BLAKE3
/// construction used by `CanonicalCodec::mint`. Currently this lives
/// inline in `octo-ident`; future `octo_sync::did` module will
/// re-export it for cross-crate reuse without a dependency cycle.
///
/// **Why inline now:** the `octo-sync` crate does not yet exist as a
/// workspace member (it lands with the RFC-0862 v1.4 amendment that
/// ships the concrete `WriterElection`-backed coordinator impl). To
/// preserve Layer B additive-only per
/// [[cipherocto-design-principles]], the canonical-hash function
/// stays in `octo-ident` until the consumer crate is committed.
#[must_use]
pub fn canonical_hash(doc: &DidDocument) -> [u8; 32] {
    const BINDING_DOMAIN: &[u8] = b"cipherocto/octoid/v1";
    let mut hasher = blake3::Hasher::new();
    hasher.update(BINDING_DOMAIN);
    hasher.update(&doc.public_key);
    let out = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(out.as_bytes());
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DidCodec;
    use std::sync::Arc;

    fn sample_doc(seed: u8) -> DidDocument {
        let mut public_key = [0u8; 32];
        public_key[31] = seed;
        DidDocument {
            public_key,
            revoked: false,
            ..Default::default()
        }
    }

    #[test]
    fn canonical_hash_is_deterministic() {
        let doc = sample_doc(7);
        let a = canonical_hash(&doc);
        let b = canonical_hash(&doc);
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_hash_distinguishes_public_keys() {
        let a = canonical_hash(&sample_doc(1));
        let b = canonical_hash(&sample_doc(2));
        assert_ne!(a, b, "distinct public keys must produce distinct hashes");
    }

    #[test]
    fn canonical_hash_matches_mint_hash() {
        // The canonical_hash of a document MUST equal
        // CanonicalCodec::mint(doc.public_key).hash — this is the
        // invariant `submit_register` validates.
        let doc = sample_doc(42);
        let minted = crate::CanonicalCodec::mint(&doc.public_key);
        assert_eq!(canonical_hash(&doc), minted.hash);
    }

    #[test]
    fn chain_id_round_trips_via_display() {
        let cid = ChainId::new("cipherocto-mainnet").expect("static literal");
        assert_eq!(cid.as_str(), "cipherocto-mainnet");
        assert_eq!(format!("{cid}"), "cipherocto-mainnet");
    }

    #[test]
    fn error_display_messages_are_nonempty() {
        // Each variant must produce a non-empty message (operator
        // observability — no `#[error("")]` placeholders).
        let cases = [
            DidWriteCoordinatorError::WriterUnavailable,
            DidWriteCoordinatorError::HashDocumentMismatch,
            DidWriteCoordinatorError::ChainIdMismatch,
            DidWriteCoordinatorError::WalCorruption("test".to_owned()),
        ];
        for err in &cases {
            assert!(!err.to_string().is_empty(), "{err:?} has empty message");
        }
    }

    /// Mock coordinator used for default-impl tests. Implements only
    /// the required methods (no override of the deprecated LWW hook).
    struct MockCoordinator {
        /// Records the last `(canonical_did_hash, chain_id, document)`
        /// tuple passed to `submit_register_validated`. Used to assert
        /// the default impl delegates correctly after the
        /// canonical-hash check.
        last_register: parking_lot::Mutex<Option<([u8; 32], ChainId, DidDocument)>>,
        /// Records the last `(canonical_did_hash, chain_id)` tuple
        /// passed to `submit_revoke`.
        last_revoke: parking_lot::Mutex<Option<([u8; 32], ChainId)>>,
        /// When set, `submit_register_validated` returns this error
        /// instead of recording the call.
        register_error: parking_lot::Mutex<Option<DidWriteCoordinatorError>>,
    }

    impl MockCoordinator {
        fn new() -> Self {
            Self {
                last_register: parking_lot::Mutex::new(None),
                last_revoke: parking_lot::Mutex::new(None),
                register_error: parking_lot::Mutex::new(None),
            }
        }
    }

    impl sealed::DidWriteCoordinatorSealed for MockCoordinator {}

    #[async_trait]
    impl DidWriteCoordinator for MockCoordinator {
        async fn submit_register_validated(
            &self,
            canonical_did_hash: &[u8; 32],
            chain_id: &ChainId,
            document: &DidDocument,
        ) -> Result<(), DidWriteCoordinatorError> {
            if let Some(err) = self.register_error.lock().clone() {
                return Err(err);
            }
            *self.last_register.lock() =
                Some((*canonical_did_hash, chain_id.clone(), document.clone()));
            Ok(())
        }

        async fn submit_revoke(
            &self,
            canonical_did_hash: &[u8; 32],
            chain_id: &ChainId,
        ) -> Result<(), DidWriteCoordinatorError> {
            *self.last_revoke.lock() = Some((*canonical_did_hash, chain_id.clone()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn submit_register_validates_canonical_hash_and_delegates() {
        let coord = MockCoordinator::new();
        let doc = sample_doc(11);
        let hash = canonical_hash(&doc);
        let chain = ChainId::new("test-chain").expect("static literal");

        // Matching hash delegates to submit_register_validated.
        let result = coord.submit_register(&hash, &chain, &doc).await;
        assert!(result.is_ok(), "expected Ok, got {result:?}");

        let recorded = coord.last_register.lock().clone();
        assert!(
            recorded.is_some(),
            "submit_register_validated was not called"
        );
        let (recorded_hash, recorded_chain, recorded_doc) = recorded.unwrap();
        assert_eq!(recorded_hash, hash);
        assert_eq!(recorded_chain, chain);
        assert_eq!(recorded_doc, doc);
    }

    #[tokio::test]
    async fn submit_register_rejects_hash_document_mismatch() {
        let coord = MockCoordinator::new();
        let doc = sample_doc(13);
        let wrong_hash = [0xFFu8; 32];
        let chain = ChainId::new("test-chain").expect("static literal");

        let result = coord.submit_register(&wrong_hash, &chain, &doc).await;
        assert_eq!(
            result,
            Err(DidWriteCoordinatorError::HashDocumentMismatch),
            "expected HashDocumentMismatch, got {result:?}"
        );

        // Coordinator's submit_register_validated MUST NOT have been
        // called (validation short-circuits before delegation).
        assert!(
            coord.last_register.lock().is_none(),
            "validation must short-circuit before delegation"
        );
    }

    #[tokio::test]
    async fn submit_register_propagates_coordinator_error() {
        let coord = MockCoordinator::new();
        *coord.register_error.lock() = Some(DidWriteCoordinatorError::WriterUnavailable);

        let doc = sample_doc(17);
        let hash = canonical_hash(&doc);
        let chain = ChainId::new("test-chain").expect("static literal");

        let result = coord.submit_register(&hash, &chain, &doc).await;
        assert_eq!(
            result,
            Err(DidWriteCoordinatorError::WriterUnavailable),
            "expected WriterUnavailable from coordinator, got {result:?}"
        );
    }

    #[tokio::test]
    async fn submit_revoke_records_call() {
        let coord = MockCoordinator::new();
        let hash = [0x42u8; 32];
        let chain = ChainId::new("test-chain").expect("static literal");

        let result = coord.submit_revoke(&hash, &chain).await;
        assert!(result.is_ok());

        let recorded = coord.last_revoke.lock().clone();
        assert!(recorded.is_some(), "submit_revoke was not called");
        let (recorded_hash, recorded_chain) = recorded.unwrap();
        assert_eq!(recorded_hash, hash);
        assert_eq!(recorded_chain, chain);
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn submit_register_local_fallback_returns_writer_unavailable_by_default() {
        // Per RFC-0862 v1.3 R12 the default LWW fallback returns
        // WriterUnavailable. Fail-closed semantics; future LWW impls
        // override this method.
        let coord = MockCoordinator::new();
        let doc = sample_doc(19);
        let hash = canonical_hash(&doc);
        let chain = ChainId::new("test-chain").expect("static literal");

        let result = coord
            .submit_register_local_fallback(&hash, &chain, &doc)
            .await;
        assert_eq!(
            result,
            Err(DidWriteCoordinatorError::WriterUnavailable),
            "default LWW fallback must fail-closed"
        );
    }

    #[tokio::test]
    async fn dyn_compatible_via_arc() {
        // Verify the trait is dyn-compatible by injecting via
        // `Arc<dyn DidWriteCoordinator>`.
        let coord: Arc<dyn DidWriteCoordinator> = Arc::new(MockCoordinator::new());
        let doc = sample_doc(23);
        let hash = canonical_hash(&doc);
        let chain = ChainId::new("test-chain").expect("static literal");

        let result = coord.submit_register(&hash, &chain, &doc).await;
        assert!(result.is_ok(), "dyn-dispatched call failed: {result:?}");
    }
}
