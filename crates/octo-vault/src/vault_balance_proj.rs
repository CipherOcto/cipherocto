//! `project_vault_balance` — canonical 7-param SUM projection
//! (RFC-0960-v37 §2.2).
//!
//! Split from `vault_operations.rs` per Wave 1.5 hygiene callout.
//!
//! The substrate-local bounded-LRU+TTL cache is owned here so the
//! lib-test reset helper can reference the SAME `OnceLock` instance
//! that production code initializes.

use std::sync::{Mutex, MutexGuard, OnceLock};

use octo_cap_macaroon::{AssetRegistry, ChainId, VaultId};

use crate::vault_balance_projection::{
    map_resolver_error, CacheKey, ProjectionError, ProjectionSource, TransferEventLog,
    VaultAssetResolver, VaultBalanceCache, VaultBalanceProjection,
};

/// Module-level static for the substrate-local bounded-LRU+TTL cache
/// (per RFC-0960-v37 §2.3). ONE instance per process; production Layer D
/// adapter may override with an `lru::LruCache`-backed impl at config-time
/// injection. The substrate-local default is a `Mutex<HashMap>` with TTL
/// eviction; the `current_unix_seconds` parameter supplied to
/// [`project_vault_balance`] is authoritative for the cache TTL check
/// (the substrate NEVER calls `SystemTime::now()` itself — this locks
/// determinism per RFC-0008 Class A).
///
/// Module-level (rather than function-local) so the lib-test reset
/// helper `reset_substrate_cache_for_test` can reference the SAME
/// `OnceLock` instance that production code initializes. Two separate
/// `OnceLock` statics (one in the function body, one in tests) would
/// not coexist — `reset_substrate_cache_for_test` would silently be a
/// no-op while production held the populated cache.
static SUBSTRATE_CACHE: OnceLock<Mutex<VaultBalanceCache>> = OnceLock::new();

fn substrate_local_cache() -> &'static Mutex<VaultBalanceCache> {
    SUBSTRATE_CACHE.get_or_init(|| Mutex::new(VaultBalanceCache::new(60)))
}

/// Process-global test-serialization mutex. Held by
/// [`SubstrateCacheBypassGuard`] for the duration of any test that
/// exercises the substrate cache. Two reasons this exists (per
/// Wave 2.5 fix 4):
///
/// - The previous `reset_substrate_cache_for_test()` design cleared
///   the cache then RETURNED — leaving a window where a parallel
///   cargo-test thread could repopulate the cache between the clear
///   and the test's first `project_vault_balance` call. That race
///   surfaced as `tv_vlt5_cache_hit_returns_cache_source_kind` flaking
///   intermittently (~2 of 5 runs in CI) when `tv_vlt6_*` ran first
///   in the parallel-test schedule.
/// - The `last_seen_sequence: DashMap<OverlayIdentity, u64>` inside
///   `ProducerTrustList` (see `cache_subscriber.rs`) is per-trust-list
///   and so can't leak between tests that construct their own TL, but
///   the substrate cache IS global and CAN leak. The guard pattern
///   here serializes only tests that touch the cache — non-cache
///   tests still run in parallel.
static TEST_SERIAL: Mutex<()> = Mutex::new(());

/// Drop guard returned by [`reset_substrate_cache_for_test`]. Holds
/// the [`TEST_SERIAL`] mutex for the test's lifetime so a parallel
/// test cannot repopulate the cache mid-assertion. Drop happens
/// automatically when the guard goes out of scope at the end of the
/// test body, releasing the mutex for the next test.
///
/// Tests MUST bind the return value to a named binding (e.g.
/// `let _guard = octo_vault::reset_substrate_cache_for_test();`) —
/// a bare `octo_vault::reset_substrate_cache_for_test();` discards
/// the guard immediately and offers no isolation. The caller is
/// responsible for that binding discipline.
#[cfg(any(test, feature = "testing"))]
pub struct SubstrateCacheBypassGuard {
    _guard: MutexGuard<'static, ()>,
}

/// Test-only helper: reset the process-global substrate cache to a
/// fresh empty state AND acquire the [`TEST_SERIAL`] mutex for the
/// caller's lifetime. Returns a [`SubstrateCacheBypassGuard`] the
/// caller MUST bind to a local so the lock is held for the test's
/// duration. No-op when the cache hasn't been initialized yet
/// (safe to call unconditionally). Production code MUST NOT call
/// this; the helper is gated behind the `testing` feature flag to
/// keep it out of release builds.
///
/// Wave 2.5 fix 4 added the Drop-guard pattern after the bare-return
/// variant was found to race against parallel cargo-test threads
/// that repopulate the cache between `invalidate_all()` and the
/// test's first `project_vault_balance` call.
#[cfg(any(test, feature = "testing"))]
pub fn reset_substrate_cache_for_test() -> SubstrateCacheBypassGuard {
    let guard = TEST_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(cache) = SUBSTRATE_CACHE.get() {
        let mut g = cache.lock().unwrap_or_else(|p| p.into_inner());
        g.invalidate_all();
    }
    SubstrateCacheBypassGuard { _guard: guard }
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

    // Single critical section: lock once over the full read-merge-write
    // path so two concurrent callers cannot both miss-then-fresh.
    // The `Mutex` guard is held until the cache `put` returns; no
    // intermediate drop is permitted.
    let mut cache = substrate_local_cache()
        .lock()
        .unwrap_or_else(|p| p.into_inner());

    // Cache lookup. `cache.get(key, now_unix_seconds)` enforces the
    // TTL on `cached_at_unix_seconds` internally (RFC-0008 Class A —
    // substrate NEVER calls `SystemTime::now()`). The caller-side
    // `current_registry_epoch` regression gate is the only check that
    // lives outside the cache substrate; the previous outer TTL check
    // duplicated cache.get's TTL enforcement AND used the wrong field
    // (`projected_at_unix_seconds` is the last-event timestamp, not
    // the cache-fill time) so the substrate-side TTL is authoritative.
    if let Some(mut cached) = cache.get(&cache_key, current_unix_seconds) {
        if cached.registry_snapshot_epoch >= current_registry_epoch {
            cached.source_kind = ProjectionSource::Cache;
            return Ok(cached);
        }
    }

    // Cache miss / epoch regression / TTL expiry — compute fresh SUM
    // projection over the supplied log. The substrate-local
    // `project()` is a 5-param helper (RFC-0960-v37 §2.2 algorithm);
    // the 7-param surface here wraps it with the cache + epoch +
    // unix-seconds clock plumbing per RFC-0011-e §Substrate Additions.
    let fresh = crate::vault_balance_projection::project(
        chain_id,
        vault_id,
        &asset_id,
        log,
        current_unix_seconds,
    )?;

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

    cache.put(cache_key, projection.clone(), current_unix_seconds);
    Ok(projection)
}
