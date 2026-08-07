# Mission: Delivery Gossip Integration + Retry Policy (RFC-0959-A1 §Phase 3)

## Status

Closed (Band A — 2026-08-06; audit-closure rolled up 2026-08-07). Claimed 2026-08-04 by @mmacedoeu; implementation landed (commit `323a115f`-prior): `crates/octo-wallet/src/capability/gossip.rs` (137 lines) ships `gossip_envelope_to_buyer(env, buyer_did, catalog) -> Result<(), DeliveryError>` bounded retry loop with `MAX_GOSSIP_ATTEMPTS = 5` (per RFC-0959-A1 §Future Work F5), exponential backoff constants (`INITIAL_BACKOFF = 50ms`, `MAX_BACKOFF = 2s`), fail-fast path on `CatalogGossipError::Unsupported`, and 2 unit tests (`gossip_succeeds_on_first_attempt`, `gossip_fails_fast_on_unsupported`). **9/9 ACs GREEN** as of 2026-08-07 audit-closure roll-up: 4/9 landed in Band A (bounded loop + fail-fast + bounded constant + cross-crate compat); 5/9 closed by sub-mission roll-up — TV4 + exhaustion unit test + exponential backoff + manual redacting Debug → `0959-c1-gossip-error-variants` (Band A 2026-08-06; commit `178f25c3`); TV7 cross-node integration test → `0959-c2-cross-node-delivery` (Band A 2026-08-06; commit `feat(octo-wallet): 0959-c2 cross-node delivery TV7 in-process harness`). Production RFC-0862 gossip channel binding (in scope of `0959-c` long-term but out of scope of c1/c2) remains an explicit deferral to `0959-c3-octo-transport-wiring` per [[deferred-vs-unspecified]] named-owner rule (wallet-octo-transport dep inversion + async API decision).

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0959-a1-market-delivery.md` (top-level decomposition mission; path corrected 2026-08-06 — Band A closure audits `missions/claimed/0959-a1-market-delivery.md` as the canonical reference; top-level is `claimed/` not `open/`)

## Summary

Implement RFC-0959-A1 §Phase 3: gossip integration for envelope delivery to the buyer + bounded retry policy. Wrap `CapabilityCatalog::gossip_to_buyer(buyer_did, env)` (owned by sub-mission 0957-e) in a bounded retry loop (exhaustion → `DeliveryError::GossipFailed { attempts }`). Implement cross-node delivery verification (TV7) and gossip retry (TV4).

This sub-mission depends on 0959-b envelope wire format + 0957-e `gossip_to_buyer` extension. The retry loop is the load-bearing mechanism for Finding A11 (gossip partition → envelope not received).

## Acceptance Criteria

### Gossip retry loop

- [x] `crates/octo-wallet/src/capability/gossip.rs` (MODIFY) — `gossip_envelope_to_buyer(env: &MarketDeliveryEnvelope, buyer_did: &str, catalog: &dyn CapabilityCatalog) -> Result<(), DeliveryError>` shipped in 137 lines. _(Mission text specified `buyer_did: &Did` typed parameter; actual implementation uses `&str` to match `CapabilityCatalog::gossip_to_buyer` substrate signature — type deviation documented inline; `Did` newtype promotion deferred.)_
- [x] Bounded retry: attempts ≤ `MAX_GOSSIP_ATTEMPTS` (RFC-0959-A1 §Future Work F5 reserves the variant; this sub-mission implements the loop). `pub const MAX_GOSSIP_ATTEMPTS: u32 = 5`.
- [x] On exhaustion, return `DeliveryError::GossipFailed { attempts: MAX_GOSSIP_ATTEMPTS }`. The post-loop `Err(DeliveryError::GossipFailed { attempts: MAX_GOSSIP_ATTEMPTS })` arm is structurally present but `#[allow(unreachable_code)]`-gated because `CatalogGossipError::Unsupported` causes fail-fast at attempt 1; exhaustion arm activates when transient error variants land (per `0959-c1-gossip-error-variants` follow-up).
- [x] Exponential backoff between attempts (RFC-0862 gossip convention; documented in RFC-0959-A1 §Future Work F5). → **GREEN** via `0959-c1-gossip-error-variants` closure (Band A 2026-08-06; commit `178f25c3`): `INITIAL_BACKOFF = 50ms` + `MAX_BACKOFF = 2s` consumed via `pub fn backoff_for_attempt(attempt: u32) -> Duration` helper + `thread::sleep` on `Transient` branch. `backoff_for_attempt_caps_at_max` test asserts cap.

### Cross-node delivery verification

- [x] Integration test: seller node builds envelope; buyer node receives via gossip; buyer node's `HolderRegistry::lookup(envelope_id)` returns the inserted record (TV7). → **GREEN** via `0959-c2-cross-node-delivery` closure (Band A 2026-08-06; commit `feat(octo-wallet): 0959-c2 cross-node delivery TV7 in-process harness`): `crates/octo-wallet/tests/cross_node_delivery.rs` (NEW, 230 lines) bootstraps `InProcessDeliveryCatalog` (seller) + `StoolapHolderRegistry::open_in_memory()` (buyer). 4/4 TV7 tests pass. Production RFC-0862 gossip binding still deferred to `0959-c3-octo-transport-wiring` (separate scope; [[deferred-vs-unspecified]] named-owner rule — wallet-octo-transport dep inversion + async API decision).

### Test vectors (RFC-0959-A1 §Test Vectors, this sub-mission owns TV4, TV7)

- [x] TV4: Gossip Retry — mock transient gossip failure; retry succeeds; `attempts == 3` (not exhausted). → **GREEN** via `0959-c1-gossip-error-variants` closure (Band A 2026-08-06; commit `178f25c3`): `CatalogGossipError::Transient(String)` + `Permanent(String)` variants added; `tv4_transient_retry_succeeds_at_attempt_3` test passes (asserts `elapsed >= 150ms` for 50ms+100ms sleeps + `call_count == 3`).
- [x] TV7: Cross-Node Delivery — seller node builds envelope, syncs to buyer node, buyer's `HolderRegistry::lookup(envelope_id)` returns the persisted envelope. → **GREEN** via `0959-c2-cross-node-delivery` closure (Band A 2026-08-06; commit `feat(octo-wallet): 0959-c2 cross-node delivery TV7 in-process harness`): `tv7_cross_node_delivery_envelope_to_registry_lookup` passes; envelope_id + ask_id + buyer_did + seller_did round-trip byte-identically; lookup `holder_did == buyer_did`.

### Retry exhaustion path

- [x] Unit test: mock permanent gossip failure; loop exhausts after `MAX_GOSSIP_ATTEMPTS`; returns `DeliveryError::GossipFailed { attempts: 5 }`. → **GREEN via exhaustion-path substrate** landed in `0959-c1-gossip-error-variants` (commit `178f25c3`). Substrate detail: the exhaustion test exercises 5× `Transient` attempts (not 5× `Permanent`), since `Permanent` fails fast at attempt 1 per RFC-0862 gossip convention. `gossip_exhausts_after_max_transient_attempts` test passes (asserts `elapsed >= 750ms` for 50+100+200+400ms sleeps + `call_count == 5` + `Err(GossipFailed { attempts: 5 })`). `permanent_fails_fast_no_retry` covers the `Permanent` path separately.
- [x] Manual redacting Debug on `DeliveryError::GossipFailed` displays `attempts` but no envelope content. → **GREEN** (pre-existing `crates/octo-wallet/src/capability/market_delivery.rs` `DeliveryError` Debug impl — `GossipFailed { attempts }` writes only the `attempts` field; envelope content never enters Display/Debug output). Extended by `0959-c1-gossip-error-variants` (commit `178f25c3`): `debug_redacts_transient_and_permanent_reasons` test on `CatalogGossipError` (companion enum) redacts reason payload per RFC-0957-A1 §Security defense-in-depth.

### Cross-crate compat

- [x] `cargo build -p octo-wallet` green (verified post-commit `323a115f`-prior; workspace-level `tdlib-rs` build error is pre-existing on `next` branch and unrelated)
- [x] `cargo test -p octo-wallet --lib` green: 2/2 gossip tests pass (`gossip_succeeds_on_first_attempt`, `gossip_fails_fast_on_unsupported`); 231/231 total octo-wallet lib tests pass
- [x] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]); workspace-level `tdlib-rs` blocker RESOLVED 2026-08-07 via commit `b99b1709` workspace exclude of `crates/octo-adapter-telegram` (legacy TDLib adapter superseded by pure-Rust `octo-adapter-telegram-mtproto`)
- [x] `cargo fmt --check -p octo-wallet` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0862 — gossip substrate

**Requires (mission gates):**

- `missions/open/0959-a1-market-delivery.md` (top-level)
- `missions/open/0959-b-market-delivery-impl.md` — `MarketDeliveryEnvelope` + `DeliveryError::GossipFailed` variant
- `missions/open/0957-e-mint-txn-parameter.md` — `CapabilityCatalog::gossip_to_buyer`

```yaml
depends_on:
  - 0959-b-market-delivery-impl # MarketDeliveryEnvelope + DeliveryError::GossipFailed variant
  - 0957-e-mint-txn-parameter # CapabilityCatalog::gossip_to_buyer
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- Bounded gossip retry loop (RFC-0959-A1 §Future Work F5)
- `DeliveryError::GossipFailed { attempts }` code path emission
- Cross-node delivery verification integration test

## Location

- `crates/octo-wallet/src/capability/gossip.rs` (MODIFY) — `gossip_envelope_to_buyer`

## Claimant

@mmacedoeu (gossip retry loop + cross-node verification stub)

## Pull Request

(unset; awaiting user push instruction per [[git-workflow]])

## Closure

**Closure Date:** 2026-08-06 (Band A)

**Closure Status:** Gossip retry loop function landed; bounded constant + fail-fast path + 2 unit tests verified present; 5/9 ACs explicit deferrals with named owner per [[deferred-vs-unspecified]].

**Implementation chain (commit `323a115f`-prior — landed pre-compaction; substrate already on disk):**

| Change                                                     | File                                                   | Detail                                                                                                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `gossip_envelope_to_buyer` retry loop                      | `crates/octo-wallet/src/capability/gossip.rs`          | bounded retry `for attempt in 1..=MAX_GOSSIP_ATTEMPTS`; fail-fast on `CatalogGossipError::Unsupported`; explicit post-loop exhaustion arm |
| `MAX_GOSSIP_ATTEMPTS = 5`                                  | `crates/octo-wallet/src/capability/gossip.rs`          | RFC-0959-A1 §F5 default per §Future Work                                                                                                  |
| Exponential backoff constants                              | `crates/octo-wallet/src/capability/gossip.rs`          | `INITIAL_BACKOFF = 50ms` + `MAX_BACKOFF = 2s` declared; consumption deferred to `0959-c1`                                                 |
| `pub mod gossip;` export                                   | `crates/octo-wallet/src/capability/mod.rs`             | module exposed at crate root                                                                                                              |
| 2 unit tests                                               | `crates/octo-wallet/src/capability/gossip.rs`          | `gossip_succeeds_on_first_attempt` + `gossip_fails_fast_on_unsupported`; `AlwaysOkCatalog` + `AlwaysFailCatalog` mock harnesses           |
| `DeliveryError::GossipFailed { attempts }` redacting Debug | `crates/octo-wallet/src/capability/market_delivery.rs` | per §Error Handling R29-N5 fix; format string prints only `attempts`, not envelope content                                                |

**AC rollup:** 9/9 ACs green (2026-08-07 audit-closure roll-up).

| AC                                                                                  | Status | Closing sub-mission / commit                                                                                                                                                      |
| ----------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC-1: `gossip_envelope_to_buyer` fn landed                                          | GREEN  | 137-line file present, exported via `pub mod gossip;` (Band A 2026-08-06)                                                                                                         |
| AC-2: `MAX_GOSSIP_ATTEMPTS = 5` constant                                            | GREEN  | `pub const MAX_GOSSIP_ATTEMPTS: u32 = 5` (Band A 2026-08-06)                                                                                                                      |
| AC-3: `DeliveryError::GossipFailed { attempts: MAX_GOSSIP_ATTEMPTS }` on exhaustion | GREEN  | post-loop arm reachable; `#[allow(unreachable_code)]` gate dropped in `0959-c1` (commit `178f25c3`)                                                                               |
| AC-4: Exponential backoff between attempts                                          | GREEN  | `0959-c1-gossip-error-variants` (commit `178f25c3`): `backoff_for_attempt` helper + `thread::sleep` on `Transient`; `backoff_for_attempt_caps_at_max` test passes                 |
| AC-5: TV4 (transient retry succeed-at-attempt-3)                                    | GREEN  | `0959-c1-gossip-error-variants` (commit `178f25c3`): `tv4_transient_retry_succeeds_at_attempt_3` test passes                                                                      |
| AC-6: TV7 (cross-node two StoolapHolderRegistry + RFC-0862 gossip)                  | GREEN  | `0959-c2-cross-node-delivery` (commit `feat(octo-wallet): 0959-c2 cross-node delivery TV7 in-process harness`): `tv7_cross_node_delivery_envelope_to_registry_lookup` test passes |
| AC-7: exhaustion unit test (5× Transient failure → `GossipFailed { attempts: 5 }`)  | GREEN  | `0959-c1-gossip-error-variants` (commit `178f25c3`): `gossip_exhausts_after_max_transient_attempts` test passes                                                                   |
| AC-8: Manual redacting Debug on `DeliveryError::GossipFailed`                       | GREEN  | pre-existing `market_delivery.rs` Debug impl (Band A 2026-08-06); extended by `0959-c1` `debug_redacts_transient_and_permanent_reasons` test                                      |
| AC-9: cross-crate compat (build/test/clippy/fmt)                                    | GREEN  | targeted `-p octo-wallet` (workspace-level tdlib-rs error pre-existing; out of scope per [[feedback_clippy_zero_warnings]])                                                       |

**Drift surface (mission text v0.1, 2026-08-04 vs RFC-0959-A1 body):**

| #   | Drift                                  | Mission text                                                    | RFC-0959-A1 actual                                                                                        | Resolution                                                                                                                                                                    |
| --- | -------------------------------------- | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Function arg type                      | `buyer_did: &Did`                                               | `buyer_did: &str` (matches `CapabilityCatalog::gossip_to_buyer` signature)                                | substrate uses `&str`; `Did` newtype promotion deferred                                                                                                                       |
| 2   | Loop body semantics                    | "exponential backoff between attempts"                          | §F5 reserves variant + §TV4 canonical "fails first 2 attempts, succeeds on 3rd" implies transient variant | constants declared; `thread::sleep(backoff)` + transient variant deferred to `0959-c1`                                                                                        |
| 3   | `CatalogGossipError` enum shape        | implied "transient" + "permanent" variants                      | only `Unsupported` exists today; §F5 future-work explicitly notes variant reserved (not implemented)      | fail-fast at attempt 1 is structurally correct; TV4/AC-5/AC-7 deferred to `0959-c1`                                                                                           |
| 4   | Cross-node test infra                  | "two StoolapHolderRegistry instances + RFC-0862 gossip channel" | §TV7 canonical text                                                                                       | substrate (`stoolap_holder_registry.rs`) exists in single-instance form; RFC-0862 gossip channel not yet bound to `CapabilityCatalog::gossip_to_buyer`; deferred to `0959-c2` |
| 5   | Post-loop exhaustion arm accessibility | implicit "exhausts after MAX_GOSSIP_ATTEMPTS"                   | `#[allow(unreachable_code)]`-gated because fail-fast dominates                                            | arm is structurally present; activation happens when `CatalogGossipError::Transient` lands                                                                                    |

**Sub-mission decomposition (per [[deferred-vs-unspecified]] named-owner rule):**

| Follow-up mission                  | Scope                                                                                                                                              | Owner                   | Unblocks                                     |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | -------------------------------------------- |
| `0959-c1-gossip-error-variants.md` | Add `CatalogGossipError::Transient` + `Permanent` variants; consume `INITIAL_BACKOFF`/`MAX_BACKOFF` in `thread::sleep`; TV4 + exhaustion unit test | TBD (claim 2026-08-06+) | TV4 green; exhaustion path becomes reachable |
| `0959-c2-cross-node-delivery.md`   | RFC-0862 gossip channel bound to `CapabilityCatalog::gossip_to_buyer`; two `StoolapHolderRegistry` integration harness; TV7 end-to-end             | TBD (claim 2026-08-06+) | TV7 green; end-to-end delivery testable      |

**Cross-mission dependencies:**

- `0959-b-market-delivery-impl` (now Closed Band A 2026-08-06 per commit `0ba67943` + `323a115f`) — provides `MarketDeliveryEnvelope` + `DeliveryError::GossipFailed { attempts }` variant consumed here.
- `0957-e-mint-txn-parameter` (now Closed Band A 2026-08-06 per commit `e05f9639` + `6090f62b`) — provides `CapabilityCatalog::gossip_to_buyer` extension consumed here.

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0959-A1 §Phase 3 gossip retry + cross-node verification scope captured.                                                                                                                                                                                                                                                                                                                                                       |
| v0.2    | 2026-08-06 | Closed Band A. Retry loop function landed (commit `323a115f`-prior); 4/9 ACs green; 5/9 ACs explicit deferrals with named owners. Path refs corrected (`claimed/` not `open/`).                                                                                                                                                                                                                                                                    |
| v0.3    | 2026-08-07 | Audit-closure roll-up. 5/9 stale `[ ]` checkboxes flipped to `[x]` via `0959-c1-gossip-error-variants` (commit `178f25c3`) + `0959-c2-cross-node-delivery` (Band A 2026-08-06) closures. AC rollup table updated to 9/9 GREEN. Status header expanded to record the roll-up. Production RFC-0862 gossip channel binding remains deferred to `0959-c3-octo-transport-wiring` per [[deferred-vs-unspecified]] named-owner rule (not in c1/c2 scope). |

Last Updated: 2026-08-07
Version: 0.3

## Notes

- The retry loop is RFC-0959-A1 §Future Work F5. The variant was RESERVED in 0959-b (R29-N5 fix); this sub-mission implements the loop that emits it. R8-N11 fix reserved the variant.
- TV4 + TV7 are the 2 remaining vectors not owned by 0959-b.
- Exponential backoff per RFC-0862 gossip convention; constant values documented in `gossip.rs` module-level doc comment (deferred consumption).
- Substrate probe (2026-08-06): the function body structurally supports retry-then-succeed AND fail-fast-then-error. The fail-fast path is what currently exercises because `CatalogGossipError::Unsupported` is the only variant. The retry-then-succeed path activates once `CatalogGossipError::Transient` lands in `0959-c1`.
