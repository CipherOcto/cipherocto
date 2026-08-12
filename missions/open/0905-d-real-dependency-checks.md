# Mission: 0905-d — Real Dependency Checks for `/healthz/ready`

## Status

Open (filed 2026-08-12). Follow-on to `0905-c-health-endpoints` (LANDED 2026-08-11).

## Summary

`crates/quota-router-core/src/health.rs::DefaultDependencyChecker` returns `Ok` for all
dependencies (stoolap, config, providers). This is a placeholder per the 0905-c
checklist note "reserved for future real dependency checks". This mission lands
real per-dependency health probes.

## Substrate (already shipped)

- `crates/quota-router-core/src/health.rs` — `DependencyChecker` trait + `DefaultDependencyChecker`
  placeholder + `HealthHandler::handle_readiness` aggregator (worst-case across checks).
- 7 pre-existing unit tests pin the current `Ok`-always behaviour.
- 2 proxy.rs integration tests verify the 200/503 status code mapping.

## Deferred work (explicit, not unspecified)

The 0905-c checklist flagged "reserved for future real dependency checks" without
naming an owner. This mission files the follow-up per [[deferred-vs-unspecified]].

## Scope

| AC | Description |
|----|-------------|
| AC-1 | `StoolapDependencyChecker` — executes `SELECT 1` against the configured stoolap connection; 200ms timeout per RFC-0905 §Healthcheck |
| AC-2 | `ConfigDependencyChecker` — re-parses `Config::load()` and validates required keys; 50ms timeout |
| AC-3 | `ProvidersDependencyChecker` — checks provider registry consistency (no empty registry); 50ms timeout |
| AC-4 | `CompositeDependencyChecker` — runs all three concurrently (`tokio::join!`), aggregates worst-case |
| AC-5 | Default constructor in `proxy.rs::handle_request` swaps `DefaultDependencyChecker` → `CompositeDependencyChecker` |
| AC-6 | Tests: 3 unit tests per checker (success/timeout/error) + 1 integration test verifying 503 on stoolap unreachable |
| AC-7 | `cargo clippy -p quota-router-core --all-targets -- -D warnings` clean |
| AC-8 | `cargo fmt --all -- --check` clean |

## Out of Scope

- K8s liveness `/healthz` (always Ok) — unchanged from 0905-c
- Readiness probe path/route registration — unchanged
- HTTP proxy retries + circuit breaker around dependency checks — separate mission if needed

## Cross-references

- RFC-0905 (Economics): Observability and Logging — §Healthcheck spec
- Mission `0905-c-health-endpoints` (LANDED 2026-08-11) — substrate
- `crates/quota-router-core/src/health.rs` — DependencyChecker trait
- `crates/quota-router-core/src/proxy.rs` — handle_request route registration

## Layer Discipline

- Touches `quota-router-core` (Layer B) only — no extension crates needed.
- No new Cargo deps; `tokio::join!` already in workspace.

## Version History

| Version | Date       | Status | Changes |
|---------|------------|--------|---------|
| v0.1    | 2026-08-12 | open   | Mission filed (follow-on to 0905-c-health-endpoints per [[deferred-vs-unspecified]]) |
