# RFC-0846 (Networking): WebSocket Infrastructure for quota-router

## Status

Planned

## Summary

Define WebSocket infrastructure for quota-router to support bidirectional, stateful communication required by OpenAI's Realtime API and future WebSocket-based endpoints.

## Why Needed

1. **Mission 0951-h blocked:** The `/v1/realtime` WebSocket endpoint cannot be implemented without WebSocket infrastructure
2. **RFC-0951 incomplete:** Extended API endpoints RFC includes `/v1/realtime` but defers WS infrastructure to a separate RFC
3. **Streaming parity:** HTTP streaming (RFC-0941) is unidirectional; WebSocket enables bidirectional event streaming
4. **Mode gate compatibility:** Both litellm-mode (reqwest) and any-llm-mode (py_bridge) must support WebSocket — requires verification

## Scope

### In Scope
- WebSocket client using `tokio-tungstenite`
- Hyper WebSocket upgrade support in existing proxy.rs
- Connection lifecycle management (connect, upgrade, relay, close)
- Authentication flow for WebSocket upgrades
- Bidirectional event relay architecture
- Binary frame handling for audio data (interleaved with JSON events)
- Session state management:
  - Capture and track OpenAI `session_id` for quota-router use (budget/rate limiting)
  - Relay session events transparently
  - Handle session expiry and cleanup
- Error handling and close codes
- Security: frame size limits, connection limits, rate limiting

### Out of Scope
- Audio transcoding or format conversion
- Multiple simultaneous upstream connections
- Non-OpenAI WebSocket providers (provider-specific protocols)
- WebSocket over HTTP/3 (future work)

## Dependencies

**Requires:**
- RFC-0920 (Economics): Unified Python SDK Dual-Mode Compatibility
- RFC-0951 (Economics): Extended API Endpoints (for realtime endpoint spec)

**Optional:**
- RFC-0933 (Economics): Rate Limiting Integration
- RFC-0941 (Economics): Streaming Parity

## Next Steps After Acceptance

1. Create mission: "0951-h-a — WebSocket Core Infrastructure"
2. Create mission: "0951-h-b — Realtime Endpoint Implementation"
3. Verify Python SDK WebSocket support for any-llm-mode

## Notes

This is a placeholder RFC. Full specification will be developed during Draft phase.

For the Realtime endpoint specifically, see RFC-0951 §/v1/realtime (WebSocket).