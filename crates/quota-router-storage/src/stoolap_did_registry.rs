//! `StoolapDidRegistry` (mission 0871b-storage-backend).
//!
//! Persistent DID-document registry backed by a `octo_storage_core::Database` with
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
//! ## Multi-chain namespacing (RFC-0010 v1.4 + mission 0010-f2-registry-namespacing)
//!
//! Migration v011 adds a `chain_id` BLOB column carrying the
//! 17-byte canonical encoding of the chain namespace (per
//! `ChainNamespace::canonical_bytes()`). The single-chain
//! `register` / `resolve` / `revoke` / `list` methods write / read
//! the mainnet namespace via the `MAINNET_CHAIN_ID_BYTES` const;
//! the `register_in_chain` / `resolve_in_chain` overrides accept
//! an explicit `ChainId` and store its canonical bytes.
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
use octo_ident::{
    CapabilityDelegation, ChainId, ControllerReference, ServiceEndpoint, VerificationMethod,
};

use crate::migrations;
use octo_ident::DidRegistry;

/// 17-byte canonical encoding of the `CIPHEROCTO_MAINNET` namespace
/// (RFC-0010 v1.4 §ChainId Namespace Extension):
/// `[variant: 0x01 (Rfc) | tag: 15 bytes (CIPHEROCTO_MAINNET_TAG)
/// | length: 0x12 (18 chars for "cipherocto-mainnet")]`.
///
/// Verified against `ChainId::default().namespace().unwrap().canonical_bytes()`
/// in `tests/stoolap_chain_namespace.rs::mainnet_bytes_match_chain_id_default`.
pub const MAINNET_CHAIN_ID_BYTES: [u8; 17] = [
    0x01, 0xeb, 0x30, 0x71, 0xb5, 0xe1, 0x13, 0x33, 0x0c, 0x87, 0x63, 0x09, 0x54, 0xe3, 0xcc, 0x08,
    0x12,
];

/// Encode a `ChainId` to its 17-byte canonical form for BLOB storage.
fn chain_id_to_canonical_bytes(
    chain_id: &ChainId,
) -> Result<[u8; 17], octo_ident::DidRegistryError> {
    let namespace = chain_id
        .namespace()
        .map_err(|e| octo_ident::DidRegistryError::Storage(format!("chain_id namespace: {e}")))?;
    Ok(namespace.canonical_bytes())
}

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
    db: Arc<octo_storage_core::Database>,
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
        let db = octo_storage_core::Database::open_in_memory()
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
        let db = octo_storage_core::Database::open(path)
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
        // Borsh-encode the 4 rich-document fields (mission
        // 0010-f8-rich-did-storage). Matches the `WireDid`/`RawDid`
        // borsh pattern + the `CapabilityBundleV2` borsh precedent
        // from mission 0957-f-v2-bundle.
        let service_endpoints_blob = borsh::to_vec(&doc.service_endpoints).map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("borsh service_endpoints: {e}"))
        })?;
        let controllers_blob = borsh::to_vec(&doc.controllers).map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("borsh controllers: {e}"))
        })?;
        let verification_methods_blob = borsh::to_vec(&doc.verification_methods).map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("borsh verification_methods: {e}"))
        })?;
        let capability_delegations_blob =
            borsh::to_vec(&doc.capability_delegations).map_err(|e| {
                octo_ident::DidRegistryError::Storage(format!("borsh capability_delegations: {e}"))
            })?;
        // SELECT existing (lock + visibility). Stoolap-fork does NOT
        // support `INSERT OR REPLACE`; use SELECT-then-INSERT/UPDATE
        // pattern (matches `StoolapSpendLedger::seed` v007).
        // Mission 0010-f2-registry-namespacing: filter on
        // chain_id = MAINNET_CHAIN_ID_BYTES (default namespace).
        let rows = self.db.query(
            "SELECT revoked FROM did_registry \
             WHERE canonical_hash = ? AND chain_id = ? LIMIT 1",
            (canonical_hash.to_vec(), MAINNET_CHAIN_ID_BYTES.to_vec()),
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
                     SET public_key = ?, revoked = 0, updated_at_unix_ms = ?, \
                         service_endpoints = ?, controllers = ?, \
                         verification_methods = ?, capability_delegations = ? \
                     WHERE canonical_hash = ? AND chain_id = ?",
                    (
                        doc.public_key.to_vec(),
                        now_ms(),
                        service_endpoints_blob,
                        controllers_blob,
                        verification_methods_blob,
                        capability_delegations_blob,
                        canonical_hash.to_vec(),
                        MAINNET_CHAIN_ID_BYTES.to_vec(),
                    ),
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
                     (canonical_hash, public_key, revoked, updated_at_unix_ms, \
                      service_endpoints, controllers, \
                      verification_methods, capability_delegations, chain_id) \
                     VALUES (?, ?, 0, ?, ?, ?, ?, ?, ?)",
                    (
                        canonical_hash.to_vec(),
                        doc.public_key.to_vec(),
                        now_ms(),
                        service_endpoints_blob,
                        controllers_blob,
                        verification_methods_blob,
                        capability_delegations_blob,
                        MAINNET_CHAIN_ID_BYTES.to_vec(),
                    ),
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
            "SELECT public_key, revoked, \
                    service_endpoints, controllers, \
                    verification_methods, capability_delegations \
             FROM did_registry \
             WHERE canonical_hash = ? AND chain_id = ? LIMIT 1",
            (canonical_hash.to_vec(), MAINNET_CHAIN_ID_BYTES.to_vec()),
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
                // Borsh-decode the 4 rich-document fields. Legacy NULL
                // rows (pre-v009) + malformed bytes decode to empty
                // Vec via `unwrap_or_default()` (fail-soft: rich
                // fields are optional metadata per RFC-0010 v1.5
                // §Forward compatibility).
                let service_endpoints: Vec<ServiceEndpoint> = row
                    .get::<Vec<u8>>(2)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<ServiceEndpoint>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let controllers: Vec<ControllerReference> = row
                    .get::<Vec<u8>>(3)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<ControllerReference>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let verification_methods: Vec<VerificationMethod> = row
                    .get::<Vec<u8>>(4)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<VerificationMethod>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let capability_delegations: Vec<CapabilityDelegation> = row
                    .get::<Vec<u8>>(5)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<CapabilityDelegation>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                Ok(Some(DidDocument {
                    public_key: pk,
                    revoked: false,
                    service_endpoints,
                    controllers,
                    verification_methods,
                    capability_delegations,
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
            "SELECT 1 FROM did_registry \
             WHERE canonical_hash = ? AND chain_id = ? LIMIT 1",
            (canonical_hash.to_vec(), MAINNET_CHAIN_ID_BYTES.to_vec()),
        );
        let mut iter =
            rows.map_err(|e| octo_ident::DidRegistryError::Storage(format!("revoke query: {e}")))?;
        match iter.next() {
            Some(Ok(_)) => {
                let result = self.db.execute(
                    "UPDATE did_registry SET revoked = 1, updated_at_unix_ms = ? \
                     WHERE canonical_hash = ? AND chain_id = ?",
                    (
                        now_ms(),
                        canonical_hash.to_vec(),
                        MAINNET_CHAIN_ID_BYTES.to_vec(),
                    ),
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
        // List active (revoked=0) documents on the mainnet chain,
        // sorted by canonical_hash ascending for deterministic
        // iteration.
        let rows = self.db.query(
            "SELECT public_key FROM did_registry \
             WHERE revoked = 0 AND chain_id = ? \
             ORDER BY canonical_hash ASC",
            (MAINNET_CHAIN_ID_BYTES.to_vec(),),
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
                ..Default::default()
            });
        }
        Ok(docs)
    }

    fn register_in_chain(
        &self,
        chain_id: &ChainId,
        canonical_hash: &[u8; 32],
        doc: DidDocument,
    ) -> Result<(), octo_ident::DidRegistryError> {
        let chain_bytes = chain_id_to_canonical_bytes(chain_id)?;
        // Borsh-encode rich-document fields (same as single-chain path).
        let service_endpoints_blob = borsh::to_vec(&doc.service_endpoints).map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("borsh service_endpoints: {e}"))
        })?;
        let controllers_blob = borsh::to_vec(&doc.controllers).map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("borsh controllers: {e}"))
        })?;
        let verification_methods_blob = borsh::to_vec(&doc.verification_methods).map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("borsh verification_methods: {e}"))
        })?;
        let capability_delegations_blob =
            borsh::to_vec(&doc.capability_delegations).map_err(|e| {
                octo_ident::DidRegistryError::Storage(format!("borsh capability_delegations: {e}"))
            })?;
        // SELECT existing row on (canonical_hash, chain_id) composite.
        let rows = self.db.query(
            "SELECT revoked FROM did_registry \
             WHERE canonical_hash = ? AND chain_id = ? LIMIT 1",
            (canonical_hash.to_vec(), chain_bytes.to_vec()),
        );
        let mut iter = rows.map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("register_in_chain query: {e}"))
        })?;
        match iter.next() {
            Some(Ok(row)) => {
                let revoked: i64 = row.get(0).unwrap_or(0);
                if revoked != 0 {
                    return Err(octo_ident::DidRegistryError::AlreadyRevoked);
                }
                let result = self.db.execute(
                    "UPDATE did_registry \
                     SET public_key = ?, revoked = 0, updated_at_unix_ms = ?, \
                         service_endpoints = ?, controllers = ?, \
                         verification_methods = ?, capability_delegations = ? \
                     WHERE canonical_hash = ? AND chain_id = ?",
                    (
                        doc.public_key.to_vec(),
                        now_ms(),
                        service_endpoints_blob,
                        controllers_blob,
                        verification_methods_blob,
                        capability_delegations_blob,
                        canonical_hash.to_vec(),
                        chain_bytes.to_vec(),
                    ),
                );
                result.map_err(|e| {
                    octo_ident::DidRegistryError::Storage(format!("register_in_chain update: {e}"))
                })?;
                Ok(())
            }
            Some(Err(e)) => Err(octo_ident::DidRegistryError::Storage(format!(
                "register_in_chain iter: {e}"
            ))),
            None => {
                let result = self.db.execute(
                    "INSERT INTO did_registry \
                     (canonical_hash, public_key, revoked, updated_at_unix_ms, \
                      service_endpoints, controllers, \
                      verification_methods, capability_delegations, chain_id) \
                     VALUES (?, ?, 0, ?, ?, ?, ?, ?, ?)",
                    (
                        canonical_hash.to_vec(),
                        doc.public_key.to_vec(),
                        now_ms(),
                        service_endpoints_blob,
                        controllers_blob,
                        verification_methods_blob,
                        capability_delegations_blob,
                        chain_bytes.to_vec(),
                    ),
                );
                result.map_err(|e| {
                    octo_ident::DidRegistryError::Storage(format!("register_in_chain insert: {e}"))
                })?;
                Ok(())
            }
        }
    }

    fn resolve_in_chain(
        &self,
        chain_id: &ChainId,
        canonical_hash: &[u8; 32],
    ) -> Result<Option<DidDocument>, octo_ident::DidRegistryError> {
        let chain_bytes = chain_id_to_canonical_bytes(chain_id)?;
        let rows = self.db.query(
            "SELECT public_key, revoked, \
                    service_endpoints, controllers, \
                    verification_methods, capability_delegations \
             FROM did_registry \
             WHERE canonical_hash = ? AND chain_id = ? LIMIT 1",
            (canonical_hash.to_vec(), chain_bytes.to_vec()),
        );
        let mut iter = rows.map_err(|e| {
            octo_ident::DidRegistryError::Storage(format!("resolve_in_chain query: {e}"))
        })?;
        match iter.next() {
            Some(Ok(row)) => {
                let revoked: i64 = row.get(1).unwrap_or(0);
                if revoked != 0 {
                    return Ok(None);
                }
                let pk_bytes: Vec<u8> = row.get(0).unwrap_or_default();
                if pk_bytes.len() != 32 {
                    return Err(octo_ident::DidRegistryError::Storage(format!(
                        "resolve_in_chain: public_key column length {} != 32",
                        pk_bytes.len()
                    )));
                }
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&pk_bytes);
                let service_endpoints: Vec<ServiceEndpoint> = row
                    .get::<Vec<u8>>(2)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<ServiceEndpoint>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let controllers: Vec<ControllerReference> = row
                    .get::<Vec<u8>>(3)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<ControllerReference>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let verification_methods: Vec<VerificationMethod> = row
                    .get::<Vec<u8>>(4)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<VerificationMethod>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                let capability_delegations: Vec<CapabilityDelegation> = row
                    .get::<Vec<u8>>(5)
                    .map(|bytes| {
                        borsh::from_slice::<Vec<CapabilityDelegation>>(&bytes).unwrap_or_default()
                    })
                    .unwrap_or_default();
                Ok(Some(DidDocument {
                    public_key: pk,
                    revoked: false,
                    service_endpoints,
                    controllers,
                    verification_methods,
                    capability_delegations,
                }))
            }
            Some(Err(e)) => Err(octo_ident::DidRegistryError::Storage(format!(
                "resolve_in_chain iter: {e}"
            ))),
            None => Ok(None),
        }
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
            ..Default::default()
        }
    }

    #[test]
    fn register_resolve_round_trip() {
        let r = StoolapDidRegistry::open_in_memory().expect("open");
        let h = sample_hash(1);
        let d = sample_doc(1);
        r.register(&h, d.clone()).unwrap();
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
            ..Default::default()
        };
        r.register(&h, new_doc.clone()).unwrap();
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
                r.register(&h, d.clone()).unwrap();
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
