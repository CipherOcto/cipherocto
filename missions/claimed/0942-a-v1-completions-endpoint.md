# Mission: 0942-a — /v1/completions Endpoint

## Status

Complete

Open

## RFC

RFC-0942 (Economics): Additional API Endpoints

## Dependencies

- Mission 0940-a: any-llm-mode Proxy (COMPLETE — 7cdaf63)

## Context

LiteLLM supports the legacy `/v1/completions` endpoint for text completions. This mission adds path-based routing for this endpoint.

## Acceptance Criteria

- [x] Add `/v1/completions` path routing in handle_request
- [x] Parse legacy completion request (model, prompt, max_tokens, temperature, etc.)
- [x] Convert to chat completion format (prompt → user message)
- [x] Forward to provider via existing completion path
- [x] Return OpenAI-compatible response format

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add path routing and handler

## Verification

```bash
cargo test -p quota-router-core --lib
cargo clippy -p quota-router-core -- -D warnings
cargo fmt -- --check
```
