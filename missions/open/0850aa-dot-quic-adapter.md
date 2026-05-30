# 0850aa — QUIC Adapter Implementation

**RFC:** RFC-0850 §8.7 (QUIC Transport Profile)
**Status:** Open
**Priority:** High
**Created:** 2026-05-30

## Summary

Implement the QUIC platform adapter (`PlatformType::Quic = 0x0015`) using the `quinn` crate, per RFC-0850 §8.7. QUIC is the preferred native transport for gateway-to-gateway communication, offering 0-RTT, multiplexed streams, connection migration, and built-in TLS 1.3 encryption.

## Acceptance Criteria

- [ ] `PlatformType::Quic = 0x0015` added to domain registry
- [ ] `QuicAdapter` struct implementing `PlatformAdapter` trait
- [ ] Control stream protocol (capabilities, ping/pong, shutdown, key rotation)
- [ ] Envelope stream framing (length-prefixed, unidirectional, raw binary)
- [ ] Onion stream protocol (bidirectional, per-route, hop-indexed)
- [ ] Two-layer handshake: QUIC TLS 1.3 + RFC-0853 overlay session
- [ ] GDP integration (QUIC peer registration as `PlatformType::Quic`)
- [ ] 0-RTT with replay protection (RFC-0853 §7)
- [ ] Connection migration support (RFC 9000 §9)
- [ ] `supports_raw_binary: true` in capabilities
- [ ] Gateway configuration schema (listen_addr, tls_cert, max_streams, etc.)
- [ ] 15+ tests (connection, stream framing, control protocol, session, migration)
- [ ] cargo fmt + clippy clean

## Implementation Plan

### 1. Domain Registry
- Add `Quic = 0x0015` to `PlatformType` enum in `crates/octo-network/src/dot/domain.rs`
- Add `from_u16` match arm

### 2. Adapter Crate
- Create `crates/octo-adapter-quic/` with `quinn` dependency
- Implement `QuicConfig` (listen_addr, tls_cert, tls_key, max_streams, idle_timeout, 0-rtt)
- Implement `QuicAdapter` with connection pool, stream management

### 3. Stream Protocol
- Control stream (stream 0): length-prefixed `[u32 len][u16 type][payload]`
- Envelope streams: unidirectional, `[u32 len][u8 type][raw_bytes]`, FIN after write
- Onion streams: bidirectional, `[u32 len][u8 type][u16 hop_index][encrypted_layer]`

### 4. Session Layer
- After QUIC handshake, execute RFC-0853 §5 over control stream
- X25519 ephemeral key exchange
- HKDF-BLAKE3 session key derivation
- Ed25519 signed transcript for mutual authentication
- Session key rotation via control message

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

- `quinn` crate (QUIC implementation in Rust)
- `rustls` (TLS 1.3, used by quinn)
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
