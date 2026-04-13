# Mission: RFC-0903 Phase 3 — Route Permission Checking + Path Normalization

## Status

Open

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Summary

Implement route authorization checking (`check_route_permission()`) and path normalization (`normalize_path()`) to prevent authorization bypass attacks. RFC-0903 requires that all route authorization checks operate on normalized paths to prevent bypasses like `/v1/chat/../management`.

## Motivation

Security-critical gaps exist in the current authorization flow:
1. **Path normalization missing:** A request to `/v1/chat/../management` could bypass authorization by traversing up the path tree
2. **Route permission checking missing:** `check_route_permission()` is defined in RFC-0903 but not implemented
3. **Double-encoded path rejection missing:** Attackers may use `%2e%2e` or `%252e%252e` for bypass attempts

## Dependencies

- Mission: 0903-b-key-management-rest-api (depends on ApiKey model stability)

## Acceptance Criteria

- [ ] **normalize_path():** Implement path normalization with security checks:
  ```rust
  fn normalize_path(path: &str) -> Result<String, ()>
  ```
  - Decode percent encoding first
  - Split by `/` and process segments: skip `.`, pop on `..`
  - **Reject double-encoded sequences:** %252E, %252F, %25. or %25/ in any combination
  - Return `Err(())` on security violation, `Ok(normalized_path)` on success
- [ ] **check_route_permission():** Implement route authorization per key_type and allowed_routes:
  ```rust
  pub fn check_route_permission(key: &ApiKey, route: &str) -> bool
  ```
  - Normalize path BEFORE checking (prevent bypass)
  - Check explicit `allowed_routes` first (JSON array format: `["\/v1\/chat","\/v1\/embeddings"]`)
  - Fall back to key_type defaults:
    - `LlmApi`: `/v1/chat`, `/v1/completions`, `/v1/embeddings` and subroutes
    - `Management`: `/key/`, `/team/`, `/user/` routes
    - `ReadOnly`: `/models/`, `/info` routes
    - `Default`: allow all
  - Enforce trailing slash or exact match for allowed_routes
- [ ] **Integration:** Wire normalize_path into middleware validation flow:
  - `validate_request_key()` → `check_route_permission()` → reject if false
  - Path normalization errors should reject the request (not silently pass)
- [ ] **Security tests:**
  - Bypass attempt: `/v1/chat/../management` → rejected
  - Bypass attempt: `/v1/chat/%2e%2e/management` → rejected
  - Bypass attempt: double-encoded → rejected
  - Valid routes: `/v1/chat/completions` → accepted for LlmApi key
  - Valid routes: `/team/list` → accepted for Management key

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/keys/mod.rs` | Add normalize_path, check_route_permission functions |
| `crates/quota-router-core/src/middleware.rs` | Integrate route permission checking into validation flow |
| `crates/quota-router-core/src/keys/models.rs` | Ensure KeyType variants match RFC-0903 definition |

## Complexity

Low-Medium — focused security hardening with clear attack vectors to test

## Reference

- RFC-0903 §Authorization Route Mapping (lines 741-788)
- RFC-0903 §Route Normalization (lines 1187-1222)
- RFC-0903 §Prefix Usage (lines 2114-2117)
