# Mission: DOT WebRTC Adapter

## Status

Implemented (7 tests, stub data channel, signaling)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a WebRTC data channel adapter as a `cdylib` plugin. WebRTC enables browser-to-browser and browser-to-server communication without intermediaries — ideal for web-based CipherOcto clients.

## Acceptance Criteria

- [ ] `crates/octo-adapter-webrtc/` crate compiles to `cdylib`
- [ ] Implements `PlatformAdapter` trait with all methods
- [ ] `send_envelope()` sends via WebRTC data channel
- [ ] `receive_messages()` listens on WebRTC data channel
- [ ] `canonicalize()` extracts envelope from data channel message
- [ ] `CapabilityReport`: max_payload=262144 (256KB data channel limit), rate_limit=1000/sec
- [ ] `domain_id()`: `BroadcastDomainId(0x000D, BLAKE3(peer_id))`
- [ ] Config: `signaling_url`, `ice_servers`, `peer_id`
- [ ] Signaling: uses configurable signaling server for SDP exchange
- [ ] Unit tests with mock data channel

## Location

`crates/octo-adapter-webrtc/`

## Complexity

High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- WebRTC data channels: reliable, ordered delivery (SCTP over DTLS)
- Signaling: requires a signaling server for SDP offer/answer exchange
- ICE: STUN/TURN servers for NAT traversal
- Use `webrtc-rs` crate for Rust WebRTC implementation
- Data channel message size: up to 256KB (Chrome), 64KB (Firefox)
- Latency: sub-100ms for same-region peers
- Use case: browser-based CipherOcto clients, P2P web apps

## Additional Requirements (from Audit)

- [ ] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [ ] Add tests to match ZeroClaw coverage (see Mission 0850u)
