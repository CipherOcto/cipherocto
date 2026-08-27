//! Substrate primitives layer — canonical home for trait declarations +
//! newtypes per RFC-0105 v3.5 §3.1 / §3.11 / §3.12.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles`, this module is Layer A frozen
//! substrate (RFC-frozen, semver-major only, years-stable). Traits and
//! newtypes declared here MUST NOT have semver-minor breakage.
//!
//! ## Mission D + Mission E substrate additions
//!
//! All traits, newtypes, and primitives land here per RFC-0105 v3.5
//! canonical home rule (single-source-of-truth). Concrete impls (e.g.,
//! `InMemoryAssetRegistry`, `CachedAssetRegistry`,
//! `WALPrimaryNonceRegistry`) live in `octo-vault` (Layer B substrate
//! handle + storage coupling). Both consumer crates (RFC-0960 v3.6,
//! RFC-0965 v2.1, RFC-0959 v2.8) depend on `octo-cap-macaroon` for
//! these primitives — NOT on `octo-vault` — to avoid circular deps
//! (octo-vault depends on octo-cap-macaroon for the substrate handle;
//! consumer crates depend on octo-cap-macaroon for the substrate
//! types).

use std::collections::{HashMap, HashSet};

use blake3::Hasher as Blake3Hasher;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// =============================================================================
// Newtypes (Mission D §12)
// =============================================================================

/// 32-byte canonical identifier for a cipherocto asset. Derived from
/// a role-token string per RFC-0105 v3.5 §3.1 L168 with derivation
/// rule `BLAKE3("cipherocto/asset/v1/" + role_token)`. The full
/// `AssetId::derive` + `derive_v1` impls live in `octo-vault` (Layer B)
/// which has access to the `chain_id` + `owner_did` context; this
/// newtype is the substrate-import path.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct AssetId(pub [u8; 32]);

impl AssetId {
    /// Build from raw 32 bytes (no derivation).
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derive canonical `asset_id` per RFC-0105 v3.5 §3.1 L168 + RFC-0960
    /// §20.3.1: `BLAKE3("cipherocto/asset/v1/" || role_token)`.
    ///
    /// This derivation rule is substrate-frozen (Layer A years-stable).
    /// Changing the domain prefix or the input concatenation scheme is a
    /// wire-format break — pin in `crates/octo-vault/tests/test_vectors.rs`.
    #[must_use]
    pub fn derive(role_token: &str) -> Self {
        let mut h = Blake3Hasher::new();
        h.update(b"cipherocto/asset/v1/");
        h.update(role_token.as_bytes());
        let bytes = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes.as_bytes());
        Self(out)
    }
}

/// 32-byte nonce. Anti-replay protection across event types
/// (RFC-0105 v3.5 §3.11 NonceRegistry substrate).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Nonce(pub [u8; 32]);

impl Nonce {
    /// Build from raw 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// u64 epoch counter (governance-pulse interval epochs per RFC-0105
/// §2.4).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Epoch(pub u64);

impl Epoch {
    /// Build from a u64 counter.
    pub const fn new(counter: u64) -> Self {
        Self(counter)
    }

    /// Borrow the inner counter.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 64-byte Ed25519 governance signature, paired with `GovernancePubkey`
/// (canonical home: `crate::governance_signature`).
///
/// `Borsh` derives added per Mission F (RFC-0960 v3.6) BurnEventRef
/// wire form. `serde` uses a hex-string adapter (canonical wire form
/// per RFC-0105 v3.5 §3.12 substrate convention) — `[u8; 64]` does
/// not impl `serde::Serialize`/`Deserialize` from the rust derive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct GovernanceSignature {
    /// Raw Ed25519 signature bytes.
    pub sig: [u8; 64],
}

impl GovernanceSignature {
    /// Build from raw 64 signature bytes.
    pub const fn from_bytes(sig: [u8; 64]) -> Self {
        Self { sig }
    }

    /// Borrow the signature bytes.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.sig
    }
}

/// 32-byte chain identifier. `chain_id = BLAKE3("cipherocto/chain/v1/" + chain_string)`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ChainId(pub [u8; 32]);

impl ChainId {
    /// Build from raw 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derive canonical `chain_id` per RFC-0960 §20.3.2:
    /// `BLAKE3("cipherocto/chain/v1/" || chain_string)`.
    ///
    /// This derivation rule is substrate-frozen (Layer A years-stable).
    #[must_use]
    pub fn derive(chain_string: &str) -> Self {
        let mut h = Blake3Hasher::new();
        h.update(b"cipherocto/chain/v1/");
        h.update(chain_string.as_bytes());
        let bytes = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes.as_bytes());
        Self(out)
    }
}

/// 32-byte vault identifier. Derived as
/// `BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)`
/// per RFC-0105 §8.10. The full derivation lives in `octo-vault`; this
/// newtype is the substrate-import path.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct VaultId(pub [u8; 32]);

impl VaultId {
    /// Build from raw 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Derive the sovereign-asset nonce namespace key per asset_id
/// (RFC-0105 v3.5 §3.11 L633).
///
/// `sovereign_nonce_namespace(asset_id) = blake3("octo:sovereign-nonce-ns:v1" || asset_id)`
#[must_use]
pub fn sovereign_nonce_namespace(asset_id: &AssetId) -> [u8; 32] {
    let mut h = Blake3Hasher::new();
    h.update(b"octo:sovereign-nonce-ns:v1");
    h.update(asset_id.as_bytes());
    let bytes = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_bytes());
    out
}

// =============================================================================
// AssetKind + AssetMetadata (RFC-0105 v3.5 §3.1)
// =============================================================================

/// Maximum wire-scale accepted by `AssetMetadata` (RFC-0105 v3.5 §3.1
/// L114-211). Scales above 18 are rejected at registration time.
pub const MAX_SCALE: u8 = 18;

/// Asset classification (RFC-0105 v3.5 §3.1).
///
/// `#[non_exhaustive]` per CLAUDE.md §Architectural Principles
/// "Extension over enumeration": Layer A frozen substrate enums MUST
/// be non-exhaustive so future asset kinds (RFC-driven additive
/// evolution) do NOT force every downstream consumer to a central
/// edit.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[non_exhaustive]
pub enum AssetKind {
    /// The native OCTO-W (formerly `MicroOctoW` pre-RFC-0105 v2.0) governance token.
    OctoW,
    /// Generic managed asset (vault-governed). `governance_pubkey` is required.
    ManagedAsset,
    /// Bridged external asset (cross-chain bridge-issued).
    BridgedExternalAsset,
    /// Wrapped cross-chain asset.
    WrappedCrossChainAsset,
    /// Sovereign role token (e.g., OCTO-A, OCTO-B, OCTO-O, OCTO-W).
    /// Burned by chain rule, NOT by vault governance key.
    SovereignRoleToken,
}

impl AssetKind {
    /// Stable string form for the `kind TEXT NOT NULL` column.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::OctoW => "OctoW",
            Self::ManagedAsset => "ManagedAsset",
            Self::BridgedExternalAsset => "BridgedExternalAsset",
            Self::WrappedCrossChainAsset => "WrappedCrossChainAsset",
            Self::SovereignRoleToken => "SovereignRoleToken",
        }
    }

    /// Parse the column's `TEXT` value back into the enum. Returns `None`
    /// for unknown / future variants so the substrate fails-closed.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "OctoW" => Some(Self::OctoW),
            "ManagedAsset" => Some(Self::ManagedAsset),
            "BridgedExternalAsset" => Some(Self::BridgedExternalAsset),
            "WrappedCrossChainAsset" => Some(Self::WrappedCrossChainAsset),
            "SovereignRoleToken" => Some(Self::SovereignRoleToken),
            _ => None,
        }
    }
}

/// Per-asset metadata (RFC-0105 v3.5 §3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AssetMetadata {
    /// Wire scale (0..=`MAX_SCALE`).
    pub wire_scale: u8,
    /// Display decimals (UI hint).
    pub display_decimals: u8,
    /// Display denomination string (e.g., "OCTO-W", "USDC").
    pub denomination: String,
    /// Symbol (e.g., "OCTO-W", "USDC").
    pub symbol: String,
    /// Asset kind.
    pub kind: AssetKind,
    /// Optional governance pubkey (None for sovereign assets burned by chain rule).
    pub governance_pubkey: Option<[u8; 32]>,
    /// Optional chain_id (set for BRIDGED/WRAPPED cross-chain assets).
    pub chain_id: Option<ChainId>,
    /// Asset name (canonical).
    pub asset_name: String,
    /// Tombstone flag — true means asset is retired; lookups fail-closed.
    pub tombstoned: bool,
}

impl Default for AssetMetadata {
    /// Default metadata: zero-scale, untombstoned, kind `OctoW`. Tests
    /// that need richer metadata MUST use struct update syntax or a
    /// dedicated helper (the type is `#[non_exhaustive]`).
    fn default() -> Self {
        Self::new(0, 0, String::new(), String::new(), AssetKind::OctoW)
    }
}

impl AssetMetadata {
    /// Canonical constructor (RFC-0105 v3.5 §3.1). Required because
    /// `AssetMetadata` is `#[non_exhaustive]` — external crates cannot
    /// use struct expression syntax even with `..Default::default()`.
    /// `governance_pubkey`, `chain_id`, and `tombstoned` default to
    /// `None` / `false` and can be set via the dedicated setters below.
    #[must_use]
    pub fn new(
        wire_scale: u8,
        display_decimals: u8,
        denomination: String,
        symbol: String,
        kind: AssetKind,
    ) -> Self {
        Self {
            wire_scale,
            display_decimals,
            denomination,
            symbol,
            kind,
            governance_pubkey: None,
            chain_id: None,
            asset_name: String::new(),
            tombstoned: false,
        }
    }

    /// Setter for `asset_name` (RFC-0105 v3.5 §3.1). Required because
    /// `#[non_exhaustive]` blocks struct update from external crates.
    #[must_use]
    pub fn with_asset_name(mut self, asset_name: impl Into<String>) -> Self {
        self.asset_name = asset_name.into();
        self
    }

    /// Setter for `governance_pubkey` (RFC-0105 v3.5 §3.1).
    #[must_use]
    pub fn with_governance_pubkey(mut self, pk: [u8; 32]) -> Self {
        self.governance_pubkey = Some(pk);
        self
    }

    /// Setter for `chain_id` (RFC-0105 v3.5 §3.1).
    #[must_use]
    pub fn with_chain_id(mut self, chain_id: ChainId) -> Self {
        self.chain_id = Some(chain_id);
        self
    }

    /// Setter for `tombstoned` flag (RFC-0105 v3.5 §3.1).
    #[must_use]
    pub fn with_tombstoned(mut self, tombstoned: bool) -> Self {
        self.tombstoned = tombstoned;
        self
    }
}

/// Asset registry errors (RFC-0105 v3.5 §3.1 + §3.5 L316-352).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AssetError {
    /// Asset ID not registered.
    #[error("asset_id not registered")]
    AssetUnknown,
    /// `wire_scale` exceeds `MAX_SCALE`.
    #[error("wire_scale {0} exceeds MAX_SCALE = {MAX_SCALE}")]
    ScaleOutOfRange(u8),
    /// Cached registry returned no entry (transient miss; caller retries against live registry).
    #[error("bounded cache miss; caller should retry against live registry")]
    BoundedCacheMiss,
}

/// AssetRegistry trait — canonical home per RFC-0105 v3.5 §3.1 L174-210.
pub trait AssetRegistry: Send + Sync {
    /// Resolve `asset_id` to its metadata. Returns `Err(AssetUnknown)` if
    /// the asset is not registered OR if it is tombstoned.
    fn metadata(&self, asset_id: &AssetId) -> Result<AssetMetadata, AssetError>;
}

/// Vault containment errors (RFC-0105 v3.5 §3.1 + Mission F v3.6 §2.2 Gate 3).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum VaultAssetError {
    /// `vault_id` not present in the registry.
    #[error("vault_id not registered")]
    VaultUnknown,
    /// `(vault_id, asset_id)` not in vault's asset containment.
    #[error("vault does not contain this asset")]
    VaultAssetMismatch,
}

/// VaultRegistry trait — canonical home per RFC-0105 v3.5 §3.1 + Mission F
/// v3.6 §2.2 Gate 3 (`contains_asset` check).
pub trait VaultRegistry: Send + Sync {
    /// Check whether `vault_id` is registered.
    fn vault_exists(&self, vault_id: &VaultId) -> bool;
    /// Check whether `vault_id` contains `asset_id` in its asset set.
    fn contains_asset(&self, vault_id: &VaultId, asset_id: &AssetId)
        -> Result<(), VaultAssetError>;
}

/// In-memory `VaultRegistry` impl for tests + substrate-local lookups.
#[derive(Debug, Default)]
pub struct InMemoryVaultRegistry {
    vaults: HashSet<VaultId>,
    vault_assets: HashMap<VaultId, HashSet<AssetId>>,
}

impl InMemoryVaultRegistry {
    /// Create a new in-memory vault registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a vault (no asset containment yet).
    pub fn register_vault(&mut self, vault_id: VaultId) -> bool {
        self.vaults.insert(vault_id)
    }

    /// Add `asset_id` to `vault_id`'s asset set.
    pub fn add_asset(&mut self, vault_id: &VaultId, asset_id: AssetId) {
        self.vault_assets
            .entry(*vault_id)
            .or_default()
            .insert(asset_id);
    }
}

impl VaultRegistry for InMemoryVaultRegistry {
    fn vault_exists(&self, vault_id: &VaultId) -> bool {
        self.vaults.contains(vault_id)
    }

    fn contains_asset(
        &self,
        vault_id: &VaultId,
        asset_id: &AssetId,
    ) -> Result<(), VaultAssetError> {
        if !self.vaults.contains(vault_id) {
            return Err(VaultAssetError::VaultUnknown);
        }
        match self.vault_assets.get(vault_id) {
            Some(set) if set.contains(asset_id) => Ok(()),
            _ => Err(VaultAssetError::VaultAssetMismatch),
        }
    }
}

// =============================================================================
// NonceRegistry (RFC-0105 v3.5 §3.11 + v3.5-r8 PROPOSAL)
// =============================================================================

/// Discriminator for the nonce observation key (Mission D v3.5-r8
/// PROPOSAL). Each event type uses a distinct LRU bucket per
/// `(event_kind, pk, nonce)` triple, preventing cross-event nonce
/// collision.
///
/// `#[repr(u8)]` with explicit discriminants prevents mid-list
/// insertion from shifting existing LRU bucket tags. Reserved 0 (nil)
/// + 4-127 (future additive) + 128-255 (vendor extension).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NonceEventKind {
    /// `BurnEventRef` (RFC-0960 v3.6).
    Burn = 1,
    /// `SettlementEvent` (RFC-0959 v2.8).
    Settlement = 2,
    /// `PaymentCaveat` (RFC-0965 v2.1).
    Payment = 3,
}

impl NonceEventKind {
    /// Stable u8 tag for LRU bucket hashing.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// Nonce registry errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NonceError {
    /// `(event_kind, pk, nonce)` tuple was previously observed.
    #[error("nonce already observed for this (event_kind, pk, nonce) triple")]
    AlreadyObserved {
        /// Event kind discriminator.
        event_kind: NonceEventKind,
        /// Governance pubkey or sovereign namespace.
        pk: [u8; 32],
        /// Observed nonce.
        nonce: [u8; 32],
    },
    /// `unobserve` failed because the target was never observed or
    /// already rolled back (v3.5-r8 PROPOSAL).
    #[error("nonce was not observed (unobserve target missing)")]
    NotObserved {
        /// Event kind discriminator.
        event_kind: NonceEventKind,
        /// Governance pubkey or sovereign namespace.
        pk: [u8; 32],
        /// Nonce targeted for unobserve.
        nonce: [u8; 32],
    },
    /// WAL persistence failure (e.g., disk full). Caller MUST retry
    /// with backoff. (NEW in v3.5-r5.)
    #[error("nonce registry persistence failure (WAL write)")]
    PersistenceFailure,
    /// WAL is recovering from outage; caller SHOULD retry with backoff.
    /// (NEW in v3.5-r6.)
    #[error("nonce registry WAL is recovering from outage")]
    WalRecovering,
}

/// Trait for tracking observed `(event_kind, pk, nonce)` triples.
pub trait NonceRegistry: Send + Sync {
    /// Mark `(event_kind, pk, nonce)` as observed. Returns
    /// `Err(AlreadyObserved)` on duplicate, `Err(PersistenceFailure)`
    /// on WAL failure, `Err(WalRecovering)` if WAL is in recovery.
    fn observe(
        &mut self,
        event_kind: NonceEventKind,
        pk: &[u8; 32],
        nonce: &[u8; 32],
    ) -> Result<(), NonceError>;

    /// Read-only check: was `(event_kind, pk, nonce)` previously observed?
    fn observe_readonly(
        &self,
        event_kind: NonceEventKind,
        pk: &[u8; 32],
        nonce: &[u8; 32],
    ) -> Result<(), NonceError>;

    /// Remove the observation (v3.5-r8 PROPOSAL). Used for atomicity
    /// rollback. Returns `Err(NotObserved)` if the target was never
    /// observed (or already rolled back).
    fn unobserve(
        &mut self,
        event_kind: NonceEventKind,
        pk: &[u8; 32],
        nonce: &[u8; 32],
    ) -> Result<(), NonceError>;
}

/// Compute the LRU bucket key for a `(event_kind, pk)` pair
/// (Mission D v3.5-r8 PROPOSAL).
///
/// `bucket_key = blake3("octo:nonce:v1:" || event_kind_tag || pk)`
#[must_use]
pub fn nonce_bucket_key(event_kind: NonceEventKind, pk: &[u8; 32]) -> [u8; 32] {
    let mut h = Blake3Hasher::new();
    h.update(b"octo:nonce:v1:");
    h.update(&[event_kind.tag()]);
    h.update(pk);
    let bytes = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_bytes());
    out
}

/// Helper: derive the observe key for a sovereign asset burn/settlement.
#[must_use]
pub fn sovereign_observe_key(asset_id: &AssetId) -> [u8; 32] {
    sovereign_nonce_namespace(asset_id)
}

// =============================================================================
// In-memory impls (test + substrate-local use only)
// =============================================================================

/// In-memory `AssetRegistry` impl for tests + substrate-local lookups.
#[derive(Debug, Default)]
pub struct InMemoryAssetRegistry {
    map: HashMap<AssetId, AssetMetadata>,
}

impl InMemoryAssetRegistry {
    /// Create a new in-memory registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an asset.
    pub fn register(
        &mut self,
        asset_id: AssetId,
        metadata: AssetMetadata,
    ) -> Option<AssetMetadata> {
        self.map.insert(asset_id, metadata)
    }

    /// Tombstone an asset (RFC-0105 §3.1 tombstone flag).
    pub fn tombstone(&mut self, asset_id: &AssetId) {
        if let Some(m) = self.map.get_mut(asset_id) {
            m.tombstoned = true;
        }
    }
}

impl AssetRegistry for InMemoryAssetRegistry {
    fn metadata(&self, asset_id: &AssetId) -> Result<AssetMetadata, AssetError> {
        self.map
            .get(asset_id)
            .filter(|m| !m.tombstoned)
            .cloned()
            .ok_or(AssetError::AssetUnknown)
    }
}

/// In-memory `NonceRegistry` impl for tests.
#[derive(Debug, Default)]
pub struct InMemoryNonceRegistry {
    observed: HashSet<(NonceEventKind, [u8; 32], [u8; 32])>,
}

impl InMemoryNonceRegistry {
    /// Create a new in-memory nonce registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl NonceRegistry for InMemoryNonceRegistry {
    fn observe(
        &mut self,
        event_kind: NonceEventKind,
        pk: &[u8; 32],
        nonce: &[u8; 32],
    ) -> Result<(), NonceError> {
        let key = (event_kind, *pk, *nonce);
        if self.observed.contains(&key) {
            return Err(NonceError::AlreadyObserved {
                event_kind,
                pk: *pk,
                nonce: *nonce,
            });
        }
        self.observed.insert(key);
        Ok(())
    }

    fn observe_readonly(
        &self,
        event_kind: NonceEventKind,
        pk: &[u8; 32],
        nonce: &[u8; 32],
    ) -> Result<(), NonceError> {
        let key = (event_kind, *pk, *nonce);
        if self.observed.contains(&key) {
            return Err(NonceError::AlreadyObserved {
                event_kind,
                pk: *pk,
                nonce: *nonce,
            });
        }
        Ok(())
    }

    fn unobserve(
        &mut self,
        event_kind: NonceEventKind,
        pk: &[u8; 32],
        nonce: &[u8; 32],
    ) -> Result<(), NonceError> {
        let key = (event_kind, *pk, *nonce);
        if !self.observed.remove(&key) {
            return Err(NonceError::NotObserved {
                event_kind,
                pk: *pk,
                nonce: *nonce,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_round_trip() {
        let n = Nonce([7u8; 32]);
        assert_eq!(n.as_bytes(), &[7u8; 32]);
        assert_eq!(Nonce::from_bytes([7u8; 32]), n);
    }

    #[test]
    fn epoch_round_trip() {
        let e = Epoch::new(42);
        assert_eq!(e.get(), 42);
    }

    #[test]
    fn governance_signature_round_trip() {
        let s = GovernanceSignature::from_bytes([9u8; 64]);
        assert_eq!(s.as_bytes()[0], 9);
    }

    #[test]
    fn sovereign_nonce_namespace_is_deterministic() {
        let asset = AssetId::from_bytes([1u8; 32]);
        assert_eq!(
            sovereign_nonce_namespace(&asset),
            sovereign_nonce_namespace(&asset)
        );
    }

    #[test]
    fn sovereign_nonce_namespace_changes_with_asset() {
        let a = sovereign_nonce_namespace(&AssetId::from_bytes([1u8; 32]));
        let b = sovereign_nonce_namespace(&AssetId::from_bytes([2u8; 32]));
        assert_ne!(a, b);
    }

    #[test]
    fn sovereign_nonce_namespace_is_not_all_zero() {
        let asset = AssetId::from_bytes([1u8; 32]);
        let ns = sovereign_nonce_namespace(&asset);
        assert_ne!(ns, [0u8; 32]);
    }

    #[test]
    fn asset_kind_round_trips() {
        for k in [
            AssetKind::OctoW,
            AssetKind::ManagedAsset,
            AssetKind::BridgedExternalAsset,
            AssetKind::WrappedCrossChainAsset,
            AssetKind::SovereignRoleToken,
        ] {
            assert_eq!(AssetKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(AssetKind::parse("Unknown"), None);
    }

    #[test]
    fn nonce_event_kind_explicit_discriminants() {
        assert_eq!(NonceEventKind::Burn.tag(), 1);
        assert_eq!(NonceEventKind::Settlement.tag(), 2);
        assert_eq!(NonceEventKind::Payment.tag(), 3);
    }

    #[test]
    fn bucket_key_changes_with_event_kind() {
        let pk = [1u8; 32];
        let a = nonce_bucket_key(NonceEventKind::Burn, &pk);
        let b = nonce_bucket_key(NonceEventKind::Settlement, &pk);
        assert_ne!(a, b);
    }

    #[test]
    fn max_scale_constant_is_18() {
        assert_eq!(MAX_SCALE, 18);
    }
}
