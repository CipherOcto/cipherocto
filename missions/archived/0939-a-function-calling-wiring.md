# Mission: 0939-a — Wire Function Calling Through All Providers

## Status

Open

## RFC

RFC-0939 (Economics): Function Calling & Tool Use

## Dependencies

- Mission 1.1: Function Calling Types (COMPLETE — d9a6a6b)
- Mission 0941-a: Streaming Parity (COMPLETE — 92a7936)

## Context

The types for function calling were added in Mission 1.1 (Tool, ToolCall, ToolChoice, etc.), but the providers don't pass these fields through to the actual API calls. This mission wires function calling through all 10 native_http providers.

## Acceptance Criteria

### OpenAI-Compatible Providers (OpenAI, Groq, Together, Ollama, Mistral, Azure)

- [ ] Pass `tools`, `tool_choice`, `response_format`, `seed` in completion request body
- [ ] Pass `tools`, `tool_choice` in streaming request body
- [ ] Parse `tool_calls` from response

### Anthropic

- [ ] Convert OpenAI tool format to Anthropic format
- [ ] Pass tools in completion request body
- [ ] Parse tool_use from response

### Other Providers (Gemini, Bedrock, Replicate)

- [ ] Pass tools if supported
- [ ] Handle gracefully if not supported (ignore tools field)

## Files to Modify

- `crates/quota-router-core/src/native_http/openai.rs` — pass tools/tool_choice
- `crates/quota-router-core/src/native_http/groq.rs` — pass tools/tool_choice
- `crates/quota-router-core/src/native_http/together.rs` — pass tools/tool_choice
- `crates/quota-router-core/src/native_http/ollama.rs` — pass tools/tool_choice
- `crates/quota-router-core/src/native_http/mistral.rs` — pass tools/tool_choice
- `crates/quota-router-core/src/native_http/azure.rs` — pass tools/tool_choice
- `crates/quota-router-core/src/native_http/anthropic.rs` — convert and pass tools

## Verification

```bash
cargo test -p quota-router-core --lib
cargo clippy -p quota-router-core -- -D warnings
cargo fmt -- --check
```
