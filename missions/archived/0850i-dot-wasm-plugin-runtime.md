# Mission: DOT WASM Plugin Runtime

## Status

Implemented (10 tests, wasmtime runtime, host functions, ABI validation)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.3

## Summary

Implement a WASM plugin runtime that loads community-contributed platform adapters from `.wasm` files with sandboxed execution, enabling third parties to add transport adapters without modifying CipherOcto core.

## Design

See `docs/plans/2026-05-28-social-platform-transport-adapters-design.md` — WASM Plugin API.

## Acceptance Criteria

- [ ] `WasmAdapterRuntime` struct using `wasmtime` for WASM execution
- [ ] Host functions: `http_request`, `log`, `current_epoch`
- [ ] `http_request` enforces TLS-only, configurable domain allowlists
- [ ] WASM adapter exports: `adapter_version`, `platform_type`, `create`, `destroy`, `send`, `receive`
- [ ] Config passed as JSON blob to `create()` in WASM linear memory
- [ ] Memory isolation: each adapter gets its own WASM instance
- [ ] Resource limits: max memory (16MB), max execution time (5s per call)
- [ ] Integration with `AdapterRegistry`: WASM adapters loaded alongside cdylib
- [ ] Unit tests with a minimal test WASM adapter
- [ ] Documentation: WASM adapter development guide

## Location

`crates/octo-network/src/dot/adapters/wasm_runtime.rs`

## Complexity

High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- Use `wasmtime` crate (not `wasmer`) for WASM execution
- Host function `http_request` uses `reqwest` under the hood — adapter cannot make raw TCP connections
- Domain allowlist: config-driven, defaults to empty (no HTTP allowed unless explicitly configured)
- WASM adapter memory: linear memory with pointer+length convention for passing data
- Error handling: WASM traps (out of memory, timeout) are caught and reported as adapter errors
- ABI version: WASM adapters use same version number as cdylib ABI
- Serialization: JSON for config, raw bytes for envelope data
