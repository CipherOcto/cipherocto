# producer-wrapper-consumer-wiring — wire produce_burn/produce_payment/produce_settlement into MintHandler + SettlementEventRepository + cache_subscriber bootstrap

**Status:** claimed (2026-08-27)
**Substrate:** RFC-0960 §6 (Mission B producer fan-in) + RFC-0959 §2.1 (SettlementEventRepository substrate) + RFC-0965 §2.1 (PaymentCaveat)
**Parent:** R3 review follow-on (producer wrapper consumer wiring — producers exist but are unwired at the call site)
**Depends on:**

- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `produce_burn` / `produce_payment` / `produce_settlement` fn signatures must exist
- Mission `cache-subscriber-bus-wiring.md` — `init_cache_subscriber` factory must exist
- Mission `caveat-central-enum-non-exhaustive.md` (parallel) — `Caveat::Payment` discriminant match arms
- Mission `cache-bus-auth.md` (parallel) — `produce(...)` gains `producer_did: &OverlayIdentity` + `producer_sequence: &AtomicU64` parameters from the auth envelope; ALL wire-up sites below MUST thread these parameters end-to-end. Landing either mission BEFORE cache-bus-auth → mass cache staleness (subscribers reject unverifiable envelopes).

## Motivation

Mission B landed 3 producer wrapper functions at Layer C sites:

- `produce_burn` at `crates/octo-policy/src/event_log_producer.rs` (8-param signature)
- `produce_payment` at `crates/octo-wallet-node/src/handlers/event_log_producer.rs` (6-param signature)
- `produce_settlement` at `crates/quota-router-sm-engine/src/event_log_producer.rs` (6-param signature)

Currently, NO caller invokes these functions — they exist but are unwired. The wire-up is the bridge between substrate (RFCs) and runtime (Layer C binaries):

1. **`MintHandler`** at `crates/octo-wallet-node/src/handlers/mint.rs` mints capabilities but does NOT call `produce_payment` to log the mint event into `transfer_events`. Minted capabilities become invisible to the projection layer.
2. **`SettlementEventRepository::insert`** at `crates/quota-router-storage/src/settlement_event_repo.rs` (Layer C) inserts settlement events but does NOT call `produce_settlement`. Settlements become invisible to the projection layer.
3. **`cache_subscriber` bootstrap** at every Layer C binary's `main` (octo-wallet-node / quota-router-sm-engine / octo-policy) does NOT exist — even though `init_cache_subscriber` factory lands (separate mission). Wire-up is per-binary.

## Scope

Wire `produce_burn` / `produce_payment` / `produce_settlement` at their respective Layer C call sites + bootstrap `init_cache_subscriber` at each Layer C binary's process init.

### Sub-steps

1. **`MintHandler` wire-up** — `crates/octo-wallet-node/src/handlers/mint.rs`. **Produce-BEFORE-write atomicity** (insert `transfer_events` row FIRST, then perform the canonical mint write; on producer failure, mint fails-closed without partial state). Pass `chain_id` from `MintRequest`, `from_vault_id` from the minter's wallet, `to_vault_id` from the recipient caveat, `caveat: PaymentCaveat` from the mint request, `amount: Dqa` parsed from the mint amount, `occurred_at_unix: current_unix_seconds`, **`producer_did: &OverlayIdentity`** (the wallet-node's own DID per RFC-0853 §3 Sovereign Identity Model; loaded from node-identity substrate at `MintHandler` construction; threading path: `start_server → Builder → MintHandler.producer_did`), **`producer_sequence: &AtomicU64`** (per-process monotonic counter on the wallet-node; same lifetime as the producer). Producer error → mint fails (no partial mint). Order: `produce_payment → mint_write` (not `mint_write → produce_payment`).

2. **`SettlementEventRepository::insert` wire-up** — `crates/quota-router-storage/src/settlement_event_repo.rs`. Insert `produce_settlement(...)` call inside the existing Stoolap txn boundary, AFTER `SettlementEvent::new` (Mission G) constructs the event. Same produce-BEFORE-write order as sub-step 1: `produce_settlement → settlement_row_insert` (NOT the reverse). Transaction rolls back on either failure — atomicity preserved via existing Stoolap txn (RFC-0913 commit-coupled NOTIFY guarantees the invalidation bus fires only on commit). **Auth params:** `producer_did` from the repository's `WalletNodeOverlay` field (cross-crate share required — wire at Cycle 1; verified at landing time via `grep -n "OverlayIdentity" crates/quota-router-storage/src/`); `producer_sequence` from a per-process `AtomicU64` cell on the repository.

3. **`BurnEventRef::consume` wire-up** — wherever `consume()` is called (verified at landing time via `grep -rn "burn_event::consume" crates/`), the caller MUST call `produce_burn` BEFORE `consume` is considered complete (NOT after). The fail-closed contract: `consume()` is rewritten to intern an internal `produce_burn` call at the start of its body — if `produce_burn` returns `Err`, `consume()` returns that error WITHOUT committing the canonical burn. This matches sub-steps 1 + 2's produce-BEFORE-write order uniformly (no TOCTOU window where canonical burn is recorded but projection layer is stale). **Auth params:** pass the existing `policy_engine.overlay_identity` + `policy_engine.producer_sequence` from the same sites that already hold the burn event handle. No new fields required (octo-policy already owns its own `OverlayIdentity` via RFC-0853 §3 Sovereign Identity Model (OverlayIdentity struct) + RFC-0009 process substrate (identity management)).

4. **`init_cache_subscriber` bootstrap** — at each Layer C binary's `main`:
   - `crates/octo-wallet-node/src/main.rs` (if exists; else `lib.rs::start_server`) — spawn cache subscriber
   - `crates/quota-router-sm-engine/src/main.rs` (if exists; else `lib.rs::start_server`) — spawn cache subscriber
   - `crates/octo-policy/src/main.rs` (if exists; else `lib.rs::start_server`) — spawn cache subscriber

5. **Layer C trait plumbing** — `MintHandler`, `SettlementEventRepository`, and `BurnEventRef` consumers must each gain access to `TransferEventLog`, `VaultProjectionInvalidationEmitter`, `AssetRegistry`, `VaultAssetResolver`, `NonceRegistry`. Plumbing strategy: add these as fields on each consumer struct OR pass via builder. Builder pattern preferred (avoids god-object; matches `cipherocto-design-principles` §No god-objects).

6. **Test fixture bootstrap** — each consumer-site test (`crates/octo-wallet-node/tests/`, `crates/quota-router-storage/tests/`, `crates/octo-policy/src/burn_event.rs`) gains a `StubTransferEventLog` / `StubEmitter` fixture set. Reusable fixtures land at `crates/octo-vault/src/testing.rs` (NEW) per §Composition over inheritance.

## Out of Scope

- Adding new producer fns (`produce_payment` is the only Payment-producer; no `produce_refund` / `produce_topup` etc. — each is a separate mission)
- Modifying the producer fn signatures (those landed at Mission B; breaking changes belong to other missions like `l4-parallel-transfer-event-log-elimination.md`)
- Migrating `MintHandler` from sync to async (separate decision tracked per `mode-gate-never-equals-interface`)
- Subscribing to the bus at Layer C binaries that do NOT consume the cache (e.g., CLI tools, single-shot bin scripts — they may not need the subscriber)

## Test Vectors

- TV-PW-1: `MintHandler::mint` calls `produce_payment` end-to-end; `transfer_events` row inserted with `from_vault_id = minter.wallet`, `to_vault_id = caveat.recipient`, `asset_id = caveat.asset_id`
- TV-PW-2: `SettlementEventRepository::insert` calls `produce_settlement` inside the Stoolap txn; `transfer_events` row inserted atomically with the settlement row (rollback on either failure)
- TV-PW-3: All `burn_event::consume` call sites call `produce_burn` BEFORE `consume()` returns `Ok` (grep `burn_event::consume` returns N sites; grep `produce_burn` returns N sites at matching call stacks; verify NO site has a literal `consume()?; produce_burn(...)` ordering — that pattern is forbidden by the atomicity guarantee)
- TV-PW-4: Cache subscriber bootstrap runs BEFORE first `produce_*` call at each binary; verified via init-order test (spawn subscriber, then call `produce_*`; first envelope observed by subscriber)
- TV-PW-5: StubTransferEventLog / StubEmitter fixtures are reusable across 3+ test files (`grep -rn "StubTransferEventLog\|StubEmitter" crates/` ≥ 3 matches)
- TV-PW-6: Producer fn error in `MintHandler::mint` causes the mint to fail (no partial mint); verified by injecting a stub that returns `Err(ProducerError::...)` and asserting mint returns `Err`
- TV-PW-7: All 3 producer wrappers are wired at landing time (grep `produce_burn\|produce_payment\|produce_settlement` returns ≥ 3 call sites in production code, not just definitions)
- TV-PW-8: All 3 wire-up sites pass non-default `producer_did: &OverlayIdentity` (verification: `grep -rn "produce_payment" crates/octo-wallet-node/src/handlers/mint.rs` returns ≥ 1 matches with `producer_did` token visible; same for `produce_settlement` at `settlement_event_repo.rs` and `produce_burn` at `burn_event.rs`)
- TV-PW-9: NO site uses the reverted-order pattern (gate: `grep -rnE '(^|[^_])consume\([^{]*\)\s*;\s*produce_burn' crates/ agents/ use-cases/` returns 0 matches; `grep -rn 'produce_burn' crates/octo-policy/src/burn_event.rs | wc -l` ≥ number of `consume(` call sites + 1 for the inlined-at-start-of-consume instance; the only legal ordering is `produce_*` first, then canonical write/commit)
- TV-PW-10: `producer_sequence: &AtomicU64` is captured pre-sign and incremented post-sign via `fetch_add(1, Relaxed)` at every wire-up site (TV-PW-8's 3 sites all show `producer_sequence.fetch_add(1, Relaxed)` in the surrounding block)

## Layer direction (per `cipherocto-design-principles`)

- `octo-wallet-node` (Layer C) — `MintHandler` wire-up
- `quota-router-storage` (Layer C) — `SettlementEventRepository::insert` wire-up
- `octo-policy` (Layer C) — `BurnEventRef::consume` callers wire-up
- `octo-vault` (Layer B) — `testing.rs` stub fixtures (additive)
- No layer inversion

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
cargo test --workspace --features full  # integration tests

# Wire-up gates
grep -rn "produce_burn" crates/octo-policy/src/  # expect: ≥ 2 (defn + caller)
grep -rn "produce_payment" crates/octo-wallet-node/src/  # expect: ≥ 2
grep -rn "produce_settlement" crates/quota-router-storage/src/  # expect: ≥ 2

# Burn consumer sites all wire up
grep -rn "burn_event::consume" crates/  # returns N
grep -rn "produce_burn" crates/  # returns N+1 (defn + each caller)

# Cache subscriber bootstrap
grep -rn "init_cache_subscriber\|spawn_cache_subscriber" crates/
# expect: ≥ 3 (one per Layer C binary)

# End-to-end projection integration test
cargo test --workspace --lib projection_end_to_end
# (NEW test exercising full MintHandler → produce_payment → cache invalidation → projection re-compute)
```

## Backward compat

- `MintHandler::mint` signature MAY change (gain new params for the producer chain) — depends on builder pattern adopted at sub-step 5. **Source-breaking** if signature changes; mitigated by additive builder.
- `SettlementEventRepository::insert` signature UNCHANGED (wire-up is internal — `produce_settlement` called inside the existing method body).
- `BurnEventRef::consume` signature UNCHANGED (signatures are stable; behavioral change is internal — `consume()` inlines `produce_burn` at the start of its body per sub-step 3, callers do NOT call `produce_burn` themselves after `consume` returns).
- New `crates/octo-vault/src/testing.rs` — additive (Layer B test-only fixture).

## Risk

- HIGH: atomicity gap — if `produce_*` fails AFTER the original write, the system is in an inconsistent state (write succeeded, projection not invalidated). Mitigation: produce-BEFORE-write order (insert `transfer_events` first, then write the canonical record); on producer failure, write fails-closed. All 3 sub-steps (1, 2, 3) enforce this order uniformly — sub-step 3 inlines `produce_burn` at the START of `consume()` so the burn is non-committable without a successful producer emission.
- MEDIUM: lock contention — `process_drain_lock()` shared across 3 producers serializes ALL writes. Mitigation: lock acquired only around the `produce_*` body, not the entire `MintHandler::mint` (which may do non-write work).
- MEDIUM: cache subscriber bootstrap race — if subscriber starts AFTER first producer call, the first envelope is missed. Mitigation: subscriber init in `main` BEFORE any handler is registered; verified by TV-PW-4.
- LOW: stub fixture proliferation — if not consolidated at `crates/octo-vault/src/testing.rs`, each test file invents its own. Mitigation: sub-step 6 mandates single canonical location.

## Cross-references

- RFC-0960 §6 — Mission B producer fan-in (substrate)
- RFC-0959 §2.1 — SettlementEventRepository substrate
- RFC-0965 §2.1 — PaymentCaveat (Payment producer input)
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — producer fn definitions
- Mission `cache-subscriber-bus-wiring.md` — `init_cache_subscriber` factory (parallel)
- Mission `caveat-central-enum-non-exhaustive.md` — Caveat::Payment discriminant (parallel)
- Mission `cache-bus-auth.md` — `producer_did` + `producer_sequence` params on `produce(...)` (parallel, mandatory dep)
- Mission `l4-parallel-transfer-event-log-elimination.md` — reduces `produce_burn` to 7 params (parallel; affects this mission's signature planning)
- `cipherocto-design-principles` §No god-objects — builder pattern preferred

## Claimant

@mmacedoeu

## Pull Request

#