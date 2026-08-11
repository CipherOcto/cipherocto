//! `InMemoryDidRegistry` — test + single-process `DidRegistry` impl.
//!
//! Thread-safe via `parking_lot::RwLock<HashMap<[u8;32], DidDocument>>`.
//! Production deployments use `StoolapDidRegistry` in `quota-router-storage`
//! for persistence across restarts and multi-instance sharing.
//!
//! ## Layer discipline
//!
//! This crate is `octo-ident` (Layer B per
//! [[cipherocto-design-principles]] §Layer A/B/C/D/E). The in-memory
//! impl is a test convenience; the production impl lives in
//! `quota-router-storage` (Layer B-adjacent).
//!
//! `parking_lot::RwLock` is a dev-dep only (production code uses
//! `StoolapDidRegistry`); see `Cargo.toml` for the rationale.

use std::collections::HashMap;

use parking_lot::RwLock;

use crate::registry::{DidDocument, DidRegistry, DidRegistryError};

/// In-memory `DidRegistry` impl. Thread-safe.
///
/// `Send + Sync` via `Arc<RwLock<HashMap>>` pattern (the registry is
/// typically wrapped in `Arc` at consumer boundaries).
#[derive(Default)]
pub struct InMemoryDidRegistry {
    /// Maps canonical 32-byte DID hash → `DidDocument`. Revoked DIDs
    /// remain in the map with `revoked: true` (so re-register fails
    /// per `DidRegistry::register` semantics).
    inner: RwLock<HashMap<[u8; 32], DidDocument>>,
}

impl std::fmt::Debug for InMemoryDidRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let guard = self.inner.read();
        f.debug_struct("InMemoryDidRegistry")
            .field("count", &guard.len())
            .finish_non_exhaustive()
    }
}

impl DidRegistry for InMemoryDidRegistry {
    fn register(
        &self,
        canonical_hash: &[u8; 32],
        doc: DidDocument,
    ) -> Result<(), DidRegistryError> {
        let mut guard = self.inner.write();
        if let Some(existing) = guard.get(canonical_hash) {
            if existing.revoked {
                return Err(DidRegistryError::AlreadyRevoked);
            }
        }
        guard.insert(*canonical_hash, doc);
        Ok(())
    }

    fn resolve(&self, canonical_hash: &[u8; 32]) -> Result<Option<DidDocument>, DidRegistryError> {
        let guard = self.inner.read();
        // Revoked → indistinguishable from unknown (fail-closed).
        Ok(guard.get(canonical_hash).copied().filter(|d| !d.revoked))
    }

    fn revoke(&self, canonical_hash: &[u8; 32]) -> Result<(), DidRegistryError> {
        let mut guard = self.inner.write();
        match guard.get_mut(canonical_hash) {
            Some(existing) => {
                existing.revoked = true;
                Ok(())
            }
            None => Err(DidRegistryError::UnknownDid),
        }
    }

    fn list(&self) -> Result<Vec<DidDocument>, DidRegistryError> {
        let guard = self.inner.read();
        let mut docs: Vec<DidDocument> = guard.values().copied().filter(|d| !d.revoked).collect();
        // Sort by canonical_hash ascending for deterministic iteration.
        docs.sort_by_key(|d| {
            // DidDocument does not carry canonical_hash directly; consumers
            // who need hash→doc pairs should use `resolve` per-hash. Sort
            // here uses the public_key as a stable proxy (32 bytes; total
            // ordering) — deterministic across calls.
            d.public_key
        });
        Ok(docs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hash(seed: u8) -> [u8; 32] {
        let mut h = [0u8; 32];
        for (i, b) in h.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        h
    }

    fn sample_doc(seed: u8) -> DidDocument {
        DidDocument {
            public_key: sample_hash(seed),
            revoked: false,
        }
    }

    #[test]
    fn register_resolve_round_trip() {
        let r = InMemoryDidRegistry::default();
        let h = sample_hash(1);
        let d = sample_doc(1);
        r.register(&h, d).unwrap();
        let resolved = r.resolve(&h).unwrap();
        assert_eq!(resolved, Some(d));
    }

    #[test]
    fn register_upsert_overwrites_existing() {
        let r = InMemoryDidRegistry::default();
        let h = sample_hash(2);
        r.register(&h, sample_doc(2)).unwrap();
        let new_doc = DidDocument {
            public_key: [0xFFu8; 32],
            revoked: false,
        };
        r.register(&h, new_doc).unwrap();
        assert_eq!(r.resolve(&h).unwrap(), Some(new_doc));
    }

    #[test]
    fn resolve_unknown_returns_none() {
        let r = InMemoryDidRegistry::default();
        assert_eq!(r.resolve(&sample_hash(99)).unwrap(), None);
    }

    #[test]
    fn revoke_marks_resolve_none() {
        let r = InMemoryDidRegistry::default();
        let h = sample_hash(3);
        r.register(&h, sample_doc(3)).unwrap();
        r.revoke(&h).unwrap();
        // Revoked → indistinguishable from unknown.
        assert_eq!(r.resolve(&h).unwrap(), None);
    }

    #[test]
    fn revoke_unknown_errors() {
        let r = InMemoryDidRegistry::default();
        let err = r.revoke(&sample_hash(42)).unwrap_err();
        assert_eq!(err, DidRegistryError::UnknownDid);
    }

    #[test]
    fn register_after_revoke_errors() {
        let r = InMemoryDidRegistry::default();
        let h = sample_hash(4);
        r.register(&h, sample_doc(4)).unwrap();
        r.revoke(&h).unwrap();
        let err = r.register(&h, sample_doc(4)).unwrap_err();
        assert_eq!(err, DidRegistryError::AlreadyRevoked);
    }

    #[test]
    fn list_returns_all_active_dids() {
        let r = InMemoryDidRegistry::default();
        r.register(&sample_hash(10), sample_doc(10)).unwrap();
        r.register(&sample_hash(11), sample_doc(11)).unwrap();
        r.register(&sample_hash(12), sample_doc(12)).unwrap();
        r.revoke(&sample_hash(11)).unwrap();
        let docs = r.list().unwrap();
        assert_eq!(docs.len(), 2);
        // Sorted by public_key (proxy for canonical_hash) ascending.
        assert_eq!(docs[0].public_key, sample_hash(10));
        assert_eq!(docs[1].public_key, sample_hash(12));
    }

    #[test]
    fn revoke_is_idempotent_for_already_revoked() {
        let r = InMemoryDidRegistry::default();
        let h = sample_hash(5);
        r.register(&h, sample_doc(5)).unwrap();
        r.revoke(&h).unwrap();
        // Second revoke: row IS still present (just marked revoked), so
        // idempotent semantics — no error.
        r.revoke(&h)
            .expect("idempotent revoke of revoked DID must succeed");
    }
}
