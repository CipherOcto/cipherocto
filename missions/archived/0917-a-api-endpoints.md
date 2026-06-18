# Mission: 0917-a — API Endpoints

## Status

Complete

Open

## RFC

RFC-0917 (Economics): Dual-Mode Query Router

## Dependencies

- Mission-0932-a: Gateway Auth Wiring (provides auth)
- Mission-0929-d: Wire DispatchInfo to Proxy Dispatch Path (implicit dependency — /chat/completions dispatch uses DispatchInfo)

## Context

The proxy currently handles all requests as chat completions with no path-based routing. LiteLLM and any-llm both expose `/v1/embeddings` and `/v1/models` endpoints. This mission adds path-based routing and the missing endpoints.

## Acceptance Criteria

### Endpoints

- [x] `POST /v1/embeddings` — text embeddings
- [x] `GET /v1/models` — list available models
- [x] `GET /v1/models/{model}` — get model info

### Embeddings

- [x] Parse embedding request body
- [x] Route to embedding-capable provider
- [x] Return embedding response in OpenAI format

### Models

- [x] List all models from DispatchInfo map
- [x] Filter by provider if requested
- [x] Return model list in OpenAI format

### Tests

- [x] `/v1/embeddings` returns valid embeddings
- [x] `/v1/models` returns all configured models
- [x] `/v1/models/{model}` returns specific model info
- [x] Both endpoints require auth (if auth enabled)

## Key Files

- `crates/quota-router-core/src/proxy.rs` — add route handlers
- `crates/quota-router-core/src/native_http/mod.rs` — HttpProvider::embedding()
- `crates/quota-router-core/src/config.rs` — DispatchInfo map

## Notes

The existing `HttpProvider` trait has `embedding()` method. The `DispatchInfo` map has all model metadata. This mission is about adding route handlers.

### H1: Hard Blocker

Hard blocker: Mission-0932-a (Gateway Auth) must be complete before /chat/completions endpoint can be tested with real auth. The endpoint can be implemented without auth but cannot be integration-tested.
