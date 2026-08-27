# cache-subscriber-bus-wiring — crates/octo-vault/src/cache_subscriber.rs (Mission B §6)

**Status:** claimed (2026-08-27)
**Substrate:** RFC-0960 §2.4 (invalidation bus) + RFC-0913 (Stoolap pub/sub commit-coupled NOTIFY consumer pattern)
**Parent:** RFC-0960 §6 Mission B sub-step 6 (R3 review follow-on)
**Depends on:**

- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `VaultProjectionInvalidationEmitter` trait + `VaultProjectionInvalidationEnvelope` wire form must exist first
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) — `VaultBalanceCache::invalidate` consumer target

## Motivation

Mission B §6 reserved the per-process subscriber task for a follow-on mission. The subscriber is the consumer half of the invalidation bus — without it, `bus.emit(VaultProjectionInvalidationEnvelope)` calls succeed but `VaultBalanceCache::invalidate` is never invoked, defeating the cache coherence contract.

The substrate port trait `VaultProjectionInvalidationEmitter` (defined in `crates/octo-vault/src/event_log_producer.rs`) lands at Mission B; the concrete pub/sub subscription lives at `octo-vault-stoolap` (Layer D adapter). The subscriber task itself is Layer B (octo-vault process-internal).

## Scope

Create `crates/octo-vault/src/cache_subscriber.rs` — a per-process subscriber that:
1. Subscribes to wildcard `cache:projection:*` over a `Subscriber` port trait
2. Deserializes incoming `VaultProjectionInvalidationEnvelope` bytes
3. Calls `VaultBalanceCache::invalidate(&cache_key)` on a shared cache handle
4. Spawns at `octo-vault` process init, BEFORE any producer fan-in

### Sub-steps

1. **`Subscriber` port trait** — `crates/octo-vault/src/cache_subscriber.rs` (NEW). Defines:
   ```rust
   pub trait VaultProjectionInvalidationSubscriber: Send + Sync {
       /// Blocking receive; returns `None` on channel close.
       fn recv(&self) -> Option<VaultProjectionInvalidationEnvelope>;
   }
   ```
   Production impl at `octo-vault-stoolap` Layer D (Stoolap NOTIFY/LISTEN adapter).

2. **Subscriber task spawner** — `pub fn spawn_cache_subscriber(
       cache: Arc<Mutex<VaultBalanceCache>>,
       subscriber: Arc<dyn VaultProjectionInvalidationSubscriber>,
   ) -> JoinHandle<()>`
   The task loop: `while let Some(env) = subscriber.recv() { cache.lock().unwrap_or_else(PoisonError::into_inner).invalidate_all(); }` (whole-cache invalidation per RFC-0960 §2.4; per-key invalidation reserved for Cycle 2).

3. **Process-init wiring** — `crates/octo-vault/src/lib.rs` exports a `pub fn init_cache_subscriber(...)` factory. Layer C crates (octo-wallet-node / quota-router-sm-engine / octo-policy) call this in their `main` / server bootstrap. **Note:** server bootstrap is OUT OF SCOPE for this mission — only the substrate `init_cache_subscriber` lands. Wire-up at each Layer C binary is a follow-on (tracked at `producer-wrapper-consumer-wiring.md`).

4. **Lock ordering discipline** — subscriber acquires `cache: Arc<Mutex<VaultBalanceCache>>` lock; producers acquire `drain_lock: Arc<Mutex<()>>` then `TransferEventLog::insert` (NOT the cache). Lock ordering rule: subscriber holds cache lock only, never drain_lock; producers hold drain_lock only, never cache lock. This rule is captured in `cipherocto-design-principles` §Lock-ordering cross-boundary.

5. **Wildcard subscription** — subscriber port receives envelopes for ANY `cache:projection:<hex(vault_id)>` channel. Concrete adapter at Layer D opens a single wildcard subscription rather than N per-vault subscriptions (memory-efficient at scale).

## Out of Scope

- Concrete Stoolap NOTIFY/LISTEN impl (Layer D adapter at `crates/octo-vault-stoolap/src/cache_subscriber_impl.rs` — separate mission)
- Wire-up at each Layer C binary's `main` (server bootstrap) — tracked at `producer-wrapper-consumer-wiring.md`
- Per-key invalidation (Cycle 2 — current scope is whole-cache invalidation per `VaultBalanceCache::invalidate_all`)
- Metrics/observability hooks (separate mission per RFC-0937 Prometheus substrate)

## Test Vectors

- TV-CS-1: `init_cache_subscriber` returns `JoinHandle<()>` that is `JoinHandle::is_finished() == false` immediately after spawn
- TV-CS-2: Mock subscriber returning 3 envelopes → cache `len()` drops from 3 entries to 0
- TV-CS-3: Subscriber returns `None` (channel close) → task exits cleanly (`JoinHandle::join()` returns `Ok(())`)
- TV-CS-4: Lock-poisoning resilience — drop a guard while subscriber is mid-invalidate → next iteration recovers via `unwrap_or_else(PoisonError::into_inner)`
- TV-CS-5: Wildcard envelope for vault A invalidates cache entry for vault A (and B, C if present); single-bus round-trip
- TV-CS-6: Subscriber init runs BEFORE any producer call (test: spawn subscriber, then call `produce_burn`; both observe cache empty initially, then receive envelope after producer inserts)

## Layer direction (per `cipherocto-design-principles`)

- `octo-vault` (Layer B) — port trait `VaultProjectionInvalidationSubscriber` + `spawn_cache_subscriber` factory + `init_cache_subscriber` bootstrap
- `octo-vault-stoolap` (Layer D adapter, separate mission) — Stoolap NOTIFY/LISTEN impl
- Layer C binaries (octo-wallet-node / quota-router-sm-engine / octo-policy) call `init_cache_subscriber` in their bootstrap — tracked elsewhere
- No Layer B → Layer C inversion: subscriber does NOT import Layer C crates

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
cargo doc --no-deps --workspace  # no broken links
```

## Backward compat

- NEW file at `crates/octo-vault/src/cache_subscriber.rs` — additive (Layer B)
- `VaultBalanceCache::invalidate_all` already exists from Mission A; this mission only adds the bus-driven caller
- No existing call sites affected

## Risk

- HIGH: cache-subscriber bootstrap race — subscriber starts AFTER first `produce_*` call → first envelope missed, projection silently serves stale balances from the pre-projection-substrate `transfer_events` set (the table state before the RFC-0960 VaultBalanceProjection migration landed) for an unbounded window until the next event fires (silent consistency loss per RFC-0960 §7 Risk Callouts row 7). Mitigation: subscriber init in `main` BEFORE any handler is registered; mirror TV-PW-4 init-order verification pattern from `producer-wrapper-consumer-wiring`; RFC-0913 commit-coupled NOTIFY ensures bus event fires only on commit (no in-flight envelope loss).
- MEDIUM: wildcard subscription memory ceiling — `cache:projection:*` channel cardinality scales with vault count; Stoolap NOTIFY/LISTEN per-channel overhead is non-trivial at scale. Mitigation: bounded by vault count, NOTIFY/LISTEN per-channel overhead acceptable up to 100k vaults; revisit at higher scale.
- LOW: Stoolap NOTIFY/LISTEN transient disconnect — short network blip drops bus events. Mitigation: Stoolap client auto-reconnect (built into `stoolap` fork per `stoolap-general-purpose-db`); verified by stochastic disconnect test.

## Cross-references

- RFC-0960 §2.4 — invalidation bus + `cache:projection:<hex(vault_id)>` channel naming + `VaultProjectionInvalidationEmitter` + `VaultProjectionInvalidationEnvelope` (Mission B)
- RFC-0913 — Stoolap NOTIFY/LISTEN pub/sub pattern (Layer D adapter consumer)
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) — `VaultBalanceCache::invalidate_all`
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `VaultProjectionInvalidationEmitter` trait producer-side
- `producer-wrapper-consumer-wiring.md` — Layer C binary bootstrap wire-up (separate mission)

## Claimant

@mmacedoeu

## Pull Request

#