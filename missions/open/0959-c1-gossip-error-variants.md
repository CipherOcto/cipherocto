# Mission: Gossip Error Variants + Backoff Consumption + TV4 (RFC-0959-A1 §Phase 3 follow-up)

## Status

Open (filed 2026-08-06 by mission `0959-c-delivery-gossip-integration.md` Band A closure). Per [[deferred-vs-unspecified]] named-owner rule, this follow-up mission owns the deferred `CatalogGossipError::Transient` + `Permanent` variants + `INITIAL_BACKOFF`/`MAX_BACKOFF` consumption + TV4 (transient retry succeed-at-attempt-3) + exhaustion unit test (5 attempts all fail).

**Sub-mission of:** `missions/claimed/0959-c-delivery-gossip-integration.md` (Band A closed 2026-08-06; commits `2f078974` + `323a115f`-prior).

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

## Summary

Add `CatalogGossipError::Transient` + `Permanent` variants to enable the bounded-retry exhaustion path + exponential backoff consumption in `gossip_envelope_to_buyer`. Author TV4 (transient retry succeed-at-attempt-3) + exhaustion unit test (5 attempts all fail → `GossipFailed { attempts: 5 }`). Drops the `#[allow(unreachable_code)]` gate on the post-loop arm in `crates/octo-wallet/src/capability/gossip.rs`.

The `0959-c` Band A closure deferred this work because the only existing `CatalogGossipError` variant today is `Unsupported` (fail-fast at attempt 1), so the retry-exhaustion arm + exponential backoff consumption are structurally unreachable. Adding `Transient` + `Permanent` activates the full retry surface.

## Acceptance Criteria

### CatalogGossipError variants

- [ ] `crates/zk-verifier` / `crates/octo-wallet/src/capability/gossip.rs` — extend `CatalogGossipError` with `Transient(String)` (retryable; reason string for observability) + `Permanent(String)` (non-retryable; reason string).
- [ ] `Unsupported` variant retained (fail-fast at attempt 1 — same behavior as today).
- [ ] Manual redacting Debug impl: reason strings are operator-facing diagnostic only; redact any sender/did fields (defense in depth per RFC-0959-A1 §Security).

### Backoff consumption

- [ ] `gossip.rs` loop body: on `Transient` error, `thread::sleep(backoff)` with `backoff = min(INITIAL_BACKOFF * 2^(attempt-1), MAX_BACKOFF)` exponential schedule.
- [ ] `INITIAL_BACKOFF = 50ms` (existing constant) consumed; `MAX_BACKOFF = 2s` (existing constant) caps the schedule.
- [ ] Drop `#[allow(unreachable_code)]` gate on post-loop `Err(GossipFailed { attempts: MAX_GOSSIP_ATTEMPTS })` arm now that the loop can reach exhaustion.

### Test vectors (RFC-0959-A1 §Test Vectors)

- [ ] TV4: Gossip Retry — mock harness returns `Err(Transient(_))` for attempts 1-2, then `Ok(())` on attempt 3. Assert: `attempts == 3`, `Ok(())` returned.
- [ ] Exhaustion unit test: mock harness returns `Err(Transient(_))` for all 5 attempts. Assert: `Err(DeliveryError::GossipFailed { attempts: 5 })` returned.

### Cross-crate compat

- [ ] `cargo build -p octo-wallet` green
- [ ] `cargo test -p octo-wallet --lib` green (existing 231 tests + 2 new TV4 + exhaustion tests = 233+ total)
- [ ] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [ ] `cargo fmt --check -p octo-wallet` clean

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

TBD (claim 2026-08-06+)

## Notes

- The `Transient` + `Permanent` distinction enables the bounded-retry exhaustion path. Real upstream gossip channels (RFC-0862) naturally distinguish transient network failures (retry) from permanent schema/capability mismatches (fail-fast) — same `Unsupported` semantics as today.
- Backoff schedule is exponential with `MAX_BACKOFF = 2s` cap. Total worst-case latency under exhaustion: 50ms + 100ms + 200ms + 400ms + 800ms = 1.55s sleep budget + 5 actual gossip attempts.
- Exhaustion unit test (5 attempts all fail) is the previously-deferred AC-7 from mission `0959-c` Band A closure.
