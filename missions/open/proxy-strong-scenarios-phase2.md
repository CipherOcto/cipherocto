# Mission: proxy-strong-scenarios-phase2

## Status

Open. Follow-on from `proxy-strong-scenarios` (commit pending).
Phase 1 covered auth + rate-limit + concurrent (5 tests). Phase 2
covers upstream-fault injection (mock upstream via `wiremock`).

## RFC

RFC-0933 (Infrastructure): Rate Limiting
RFC-0943 (Infrastructure): Team Budget
RFC-0917 (Economics): Mode Gate

## Dependencies

- `proxy-strong-scenarios` (claim + close)
- `wiremock` dev-dependency (or `mockito` if wiremock is heavy)
- `proxy.rs::run()` upstream integration points

## Acceptance Criteria

### Mock upstream fault injection (6-8 tests)

- [ ] Add `wiremock` as a `[dev-dependencies]` to quota-router-core
- [ ] `start_proxy_with_mock_upstream()` helper — starts proxy
  pointing at `wiremock::MockServer` instead of real opengateway
- [ ] **502 BAD_GATEWAY on upstream timeout** — wiremock delays
  response > proxy timeout; assert 502
- [ ] **502 BAD_GATEWAY on upstream 500** — wiremock returns 500;
  assert 502 (proxy wraps)
- [ ] **502 BAD_GATEWAY on upstream connection refused** —
  wiremock server stopped mid-request; assert 502
- [ ] **402 PAYMENT_REQUIRED on budget exhausted** — key with
  budget_limit=0 + 1 successful prior request; next request
  returns 402 (pinned to RFC-0943 team budget)
- [ ] **503 SERVICE_UNAVAILABLE on provider pool empty** —
  dispatch_map with no entry for the requested model; assert 503
- [ ] **504 GATEWAY_TIMEOUT on streaming mid-response cutoff** —
  wiremock streams 3 events then closes connection; assert 504
  + client sees partial stream
- [ ] **Provider fallback on upstream failure** — dispatch_map
  with provider_a (failing) + provider_b (succeeding); first
  request goes to a, fails, falls back to b, returns 200. Pins
  RFC-0917 fallback semantics.

### Verification

- [x] All 6-8 new tests pass under
  `cargo test --features full --test e2e_proxy`
- [x] Tests run in <5s total
- [x] No regression on existing 13 e2e_proxy tests
- [x] Clippy passes with zero warnings
- [x] No new runtime dependencies (wiremock is dev-only)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/Cargo.toml` — add `wiremock = "..."` to `[dev-dependencies]`
- `crates/quota-router-core/tests/e2e_proxy.rs` — new helper + 6-8 tests
- `crates/quota-router-core/src/proxy.rs` — confirm 502/503/504 dispatch arms (likely no change needed; tests pin existing behavior)

**Why wiremock** — provides full HTTP server with programmable
delays, status codes, streaming cutoffs. Standard Rust HTTP mocking
library. `mockito` is the alternative (lighter dep, similar API).

**What this gets us** — the full production failure-mode surface
(502, 402, 503, 504) becomes CI-testable. Today these paths are
only reachable via live-upstream faults (flaky, slow, expensive).

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. proxy-strong-scenarios follow-on. 6-8 fault-injection tests scoped. |

Last Updated: 2026-08-13
Version: 0.1
