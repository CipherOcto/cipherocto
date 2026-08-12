# Mission: 0905-c — Health Endpoints

## Status

claimed 2026-08-11 (@claude).

**Substrate:** `crates/quota-router-core/src/health.rs` fully implements
`HealthConfig` + `HealthHandler` + `DependencyChecker` + `DefaultDependencyChecker`
+ 7 unit tests covering liveness/readiness/dependency semantics. The gap
was route registration in `proxy.rs::handle_request`.

## Summary

Register K8s-compatible liveness and readiness probes (`/healthz` and
`/healthz/ready`) in `proxy.rs::handle_request`. The existing
`HealthHandler` returns the spec-compliant response shapes; the wiring
picks them up at request routing time.

## What landed

- [x] `crates/quota-router-core/src/proxy.rs` — added `/healthz` (liveness)
  and `/healthz/ready` (readiness) route handlers in `handle_request`.
  Legacy `/health` and `/ready` paths preserved unchanged.
- [x] `HealthHandler` instantiated with `DefaultDependencyChecker` (returns
  Ok for all checks; reserved for future real dependency checks).
- [x] Status code mapping: 200 for Ok/Degraded, 503 for Unhealthy.
- [x] JSON `content-type` header on healthz responses.
- [x] 2 new proxy.rs integration tests:
  - `test_handle_request_healthz_liveness` — asserts 200 + JSON content-type
  - `test_handle_request_healthz_ready_ok` — asserts 200 with default checker
- [x] 7 pre-existing health.rs unit tests still pass (no regression).
- [x] 2 pre-existing admin.rs tests `test_route_get_healthz` and
  `test_route_get_healthz_ready` also pass (admin module already had
  K8s-compatible coverage).

## Acceptance Criteria

- [x] GET /healthz — liveness probe (always 200 OK if process running)
- [x] GET /healthz/ready — readiness probe (200 OK default; 503 on Unhealthy)
- [x] Liveness response format: `{"status": "ok"}`
- [x] Readiness response format: `{"status": "ok"|"degraded"|"unhealthy", "checks": {...}}`
- [x] Readiness checks: stoolsap, config, providers (via `DependencyChecker`)
- [x] 200 for healthy/degraded, 503 for unhealthy
- [x] K8s-compatible probe semantics
- [x] HealthConfig in `config.rs` (canonical entry: `pub health: HealthConfig`)
- [x] `cargo test -p quota-router-core --lib` green (1557/1557)
- [x] `cargo clippy -p quota-router-core --all-targets -- -D warnings` clean
- [x] `cargo fmt --all` clean

## Implementation Notes

- `HealthHandler::handle_liveness` returns `(200, body)` — always Ok.
- `HealthHandler::handle_readiness` returns `(200|503, body)` based on
  the worst-case check across `stoolap`, `config`, `providers`.
- `StatusCode::from_u16(status)` converts the handler's `u16` return to
  `StatusCode`; falls back to `500 INTERNAL_SERVER_ERROR` for invalid
  codes (defensive — handler only returns 200 or 503).
- Legacy `/health` and `/ready` paths split into separate `if` blocks
  (was previously `if path == "/health" || path == "/ready"`) so the
  K8s probe semantics can evolve independently without breaking existing
  operators.

## Cross-references

- RFC-0905 (Economics): Observability and Logging
- `crates/quota-router-core/src/health.rs` — handler implementation
- `crates/quota-router-core/src/proxy.rs` — route registration
- `crates/quota-router-core/src/admin.rs` — pre-existing admin coverage
- `crates/quota-router-core/src/config.rs` — `HealthConfig` re-export

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-11 | claimed  | Mission moved `open/` → `claimed/`; route registration landing |
| v0.2    | 2026-08-11 | closed   | LANDED 2026-08-11. 1557/1557 lib tests pass; clippy + fmt clean |
