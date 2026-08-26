---
rfc: 0960-v3.7
title: Vault Balance Projection Substrate (Event-Sourced Balance Computation)
status: Accepted
version: 3.7
date: 2026-08-26
amends: 0960
builds_on:
  - rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md
  - rfcs/accepted/economics/0105-v35-payment-caveat-asset-generality.md
  - rfcs/accepted/economics/0965-v21-payment-caveat-asset-binding.md
  - rfcs/accepted/economics/0960-v36-burn-event-dqa-migration.md
  - rfcs/accepted/economics/0959-v28-settlement-cost-dqa-migration.md
  - rfcs/accepted/economics/0913-stoolap-pubsub-cache-invalidation.md
  - rfcs/accepted/economics/0963-resource-shard-routing.md
  - rfcs/accepted/economics/0102-wallet-cryptography.md
---

# RFC-0960 v3.7 — Vault Balance Projection Substrate

## 0. Status

**Accepted (3.7, 2026-08-26).** Amendment to RFC-0960. Additive within grand-design RFC-0960 (Vaults, Capabilities, Reservations). DOC-ONLY amendment — substrate implementations land via follow-on missions per §6.

**Promotion trail:** R1 5-lens sweep 2026-08-26 produced 79 findings (13 CRIT + 24 HIGH + 31 MED + 7 LOW + 2 MINOR). R2 fix-all over-claimed (8 regressions). R3 fix-all reconciled to v014 substrate reality (ZERO_VAULT_ID sentinel + max_occurred_at_unix + VaultAssetResolver trait); R3 verification surfaced 4 NEW (chrono/hex Cargo dep gaps, computed_at_unix_ms ONE-clock violation, ProjectionSource #[repr(u8)]); R3 NEW fix-all closed. R4 surfaced 2 minor (chrono+hex §6 dep list, VH r3a word count); R4 fix-all closed. **R5=0 DRY achieved (3 consecutive zero-finding rounds R3=0 R4=0 R5=0, exceeds 2-round loop-until-DRY requirement).** Per BLUEPRINT.md RFC process: 7-day minimum + 2 maintainer approvals per the closure audit at `docs/audits/rfc-0960-v37-dr...-2026-08-26.md`.

**Promotion trail:** R1 sweep produced 13 CRIT + 24 HIGH + 31 MED + 7 LOW + 2 MINOR findings (substrate-impossibility cluster + conventions cluster). R2 fix-all over-claimed (introduced 8 new substrate-mismatch regressions). R3 fix-all reconciles to v014 substrate reality: `TransferEventRef` aligns with `v014__create_transfer_events.sql` NOT NULL columns via `ZERO_VAULT_ID` sentinel; `last_chain_seq` collapses to `max_occurred_at_unix` per v014 unix-time substrate; `vault_registry.asset_for` becomes a NEW `VaultAssetResolver` trait (existing `VaultRegistry::contains_asset` does not return asset_id).

**Substrate anchor:** the event-log substrate for vault balance projection is PRESENT but UNWIRED. `v014__create_transfer_events.sql` defines the canonical append-only event log (PK `(chain_id, event_id)`, columns `from_vault_id BLOB(32) NOT NULL`, `to_vault_id BLOB(32) NOT NULL`, `amount DQA(12)`, `occurred_at_unix BIGINT NOT NULL`, `corrections BLOB` Datomic-style, etc.). The v014 schema is INVARIANT — this RFC reconciles the spec to it, not the other way around. `crates/octo-vault/src/lib.rs` `Vault.balance_dqa_micros: i64` is STRANDED (zero production write sites). `crates/quota-router-core/src/balance.rs` `Balance { amount: u64 }` is pre-Dqa, OCTO-W-only, API-key-keyed — superseded by this RFC.

**Cross-substrate anchors (canonical homes):**

- `AssetRegistry` lives at RFC-0105 §3.1
- `NonceRegistry` lives at RFC-0105 §3.11
- `verify_governance_signature` + `blake3_hash` live at RFC-0105 §3.12
- `Bounded LRU cache + registry-snapshot epoch` lives at RFC-0105 §3.5
- `Tri-invariant (chain_id, vault_id, asset_id) namespace` lives at RFC-0105 §3.13
- `VaultRegistry::contains_asset` + `VaultRegistryError` live at RFC-0959 §2.1 (substrate trait)
- `PaymentCaveat` substrate lives at RFC-0965 §2.1
- `SettlementEvent` substrate lives at RFC-0959 §2.1
- `BurnEventRef` substrate lives at RFC-0960 §2 BurnEventRef Specification
- `Dqa` lives at `determin/src/dqa.rs`; `DqaEncoding` wire form lives at RFC-0862 §Substrate types (16-byte BE: `value: i64`, `scale: u8`, `_reserved: [u8; 7]`)

**Dqa API (verified at `determin/src/dqa.rs`):** `new`, `add(self, Self) -> Result<Self, DqaError>`, `subtract(self, Self) -> Result<Self, DqaError>`, `compare`, `from_f64`, `to_f64`. The `subtract` method IS fallible — underflow returns `Err(DqaError::Underflow)`.

**New port added by this RFC:** `VaultAssetResolver` (see §2.1). The existing `VaultRegistry::contains_asset` returns `Result<(), VaultRegistryError>` and CANNOT return the resolved `asset_id`. This RFC adds `VaultAssetResolver::resolve_asset_for(vault_id) -> Result<AssetId, _>` as a SEPARATE trait, with production impl landing in Mission A via the existing `vaults` PK `(chain_id, owner_did, asset_id)` + UNIQUE INDEX on `vault_id`.

**Channel naming convention (NEW — introduced by this RFC, not inherited from RFC-0913):** invalidation channel is `cache:projection:<hex(vault_id)>`. RFC-0913 itself uses FLAT channels (`cache:invalidate`, `key:revoke`, `txn:commit`); this RFC introduces per-vault channels as a new convention for projection-bust specificity. Subscriber wildcard `cache:projection:*` follows RFC-0913's wildcard pattern.

**Layer classification:** substrate code lands in `crates/octo-vault/` (Layer B vault substrate, self-declared in `Cargo.toml`). No cross-layer dependency inversion.

## 1. Motivation

### 1.1 The legacy `Balance` gap

`crates/quota-router-core/src/balance.rs` declares `pub struct Balance { pub amount: u64 }` — a pre-Dqa, OCTO-W-only, API-key-keyed balance carrier. RFC-0960 grand-design §5 Event-Sourced Ledger establishes the principle:

> "Event-sourced ledger | All state is SUM(events) projection; no mutable balance rows as source of truth"

§2.5 Transfer elaborates:

> "Balance = SUM(in) - SUM(out) - SUM(active escrow holds) over `transfer_events`. Materialised as a cached projection (the existing `octo_w_balances` table). The cache is a cache — the source is the log."

§5 Event-Sourced Ledger documents the failure mode that emerges when the cache pretends to be the source:

> "The Phase 1 finding (`saturating_sub` on `Balance::deduct`) is the bug you get when the cache pretends to be the source."

The substrate has drifted from the principle: there is no SUM projection over `transfer_events`, no cache invalidation hook on event insert, no event-log producer wired into the existing `SettlementEventRepository::insert` path.

### 1.2 User-stated intent (verbatim)

Per the user's goal for this RFC cycle: **"Balance should be vault based, computed from events, not stored directly, in memory cache is Ok."**

This amendment codifies that intent as substrate specification:

- **Vault-based:** projection PK is `(chain_id, vault_id)` per RFC-0960 grand-design §2.6 Vault Substrate (NOT the legacy `key_id TEXT` API-key shape). `asset_id` is derived from `vault_id` via the new `VaultAssetResolver` trait.
- **Computed from events:** the projection is a SUM projection over `transfer_events` filtered by `(chain_id, vault_id)`.
- **Not stored directly:** the `Vault.balance_dqa_micros: i64` field is REMOVED; the `octo_w_balances` table is RETIRED behind a feature flag.
- **In-memory cache OK:** the projection cache is allowed (bounded LRU + unix-seconds TTL), but it is a cache — the source of truth is the event log.

### 1.3 RFC-0960 grand-design §5 principle (binding requirement)

The principle at §5 Event-Sourced Ledger is RESTATED as a binding requirement of this RFC: **the canonical state for any vault balance is the SUM projection over `transfer_events`, and any cached balance value is a derivative cache invalidated by event-log inserts**. Direct mutation of balance values outside the event-log producer path is a substrate-mandated invariant violation.

### 1.4 Audit-trio cross-alignment

The audit-trio (RFC-0105 §3.13 tri-invariant) establishes that `PaymentCaveat`, `BurnEventRef`, and `SettlementEvent` operate on a shared `(chain_id, vault_id, asset_id)` namespace. This RFC introduces the projection substrate that consumes all three event types as inputs to the SUM projection. The projection PK `(chain_id, vault_id)` is a STRICT SUBSET of the tri-invariant namespace; `asset_id` is resolved at cache-write time via `VaultAssetResolver::resolve_asset_for` and bound into the cache entry.

### 1.5 Scope reduction — escrow term

The §2.5 Transfer formula's third term `SUM(active escrow holds)` has no substrate (`crates/` has no `EscrowHoldRegistry`, no `escrow_holds` table, no DDL in any migration). This amendment SCOPES OUT this term and projects only `SUM(in.to_vault) - SUM(out.from_vault)`. The escrow term is deferred to a follow-on RFC once the `escrow_holds` substrate is specced and landed. Callers needing the escrow term must use the existing `Escrow { amount_micro_octo_w: Dqa, state }` substrate at `crates/quota-router-core/src/marketplace/escrow.rs` `Escrow` struct directly until follow-on lands.

## 2. Specification (NEW, greenfield)

### 2.1 Types

```rust
// crates/octo-vault/src/vault_balance_projection.rs (NEW file — greenfield substrate)

use octo_determin::Dqa;
use octo_vault::{AssetId, ChainId, VaultId};

/// Cached projection of a vault balance over `transfer_events` (RFC-0960 §5 Event-Sourced Ledger).
///
/// Projection PK is `(chain_id, vault_id)`; `asset_id` is derived from `vault_id` via
/// `VaultAssetResolver::resolve_asset_for`. The cache value is a derivative — the source of
/// truth is the event log itself; this struct is invalidatable by
/// `VaultProjectionInvalidationEnvelope` (§2.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultBalanceProjection {
    pub chain_id: ChainId,
    pub vault_id: VaultId,
    /// Resolved from `vault_id` at cache-write time via `VaultAssetResolver`. NOT a filter
    /// predicate on the SUM.
    pub asset_id: AssetId,
    /// Projected balance = SUM(in.to_vault) - SUM(out.from_vault) for the (chain, vault).
    /// Escrow term deferred (see §1.5). Dqa — the canonical numeric substrate.
    pub projected_balance: Dqa,
    /// `occurred_at_unix` of the last `transfer_events` row included in this projection.
    /// `None` = empty log (no rows yet). Uses unix seconds (i64-compatible via v014 `BIGINT`).
    pub projected_at_unix_seconds: Option<i64>,
    /// RFC-0105 §3.5 registry-snapshot epoch captured at projection time. Cache invalidation
    /// triggers when live `registry_snapshot_epoch` advances past this value (asset rotation).
    pub registry_snapshot_epoch: u64,
    pub source_kind: ProjectionSource,
}

/// Stable `#[repr(u8)]` mapping contract (binds the SQL `source_kind INT` column):
/// `Cache = 0`, `FreshLogScan = 1`, `EpochRebuild = 2`. New variants MUST append
/// (never reorder) and MUST reserve their discriminant in this contract.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ProjectionSource {
    /// Cache hit on a non-stale entry. TTL = `(current_unix_seconds - projected_at_unix_seconds) < ttl_seconds`.
    Cache,
    /// Fresh SUM projection just computed over `transfer_events` (cold path).
    FreshLogScan,
    /// Full rebuild after a `VaultProjectionInvalidationEnvelope` invalidated an entry.
    EpochRebuild,
}

/// Sentinel `VaultId` representing "no vault in this direction". v014 schema has
/// `from_vault_id` and `to_vault_id` both `NOT NULL`, so producers MUST set one of them
/// to this sentinel for drain-direction events (Payment/Settlement/Burn, where there is
/// no receiving vault).
///
/// Convention: `ZERO_VAULT_ID = [0u8; 32]`. The projection SUM filters this out:
/// - SUM(in) WHERE `to_vault_id = vault_id AND to_vault_id != ZERO_VAULT_ID`
/// - SUM(out) WHERE `from_vault_id = vault_id AND from_vault_id != ZERO_VAULT_ID`
pub const ZERO_VAULT_ID: VaultId = VaultId([0u8; 32]);

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("asset rotated: cache snapshot {snapshot}, live {live}")]
    AssetRotated { snapshot: u64, live: u64 },
    #[error("vault unknown: {vault_id:?}")]
    VaultUnknown { vault_id: VaultId },
    #[error("vault {vault_id:?} does not hold asset {asset_id:?}")]
    VaultAssetMismatch { vault_id: VaultId, asset_id: AssetId },
    #[error("balance underflow: sum_in={sum_in}, sum_out={sum_out}")]
    BalanceUnderflow { sum_in: Dqa, sum_out: Dqa },
    #[error("tri-invariant violation: {detail}")]
    TriInvariantViolation { detail: &'static str },
    #[error("transfer_events query failed: {source}")]
    LogQueryFailed {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// NEW port introduced by this RFC. The existing `VaultRegistry::contains_asset` returns
/// `Result<(), VaultRegistryError>` and CANNOT return the resolved `asset_id`. This trait
/// is the asset-resolution path; production impl lands in Mission A via the `vaults`
/// table PK `(chain_id, owner_did, asset_id)` + UNIQUE INDEX on `vault_id`.
pub trait VaultAssetResolver {
    fn resolve_asset_for(&self, vault_id: &VaultId) -> Result<AssetId, VaultAssetResolverError>;
}

#[derive(Debug, thiserror::Error)]
pub enum VaultAssetResolverError {
    #[error("vault unknown: {0:?}")]
    UnknownVault(VaultId),
    #[error("storage fault: {0}")]
    StorageFault(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

### 2.2 Projection algorithm

```rust
// crates/octo-vault/src/vault_balance_projection.rs (continued)

use crate::{AssetRegistry, NonceRegistry};  // RFC-0105 §3.1 + §3.11

/// Compute the SUM projection for `(chain_id, vault_id)` over `transfer_events`.
/// Cold-path projection; the cache hit path is `VaultBalanceCache::get_or_compute`.
///
/// Algorithm (RFC-0960 §2.5 Transfer formula, escrow term deferred per §1.5):
///   balance = SUM(amount WHERE to_vault_id = vault_id AND to_vault_id != ZERO_VAULT_ID)
///           - SUM(amount WHERE from_vault_id = vault_id AND from_vault_id != ZERO_VAULT_ID)
///
/// Asset-key derivation: `vault_id → asset_id` via `VaultAssetResolver::resolve_asset_for`
/// (NOT a SUM filter predicate — resolved at projection time and bound into cache entry).
///
/// Correction-fold ordering (RFC-0960 §5 Datomic-style): each row with non-null
/// `corrections` (pointing to a corrected prior event) excludes the corrected event's
/// amount from the SUM. Corrections are applied in BLAKE3-hash-ascending order on the
/// `corrections` BLOB content — deterministically sorted by canonical BLOB bytes.
pub fn project_vault_balance(
    chain_id: &ChainId,
    vault_id: &VaultId,
    registry: &dyn AssetRegistry,
    asset_resolver: &dyn VaultAssetResolver,
    log: &impl TransferEventLog,
    current_registry_epoch: u64,
    current_unix_seconds: i64,
) -> Result<VaultBalanceProjection, ProjectionError> {
    // 1. Resolve asset_id from vault_id via VaultAssetResolver.
    let asset_id = asset_resolver.resolve_asset_for(vault_id)
        .map_err(|e| match e {
            VaultAssetResolverError::UnknownVault(vid) => ProjectionError::VaultUnknown { vault_id: vid },
            VaultAssetResolverError::StorageFault(_) => ProjectionError::LogQueryFailed {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, "vault resolver fault")),
            },
        })?;

    // 2. Validate scale-resolution invariant: asset `wire_scale` matches projection substrate scale.
    let meta = registry.metadata(&asset_id)
        .map_err(|_| ProjectionError::AssetRotated { snapshot: 0, live: current_registry_epoch })?;

    // 3. SUM(in) — events with to_vault_id = vault_id (excluding ZERO_VAULT_ID).
    let sum_in = log.sum_to_vault(chain_id, vault_id)
        .map_err(|e| ProjectionError::LogQueryFailed { source: Box::new(e) })?;

    // 4. SUM(out) — events with from_vault_id = vault_id (excluding ZERO_VAULT_ID).
    let sum_out = log.sum_from_vault(chain_id, vault_id)
        .map_err(|e| ProjectionError::LogQueryFailed { source: Box::new(e) })?;

    // 5. Apply Dqa arithmetic (real API: `subtract` IS fallible, returns Result).
    let projected_balance = sum_in.subtract(sum_out)
        .map_err(|_| ProjectionError::BalanceUnderflow { sum_in, sum_out })?;

    // 6. Last occurred_at_unix anchor (v014 schema field; i64 unix seconds).
    let projected_at_unix_seconds = log.max_occurred_at_unix(chain_id, vault_id)
        .map_err(|e| ProjectionError::LogQueryFailed { source: Box::new(e) })?;

    Ok(VaultBalanceProjection {
        chain_id: chain_id.clone(),
        vault_id: vault_id.clone(),
        asset_id,
        projected_balance,
        projected_at_unix_seconds,
        registry_snapshot_epoch: current_registry_epoch,
        source_kind: ProjectionSource::FreshLogScan,
    })
}

/// Substrate trait for the `transfer_events` query + write surface.
/// Production impl lands in Mission A at `crates/octo-vault/src/transfer_event_log/stoolap.rs`
/// (the `StoolapTransferEventLog` struct implementing `TransferEventLog`).
pub trait TransferEventLog {
    /// SUM(amount) WHERE chain_id = ? AND to_vault_id = ? AND to_vault_id != ZERO_VAULT_ID.
    fn sum_to_vault(&self, chain_id: &ChainId, vault_id: &VaultId)
        -> Result<Dqa, TransferEventLogError>;
    /// SUM(amount) WHERE chain_id = ? AND from_vault_id = ? AND from_vault_id != ZERO_VAULT_ID.
    fn sum_from_vault(&self, chain_id: &ChainId, vault_id: &VaultId)
        -> Result<Dqa, TransferEventLogError>;
    /// MAX(occurred_at_unix) WHERE chain_id = ? AND (to_vault_id = ? OR from_vault_id = ?).
    fn max_occurred_at_unix(&self, chain_id: &ChainId, vault_id: &VaultId)
        -> Result<Option<i64>, TransferEventLogError>;
    /// Atomically insert a row into `transfer_events`. Called by `EventLogProducer::produce`.
    /// Per RFC-0913 commit-coupled NOTIFY, the envelope is emitted AFTER this returns Ok.
    fn insert(&self, ev: &TransferEventRef) -> Result<(), TransferEventLogError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TransferEventLogError {
    #[error("query failed: {0}")]
    QueryFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("insert failed: {0}")]
    InsertFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("constraint violation: {detail}")]
    ConstraintViolation { detail: &'static str },
}
```

### 2.3 Cache topology — bounded LRU + unix-seconds TTL

```rust
// crates/octo-vault/src/vault_balance_projection.rs (continued)

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use parking_lot::RwLock;

/// Bounded-LRU cache + unix-seconds TTL per RFC-0105 §3.5 GREENFIELD topology.
///
/// Cache PK = `(chain_id, vault_id)` pair (matches v014 `transfer_events` filter shape).
/// RFC-0963 §Routing key selection aligns: a single-shard balance read maps 1:1 onto
/// a single cache lookup.
///
/// Cache TTL = `(current_unix_seconds - projected_at_unix_seconds) < ttl_seconds`. ONE clock
/// (unix seconds) — no ms/seconds/epoch mixing. `VaultProjectionInvalidationEnvelope`
/// (§2.4) forces immediate invalidation regardless of TTL.
///
/// Cargo deps added by Mission A (octo-vault/Cargo.toml):
/// - `lru = { version = "0.12", features = ["std"] }`
/// - `parking_lot = "<version>"` (version pinned at Mission A integration time)
/// - `serde = { version = "1", features = ["derive"] }`
/// - `chrono = { version = "<version>", default-features = false, features = ["clock"] }` (default clock provider)
/// - `hex = "<version>"` (vault_id → channel-name encoding)
pub struct VaultBalanceCache {
    inner: Arc<RwLock<LruCache<ProjectionKey, VaultBalanceProjection>>>,
    current_unix_seconds: Arc<dyn Fn() -> i64 + Send + Sync>,  // unix-seconds provider
    ttl_seconds: i64,  // default 2; settable per deployment
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectionKey {
    pub chain_id: ChainId,
    pub vault_id: VaultId,
}

impl VaultBalanceCache {
    pub fn new(capacity: NonZeroUsize, ttl_seconds: i64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(LruCache::new(capacity))),
            current_unix_seconds: Arc::new(|| chrono::Utc::now().timestamp()),
            ttl_seconds,
        }
    }

    /// Read path: cache hit if entry exists AND TTL is fresh AND registry epoch matches.
    /// Uses `get` (NOT `peek`) so LRU recency is correctly updated.
    pub fn get_or_compute(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        log: &impl TransferEventLog,
        current_registry_epoch: u64,
    ) -> Result<VaultBalanceProjection, ProjectionError> {
        let key = ProjectionKey { chain_id: chain_id.clone(), vault_id: vault_id.clone() };
        let now = (self.current_unix_seconds)();

        // Cache hit path.
        if let Some(cached) = self.inner.write().get(&key).cloned() {
            let ttl_fresh = match cached.projected_at_unix_seconds {
                Some(prev) => now.saturating_sub(prev) < self.ttl_seconds,
                None => false,  // no anchor = stale
            };
            let registry_fresh = cached.registry_snapshot_epoch >= current_registry_epoch;
            if ttl_fresh && registry_fresh {
                return Ok(VaultBalanceProjection {
                    source_kind: ProjectionSource::Cache,
                    ..cached
                });
            }
        }

        // Cold path.
        let mut projection = project_vault_balance(
            chain_id, vault_id, registry, asset_resolver, log,
            current_registry_epoch, now,
        )?;
        projection.source_kind = ProjectionSource::EpochRebuild;
        self.inner.write().put(key, projection.clone());
        Ok(projection)
    }

    /// Invalidation hook called by subscriber on `VaultProjectionInvalidationEnvelope`.
    /// Stale envelope (older `triggered_at_unix_seconds` than cached entry) = no-op.
    pub fn invalidate(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
        triggered_at_unix_seconds: i64,
    ) {
        let key = ProjectionKey { chain_id: chain_id.clone(), vault_id: vault_id.clone() };
        let mut cache = self.inner.write();
        if let Some(existing) = cache.peek(&key) {
            if let Some(prev) = existing.projected_at_unix_seconds {
                if triggered_at_unix_seconds <= prev {
                    return;
                }
            }
        }
        cache.pop(&key);
    }

    /// Test-only: pin a cached entry with a known value.
    #[cfg(any(test, feature = "test-util"))]
    pub fn pin_for_test(&self, projection: VaultBalanceProjection) {
        let key = ProjectionKey { chain_id: projection.chain_id.clone(), vault_id: projection.vault_id.clone() };
        self.inner.write().put(key, projection);
    }
}
```

**Cache sizing note:** `NonZeroUsize` per `lru 0.12` API. Default `capacity = 100_000` entries. `LruCache::get` updates recency (correct LRU semantics). Asset-generality flows through `vault_id → asset_id` derivation; cache size scales with vault count, not asset×vault count.

### 2.4 Invalidation bus — `VaultProjectionInvalidationEnvelope` over RFC-0913

```rust
// crates/octo-vault/src/event_log_producer.rs (NEW file — greenfield substrate)

use octo_vault::{AssetId, ChainId, VaultId};
use serde::{Deserialize, Serialize};

/// Invalidation bus payload — RFC-0913 Stoolap NOTIFY/SUBSCRIBE consumer.
///
/// When a producer inserts a row into `transfer_events`, it MUST emit this envelope
/// over the per-vault channel. Subscribers consume and call
/// `VaultBalanceCache::invalidate(chain_id, vault_id, triggered_at_unix_seconds)`.
///
/// Per RFC-0913, NOTIFY is commit-coupled: subscribers observe the envelope AFTER the
/// underlying INSERT commits (no subscribers-before-producers race).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultProjectionInvalidationEnvelope {
    pub chain_id: ChainId,
    pub vault_id: VaultId,
    pub asset_id: AssetId,
    /// Unix seconds of the row that triggered the invalidation. Stale envelope
    /// (triggered_at_unix_seconds <= cached entry's projected_at_unix_seconds) is a no-op.
    pub triggered_at_unix_seconds: i64,
}

/// Per-vault channel name. INTRODUCED by this RFC; RFC-0913's own channels are flat
/// (`cache:invalidate`, `key:revoke`, `txn:commit`). Subscribers wildcard `cache:projection:*`
/// per RFC-0913 wildcard pattern.
pub fn projection_channel(vault_id: &VaultId) -> String {
    format!("cache:projection:{}", hex::encode(vault_id.0))
}

pub trait VaultProjectionInvalidationEmitter {
    /// Emit an envelope on the per-vault channel. MUST be commit-coupled to the
    /// `TransferEventLog::insert` that produced it.
    fn emit(&self, envelope: &VaultProjectionInvalidationEnvelope)
        -> Result<(), InvalidationEmitError>;
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidationEmitError {
    #[error("pubsub emit failed: {0}")]
    PubSubFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("channel unavailable: {0}")]
    ChannelUnavailable(&'static str),
}
```

**Subscriber wiring:** cache subscriber runs as a long-lived background task per process. It opens `CREATE SUBSCRIPTION projection_bust_sub ON 'cache:projection:*'` (RFC-0913 wildcard) and, on each NOTIFY, parses the envelope and calls `VaultBalanceCache::invalidate(...)`.

**Atomicity guarantee:** every producer path holds the SAME per-process `drain_lock` + RFC-0913 commit-coupled NOTIFY. Default `produce` body (see §2.5):

1. Acquire `drain_lock: Arc<Mutex<()>>`
2. `validate_pre_insert(input)` (tri-invariant check)
3. `log.insert(ev)` (Stoolap transaction; commit-coupled to NOTIFY)
4. `bus.emit(&envelope)` (RFC-0913 NOTIFY)
5. Release `drain_lock`

Per-process lock + commit-coupled NOTIFY = no cross-thread or cross-process races.

### 2.5 `EventLogProducer` trait + three concrete impls

```rust
// crates/octo-vault/src/event_log_producer.rs (continued)

use octo_determin::Dqa;
use crate::{AssetId, ChainId, VaultId, ZERO_VAULT_ID};
use std::sync::{Arc, Mutex};

pub trait EventLogProducer {
    type Input;
    fn drain_lock(&self) -> &Arc<Mutex<()>>;
    /// Tri-invariant validation hook — REQUIRED. Reject inputs that violate RFC-0105 §3.13
    /// BEFORE the `log.insert` call. Subclasses overriding `produce` MUST re-call this.
    fn validate_pre_insert(
        &self,
        input: &Self::Input,
        registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError>;
    fn to_transfer_event(
        &self,
        input: Self::Input,
        registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError>;
    /// Default `produce` body. Validates + inserts + emits under drain_lock.
    fn produce(
        &self,
        input: Self::Input,
        registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        nonce_registry: &dyn NonceRegistry,
        log: &impl TransferEventLog,
        bus: &impl VaultProjectionInvalidationEmitter,
        current_unix_seconds: i64,
    ) -> Result<TransferEventRef, ProducerError> {
        let _guard = self.drain_lock().lock().unwrap_or_else(|e| e.into_inner());
        self.validate_pre_insert(&input, registry, asset_resolver)?;
        let ev = self.to_transfer_event(input, registry, asset_resolver, nonce_registry)?;
        log.insert(&ev).map_err(ProducerError::from)?;
        let envelope = VaultProjectionInvalidationEnvelope {
            chain_id: ev.chain_id.clone(),
            vault_id: ev.vault_id.clone(),
            asset_id: ev.asset_id.clone(),
            triggered_at_unix_seconds: current_unix_seconds,
        };
        bus.emit(&envelope).map_err(ProducerError::from)?;
        Ok(ev)
    }
}

/// Canonical row written to `transfer_events`. v014 schema: BOTH `from_vault_id` and
/// `to_vault_id` are NOT NULL, so producers use `ZERO_VAULT_ID` for the no-vault direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferEventRef {
    pub chain_id: ChainId,
    pub from_vault_id: VaultId,   // NOT Option; ZERO_VAULT_ID for drain-direction events
    pub to_vault_id: VaultId,     // NOT Option; ZERO_VAULT_ID for drain-direction events
    pub asset_id: AssetId,
    pub amount: Dqa,
    pub occurred_at_unix: i64,    // v014 BIGINT column
    pub corrections: Option<Vec<[u8; 32]>>,
}

// ============================================================================
// PaymentEventProducer — wraps PaymentCaveat (RFC-0965 §2.1)
// ============================================================================
pub struct PaymentEventProducer {
    pub drain_lock: Arc<Mutex<()>>,
}

impl EventLogProducer for PaymentEventProducer {
    type Input = PaymentProducerInput;
    fn drain_lock(&self) -> &Arc<Mutex<()>> { &self.drain_lock }
    fn validate_pre_insert(
        &self,
        input: &Self::Input,
        registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError> {
        let _ = registry.metadata(&input.caveat_asset_id)
            .map_err(|_| ProducerError::TriInvariantViolation { detail: "unknown caveat asset" })?;
        Ok(())
    }
    fn to_transfer_event(
        &self,
        input: Self::Input,
        _registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        _nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError> {
        let asset_id = asset_resolver.resolve_asset_for(&input.vault_id)
            .map_err(|_| ProducerError::TriInvariantViolation { detail: "vault asset mismatch" })?;
        Ok(TransferEventRef {
            chain_id: input.chain_id,
            from_vault_id: input.vault_id,
            to_vault_id: ZERO_VAULT_ID,  // payment is a drain
            asset_id,
            amount: input.amount,
            occurred_at_unix: input.occurred_at_unix,
            corrections: None,
        })
    }
}

pub struct PaymentProducerInput {
    pub chain_id: ChainId,
    pub vault_id: VaultId,
    pub caveat_asset_id: AssetId,
    pub amount: Dqa,
    pub occurred_at_unix: i64,
}

// ============================================================================
// SettlementEventProducer — wraps SettlementEvent (RFC-0959 §2.1)
// ============================================================================
pub struct SettlementEventProducer {
    pub drain_lock: Arc<Mutex<()>>,
}

impl EventLogProducer for SettlementEventProducer {
    type Input = SettlementProducerInput;
    fn drain_lock(&self) -> &Arc<Mutex<()>> { &self.drain_lock }
    fn validate_pre_insert(
        &self,
        input: &Self::Input,
        registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError> {
        let _ = registry.metadata(&input.cost_asset_id)
            .map_err(|_| ProducerError::TriInvariantViolation { detail: "unknown cost asset" })?;
        Ok(())
    }
    fn to_transfer_event(
        &self,
        input: Self::Input,
        _registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        _nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError> {
        let asset_id = asset_resolver.resolve_asset_for(&input.cost_vault_id)
            .map_err(|_| ProducerError::TriInvariantViolation { detail: "vault asset mismatch" })?;
        Ok(TransferEventRef {
            chain_id: input.chain_id,
            from_vault_id: input.cost_vault_id,
            to_vault_id: ZERO_VAULT_ID,  // settlement is a cost, not a transfer
            asset_id,
            amount: input.cost,
            occurred_at_unix: input.settled_at_unix,
            corrections: None,
        })
    }
}

pub struct SettlementProducerInput {
    pub chain_id: ChainId,
    pub cost_vault_id: VaultId,
    pub cost_asset_id: AssetId,
    pub cost: Dqa,
    pub settled_at_unix: i64,
}

// ============================================================================
// BurnEventProducer — wraps BurnEventRef (RFC-0960 §2 BurnEventRef Specification)
// ============================================================================
pub struct BurnEventProducer {
    pub drain_lock: Arc<Mutex<()>>,
}

impl EventLogProducer for BurnEventProducer {
    type Input = BurnProducerInput;
    fn drain_lock(&self) -> &Arc<Mutex<()>> { &self.drain_lock }
    fn validate_pre_insert(
        &self,
        input: &Self::Input,
        registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError> {
        let _ = registry.metadata(&input.asset_id)
            .map_err(|_| ProducerError::TriInvariantViolation { detail: "unknown asset" })?;
        Ok(())
    }
    fn to_transfer_event(
        &self,
        input: Self::Input,
        _registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        _nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError> {
        let asset_id = asset_resolver.resolve_asset_for(&input.vault_id)
            .map_err(|_| ProducerError::TriInvariantViolation { detail: "vault asset mismatch" })?;
        Ok(TransferEventRef {
            chain_id: input.chain_id,
            from_vault_id: input.vault_id,
            to_vault_id: ZERO_VAULT_ID,  // burn is a drain
            asset_id,
            amount: input.amount,
            occurred_at_unix: input.occurred_at_unix,
            corrections: None,
        })
    }
}

pub struct BurnProducerInput {
    pub chain_id: ChainId,
    pub vault_id: VaultId,
    pub asset_id: AssetId,
    pub amount: Dqa,
    pub occurred_at_unix: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ProducerError {
    #[error("tri-invariant violation: {detail}")]
    TriInvariantViolation { detail: &'static str },
    #[error("log insert failed: {0}")]
    LogInsertFailed(#[source] TransferEventLogError),
    #[error("invalidation emit failed: {0}")]
    InvalidationEmitFailed(#[source] InvalidationEmitError),
    #[error("vault/asset resolution failed: {detail}")]
    VaultAssetResolution { detail: &'static str },
}
```

**Wiring sites (Mission B scope, verified by grep at landing):**

- `PaymentEventProducer` → wired into the `MintHandler` struct's payment-issuance code path (located at `crates/octo-wallet-node/src/handlers/mint.rs`). Mission B TV includes a grep verification that the wire site exists at landing time.
- `SettlementEventProducer` → wired into `SettlementEventRepository::insert` ATOMICALLY (struct at `crates/quota-router-storage/src/settlement_event_repo.rs`, `insert` method on the `SettlementEventRepository` impl block). Mission B TV verifies.
- `BurnEventProducer` → wired into `BurnEventRef::consume` AFTER nonce observation + audit-sink write (struct per RFC-0960 §2 BurnEventRef Specification). Mission B TV verifies.

### 2.6 Asset-generality contract

The projection PK is `(chain_id, vault_id)`, NEVER `(key_id)`. Asset-generality flows through `vault_id → VaultAssetResolver::resolve_asset_for`.

The legacy API-key-keyed `Balance { amount: u64 }` at `crates/quota-router-core/src/balance.rs` `Balance` struct + the `octo_w_balances` table at `crates/quota-router-core/src/schema.rs` `octo_w_balances` `CREATE TABLE` block are SUPERSEDED by this RFC.

Conformance with RFC-0105 §3.13 tri-invariant: the projection PK shape `(chain_id, vault_id)` is a STRICT SUBSET of the tri-invariant namespace `(chain_id, vault_id, asset_id)`. A producer emitting a `TransferEventRef` with `asset_id` not equal to `asset_resolver.resolve_asset_for(vault_id)` MUST be rejected with `ProducerError::TriInvariantViolation` BEFORE the `log.insert` call.

### 2.7 Error-scenario matrix

| Variant                                      | Trigger                                                                       | Mitigation                             |
| -------------------------------------------- | ----------------------------------------------------------------------------- | -------------------------------------- |
| `ProjectionError::AssetRotated`              | `cached.registry_snapshot_epoch < live.registry_snapshot_epoch`               | Cache miss → fresh projection          |
| `ProjectionError::VaultUnknown`              | `VaultAssetResolver` returns `UnknownVault`                                   | Caller must register vault first       |
| `ProjectionError::VaultAssetMismatch`        | `VaultAssetResolver` returns `UnknownVault` for derived asset lookup          | Caller must verify vault↔asset binding |
| `ProjectionError::BalanceUnderflow`          | `sum_in.subtract(sum_out)` returns `Err` (sum_out > sum_in)                   | Accounting fault; surface to caller    |
| `ProjectionError::TriInvariantViolation`     | Producer `validate_pre_insert` rejection                                      | Caller must fix input                  |
| `ProjectionError::LogQueryFailed`            | Stoolap query error on `sum_to_vault`/`sum_from_vault`/`max_occurred_at_unix` | Retry; surface to caller               |
| `ProducerError::TriInvariantViolation`       | Producer `validate_pre_insert` rejection                                      | Caller must fix input                  |
| `ProducerError::LogInsertFailed`             | Stoolap INSERT error on `transfer_events`                                     | Retry; surface to caller               |
| `ProducerError::InvalidationEmitFailed`      | RFC-0913 NOTIFY error after successful INSERT                                 | Retry; reconciliation job catches      |
| `ProducerError::VaultAssetResolution`        | `asset_resolver.resolve_asset_for` failed in `to_transfer_event`              | Caller must verify vault↔asset binding |
| `VaultAssetResolverError::UnknownVault`      | `vaults` table has no row for `vault_id`                                      | Caller must register vault first       |
| `VaultAssetResolverError::StorageFault`      | Stoolap query fault on `vaults` lookup                                        | Retry; surface to caller               |
| `InvalidationEmitError::PubSubFailed`        | RFC-0913 substrate NOTIFY error                                               | Retry; reconciliation job              |
| `InvalidationEmitError::ChannelUnavailable`  | RFC-0913 substrate channel not subscribed                                     | Init subsystem issue                   |
| `TransferEventLogError::QueryFailed`         | Stoolap query error                                                           | Retry; surface to caller               |
| `TransferEventLogError::InsertFailed`        | Stoolap INSERT error (constraint violation, deadlock)                         | Retry; surface to caller               |
| `TransferEventLogError::ConstraintViolation` | PK uniqueness or NOT NULL violation on `transfer_events` insert               | Caller must fix input                  |

## 3. Wire Form (NEW, greenfield)

### 3.1 `vault_balance_projection_cache` table

```sql
-- crates/octo-vault/migrations/v017__create_vault_balance_projection_cache.sql (NEW migration)

CREATE TABLE vault_balance_projection_cache (
    chain_id                  BLOB(32)    NOT NULL,
    vault_id                  BLOB(32)    NOT NULL,
    asset_id                  BLOB(32)    NOT NULL,    -- resolved from vault_id via VaultAssetResolver
    projected_balance         DQA(12)     NOT NULL,    -- 16-byte BE wire form per RFC-0862 §Substrate types
    projected_at_unix_seconds BIGINT,                  -- nullable; NULL = empty log; matches v014 occurred_at_unix column type
    registry_snapshot_epoch   BIGINT      NOT NULL,    -- RFC-0105 §3.5 anchor for asset-rotation invalidation
    source_kind               INT         NOT NULL,    -- 0=Cache, 1=FreshLogScan, 2=EpochRebuild (matches #[repr(u8)] ProjectionSource contract)
    PRIMARY KEY (chain_id, vault_id)
);

CREATE INDEX idx_vbpc_unix ON vault_balance_projection_cache(chain_id, projected_at_unix_seconds);
```

**Migration number v017 — selection rationale:**

- Existing migrations in `crates/octo-vault/migrations/`: `v013__create_vaults.sql`, `v014__create_transfer_events.sql`. Numbers `v015` and `v016` are NOT currently assigned in `octo-vault`.
- The centralized migration runner in `octo-storage-core` uses GLOBAL numbering across crates. Other crates may have migrations numbered `v015`/`v016`; Mission A landing MUST verify that `v017` is unclaimed across the entire workspace at landing time (verified by grep + `cargo test --workspace` migration test passing). If `v017` IS claimed, the next free global number is `v018` (or higher).
- Substrate-side migration runner is centralized in `octo-storage-core`; numbering is global.

**Schema rationale:**

- PK `(chain_id, vault_id)` only — `asset_id` is derived, not part of the key.
- `BIGINT` for `projected_at_unix_seconds` matches v014 `occurred_at_unix BIGINT` (both i64 unix-seconds).
- `BIGINT` for `registry_snapshot_epoch` (i64; sufficient for any realistic epoch count).
- `INT` for `source_kind` (matches `INT` precedent in v014 schema). Bound to `#[repr(u8)] ProjectionSource` per the §2.1 contract; new variants append, never reorder.
- No escrow-hold column (escrow term deferred per §1.5).
- No `computed_at_*` column — `projected_at_unix_seconds` (already present) is the single cache-write timestamp per the §2.3 ONE-clock rule. Adding a separate `computed_at_unix_ms` would violate the rule and add no information not already in `projected_at_unix_seconds`.

### 3.2 `VaultProjectionInvalidationEnvelope` wire form

```json
{
  "vault_projection_invalidation_envelope": {
    "chain_id": "<32B-hex>",
    "vault_id": "<32B-hex>",
    "asset_id": "<32B-hex>",
    "triggered_at_unix_seconds": 1724697600
  }
}
```

Per-vault channel: `cache:projection:<hex(vault_id)>`. Subscriber wildcard: `cache:projection:*` (RFC-0913 wildcard pattern).

## 4. Cross-Reference Updates

- RFC-0105 §3.1 AssetRegistry; §3.5 Bounded LRU cache + epoch pattern; §3.11 NonceRegistry; §3.12 Cryptographic Primitives; §3.13 Tri-invariant. Imports per single-source-of-truth rule.
- RFC-0965 §2.1 PaymentCaveat substrate (upstream boundary). `PaymentEventProducer` consumes `PaymentCaveat.asset_id` for tri-invariant check.
- RFC-0959 §2.1 SettlementEvent substrate (upstream boundary). `SettlementEventProducer` consumes `SettlementEvent.cost_vault_id + cost_asset_id`.
- RFC-0960 §2 BurnEventRef Specification (RFC-0960 v3.6 adds the BurnEventRef GREENFIELD substrate). `BurnEventProducer` consumes `BurnEventRef.asset_id + vault_id + amount`.
- RFC-0913 — Stoolap pub/sub cache-invalidation substrate. Channel names: flat (`cache:invalidate`, `key:revoke`, `txn:commit`); wildcard pattern. This RFC CONSUMES RFC-0913 NOTIFY/SUBSCRIBE without redefining the substrate. The per-vault `cache:projection:<hex(vault_id)>` channel is INTRODUCED by this RFC.
- RFC-0963 §Routing key selection — `vault_id` is the shard routing key; v3.7 cache PK shape `(chain_id, vault_id)` aligns with single-shard balance reads.
- RFC-0102 §Key Derivation — `vault_id` blake3 derivation provides canonical `vault_id` bytes used as the cache PK second component.
- RFC-0862 §Substrate types — DqaEncoding 16-byte BE wire form.

## 5. Backward Compatibility

### 5.1 Single Timeline

| Cycle               | `Balance` struct                                                                 | `Vault.balance_dqa_micros`              | `octo_w_balances` table                   | `legacy_octo_w` flag      | Mission           |
| ------------------- | -------------------------------------------------------------------------------- | --------------------------------------- | ----------------------------------------- | ------------------------- | ----------------- |
| 1 (3.7 acceptance)  | `#[deprecated]` on struct + all methods + `get_octo_w_balance` + `deduct_octo_w` | `#[deprecated]` on field; column kept   | Init retained; reads serve legacy callers | `deprecated` (default ON) | Mission C prep    |
| 2 (1 release later) | REMOVED from substrate                                                           | REMOVED from struct; column kept        | Init GATED behind flag                    | `off` (default OFF)       | Mission C core    |
| 3 (1 release later) | —                                                                                | Migration drops `vaults.balance` column | Init REMOVED; table dropped               | —                         | Mission C cleanup |

**External adoption risk:** RFC-0904 is **Accepted** (NOT "Final"). The 3-cycle window for `octo_w_balances` is justified by external-adoption risk — downstream consumers may have pinned the table. Mission C Cycle-3 table drop RESERVES the right to refuse if external adoption is detected (verification via 3rd-party registry at landing time).

### 5.2 Stranded-field removal — `Vault.balance_dqa_micros`

`Vault.balance_dqa_micros: i64` (`crates/octo-vault/src/lib.rs` `Vault` struct) is STRANDED (zero production write sites). Per the Cycle-1 timeline, the field receives `#[deprecated]` at 3.7 acceptance; the column itself drops at Cycle 3.

**Positive evidence:** the verify-time boundary at `crates/octo-cap-macaroon/src/vault_lookup.rs` `VaultRowSnapshot` excludes `balance` by design, so removing the field does NOT break the substrate-isolation boundary.

### 5.3 Feature flag — `legacy_octo_w` table retention

`octo_w_balances` table init at `crates/quota-router-core/src/schema.rs` `octo_w_balances` `CREATE TABLE` is gated behind `legacy_octo_w` feature flag. Per §5.1 timeline: Cycle 1 (deprecated, default ON), Cycle 2 (off, default OFF), Cycle 3 (removed).

## 6. Implementation Path (follow-on missions)

DOC-ONLY. Substrate implementation lands via three follow-on missions under `missions/open/`:

- **Mission A — VaultBalanceProjection substrate:** introduce `VaultBalanceProjection` (§2.1) + projection algorithm (§2.2) + bounded-LRU cache (§2.3). Add `v017__create_vault_balance_projection_cache.sql` migration (§3.1). Land `StoolapTransferEventLog` impl at `crates/octo-vault/src/transfer_event_log/stoolap.rs`. Land `SqliteVaultAssetResolver` impl at `crates/octo-vault/src/vault_asset_resolver/sqlite.rs`. Add `octo-vault` Cargo deps: `lru`, `parking_lot`, `serde`, `chrono` (default clock provider per §2.3), `hex` (channel-name encoding per §2.4). New files: `crates/octo-vault/src/vault_balance_projection.rs`, `crates/octo-vault/src/event_log_producer.rs`, `crates/octo-vault/migrations/v017__create_vault_balance_projection_cache.sql`. Mission A AC additions: verify `v017` migration number is unclaimed globally (grep across `crates/*/migrations/`); the verification is a `cargo test --workspace` migration-ordering test passing.
- **Mission B — EventLogProducer wiring:** introduce `EventLogProducer` trait + 3 concrete impls (§2.5) + `VaultProjectionInvalidationEmitter` trait (§2.4). Wire `SettlementEventProducer` into `SettlementEventRepository::insert` ATOMICALLY. Wire `BurnEventProducer` into `BurnEventRef::consume` AFTER nonce observation. Wire `PaymentEventProducer` into `MintHandler` payment-issuance path. Add per-process subscriber task that consumes `VaultProjectionInvalidationEnvelope` (wildcard `cache:projection:*`) and calls `VaultBalanceCache::invalidate`.
- **Mission C — Legacy deletion:** apply §5.1 timeline (Cycle 1 deprecation stub → Cycle 2 core deletion → Cycle 3 column drop). Add `#[deprecated]` attributes. Verify 5 callers of `Balance::new` migrate. Verify `KeyStorage` trait method `get_octo_w_balance` + `deduct_octo_w` migrate to `VaultBalanceProjection::get_or_compute`.

## 7. Risk Callouts

1. **Legacy `Balance { amount: u64 }` removal — 5 callers.** Mitigation per §5.1 3-cycle deprecation stub.
2. **`Vault.balance_dqa_micros` removal — 0 production callers (stranded).** Mitigation: grep-verify 0 write sites; positive evidence via `VaultRowSnapshot` boundary exclusion.
3. **`octo_w_balances` removal — external adoption risk.** Mitigation per §5.3 feature flag + 3-cycle window + reserved right to refuse Cycle-3 removal.
4. **Concurrent producer fan-in races (HIGH).** Mitigation per §2.4 unified atomicity guarantee (`drain_lock` + RFC-0913 commit-coupled NOTIFY); Mission B TV covers 1000-concurrent-producer race.
5. **Asset-rotation cache break (HIGH).** Mitigation per §2.3 `registry_snapshot_epoch` in cache entry + invalidation when live epoch advances past snapshot.
6. **Tri-invariant producer-side enforcement (HIGH).** Mitigation per §2.5 `validate_pre_insert` as REQUIRED trait method invoked by default `produce` body.
7. **Subscribers running before producers (MEDIUM).** Mitigation per RFC-0913 commit-coupled NOTIFY.
8. **Migration number collision (R3 HIGH).** `v017` selected by elimination within `octo-vault` (currently v013+v014) but must be verified globally across `crates/*/migrations/` at Mission A landing. Mitigation: Mission A AC includes global `v017`-free verification via `cargo test --workspace` migration-ordering test.
9. **Layer classification:** substrate code lives in `crates/octo-vault/` (Layer B; per `Cargo.toml` self-declaration):
   - `VaultBalanceProjection` algorithm + cache = Layer B (vault substrate business logic)
   - `EventLogProducer` trait = Layer B-additive
   - `VaultProjectionInvalidationEnvelope` + channel = Layer B (vault-internal bus)
   - `VaultAssetResolver` trait = Layer B-additive
   - Deletion of legacy `Balance` + `octo_w_balances` = Layer B-breaking (controlled break, justified by source-of-truth migration within the same crate)
     No cross-layer dependency inversion.

## 8. Naming Cleanup

| Concept                | Name                                  | Rationale              |
| ---------------------- | ------------------------------------- | ---------------------- |
| Projection value       | `VaultBalanceProjection`              | Same stem              |
| Cache container        | `VaultBalanceProjectionCache`         | Distinct registry      |
| Cache key              | `VaultBalanceProjectionKey`           | Uniform prefix         |
| Provenance enum        | `VaultBalanceProjectionSource`        | Uniform prefix         |
| Sentinel no-vault id   | `ZERO_VAULT_ID`                       | v014 NOT NULL          |
| Invalidation envelope  | `VaultProjectionInvalidationEnvelope` | RFC-0913 verb          |
| Channel name           | `cache:projection:<hex(vault_id)>`    | New per-vault          |
| Producer trait         | `EventLogProducer`                    | General producer       |
| Asset-resolution trait | `VaultAssetResolver`                  | Distinct from registry |
| Time primitive         | unix seconds (i64)                    | Matches v014 BIGINT    |

## 9. Version History

| Version    | Date       | Author                   | Note                                                                        |
| ---------- | ---------- | ------------------------ | --------------------------------------------------------------------------- |
| 3.6        | 2026-08-26 | @cipherocto + @mmacedoeu | BurnEventRef DQA migration.                                                 |
| 3.7-r1     | 2026-08-26 | @mmacedoeu               | Initial 3.7 draft. R1 5-lens sweep: 79 findings.                            |
| 3.7-r2     | 2026-08-26 | @mmacedoeu               | R2 fix-all over-claimed; introduced 8 regressions.                          |
| 3.7-r3     | 2026-08-26 | @mmacedoeu               | R3 fix-all: substrate reconciled to v014; 17 residuals closed.              |
| 3.7-r3a    | 2026-08-26 | @mmacedoeu               | chrono+hex deps; computed_at_unix_ms dropped; ProjectionSource #[repr(u8)]. |
| 3.7-accept | 2026-08-26 | @cipherocto + @mmacedoeu | DRY at R5; promoted Draft to Accepted.                                      |

## 10. Pending (concrete test vectors)

- [ ] R3 validation pass — Guard 2 §-cite validator + Prettier format.
- [ ] TV-VP1: empty log → `projected_balance = Dqa::new(0, 12)` + `projected_at_unix_seconds = None`. Harness: `crates/octo-vault/tests/tv_0960_v37_projection.rs`.
- [ ] TV-VP2: 3-event transfer `[+] 100, [-] 30, [-] 40` (DQA scale=0) for same vault → projection `30`. Invalidation listener transitions `Cache` → `EpochRebuild` on first event-insert. Harness: `crates/octo-vault/tests/tv_0960_v37_invalidation.rs`.
- [ ] TV-VP3: asset-generality — independent projection per `(chain, vault)`. Harness: `crates/octo-vault/tests/tv_0960_v37_asset_isolation.rs`.
- [ ] TV-VP4: tri-invariant producer rejection — `PaymentEventProducer.produce(input)` where `input.caveat_asset_id != asset_resolver.resolve_asset_for(input.vault_id)` returns `Err(ProducerError::TriInvariantViolation)` and does NOT touch `transfer_events`. Harness: `crates/octo-vault/tests/tv_0960_v37_tri_invariant.rs`.
- [ ] TV-VP5: correction-fold ordering — corrections applied in BLAKE3-hash-ascending order on `corrections` BLOB content. Harness: `crates/octo-vault/tests/tv_0960_v37_correction_fold.rs`.
- [ ] TV-VP6: 1000-concurrent-producer race — three producers inserting concurrently; all inserts succeed, all bust envelopes emitted, subscribers observe in commit order. Harness: `crates/octo-vault/tests/tv_0960_v37_concurrent_producers.rs`.
- [ ] TV-VP7: legacy `Balance` removal — grep `crates/` for `pub struct Balance` returns 0 matches after Mission C Cycle 2. Harness: `crates/quota-router-core/tests/tv_0960_v37_legacy_removal.rs`.
- [ ] TV-VP8: `octo_w_balances` feature flag — with `legacy_octo_w = "off"`, table NOT initialized; with `"deprecated"`, initialized. Harness: `crates/quota-router-core/tests/tv_0960_v37_feature_flag.rs`.
- [ ] TV-VP9: `ZERO_VAULT_ID` sentinel — projection correctly excludes `ZERO_VAULT_ID` from SUM(in) and SUM(out). Harness: `crates/octo-vault/tests/tv_0960_v37_sentinel.rs`.
- [ ] TV-VP10: `VaultAssetResolver` integration — `resolve_asset_for(unknown_vault)` returns `Err(VaultAssetResolverError::UnknownVault)` and projection surfaces `ProjectionError::VaultUnknown`. Harness: `crates/octo-vault/tests/tv_0960_v37_asset_resolver.rs`.
- [ ] Acceptance promotion (7-day minimum + 2 maintainer approvals per BLUEPRINT.md).

---

**End of RFC-0960 v3.7 (revision r3 — Draft 2026-08-26).**
