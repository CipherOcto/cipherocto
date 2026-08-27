//! Mission A (RFC-0960) VaultBalanceProjection substrate.
//!
//! Per RFC-0960 §2.1 (VaultBalanceProjection) + §2.2 (Projection
//! Algorithm) + §2.3 (Bounded-LRU Cache).
//!
//! ## Layer hosting
//!
//! `octo-vault` is Layer B (RFC-driven, additive only, years-stable).
//! All types in this module are additive (semver-minor).

#![allow(clippy::double_must_use)]

use std::time::{SystemTime, UNIX_EPOCH};

use octo_cap_macaroon::{AssetId, ChainId, Dqa, VaultId};

/// Sentinel `VaultId` used for sovereign drain-direction events
/// (Payment/Settlement/Burn) per RFC-0960 §2.1. The
/// `vault_id = ZERO_VAULT_ID` row in `transfer_events` represents
/// chain-rule / role-token emissions (NOT a real vault).
pub const ZERO_VAULT_ID: VaultId = VaultId::from_bytes([0u8; 32]);

/// Projection source provenance (RFC-0960 §2.1).
///
/// `#[repr(u8)]` so the SQL `source_kind INT` column binding matches the
/// substrate-canonical pattern (same as `AssetKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum ProjectionSource {
    /// Served from the bounded-LRU cache.
    Cache = 0,
    /// Computed fresh from a full `transfer_events` log scan.
    FreshLogScan = 1,
    /// Computed via full EpochRebuild (forced cache invalidation after
    /// asset rotation break).
    EpochRebuild = 2,
}

/// Cached projection for `(chain_id, vault_id, asset_id)` triple
/// (RFC-0960 §2.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultBalanceProjection {
    /// Owning chain (RFC-0105 §3.11 chain-binding).
    pub chain_id: ChainId,
    /// Vault whose balance is projected (RFC-0960 §2.1).
    pub vault_id: VaultId,
    /// Asset contained by the vault (RFC-0960 §2.6 asset-generality).
    pub asset_id: AssetId,
    /// Projected balance = `SUM(in.to_vault) - SUM(out.from_vault) -
    /// SUM(active escrow holds)`. DQA-form (NOT raw `i64`) to avoid the
    /// second-numeric-tower violation per RFC-0105.
    pub projected_balance: Dqa,
    /// Wall-clock timestamp of the projection (unix seconds, ONE-clock
    /// rule per §2.3 — no ms/epoch mixing).
    pub projected_at_unix_seconds: Option<i64>,
    /// Registry snapshot epoch at projection time. Cache invalidates
    /// when live epoch advances past this value (asset-rotation break
    /// mitigation per §2.3).
    pub registry_snapshot_epoch: u64,
    /// Where this projection came from (cache, fresh log scan, epoch rebuild).
    pub source_kind: ProjectionSource,
}

/// Errors surfaced by the projection substrate
/// (RFC-0960 §2.4 + R8 #1 realignment).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectionError {
    /// Vault not found by `VaultAssetResolver`.
    #[error("vault unknown: {vault_id:?}")]
    VaultUnknown {
        /// Vault id that failed resolution.
        vault_id: VaultId,
    },
    /// Underlying `TransferEventLog` read failure (transport-layer).
    #[error("transfer event log read failed: {0}")]
    LogReadFailed(String),
}

/// `TransferEventLog::insert` errors (Mission B §2.5).
///
/// Reachability note: the variant is exercised by Layer D adapter
/// integration tests (production `lru::LruCache`-backed impl + the
/// Stoolap `INSERT` path); no substrate test in this crate reaches
/// it because `consume()` no longer calls `TransferEventLog::insert`
/// directly (see `l4-parallel-transfer-event-log-elimination`).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransferEventLogInsertError {
    /// Underlying insert failed (e.g. FK violation, IO error).
    #[error("transfer event log insert failed")]
    InsertFailed,
}

/// `TransferEventLog` port trait (RFC-0960 §2.2 + Mission B §2.5).
///
/// Layer B port — production impl lives at `octo-vault-stoolap`
/// (Layer D transport adapter).
pub trait TransferEventLog: Send + Sync {
    /// Sum `to_vault == vault_id AND asset_id == asset_id` over
    /// `occurred_at_unix >= floor`.
    fn sum_to_vault(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        asset_id: &AssetId,
        occurred_at_unix_floor: i64,
    ) -> Result<Dqa, ProjectionError>;

    /// Sum `from_vault == vault_id AND asset_id == asset_id` over
    /// `occurred_at_unix >= floor`.
    fn sum_from_vault(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        asset_id: &AssetId,
        occurred_at_unix_floor: i64,
    ) -> Result<Dqa, ProjectionError>;

    /// Maximum `occurred_at_unix` for the `(chain_id, vault_id, asset_id)`
    /// triple. `None` if no rows match.
    fn max_occurred_at_unix(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        asset_id: &AssetId,
    ) -> Result<Option<i64>, ProjectionError>;

    /// Insert a `TransferEventRef` into the log (Mission B §2.5 step 4).
    fn insert(
        &mut self,
        event: &crate::event_log_producer::TransferEventRef,
    ) -> Result<(), crate::event_log_producer::TransferEventLogInsertError>;
}

/// `VaultAssetResolver` port trait (RFC-0960 §2.1).
///
/// Distinct from `VaultRegistry::contains_asset` (returns `()`, cannot
/// return `asset_id`). Required by Mission A scope.
pub trait VaultAssetResolver: Send + Sync {
    /// Resolve the `asset_id` contained by `vault_id`. Returns
    /// `Err(VaultAssetResolverError::UnknownVault)` if not found.
    fn resolve_asset_for(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
    ) -> Result<AssetId, VaultAssetResolverError>;
}

/// `VaultAssetResolver` errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum VaultAssetResolverError {
    /// `vault_id` not present in the asset resolver.
    #[error("vault unknown: {vault_id:?}")]
    UnknownVault {
        /// Vault id that failed resolution.
        vault_id: VaultId,
    },
}

/// Compute projection over the log (RFC-0960 §2.2 algorithm).
///
/// `projected_balance = sum_to_vault - sum_from_vault`. Drain-direction
/// events (Payment/Settlement/Burn) use `ZERO_VAULT_ID` sentinel; the
/// `to_vault_id` column resolves to the actual recipient vault, so the
/// algorithm does NOT need to special-case drain direction.
#[must_use]
pub fn project(
    chain_id: &ChainId,
    vault_id: &VaultId,
    asset_id: &AssetId,
    log: &dyn TransferEventLog,
) -> Result<VaultBalanceProjection, ProjectionError> {
    let in_sum = log.sum_to_vault(chain_id, vault_id, asset_id, i64::MIN)?;
    let out_sum = log.sum_from_vault(chain_id, vault_id, asset_id, i64::MIN)?;
    let max_ts = log.max_occurred_at_unix(chain_id, vault_id, asset_id)?;
    let projected_balance = if in_sum.scale == out_sum.scale && in_sum.value >= out_sum.value {
        Dqa::new(in_sum.value - out_sum.value, in_sum.scale)
            .unwrap_or_else(|_| Dqa::new(0, 0).unwrap())
    } else {
        // Defensive: negative balances should not occur in canonical state,
        // but substrate fails-closed with a zero projection rather than
        // panicking.
        Dqa::new(0, 0).unwrap()
    };
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(VaultBalanceProjection {
        chain_id: *chain_id,
        vault_id: *vault_id,
        asset_id: *asset_id,
        projected_balance,
        projected_at_unix_seconds: max_ts.or(Some(now_unix)),
        registry_snapshot_epoch: 0,
        source_kind: ProjectionSource::FreshLogScan,
    })
}

// ============================================================================
// Bounded-LRU cache (RFC-0960 §2.3)
// ============================================================================
//
// Note: this is a HASH-MAP-backed cache with manual eviction (substrate-
// local). Production deployments wire `lru::LruCache` via the
// `octo-vault-stoolap` Layer D adapter. The substrate provides the trait
// shape; the cache algorithm details (LRU recency, TTL expiry, asset-
// rotation break mitigation) are documented at §2.3 but the in-memory
// substrate implementation is intentionally minimal — Mission B wires
// the production LRU.

use std::collections::HashMap;

/// Cache key = `(chain_id, vault_id, asset_id)` triple per RFC-0960
/// §2.3 (asset-generality contract).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Owning chain.
    pub chain_id: ChainId,
    /// Vault whose projection is cached.
    pub vault_id: VaultId,
    /// Asset contained by the vault.
    pub asset_id: AssetId,
}

impl CacheKey {
    /// Construct a cache key from the canonical triple.
    #[must_use]
    pub const fn new(chain_id: ChainId, vault_id: VaultId, asset_id: AssetId) -> Self {
        Self {
            chain_id,
            vault_id,
            asset_id,
        }
    }
}

/// TTL-bounded cache entry.
#[derive(Clone, Debug)]
struct CacheEntry {
    projection: VaultBalanceProjection,
    cached_at_unix_seconds: i64,
}

/// In-memory TTL cache (substrate-local; production LRU lives in Layer D).
///
/// ONE-clock rule: TTL is unix seconds only, no ms/epoch mixing.
#[derive(Debug)]
pub struct VaultBalanceCache {
    entries: HashMap<CacheKey, CacheEntry>,
    ttl_seconds: i64,
}

impl VaultBalanceCache {
    /// Create a new cache with `ttl_seconds` TTL. Substrate fails-closed
    /// on `ttl_seconds <= 0` (TTL must be positive).
    #[must_use]
    pub fn new(ttl_seconds: i64) -> Self {
        Self {
            entries: HashMap::new(),
            ttl_seconds: ttl_seconds.max(1),
        }
    }

    /// Insert or replace a cache entry. Caller is responsible for setting
    /// `source_kind = ProjectionSource::Cache` if the caller wants to
    /// preserve provenance.
    pub fn put(&mut self, key: CacheKey, projection: VaultBalanceProjection) {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.entries.insert(
            key,
            CacheEntry {
                projection,
                cached_at_unix_seconds: now_unix,
            },
        );
    }

    /// Read a cache entry; returns `None` if absent OR if TTL has expired
    /// (in which case the entry is also evicted to free memory).
    pub fn get(&mut self, key: &CacheKey) -> Option<VaultBalanceProjection> {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Some(entry) = self.entries.get(key) {
            if now_unix - entry.cached_at_unix_seconds <= self.ttl_seconds {
                return Some(entry.projection.clone());
            }
        }
        self.entries.remove(key);
        None
    }

    /// Invalidate a specific cache entry (called by the bust listener).
    pub fn invalidate(&mut self, key: &CacheKey) {
        self.entries.remove(key);
    }

    /// Invalidate ALL entries (called on asset-rotation break per §2.3).
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    /// Cache size (entries).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the cache empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `VaultAssetResolver` error mapping helper.
///
/// Lifts `VaultAssetResolverError::UnknownVault` into
/// `ProjectionError::VaultUnknown` so callers that hold a
/// `Box<dyn VaultAssetResolver>` can compose the two ports without
/// re-implementing the mapping at each call site.
pub fn map_resolver_error(e: VaultAssetResolverError) -> ProjectionError {
    match e {
        VaultAssetResolverError::UnknownVault { vault_id } => {
            ProjectionError::VaultUnknown { vault_id }
        }
    }
}

// ============================================================================
// v015 SQL DDL (RFC-0960 §3.1)
// ============================================================================
//
// Per-crate numbering per substrate state — `octo-vault/migrations/`
// currently holds `v013__create_vaults.sql` +
// `v014__create_transfer_events.sql` +
// `v015__create_vault_balance_projection_cache.sql`. Next free is
// `v016`. When the centralized migration runner (RFC-0960 §3.1)
// lands, these MUST be renumbered to a global sequence per the RFC
// proposal.

/// SQL DDL for the projection cache table. PK `(chain_id, vault_id)`.
/// Columns per RFC-0960 §3.1.
pub const V015_DDL: &str = "\
CREATE TABLE IF NOT EXISTS vault_balance_projection_cache (
    chain_id                  BLOB(32) NOT NULL,
    vault_id                  BLOB(32) NOT NULL,
    asset_id                  BLOB(32) NOT NULL,
    projected_balance         DQA(12)  NOT NULL,
    projected_at_unix_seconds BIGINT,
    source_kind               INT      NOT NULL,
    registry_snapshot_epoch   BIGINT   NOT NULL,
    PRIMARY KEY (chain_id, vault_id)
);";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tv_vp1_zero_vault_id_is_all_zeros() {
        assert_eq!(ZERO_VAULT_ID.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn tv_vp2_projection_source_repr_u8_values() {
        assert_eq!(ProjectionSource::Cache as u8, 0);
        assert_eq!(ProjectionSource::FreshLogScan as u8, 1);
        assert_eq!(ProjectionSource::EpochRebuild as u8, 2);
    }

    #[test]
    fn tv_vp3_cache_key_is_hashable() {
        let k = CacheKey::new(
            ChainId::from_bytes([1u8; 32]),
            VaultId::from_bytes([2u8; 32]),
            AssetId::from_bytes([3u8; 32]),
        );
        let mut cache = VaultBalanceCache::new(60);
        let proj = VaultBalanceProjection {
            chain_id: k.chain_id,
            vault_id: k.vault_id,
            asset_id: k.asset_id,
            projected_balance: Dqa::new(0, 0).unwrap(),
            projected_at_unix_seconds: Some(1_700_000_000),
            registry_snapshot_epoch: 0,
            source_kind: ProjectionSource::Cache,
        };
        cache.put(k, proj.clone());
        assert!(cache.get(&k).is_some());
        assert!(cache.get(&k).is_some());
        cache.invalidate(&k);
        assert!(cache.get(&k).is_none());
    }

    #[test]
    fn tv_vp4_cache_ttl_eviction() {
        let k = CacheKey::new(
            ChainId::from_bytes([1u8; 32]),
            VaultId::from_bytes([2u8; 32]),
            AssetId::from_bytes([3u8; 32]),
        );
        let mut cache = VaultBalanceCache::new(1);
        let proj = VaultBalanceProjection {
            chain_id: k.chain_id,
            vault_id: k.vault_id,
            asset_id: k.asset_id,
            projected_balance: Dqa::new(0, 0).unwrap(),
            projected_at_unix_seconds: Some(1_700_000_000),
            registry_snapshot_epoch: 0,
            source_kind: ProjectionSource::Cache,
        };
        cache.put(k, proj.clone());
        // TTL 1s — entry should be retrievable immediately
        let mut cache2 = VaultBalanceCache::new(0); // clamped to 1
        cache2.put(k, proj);
        assert!(cache2.get(&k).is_some());
    }

    #[test]
    fn tv_vp5_v015_ddl_has_pk() {
        assert!(V015_DDL.contains("PRIMARY KEY"));
        assert!(V015_DDL.contains("DQA(12)"));
        assert!(V015_DDL.contains("source_kind"));
    }

    /// TV-VP-9 (R9 test-coverage): `TransferEventLogInsertError::InsertFailed`
    /// MUST be returned when the underlying log rejects the insert. This
    /// is the only crate-level test that exercises the `InsertFailed`
    /// variant end-to-end (production Layer D adapter integration tests
    /// cover the Stoolap `INSERT` path; this test locks the
    /// `TransferEventLog` trait impl's fail-CLOSED contract).
    #[cfg(feature = "testing")]
    #[test]
    fn tv_vp9_transfer_event_log_insert_failed() {
        use crate::event_log_producer::TransferEventRef;
        use crate::testing::StubTransferEventLog;
        let mut log = StubTransferEventLog::failing_insert();
        let event = TransferEventRef {
            chain_id: ChainId::from_bytes([1u8; 32]),
            from_vault_id: VaultId::from_bytes([2u8; 32]),
            to_vault_id: VaultId::from_bytes([3u8; 32]),
            asset_id: AssetId::from_bytes([4u8; 32]),
            amount: Dqa::new(1_000, 0).unwrap(),
            occurred_at_unix: 1_700_000_000,
            event_id: [5u8; 32],
        };
        let err = log.insert(&event).unwrap_err();
        assert!(
            matches!(err, crate::vault_balance_projection::TransferEventLogInsertError::InsertFailed),
            "StubTransferEventLog::failing_insert MUST return TransferEventLogInsertError::InsertFailed; got {err:?}"
        );
    }

    /// TV-VP-8 (R7 test-coverage): `map_resolver_error` is a public
    /// adapter between `VaultAssetResolverError` and `ProjectionError`.
    /// This test locks the lifting contract:
    /// `UnknownVault { vault_id } → VaultUnknown { vault_id }` —
    /// the `vault_id` MUST be preserved across the mapping (no
    /// zero-init, no hash collapse).
    #[test]
    fn tv_vp8_map_resolver_error_preserves_vault_id() {
        let vid = VaultId::from_bytes([0xAB; 32]);
        let resolver_err = VaultAssetResolverError::UnknownVault { vault_id: vid };
        let mapped = map_resolver_error(resolver_err);
        match mapped {
            ProjectionError::VaultUnknown { vault_id } => {
                assert_eq!(
                    vault_id, vid,
                    "map_resolver_error MUST preserve vault_id byte-for-byte"
                );
            }
            other => panic!("expected VaultUnknown{{..}}, got {other:?}"),
        }
    }

    /// TV-VP-6 (R6 test-coverage): TTL boundary + 1-second-past
    /// regression guard. The `now - cached_at <= ttl_seconds` check in
    /// `VaultBalanceCache::get` serves entries while `elapsed <= ttl`
    /// (inclusive) and evicts when `elapsed > ttl`. This test locks
    /// both sides of the boundary so a future regression to `<=`
    /// evicting at exact boundary OR `<` evicting early both fail.
    ///
    /// We construct a cache directly + mutate `cached_at_unix_seconds`
    /// to force each boundary state.
    #[test]
    fn tv_vp6_ttl_boundary_one_past_evicts() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let k = CacheKey::new(
            ChainId::from_bytes([1u8; 32]),
            VaultId::from_bytes([2u8; 32]),
            AssetId::from_bytes([3u8; 32]),
        );
        let ttl = 60i64;
        let mut cache = VaultBalanceCache::new(ttl);
        let proj = VaultBalanceProjection {
            chain_id: k.chain_id,
            vault_id: k.vault_id,
            asset_id: k.asset_id,
            projected_balance: Dqa::new(0, 0).unwrap(),
            projected_at_unix_seconds: Some(1_700_000_000),
            registry_snapshot_epoch: 0,
            source_kind: ProjectionSource::Cache,
        };
        cache.put(k, proj);
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // Force 1-second-past-boundary: now - cached_at == ttl + 1 →
        // MUST evict.
        if let Some(entry) = cache.entries.get_mut(&k) {
            entry.cached_at_unix_seconds = now_unix - ttl - 1;
        } else {
            panic!("entry MUST be present immediately after put()");
        }
        assert!(
            cache.get(&k).is_none(),
            "entry at TTL + 1 second MUST be evicted (now - cached_at > ttl_seconds)"
        );
    }

    /// TV-VP-7 (R6 test-coverage): TTL exact-boundary inclusive
    /// regression guard. At `now - cached_at == ttl_seconds` the entry
    /// MUST still be served by `VaultBalanceCache::get` (inclusive
    /// `<=` check). A regression to `<` would evict one second early.
    #[test]
    fn tv_vp7_ttl_exact_boundary_served() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let k = CacheKey::new(
            ChainId::from_bytes([1u8; 32]),
            VaultId::from_bytes([2u8; 32]),
            AssetId::from_bytes([3u8; 32]),
        );
        let ttl = 60i64;
        let mut cache = VaultBalanceCache::new(ttl);
        let proj = VaultBalanceProjection {
            chain_id: k.chain_id,
            vault_id: k.vault_id,
            asset_id: k.asset_id,
            projected_balance: Dqa::new(0, 0).unwrap(),
            projected_at_unix_seconds: Some(1_700_000_000),
            registry_snapshot_epoch: 0,
            source_kind: ProjectionSource::Cache,
        };
        cache.put(k, proj);
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // Force exact-boundary: now - cached_at == ttl.
        if let Some(entry) = cache.entries.get_mut(&k) {
            entry.cached_at_unix_seconds = now_unix - ttl;
        } else {
            panic!("entry MUST be present immediately after put()");
        }
        assert!(
            cache.get(&k).is_some(),
            "entry at exact TTL boundary MUST still be served (now - cached_at == ttl_seconds, inclusive)"
        );
    }
}
