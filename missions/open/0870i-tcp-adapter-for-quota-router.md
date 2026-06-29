# Mission: 0870i — TCP Adapter for Quota Router E2E Tests

## Status

Open

## RFC

RFC-0850 v1.2.0 (Networking): Deterministic Overlay Transport — §8.8 TCP Transport Profile
RFC-0863 v1.5 (Networking): General-Purpose Network Integration — PlatformAdapter bridge
RFC-0870 v1.11 (Networking): Distributed Quota Router Network — transport integration

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types
- 0870b (must complete first) — gossip, HMAC signing
- 0870c (must complete first) — handler, route API
- 0870d (must complete first) — HMAC verification, rate limiting

## Summary

Implement a `TcpAdapter` that implements `PlatformAdapter` for `PlatformType::Tcp = 0x0016`. This adapter provides TCP-based DOT envelope transport for the quota router mesh, enabling real L3 cross-process E2E tests that exercise the full production code path through `PlatformAdapterBridge` → `NodeTransport` → `QuotaRouterHandler`.

## Design

### Architecture

```
QuotaRouterHandler (NetworkReceiver)
        ↑
   NodeTransport (fan-out/failover)
        ↑
   PlatformAdapterBridge (NetworkSender)
        ↑
   TcpAdapter (PlatformAdapter)
        ↑
   TCP socket (tokio::net::TcpStream / TcpListener)
```

### TcpAdapter struct

```rust
pub struct TcpAdapter {
    /// Local listening address
    listen_addr: SocketAddr,
    /// Connected peers (peer_id → TcpStream)
    peers: Arc<RwLock<BTreeMap<[u8; 32], TcpStream>>>,
    /// Outbound connection queue
    connect_queue: mpsc::Sender<SocketAddr>,
    /// Health status
    healthy: AtomicBool,
}
```

### PlatformAdapter implementation

```rust
#[async_trait]
impl PlatformAdapter for TcpAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Serialize envelope to bytes
        // Find peer connection by domain
        // Length-prefix frame: [u32 len][payload]
        // Send over TcpStream
    }

    async fn receive_messages(
        &self,
        domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // Accept incoming TCP connections
        // Read length-prefixed frames
        // Return as RawPlatformMessage
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        // Parse length-prefixed frame
        // Deserialize DOT envelope
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            supports_raw_binary: true,
            supports_text: false,
            max_payload_bytes: 16 * 1024 * 1024, // 16MB
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
        BroadcastDomainId::new(PlatformType::Tcp, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Tcp
    }
}
```

### Framing protocol

```
TCP Frame:
  frame_len: u32 (big-endian) — length of payload in bytes
  payload: [u8; frame_len] — DOT envelope bytes

Maximum frame size: 16MB
Keepalive: TCP keepalive every 30s
Idle timeout: 120s
```

### Connection management

- **Outbound:** `TcpAdapter::connect(addr)` — async TCP connect with exponential backoff
- **Inbound:** `TcpAdapter::listen()` — accept loop spawning reader tasks per connection
- **Health:** `is_healthy()` returns false if no connections available
- **Reconnection:** Exponential backoff starting at 1s, capped at 30s, max 5 retries

### Binary integration

Update `quota-router-node` binary to wire `TcpAdapter`:

```rust
let tcp_adapter = TcpAdapter::new(listen_addr);
let sender: Arc<dyn NetworkSender> = Arc::new(
    PlatformAdapterBridge::new(Box::new(tcp_adapter))
);
let transport = NodeTransport::new(vec![sender]);
// Wire transport into QuotaRouterNode
```

## Acceptance Criteria

- [ ] `crates/octo-adapter-tcp/` crate created with `TcpAdapter` implementing `PlatformAdapter`
- [ ] `PlatformType::Tcp = 0x0016` registered in `octo-network` domain registry
- [ ] TCP framing: length-prefix `[u32 len][payload]` working correctly
- [ ] `send_envelope` sends DOT envelopes over TCP to connected peers
- [ ] `receive_messages` accepts incoming TCP connections and reads frames
- [ ] `canonicalize` parses raw TCP frames into `DeterministicEnvelope`
- [ ] `PlatformAdapterBridge::new(Box::new(tcp_adapter))` compiles and produces `NetworkSender`
- [ ] `quota-router-node` binary wires `TcpAdapter` via `PlatformAdapterBridge`
- [ ] L3 TCP tests exercise real message flow through `NodeTransport` → `PlatformAdapterBridge` → `TcpAdapter` → TCP
- [ ] Unit tests: framing roundtrip, connection management, health check
- [ ] Integration test: two `quota-router-node` processes exchange a `ForwardRequest` over TCP
- [ ] `cargo clippy -p octo-adapter-tcp -- -D warnings` clean
- [ ] `cargo fmt --check` passes

## Complexity

High (~800-1200 lines). Adapter crate + framing + connection management + binary integration.

## Implementation Notes

- Use `tokio::net::TcpStream` and `tokio::net::TcpListener` for async TCP
- Frame parsing must handle partial reads (TCP is a byte stream, not message-oriented)
- The adapter does NOT handle TLS — that's a separate concern (RFC-0853)
- For L3 tests, the adapter runs in the same process as the binary — no separate certificate management needed
- The `send_envelope` method must be thread-safe (called from multiple async tasks)
- Connection pool should be bounded (max 128 connections per adapter, matching `PeerCache` limits)

## Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `PlatformType::Tcp = 0x0016` | This mission (domain.rs enum update) |
| `TcpAdapter` (PlatformAdapter impl) | This mission (octo-adapter-tcp crate) |
| TCP framing protocol | This mission |
| `PlatformAdapterBridge` wrapping TcpAdapter | This mission (binary integration) |
