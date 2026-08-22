---
title: "RFC-0939: Function Calling & Tool Use"
status: Draft
version: 0.1.0
created: 2026-05-16
updated: 2026-05-16
authors:
  - quota-router team
related:
  - RFC-0917 (Dual-Mode Query Router)
  - RFC-0920 (Unified Python SDK)
---

# RFC-0939: Function Calling & Tool Use

## Status

Draft

## Summary

Extend Message, HttpCompletionRequest, HttpCompletionResponse, and ChatCompletionChunk types to support OpenAI-compatible function calling and tool use across all providers.

## Motivation

Function calling is table stakes for modern LLM applications. The current `Message` struct only has `{role, content}`, which breaks function calling, tool use, structured output, and seed-based reproducibility. Without this, quota-router cannot proxy most real-world requests.

## Specification

### New Types

```rust
/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub r#type: String,  // "function"
    pub function: FunctionDefinition,
}

/// Function definition within a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,  // JSON Schema
}

/// Tool call in assistant response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,  // "function"
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,  // JSON string
}

/// Tool choice for request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    String(String),  // "none", "auto", "required"
    Specific(SpecificToolChoice),
}

/// Specific tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificToolChoice {
    pub r#type: String,  // "function"
    pub function: FunctionName,
}

/// Function name for specific tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionName {
    pub name: String,
}

/// Response format specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    pub r#type: String,  // "text", "json_object", "json_schema"
    pub json_schema: Option<serde_json::Value>,
}
```

### Extended Message

```rust
pub struct Message {
    pub role: String,
    pub content: Option<String>,  // Now optional (may be null for tool_calls)
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub function_call: Option<FunctionCall>,  // Legacy format
}
```

### Extended HttpCompletionRequest

```rust
pub struct HttpCompletionRequest {
    // ... existing fields ...
    pub tools: Option<Vec<Tool>>,
    pub tool_choice: Option<ToolChoice>,
    pub response_format: Option<ResponseFormat>,
    pub seed: Option<i64>,
    pub logprobs: Option<bool>,
    pub top_logprobs: Option<usize>,
    pub parallel_tool_calls: Option<bool>,
}
```

### Extended Choice

```rust
pub struct Choice {
    pub index: usize,
    pub message: Message,
    pub finish_reason: Option<String>,
    pub logprobs: Option<LogProbs>,
}

pub struct LogProbs {
    pub content: Option<Vec<TokenLogProb>>,
}

pub struct TokenLogProb {
    pub token: String,
    pub logprob: f64,
    pub top_logprobs: Option<HashMap<String, f64>>,
}
```

### Provider Format Conversion

| Provider | Tool Format | Notes |
|----------|-------------|-------|
| OpenAI | Native | Pass through directly |
| Mistral | OpenAI-compatible | Pass through |
| Groq | OpenAI-compatible | Pass through |
| Together | OpenAI-compatible | Pass through |
| Ollama | OpenAI-compatible | Pass through |
| Azure | OpenAI-compatible | Pass through |
| Anthropic | Different schema | Convert: tools[].function -> tools[].input_schema |
| Gemini | Different schema | Convert: tools[].function -> function_declarations[] |
| Bedrock | Converse API | Convert: tools[].function -> toolConfig.tools[].toolSpec |
| Replicate | N/A | May not support function calling |

### Backward Compatibility

All new fields are `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]`. Existing requests and responses work unchanged.

## Acceptance Criteria

- [ ] Message has optional `tool_calls`, `tool_call_id`, `function_call`
- [ ] HttpCompletionRequest has `tools`, `tool_choice`, `response_format`, `seed`
- [ ] Choice.message can contain tool_calls in response
- [ ] ChatCompletionChunk.delta can contain tool_calls for streaming
- [ ] Function calling works end-to-end for OpenAI
- [ ] Function calling works end-to-end for Anthropic (with format conversion)
- [ ] All existing tests pass (backward compatible)

## Version History

| Version | Date | Change |
|---------|------|--------|
| 0.1.0 | 2026-05-16 | Initial Draft — function calling types + Message/HttpCompletionRequest extensions + provider format conversion matrix (per Phase 5 R6 closure of F-P5.3-2 actionable surface) |

## References

- OpenAI Function Calling: https://platform.openai.com/docs/guides/function-calling
- Anthropic Tool Use: https://docs.anthropic.com/en/docs/tool-use
- Gemini Function Calling: https://ai.google.dev/gemini-api/docs/function-calling
