---
title: "RFC-0941: Streaming Parity Across All Providers"
status: Draft
version: 0.1.0
created: 2026-05-16
updated: 2026-05-16
authors:
  - quota-router team
related:
  - RFC-0917 (Dual-Mode Query Router)
  - RFC-0940 (any-llm-Mode HTTP Proxy Parity)
---

# RFC-0941: Streaming Parity Across All Providers

## Status

Draft

## Summary

Add streaming support to all 10 native_http providers and the PyBridgeProvider trait.

## Motivation

Only 2 of 10 native_http providers support streaming (OpenAI, Anthropic). This breaks streaming for users of Groq, Together, Ollama, Mistral, Azure, Gemini, Bedrock, and Replicate.

## Specification

### Provider Streaming Compatibility

| Provider | SSE Format | Implementation |
|----------|------------|----------------|
| OpenAI | OpenAI SSE | DONE |
| Anthropic | Anthropic SSE | DONE |
| Groq | OpenAI SSE | Reuse OpenAI streaming |
| Together | OpenAI SSE | Reuse OpenAI streaming |
| Ollama | OpenAI SSE | Reuse OpenAI streaming |
| Mistral | OpenAI SSE | Reuse OpenAI streaming |
| Azure | OpenAI SSE | Reuse OpenAI streaming |
| Gemini | Google SSE | Custom parser |
| Bedrock | AWS EventStream | Custom parser |
| Replicate | Replicate SSE | Custom parser |

### Implementation Strategy

For OpenAI-compatible providers (Groq, Together, Ollama, Mistral, Azure):
1. Add `supports_streaming() -> true`
2. Implement `streaming_completion()` using the same SSE parsing as OpenAI
3. Extract SSE parsing into a shared helper function

### SSE Parsing Helper

```rust
/// Parse OpenAI-compatible SSE stream
pub async fn parse_openai_sse(
    response: reqwest::Response,
    tx: mpsc::Sender<ChatCompletionChunk>,
) -> Result<(), ProviderError> {
    // Parse SSE lines: "data: {json}\n\n"
    // Send each chunk via tx
    // Handle "data: [DONE]" termination
}
```

## Acceptance Criteria

- [ ] All 10 native_http providers return `supports_streaming() -> true`
- [ ] OpenAI-compatible providers share SSE parsing code
- [ ] Streaming works end-to-end for Groq, Together, Ollama, Mistral, Azure
- [ ] Gemini, Bedrock, Replicate have custom streaming parsers
- [ ] PyBridgeProvider trait has `streaming_completion()` method
- [ ] All existing tests pass

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-05-16 | Initial draft |
