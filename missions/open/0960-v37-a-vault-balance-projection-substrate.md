# 0960-v37-a-vault-balance-projection-substrate — substrate + cache + v015 migration

**Status:** Open
**Substrate:** RFC-0960 §2 Specification (VaultBalanceProjection + bounded-LRU cache + VaultAssetResolver)
**Parent:** RFC-0960 §6 Implementation Path Mission A
**Depends on:**

- RFC-0960
- Mission D (`0105-v35-asset-registry-nonce-registry-substrate.md`) — provides `AssetRegistry`, `AssetError`, `AssetKind`, `AssetMetadata`, `MAX_SCALE`, `NonceRegistry`, `NonceError`, `newtypes::{Nonce, Epoch, GovernanceSignature}`, `BridgeChainNamespace`, `sovereign_nonce_namespace`, `verify_governance_signature`, `blake3_hash` canonical substrate imports used by VaultAssetResolver trait boundary.
- Mission E (`0965-v21-payment-caveat-asset-binding-substrate.md`) — `PaymentCaveat.asset_id` consumed by `PaymentEventProducer` (Mission B) for tri-invariant check; Mission A substrate shape must NOT preclude the consumer contract.

## Scope

Land the substrate half of RFC-0960 v3.7: introduce `VaultBalanceProjection`

- projection algorithm + bounded-LRU cache + `v015` SQL DDL + `VaultAssetResolver`
  port + `StoolapTransferEventLog` port impl. Mission B wires the `EventLogProducer`
  trait; Mission C handles legacy deletion. Mission A owns everything else
  substrate-side.

### Mission A sub-steps

1. **`VaultBalanceProjection` struct** — `crates/octo-vault/src/vault_balance_projection.rs`
   (NEW). Per RFC-0960 §2.1:

   ```rust
   pub struct VaultBalanceProjection {
       pub chain_id: ChainId,
       pub vault_id: VaultId,
       pub asset_id: AssetId,
       pub projected_balance: Dqa,
       pub projected_at_unix_seconds: Option<i64>,
       pub registry_snapshot_epoch: u64,
       pub source_kind: ProjectionSource,  // #[repr(u8)] Cache=0/FreshLogScan=1/EpochRebuild=2
   }

   pub const ZERO_VAULT_ID: VaultId = VaultId([0u8; 32]);

   #[repr(u8)]
   pub enum ProjectionSource { Cache = 0, FreshLogScan = 1, EpochRebuild = 2 }
   ```

2. **Projection algorithm** — `crates/octo-vault/src/vault_balance_projection.rs`.
   `sum_to_vault(vault_id, asset_id, occurred_at_unix_floor) -> Dqa` +
   `sum_from_vault(...)` over the `TransferEventLog`. Uses `max_occurred_at_unix`
   (not `last_chain_seq` — column does not exist on v014). Drain-direction events
   (Payment/Settlement/Burn) use `ZERO_VAULT_ID` sentinel per §2.1.

3. **Bounded-LRU cache** — same file. `VaultBalanceCache::new(NonZeroUsize,
Duration)` (capacity + TTL); `get_or_compute(...)` checks live
   `registry_snapshot_epoch` and invalidates if live epoch advances past
   snapshot (asset-rotation break mitigation per §2.3). ONE-clock rule:
   TTL = unix seconds only, no ms/epoch mixing.

4. **`TransferEventLog` port trait** — `crates/octo-vault/src/event_log_producer.rs`
   (NEW shared with Mission B). Methods: `sum_to_vault`/`sum_from_vault`/
   `max_occurred_at_unix`/`insert`. Production impl lands at
   `crates/octo-vault-stoolap/src/transfer_event_log.rs` (Layer D
   transport adapter crate, NOT `crates/octo-vault` Layer B — port
   trait in B, adapter in D per [[cipherocto-design-principles]]).

5. **`VaultAssetResolver` port trait** — NEW per RFC-0960 §2.1. Distinct from
   `VaultRegistry::contains_asset` (returns `()`, cannot return asset_id).
   Production impl lands at `crates/octo-vault-stoolap/src/vault_asset_resolver.rs`
   (Layer D adapter crate) using the existing `vaults` PK
   `(chain_id, owner_did, asset_id)` + UNIQUE INDEX on `vault_id`.

6. **v015 SQL DDL migration** — `crates/octo-vault/migrations/v015__create_vault_balance_projection_cache.sql`
   (NEW). **Per-crate numbering** (current substrate state: each crate
   owns its own counter; `crates/octo-vault/migrations/` has v013+v014
   → next free is v015). PK `(chain_id, vault_id)`. Columns:
   `projected_balance DQA(12)`, `projected_at_unix_seconds BIGINT`,
   `source_kind INT NOT NULL`, `registry_snapshot_epoch BIGINT NOT NULL`.

   **Note on RFC vs substrate:** RFC-0960 v3.7 §3.1 L748-752 claims
   "centralized migration runner in `octo-storage-core` uses GLOBAL
   numbering across crates" and recommends `v017`. On-disk substrate
   state at landing time is per-crate (verified via `ls
crates/*/migrations/`): `octo-reputation` has v001-v012,
   `quota-router-sm-engine` has 000-006, `octo-vault` has v013-v014,
   `quota-router-storage` has v001-v020. The RFC's centralization
   proposal is a forward-pointer requiring a separate substrate
   migration (not in scope for Mission A). Mission A lands at per-crate
   v015; when the centralization lands, this migration MUST be renumbered
   to the RFC-proposed global v017.

   **Mission A AC additions:**
   - Verify `v015` is unclaimed in `crates/octo-vault/migrations/` via
     grep BEFORE committing. If claimed, bump to next free per-crate
     number (next = v015 → v016 → v017...).
   - `cargo test --workspace` migration-ordering test passes.

### Cargo deps (add to `crates/octo-vault/Cargo.toml`)

- `lru = "<version>"` (bounded LRU; pin at integration time)
- `parking_lot = "<version>"` (sharded mutex for cache)
- `serde = { version = "<version>", features = ["derive"] }`
- `chrono = { version = "<version>", default-features = false, features = ["clock"] }` (default clock provider per §2.3)
- `hex = "<version>"` (channel-name encoding per §2.4)

### Cargo deps (NEW `crates/octo-vault-stoolap/Cargo.toml`)

- `octo-vault = { path = "../octo-vault" }` (port trait dependencies per Layer D direction)
- `stoolap = { version = "<pin>", features = ["pubsub", "wal"] }` (Layer D substrate adapter; pin per [[feedback_stoolap_persistence]])
- `tokio = { version = "<version>", features = ["sync", "rt"] }` (pub/sub subscriber runtime)
- `serde = { version = "<version>", features = ["derive"] }`
- `hex = "<version>"` (channel-name encoding per §2.4)

## Test Vectors

- TV-VP1: empty log → `projected_balance = DQA::zero()` + `source_kind = FreshLogScan`
- TV-VP2: 3-event transfer (in/out/drain) → projection for affected vault;
  bust listener invalidates cache entry → next read = `FreshLogScan`
- TV-VP3: asset-generality — independent projection per `(chain_id, vault_id, asset_id)` triple
- TV-VP4: tri-invariant producer rejection — `validate_pre_insert` returns
  `Err(ProducerError::TriInvariantViolation)` (Mission B canonical
  error type name) BEFORE log insert; log row count unchanged after
  rejection
- TV-VP5: correction-fold ordering (RFC-0960 v3.7 §10 TV-VP5 — R8 #1
  realignment) — corrections applied in BLAKE3-hash-ascending order
  on `corrections` BLOB content
- TV-VP6: 1000-concurrent-producer race (RFC-0960 v3.7 §10 TV-VP6) —
  `drain_lock` enforces serialization (no concurrent inserts; serialized
  queue drained cleanly)
- TV-VP7: legacy `Balance` removal (RFC-0960 v3.7 §10 TV-VP7) —
  **DEFERRED to Mission C scope** (RFC §5.1 Cycle 2 deletion; Mission A
  does not own this vector)
- TV-VP8: `octo_w_balances` feature flag (RFC-0960 v3.7 §10 TV-VP8) —
  **DEFERRED to Mission C scope** (RFC §5.1 Cycle 2 gating; Mission A
  does not own this vector)
- TV-VP9: `ZERO_VAULT_ID` sentinel exclusion (RFC-0960 v3.7 §10 TV-VP9) —
  projection correctly excludes `ZERO_VAULT_ID` from `SUM(in)` and
  `SUM(out)`; the v014 `occurred_at_unix BIGINT` column has no
  `last_chain_seq` to break determinism
- TV-VP10: `VaultAssetResolver` integration (RFC-0960 v3.7 §10 TV-VP10 —
  R8 #1 realignment) — `resolve_asset_for(unknown_vault)` returns
  `Err(VaultAssetResolverError::UnknownVault)` and projection surfaces
  `ProjectionError::VaultUnknown`
- TV-VP11: `lru::get` updates recency; `lru::peek` does NOT (Mission A
  substrate-fidelity vector — RFC §10 does not enumerate this; added
  by Mission A scope as LRU-API conformance vector)
- TV-VP12: `ProjectionSource` SQL binding — variant value matches
  `source_kind INT` per `#[repr(u8)]` (NEW TV not in RFC §10 — added
  by Mission A scope as SQL-binding conformance vector; matches the
  substrate-canonical `#[repr(u8)]` pattern used elsewhere)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-vault` (Layer B) — `VaultBalanceProjection` + cache + `TransferEventLog` port + `VaultAssetResolver` port (trait definitions only)
- `octo-vault-stoolap` (Layer D transport adapter, NEW) — Stoolap-backed impl of `TransferEventLog` + `VaultAssetResolver` + `VaultProjectionInvalidationEmitter`. Per [[cipherocto-design-principles]] Layer D depends on Layer B; adapters live in their own crate. The `VaultProjectionInvalidationEmitter` is the RFC-0960 v3.7 §2.4 anchor; the substrate-convention `InvalidationBus` port trait referenced in Mission B derives from this emitter trait boundary.
- New `VaultAssetResolver` trait = Layer B-additive (does NOT modify existing `VaultRegistry` contract)
- No cross-layer inversion; vault substrate code stays in `crates/octo-vault/`
- Semver impact: octo-vault = semver-minor (additive ports); octo-vault-stoolap = new crate (semver-minor initial release)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
cargo test --workspace  # migration-ordering test must pass
```

## Backward compat

- No breaking changes to existing `octo-vault` public API (Mission A is additive)
- Vault substrate existing types unchanged: `Vault`, `VaultId`, `VaultRegistry` contracts preserved
- `Vault.balance_dqa_micros: i64` field kept (deprecation handled by Mission C)
- `crates/octo-vault` Cargo.toml deps added; no removals
- NEW `crates/octo-vault-stoolap` crate added (Layer D transport adapter)

## Cross-references

- RFC-0960 §2 Specification — substrate spec
- RFC-0960 §2.1 — types + `ZERO_VAULT_ID` + `VaultAssetResolver`
- RFC-0960 §2.2 — projection algorithm
- RFC-0960 §2.3 — bounded-LRU cache + asset-rotation mitigation
- RFC-0960 §6 Mission A — canonical scope
- RFC-0960 v3.7 (text)
- [[cipherocto-design-principles]] — Layer B additive-only rule
- RFC-0105 v3.5 §3.13 L669 — **audit-batch replay enforcement** (NEW
  v3.5-r6): per-tuple fresh pairwise check; Mission A's substrate
  shape (per-event `validate_pre_insert`) does NOT preclude the
  downstream batch-replay fresh-check requirement; Mission A does NOT
  cache pairwise results.
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — depends on Mission A trait landing
- Mission C (`0960-v37-c-legacy-balance-deprecation.md`) — depends on Mission A substrate landing

## Claimant

@unassigned

## Pull Request

#
