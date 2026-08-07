# Mission: Gossip Error Variants + Backoff Consumption + TV4 (RFC-0959-A1 §Phase 3 follow-up)

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-06 by @mmacedoeu; implementation landed (commit `178f25c3`): `crates/octo-wallet/src/capability/macaroon.rs` extended `CatalogGossipError` with `Transient(String)` + `Permanent(String)` variants + manual redacting `Debug` impl (RFC-0957-A1 §Security). `crates/octo-wallet/src/capability/gossip.rs` consumes `INITIAL_BACKOFF`/`MAX_BACKOFF` via new `backoff_for_attempt(attempt: u32) -> Duration` helper + `thread::sleep` on `Transient` branch; `Unsupported | Permanent` merged into fail-fast arm (no retry, no backoff); `#[allow(unreachable_code)]` gate dropped; `#[allow(clippy::never_loop)]` gate also dropped. 5/5 ACs green. 7/7 gossip unit tests pass (2 pre-existing + 5 new: `backoff_for_attempt_caps_at_max`, `tv4_transient_retry_succeeds_at_attempt_3`, `gossip_exhausts_after_max_transient_attempts`, `permanent_fails_fast_no_retry`, `debug_redacts_transient_and_permanent_reasons`).

**Sub-mission of:** `missions/claimed/0959-c-delivery-gossip-integration.md` (Band A closed 2026-08-06; commits `2f078974` + `323a115f`-prior).

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

## Summary

Add `CatalogGossipError::Transient` + `Permanent` variants to enable the bounded-retry exhaustion path + exponential backoff consumption in `gossip_envelope_to_buyer`. Author TV4 (transient retry succeed-at-attempt-3) + exhaustion unit test (5 attempts all fail → `GossipFailed { attempts: 5 }`). Drops the `#[allow(unreachable_code)]` gate on the post-loop arm in `crates/octo-wallet/src/capability/gossip.rs`.

The `0959-c` Band A closure deferred this work because the only existing `CatalogGossipError` variant today is `Unsupported` (fail-fast at attempt 1), so the retry-exhaustion arm + exponential backoff consumption are structurally unreachable. Adding `Transient` + `Permanent` activates the full retry surface.

## Acceptance Criteria

### CatalogGossipError variants

- [x] `crates/octo-wallet/src/capability/macaroon.rs` — extend `CatalogGossipError` with `Transient(String)` (retryable; reason string for observability) + `Permanent(String)` (non-retryable; reason string). → **GREEN** (commit `178f25c3`)
- [x] `Unsupported` variant retained (fail-fast at attempt 1 — same behavior as today). → **GREEN** (commit `178f25c3`)
- [x] Manual redacting Debug impl: reason strings are operator-facing diagnostic only; redact any sender/did fields (defense in depth per RFC-0959-A1 §Security). → **GREEN** (commit `178f25c3`; `debug_redacts_transient_and_permanent_reasons` test passes)

### Backoff consumption

- [x] `gossip.rs` loop body: on `Transient` error, `thread::sleep(backoff)` with `backoff = min(INITIAL_BACKOFF * 2^(attempt-1), MAX_BACKOFF)` exponential schedule. → **GREEN** (commit `178f25c3`; `backoff_for_attempt()` helper + saturating shift)
- [x] `INITIAL_BACKOFF = 50ms` (existing constant) consumed; `MAX_BACKOFF = 2s` (existing constant) caps the schedule. → **GREEN** (commit `178f25c3`; `backoff_for_attempt_caps_at_max` test asserts cap)
- [x] Drop `#[allow(unreachable_code)]` gate on post-loop `Err(GossipFailed { attempts: MAX_GOSSIP_ATTEMPTS })` arm now that the loop can reach exhaustion. → **GREEN** (commit `178f25c3`; also dropped `#[allow(clippy::never_loop)]`)

### Test vectors (RFC-0959-A1 §Test Vectors)

- [x] TV4: Gossip Retry — mock harness returns `Err(Transient(_))` for attempts 1-2, then `Ok(())` on attempt 3. Assert: `attempts == 3`, `Ok(())` returned. → **GREEN** (commit `178f25c3`; `tv4_transient_retry_succeeds_at_attempt_3` test passes; asserts `elapsed >= 150ms` (50ms + 100ms sleeps) + `call_count == 3`)
- [x] Exhaustion unit test: mock harness returns `Err(Transient(_))` for all 5 attempts. Assert: `Err(DeliveryError::GossipFailed { attempts: 5 })` returned. → **GREEN** (commit `178f25c3`; `gossip_exhausts_after_max_transient_attempts` test passes; asserts `elapsed >= 750ms` (50+100+200+400ms sleeps) + `call_count == 5`)

### Cross-crate compat

- [x] `cargo build -p octo-wallet` green → **GREEN** (commit `178f25c3`)
- [x] `cargo test -p octo-wallet --lib` green (238/238 — 231 pre-existing + 5 new gossip tests + 2 pre-existing gossip tests = 238 total) → **GREEN** (commit `178f25c3`)
- [x] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]) → **GREEN** (commit `178f25c3`)
- [x] `cargo fmt --check -p octo-wallet` clean → **GREEN** (commit `178f25c3`)

## Dependencies

**Requires (RFC gates):**

- RFC-0862 — gossip substrate
- RFC-0959-A1 — Market Delivery Envelope (Amendment)

**Requires (mission gates):**

- `missions/claimed/0959-c-delivery-gossip-integration.md` (Band A closed 2026-08-06) — provides bounded retry loop + `INITIAL_BACKOFF`/`MAX_BACKOFF` constants + `GossipFailed { attempts }` emission consumed here
- `missions/claimed/0959-b-market-delivery-impl.md` (Band A closed 2026-08-06) — provides `MarketDeliveryEnvelope` + `DeliveryError::GossipFailed { attempts }` variant

```yaml
depends_on:
  - 0959-c-delivery-gossip-integration # bounded retry loop + backoff constants
  - 0959-b-market-delivery-impl # MarketDeliveryEnvelope + DeliveryError::GossipFailed
```

## Location

- `crates/octo-wallet/src/capability/gossip.rs` (MODIFY) — `CatalogGossipError` extension + backoff consumption + TV4 + exhaustion test

## Claimant

@mmacedoeu (CatalogGossipError variants + backoff consumption + TV4 + exhaustion test)

## Pull Request

(unset; awaiting user push instruction per [[git-workflow]])

## Closure

**Closure Date:** 2026-08-06 (Band A)

**Closure Status:** All 5 ACs green. Implementation landed in commit `178f25c3`. Cross-crate compat (build/test/clippy/fmt) verified clean.

**Implementation chain (commit `178f25c3`):**

| Change | File | Detail |
|---|---|---|
| `CatalogGossipError::Transient(String)` + `Permanent(String)` variants | `crates/octo-wallet/src/capability/macaroon.rs` | typed enum extended; `thiserror::Error` derived for `Display`; manual `Debug` impl redacts reason payload (RFC-0957-A1 §Security defense-in-depth) |
| Manual redacting `Debug` impl on `CatalogGossipError` | `crates/octo-wallet/src/capability/macaroon.rs` | `Unsupported` → `"Unsupported"`; `Transient(_)` → `Transient("[REDACTED reason]")`; `Permanent(_)` → `Permanent("[REDACTED reason]")` |
| `backoff_for_attempt(attempt: u32) -> Duration` helper | `crates/octo-wallet/src/capability/gossip.rs` | `min(INITIAL_BACKOFF * 2^(attempt-1), MAX_BACKOFF)`; saturating shift + saturating mul; caps at `MAX_BACKOFF = 2s` |
| Bounded retry with exponential backoff | `crates/octo-wallet/src/capability/gossip.rs` | on `Transient`: `thread::sleep(backoff_for_attempt(attempt))` before next attempt (skip on final); on `Unsupported \| Permanent`: fail-fast at attempt 1 |
| Drop `#[allow(unreachable_code)]` gate | `crates/octo-wallet/src/capability/gossip.rs` | post-loop `Err(GossipFailed { attempts: MAX_GOSSIP_ATTEMPTS })` arm now reachable (5× Transient exhaustion) |
| Drop `#[allow(clippy::never_loop)]` gate | `crates/octo-wallet/src/capability/gossip.rs` | loop now produces real exhaustion path; no longer structurally never-iterated |
| 5 new unit tests | `crates/octo-wallet/src/capability/gossip.rs` | `backoff_for_attempt_caps_at_max`, `tv4_transient_retry_succeeds_at_attempt_3`, `gossip_exhausts_after_max_transient_attempts`, `permanent_fails_fast_no_retry`, `debug_redacts_transient_and_permanent_reasons` |
| 3 new test catalogs | `crates/octo-wallet/src/capability/gossip.rs` | `AlwaysPermanentCatalog`, `AlwaysTransientCatalog` (atomic call counter), `AlwaysTransientThenOk` (atomic counter + initial-fail-count) |

**AC rollup:** 5/5 ACs green. 7/7 gossip tests pass.

**Drift surface (mission text v0.1, 2026-08-06 vs RFC-0959-A1 body):**

| # | Drift | Mission text | RFC-0959-A1 actual | Resolution |
|---|---|---|---|---|
| 1 | Test catalog placement | "mock harness" | 3 distinct typed catalogs (Permanent / Transient / Transient-then-Ok) | substrate uses typed catalogs for clarity; each catalog tests one failure mode |
| 2 | Backoff helper visibility | (not specified) | `pub fn backoff_for_attempt(attempt: u32) -> Duration` exposed | exposed for direct testing (`backoff_for_attempt_caps_at_max`); loop uses it internally |
| 3 | `match_same_arms` lint on `Unsupported` vs `Permanent` | separate match arms | merged arm `Err(Unsupported \| Permanent(_))` | clippy `match_same_arms` flagged; merged + nested per `unnested_or_patterns` |

**Cross-mission dependencies (post-closure):**

- `0959-c-delivery-gossip-integration` (Closed Band A 2026-08-06) — provided bounded retry loop + `MAX_GOSSIP_ATTEMPTS` + `INITIAL_BACKOFF`/`MAX_BACKOFF` constants + `DeliveryError::GossipFailed { attempts }` consumption
- `0959-b-market-delivery-impl` (Closed Band A 2026-08-06) — provided `MarketDeliveryEnvelope` + `DeliveryError::GossipFailed { attempts }` variant

**Unblocked:** `0959-c2-cross-node-delivery` (TV7 cross-node delivery infrastructure) is the remaining follow-up from `0959-c` Band A closure. This mission does NOT unblock it — TV7 needs cross-node gossip channel substrate, not error variants.

**Version History:**

| Version | Date | Change |
|---|---|---|
| v0.1 | 2026-08-06 | Mission filed by `0959-c` Band A closure. Deferred ACs from `0959-c` captured (CatalogGossipError variants + backoff consumption + TV4 + exhaustion test). |
| v0.2 | 2026-08-06 | Closed Band A. CatalogGossipError extended (Transient + Permanent + manual redacting Debug); backoff consumed via `backoff_for_attempt` helper; `#[allow]` gates dropped; 7/7 gossip tests green; cross-crate compat clean. |

Last Updated: 2026-08-06
Version: 0.2

## Notes

- The `Transient` + `Permanent` distinction enables the bounded-retry exhaustion path. Real upstream gossip channels (RFC-0862) naturally distinguish transient network failures (retry) from permanent schema/capability mismatches (fail-fast) — same `Unsupported` semantics as today.
- Backoff schedule is exponential with `MAX_BACKOFF = 2s` cap. Total worst-case latency under exhaustion: 50ms + 100ms + 200ms + 400ms + 800ms = 1.55s sleep budget + 5 actual gossip attempts.
- Exhaustion unit test (5 attempts all fail) is the previously-deferred AC-7 from mission `0959-c` Band A closure.
