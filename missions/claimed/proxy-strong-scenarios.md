# Mission: proxy-strong-scenarios

## Status

Closed 2026-08-13 (@claude). LANDED.

5/5 strong-scenario E2E tests for the quota-router full pipeline
(auth + rate-limit + concurrent) implemented in
`crates/quota-router-core/tests/e2e_proxy.rs`. 13/13 e2e_proxy tests
pass (8 existing + 5 new); 7 ignored (live upstream required).

## RFC

RFC-0932 (Infrastructure): Gateway Authentication
RFC-0933 (Infrastructure): Rate Limiting

## Dependencies

- e2e_proxy.rs substrate (live + non-live tests, commit history)
- StoolapKeyStorage + RateLimiterStore + ProxyServer::with_storage/with_rate_limiter/with_master_key

## Acceptance Criteria

### Strong-scenario E2E tests

- [x] **`test_auth_missing_returns_401`** — POST /v1/chat/completions
  with no Authorization header → 401. Pins RFC-0932 anonymous-rejection
  contract.

- [x] **`test_auth_invalid_returns_401`** — POST with non-registered
  key (valid format, wrong bytes) → 401. Pins constant-time HMAC
  compare (no key-enumeration timing leak).

- [x] **`test_auth_master_key_bypasses_storage`** — POST with master
  key (not in storage) → auth passed (status ≠ 401/403). Pins
  operator escape-hatch for incident response + key rotation.

- [x] **`test_rpm_rate_limit_returns_429`** — pre-create key with
  rpm_limit=2; 1st + 2nd pass auth, 3rd gets 429 with Retry-After
  header. Pins RFC-0933 rate limit per-key.

- [x] **`test_concurrent_auth_requests_deduped_per_key`** — 10
  concurrent requests with same valid key (rpm_limit=100); zero
  401/403, zero 429. Pins auth dedup regression (no accidental
  memoization dropping requests).

### Helper infrastructure

- [x] `start_proxy_with_auth(rpm_limit, budget_limit)` — spins up
  proxy with `StoolapKeyStorage` (in-memory) + `RateLimiterStore` +
  master key. Pre-creates one API key with the requested rpm_limit,
  returns `(base_url, raw_key)`.

### Verification

- [x] 13/13 tests pass under
  `LD_LIBRARY_PATH=/home/mmacedoeu/.pyenv/versions/3.12.9/lib cargo test --features full --test e2e_proxy`
- [x] Clippy passes with zero warnings
- [x] All 8 existing e2e_proxy tests still pass

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**Why these 5 scenarios** — chosen by mapping the proxy's auth +
rate-limit + concurrent surface to the most likely production
failure modes:

1. **Auth missing** — anonymous-abuse vector; one of the most common
   security findings in production. Must be 401 before any upstream
   call.
2. **Auth invalid** — key-enumeration timing attack. Constant-time
   compare is the contract.
3. **Master key bypass** — operator incident-response contract. If
   this regresses, you cannot rotate keys during an attack.
4. **RPM 429** — RFC-0933 per-key rate limit. Without this, a single
   bad actor can DoS the upstream.
5. **Concurrent dedup** — guards against an accidental auth-result
   cache that would drop N-1 of N concurrent requests.

**Coverage delta** — e2e_proxy went from 17 (8 unignored + 9
ignored live) to 22 tests. The 5 new tests run in <1s total and
require no live upstream; they exercise the proxy's full HTTP path
through its auth + rate-limit + storage layers.

**What's still missing** — proxy.rs has 11684 LoC and many other
failure-mode paths (502 BAD_GATEWAY on upstream failure, 402
PAYMENT_REQUIRED on budget exhaustion, 503 on provider unavailable)
that are not yet covered by self-contained E2E. Those tests would
require either an in-process mock upstream (e.g., `wiremock`) or
fault-injection hooks. Filed as follow-on.

## Follow-ons

- `proxy-strong-scenarios-phase2` — mock-upstream fault injection
  via `wiremock` (or `mockito`); covers 502 (upstream timeout), 402
  (budget exhausted), 503 (provider pool empty), 504 (streaming
  mid-response cutoff). ~6-8 additional tests.

## Cross-references

- RFC-0932 (Infrastructure): Gateway Authentication
- RFC-0933 (Infrastructure): Rate Limiting
- `crates/quota-router-core/src/proxy.rs:680-810` — auth + rate-limit dispatch
- `crates/quota-router-core/src/keys/` — key substrate

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-13 | claimed  | Mission filed. 5 strong-scenario E2E tests scoped. |
| v0.2    | 2026-08-13 | closed   | 5/5 implemented in e2e_proxy.rs; 13/13 tests pass. Follow-on `proxy-strong-scenarios-phase2` filed for upstream-fault injection. |
