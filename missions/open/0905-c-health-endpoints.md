# Mission: 0905-c — Health Endpoints

## Status

Open

## RFC

RFC-0905 (Economics): Observability and Logging

## Dependencies

None

## Acceptance Criteria

- [ ] Implement `GET /healthz` — liveness probe (always 200 OK if process running)
- [ ] Implement `GET /healthz/ready` — readiness probe (checks dependencies)
- [ ] Liveness response format: `{"status": "ok", "timestamp": "...", "version": "..."}`
- [ ] Readiness response format: `{"status": "ok"|"degraded"|"unhealthy", "checks": {...}}`
- [ ] Readiness checks: database connectivity, provider availability, memory usage
- [ ] Return 200 for healthy/degraded, 503 for unhealthy
- [ ] K8s-compatible probe semantics
- [ ] Add `HealthConfig` to `config.rs` (enabled, port, checks)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/health.rs` — New
- `crates/quota-router-core/src/config.rs` — Add HealthConfig
- `crates/quota-router-core/src/proxy.rs` — Register health routes
