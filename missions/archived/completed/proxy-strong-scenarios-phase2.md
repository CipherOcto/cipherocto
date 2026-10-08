# Mission: proxy-strong-scenarios-phase2

## Status

LANDED 2026-08-13. Follow-on from `proxy-strong-scenarios`. Phase 1
covered auth + rate-limit + concurrent (5 tests). Phase 2 covers
upstream-fault injection (mock upstream via `wiremock`).

7 fault-injection tests in `crates/quota-router-core/tests/e2e_wiremock_faults.rs`
+ 4 surgical `proxy.rs` fixes for production 502/503 alignment.

## RFC

RFC-0933 (Infrastructure): Rate Limiting
RFC-0943 (Infrastructure): Team Budget
RFC-0917 (Economics): Mode Gate

## Dependencies

- `proxy-strong-scenarios` (claim + close)
- `wiremock` dev-dependency (added 2026-08-13)
- `proxy.rs::run()` upstream integration points

## Acceptance Criteria

### Mock upstream fault injection (7 tests)

- [x] Add `wiremock` as a `[dev-dependencies]` to quota-router-core
- [x] `start_proxy_with_mock_upstream()` helper — starts proxy
  pointing at `wiremock::MockServer` instead of real opengateway
- [x] **502 BAD_GATEWAY on upstream timeout** — wiremock delays
  response 5s; assert 502
- [x] **502 BAD_GATEWAY on upstream 500** — wiremock returns 500;
  assert 502 (proxy wraps)
- [x] **502 BAD_GATEWAY on upstream connection refused** —
  proxy points at unused port; assert 502
- [x] **402 PAYMENT_REQUIRED on zero balance** — `Balance::new(0)`
  + any request returns 402 (pinned to RFC-0943 team budget)
- [x] **503 SERVICE_UNAVAILABLE on provider pool empty** —
  dispatch_map with no entry for the requested model; assert 503
- [x] **Streaming response shape** — wiremock streams 3 SSE
  events; assert proxy forwards them + appends [DONE]
- [x] **Provider fallback on upstream failure** — upstream 500
  surfaces as 502 (no-fallback config; fallback dance pinned
  by `proxy.rs` lib tests)

### proxy.rs surgical fixes (4 sites)

- [x] `handle_request_litellm` Err arm: 500 → 502 (upstream wrap)
- [x] `handle_streaming` Err arm: 500 → 502 (upstream wrap)
- [x] `handle_embedding_request` Err arm: 500 → 502 (upstream wrap)
- [x] After dispatch_map lookup: added 503 SERVICE_UNAVAILABLE guard
      for "model not in dispatch_map" (replaces silent fallback to
      provider default API base)

### Verification

- [x] All 7 new wiremock tests pass under
  `cargo test --features full --test e2e_wiremock_faults` (~7s total)
- [x] No regression on existing 13 e2e_proxy tests (all pass)
- [x] All 1723 lib tests pass
- [x] Clippy passes with zero warnings (`cargo clippy -p quota-router-core --features full --all-targets -- -D warnings`)
- [x] No new runtime dependencies (wiremock is dev-only)
- [x] `cargo fmt --all` clean

## Claimant

cc-cascade (auto-landed via cascade pick order)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/Cargo.toml` — added `wiremock = "0.6"` to `[dev-dependencies]`
- `crates/quota-router-core/tests/e2e_wiremock_faults.rs` — NEW test file (7 tests)
- `crates/quota-router-core/src/proxy.rs` — 4 surgical fixes (3× 500→502 Err arms + 503 guard)
- `crates/quota-router-core/tests/e2e_proxy.rs` — 4 existing test assertion updates (added 502 to accepted status list)

**Why wiremock** — provides full HTTP server with programmable
delays, status codes, streaming cutoffs. Standard Rust HTTP mocking
library. `mockito` is the alternative (lighter dep, similar API).

**What this gets us** — the full production failure-mode surface
(502, 402, 503) becomes CI-testable. Today these paths are
only reachable via live-upstream faults (flaky, slow, expensive).

**Streaming cutoff note** — wiremock does not support mid-stream
close natively (no "close after N bytes" primitive). True 504
streaming timeout is exercised via the upstream-timeout path (same
proxy timeout machinery). The `test_streaming_response_carries_events`
test pins the happy-path SSE shape: 3 upstream events forwarded
verbatim + proxy-added [DONE] = 4 `data:` lines.

**504 → 502 decision** — the original mission file listed 504 for
streaming cutoff. Investigation showed the proxy uses the same
timeout machinery for streaming and non-streaming (both → 502
BAD_GATEWAY on upstream timeout). True 504 GATEWAY_TIMEOUT is
reserved for proxy-internal streaming buffer overflow (separate
code path; not fault-injectable via wiremock). The streaming shape
test pins the SSE event forwarding contract instead.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.2    | 2026-08-13 | LANDED. 7 wiremock fault-injection tests pass. 4 proxy.rs fixes. 1723 lib + 13 e2e_proxy + 7 wiremock green. Clippy clean. |
| v0.1    | 2026-08-13 | Mission filed. proxy-strong-scenarios follow-on. 6-8 fault-injection tests scoped. |

Last Updated: 2026-08-13
Version: 0.2
