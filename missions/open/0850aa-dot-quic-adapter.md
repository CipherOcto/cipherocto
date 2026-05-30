# 0850aa — QUIC Adapter Implementation

**RFC:** RFC-0850 §8.7 (QUIC Transport Profile)
**Status:** Open
**Priority:** High
**Created:** 2026-05-30

## Summary

Implement the QUIC platform adapter (`PlatformType::Quic = 0x0015`) using the `quinn` crate, per RFC-0850 §8.7. QUIC is the preferred native transport for gateway-to-gateway communication, offering 0-RTT, multiplexed streams, connection migration, and built-in TLS 1.3 encryption.

## Acceptance Criteria

- [x] `PlatformType::Quic = 0x0015` added to domain registry
- [x] `QuicAdapter` struct implementing `PlatformAdapter` trait
- [x] Control stream protocol (capabilities, ping/pong, shutdown, key rotation)
- [x] Envelope stream framing (length-prefixed, unidirectional, raw binary)
- [x] Onion stream protocol (bidirectional, per-route, hop-indexed)
- [x] Two-layer handshake: QUIC TLS 1.3 (required) + RFC-0853 overlay session (optional, mission-scoped only)
- [ ] GDP integration (QUIC peer registration as `PlatformType::Quic`)
- [ ] 0-RTT with replay protection (RFC-0853 §7)
- [ ] Connection migration support (RFC 9000 §9)
- [x] `supports_raw_binary: true` in capabilities
- [x] Gateway configuration schema (listen_addr, tls_cert, max_streams, etc.)
- [x] 19 tests (connection, stream framing, control protocol, session, migration)
- [x] cargo fmt + clippy clean

## Implementation Plan

### 1. Domain Registry
- Add `Quic = 0x0015` to `PlatformType` enum in `crates/octo-network/src/dot/domain.rs`
- Add `from_u16` match arm

### 2. Adapter Crate
- Create `crates/octo-adapter-quic/` with `quinn` dependency
- Implement `QuicConfig` (listen_addr, tls_cert, tls_key, max_streams, idle_timeout, 0-rtt)
- Implement `QuicAdapter` with connection pool, stream management

### 3. Stream Protocol
- Control stream (stream 0): framed `[u32 frame_len][u16 type][payload]`, `frame_len = 2 + payload.len()`
- Envelope streams: unidirectional, `[u32 frame_len][u16 type][raw_bytes]`, FIN after write
- Onion streams: bidirectional, `[u32 frame_len][u16 type][u16 hop_index][encrypted_layer]`
- All type fields are u16 for consistency

### 4. Session Layer
- QUIC TLS 1.3 handles transport-layer mutual authentication
- Overlay session (RFC-0853 §5) is OPTIONAL, only for mission-scoped operations
- If needed: X25519 ephemeral key exchange, HKDF-BLAKE3 session key, Ed25519 signed transcript
- 10-second timeout on overlay session establishment

### 5. GDP Integration
- Register QUIC gateways in GDP discovery state
- Bootstrap via seed list multiaddrs
- Liveness via control stream ping/pong

### 6. Tests
- Connection establishment (1-RTT)
- 0-RTT resumption
- Envelope send/receive (raw binary)
- Control stream protocol
- Stream multiplexing (no head-of-line blocking)
- Connection migration simulation
- Session key derivation
- Replay protection on 0-RTT data

## Dependencies

- `quinn` crate (QUIC implementation in Rust, includes `rustls`)
- RFC-0853 `ocrypt::session` (overlay session keys)
- RFC-0851 `gdp::discovery` (peer discovery)

## Files to Create/Modify

| File | Action |
|------|--------|
| `crates/octo-network/src/dot/domain.rs` | Add `Quic = 0x0015` |
| `crates/octo-adapter-quic/Cargo.toml` | New crate |
| `crates/octo-adapter-quic/src/lib.rs` | Adapter + config + tests |
| `crates/octo-adapter-quic/src/stream.rs` | Stream framing protocol |
| `crates/octo-adapter-quic/src/session.rs` | Overlay session handshake |
| `crates/octo-adapter-quic/src/control.rs` | Control stream protocol |
| `crates/octo-network/Cargo.toml` | Add `quic` feature gate |
