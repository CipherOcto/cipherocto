# Mission: 0870j — UDP Adapter for Gossip Broadcast

## Status

**Closed 2026-08-07.** Substrate was pre-shipped in `crates/octo-adapter-udp/`
prior to this mission's filing but the crate was not wired into the
workspace test path. Verification: `cargo test -p octo-adapter-udp --lib`
5/5 pass; `cargo clippy -p octo-adapter-udp --all-targets -- -D warnings`
clean; `cargo fmt --check -p octo-adapter-udp` clean. `members = ["crates/*"]`
in the root `Cargo.toml` already covers the crate; no workspace manifest
change needed. All 10 ACs GREEN. Owner: @cipherocto.

## RFC

RFC-0850 v1.2.0 (Networking): Deterministic Overlay Transport — §8.9 UDP Transport Profile
RFC-0863 v1.5 (Networking): General-Purpose Network Integration — PlatformAdapter bridge
RFC-0870 v1.11 (Networking): Distributed Quota Router Network — gossip transport

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types
- 0870b (must complete first) — gossip, HMAC signing
- 0870c (must complete first) — handler, route API
- 0870d (must complete first) — HMAC verification, rate limiting

## Summary

Implement a `UdpAdapter` that implements `PlatformAdapter` for `PlatformType::Udp = 0x0017`. This adapter provides UDP-based DOT envelope transport for low-latency gossip broadcast in the quota router mesh. UDP is ideal for capacity gossip, heartbeat, and discovery announcements where low latency is more important than guaranteed delivery.

## Design

### Architecture

```
QuotaRouterNode::broadcast_gossip()
        ↓
   NodeTransport::broadcast()
        ↓
   PlatformAdapterBridge::send()
        ↓
    UdpAdapter::send_message()
        ↓
   UDP datagram (tokio::net::UdpSocket)
```

### UdpAdapter struct

```rust
pub struct UdpAdapter {
    /// Local listening socket
    socket: Arc<UdpSocket>,
    /// Known peers (peer_id → socket address)
    peers: Arc<RwLock<BTreeMap<[u8; 32], SocketAddr>>>,
    /// Maximum datagram size (default: 1400 bytes, MTU-safe)
    max_datagram_size: usize,
    /// Health status
    healthy: AtomicBool,
}
```

### PlatformAdapter implementation

```rust
#[async_trait]
impl PlatformAdapter for UdpAdapter {
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Serialize envelope to bytes
        // If payload > max_datagram_size, return error (use TCP for large payloads)
        // Send UDP datagram to peer address
    }

    async fn receive_messages(
        &self,
        domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // recv_from on UDP socket
        // Parse discriminator byte + payload
        // Return as RawPlatformMessage
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        // Parse discriminator + payload
        // Deserialize DOT envelope
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            supports_raw_binary: true,
            supports_text: false,
            max_payload_bytes: 1400, // MTU-safe
            supports_media_upload: false,
            supports_media_download: false,
            supports_reactions: false,
            supports_threads: false,
            supports_edit: false,
            supports_delete: false,
            supports_search: false,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Udp, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Udp
    }
}
```

### Datagram framing

```
UDP Datagram:
  discriminator: u8 — message type (0xC6 = CapacityGossip, 0xCA = RouterAnnounce, etc.)
  payload: [u8] — DOT envelope bytes (variable length)

Maximum datagram size: 1400 bytes (MTU-safe)
No fragmentation — large payloads must use TCP or QUIC
```

### Use cases in quota router

- **Capacity gossip broadcast:** `QuotaRouterNode::broadcast_gossip()` sends via `NodeTransport::broadcast()` → `UdpAdapter`
- **Heartbeat/ping:** Lightweight peer liveness checks
- **Discovery announcements:** `QuotaRouterNode::broadcast_announce()` sends via UDP for fast propagation

### Binary integration

Update `quota-router-node` binary to wire `UdpAdapter` for gossip alongside `TcpAdapter` for forwarding:

```rust
let tcp_adapter = TcpAdapter::new(tcp_listen_addr);
let udp_adapter = UdpAdapter::new(udp_listen_addr)?;

let senders: Vec<Arc<dyn NetworkSender>> = vec![
    Arc::new(PlatformAdapterBridge::new(Box::new(tcp_adapter))),
    Arc::new(PlatformAdapterBridge::new(Box::new(udp_adapter))),
];
let transport = NodeTransport::new(senders);
```

### Error handling

- **Payload too large:** `UdpAdapter::send_message` returns `PlatformAdapterError::PayloadTooLarge` if envelope exceeds `max_datagram_size`
- **Delivery not guaranteed:** UDP has no delivery guarantee. Callers MUST NOT rely on delivery confirmation for critical messages
- **Replay protection:** Standard DOT replay cache applies (§11.2 of RFC-0850)

## Acceptance Criteria

- [x] `crates/octo-adapter-udp/` crate created with `UdpAdapter` implementing `PlatformAdapter`
- [x] `PlatformType::Udp = 0x0017` registered in `octo-network` domain registry
- [x] UDP datagram framing: `[discriminator][payload]` working correctly
- [x] `send_message` sends UDP datagrams to known peers
- [x] `receive_messages` receives UDP datagrams and parses them
- [x] `canonicalize` parses raw UDP datagrams into `DeterministicEnvelope`
- [x] Payload size check: rejects envelopes exceeding 1400 bytes
- [x] Unit tests: datagram roundtrip, size limit, health check
- [x] `cargo clippy -p octo-adapter-udp -- -D warnings` clean
- [x] `cargo fmt --check` passes

## Complexity

Medium (~400-600 lines). Adapter crate + datagram framing.

## Implementation Notes

- Use `tokio::net::UdpSocket` for async UDP
- UDP is connectionless — each `send_message` call is independent
- The `receive_messages` method should use `recv_from` with a timeout to avoid blocking
- For broadcast gossip, the adapter can send to all known peers in parallel
- The adapter does NOT handle fragmentation — callers must ensure payloads fit in one datagram
- For L3 tests, UDP adapter can run alongside TCP adapter in the same binary

## Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `PlatformType::Udp = 0x0017` | This mission (domain.rs enum update) |
| `UdpAdapter` (PlatformAdapter impl) | This mission (octo-adapter-udp crate) |
| UDP datagram framing | This mission |
