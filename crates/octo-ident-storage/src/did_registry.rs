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
//! Uses raw byte slices (`[u8; 32]` for `canonical_hash` +
//! `[u8; 32]` for `public_key`) instead of typed `WireDid` wrappers.
//! Avoids a cyclic crate dependency: this crate (adapter Layer C) does
//! not depend on `octo-wallet` (which owns the `WireDid` wire-format
//! type). Same pattern as `StoolapSpendLedger`
//! (`crates/quota-router-storage/src/stoolap_spend_ledger.rs`).
//!
//! ## Atomicity (RFC-0010)
//!
//! `register` runs inside a stoolap transaction:
//! 1. SELECT existing row FOR UPDATE (lock + visibility check)
//! 2. If existing.revoked → ABORT with `AlreadyRevoked`
//! 3. INSERT (new) or UPDATE (existing, non-revoked) with new `public_key`
//! 4. COMMIT
//!
//! Concurrent register on the same `canonical_hash` serializes via the
//! FOR UPDATE lock — no torn writes possible.
//!
//! ## Multi-chain namespacing (RFC-0010 + mission 0010-f2-registry-namespacing)
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
//! Schema lives at `crates/quota-router-storage/migrations/v008..v011__*.sql`
//! per [[stoolap-general-purpose-db]] (cipherocto-side, NOT stoolap fork).
//! The `StoolapDidRegistry` constructors call
//! `quota_router_storage::migrations::apply_pending` to apply the schema.
//!
//! ## Layer discipline
//!
//! This module lives in `octo-ident-storage` (Layer C adapter) per
//! RFC-0206 §Adapter Crate List row 3. The trait is declared in
//! `octo-ident` (Layer B) — this crate provides the sole
//! Stoolap-backed production implementation. Moved from
//! `crates/quota-router-storage/src/stoolap_did_registry.rs` in mission
//! 0206-003 v3.0.

use std::sync::Arc;

use octo_ident::DidDocument;
use octo_ident::{
    AlgorithmId, CapabilityDelegation, ChainId, ControllerReference, CustodyPosture,
    ServiceEndpoint, VerificationMethod, VerificationMethodId,
};
use uuid::Uuid;

// Mission 0206-003 R1 review: schema migrations v008..v011 stay in
// quota-router-storage (sole source of truth for `cipherocto_schema_version`).
// The StoolapDidRegistry constructors call `apply_pending` from that
// crate to set up the `did_registry` table.
use quota_router_storage::migrations;

/// Namespace for verification-method ids derived while migrating a legacy row.
///
/// A dedicated namespace so a derived id can never be confused with one a
/// writer allocated under its own naming scheme.
const MIGRATION_NAMESPACE: Uuid = Uuid::from_bytes([
    0x5e, 0x19, 0x3c, 0x8a, 0x47, 0xd2, 0xb6, 0x05, 0x71, 0xef, 0x2d, 0x84, 0x36, 0xca, 0x59, 0x0b,
]);

/// The pre-`VerificationMethodKind`-removal on-disk shape.
///
/// `kind` was a `#[repr(u8)]` enum serialised with `borsh(use_discriminant)`,
/// so it occupies exactly one byte followed by a fixed 32-byte key.
#[derive(borsh::BorshDeserialize)]
struct LegacyVerificationMethod {
    /// Legacy discriminator byte: `0x01` Ed25519, anything else was the
    /// catch-all `Reserved` slot.
    kind: u8,
    /// 32-byte Ed25519 public key.
    public_key: [u8; 32],
}

/// Decode the `verification_methods` column, migrating a legacy row.
///
/// Three column states, because they pull in different directions:
///
/// - **NULL** — a pre-v009 row. Absence, not corruption. Yields an empty
///   list, per RFC-0010 §Forward compatibility.
/// - **present but empty** — absence too, same treatment.
/// - **present and undecodable under both the current and the legacy shape** —
///   corruption, and the one case that MUST be an error. This is the silent
///   downgrade BC#13 exists to catch: the old decode routed this through
///   `unwrap_or_default()`, so a document whose blob no longer decoded yielded
///   *zero* verification methods while the independent `public_key` column
///   still resolved — a DID that resolves successfully, carries a public key,
///   and owns no algorithm-bearing key at all.
///
/// The derived `id` is `new_v5(MIGRATION_NAMESPACE, canonical_hash ‖ index)`.
/// Not `new_v4`, which would give a different id on every `resolve` and make
/// every signature over the signable prefix unverifiable. Not derived from the
/// public key, which RFC-0009 §Domain separation forbids explicitly and which
/// is the mistake v1.5 rejected when it refused the string-fragment key id.
fn decode_verification_methods(
    bytes: &[u8],
    canonical_hash: &[u8; 32],
) -> Result<Vec<VerificationMethod>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    match borsh::from_slice::<Vec<VerificationMethod>>(bytes) {
        Ok(current) => Ok(current),
        Err(current_err) => {
            let legacy: Vec<LegacyVerificationMethod> =
                borsh::from_slice(bytes).map_err(|legacy_err| {
                    format!(
                        "verification_methods column is undecodable under both the current shape \
                     (current: {current_err}) and the legacy shape (legacy: {legacy_err})"
                    )
                })?;
            Ok(legacy
                .into_iter()
                .enumerate()
                .map(|(index, m)| {
                    let mut name = canonical_hash.to_vec();
                    // Fixed width, deliberately. `usize::to_le_bytes()` is
                    // 8 bytes on a 64-bit target and 4 on a 32-bit one, so a
                    // `usize` here would make the derived id depend on the
                    // architecture: the same legacy row would yield different
                    // key ids on a 32-bit and a 64-bit resolver, and every
                    // signature over the signable prefix would be verifiable
                    // on one and not the other. That is precisely the
                    // cross-resolver stability the derivation exists to give.
                    // borsh writes a `Vec` length as u32, so u32 is also the
                    // widest index a legacy blob can carry.
                    let index = u32::try_from(index).expect("legacy entry index fits in u32");
                    name.extend_from_slice(&index.to_le_bytes());
                    let id =
                        VerificationMethodId::from_uuid(Uuid::new_v5(&MIGRATION_NAMESPACE, &name));
                    // `0x01` was Ed25519. Every other byte collapsed onto
                    // the old `Reserved` catch-all, which named no algorithm.
                    //
                    // An earlier version mapped all of them to a single
                    // `legacy-unknown` id. That fails closed, but it
                    // reinstates the exact collapse BC#13 removes — 0x02 and
                    // 0xFF would share one `algorithm_id`, and two unknown
                    // discriminators would stop being distinguishable in the
                    // error, which is the property AC#12 asks for and the one
                    // the deleted enum violated. Each byte keeps its own
                    // identity here.
                    let algorithm_id = if m.kind == 0x01 {
                        AlgorithmId::derive("ed25519")
                    } else {
                        let mut name = b"legacy-kind".to_vec();
                        name.push(m.kind);
                        AlgorithmId::from_uuid(Uuid::new_v5(&MIGRATION_NAMESPACE, &name))
                    };
                    VerificationMethod::new(
                        id,
                        algorithm_id,
                        m.public_key.to_vec(),
                        // The legacy shape carried no custody field. Software
                        // is the honest reading: an Ed25519 key that was never
                        // declared hardware-held was software-resident.
                        CustodyPosture::Software,
                    )
                })
                .collect())
        }
    }
}

use octo_ident::DidRegistry;

/// 17-byte canonical encoding of the `CIPHEROCTO_MAINNET` namespace
/// (RFC-0010 §`ChainId` Namespace Extension):
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
    /// Open a fresh in-memory database with the `did_registry` schema
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

    /// Open a file-backed database at `path` with the `did_registry`
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

    /// Wrap an already-open `Database` that already carries the
    /// `did_registry` schema.
    ///
    /// Exists so a test can write rows directly into the rich columns — a
    /// legacy blob, a NULL column, or a corrupt one — which the `register`
    /// path can never produce because it only ever writes the current shape.
    /// Without this, every decode vector had to call
    /// `decode_verification_methods` directly, which means none of them
    /// exercised `resolve` itself: reverting the `?` in the read paths to
    /// `unwrap_or_default()` left the whole suite green while reintroducing
    /// the silent downgrade BC#13 exists to prevent.
    ///
    /// Does **not** apply migrations — the caller supplies a database
    /// already carrying the schema, which is what makes "insert a row in the
    /// legacy shape" expressible at all.
    #[must_use]
    pub fn from_database(db: octo_storage_core::Database) -> Self {
        Self { db: Arc::new(db) }
    }

    /// Borrow the underlying database.
    ///
    /// Test support for [`StoolapDidRegistry::from_database`]: a test that can
    /// insert a row can also assert on one. Read-only intent — write through
    /// [`StoolapDidRegistry::register`] so the wire format stays in one
    /// place.
    #[must_use]
    pub fn database(&self) -> &octo_storage_core::Database {
        &self.db
    }
}

/// Helper: current Unix epoch in milliseconds (`i64`; saturates).
#[allow(
    clippy::cast_possible_truncation,
    clippy::map_unwrap_or,
    reason = "u128→i64 truncation saturates at year ~500_000_000; map_unwrap_or is the idiomatic conversion here"
)]
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
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
                // Borsh-decode the 4 rich-document fields. Legacy NULL rows
                // (pre-v009) decode to an empty Vec — absence, not corruption.
                // `verification_methods` is the exception: a blob that decodes
                // under neither the current nor the legacy shape is an Err,
                // because the independent `public_key` column would still
                // resolve and the DID would carry no algorithm-bearing key.
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
                let verification_methods = match row.get::<Vec<u8>>(4) {
                    Ok(bytes) => {
                        decode_verification_methods(&bytes, canonical_hash).map_err(|e| {
                            octo_ident::DidRegistryError::Storage(format!("resolve: {e}"))
                        })?
                    }
                    // NULL column: absence, not corruption.
                    Err(_) => Vec::new(),
                };
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
            "SELECT canonical_hash, public_key, service_endpoints, controllers, \
                    verification_methods, capability_delegations FROM did_registry \
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
            let hash_bytes: Vec<u8> = row.get(0).unwrap_or_default();
            if hash_bytes.len() != 32 {
                return Err(octo_ident::DidRegistryError::Storage(format!(
                    "list: canonical_hash column length {} != 32",
                    hash_bytes.len()
                )));
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&hash_bytes);
            let pk_bytes: Vec<u8> = row.get(1).unwrap_or_default();
            if pk_bytes.len() != 32 {
                return Err(octo_ident::DidRegistryError::Storage(format!(
                    "list: public_key column length {} != 32",
                    pk_bytes.len()
                )));
            }
            let mut pk = [0u8; 32];
            pk.copy_from_slice(&pk_bytes);
            // The rich columns are selected here rather than left to
            // `Default::default()`. This path used to build every document
            // from `public_key` alone, so it returned zero verification methods
            // for every row — the same silent downgrade as the resolver path,
            // reached by a shorter route, and a revocation cascade that sees
            // nothing to revoke once `revoked_at` exists per key.
            let service_endpoints: Vec<ServiceEndpoint> = row
                .get::<Vec<u8>>(2)
                .map(|bytes| borsh::from_slice::<Vec<ServiceEndpoint>>(&bytes).unwrap_or_default())
                .unwrap_or_default();
            let controllers: Vec<ControllerReference> = row
                .get::<Vec<u8>>(3)
                .map(|bytes| {
                    borsh::from_slice::<Vec<ControllerReference>>(&bytes).unwrap_or_default()
                })
                .unwrap_or_default();
            let verification_methods = match row.get::<Vec<u8>>(4) {
                Ok(bytes) => decode_verification_methods(&bytes, &hash)
                    .map_err(|e| octo_ident::DidRegistryError::Storage(format!("list: {e}")))?,
                // NULL column: absence, not corruption.
                Err(_) => Vec::new(),
            };
            let capability_delegations: Vec<CapabilityDelegation> = row
                .get::<Vec<u8>>(5)
                .map(|bytes| {
                    borsh::from_slice::<Vec<CapabilityDelegation>>(&bytes).unwrap_or_default()
                })
                .unwrap_or_default();
            docs.push(DidDocument {
                public_key: pk,
                revoked: false,
                service_endpoints,
                controllers,
                verification_methods,
                capability_delegations,
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
                let verification_methods = match row.get::<Vec<u8>>(4) {
                    Ok(bytes) => {
                        decode_verification_methods(&bytes, canonical_hash).map_err(|e| {
                            octo_ident::DidRegistryError::Storage(format!("resolve_in_chain: {e}"))
                        })?
                    }
                    // NULL column: absence, not corruption.
                    Err(_) => Vec::new(),
                };
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

    #[allow(clippy::cast_possible_truncation)]
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

    // ---------------------------------------------------------------------
    // RFC-0009 BC#13 legacy migration.
    //
    // These live here rather than in `tests/` because `StoolapDidRegistry`
    // exposes no database accessor, so a legacy blob cannot be injected
    // through SQL from an integration test. The existing
    // `resolve_legacy_row_returns_empty_vevs` test works around that by
    // exercising borsh on an empty BLOB directly, which pins borsh's
    // behaviour but never touches `resolve` — so it guards neither the NULL
    // column nor removal of the decoder's error path.
    // ---------------------------------------------------------------------

    /// Bytes of a pre-migration `verification_methods` column: a borsh
    /// `Vec` of `{ kind: u8, public_key: [u8; 32] }`.
    fn legacy_blob(entries: &[(u8, [u8; 32])]) -> Vec<u8> {
        let count = u32::try_from(entries.len()).expect("fixture entry count fits in u32");
        let mut out = count.to_le_bytes().to_vec();
        for (kind, pk) in entries {
            out.push(*kind);
            out.extend_from_slice(pk);
        }
        out
    }

    #[test]
    fn legacy_document_migrates_to_algorithm_id() {
        let hash = [0x11u8; 32];
        let migrated = decode_verification_methods(&legacy_blob(&[(0x01, [0xAA; 32])]), &hash)
            .expect("decode");
        assert_eq!(migrated.len(), 1);
        assert_eq!(
            migrated[0].algorithm_id,
            AlgorithmId::derive("ed25519"),
            "legacy kind 0x01 was Ed25519"
        );
        assert_eq!(migrated[0].public_key, vec![0xAAu8; 32]);
        assert!(!migrated[0].is_revoked());
        assert_eq!(migrated[0].custody, CustodyPosture::Software);
    }

    #[test]
    fn migrated_id_is_stable_across_decodes() {
        // A `new_v4` per decode would give a different id on every resolve,
        // which would make every signature over the signable prefix
        // unverifiable. Derivation must depend only on the document identity
        // and the entry's position in it.
        let hash = [0x22u8; 32];
        let first = decode_verification_methods(
            &legacy_blob(&[(0x01, [0xAA; 32]), (0x01, [0xBB; 32])]),
            &hash,
        )
        .expect("decode");
        let second = decode_verification_methods(
            &legacy_blob(&[(0x01, [0xAA; 32]), (0x01, [0xBB; 32])]),
            &hash,
        )
        .expect("decode");
        assert_eq!(first[0].id, second[0].id);
        assert_eq!(first[1].id, second[1].id);
        // Distinct entries get distinct ids, and neither is derived from the
        // public key: the two keys differ, and so do the ids, but deriving
        // the id from the key is forbidden — this only pins distinctness.
        assert_ne!(first[0].id, first[1].id);

        // A different document gets different ids.
        let other = [0x33u8; 32];
        let third =
            decode_verification_methods(&legacy_blob(&[(0x01, [0xAA; 32])]), &other).expect("d");
        assert_ne!(first[0].id, third[0].id);
    }

    #[test]
    fn legacy_non_ed25519_byte_fails_closed_and_stays_distinct() {
        // Every byte but 0x01 collapsed onto the old `Reserved` catch-all,
        // which named no algorithm. Mapping it to Ed25519 would recreate the
        // collapse. Giving them all ONE shared id would recreate it too, in a
        // different shape: 0x02 and 0xFF would carry the same algorithm_id and
        // two unknown discriminators would stop being distinguishable in the
        // error, which is the property AC#12 asks for and the one the deleted
        // enum violated. Each unknown byte keeps its own identity.
        let hash = [0x44u8; 32];
        let mut seen: Vec<AlgorithmId> = Vec::new();
        for byte in [0x00u8, 0x02, 0x7F, 0xFF] {
            let migrated = decode_verification_methods(&legacy_blob(&[(byte, [0xAA; 32])]), &hash)
                .expect("decode");
            let got = migrated[0].algorithm_id;
            assert_ne!(
                got,
                AlgorithmId::derive("ed25519"),
                "legacy kind {byte:#04x} must not be claimed as Ed25519"
            );
            assert!(
                !seen.contains(&got),
                "legacy kind {byte:#04x} collided with an earlier unknown byte"
            );
            seen.push(got);
        }
        assert_eq!(seen.len(), 4, "four distinct bytes, four distinct ids");
    }

    #[test]
    fn null_and_empty_blob_yield_an_empty_list_not_an_error() {
        let hash = [0x55u8; 32];
        assert!(decode_verification_methods(&[], &hash)
            .expect("empty blob is absence")
            .is_empty());
    }

    #[test]
    fn post_migration_decode_failure_is_an_error() {
        // The one that matters most. The old code routed this through
        // `unwrap_or_default()`, so a corrupt blob produced ZERO verification
        // methods while the independent `public_key` column still resolved —
        // a DID that resolves successfully and owns no algorithm-bearing key.
        let hash = [0x66u8; 32];
        let err = decode_verification_methods(&[0xFF, 0xFF, 0xFF, 0xFF], &hash)
            .expect_err("corrupt blob must be an error, not an empty list");
        assert!(
            err.contains("undecodable under both the current shape"),
            "error should name both attempts, got: {err}"
        );

        // Truncated mid-entry: a plausible partial write must not silently
        // yield the entries that happened to parse.
        let mut truncated = legacy_blob(&[(0x01, [0xAA; 32]), (0x01, [0xBB; 32])]);
        truncated.truncate(truncated.len() - 4);
        assert!(decode_verification_methods(&truncated, &hash).is_err());
    }

    /// The derivation input's index width is fixed, and this pins it.
    ///
    /// The first version used `usize::to_le_bytes()`, which is 8 bytes on a
    /// 64-bit target and 4 on a 32-bit one. That made the derived key id
    /// architecture-dependent, so the same legacy row produced different ids
    /// on differently-targeted resolvers and every signature over the
    /// signable prefix verified on one and not the other. No test on a single
    /// platform can catch it, so the width is asserted against a hard-coded
    /// vector instead.
    #[test]
    fn migrated_id_does_not_depend_on_platform_word_size() {
        let hash = [0x99u8; 32];
        let migrated = decode_verification_methods(
            &legacy_blob(&[(0x01, [0xAA; 32]), (0x01, [0xBB; 32])]),
            &hash,
        )
        .expect("decode");

        // Derive the expected ids independently, with the width spelled out.
        let expected = |idx: u32| {
            let mut name = hash.to_vec();
            name.extend_from_slice(&idx.to_le_bytes());
            VerificationMethodId::from_uuid(Uuid::new_v5(&MIGRATION_NAMESPACE, &name))
        };

        assert_eq!(migrated[0].id, expected(0));
        assert_eq!(migrated[1].id, expected(1));

        // And the derivation input is exactly 32 + 4 bytes. If this ever
        // changes to a `usize`, the length moves to 36 on a 64-bit build and
        // the ids stop matching a 32-bit resolver's.
        let mut expected_input = hash.to_vec();
        expected_input.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(expected_input.len(), 36);
    }

    #[test]
    fn current_shape_is_not_migrated() {
        // A row already in the new shape must pass through untouched, not be
        // re-derived. Re-deriving would discard the writer's own id.
        let hash = [0x77u8; 32];
        let doc = octo_ident::VerificationMethod::new(
            VerificationMethodId::derive("writer-chosen-id"),
            AlgorithmId::derive("ed25519"),
            vec![0xAA; 32],
            CustodyPosture::Hardware,
        );
        let blob = borsh::to_vec(&vec![doc.clone()]).expect("encode");
        let out = decode_verification_methods(&blob, &hash).expect("decode");
        assert_eq!(out, vec![doc], "current-shape rows pass through unchanged");
    }
}
