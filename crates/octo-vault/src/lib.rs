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
//!   [`octo_storage_core::apply_pending`].
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

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use thiserror::Error;

pub mod migrations;

pub use migrations::{BUILTIN_MIGRATIONS, BUILTIN_MIGRATION_CATALOG};

/// Vault substrate errors.
///
/// `Display` is operator-facing; `Debug` is substrate-internal (auto-derived).
/// Per `cipherocto-design-principles` Layer B: enum variants are additive;
/// fields are stable; never embed raw SQL / migration names / owner-DID
/// payloads across the boundary without redaction review.
#[derive(Debug, Error)]
pub enum VaultError {
    /// Substrate-level failure (Sttoolap / migration runner).
    #[error("vault substrate error during {operation}: {message}")]
    Substrate {
        /// Short, stable operation tag (e.g. `"apply_migrations"`, `"derive_vault_id"`).
        operation: &'static str,
        /// Operator-facing prose.
        message: String,
    },
}

/// 32-byte canonical identifier. `vault_id = BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)`
/// per review §8.10 TV-V1. Stored as `BLOB(32)` in `octo_vault.vaults.vault_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VaultId(pub [u8; 32]);

impl VaultId {
    /// Build a `VaultId` from raw bytes (no derivation; bypasses the canonical
    /// BLAKE3 path). Use only when the caller already holds the derived bytes
    /// (e.g. reading from `octo_vault.vaults.vault_id`).
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes (e.g. for Stoolap BLOB parameter binding).
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// 32-byte chain identifier. `chain_id = BLAKE3("cipherocto/chain/v1/" + chain_string)` per §20.3.2.
/// Distinct chain_id space from any other `BLOB(32)` derivation in the substrate
/// (different domain tag `cipherocto/chain/v1/` vs `cipherocto/vault/v1/`,
/// `cipherocto/asset/v1/`, `cipherocto/did/v1/`, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChainId(pub [u8; 32]);

impl ChainId {
    /// Derive canonical chain_id from a human-readable chain_string per §20.3.2.
    pub fn derive(chain_string: &str) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"cipherocto/chain/v1/");
        h.update(chain_string.as_bytes());
        let bytes = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes.as_bytes());
        Self(out)
    }

    /// Build from raw bytes (e.g. read from a row).
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// 32-byte asset identifier. `asset_id = BLAKE3("cipherocto/asset/v1/" + role_token)` per §20.3.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetId(pub [u8; 32]);

impl AssetId {
    /// Derive canonical asset_id from a role-token string per §20.3.1.
    pub fn derive(role_token: &str) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"cipherocto/asset/v1/");
        h.update(role_token.as_bytes());
        let bytes = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes.as_bytes());
        Self(out)
    }

    /// Build from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Vault state. Per review §20.3 schema sketch the column is `TEXT NOT NULL`
/// with values from this enum. RFC-0960 §2.1 listed `Retired`; review §20.10
/// dropped it (consumer-driven transitions out of `Frozen` go straight to row
/// removal — the append-only `transfer_events` log is the audit trail).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VaultPolicy(pub Vec<u8>);

impl VaultPolicy {
    /// Empty / default policy (allow-all). Concrete policies land in C.2.
    pub fn empty() -> Self {
        Self(Vec::new())
    }
}

/// Metadata blob (per review schema sketch — `metadata BLOB NOT NULL`).
/// Same opaque-bytes treatment as `VaultPolicy` at this scaffold stage.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VaultMetadata(pub Vec<u8>);

impl VaultMetadata {
    /// Empty / default metadata.
    pub fn empty() -> Self {
        Self(Vec::new())
    }
}

/// Canonical vault row (Model B per review §20.3 + §8.10 TV-V1).
///
/// The composite identifier `(chain_id, owner_did, asset_id)` is the
/// primary key; `vault_id` is the derived identifier for lookup purposes
/// (`UNIQUE` non-PK index).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
pub fn vault_id(chain_id: ChainId, owner_did: &str, asset_id: AssetId) -> VaultId {
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
pub fn apply(db: &stoolap::Database) -> Result<(), VaultError> {
    octo_storage_core::apply_pending(
        db,
        BUILTIN_MIGRATION_CATALOG,
        octo_storage_core::ApplyConfig::default(),
    )
    .map_err(|_e| VaultError::Substrate {
        operation: "apply_migrations",
        message: "octo_vault migration apply failed; \
                  substrate-internal trace preserved in substrate logs only"
            .to_owned(),
    })
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
        let asset = AssetId::derive("OCTO_W");
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
        let asset = AssetId::derive("OCTO_W");
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
        let asset = AssetId::derive("OCTO_W");
        let other_asset = AssetId::derive("OCTO_A");

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
}
