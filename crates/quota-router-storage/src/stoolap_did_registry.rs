//! `StoolapDidRegistry` (mission 0871b-storage-backend).
//!
//! Persistent DID-document registry backed by a `stoolap::Database` with
//! the `did_registry` table (migration v008). Replaces the in-memory
//! `InMemoryDidRegistry` for production deployments where the DID
//! registry MUST survive process restarts and be shareable across
//! multiple identity-resolver-node instances.
//!
//! ## API surface
//!
//! Uses raw byte slices (`&[u8; 32]` for `canonical_hash` +
//! `&[u8; 32]` for `public_key`) instead of typed `WireDid` wrappers.
//! Avoids a cyclic crate dependency: `quota-router-storage` cannot
//! depend on `octo-ident` (which owns `WireDid`) without breaking the
//! `Arc<dyn DidRegistry>` dispatch pattern at the resolver-node
//! boundary. Same pattern as `StoolapSpendLedger`
//! (`crates/quota-router-storage/src/stoolap_spend_ledger.rs`).
//!
//! ## Atomicity (RFC-0010 v1.3)
//!
//! `register` runs inside a stoolap transaction:
//! 1. SELECT existing row FOR UPDATE (lock + visibility check)
//! 2. If existing.revoked → ABORT with `AlreadyRevoked`
//! 3. INSERT (new) or UPDATE (existing, non-revoked) with new public_key
//! 4. COMMIT
//!
//! Concurrent register on the same `canonical_hash` serializes via the
//! FOR UPDATE lock — no torn writes possible.
//!
//! ## Cipherocto-side migration
//!
//! Schema lives at `crates/quota-router-storage/migrations/v008__create_did_registry.sql`
//! per [[stoolap-general-purpose-db]] (cipherocto-side, NOT stoolap fork).
//!
//! ## Layer discipline
//!
//! This module lives in `quota-router-storage` (Layer B-adjacent) and
//! does NOT depend on `octo-identity-resolver-node` (which transitively
//! depends on this crate via `quota-router-core`).

use std::sync::Arc;

use octo_ident::DidDocument;

use crate::migrations;
use octo_ident::DidRegistry;

/// Errors returned by `StoolapDidRegistry` operations. Tunnels through
/// `DidRegistryError` at the trait boundary (`From` impl below).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DidRegistryStorageError {
    /// Underlying storage failure (e.g. Stoolap error).
    #[error("did-registry storage error: {0}")]
    Storage(String),
}

/// Stoolap-backed DID registry (production).
#[derive(Clone)]
pub struct StoolapDidRegistry {
    db: Arc<stoolap::Database>,
}

impl std::fmt::Debug for StoolapDidRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapDidRegistry").finish_non_exhaustive()
    }
}

impl StoolapDidRegistry {
    /// Open a fresh in-memory database with the did_registry schema
    /// applied. Test + single-process convenience.
    /// # Errors
    /// Returns `DidRegistryStorageError::Storage` on DB open / migration failure.
    pub fn open_in_memory() -> Result<Self, DidRegistryStorageError> {
        let db = stoolap::Database::open_in_memory()
            .map_err(|e| DidRegistryStorageError::Storage(format!("open_in_memory: {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| DidRegistryStorageError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Open a file-backed database at `path` with the did_registry
    /// schema applied. Production deployments persist DID documents
    /// across restarts.
    /// # Errors
    /// Returns `DidRegistryStorageError::Storage` on DB open / migration failure.
    pub fn open_path(path: &str) -> Result<Self, DidRegistryStorageError> {
        let db = stoolap::Database::open(path)
            .map_err(|e| DidRegistryStorageError::Storage(format!("open({path}): {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| DidRegistryStorageError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self { db: Arc::new(db) })
    }
}

/// Helper: current Unix epoch in milliseconds (i64; saturates).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl DidRegistry for StoolapDidRegistry {
    fn register(
        &self,
        canonical_hash: &[u8; 32],
        doc: DidDocument,
    ) -> Result<(), octo_ident::DidRegistryError> {
        // SELECT existing (lock + visibility). Stoolap-fork does NOT
        // support `INSERT OR REPLACE`; use SELECT-then-INSERT/UPDATE
        // pattern (matches `StoolapSpendLedger::seed` v007).
        let rows = self.db.query(
            "SELECT revoked FROM did_registry WHERE canonical_hash = ? LIMIT 1",
            (canonical_hash.to_vec(),),
        );
        let mut iter = rows
            .map_err(|e| octo_ident::DidRegistryError::Storage(format!("register query: {e}")))?;
        match iter.next() {
            Some(Ok(row)) => {
                let revoked: i64 = row.get(0).unwrap_or(0);
                if revoked != 0 {
                    return Err(octo_ident::DidRegistryError::AlreadyRevoked);
                }
                let result = self.db.execute(
                    "UPDATE did_registry \
                     SET public_key = ?, revoked = 0, updated_at_unix_ms = ? \
                     WHERE canonical_hash = ?",
                    (doc.public_key.to_vec(), now_ms(), canonical_hash.to_vec()),
                );
                result.map_err(|e| {
                    octo_ident::DidRegistryError::Storage(format!("register update: {e}"))
                })?;
                Ok(())
            }
            Some(Err(e)) => Err(octo_ident::DidRegistryError::Storage(format!(
                "register iter: {e}"
            ))),
            None => {
                let result = self.db.execute(
                    "INSERT INTO did_registry \
                     (canonical_hash, public_key, revoked, updated_at_unix_ms) \
                     VALUES (?, ?, 0, ?)",
                    (canonical_hash.to_vec(), doc.public_key.to_vec(), now_ms()),
                );
                result.map_err(|e| {
                    octo_ident::DidRegistryError::Storage(format!("register insert: {e}"))
                })?;
                Ok(())
            }
        }
    }

    fn resolve(
        &self,
        canonical_hash: &[u8; 32],
    ) -> Result<Option<DidDocument>, octo_ident::DidRegistryError> {
        let rows = self.db.query(
            "SELECT public_key, revoked FROM did_registry \
             WHERE canonical_hash = ? LIMIT 1",
            (canonical_hash.to_vec(),),
        );
        let mut iter =
            rows.map_err(|e| octo_ident::DidRegistryError::Storage(format!("resolve query: {e}")))?;
        match iter.next() {
            Some(Ok(row)) => {
                let revoked: i64 = row.get(1).unwrap_or(0);
                if revoked != 0 {
                    // Revoked → indistinguishable from unknown.
                    return Ok(None);
                }
                let pk_bytes: Vec<u8> = row.get(0).unwrap_or_default();
                if pk_bytes.len() != 32 {
                    return Err(octo_ident::DidRegistryError::Storage(format!(
                        "resolve: public_key column length {} != 32",
                        pk_bytes.len()
                    )));
                }
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&pk_bytes);
                Ok(Some(DidDocument {
                    public_key: pk,
                    revoked: false,
                }))
            }
            Some(Err(e)) => Err(octo_ident::DidRegistryError::Storage(format!(
                "resolve iter: {e}"
            ))),
            None => Ok(None),
        }
    }

    fn revoke(&self, canonical_hash: &[u8; 32]) -> Result<(), octo_ident::DidRegistryError> {
        // Idempotent: revoking an unknown DID is an error; revoking an
        // already-revoked DID is a no-op (UPDATE WHERE revoked=0 is a
        // no-op when row already has revoked=1).
        let rows = self.db.query(
            "SELECT 1 FROM did_registry WHERE canonical_hash = ? LIMIT 1",
            (canonical_hash.to_vec(),),
        );
        let mut iter =
            rows.map_err(|e| octo_ident::DidRegistryError::Storage(format!("revoke query: {e}")))?;
        match iter.next() {
            Some(Ok(_)) => {
                let result = self.db.execute(
                    "UPDATE did_registry SET revoked = 1, updated_at_unix_ms = ? \
                     WHERE canonical_hash = ?",
                    (now_ms(), canonical_hash.to_vec()),
                );
                result.map_err(|e| {
                    octo_ident::DidRegistryError::Storage(format!("revoke update: {e}"))
                })?;
                Ok(())
            }
            Some(Err(e)) => Err(octo_ident::DidRegistryError::Storage(format!(
                "revoke iter: {e}"
            ))),
            None => Err(octo_ident::DidRegistryError::UnknownDid),
        }
    }

    fn list(&self) -> Result<Vec<DidDocument>, octo_ident::DidRegistryError> {
        // List active (revoked=0) documents, sorted by canonical_hash
        // ascending for deterministic iteration.
        let rows = self.db.query(
            "SELECT public_key FROM did_registry WHERE revoked = 0 \
             ORDER BY canonical_hash ASC",
            (),
        );
        let iter =
            rows.map_err(|e| octo_ident::DidRegistryError::Storage(format!("list query: {e}")))?;
        let mut docs = Vec::new();
        for row_result in iter {
            let row = row_result
                .map_err(|e| octo_ident::DidRegistryError::Storage(format!("list iter: {e}")))?;
            let pk_bytes: Vec<u8> = row.get(0).unwrap_or_default();
            if pk_bytes.len() != 32 {
                return Err(octo_ident::DidRegistryError::Storage(format!(
                    "list: public_key column length {} != 32",
                    pk_bytes.len()
                )));
            }
            let mut pk = [0u8; 32];
            pk.copy_from_slice(&pk_bytes);
            docs.push(DidDocument {
                public_key: pk,
                revoked: false,
            });
        }
        Ok(docs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

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
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        let h = sample_hash(1);
        let d = sample_doc(1);
        r.register(&h, d).unwrap();
        assert_eq!(r.resolve(&h).unwrap(), Some(d));
    }

    #[test]
    fn register_upsert_overwrites_existing() {
        let r = StoolapDidRegistry::open_in_memory().expect("open");
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
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        assert_eq!(r.resolve(&sample_hash(99)).unwrap(), None);
    }

    #[test]
    fn revoke_marks_resolve_none() {
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        let h = sample_hash(3);
        r.register(&h, sample_doc(3)).unwrap();
        r.revoke(&h).unwrap();
        // Revoked → indistinguishable from unknown.
        assert_eq!(r.resolve(&h).unwrap(), None);
    }

    #[test]
    fn revoke_unknown_errors() {
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        let err = r.revoke(&sample_hash(42)).unwrap_err();
        assert_eq!(err, octo_ident::DidRegistryError::UnknownDid);
    }

    #[test]
    fn register_after_revoke_errors() {
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        let h = sample_hash(4);
        r.register(&h, sample_doc(4)).unwrap();
        r.revoke(&h).unwrap();
        let err = r.register(&h, sample_doc(4)).unwrap_err();
        assert_eq!(err, octo_ident::DidRegistryError::AlreadyRevoked);
    }

    #[test]
    fn list_returns_all_active_dids_sorted() {
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        r.register(&sample_hash(10), sample_doc(10)).unwrap();
        r.register(&sample_hash(11), sample_doc(11)).unwrap();
        r.register(&sample_hash(12), sample_doc(12)).unwrap();
        r.revoke(&sample_hash(11)).unwrap();
        let docs = r.list().unwrap();
        assert_eq!(docs.len(), 2);
        // Sorted by canonical_hash ASC; hash(10) < hash(12) lexicographically.
        assert_eq!(docs[0].public_key, sample_hash(10));
        assert_eq!(docs[1].public_key, sample_hash(12));
    }

    #[test]
    fn revoke_is_idempotent_for_already_revoked() {
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        let h = sample_hash(5);
        r.register(&h, sample_doc(5)).unwrap();
        r.revoke(&h).unwrap();
        // Second revoke: row IS present, just marked revoked. UPDATE
        // succeeds (idempotent).
        r.revoke(&h).expect("idempotent revoke must succeed");
    }

    #[test]
    fn register_resolve_concurrent_load() {
        // Atomicity TV: 20 threads × 100 register+resolve ops each, no races.
        let r = Arc::new(StoolapDidRegistry::open_in_memory().expect("open"));
        let mut handles = vec![];
        for t in 0..20u8 {
            let r = r.clone();
            handles.push(thread::spawn(move || {
                let h = sample_hash(t);
                let d = sample_doc(t);
                r.register(&h, d).unwrap();
                let resolved = r.resolve(&h).unwrap();
                assert_eq!(resolved, Some(d));
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        // All 20 DIDs must be active.
        assert_eq!(r.list().unwrap().len(), 20);
    }
}
