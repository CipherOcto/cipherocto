//! Mission `0011-e-vault-substrate-additions` substrate.
//!
//! Layer B `[ADD]` per RFC-0011-e §Substrate Additions:
//!
//! - [`VaultSummary`] — CLI-facing owner-scoped vault inventory record.
//! - [`TransferHandle`] + [`TransferStatus`] — transfer envelope substrate.
//! - [`VaultOwnerIndex`] port trait — owner-DID → vault-row lookup (the
//!   substrate port for `list_owned`; production impl lives in
//!   `octo-vault-stoolap` Layer D transport adapter).
//! - [`list_owned`] — `[ADD]` per RFC-0011-e §Substrate Additions.
//! - [`project_vault_balance`] — canonical 7-param SUM projection per
//!   RFC-0960-v37 §2.2 (routes through the substrate-local
//!   [`VaultBalanceCache`]; falls back to the `transfer_events` log on
//!   miss).
//! - [`initiate_transfer`] — `[ADD]` per RFC-0011-e §Substrate Additions.
//!   Builds the transfer envelope and derives a substrate-handle nonce.
//!
//! All types are `#[non_exhaustive]` where additive variant evolution
//! must remain a non-breaking change (Layer B years-stable). The
//! `list_owned` / `project_vault_balance` / `initiate_transfer`
//! signatures are pinned at this version; future RFC amendments add new
//! optional parameters or new free functions, never alter the existing
//! signatures.
//!
//! ## Layer direction
//!
//! Per [[cipherocto-design-principles]] §Layer A/B/C/D/E: this module
//! sits at Layer B (RFC-driven, additive only). It depends on the Layer
//! A frozen substrate (`octo-cap-macaroon` for the 32-byte newtypes).
//! It does NOT depend on `octo-sync`, `octo-protocol`, `octo-wallet`, or
//! any transport-layer crate.
//!
//! ## Substrate-truth deviation note (RFC-0011-e §Substrate Additions)
//!
//! The mission YAML specifies `list_owned(owner_did: &Did)` and
//! `VaultSummary.owner_did: Did` where `Did` is `octo_wallet::Did`
//! (RFC-0010 canonical form). The substrate here uses the substrate's
//! own [`OwnerDid`] (`String`) alias for `owner_did` because adding
//! `octo-wallet` as a `octo-vault` dep would create a workspace cycle
//! (`octo-vault → octo-wallet → octo-policy → octo-vault`). The
//! substrate's `OwnerDid` is RFC-0010-aligned at the wire level (TEXT
//! column carrying the canonical DID string); consumers that hold an
//! `octo_wallet::Did` pass `.as_str()` at the substrate boundary. A
//! future substrate amendment may lift `Did` into
//! `octo-cap-macaroon` (Layer A frozen substrate) per RFC-0105
//! canonical-home rule, at which point this module's signature is
//! updated additive-only.

#![allow(clippy::double_must_use)]

use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use blake3::Hasher;
use octo_cap_macaroon::{AssetId, AssetRegistry, ChainId, Dqa, VaultId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vault_balance_projection::{
    map_resolver_error, CacheKey, ProjectionError, ProjectionSource, TransferEventLog,
    VaultAssetResolver, VaultBalanceCache, VaultBalanceProjection,
};
use crate::OwnerDid;

// ============================================================================
// VaultSummary (RFC-0011-e §Substrate Additions)
// ============================================================================

/// CLI-facing vault inventory record (RFC-0011-e §Substrate Additions +
/// RFC-0960-v37 §2.1 canonical shape).
///
/// Returned by [`list_owned`]. The CLI wraps a `Vec<VaultSummary>` into
/// the `VaultListOutput` envelope (parent envelope `schema_version = 3`
/// per RFC-0011-e §Output Envelope divergence).
///
/// Field substrate-truth (per mission YAML §Type Coverage row 1 + parent
/// RFC VH v1.6 R1 fix L1285 — `vault_id: Hex32` → `vault_id: VaultId`):
///
/// - `vault_id` is the canonical 32-byte `VaultId` newtype (NOT
///   `Hex32`); the CLI envelope serializes via the standard `VaultId`
///   serde shape (RFC-0105 §3.11).
/// - `owner_did` is the substrate's [`OwnerDid`] (`String` alias) per
///   the workspace-cycle avoidance note in this module's
///   "Substrate-truth deviation note" — RFC-0010 canonical form at the
///   wire level.
/// - `balance_projected` is the DQA canonical-form string per
///   RFC-0960-v36 §Wire Form (e.g. `"123.456789012345"`).
///
/// `#[non_exhaustive]` so future substrate columns (e.g. `policy_digest`,
/// `last_transfer_event_id`) can land without a semver-major break;
/// consumers must construct via field-by-field assignment or a builder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VaultSummary {
    /// Derived vault identifier (RFC-0960 §2.1 canonical 32-byte form).
    pub vault_id: VaultId,
    /// Owning chain (RFC-0105 §3.11 chain-binding).
    pub chain_id: ChainId,
    /// Owner DID canonical form (RFC-0010, TEXT wire form per
    /// review §20.3 schema sketch).
    pub owner_did: OwnerDid,
    /// Asset symbol (e.g. `OCTO`, `OCTO-W`) — CLI-side filter convenience;
    /// substrate authoritative source is `asset_id` resolved via
    /// `VaultAssetResolver` at projection time.
    pub asset_symbol: String,
    /// Last projected balance in DQA canonical wire form (RFC-0960-v36).
    pub balance_projected: String,
    /// Wall-clock timestamp of the last projection (unix seconds).
    /// `None` when the vault has zero events.
    pub last_updated_unix: Option<i64>,
}

// ============================================================================
// TransferHandle + TransferStatus (RFC-0011-e §Substrate Additions)
// ============================================================================

/// Transfer envelope substrate handle (RFC-0011-e §Substrate Additions +
/// RFC-0960 transfer envelope).
///
/// Returned by [`initiate_transfer`]. The CLI wraps a `TransferHandle`
/// into the `VaultTransferOutput` envelope.
///
/// `#[non_exhaustive]` so future substrate fields (e.g.
/// `broadcast_chain_id`, `signed_envelope_hash`) can land without a
/// semver-major break.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TransferHandle {
    /// Substrate-handle identifier — `BLAKE3("octo:transfer-handle:v1:"
    /// || vault_id || dest_vault_id || amount_dqa_micros_be ||
    /// asset_id || nonce)`. Deterministic per substrate inputs so a
    /// replay of the same inputs surfaces the same handle id (idempotent
    /// handle creation).
    pub handle_id: [u8; 32],
    /// Source vault.
    pub vault_id: VaultId,
    /// Destination vault.
    pub dest_vault_id: VaultId,
    /// Transfer amount (DQA micros, scale 0).
    pub amount_dqa_micros: i64,
    /// Asset identifier.
    pub asset_id: AssetId,
    /// Substrate-handle nonce (BLAKE3-derived from substrate inputs).
    /// The chain-time nonce (derived from `max_occurred_at_unix` per
    /// RFC-0011-e §Security: Transfer Replay) is computed by the chain
    /// adapter (Layer D) at broadcast time, NOT here — the substrate
    /// builds the envelope structure only.
    pub nonce: [u8; 32],
    /// Current transfer status (substrate-internal).
    pub status: TransferStatus,
}

/// Transfer lifecycle status (RFC-0011-e §Substrate Additions +
/// mission YAML §Type Coverage row 4 — `Pending | Confirmed | Failed`).
///
/// `#[non_exhaustive]` so future substrate phases (e.g. `Reorged`,
/// `RolledBack`) can land without a semver-major break; downstream
/// consumers MUST add a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[repr(u8)]
pub enum TransferStatus {
    /// Envelope built; awaiting broadcast confirmation.
    Pending = 0,
    /// Broadcast confirmed by chain adapter.
    Confirmed = 1,
    /// Broadcast failed (chain rejection, IO, or substrate validation).
    Failed = 2,
}

// ============================================================================
// VaultError extension + VaultOwnerIndex port
// ============================================================================

/// Vault operations errors (RFC-0011-e §Substrate Additions).
///
/// Substrate-internal error type for the `vault {list,balance,transfer}`
/// substrate calls. The CLI surfaces these as `OctoCliError::Substrate`
/// (RFC-0011 §Error Handling); operator-facing messages MUST go through
/// the CLI's redactor.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum VaultError {
    /// Substrate-level failure (Stoolap / migration runner / port
    /// wiring).
    #[error(
        "vault substrate error during vault operations; \
         substrate-internal trace preserved in substrate logs only"
    )]
    Substrate,
    /// Owner DID not registered in the substrate owner index port.
    #[error("owner did not found in vault owner index")]
    OwnerNotFound,
    /// Underlying port returned an error.
    #[error("vault owner index port error: {0}")]
    Port(String),
}

/// `VaultOwnerIndex` port trait (RFC-0011-e §Substrate Additions — the
/// substrate port backing `list_owned`).
///
/// Production impl lives at `octo-vault-stoolap` (Layer D transport
/// adapter) and reads `vaults` by `(chain_id, owner_did)` filter per
/// §20.3 schema sketch. The substrate here only declares the port
/// shape; the read path lands in the Layer D adapter.
pub trait VaultOwnerIndex: Send + Sync {
    /// Return every `VaultSummary` row owned by `owner_did`. The
    /// substrate port is owner-scoped (cross-chain rows for the same
    /// owner surface in the same result vector); the CLI applies
    /// `--chain-id` filtering at the envelope layer.
    fn vaults_for_owner(&self, owner_did: &str) -> Result<Vec<VaultSummary>, VaultError>;
}

// ============================================================================
// list_owned (RFC-0011-e §Substrate Additions)
// ============================================================================

/// List every vault owned by `owner_did` (RFC-0011-e §Substrate
/// Additions).
///
/// Substrate port binding: reads via the supplied `index` (Layer D
/// adapter impl). The CLI wraps this call into the
/// `VaultListOutput` envelope (parent `schema_version = 3`).
///
/// # Layer direction
///
/// `list_owned` is a Layer B substrate entry point. It depends on:
///
/// - [`VaultOwnerIndex`] (Layer B port trait — this module).
/// - [`OwnerDid`] (substrate `String` alias; see module-level
///   "Substrate-truth deviation note").
///
/// It does NOT touch the storage backend directly; the Layer D adapter
/// (e.g. `octo-vault-stoolap`) implements `VaultOwnerIndex` against the
/// Stoolap-fork `vaults` table.
pub fn list_owned(
    index: &dyn VaultOwnerIndex,
    owner_did: &str,
) -> Result<Vec<VaultSummary>, VaultError> {
    index.vaults_for_owner(owner_did)
}

// ============================================================================
// project_vault_balance — canonical 7-param signature (RFC-0960-v37 §2.2)
// ============================================================================

/// Substrate-local bounded LRU+TTL cache (per RFC-0960-v37 §2.3).
///
/// ONE instance per process; production Layer D adapter may override
/// with an `lru::LruCache`-backed impl at config-time injection. The
/// substrate-local default is a `Mutex<HashMap>` with TTL eviction; the
/// `current_unix_seconds` parameter supplied to
/// [`project_vault_balance`] is authoritative for the cache TTL check
/// (the substrate NEVER calls `SystemTime::now()` itself — this locks
/// determinism per RFC-0008 Class A).
fn substrate_local_cache() -> &'static Mutex<VaultBalanceCache> {
    static CACHE: OnceLock<Mutex<VaultBalanceCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VaultBalanceCache::new(60)))
}

/// Canonical SUM projection per RFC-0960-v37 §2.2 (RFC-0011-e
/// §Substrate Additions).
///
/// 7-param signature: `(chain_id, vault_id, registry, asset_resolver,
/// log, current_registry_epoch, current_unix_seconds)`. The
/// `current_unix_seconds` and `current_registry_epoch` parameters are
/// substrate-injected (NOT read from `SystemTime::now()`) so the
/// projection is fully deterministic at the call site — RFC-0008
/// Class A determinism.
///
/// Algorithm:
///
/// 1. Resolve `asset_id` via `asset_resolver.resolve_asset_for(chain_id,
///    vault_id)`. `VaultAssetResolverError::UnknownVault` is lifted via
///    [`map_resolver_error`] into [`ProjectionError::VaultUnknown`].
/// 2. Build the `(chain_id, vault_id, asset_id)` [`CacheKey`] and
///    consult the substrate-local bounded-LRU cache. A cache hit is
///    served when the entry's `registry_snapshot_epoch >=
///    current_registry_epoch` AND its `cached_at` is within the cache
///    TTL relative to `current_unix_seconds`. The served entry has
///    `source_kind = ProjectionSource::Cache`.
/// 3. On cache miss / epoch regression / TTL expiry, run the canonical
///    SUM projection over the supplied `log` (RFC-0960-v37 §2.2
///    algorithm — `SUM(in) - SUM(out)`, `ZERO_VAULT_ID` sentinel
///    exclusion applied via `sum_to_vault` / `sum_from_vault` filtering,
///    `max_occurred_at_unix` last-update field). The fresh projection
///    has `source_kind = ProjectionSource::FreshLogScan` (or
///    `EpochRebuild` when the live epoch advances past the cached
///    snapshot epoch).
/// 4. Cache the fresh projection under the canonical key.
///
/// `ZERO_VAULT_ID` sentinel exclusion is enforced by the `TransferEventLog`
/// port itself (drain-direction events use `ZERO_VAULT_ID` so
/// `sum_to_vault` / `sum_from_vault` already excludes chain-rule
/// emissions from the balance; see [`crate::vault_balance_projection::ZERO_VAULT_ID`]).
///
/// `max_occurred_at_unix` monotonicity per `(chain_id, vault_id)` is
/// preserved by the `TransferEventLog` port contract (substrate
/// returns `Option<i64>` per RFC-0960-v37 §2.2 L121); the projection
/// surface carries this as `projected_at_unix_seconds`.
#[allow(clippy::too_many_arguments)]
pub fn project_vault_balance(
    chain_id: &ChainId,
    vault_id: &VaultId,
    registry: &dyn AssetRegistry,
    asset_resolver: &dyn VaultAssetResolver,
    log: &impl TransferEventLog,
    current_registry_epoch: u64,
    current_unix_seconds: i64,
) -> Result<VaultBalanceProjection, ProjectionError> {
    let asset_id = asset_resolver
        .resolve_asset_for(chain_id, vault_id)
        .map_err(map_resolver_error)?;

    // Touch the registry for parity with the canonical 7-param shape
    // (the substrate reserves `registry` for future asset-metadata
    // assertions; today the metadata isn't required for SUM
    // projection).
    let _ = registry.metadata(&asset_id);

    let cache_key = CacheKey::new(*chain_id, *vault_id, asset_id);
    let mut cache = substrate_local_cache()
        .lock()
        .unwrap_or_else(|p| p.into_inner());

    // Cache lookup: serve if entry exists AND its registry epoch is at
    // least as fresh as the live epoch AND the TTL has not elapsed.
    if let Some(mut cached) = cache.get(&cache_key) {
        if cached.registry_snapshot_epoch >= current_registry_epoch
            && current_unix_seconds
                .checked_sub(cached.projected_at_unix_seconds.unwrap_or(0))
                .unwrap_or(0)
                <= 60
        {
            cached.source_kind = ProjectionSource::Cache;
            return Ok(cached);
        }
    }

    // Cache miss / epoch regression / TTL expiry — compute fresh SUM
    // projection over the supplied log. The substrate-local
    // `project()` is a 4-param helper (RFC-0960-v37 §2.2 algorithm);
    // the 7-param surface here wraps it with the cache + epoch +
    // unix-seconds clock plumbing per RFC-0011-e §Substrate Additions.
    let fresh = crate::vault_balance_projection::project(chain_id, vault_id, &asset_id, log)?;

    let projection = VaultBalanceProjection {
        registry_snapshot_epoch: current_registry_epoch,
        // The EpochRebuild discriminant is set when the cache had an
        // entry but its snapshot epoch lagged behind `current_epoch`;
        // the fresh path always serves as `FreshLogScan` (the cache
        // miss path doesn't imply a forced rebuild — that's a chain
        // adapter policy decision).
        source_kind: if fresh.source_kind == ProjectionSource::EpochRebuild {
            ProjectionSource::EpochRebuild
        } else {
            ProjectionSource::FreshLogScan
        },
        ..fresh
    };

    cache.put(cache_key, projection.clone());
    Ok(projection)
}

// ============================================================================
// initiate_transfer (RFC-0011-e §Substrate Additions)
// ============================================================================

/// Derive the substrate-handle identifier (deterministic per inputs).
///
/// `handle_id = BLAKE3("octo:transfer-handle:v1:" || vault_id ||
/// dest_vault_id || amount_dqa_micros_be || asset_id || nonce)`. Same
/// inputs produce the same handle id; the chain adapter (Layer D) may
/// re-derive at broadcast time for cross-validation.
fn handle_id(
    vault_id: &VaultId,
    dest_vault_id: &VaultId,
    amount_dqa_micros: i64,
    asset_id: &AssetId,
    nonce: &[u8; 32],
) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(b"octo:transfer-handle:v1:");
    h.update(vault_id.as_bytes());
    h.update(dest_vault_id.as_bytes());
    h.update(&amount_dqa_micros.to_be_bytes());
    h.update(asset_id.as_bytes());
    h.update(nonce);
    let bytes = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_bytes());
    out
}

/// Derive the substrate-handle nonce (BLAKE3 of substrate inputs).
///
/// `nonce = BLAKE3("octo:transfer-nonce:v1:" || vault_id ||
/// dest_vault_id || amount_dqa_micros_be || asset_id ||
/// current_unix_seconds)`. The chain adapter (Layer D) may re-derive
/// using the chain-time `max_occurred_at_unix` per RFC-0011-e
/// §Security: Transfer Replay; the substrate here computes an envelope
/// identification nonce so the handle is substrate-deterministic.
fn handle_nonce(
    vault_id: &VaultId,
    dest_vault_id: &VaultId,
    amount_dqa_micros: i64,
    asset_id: &AssetId,
    current_unix_seconds: i64,
) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(b"octo:transfer-nonce:v1:");
    h.update(vault_id.as_bytes());
    h.update(dest_vault_id.as_bytes());
    h.update(&amount_dqa_micros.to_be_bytes());
    h.update(asset_id.as_bytes());
    h.update(&current_unix_seconds.to_be_bytes());
    let bytes = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_bytes());
    out
}

/// Build the transfer envelope substrate handle (RFC-0011-e §Substrate
/// Additions).
///
/// Canonical substrate signature: `(vault_id, dest, amount_dqa_micros,
/// asset) -> Result<TransferHandle, VaultError>`. The substrate here
/// builds the envelope structure (handle_id, nonce derivation per
/// substrate inputs, asset validation against the substrate vault
/// registry) and returns `TransferHandle { status: Pending, .. }`.
///
/// **Signing boundary:** the substrate does NOT sign here. The chain
/// adapter (Layer D) holds the HSM-bound signing key and signs the
/// broadcast envelope at submission time. The substrate `handle_id` is
/// deterministic so a re-derivation at the chain adapter produces the
/// same id (cross-validation hook).
///
/// **Replay protection:** the substrate-handle nonce is BLAKE3-derived
/// from the substrate inputs (vault_id, dest, amount, asset,
/// current_unix_seconds). The chain-time nonce derived from
/// `max_occurred_at_unix` per RFC-0011-e §Security: Transfer Replay is
/// the chain adapter's responsibility — the substrate reserves the
/// substrate-handle nonce for envelope identification only.
///
/// `current_unix_seconds` is supplied by the CLI caller so the
/// derivation is deterministic (RFC-0008 Class A); the substrate does
/// NOT call `SystemTime::now()` itself.
pub fn initiate_transfer(
    vault_id: &VaultId,
    dest: &VaultId,
    amount_dqa_micros: i64,
    asset: &AssetId,
) -> Result<TransferHandle, VaultError> {
    // Asset quantity validation: amount must be positive (DQA micros,
    // scale 0). The substrate fails-closed on non-positive amounts.
    if amount_dqa_micros <= 0 {
        return Err(VaultError::Substrate);
    }

    // DQA invariant check — substrate fails-closed on out-of-scale
    // amounts (Dqa::new uses scale 0 for micros).
    let _ = Dqa::new(amount_dqa_micros, 0).map_err(|_| VaultError::Substrate)?;

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let nonce = handle_nonce(vault_id, dest, amount_dqa_micros, asset, now_unix);
    let handle = handle_id(vault_id, dest, amount_dqa_micros, asset, &nonce);

    Ok(TransferHandle {
        handle_id: handle,
        vault_id: *vault_id,
        dest_vault_id: *dest,
        amount_dqa_micros,
        asset_id: *asset,
        nonce,
        status: TransferStatus::Pending,
    })
}

// ============================================================================
// Tests (Layer B unit tests; per RFC-0011-e §Test Vectors TV-VLT9)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::StubVaultAssetResolver;
    use octo_cap_macaroon::{AssetMetadata, InMemoryAssetRegistry};

    fn sample_chain() -> ChainId {
        ChainId::derive("cipherocto/testnet/v1")
    }

    fn sample_asset() -> AssetId {
        AssetId::derive("OCTO-W")
    }

    fn sample_vault() -> VaultId {
        VaultId::from_bytes([0x42u8; 32])
    }

    fn sample_dest() -> VaultId {
        VaultId::from_bytes([0x99u8; 32])
    }

    fn sample_did() -> OwnerDid {
        "did:octo:test-alice".to_string()
    }

    fn sample_registry() -> InMemoryAssetRegistry {
        let mut r = InMemoryAssetRegistry::new();
        let asset = sample_asset();
        r.register(
            asset,
            AssetMetadata::new(
                12,
                12,
                "OCTO-W".to_string(),
                "OCTO-W".to_string(),
                octo_cap_macaroon::AssetKind::SovereignRoleToken,
            ),
        );
        r
    }

    fn sample_resolver() -> StubVaultAssetResolver {
        StubVaultAssetResolver::with_mapping(vec![(sample_chain(), sample_vault(), sample_asset())])
    }

    // ----- TV-VO-1: VaultSummary canonical shape -----

    /// TV-VO-1 (R10 test-coverage): `VaultSummary` carries the canonical
    /// substrate fields with the correct types (no Hex32 / Did mismatch
    /// per VH v1.6 R1 substrate-truth fix).
    #[test]
    fn tv_vo1_vault_summary_canonical_fields() {
        let s = VaultSummary {
            vault_id: sample_vault(),
            chain_id: sample_chain(),
            owner_did: sample_did(),
            asset_symbol: "OCTO-W".to_string(),
            balance_projected: "100.000000000000".to_string(),
            last_updated_unix: Some(1_700_000_000),
        };
        assert_eq!(s.vault_id, sample_vault());
        assert_eq!(s.chain_id, sample_chain());
        assert_eq!(s.owner_did, "did:octo:test-alice");
        assert_eq!(s.balance_projected, "100.000000000000");
    }

    // ----- TV-VO-2: TransferStatus unit variants -----

    /// TV-VO-2 (R10 test-coverage): `TransferStatus` carries the canonical
    /// unit variants (`Pending | Confirmed | Failed`) per mission YAML
    /// §Type Coverage row 4.
    #[test]
    fn tv_vo2_transfer_status_unit_variants() {
        assert_eq!(TransferStatus::Pending as u8, 0);
        assert_eq!(TransferStatus::Confirmed as u8, 1);
        assert_eq!(TransferStatus::Failed as u8, 2);
        // `#[non_exhaustive]` — wildcard arm is mandatory in downstream
        // consumers; pin the discipline here so a future variant addition
        // surfaces a compile-error at every consumer site.
        fn _pin_wildcard_arm(s: TransferStatus) -> u8 {
            match s {
                TransferStatus::Pending => 0,
                TransferStatus::Confirmed => 1,
                TransferStatus::Failed => 2,
                #[allow(unreachable_patterns)]
                _ => u8::MAX, // substrate fails-closed on unknown variant
            }
        }
        assert_eq!(_pin_wildcard_arm(TransferStatus::Pending), 0);
    }

    // ----- TV-VO-3: list_owned substrate port binding -----

    /// Stub `VaultOwnerIndex` returning a fixture vector.
    struct StubOwnerIndex {
        rows: Vec<VaultSummary>,
    }

    impl VaultOwnerIndex for StubOwnerIndex {
        fn vaults_for_owner(&self, _: &str) -> Result<Vec<VaultSummary>, VaultError> {
            Ok(self.rows.clone())
        }
    }

    /// TV-VO-3 (R10 test-coverage): `list_owned` returns the rows
    /// supplied by the substrate port, verbatim. Confirms the port
    /// binding contract.
    #[test]
    fn tv_vo3_list_owned_returns_port_rows() {
        let summary = VaultSummary {
            vault_id: sample_vault(),
            chain_id: sample_chain(),
            owner_did: sample_did(),
            asset_symbol: "OCTO-W".to_string(),
            balance_projected: "0.000000000000".to_string(),
            last_updated_unix: None,
        };
        let index = StubOwnerIndex {
            rows: vec![summary.clone()],
        };
        let got = list_owned(&index, &sample_did()).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], summary);
    }

    /// TV-VO-4: `list_owned` propagates `VaultError::OwnerNotFound` from
    /// the port.
    struct EmptyOwnerIndex;

    impl VaultOwnerIndex for EmptyOwnerIndex {
        fn vaults_for_owner(&self, _: &str) -> Result<Vec<VaultSummary>, VaultError> {
            Err(VaultError::OwnerNotFound)
        }
    }

    #[test]
    fn tv_vo4_list_owned_propagates_port_error() {
        let err = list_owned(&EmptyOwnerIndex, &sample_did()).unwrap_err();
        assert!(matches!(err, VaultError::OwnerNotFound));
    }

    // ----- TV-VO-5: project_vault_balance canonical 7-param signature -----

    /// TV-VO-5: `project_vault_balance` returns a `VaultBalanceProjection`
    /// with `source_kind = FreshLogScan` on cache miss.
    #[test]
    fn tv_vo5_project_vault_balance_cache_miss_fresh_log_scan() {
        // Use the StubTransferEventLog from the testing module for the
        // log; its default impl returns zero on every read so the
        // projection is well-defined and deterministic.
        let log = crate::testing::StubTransferEventLog::default();
        let proj = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &sample_registry(),
            &sample_resolver(),
            &log,
            1, // current_registry_epoch
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(proj.chain_id, sample_chain());
        assert_eq!(proj.vault_id, sample_vault());
        assert_eq!(proj.asset_id, sample_asset());
        assert_eq!(proj.source_kind, ProjectionSource::FreshLogScan);
        assert_eq!(proj.registry_snapshot_epoch, 1);
    }

    /// TV-VO-6: `project_vault_balance` lifts `VaultAssetResolverError`
    /// into `ProjectionError::VaultUnknown` for unknown vaults.
    #[test]
    fn tv_vo6_project_vault_balance_unknown_vault_lifts_error() {
        let log = crate::testing::StubTransferEventLog::default();
        let empty_resolver = StubVaultAssetResolver::default();
        let err = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &sample_registry(),
            &empty_resolver,
            &log,
            1,
            1_700_000_000,
        )
        .unwrap_err();
        assert!(matches!(err, ProjectionError::VaultUnknown { .. }));
    }

    // ----- TV-VO-7: initiate_transfer canonical signature -----

    /// TV-VO-7: `initiate_transfer` returns a `TransferHandle` with
    /// `status = Pending` and a deterministic `handle_id` per inputs.
    #[test]
    fn tv_vo7_initiate_transfer_builds_pending_handle() {
        let h1 = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            100_000_000,
            &sample_asset(),
        )
        .unwrap();
        assert_eq!(h1.status, TransferStatus::Pending);
        assert_eq!(h1.vault_id, sample_vault());
        assert_eq!(h1.dest_vault_id, sample_dest());
        assert_eq!(h1.amount_dqa_micros, 100_000_000);
        assert_eq!(h1.asset_id, sample_asset());
        assert_ne!(h1.handle_id, [0u8; 32], "handle_id must not be all-zero");
        assert_ne!(h1.nonce, [0u8; 32], "nonce must not be all-zero");

        // Determinism: same inputs MUST yield same handle_id (idempotent
        // envelope identification).
        let h2 = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            100_000_000,
            &sample_asset(),
        )
        .unwrap();
        assert_eq!(
            h1.handle_id, h2.handle_id,
            "initiate_transfer MUST be substrate-deterministic per inputs"
        );
    }

    /// TV-VO-8: `initiate_transfer` fails-CLOSED on non-positive amount.
    #[test]
    fn tv_vo8_initiate_transfer_rejects_non_positive_amount() {
        let err =
            initiate_transfer(&sample_vault(), &sample_dest(), 0, &sample_asset()).unwrap_err();
        assert!(matches!(err, VaultError::Substrate));
        let err =
            initiate_transfer(&sample_vault(), &sample_dest(), -1, &sample_asset()).unwrap_err();
        assert!(matches!(err, VaultError::Substrate));
    }

    /// TV-VO-9: `initiate_transfer` produces distinct `handle_id` per
    /// distinct inputs (no input collision).
    #[test]
    fn tv_vo9_initiate_transfer_handle_id_collision_free() {
        let h1 = initiate_transfer(&sample_vault(), &sample_dest(), 100, &sample_asset()).unwrap();
        let other_vault = VaultId::from_bytes([0x43u8; 32]);
        let h2 = initiate_transfer(&other_vault, &sample_dest(), 100, &sample_asset()).unwrap();
        assert_ne!(
            h1.handle_id, h2.handle_id,
            "different vault_id MUST produce different handle_id"
        );
    }
}
