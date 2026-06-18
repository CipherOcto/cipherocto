# Mission: 0917-f — Refactor factory.rs to Use PyBridgeProvider Trait Dispatch

## Status

Open

## RFC

RFC-0917 (Economics): Dual-Mode Query Router

## Dependencies

- None (independent refactor)

## Context

`py_bridge::factory::completion()` uses a large match statement to dispatch to 41 providers. Each match arm is boilerplate: create provider, optionally set api_key, optionally set api_base, call completion. The `PyBridgeProvider` trait exists but the factory doesn't use it — it manually calls builder methods per provider.

Verified count: 41 provider match arms in py_bridge/factory.rs (2026-05-16).

Refactoring to trait dispatch would:
- Reduce factory.rs from ~400 lines to ~50 lines
- Make adding new providers a 1-line change (register in map)
- Enable runtime provider registration

## Acceptance Criteria

- [x] `factory::completion()` uses trait object dispatch instead of match arms
- [x] Provider registry maps provider name → factory function returning `Box<dyn PyBridgeProvider>`
- [x] `with_api_key()` and `with_api_base()` called via trait methods (not per-provider builders)
- [x] All 41 providers registered in the registry
- [x] Existing behavior preserved — no API changes
- [x] Clippy passes with zero warnings
- [x] All existing tests pass (294 tests)

## Files to Modify

- `crates/quota-router-core/src/py_bridge/factory.rs` — refactor to trait dispatch
- `crates/quota-router-core/src/py_bridge/mod.rs` — ensure PyBridgeProvider trait has required methods

## Notes

This is a pure refactor — no behavior change. The match-based dispatch is functionally correct but unmaintainable at 41 providers. Trait dispatch is the idiomatic Rust approach.

**Risk:** Low — pure refactor with existing test coverage. But must verify all 41 providers have `with_api_base()` (Mission 0929-c added these).

### M2: Trait Methods

PyBridgeProvider trait needs with_api_key(&self, key: &str) -> Self and with_api_base(&self, base: &str) -> Self methods added. These are builder-pattern methods for per-request configuration.
