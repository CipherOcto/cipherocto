# Mission: 0951-h — WebSocket `/v1/realtime` Endpoint

## Status

Blocked (awaiting RFC-0846 acceptance)

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

**BLOCKED by:** RFC-0846 (Networking): WebSocket Infrastructure — in `rfcs/planned/networking/0846-websocket-infrastructure.md`

**Requires:**
- RFC-0920 (Economics): Unified Python SDK Dual-Mode Compatibility
- RFC-0930 (Economics): Provider Inference from Model String
- RFC-0846 (Networking): WebSocket Infrastructure — must be accepted first

**Optional:**
- RFC-0933 (Economics): Rate Limiting Integration
- RFC-0934 (Economics): Real-Time Cost Tracking
- RFC-0940 (Economics): any-llm-Mode HTTP Proxy Parity
- RFC-0941 (Economics): Streaming Parity

## Pre-Implementation Requirements

### Phase 0: Research (REQUIRED before claiming)

This phase MUST be completed before claiming this mission.

- [ ] **Verify Python SDK WebSocket support**
  - Check OpenAI Python SDK documentation for WebSocket client
  - Verify py_bridge can relay WebSocket frames bidirectionally
  - If Python SDK doesn't support WS: any-llm-mode proxy is impossible
  - Document findings in research report

- [ ] **Verify OpenAI Realtime API binary frame handling**
  - Confirm audio data uses base64-encoded JSON frames (not raw binary WebSocket frames)
  - Document any binary frame formats if applicable

- [ ] **Document session ID tracking approach**
  - quota-router must capture OpenAI's `session_id` for budget/rate limiting
  - Document how session events will be intercepted

### 1. WebSocket Infrastructure RFC (BLOCKING — RFC-0846)

Before implementation can begin, RFC-0846 must be created and accepted covering:

- [ ] `tokio-tungstenite` dependency integration
- [ ] Hyper WebSocket upgrade support
- [ ] Connection handler architecture
- [ ] Authentication flow for WebSocket upgrades
- [ ] Binary frame handling (for audio data)
- [ ] Session state management (capture session_id for quota-router use)
- [ ] Backpressure and queuing strategies
- [ ] Integration with existing proxy infrastructure

### 2. Mode Gate Compatibility Verification

**CRITICAL per RFC-0917 invariant:** Mode gate controls HOW, not WHETHER

| Mode | WebSocket Behavior | Verification Required |
|------|-------------------|----------------------|
| litellm-mode | tokio-tungstenite → OpenAI WS endpoint | Need WS client implementation |
| any-llm-mode | py_bridge → Python SDK | Python SDK WS support unconfirmed (see Phase 0) |

### 3. OpenAI Realtime Protocol Analysis

The OpenAI Realtime API is a stateful, bidirectional protocol requiring:

**Session Lifecycle:**
- Client sends `session.update` to configure (model, voice, tools)
- Server sends `session.created` with session_id
- Client sends `conversation.item.create` to add messages
- Client sends `response.create` to trigger generation
- Server streams `response.text.delta`, `response.audio_transcript.delta`, etc.
- Client sends `session.update` to modify, `session.delete` to close

**Event Taxonomy:**
```
Client → Server:
  session.update, session.delete
  conversation.item.create, conversation.item.delete, conversation.item.truncate
  response.create, response.cancel

Server → Client:
  session.created, session.updated, session.expired
  conversation.created, conversation.item.created, conversation.item.deleted, conversation.item.truncated
  response.created, response.done, response.content_part.done
  response.text.delta, response.text.done
  response.audio_transcript.delta, response.audio_transcript.done
  response.audio.delta, response.audio.done
  error
```

**Binary Data:** Audio data is sent as base64-encoded strings within JSON events, NOT as raw binary WebSocket frames. The relay only needs to handle JSON text frames.

## Acceptance Criteria

### Phase 1: Infrastructure (Pre-requisite)
- [ ] RFC-0846 accepted
- [ ] `tokio-tungstenite` integrated into Cargo.toml
- [ ] Hyper WebSocket upgrade implemented in proxy.rs
- [ ] Connection handler architecture defined
- [ ] Binary frame handling implemented (base64 audio data)
- [ ] Session ID tracking implemented (capture from `session.created`)
- [ ] Mode compatibility verified (litellm + any-llm)

### Phase 2: Core Implementation
- [ ] WebSocket endpoint accepts WS /v1/realtime connections
- [ ] Authenticates via `?api_key=` query param
- [ ] Establishes upstream connection to OpenAI wss://api.openai.com/v1/realtime
- [ ] Session management (create, update, close)
- [ ] Bidirectional event relay (client ↔ OpenAI)
- [ ] Audio streaming support (input + output via base64-encoded JSON)

### Phase 3: Quality
- [ ] Error handling follows RFC-0920 taxonomy where applicable
- [ ] WebSocket-specific errors mapped appropriately
- [ ] Message size limits enforced (4096 bytes for text frames)
- [ ] Connection timeout and cleanup
- [ ] Unit tests pass (WS handler, event parsing, session management)
- [ ] Integration tests pass (end-to-end with mock or test provider)

### Phase 4: Verification
- [ ] Works in litellm-mode (tokio-tungstenite WS client)
- [ ] Works in any-llm-mode (py_bridge WS relay) — VERIFIED NOT BLOCKED in Phase 0
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Security Considerations

| Item | Requirement |
|------|-------------|
| Authentication | API key validation before upgrade; reject unauthenticated WS connections |
| Frame size limits | Max 4096 bytes per text frame |
| Connection limits | Max 10 concurrent WS connections per API key |
| Rate limiting | Per-message rate limiting (100 msg/s per connection) |
| Input validation | Validate all incoming event JSON schema |
| Timeout | Connection idle timeout: 5 minutes |
| Cleanup | Proper WS close handshake on logout/expiry |

## Error Handling

### WebSocket-Specific Error Mapping

| Scenario | Response | RFC-0920 Equivalent |
|----------|----------|----------------------|
| No API key | WS close 4001 | AuthenticationError |
| Invalid API key | WS close 4002 | AuthenticationError |
| Rate limited | WS close 4003 | RateLimitError |
| Invalid event JSON | WS close 4004 | InvalidRequestError |
| Frame too large | WS close 4005 | InvalidRequestError |
| Upstream WS error | WS close 4006 | ProviderError |
| Timeout/idle | WS close 4007 | TimeoutError |

## OpenAI Realtime Protocol Notes

**Provider:** OpenAI only (`wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview`)

**Not a passthrough proxy** — requires active session state management.

**Key differences from HTTP proxying:**
- Stateful bidirectional communication
- Event-driven, not request/response
- Audio data is base64-encoded JSON (not binary frames)
- Session-scoped, not request-scoped
- quota-router MUST capture `session_id` for internal tracking

**Key architectural requirements:**
1. Capture `session.created` event to extract OpenAI's `session_id`
2. Store `session_id` for budget/rate limiting association
3. Relay all events bidirectionally without modification

## Implementation Notes

### Files to Create

| File | Purpose |
|------|---------|
| `crates/quota-router-core/src/ws/mod.rs` | WS infrastructure module |
| `crates/quota-router-core/src/ws/client.rs` | WebSocket client for OpenAI |
| `crates/quota-router-core/src/handlers/realtime.rs` | WebSocket handler |
| `crates/quota-router-core/src/handlers/realtime/events.rs` | Event type definitions |
| `crates/quota-router-core/src/handlers/realtime/session.rs` | Session state management |

### Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/proxy.rs` | Add WS upgrade handler |
| `crates/quota-router-core/Cargo.toml` | Add `tokio-tungstenite`, `tokio-native-tls` |
| `crates/quota-router-core/src/auth/sso/mod.rs` | Add WS auth if using SSO |
| `crates/quota-router-pyo3/src/completion.rs` | Add WS relay for any-llm-mode |

## Performance Targets

Per RFC-0951:

| Metric | Target | Notes |
|--------|--------|-------|
| Connection establishment | <100ms | Proxy processing only, excludes upstream WS connect |
| Event relay latency | <50ms | Proxy processing time |
| Concurrent connections | 100 per instance | Per API key: 10 |
| Message throughput | 1000 msg/s total | Per connection: 100 msg/s |

## Claimant

@claude

## Pull Request

None

## Notes

- OpenAI Realtime: `wss://api.openai.com/v1/realtime`
- Protocol: WebSocket + JSON events (not HTTP)
- Audio: base64-encoded JSON (not binary frames)
- This mission is BLOCKED until RFC-0846 is accepted