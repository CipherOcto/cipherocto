# Mission: 0863d — DotGateway fan-out + NetworkReceiver

## Status

Claimed

## RFC

RFC-0863 (Networking): General-Purpose Network Integration — `octo-transport` — Phase 3: Inbound dispatch

## Dependencies

Missions that must be completed before this one:

- 0863a (must be completed) — provides `NetworkSender` trait and types
- 0863b (should be completed) — provides `NodeTransport`
- 0862j (should be completed) — provides `SyncNode` and DGP integration in `octo-network`

## Summary

Complete the inbound transport path: implement the `DotGateway::process_envelope()` fan-out stub so the gateway actually dispatches received envelopes to adapters, and implement `NetworkReceiver` for general-purpose inbound dispatch. This closes the loop — data can flow both outbound (via `NetworkSender`) and inbound (via `NetworkReceiver` + `DotGateway`).

## Design

### 1. DotGateway fan-out (`crates/octo-network/src/dot/mod.rs:175`)

Replace the TODO stub with actual adapter dispatch:

```rust
// In DotGateway::process_envelope(), step 4:
// Currently:
//   // 4. Forward to all adapters (Class C — transport-dependent)
//   // Note: In production, this would iterate over connected domains
//   // and forward to the appropriate adapter(s).
//   Ok(ProcessingResult::Forwarded)

// Replace with:
for adapter in &self.adapters {
    if let Some(domain) = envelope.domain() {
        match adapter.send_message(&domain, envelope).await {
            Ok(_receipt) => { /* log success */ }
            Err(e) => { /* log error, continue to next adapter */ }
        }
    }
}
Ok(ProcessingResult::Forwarded)
```

### 2. NetworkReceiver trait (`octo-transport/src/receiver.rs`)

```rust
/// General-purpose inbound transport handler.
#[async_trait]
pub trait NetworkReceiver: Send + Sync {
    /// Handle an incoming payload from a transport.
    async fn on_receive(&self, payload: &[u8], context: &ReceiveContext) -> Result<(), TransportError>;

    /// Return the handler name for diagnostics.
    fn name(&self) -> &str;
}

/// Context for a received payload.
pub struct ReceiveContext {
    /// The source transport name.
    pub source_transport: String,
    /// The mission ID.
    pub mission_id: [u8; 32],
    /// The sender's peer ID (if authenticated).
    pub sender_id: Option<[u8; 32]>,
}
```

### 3. Inbound dispatch flow

The inbound path flows through `NodeTransport::dispatch()`:

```
Consumer (node runtime, test harness):
  1. Poll adapters for raw bytes (PlatformAdapter::receive_messages)
  2. Canonicalize to wire bytes
  3. Call node.receive(payload, &ctx)  [public API; symmetric to node.route()]
     — or, equivalently for custom layers, call node.transport.dispatch(payload, &ctx)

NodeTransport::dispatch():
  → Iterates registered NetworkReceiver handlers
     - QuotaRouterHandler (for mesh forwarding; registered automatically by QuotaRouterNodeBuilder::build())
     - SyncNode (for sync payloads)
     - Agent handler (for agent messages, future)
     - Marketplace handler (for settlement, future)
```

### 4. Export `sync` module from `octo-network`

Make `SyncNode` and `SyncNetworkBridge` public:

```rust
// In crates/octo-network/src/lib.rs, add:
pub mod sync;
```

This unblocks the DGP integration path (already implemented in `sync/mod.rs` and `sync/dgp_integration.rs` but currently dead code).

### What this mission does NOT implement

- `NetworkSender` / outbound (0863a, 0863b)
- Sync consumer wiring (0863c)
- Agent/marketplace runtime wiring (depends on those runtimes existing)

## Acceptance Criteria

- [ ] `DotGateway::process_envelope()` forwards envelopes to adapters (not a stub)
- [ ] `NetworkReceiver` trait defined with `on_receive` and `name` methods
- [ ] `ReceiveContext` struct defined with `source_transport`, `mission_id`, `sender_id`
- [ ] `sync` module exported from `octo-network/src/lib.rs`
- [ ] `SyncNode` accessible from `octo_network::sync::SyncNode`
- [ ] Unit tests pass for DotGateway fan-out: `cargo test -p octo-network`
- [ ] Unit tests pass for NetworkReceiver: `cargo test -p octo-transport`
- [ ] Clippy clean for both crates
- [ ] `cargo fmt --check` passes

## Type Coverage

| RFC Type                | Implemented By |
| ----------------------- | -------------- |
| `NetworkReceiver` trait | This mission   |
| `ReceiveContext` struct | This mission   |
| `DotGateway` fan-out    | This mission   |
| `sync` module export    | This mission   |

## Complexity

Medium (~300-500 lines). DotGateway fan-out is ~50 lines. NetworkReceiver trait + ReceiveContext is ~100 lines. Export + wiring is ~50 lines. Tests ~100-200 lines.

## Implementation Notes

- The DotGateway fan-out iterates `self.adapters` (a `Vec<Box<dyn PlatformAdapter>>`). For each adapter, check if the envelope's domain matches the adapter's platform type, then call `send_message()`. **Note:** `DeterministicEnvelope` may not have a `domain()` method — the implementer must verify against `octo-network/src/dot/envelope.rs`. If not available, the domain must be extracted from the envelope's wire format or passed as a parameter to `process_envelope()`.
- `NetworkReceiver` is the inbound counterpart to `NetworkSender`. Handlers register with `NodeTransport` via `register_receiver()`. For the quota router mesh, this is wired automatically by `QuotaRouterNodeBuilder::build()` (no caller-side registration required). Other consumers (sync, agent, marketplace runtimes) may register their own handlers after build. The consumer is responsible for obtaining raw bytes from the wire and calling either `node.receive(payload, &ctx)` (the public API for quota router consumers) or `node.transport.dispatch(payload, &ctx)` (for custom layered inbound flows).
- Exporting `sync` module is a one-line change in `lib.rs` but unblocks the entire DGP integration path.
- The existing `SyncNode` and `SyncNetworkBridge` code in `octo-network/src/sync/` is already implemented — it just needs to be exported.
