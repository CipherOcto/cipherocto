# Mission: 0948-b1 — Admin Endpoint Rate Limiting

## Status

Closed 2026-08-13 (@claude). Substrate LANDED; admin.rs dispatch
hook + cfg plumbing DEFERRED to follow-ons (see Version History).

## RFC

RFC-0933 (Infrastructure): Rate Limiting
RFC-0948 (Economics): Prompt Management
RFC-0949 (Economics): Enterprise SSO

## Dependencies

- 0948-b drift closure (2026-08-13) ✓
- `crates/quota-router-core/src/rate_limit/` substrate (already
  exists) ✓

## Acceptance Criteria

### Substrate (LANDED in this mission)

- [x] `AdminRateLimitPolicySet` struct with 11 RFC-0933 endpoint
      groups + `AdminLimitDimension { Ip | User | ApiKey }` enum +
      `Default` impl returning RFC-0933 reference values
- [x] `AdminIdentity` struct carries `ip`, `user_id`,
      `api_key_prefix` (first 8 chars of the API key per AC)
- [x] `AdminRateLimiter::new(policies)` /
      `AdminRateLimiter::with_default_policies()` constructors
- [x] `AdminRateLimiter::set_enabled(bool)` — runtime toggle (the
      `cfg.rate_limit.admin.enabled` flag plumbs through this)
- [x] `AdminRateLimiter::policy_for(method, path)` — route lookup
      per RFC-0933 endpoint table
- [x] `AdminRateLimiter::check(method, path, identity)` — does the
      per-route per-identity bucket lookup; returns `Allowed | Blocked`
- [x] `check_admin_rate_limit(...)` HTTP-agnostic middleware helper
      returning `Option<AdminRateLimitOutcome>` (None = allowed,
      Some(Blocked) = 429)
- [x] `AdminRateLimiter::reset_all()` — ops incident-recovery hook
- [x] Per-IP hashing via `AdminIdentity::ip`
- [x] Per-user hashing via `AdminIdentity::user_id`
- [x] Per-API-key hashing via `AdminIdentity::api_key_prefix`
      (first 8 chars of bearer token per RFC-0933)
- [x] 10 unit tests pass (clippy zero warnings). Per-IP
      threshold-block, per-user independent buckets, per-key
      first-8 identity, cross-endpoint independence, unrate-limited
      route pass-through, runtime-disable, retry-after surfacing,
      identity resolution across all 3 dimensions, distinct
      prompts-versions policy, policy lookup for prompts CRUD
      methods.

### Deferred to follow-on missions

- [ ] Wire `check_admin_rate_limit` into `admin.rs::handle_request`
      dispatch (admin.rs is a 2904-line monolith; the substrate is
      the smallest-scope first landing). → Follow-on mission
      `0948-b1-dispatch-hook`.
- [ ] Return `429 Too Many Requests` with `Retry-After: <secs>`
      header on `Blocked` outcome. → Landed together with
      `0948-b1-dispatch-hook` (the dispatch arm that calls the
      middleware is the natural call site for the response builder).
- [ ] `cfg.rate_limit.admin.enabled: bool` ServerConfig plumbing
      (currently runtime-only via `set_enabled`). → Follow-on
      mission `0948-b1-server-config`.
- [ ] Pre-existing test drift on
      `crates/quota-router-core/src/marketplace/reputation_compat.rs:425`
      (asserts old `RecorderDidMalformed` after commit `eb6aaf34`
      renamed the variant to `ControllerIdMissing`). NOT caused
      by this mission's diff; flagged as mission #104
      `reputation-compat-controller-id-test-drift`.

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**Layer discipline:** the new module
`crates/quota-router-core/src/admin_rate_limit.rs` is HTTP-agnostic.
The `http::HeaderMap` / `Request<B>` types stay at the call site
(`admin.rs`) which is already inside `quota-router-core` (Layer C)
and has the existing `http`/`hyper` optional deps. The limiter takes
plain `&str` for method/path and an `AdminIdentity` struct the
caller populates from headers. This avoids creating a `http` dep on
the limiter module itself.

**Reuse:** the limiter reuses the existing `RateLimiter` RPM
enforcement from `crate::rate_limit`. One `RateLimiter` per
(route-key, dimension) bucket is created lazily on first request;
the bucket count is bounded by `enabled_routes ×
unique_identities_per_minute`. Old buckets expire with the underlying
`RateLimiter` 60s window.

**HTTP convention:** the middleware surfaces `retry_after_secs` as
part of `AdminRateLimitOutcome::Blocked { retry_after_secs, reason }`.
The caller (admin.rs dispatch hook) translates this into
`429 Too Many Requests` + `Retry-After: {retry_after_secs}` header.
The shape is intentionally RFC-7231 §7.1.3-compatible.

**Anonymous fallback:** if the caller does not populate the identity
field required by the policy (e.g. no API key on `POST /prompts`),
`AdminIdentity::resolve()` falls back to `"anonymous"`. This is a
single shared bucket — it does NOT bypass the limiter. Anonymous
bursts will exhaust the bucket and trigger a 429. Operators who want
to allow unauthenticated bursts must put those routes on a no-limit
policy (e.g. by adding them to the unrate-limited-route set).

**Files touched:**
- `crates/quota-router-core/src/admin_rate_limit.rs` (NEW — 528
  lines incl. tests)
- `crates/quota-router-core/src/lib.rs` — register
  `pub mod admin_rate_limit;` alphabetically

**Tests added:**
- `admin_rate_limit::tests::per_ip_limit_blocks_after_threshold`
- `admin_rate_limit::tests::per_user_limit_independent_buckets`
- `admin_rate_limit::tests::per_api_key_limit_with_first_8_chars`
- `admin_rate_limit::tests::cross_endpoint_breaches_independent`
- `admin_rate_limit::tests::unrate_limited_route_passes_through`
- `admin_rate_limit::tests::disabled_limiter_passes_everything`
- `admin_rate_limit::tests::retry_after_header_present_on_block`
- `admin_rate_limit::tests::identity_resolve_uses_dimension_specific_field`
- `admin_rate_limit::tests::prompts_versions_route_uses_distinct_policy`
- `admin_rate_limit::tests::policy_for_prompts_crud_methods`

10/10 admin_rate_limit tests pass. Clippy clean with `-D warnings`.

## Cross-references

- RFC-0933 §Admin endpoints rate limits
- RFC-0948 §Prompt Management
- RFC-0949 §Enterprise SSO
- Mission `0948-b1-dispatch-hook` (DEFERRED, follow-on)
- Mission `0948-b1-server-config` (DEFERRED, follow-on)
- Mission `reputation-compat-controller-id-test-drift` (DEFERRED
  test fixup unrelated to #102 diff)

## Version History

| Version | Date       | Status  | Change                                                                                                                                                                                                                                                                                                                                                  |
| ------- | ---------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | open    | Mission filed. 0948-b drift closure follow-on. 13 ACs.                                                                                                                                                                                                                                                                                                  |
| v0.2    | 2026-08-13 | closed  | Substrate LANDED. `AdminRateLimitPolicySet` (11 routes) + `AdminLimitDimension` + `AdminIdentity` + `AdminRateLimiter` + `check_admin_rate_limit` middleware. HTTP-agnostic (callers in admin.rs wire headers → `AdminIdentity`). 10 unit tests pass, clippy clean. 3 follow-ons filed: dispatch hook into admin.rs, 429 + Retry-After response, ServerConfig plumbing. |
