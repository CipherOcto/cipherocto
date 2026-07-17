# Mission: 0863b — NodeTransport: broadcast, failover, health tracking

## Status

Claimed

## RFC

RFC-0863 (Networking): General-Purpose Network Integration — `octo-transport` — §Specification §NodeTransport

## Dependencies

Missions that must be completed before this one:

- 0863a (must be completed) — provides `NetworkSender` trait and `PlatformAdapterBridge`

## Summary

Implement `NodeTransport` — the declarative transport stack that provides `broadcast()` (fan-out to all healthy transports) and `send_best()` (failover to best available). This is the consumer-facing API that any code — sync engines, agent runtimes, marketplace services — uses to send data through the network.

## Design

### New file: `octo-transport/src/node_transport.rs`

```rust
pub struct NodeTransport {
    senders: Vec<Arc<dyn NetworkSender>>,
    receivers: Vec<Arc<dyn NetworkReceiver>>,
}

impl NodeTransport {
    pub fn new(senders: Vec<Arc<dyn NetworkSender>>) -> Self;

    /// Register a handler for inbound payloads.
    /// Handlers are called in registration order by `dispatch()`.
    /// Safe to call concurrently — receivers are protected internally.
    pub fn register_receiver(&self, receiver: Arc<dyn NetworkReceiver>);

    /// Broadcast to all healthy transports concurrently.
    /// Returns count of successful sends.
    pub async fn broadcast(&self, payload: &[u8], ctx: &SendContext) -> usize;

    /// Send to the best available transport (failover).
    /// Tries transports in order, skips unhealthy, returns first success.
    pub async fn send_best(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError>;

    /// Dispatch an inbound payload to all registered receivers.
    /// Calls `on_receive()` on each receiver in registration order.
    /// Returns first error (fail-fast) or Ok if all succeed.
    pub async fn dispatch(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>;

    /// Return list of healthy transport names.
    pub fn healthy_transports(&self) -> Vec<String>;

    /// Return count of total transports.
    pub fn transport_count(&self) -> usize;
}
```

### Implementation details

#### `broadcast()`

1. Filter to healthy transports (`is_healthy() == true`)
2. Use `futures::future::join_all` to send concurrently
3. Count successes
4. Return success count

#### `send_best()`

1. Iterate transports in order
2. Skip unhealthy (`is_healthy() == false`)
3. Call `send()` on each
4. Return first `Ok(())`
5. If all fail, return `TransportError::AllTransportsFailed`

### What this mission does NOT implement

- `NetworkSender` trait (0863a)
- `PlatformAdapterBridge` (0863a)
- Sync consumer wiring (0863c)
- DotGateway fan-out (0863d — separate concern)

### Dependency addition

Add `futures = "0.3"` to `octo-transport/Cargo.toml` `[dependencies]` section (needed for `futures::future::join_all` in `broadcast()`).

## Acceptance Criteria

- [ ] `NodeTransport::new()` accepts `Vec<Arc<dyn NetworkSender>>`
- [ ] `NodeTransport::broadcast()` sends to all healthy transports concurrently
- [ ] `NodeTransport::broadcast()` returns count of successful sends
- [ ] `NodeTransport::send_best()` tries transports in order, fails over on error
- [ ] `NodeTransport::send_best()` returns `TransportError::AllTransportsFailed` when all fail
- [ ] `NodeTransport::healthy_transports()` returns names of healthy transports only
- [ ] Unhealthy transports are skipped in both `broadcast()` and `send_best()`
- [ ] `NodeTransport::register_receiver()` appends to the `receivers` vec and is safe to call concurrently
- [ ] `NodeTransport::dispatch()` with an empty `receivers` vec returns `Ok(())` (no-op)
- [ ] `NodeTransport::dispatch()` iterates `receivers` in registration order, calling `on_receive()` on each
- [ ] `NodeTransport::dispatch()` fails fast on the first receiver to return `Err` — subsequent receivers are not invoked
- [ ] Unit tests pass: `cargo test -p octo-transport`
- [ ] Clippy clean: `cargo clippy -p octo-transport -- -D warnings`
- [ ] `cargo fmt --check` passes

## Type Coverage

| RFC Type                          | Implemented By |
| --------------------------------- | -------------- |
| `NodeTransport` struct            | This mission   |
| `NodeTransport::broadcast()`      | This mission   |
| `NodeTransport::send_best()`      | This mission   |
| `NodeTransport::register_receiver()` | This mission |
| `NodeTransport::dispatch()`       | This mission   |
| `NodeTransport::receivers` field  | This mission   |

## Complexity

Low (~200-300 lines). `NodeTransport` is a thin wrapper over `Vec<Arc<dyn NetworkSender>>` with health filtering and concurrency.

## Implementation Notes

- `broadcast()` uses `futures::future::join_all` for concurrent send (same pattern as `MultiCarrierSync::broadcast()` in `octo-sync/src/carrier.rs`)
- `send_best()` is sequential — try each transport, return first success
- Health filtering: check `is_healthy()` before calling `send()`
- No health state management in `NodeTransport` itself — delegates to `NetworkSender::is_healthy()`
