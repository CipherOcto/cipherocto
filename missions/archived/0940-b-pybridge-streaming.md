# Mission: 0940-b — Add Streaming to PyBridgeProvider

## Status

Complete

Open

## RFC

RFC-0940 (Economics): any-llm-Mode HTTP Proxy Parity

## Dependencies

- Mission 0940-a: any-llm-mode Proxy (COMPLETE — 7cdaf63)

## Context

The PyBridgeProvider trait is sync-only with no streaming method. This mission adds async streaming support to the trait and implements it for all py_bridge providers.

## Acceptance Criteria

### Trait Extension

- [x] Add `streaming_completion()` method to PyBridgeProvider trait
- [x] Method returns a channel receiver for streaming chunks
- [x] Use `tokio::sync::mpsc::channel` for async streaming

### Implementation

- [x] Implement streaming for OpenAI py_bridge provider
- [x] Implement streaming for other py_bridge providers (or stub with error)
- [x] Wire streaming into handle_request_anyllm()

## Files to Modify

- `crates/quota-router-core/src/py_bridge/openai.rs` — extend trait and implement
- `crates/quota-router-core/src/py_bridge/factory.rs` — add streaming dispatch
- `crates/quota-router-core/src/proxy.rs` — wire streaming in any-llm-mode

## Verification

```bash
cargo test -p quota-router-core --lib
cargo clippy -p quota-router-core -- -D warnings
cargo fmt -- --check
```
