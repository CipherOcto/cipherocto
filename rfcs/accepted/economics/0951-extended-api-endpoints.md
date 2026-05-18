---
title: "RFC-0951: Extended API Endpoints"
status: Accepted
version: 0.1.0
created: 2026-05-18
updated: 2026-05-18
authors:
  - quota-router team
related:
  - RFC-0920 (Unified Python SDK)
  - RFC-0930 (Provider Inference from Model String)
  - RFC-0933 (Rate Limiting Integration)
  - RFC-0934 (Real-Time Cost Tracking)
---

# RFC-0951: Extended API Endpoints

## Status

Accepted

## Summary

Add 8 additional API endpoints to achieve full parity with litellm's API surface: /v1/images, /v1/audio, /v1/files, /v1/batches, /v1/responses, /v1/messages, /v1/rerank, /v1/realtime.

## Dependencies

**Requires:**

- RFC-0920 (Economics): Unified Python SDK Dual-Mode Compatibility
- RFC-0930 (Economics): Provider Inference from Model String

**Optional:**

- RFC-0933 (Economics): Rate Limiting Integration
- RFC-0934 (Economics): Real-Time Cost Tracking
- RFC-0940 (Economics): any-llm-Mode HTTP Proxy Parity
- RFC-0941 (Economics): Streaming Parity

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Full litellm API surface | 15+ endpoints |
| G2 | OpenAI-compatible responses | Drop-in replacement |
| G3 | Streaming support | All applicable endpoints |
| G4 | Dual-mode support | litellm-mode + any-llm-mode |

## Motivation

litellm provides 15+ API endpoints. quota-router currently supports 8 core endpoints. The missing 8 endpoints block users who depend on:
- Image generation (DALL-E, Stable Diffusion)
- Audio transcription/synthesis (Whisper, TTS)
- File uploads (fine-tuning, assistants)
- Batch processing (cost-efficient bulk requests)
- OpenAI Responses API (new stateful API)
- Anthropic Messages API (native format)
- Reranking (search/RAG)
- Realtime (WebSocket streaming)

## Specification

### Endpoint Matrix

| Endpoint | HTTP Method | Provider Support | Streaming |
|----------|-------------|------------------|-----------|
| /v1/images/generations | POST | OpenAI, Stability | No |
| /v1/audio/transcriptions | POST | OpenAI (Whisper) | No |
| /v1/audio/speech | POST | OpenAI (TTS) | No |
| /v1/files | POST | OpenAI | No |
| /v1/batches | POST | OpenAI | No |
| /v1/responses | POST | OpenAI | Yes |
| /v1/messages | POST | Anthropic | Yes |
| /v1/rerank | POST | Cohere, Jina | No |
| /v1/realtime | WebSocket | OpenAI | Yes (WS) |

### /v1/images/generations

```rust
// Request
struct ImageGenerationRequest {
    model: String,                    // "dall-e-3", "stable-diffusion-xl"
    prompt: String,                   // Image description
    n: Option<u32>,                   // Number of images (1-10)
    size: Option<String>,             // "1024x1024", "512x512"
    quality: Option<String>,          // "standard", "hd"
    response_format: Option<String>,  // "url", "b64_json"
    style: Option<String>,            // "vivid", "natural"
}

// Response
struct ImageGenerationResponse {
    created: u64,
    data: Vec<ImageData>,
}

struct ImageData {
    url: Option<String>,
    b64_json: Option<String>,
    revised_prompt: Option<String>,
}
```

### /v1/audio/transcriptions

```rust
// Request (multipart/form-data)
struct AudioTranscriptionRequest {
    file: Bytes,                      // Audio file
    model: String,                    // "whisper-1"
    language: Option<String>,         // "en", "es", etc.
    prompt: Option<String>,           // Context prompt
    response_format: Option<String>,  // "json", "text", "srt", "verbose_json"
    temperature: Option<f32>,         // 0-1
}

// Response
struct AudioTranscriptionResponse {
    text: String,
}
```

### /v1/audio/speech

```rust
// Request
struct TextToSpeechRequest {
    model: String,                    // "tts-1", "tts-1-hd"
    input: String,                    // Text to synthesize
    voice: String,                    // "alloy", "echo", "fable", "onyx", "nova", "shimmer"
    response_format: Option<String>,  // "mp3", "opus", "aac", "flac"
    speed: Option<f32>,               // 0.25-4.0
}

// Response: audio bytes (streaming)
```

### /v1/files

```rust
// Request (multipart/form-data)
struct FileUploadRequest {
    file: Bytes,                      // File content
    purpose: String,                  // "fine-tune", "assistants"
}

// Response
struct FileObject {
    id: String,                       // "file-abc123"
    object: String,                   // "file"
    bytes: u64,
    created_at: u64,
    filename: String,
    purpose: String,
}

// List files
GET /v1/files -> Vec<FileObject>

// Delete file
DELETE /v1/files/{file_id} -> DeletedObject
```

### /v1/batches

```rust
// Request
struct BatchRequest {
    input_file_id: String,            // File ID from /v1/files
    endpoint: String,                 // "/v1/chat/completions"
    completion_window: String,        // "24h"
    metadata: Option<HashMap<String, String>>,
}

// Response
struct BatchObject {
    id: String,                       // "batch-abc123"
    object: String,                   // "batch"
    endpoint: String,
    input_file_id: String,
    completion_window: String,
    status: String,                   // "validating", "failed", "in_progress", "finalizing", "completed", "expired", "cancelling", "cancelled"
    output_file_id: Option<String>,
    error_file_id: Option<String>,
    created_at: u64,
    in_progress_at: Option<u64>,
    expires_at: Option<u64>,
    completed_at: Option<u64>,
    request_counts: BatchRequestCounts,
}

struct BatchRequestCounts {
    total: u32,
    completed: u32,
    failed: u32,
}

// Endpoints
POST /v1/batches -> BatchObject
GET /v1/batches -> Vec<BatchObject>
GET /v1/batches/{batch_id} -> BatchObject
POST /v1/batches/{batch_id}/cancel -> BatchObject
```

### /v1/responses (OpenAI Responses API)

```rust
// Request
struct ResponseRequest {
    model: String,
    input: Vec<InputItem>,           // Text, image, or function call
    instructions: Option<String>,
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
    tools: Option<Vec<Tool>>,
    stream: Option<bool>,
}

// Response
struct ResponseObject {
    id: String,
    object: String,                  // "response"
    created_at: u64,
    model: String,
    output: Vec<OutputItem>,
    usage: Usage,
    status: String,                  // "completed", "failed", "in_progress"
}

// Endpoints
POST /v1/responses -> ResponseObject
GET /v1/responses/{response_id} -> ResponseObject
DELETE /v1/responses/{response_id} -> DeletedObject
```

### /v1/messages (Anthropic Messages API)

```rust
// Request
struct MessagesRequest {
    model: String,                   // "claude-3-opus-20240229"
    messages: Vec<Message>,
    max_tokens: u32,
    system: Option<String>,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<Vec<Tool>>,
}

// Response
struct MessagesResponse {
    id: String,
    type: String,                    // "message"
    role: String,                    // "assistant"
    content: Vec<ContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: Usage,
}

// Endpoint
POST /v1/messages -> MessagesResponse
```

### /v1/rerank

```rust
// Request
struct RerankRequest {
    model: String,                   // "rerank-english-v3.0"
    query: String,
    documents: Vec<String>,
    top_n: Option<u32>,
    return_documents: Option<bool>,
}

// Response
struct RerankResponse {
    id: String,
    results: Vec<RerankResult>,
    meta: RerankMeta,
}

struct RerankResult {
    index: u32,
    relevance_score: f64,
    document: Option<String>,
}

// Endpoint
POST /v1/rerank -> RerankResponse
```

### /v1/realtime (WebSocket)

```rust
// WebSocket connection
WS /v1/realtime?model=gpt-4o-realtime-preview

// Client events
struct RealtimeClientEvent {
    type: String,                    // "session.update", "conversation.item.create", etc.
    // ... event-specific fields
}

// Server events
struct RealtimeServerEvent {
    type: String,                    // "session.created", "response.text.delta", etc.
    // ... event-specific fields
}
```

### Routing Integration

All endpoints MUST integrate with existing routing infrastructure:
- Provider selection via model string inference (RFC-0930)
- Rate limiting (RFC-0933)
- Budget enforcement (RFC-0934)
- Fallback chains (RFC-0902)
- Streaming support (RFC-0941)

### Error Handling

All endpoints MUST use the error taxonomy from RFC-0920:
- AuthenticationError (401)
- RateLimitError (429)
- InvalidRequestError (400)
- ContextLengthExceededError (400)
- ContentFilterError (400)
- ModelNotFoundError (404)
- GatewayTimeoutError (504)
- ProviderError (502)

## Acceptance Criteria

- [ ] /v1/images/generations returns valid image URLs or base64
- [ ] /v1/audio/transcriptions returns transcribed text
- [ ] /v1/audio/speech streams audio bytes
- [ ] /v1/files supports upload, list, delete
- [ ] /v1/batches supports create, list, get, cancel
- [ ] /v1/responses supports create, get, delete
- [ ] /v1/messages returns Anthropic-compatible response
- [ ] /v1/rerank returns ranked results
- [ ] /v1/realtime supports WebSocket connection
- [ ] All endpoints work in litellm-mode (reqwest)
- [ ] All endpoints work in any-llm-mode (py_bridge)
- [ ] Streaming works for /v1/responses, /v1/messages, /v1/realtime
- [ ] Error handling uses RFC-0920 taxonomy
- [ ] All existing tests pass

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-core/src/proxy.rs` | Add route handlers |
| `crates/quota-router-core/src/handlers/images.rs` | New - image generation |
| `crates/quota-router-core/src/handlers/audio.rs` | New - audio endpoints |
| `crates/quota-router-core/src/handlers/files.rs` | New - file management |
| `crates/quota-router-core/src/handlers/batches.rs` | New - batch processing |
| `crates/quota-router-core/src/handlers/responses.rs` | New - Responses API |
| `crates/quota-router-core/src/handlers/messages.rs` | New - Messages API |
| `crates/quota-router-core/src/handlers/rerank.rs` | New - reranking |
| `crates/quota-router-core/src/handlers/realtime.rs` | New - WebSocket realtime |
| `crates/quota-router-core/src/py_bridge/factory.rs` | Add new endpoint methods |

## Performance Targets

| Endpoint | Latency | Throughput |
|----------|---------|------------|
| /v1/images | <5s | 10 req/s |
| /v1/audio | <10s | 5 req/s |
| /v1/files | <1s | 100 req/s |
| /v1/batches | <1s | 50 req/s |
| /v1/responses | <2s | 50 req/s |
| /v1/messages | <2s | 50 req/s |
| /v1/rerank | <1s | 100 req/s |
| /v1/realtime | <100ms | 1000 msg/s |

## Security Considerations

- File uploads MUST be validated (size, type, content)
- WebSocket connections MUST authenticate before streaming
- Batch endpoints MUST enforce per-user rate limits
- Audio/image endpoints MUST validate file formats

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-05-18 | Initial draft |
