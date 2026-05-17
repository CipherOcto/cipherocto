# Mission: 0942-c — /v1/moderations Endpoint

## Status

Open

## RFC

RFC-0942 (Economics): Additional API Endpoints

## Context

LiteLLM supports the /v1/moderations endpoint for content moderation. This mission adds path routing.

## Acceptance Criteria

- [ ] Add /v1/moderations path routing in handle_request
- [ ] Forward to provider's moderations API
- [ ] Return OpenAI-compatible response format

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add path routing
