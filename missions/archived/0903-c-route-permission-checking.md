# Mission: RFC-0903 Phase 3 — Route Permission Checking + Path Normalization

## Status

Completed

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Summary

Implemented route authorization checking (`check_route_permission()`) and path normalization (`normalize_path()`) to prevent authorization bypass attacks per RFC-0903 §Authorization Route Mapping and §Route Normalization.

## Implementation

### normalize_path()

Per RFC-0903 §Route Normalization — prevents path traversal bypass attacks:

- Rejects double-encoded sequences (%252E, %252F, %25., %25/)
- Decodes percent encoding before normalization
- Processes path segments: skip `.`, pop on `..`
- Returns `Err(())` on security violation, `Ok(normalized_path)` on success

### check_route_permission()

Per RFC-0903 §Authorization Route Mapping — enforces key_type-based access:

- Normalizes path BEFORE checking (prevents bypass)
- Checks explicit `allowed_routes` first (JSON array format)
- Falls back to key_type defaults:
  - `LlmApi`: `/v1/chat`, `/v1/completions`, `/v1/embeddings` and subroutes
  - `Management`: `/key/`, `/team/`, `/user/` routes
  - `ReadOnly`: `/models/`, `/info` routes
  - `Default`: allow all
- Enforces trailing slash or exact match for allowed_routes

### Middleware Integration

`validate_request_key_for_route()` in `middleware.rs` combines key validation with route authorization in one call. Returns `KeyError::RouteNotAllowed` if route permission check fails.

## Acceptance Criteria

- [x] **normalize_path():** Rejects double-encoding, decodes percent encoding, normalizes path segments
- [x] **check_route_permission():** Enforces key_type defaults, supports explicit allowed_routes
- [x] **Integration:** `validate_request_key_for_route()` in middleware
- [x] **Security tests:** 16 tests covering all bypass vectors

## Security Tests

- `/v1/chat/../management` → normalized to `/v1/management` (bypass blocked)
- `/v1/chat/%2e%2e/management` → normalized to `/v1/chat/../management` → `/v1/management` (bypass blocked)
- `/v1/chat/%252e%252e/management` → **REJECTED** (double encoding)
- `/v1/chat/%252Fadmin` → **REJECTED** (double encoding)
- `/v1/chat/%25./admin` → **REJECTED** (partial double encoding)

## Key Files Modified

| File | Change |
|------|--------|
| `crates/quota-router-core/src/keys/mod.rs` | normalize_path, check_route_permission, 16 security tests |
| `crates/quota-router-core/src/keys/errors.rs` | Added RouteNotAllowed variant |
| `crates/quota-router-core/src/middleware.rs` | validate_request_key_for_route() |
| `crates/quota-router-core/src/lib.rs` | Export new functions |
| `crates/quota-router-core/Cargo.toml` | Added percent-encoding dependency |

## Reference

- RFC-0903 §Authorization Route Mapping (lines 741-788)
- RFC-0903 §Route Normalization (lines 1187-1222)
- RFC-0903 §Prefix Usage (lines 2114-2117)

## Claimant

@claude-code

## Completed

All acceptance criteria met. Commit 4bc9021.
