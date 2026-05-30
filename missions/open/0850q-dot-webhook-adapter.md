# Mission: DOT Webhook Adapter

## Status

Implemented (14 tests)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a generic HTTP Webhook adapter as a `cdylib` plugin. This adapter enables any HTTP endpoint to participate in the DOT overlay by receiving envelopes via POST and sending via configurable endpoints.

## Acceptance Criteria

- [x] `crates/octo-adapter-webhook/` crate compiles to `cdylib`
- [x] Implements `PlatformAdapter` trait with all methods
- [x] `send_envelope()` POSTs envelope to configured endpoint URL
- [x] `receive_messages()` listens on configurable HTTP path for incoming webhooks
- [x] `canonicalize()` extracts envelope from webhook POST body
- [x] `CapabilityReport`: max_payload=1048576 (1MB), rate_limit=100/sec
- [x] `domain_id()`: `BroadcastDomainId(0x0009, BLAKE3(endpoint_url))`
- [x] Config: `send_url`, `listen_port`, `send_method` (POST or PUT), `auth_header` (optional), `verify_signature` (optional)
- [x] HMAC-SHA256 signature verification for incoming webhooks (timing-safe comparison)
- [x] Unit tests with mock HTTP server (14 tests)

## Location

`crates/octo-adapter-webhook/`

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- This is the most generic adapter — bridges any HTTP-capable system to DOT
- Receiving: embed a lightweight HTTP server (`axum`)
- Sending: configurable POST or PUT request with envelope in body
- Authentication: optional HMAC-SHA256 signature header
- Content type: `application/octet-stream` for raw envelope bytes
- Use case: enterprise integrations, CI/CD pipelines, monitoring systems

## Additional Requirements (from Audit)

- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
