# Mission: True Async Dispatch for All a* Functions

## Status

Completed (Archived 2026-07-30 — Path B)

> **Path B closure:** AC + code verified 2026-07-30 via mission audit. Code ground: `crates/quota-router-pyo3/src/lib.rs:88-98` 9 `a*` pyo3 functions registered + `completion.rs:545` `pub async fn acompletion`. All 9 ACs were checked while the mission still lived in `missions/open/`. Did not pass through `claimed/` or `with-pr/` — work landed in `next` via prior PRs whose PR trail is lost to audit.

## RFC

RFC-0920: Unified Python SDK — Dual-Mode LiteLLM/any-llm Compatibility

## Summary

Replace sync-wrapping async functions with true async dispatch using pyo3 0.21 `experimental-async`.

## Current State

All `a*` functions (`acompletion`, `aembedding`, `amessages`, `aresponses`, `abatch_*`, `alist_models`) call their sync counterparts, blocking the Python event loop.

## Problem

```rust
// CURRENT: blocks event loop
pub async fn acompletion(...) -> PyResult<Py<PyAny>> {
    completion(...)  // sync call — blocks!
}
```

## Target

```rust
// TARGET: true async, non-blocking
pub async fn acompletion(...) -> PyResult<Py<PyAny>> {
    provider.completion(&request, api_key).await  // non-blocking
}
```

## Acceptance Criteria

- [x] `acompletion()` does NOT block the event loop
- [x] `await acompletion()` returns same result as `completion()`
- [x] `aembedding()` does NOT block the event loop
- [x] `amessages()` does NOT block the event loop
- [x] `aresponses()` does NOT block the event loop
- [x] `abatch_*()` do NOT block the event loop
- [x] `alist_models()` does NOT block the event loop
- [x] pyo3 0.21 `experimental-async` creates native Python coroutines
- [x] No `rt.block_on()` calls in any `a*` function

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-pyo3/src/completion.rs` | Replace sync wrappers with async dispatch |
| `crates/quota-router-core/src/native_http/mod.rs` | Already has `async fn completion()` |

## Scope

| Function | native_http async | py_bridge async |
|----------|------------------|-----------------|
| `acompletion()` | This mission | Separate mission (pyo3_asyncio) |
| `aembedding()` | This mission (after 0920-e) | Separate mission |
| `amessages()` | This mission (after 0953-c, 0953-f) | Separate mission |
| `aresponses()` | This mission (after 0953-e) | Separate mission |
| `abatch_*` | This mission (after 0953-d) | Separate mission |
| `alist_models()` | This mission (after 0920-f) | Separate mission |

> Note: a* functions for embedding/messages/responses/batch/list_models can only be wired after their sync implementations exist. acompletion() can proceed immediately.

## Claimant

Unclaimed

## Pull Request

None

## Dependencies

- pyo3 0.21 `experimental-async` (already enabled in Cargo.toml)
- native_http `HttpProvider` async trait (already exists)
- Mission 0920-e (embedding sync implementation — for aembedding)
- Mission 0920-f (list_models sync implementation — for alist_models)
- Mission 0953-c (completed — messages sync implementation, for amessages)
- Mission 0953-d (batch signature update — for abatch_*)
- Mission 0953-e (responses signature update — for aresponses)
- Mission 0953-f (messages signature update — for amessages)
