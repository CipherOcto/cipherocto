# Mission: 0940-a — any-llm-Mode HTTP Proxy Implementation

## Status

Complete

Open

## RFC

RFC-0940 (Economics): any-llm-Mode HTTP Proxy Parity

## Dependencies

- Mission-0939-a: Function Calling Types (COMPLETE — d9a6a6b)

## Context

The current `handle_request_anyllm()` in proxy.rs is a stub that returns HTTP 400. This mission implements the full any-llm-mode HTTP proxy using py_bridge for provider dispatch.

## Acceptance Criteria

### Core Implementation

- [x] Replace `handle_request_anyllm()` stub with py_bridge::factory::completion() dispatch
- [x] Parse model string (e.g., "openai/gpt-4o") to extract provider
- [x] Call py_bridge via tokio::task::spawn_blocking for GIL safety
- [x] Convert py_bridge response to OpenAI-compatible JSON
- [x] Handle all error types with appropriate HTTP status codes

### Streaming

- [x] Implement streaming via py_bridge::factory::streaming_completion()
- [x] Use background Python thread with mpsc channel
- [x] Convert chunks to SSE format

### Fallback

- [x] Wire fallback chain for any-llm-mode
- [x] Use classify_http_error() for error classification
- [x] Implement try_fallback_models_anyllm()

### Error Handling

- [x] Map py_bridge::PyBridgeError to HTTP status codes
- [x] Return structured JSON error responses
- [x] Handle PyO3 exceptions gracefully

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — implement handle_request_anyllm()
- `crates/quota-router-core/src/py_bridge/factory.rs` — add completion() and streaming_completion()
- `crates/quota-router-core/src/py_bridge/mod.rs` — update PyBridgeProvider trait

## Verification

```bash
cargo test -p quota-router-core --lib
cargo clippy -p quota-router-core -- -D warnings
cargo fmt -- --check
# Build with any-llm-mode feature
cargo build -p quota-router-core --features any-llm-mode
```
