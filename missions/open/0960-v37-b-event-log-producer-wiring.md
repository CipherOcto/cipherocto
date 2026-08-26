# 0960-v37-b-event-log-producer-wiring — 3 EventLogProducer impls + subscriber task

**Status:** Open
**Substrate:** RFC-0960 §2.4 (invalidation bus) + §2.5 (EventLogProducer trait)
**Parent:** RFC-0960 §6 Implementation Path Mission B
**Depends on:**

- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) — `TransferEventLog` port + `VaultBalanceCache` must land first
- Mission D (`0105-v35-asset-registry-nonce-registry-substrate.md`) — canonical `AssetRegistry` / `MAX_SCALE` / `NonceRegistry` / `newtypes` imports used by producer trait boundary
- Mission E (`0965-v21-payment-caveat-asset-binding-substrate.md`) — `PaymentCaveat.asset_id` consumed by `PaymentEventProducer`
- Mission F (`0960-v36-burn-event-dqa-migration-substrate.md`) — `BurnEventRef::consume` wrapped by `BurnEventProducer`
- Mission G (`0959-v28-settlement-cost-dqa-migration-substrate.md`) — `SettlementEvent` consumed by `SettlementEventProducer`

## Scope

Wire the producer side of RFC-0960 v3.7: introduce `EventLogProducer` trait

- 3 concrete impls (Payment/Settlement/Burn) + `VaultProjectionInvalidationEmitter`
  trait + per-process subscriber task. Mission A lands the substrate ports;
  Mission B consumes them.

### Mission B sub-steps

1. **`EventLogProducer` port trait** — extend
   `crates/octo-vault/src/event_log_producer.rs` (shared with Mission A).

   ```rust
   pub trait EventLogProducer: Send + Sync {
       type Input;

       fn drain_lock(&self) -> &Arc<Mutex<()>>;

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
           // Default body (RFC-0960 v3.7 §2.5 L493-516):
           //   1. acquire self.drain_lock()
           //   2. self.validate_pre_insert(&input, registry, asset_resolver)
           //   3. let ev = self.to_transfer_event(input, registry, asset_resolver, nonce_registry)?;
           //   4. log.insert(&ev)
           //   5. bus.emit(VaultProjectionInvalidationEnvelope { ... })
           //   6. Ok(ev)
           // Subclasses overriding produce MUST re-call validate_pre_insert
           // before any state mutation (clippy lint enforces).
       }
   }
   ```

   Per RFC-0960 v3.7 §2.5 L474-516: trait uses associated type `Input`,
   `drain_lock()` accessor, 4-param `validate_pre_insert`, 5-param
   `to_transfer_event` (input + registry + asset_resolver + nonce_registry)
   returning `TransferEventRef`, and 7-param `produce` (excluding self)
   default body returning `TransferEventRef` that bundles
   drain-lock + validate + transform + insert + bus-emit.
   `drain_lock: Arc<Mutex<()>>` shared across all 3 impls enforces serial
   access to `TransferEventLog::insert`. Atomicity guarantee per §2.4.

2. **`PaymentEventProducer` impl** — `crates/octo-wallet-node/src/handlers/mint.rs`.
   Wired into the `MintHandler` struct's payment-issuance code path. Reads
   `PaymentCaveat.asset_id` per RFC-0965 §2.1. Mission B TV includes grep
   verification that the wire site exists at landing time.

3. **`SettlementEventProducer` impl** — same file
   (`crates/octo-vault/src/event_log_producer.rs`). Wraps
   `SettlementEvent::new` (struct per RFC-0959 v2.8 §2.1, lands at
   `crates/quota-router-sm-engine/src/settlement_event.rs` via Mission G)
   ATOMICALLY: the wrap site must be inside the existing settlement-event
   txn boundary so `validate_pre_insert` + `log.insert` + commit-coupled
   NOTIFY all fire within the same Stoolap transaction (RFC-0913 consumer
   pattern). **Note:** Mission G/RFC-0959 v2.8 substrate has no
   `consume()` function (nonce observation is bundled into `new()` Gate 7
   per Mission G §2.2 L130); producer wrap site calls `new()` not
   `consume()`.
   **Layer note:** the existing `SettlementEventRepository::insert`
   substrate crate remains at `crates/quota-router-storage/src/
settlement_event_repo.rs` (Layer C); the NEW `SettlementEvent` struct
   lands at `crates/quota-router-sm-engine/src/settlement_event.rs`
   (Layer C). The producer's `to_transfer_event` consumes the NEW
   struct (output of `SettlementEvent::new`) and feeds the EXISTING
   repository via the internal sink boundary.

4. **`BurnEventProducer` impl** — same file. Wraps `BurnEventRef::consume`
   (struct per RFC-0960 §2 BurnEventRef Specification). Wire site is AFTER
   nonce observation + audit-sink write, BEFORE the burn-event record is
   marked consumed.

5. **`VaultProjectionInvalidationEmitter` trait** — same file. Emits
   `VaultProjectionInvalidationEnvelope { chain_id, vault_id, source_kind }`
   over the new `cache:projection:<hex(vault_id)>` channel (RFC-0913
   consumer pattern; channel naming convention NEW per RFC-0960 §2.4).
   Wired to the producer default `produce` body so every successful insert
   fires the emit AFTER commit.

6. **Per-process subscriber task** — `crates/octo-vault/src/cache_subscriber.rs`
   (NEW). Spawns at `octo-vault` process init. Subscribes to wildcard
   `cache:projection:*` over the `VaultProjectionInvalidationEmitter` port
   trait (Layer B boundary; concrete Stoolap pub/sub impl at
   `crates/octo-vault-stoolap/` Layer D adapter crate). On envelope
   receipt: deserialize → call `VaultBalanceCache::invalidate`. The
   subscriber MUST start before any producer fan-in (verified by
   Mission B TV covering process-startup ordering).

7. **Drain lock wiring** — `drain_lock: Arc<Mutex<()>>` lives at
   `crates/octo-vault/src/lib.rs` (process-wide singleton). Each producer
   impl receives `Arc::clone(&drain_lock)` at construction.

### Mission B AC additions

- All 3 wire sites grep-verified to exist at landing time
- Per-process subscriber task spawns at `octo-vault` init, BEFORE first
  producer call (test: subscriber-init-before-producer-call)
- Wire sites use the default `produce` body OR explicitly re-call
  `validate_pre_insert` if overriding (grep + clippy lint)

## Test Vectors

- TV-VP11: `PaymentEventProducer` — happy-path mint issuance → log row
  inserted + `cache:projection:<vault>` envelope emitted; subscriber
  invalidates cache entry
- TV-VP12: `SettlementEventProducer` — wrapped `SettlementEventRepository::insert`
  fires insert + envelope atomically within one Stoolap txn (verify via
  Stoolap txn log inspection)
- TV-VP13: `BurnEventProducer` — `BurnEventRef::consume` produces log
  insert with `ZERO_VAULT_ID` sentinel + envelope after nonce observation
- TV-VP14: 1000-concurrent-producer race — `drain_lock` enforces serial
  queue; no concurrent `log.insert` calls observable
- TV-VP15: tri-invariant violation — `validate_pre_insert` returns
  `Err(ProducerError::TriInvariantViolation)`; producer's `produce`
  short-circuits BEFORE `log.insert`; log row count unchanged
- TV-VP15a: `produce()` returns `TransferEventRef` (NOT `()`) on success;
  caller has the inserted event reference for downstream consumers
- TV-VP16: subscriber running before producer — process init order
  enforces subscriber task alive before any `produce` call
- TV-VP17: subscriber wildcard `cache:projection:*` matches all per-vault
  channels; envelope deserialization round-trip preserves all fields
- TV-VP18: `produce` override that skips `validate_pre_insert` is REJECTED
  by clippy lint or compile-time bound
- TV-VP19: subscriber envelope handler calls `VaultBalanceCache::invalidate`
  correctly for `source_kind = Cache` (no-op) and `FreshLogScan`/`EpochRebuild`
  (drops entry)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-vault` (Layer B) — `EventLogProducer` trait + 3 impls + `VaultProjectionInvalidationEmitter`
- `octo-wallet-node` (Layer C) — `MintHandler` wire site
- `quota-router-storage` (Layer C) — `SettlementEventRepository::insert` wire site
- Producer trait = Layer B-additive; impls = Layer C-specialized
- Invalidation bus = Layer B-internal via `VaultProjectionInvalidationEmitter`
  port trait (vault-internal pub/sub, NOT user-facing)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
bash scripts/validate_cites.sh  # Mission B edits must not break §-cite validation
```

## Backward compat

- `EventLogProducer` trait is NEW (additive)
- `TransferEventLog::insert` signature unchanged (Mission B wraps, does not modify)
- `SettlementEventRepository::insert` signature unchanged (wrap is at call site)
- `BurnEventRef::consume` signature unchanged (wrap is at call site)
- `MintHandler` payment-issuance code path gains a producer call BEFORE the
  existing payment record write; existing behavior preserved when producer
  returns `Ok(())`

## Cross-references

- RFC-0960 §2.4 — invalidation bus + `cache:projection:<hex(vault_id)>` channel
- RFC-0960 §2.5 — `EventLogProducer` trait + 3 concrete impls
- RFC-0960 §6 Mission B — canonical scope
- RFC-0913 — pub/sub wildcard + commit-coupled NOTIFY pattern
- RFC-0965 §2.1 — `PaymentCaveat.asset_id` consumed by `PaymentEventProducer`
- RFC-0960 v3.6 §2.1 — `BurnEventRef::consume` wrapped by `BurnEventProducer`
- RFC-0959 v2.8 §2.1 — `SettlementEvent::new` (no separate `consume()` in
  substrate) wrapped by `SettlementEventProducer`
- RFC-0105 v3.5 §3.13 L669 — **audit-batch replay enforcement** (NEW
  v3.5-r6): per-tuple fresh pairwise check on every
  `(PaymentCaveat, BurnEventRef, SettlementEvent)` in the audit-batch
  replay path; per-event validate() cache MUST NOT be used. Mission B's
  producer emit path is per-event (validate_pre_insert) and does NOT
  cover the batch-replay path.
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) — block: substrate ports must land first
- Mission D (`0105-v35-asset-registry-nonce-registry-substrate.md`) — canonical substrate imports
- Mission E (`0965-v21-payment-caveat-asset-binding-substrate.md`) — `PaymentCaveat` for `PaymentEventProducer`
- Mission F (`0960-v36-burn-event-dqa-migration-substrate.md`) — `BurnEventRef` for `BurnEventProducer`
- Mission G (`0959-v28-settlement-cost-dqa-migration-substrate.md`) — `SettlementEvent` for `SettlementEventProducer`
- Mission C (`0960-v37-c-legacy-balance-deprecation.md`) — sequence: C follows B

## Claimant

@unassigned

## Pull Request

#
