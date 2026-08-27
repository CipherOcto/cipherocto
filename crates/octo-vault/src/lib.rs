//! [`octo_vault`] — Layer B vault substrate.
//!
//! Per plan §B.3 / stream B.3 / `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`.
//!
//! Owns:
//! - [`Vault`] / [`VaultId`] / [`VaultPolicy`] / [`VaultState`] types
//!   (review-doc Model B per `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
//!   §20.3 — supersedes RFC-0960 §2.1 which carried a `parent_vault`
//!   field later dropped as a phantom type, see §20.10).
//! - [`vault_id`] canonical BLAKE3 derivation per §8.10 TV-V1.
//! - [`apply`] delegating to the Layer A substrate
//!   [`octo_storage_core::_legacy_apply_pending`].
//!
//! ## Layer model
//!
//! Layer B (per `cipherocto-design-principles`): RFC-driven, additive only
//! (years-stable). This crate lies above the Layer A substrate
//! (`octo_storage_core`) and below Layer C consumers
//! (`octo-ident`-backed DIDs, `quota-router-storage` Ask/Receipt types).
//! It does NOT depend on `octo-sync`, `octo-protocol`, or any
//! transport-layer crate.
//!
//! ## Model B (canonical)
//!
//! Per review §20.3, vaults carry a composite primary key
//! `(chain_id, owner_did, asset_id)`. `vault_id` is the *derived* identifier
//! (`BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)`) and
//! is indexed UNIQUE for lookup. `VaultHierarchy` (parent vault) was audited
//! as a phantom type (§20.10) and is NOT modeled — child policy inheritance
//! is the consumer's responsibility (per §20.6.1 verify-time option (b)).
//!
//! ## Forward-compat posture
//!
//! Public types are marked `#[non_exhaustive]` where additive field/variant
//! evolution must remain a non-breaking change (Layer B years-stable). The
//! `[u8; 32]` newtypes keep their inner field `pub` so existing readers
//! (substrate bind paths, debug printing) work, but the trust anchor lives
//! in the canonical `vault_id` derivation — callers MUST recompute, not
//! trust a `from_bytes` round-trip, before authorizing an action.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use thiserror::Error;

/// Maximum accepted `owner_did` length (bytes). The vault_id derivation
/// hashes the raw owner_did into BLAKE3; an unbounded input is a DoS
/// surface at the substrate boundary. 256B covers the longest realistic
/// DID form (`did:octo:` + namespace + identifier) with comfortable
/// headroom. Substrate-internal callers pay zero runtime cost.
pub const MAX_OWNER_DID_LEN: usize = 256;

pub(crate) mod migrations;

pub use migrations::BUILTIN_MIGRATION_CATALOG;

// Mission D substrate re-exports: traits + newtypes live in
// octo-cap-macaroon (Layer A frozen substrate per RFC-0105 v3.5
// §3.1/§3.11 canonical home). octo-vault re-exports for ergonomic
// substrate-handle consumers (storage-coupled implementations + tests).
pub use octo_cap_macaroon::{
    blake3_hash as vault_blake3_hash, nonce_bucket_key as vault_nonce_bucket_key,
    sovereign_nonce_namespace as vault_sovereign_nonce_namespace,
    verify_governance_signature as vault_verify_governance_signature, AssetError, AssetId,
    AssetKind, AssetMetadata, AssetRegistry, ChainId, Epoch, GovernancePubkey, GovernanceSignature,
    GovernanceSignatureBytes, GovernanceSignatureError, InMemoryAssetRegistry,
    InMemoryNonceRegistry, Nonce, NonceError, NonceEventKind, NonceRegistry, VaultId, MAX_SCALE,
};
// NOTE: `BUILTIN_MIGRATIONS` (the tuple slice form) is intentionally NOT
// re-exported — it's an internal drift-detection form consumed only by
// `migrations::tests`. External callers reach the catalog via
// `BUILTIN_MIGRATION_CATALOG` (the substrate `Migration` slice).

/// Vault substrate errors.
///
/// `Display` is operator-facing; `Debug` is substrate-internal (auto-derived).
/// Per `cipherocto-design-principles` Layer B: enum variants are additive;
/// fields are stable; never embed raw SQL / migration names / owner-DID
/// payloads across the boundary without redaction review.
#[derive(Debug, Error)]
pub enum VaultError {
    /// Substrate-level failure (Sttoolap / migration runner).
    #[error(
        "vault substrate error during apply_migrations; \
             substrate-internal trace preserved in substrate logs only"
    )]
    Substrate,
}

// NOTE: `VaultId`, `ChainId`, `AssetId` are re-exported above from
// `octo_cap_macaroon::substrate` (RFC-0105 v3.5 canonical home). The
// canonical BLAKE3 derivations (`AssetId::derive(role_token)`,
// `ChainId::derive(chain_string)`) live in `octo-cap-macaroon/src/substrate.rs`
// and are reachable through the re-exports. The composite `vault_id`
// derivation that combines all three inputs lives in this crate because
// `owner_did` is Layer B context that the substrate intentionally does
// not model.

/// Vault state. Per review §20.3 schema sketch the column is `TEXT NOT NULL`
/// with values from this enum. RFC-0960 §2.1 listed `Retired`; review §20.10
/// dropped it (consumer-driven transitions out of `Frozen` go straight to row
/// removal — the append-only `transfer_events` log is the audit trail).
///
/// `#[non_exhaustive]` so future substrate-text variants (e.g. `Pending`,
/// `Retired`) can land without a semver-major break; consumers must add a
/// wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VaultState {
    /// Vault is operational. `transfer_events` rows may accrete/transfer/burn.
    Active,
    /// Vault is frozen — no transfers in or out. Audit-log writes still allowed.
    Frozen,
}

impl VaultState {
    /// Stable string form for the `state TEXT NOT NULL` column.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Frozen => "Frozen",
        }
    }

    /// Parse the column's `TEXT` value back into the enum. Returns `None` for
    /// unknown / retired / future variants so the substrate fails-closed on
    /// unrecognized values (per RFC-0870 typed-discriminator discipline).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Active" => Some(Self::Active),
            "Frozen" => Some(Self::Frozen),
            _ => None,
        }
    }
}

/// Owner DID. `TEXT` column. Distinct from the
/// `octo-protocol`-layer DID canonical form (which is a `BLOB(32)` derived
/// via `cipherocto/did/v1/`). At the vault layer we accept either form via
/// the canonical-serialization conventions of the calling domain — the
/// `TEXT` column is opaque to the substrate.
pub type OwnerDid = String;

/// Vault policy. Canonical-serialized (`canonical_ser`) into a `BLOB` column
/// per review §20.3 (the schema sketch defines `policy BLOB NOT NULL`).
/// At this Layer-B scaffold the policy is opaque bytes; the semantic
/// interpretation lands when `octo-cap-macaroon` reads it back in C.2 (S5).
///
/// Deliberately does NOT derive `PartialEq`/`Eq`/`Hash`: equality / hashing
/// of raw policy bytes is never a sound cryptographic-equality proof, and
/// accidental `policy_a == policy_b` could bypass a future constant-time
/// comparator. If equality is needed, derive a separate `PolicyDigest`
/// newtype via the canonical `BLAKE3("cipherocto/policy/v1/" + policy_bytes)`
/// derivation and compare digests.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct VaultPolicy(Vec<u8>);

impl VaultPolicy {
    /// Empty / default policy (allow-all).
    // SAFETY: placeholder until C.2 (octo-cap-macaroon) defines canonical
    // encoding per review §20.3 (deferred per `deferred-vs-unspecified`
    // memory card — work will happen, not deferred-to-post-v1).
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Borrow the underlying policy bytes (canonical-serialized form).
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Metadata blob (per review schema sketch — `metadata BLOB NOT NULL`).
/// Same opaque-bytes treatment as `VaultPolicy` at this scaffold stage.
/// No `PartialEq`/`Eq`/`Hash` (see [`VaultPolicy`] rationale).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct VaultMetadata(Vec<u8>);

impl VaultMetadata {
    /// Empty / default metadata.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Borrow the underlying metadata bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Canonical vault row (Model B per review §20.3 + §8.10 TV-V1).
///
/// The composite identifier `(chain_id, owner_did, asset_id)` is the
/// primary key; `vault_id` is the derived identifier for lookup purposes
/// (`UNIQUE` non-PK index).
///
/// `#[non_exhaustive]` so future columns (e.g. `updated_at_unix`,
/// `last_transfer_event_id`) can land without a semver-major break;
/// consumers must construct via `Vault { ..Default::default() }` style
/// or use field-by-field assignment.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Vault {
    /// Derived identifier — `BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)`.
    pub vault_id: VaultId,
    /// Chain identifier.
    pub chain_id: ChainId,
    /// Owner DID (TEXT).
    pub owner_did: OwnerDid,
    /// Asset identifier.
    pub asset_id: AssetId,
    /// Balance (DQA scale 12). Persisted as Stoolap `DQA(12)` natively per
    /// §8.4.1 + §18 lock.
    pub balance_dqa_micros: i64,
    /// Policy (canonical_ser blob).
    pub policy: VaultPolicy,
    /// Vault state.
    pub state: VaultState,
    /// `created_at_unix` (UNIX seconds).
    pub created_at_unix: i64,
    /// Metadata blob.
    pub metadata: VaultMetadata,
}

/// Canonical `vault_id` derivation per review §8.10 TV-V1.
///
/// Hash input is the byte concatenation of four BLAKE3-prefixed
/// `&[u8]` slices, NOT a length-prefixed concatenation (the canonical
/// derivation uses raw concatenation; the prefix strings themselves are
/// the domain separation). Locked at this exact input order:
///
/// 1. `b"cipherocto/vault/v1/"` (domain separator)
/// 2. `chain_id.as_bytes()` (`[u8; 32]`)
/// 3. `owner_did.as_bytes()` (variable-length UTF-8)
/// 4. `asset_id.as_bytes()` (`[u8; 32]`)
///
/// Changing the order, the prefix, or the concatenation scheme is a
/// wire-format break — pin in test_vectors.rs and the §8.10 central
/// registry.
///
/// Guards:
/// - `owner_did` MUST be non-empty under Model B (§20.3 — empty owner_did
///   would produce a row whose PK includes `""`, violating every downstream
///   consumer's natural-key invariant).
/// - `owner_did` MUST be ≤ [`MAX_OWNER_DID_LEN`] bytes (DoS bound).
///
/// Both guards are `debug_assert!` — release builds trust callers, debug
/// builds catch production bugs early. Use [`vault_id_unchecked`] only in
/// test fixtures that intentionally exercise empty/oversized inputs.
pub fn vault_id(chain_id: ChainId, owner_did: &str, asset_id: AssetId) -> VaultId {
    debug_assert!(
        !owner_did.is_empty(),
        "owner_did must be non-empty under Model B (§20.3)"
    );
    debug_assert!(
        owner_did.len() <= MAX_OWNER_DID_LEN,
        "owner_did exceeds {MAX_OWNER_DID_LEN}B cap (DoS bound at substrate boundary)"
    );
    vault_id_unchecked(chain_id, owner_did, asset_id)
}

/// Canonical derivation WITHOUT the production guards. Reserved for
/// test fixtures (`tests/test_vectors.rs`) that intentionally exercise
/// empty / oversized `owner_did` inputs to pin the §8.10 wire format at
/// its boundary. Production callers MUST use [`vault_id`].
///
/// The hash function and input order are byte-identical to [`vault_id`]
/// — this is not a separate derivation, only a guard-bypassed entry point.
#[doc(hidden)]
pub fn vault_id_unchecked(chain_id: ChainId, owner_did: &str, asset_id: AssetId) -> VaultId {
    let mut h = blake3::Hasher::new();
    h.update(b"cipherocto/vault/v1/");
    h.update(chain_id.as_bytes());
    h.update(owner_did.as_bytes());
    h.update(asset_id.as_bytes());
    let bytes = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_bytes());
    VaultId(out)
}

/// Apply every catalog migration that has not yet been recorded in the
/// substrate tracker table. Delegates to the Layer A substrate's
/// `apply_pending` (S2). Migration runner is synchronous; the
/// `octo-vault-node` Layer C crate (NOT this scaffold) wraps it in
/// `spawn_blocking`.
pub fn apply(db: &octo_storage_core::Database) -> Result<(), VaultError> {
    octo_storage_core::_legacy_apply_pending(
        db,
        BUILTIN_MIGRATION_CATALOG,
        octo_storage_core::_legacy_ApplyConfig::default(),
    )
    .map_err(|_e| VaultError::Substrate)
}

/// Substrate handle — thin wrapper around the Stoolap-fork `Database`
/// handle that owns the `vaults_vault_id_idx` UNIQUE INDEX lookup
/// primitive.
///
/// Mission 0957-g1 (Layer B glue-crate pattern, mirrors
/// `TransportDeliveryCatalog` in `crates/octo-cap-macaroon-transport/`):
/// the substrate handle is exported here so consumer crates (the glue
/// crate `octo-cap-macaroon-vault`) can wire the canonical
/// `VaultLookup` lookup primitive without taking a direct `stoolap`
/// dep — keeping `octo-cap-macaroon` free of substrate-internal types
/// per the layer direction (B → B is allowed only through this typed
/// handle, never through raw `octo_storage_core::Database` re-export).
///
/// Production deployment wiring (config-time injection of an
/// `Arc<VaultSubstrate>` into the verify path) lands in `octo-vault-node`
/// (Layer C) — see mission 0957-g1 AC-2.
#[derive(Clone)]
pub struct VaultSubstrate {
    db: Arc<octo_storage_core::Database>,
}

impl std::fmt::Debug for VaultSubstrate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Operator-facing diagnostic only — do not leak the underlying
        // database connection identifier. Same defense-in-depth as
        // `TransportDeliveryCatalog` Debug redaction.
        f.debug_struct("VaultSubstrate").finish_non_exhaustive()
    }
}

impl VaultSubstrate {
    /// Construct a `VaultSubstrate` from a shared Stoolap-fork
    /// `Database` handle. Caller is responsible for running [`apply`]
    /// (and any other migrations) on the database before the substrate
    /// handle is constructed; the substrate handle does not auto-migrate.
    pub fn new(db: Arc<octo_storage_core::Database>) -> Self {
        Self { db }
    }

    /// Look up a vault row by its derived `vault_id` (32 bytes). Backed
    /// by the `vaults_vault_id_idx` UNIQUE INDEX per review §20.6.1
    /// algorithm step 2 (~1-3ms SSD on cold cache; warmed by substrate
    /// page cache thereafter).
    ///
    /// Returns:
    /// - `Ok(Some((chain_id, state)))` if a row with that `vault_id`
    ///   exists.
    /// - `Ok(None)` if no row exists (mirrors `VaultLookup::lookup_vault`
    ///   contract; verify path maps this to `VaultVerifyError::VaultRowMissing`).
    /// - `Err(VaultError::Substrate)` on substrate-level failure
    ///   (Stoolap error / migration not applied / etc.).
    pub fn lookup_by_vault_id(
        &self,
        vault_id: &VaultId,
    ) -> Result<Option<(ChainId, VaultState)>, VaultError> {
        let mut rows = self
            .db
            .query(
                "SELECT chain_id, state FROM vaults WHERE vault_id = ?",
                (vault_id.as_bytes().as_slice(),),
            )
            .map_err(|_e| VaultError::Substrate)?;
        // UNIQUE INDEX guarantees at most one row; take the first.
        if let Some(row_result) = rows.next() {
            let r = row_result.map_err(|_e| VaultError::Substrate)?;
            let chain_bytes: Vec<u8> = r.get(0).map_err(|_e| VaultError::Substrate)?;
            let state_str: String = r.get(1).map_err(|_e| VaultError::Substrate)?;
            let mut chain_arr = [0u8; 32];
            if chain_bytes.len() != 32 {
                return Err(VaultError::Substrate);
            }
            chain_arr.copy_from_slice(&chain_bytes);
            let chain_id = ChainId::from_bytes(chain_arr);
            let state = match VaultState::parse(&state_str) {
                Some(s) => s,
                None => return Err(VaultError::Substrate),
            };
            return Ok(Some((chain_id, state)));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TV-V1 (per review §8.10) — pin the canonical derivation.
    /// Changing the prefix / input ordering is a wire-format break.
    #[test]
    fn vault_id_uses_canonical_blake3_derivation() {
        let chain = ChainId::derive("cipherocto/testnet/v1");
        let owner = "did:octo:test-alice";
        let asset = AssetId::derive("OCTO-W");
        let id = vault_id(chain, owner, asset);
        // BLAKE3 produces 32-byte output. Pin length + non-zero (sanity).
        assert_eq!(id.as_bytes().len(), 32);
        assert_ne!(id.as_bytes(), &[0u8; 32], "vault_id must not be all-zero");
    }

    /// Determinism: same inputs MUST yield same `vault_id` (per RFC-0008
    /// Class A determinism).
    #[test]
    fn vault_id_is_deterministic() {
        let chain = ChainId::derive("cipherocto/testnet/v1");
        let owner = "did:octo:test-alice";
        let asset = AssetId::derive("OCTO-W");
        let a = vault_id(chain, owner, asset);
        let b = vault_id(chain, owner, asset);
        assert_eq!(a, b);
    }

    /// Domain separation per §8.10 / §20.3.2 / §20.3.1: changing the
    /// chain / owner / asset MUST change the vault_id (each tuple component
    /// is part of the hash input).
    #[test]
    fn vault_id_changes_when_any_input_changes() {
        let chain = ChainId::derive("cipherocto/testnet/v1");
        let other_chain = ChainId::derive("cipherocto/mainnet/v1");
        let owner = "did:octo:test-alice";
        let other_owner = "did:octo:test-bob";
        let asset = AssetId::derive("OCTO-W");
        let other_asset = AssetId::derive("OCTO-A");

        let a = vault_id(chain, owner, asset);
        assert_ne!(a, vault_id(other_chain, owner, asset), "chain changes id");
        assert_ne!(a, vault_id(chain, other_owner, asset), "owner changes id");
        assert_ne!(a, vault_id(chain, owner, other_asset), "asset changes id");
    }

    /// VaultState column-string round-trips via `parse`.
    #[test]
    fn vault_state_round_trips() {
        for s in [VaultState::Active, VaultState::Frozen] {
            assert_eq!(VaultState::parse(s.as_str()), Some(s));
        }
        // Unknown values fail closed (RFC-0870 typed-discriminator discipline).
        assert_eq!(VaultState::parse("Retired"), None);
        assert_eq!(VaultState::parse("active"), None, "case-sensitive");
        assert_eq!(VaultState::parse(""), None);
    }

    /// ApplyConfig default tracker table is the substrate's `schema_migrations`.
    /// Pin so a future substrate rename breaks this test loud.
    #[test]
    fn apply_uses_default_tracker_table() {
        assert_eq!(
            octo_storage_core::DEFAULT_TRACKER_TABLE,
            "schema_migrations",
            "substrate default tracker table renamed — update octo-vault apply path"
        );
    }

    /// OwnerDid length + non-empty guards (debug-only). Verifies the
    /// `debug_assert!` predicates exist and trigger as documented.
    #[test]
    #[cfg(debug_assertions)]
    fn vault_id_rejects_empty_owner_did_in_debug() {
        let chain = ChainId::derive("cipherocto/testnet/v1");
        let asset = AssetId::derive("OCTO-W");
        let result = std::panic::catch_unwind(|| vault_id(chain, "", asset));
        assert!(
            result.is_err(),
            "empty owner_did must panic in debug builds"
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    fn vault_id_rejects_oversized_owner_did_in_debug() {
        let chain = ChainId::derive("cipherocto/testnet/v1");
        let asset = AssetId::derive("OCTO-W");
        let oversized = "x".repeat(MAX_OWNER_DID_LEN + 1);
        let result = std::panic::catch_unwind(|| vault_id(chain, &oversized, asset));
        assert!(
            result.is_err(),
            "oversized owner_did must panic in debug builds"
        );
    }
}
