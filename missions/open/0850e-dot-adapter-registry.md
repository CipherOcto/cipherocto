# Mission: DOT Adapter Registry & Plugin ABI

## Status

Implemented (registry, plugin ABI, backoff utility)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8 extensions

## Summary

Implement the adapter registry and plugin ABI that enables dynamic loading of platform adapters at runtime. This is the foundation for all platform-specific adapters.

## Design

See `docs/plans/2026-05-28-social-platform-transport-adapters-design.md` — Section 1: Plugin Architecture.

## Acceptance Criteria

- [ ] `AdapterRegistry` struct that scans directories for adapter plugins
- [ ] `cdylib` plugin ABI: `adapter_version()`, `platform_type()`, `create_adapter()`
- [ ] ABI version negotiation — old plugins load with graceful degradation
- [ ] JSON config passed to adapter at construction time
- [ ] Adapter lifecycle: `startup()`, `health_check()`, `shutdown()` with pending message flush
- [ ] `CapabilityReport` cached per adapter after startup
- [ ] Gateway integration: `send_envelope()` dispatches by `platform_type`
- [ ] Retry/backoff: shared exponential backoff utility for all adapters (base=1s, max=120s, jitter)
- [ ] Self-loop prevention: registry tracks adapter identities for `self_handle()` lookup
- [ ] Unit tests for registry discovery, ABI version check, adapter dispatch, retry behavior
- [ ] Integration test with mock adapter `.so`

## Location

`crates/octo-network/src/dot/adapters/registry.rs`, `crates/octo-network/src/dot/adapters/abi.rs`

## Complexity

Medium

## Prerequisites

- Mission 0850: DOT Core Envelope

## Implementation Notes

- Use `libloading` crate for `cdylib` loading
- Adapter config is `serde_json::Value` deserialized from gateway config
- Registry caches loaded adapters in `BTreeMap<PlatformType, Box<dyn PlatformAdapter>>`
- Health check interval configurable (default: 60s)
- Failed adapters log error and remain in registry but marked unhealthy
