# Mission: 0917-a — API Endpoints

## Status

Open

## RFC

RFC-0917 (Economics): Dual-Mode Query Router

## Dependencies

- Mission-0932-a: Gateway Auth Wiring (provides auth)

## Context

The proxy currently handles all requests as chat completions with no path-based routing. LiteLLM and any-llm both expose `/v1/embeddings` and `/v1/models` endpoints. This mission adds path-based routing and the missing endpoints.

## Acceptance Criteria

### Endpoints

- [ ] `POST /v1/embeddings` — text embeddings
- [ ] `GET /v1/models` — list available models
- [ ] `GET /v1/models/{model}` — get model info

### Embeddings

- [ ] Parse embedding request body
- [ ] Route to embedding-capable provider
- [ ] Return embedding response in OpenAI format

### Models

- [ ] List all models from DispatchInfo map
- [ ] Filter by provider if requested
- [ ] Return model list in OpenAI format

### Tests

- [ ] `/v1/embeddings` returns valid embeddings
- [ ] `/v1/models` returns all configured models
- [ ] `/v1/models/{model}` returns specific model info
- [ ] Both endpoints require auth (if auth enabled)

## Key Files

- `crates/quota-router-core/src/proxy.rs` — add route handlers
- `crates/quota-router-core/src/native_http/mod.rs` — HttpProvider::embedding()
- `crates/quota-router-core/src/config.rs` — DispatchInfo map

## Notes

The existing `HttpProvider` trait has `embedding()` method. The `DispatchInfo` map has all model metadata. This mission is about adding route handlers.
