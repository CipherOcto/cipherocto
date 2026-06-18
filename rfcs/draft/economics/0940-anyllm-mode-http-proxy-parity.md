---
title: "RFC-0940: any-llm-Mode HTTP Proxy Parity"
status: Draft
version: 0.1.0
created: 2026-05-16
updated: 2026-05-16
authors:
  - quota-router team
related:
  - RFC-0917 (Dual-Mode Query Router)
  - RFC-0920 (Unified Python SDK)
  - RFC-0939 (Function Calling & Tool Use)
---

# RFC-0940: any-llm-Mode HTTP Proxy Parity

## Status

Draft

## Summary

Implement `handle_request_anyllm()` in proxy.rs to provide full HTTP proxy functionality in any-llm-mode builds, using py_bridge for provider dispatch.

## Motivation

The current `handle_request_anyllm()` at proxy.rs:855-869 is a stub that returns HTTP 400 "any-llm-mode proxy not yet implemented". Anyone building with `--features any-llm-mode` gets a completely broken HTTP proxy. This blocks any-llm users from using quota-router as a gateway.

## Specification

### Architecture

```
Client Request (HTTP)
    │
    ▼
proxy.rs::handle_request()
    │
    ├─ litellm-mode: handle_request_litellm() → HttpProviderFactory → reqwest → Provider API
    │
    └─ any-llm-mode: handle_request_anyllm() → py_bridge::factory → PyO3 → Python SDK → Provider API
```

### handle_request_anyllm() Implementation

```rust
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
async fn handle_request_anyllm(
    body_str: &str,
    provider: &Provider,
    api_key: &str,
    dispatch_api_base: Option<&str>,
) -> Result<Response<SseBody>, Infallible> {
    // 1. Parse request body into NativeHttpRequest
    // 2. Extract model string (e.g., "openai/gpt-4o")
    // 3. Call py_bridge::factory::completion() via spawn_blocking (GIL safety)
    // 4. Convert py_bridge response to HTTP JSON response
    // 5. Handle streaming via py_bridge::factory::streaming_completion()
}
```

### PyO3 GIL Safety

All py_bridge calls MUST use `tokio::task::spawn_blocking()` to avoid GIL contention:

```rust
let result = tokio::task::spawn_blocking(move || {
    pyo3::Python::with_gil(|py| {
        py_bridge::factory::completion(py, &model, &request)
    })
}).await?;
```

### Streaming Support

For streaming requests, use a background Python thread:

```rust
let (tx, rx) = tokio::sync::mpsc::channel(32);

tokio::task::spawn_blocking(move || {
    pyo3::Python::with_gil(|py| {
        py_bridge::factory::streaming_completion(py, &model, &request, move |chunk| {
            let _ = tx.blocking_send(chunk);
        })
    })
});

// Convert rx to SseBody
```

### Response Format

The response MUST be OpenAI-compatible JSON:

```json
{
    "id": "chatcmpl-xxx",
    "object": "chat.completion",
    "created": 1234567890,
    "model": "gpt-4o",
    "choices": [{
        "index": 0,
        "message": {
            "role": "assistant",
            "content": "Hello!"
        },
        "finish_reason": "stop"
    }],
    "usage": {
        "prompt_tokens": 10,
        "completion_tokens": 5,
        "total_tokens": 15
    }
}
```

### Error Handling

| py_bridge Error | HTTP Status | Response |
|-----------------|-------------|----------|
| AuthError | 401 | `{"error": "authentication_error"}` |
| RateLimit | 429 | `{"error": "rate_limit_error"}` |
| InvalidRequest | 400 | `{"error": "invalid_request_error"}` |
| ProviderError | 502 | `{"error": "provider_error"}` |
| Timeout | 504 | `{"error": "gateway_timeout"}` |

### Fallback Chain

Fallback support for any-llm-mode:

```rust
if let Some(ref executor) = fallback {
    // Try primary model
    match try_completion_anyllm(&model, &request).await {
        Ok(resp) if resp.status().is_success() => return Ok(resp),
        Err(e) if is_retryable(&e) => {
            // Try fallback models
            for fallback_model in executor.config().get_fallback_models(&model, &e) {
                if let Ok(resp) = try_completion_anyllm(&fallback_model, &request).await {
                    if resp.status().is_success() {
                        return Ok(resp);
                    }
                }
            }
        }
        _ => {}
    }
}
```

## Acceptance Criteria

- [ ] `handle_request_anyllm()` dispatches to py_bridge::factory::completion()
- [ ] Streaming works via py_bridge::factory::streaming_completion()
- [ ] PyO3 calls use spawn_blocking for GIL safety
- [ ] Response format is OpenAI-compatible JSON
- [ ] Error handling maps py_bridge errors to HTTP status codes
- [ ] Fallback chain works in any-llm-mode
- [ ] All existing tests pass
- [ ] Build with `--features any-llm-mode` produces working proxy

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-core/src/proxy.rs` | Implement handle_request_anyllm() |
| `crates/quota-router-core/src/py_bridge/factory.rs` | Add completion() and streaming_completion() |
| `crates/quota-router-core/src/py_bridge/mod.rs` | Update PyBridgeProvider trait |

## Risks

1. **PyO3 GIL contention** — Mitigated by spawn_blocking
2. **Streaming over PyO3** — Background thread with mpsc channel
3. **Provider format divergence** — py_bridge handles format conversion internally

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-05-16 | Initial draft |
