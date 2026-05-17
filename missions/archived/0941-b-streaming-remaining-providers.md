# Mission: 0941-b — Streaming for Remaining Providers

## Status

Open

## RFC

RFC-0941 (Economics): Streaming Parity Across All Providers

## Context

Gemini, Bedrock, and Replicate don't support streaming yet.

## Acceptance Criteria

- [ ] Add streaming to Gemini provider
- [ ] Add streaming to Bedrock provider
- [ ] Add streaming to Replicate provider
- [ ] All 10 native_http providers support streaming

## Files to Modify

- `crates/quota-router-core/src/native_http/gemini.rs` — add streaming
- `crates/quota-router-core/src/native_http/bedrock.rs` — add streaming
- `crates/quota-router-core/src/native_http/replicate.rs` — add streaming
