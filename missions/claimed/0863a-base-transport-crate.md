# Mission: 0863a — Base: octo-transport crate + NetworkSender + PlatformAdapterBridge + AdapterFactory

## Status

Claimed

## RFC

RFC-0863 (Networking): General-Purpose Network Integration — `octo-transport` — Phase 1: Core Bridge

## Dependencies

Missions that must be completed before this one:

- RFC-0850 accepted (✅) — defines `PlatformAdapter`, `DeterministicEnvelope`, `BroadcastDomainId`
- 0862-base implemented (✅) — `octo-sync` crate with `DatabaseSyncAdapter` trait

## Summary

Create the `octo-transport` leaf workspace and implement the foundational types: `NetworkSender` trait, `PlatformAdapterBridge`, `AdapterFactory`, `SendContext`, and `TransportError`. This is the base mission — all subsequent 0863 missions depend on it.

## Design

### New crate: `octo-transport/`

```
octo-transport/
├── Cargo.toml
├── src/
│   ├── lib.rs              — crate root, re-exports
│   ├── sender.rs           — NetworkSender trait + SendContext + TransportError
│   ├── adapter_bridge.rs   — PlatformAdapterBridge (PlatformAdapter → NetworkSender)
│   └── adapter_factory.rs  — AdapterFactory (AdapterRegistry → Vec<Arc<dyn NetworkSender>>)
```

### Cargo.toml

```toml
[package]
name = "octo-transport"
version = "0.1.0"
edition = "2021"
description = "General-purpose transport integration layer for CipherOcto Network"

[dependencies]
octo-network = { path = "../crates/octo-network" }
octo-sync = { path = "../octo-sync" }
async-trait = "0.1"
thiserror = "1.0"
parking_lot = "0.12"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
```

### Types to implement

#### `NetworkSender` trait (`sender.rs`)

```rust
#[async_trait]
pub trait NetworkSender: Send + Sync {
    async fn send(&self, payload: &[u8], context: &SendContext) -> Result<(), TransportError>;
    fn name(&self) -> &str;
    fn is_healthy(&self) -> bool;
}

pub struct SendContext {
    pub mission_id: [u8; 32],
    pub domain: Option<BroadcastDomainId>,
    pub priority: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("adapter failure: {0}")]
    AdapterFailure(String),
    #[error("all transports failed")]
    AllTransportsFailed,
    #[error("envelope construction failed: {0}")]
    EnvelopeConstruction(String),
    #[error("transport unhealthy")]
    Unhealthy,
}
```

#### `PlatformAdapterBridge` (`adapter_bridge.rs`)

```rust
pub struct PlatformAdapterBridge {
    adapter: Arc<dyn PlatformAdapter>,
    domain: BroadcastDomainId,
}

impl PlatformAdapterBridge {
    pub fn new(adapter: Arc<dyn PlatformAdapter>, domain: BroadcastDomainId) -> Self;
}

#[async_trait]
impl NetworkSender for PlatformAdapterBridge {
    fn name(&self) -> &str;
    async fn send(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError>;
    fn is_healthy(&self) -> bool;
}
```

#### `AdapterFactory` (`adapter_factory.rs`)

```rust
pub struct AdapterFactory;

impl AdapterFactory {
    /// Create NetworkSenders from all registered adapters in the registry.
    pub fn from_registry(registry: &AdapterRegistry, default_domain: BroadcastDomainId)
        -> Vec<Arc<dyn NetworkSender>>;
}
```

### What this mission does NOT implement

- `NodeTransport` (0863b)
- Sync consumer wiring (0863c)
- `NetworkReceiver` / DotGateway fan-out (0863d)

## Acceptance Criteria

- [ ] `octo-transport/Cargo.toml` exists with correct dependencies
- [ ] `octo-transport/src/lib.rs` exists with module declarations and re-exports
- [ ] `NetworkSender` trait defined with 3 methods: `send`, `name`, `is_healthy`
- [ ] `SendContext` struct defined with `mission_id`, `domain`, `priority`
- [ ] `TransportError` enum defined with 4 variants
- [ ] `PlatformAdapterBridge` implements `NetworkSender` for any `PlatformAdapter`
- [ ] `PlatformAdapterBridge::send` constructs `DeterministicEnvelope` and calls `adapter.send_message()`
- [ ] `AdapterFactory::from_registry` produces `Vec<Arc<dyn NetworkSender>>` from `AdapterRegistry`
- [ ] Unit tests pass: `cargo test -p octo-transport`
- [ ] Clippy clean: `cargo clippy -p octo-transport -- -D warnings`
- [ ] `cargo fmt --check` passes

## Type Coverage

| RFC Type                       | Implemented By |
| ------------------------------ | -------------- |
| `NetworkSender` trait          | This mission   |
| `SendContext` struct           | This mission   |
| `TransportError` enum          | This mission   |
| `PlatformAdapterBridge` struct | This mission   |
| `AdapterFactory` struct        | This mission   |
| `NodeTransport` struct         | 0863b          |
| `NetworkReceiver` trait        | 0863d          |

## Complexity

Low-Medium (~300-400 lines). Core trait + bridge + factory + error types + tests.

## Implementation Notes

- `PlatformAdapterBridge::send` must construct a `DeterministicEnvelope` from the raw payload. This requires: envelope ID (BLAKE3 of payload), source key (mission-scoped), TTL, flags. Reference `DeterministicEnvelope::new()` in `octo-network/src/dot/envelope.rs`.
- `AdapterFactory` iterates `AdapterRegistry::registered_types()`, calls `get()` for each, wraps in `PlatformAdapterBridge`. Filters out unhealthy adapters.
- The crate depends on both `octo-network` (for `PlatformAdapter`, `AdapterRegistry`, `DeterministicEnvelope`) and `octo-sync` (for type compatibility). Neither upstream depends on `octo-transport`.
- Follow the `octo-determin` / `octo-sync` leaf workspace pattern.
