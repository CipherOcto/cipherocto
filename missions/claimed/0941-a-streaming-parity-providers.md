# Mission: 0941-a — Streaming Parity Across Providers

## Status

Open

## RFC

RFC-0941 (Economics): Streaming Parity Across All Providers

## Dependencies

- Mission 1.1: Function Calling Types (COMPLETE — d9a6a6b)

## Context

Only 2/10 native_http providers support streaming (OpenAI, Anthropic). This mission adds streaming to the remaining 8 providers.

## Acceptance Criteria

### OpenAI-Compatible Providers (Groq, Together, Ollama, Mistral, Azure)

- [ ] Extract shared SSE parsing into helper function
- [ ] Add `supports_streaming() -> true` to each provider
- [ ] Implement `streaming_completion()` using shared parser
- [ ] Test streaming end-to-end

### Other Providers (Gemini, Bedrock, Replicate)

- [ ] Implement custom SSE parsers for each format
- [ ] Add `supports_streaming() -> true`
- [ ] Implement `streaming_completion()`

## Files to Modify

- `crates/quota-router-core/src/native_http/mod.rs` — add shared SSE parsing helper
- `crates/quota-router-core/src/native_http/groq.rs` — add streaming
- `crates/quota-router-core/src/native_http/together.rs` — add streaming
- `crates/quota-router-core/src/native_http/ollama.rs` — add streaming
- `crates/quota-router-core/src/native_http/mistral.rs` — add streaming
- `crates/quota-router-core/src/native_http/azure.rs` — add streaming
- `crates/quota-router-core/src/native_http/gemini.rs` — add streaming
- `crates/quota-router-core/src/native_http/bedrock.rs` — add streaming
- `crates/quota-router-core/src/native_http/replicate.rs` — add streaming

## Verification

```bash
cargo test -p quota-router-core --lib
cargo clippy -p quota-router-core -- -D warnings
cargo fmt -- --check
```
