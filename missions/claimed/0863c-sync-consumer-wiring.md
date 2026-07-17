# Mission: 0863c — Sync consumer: wire sync as first consumer + stoolap-node + E2E tests

## Status

Open

## RFC

RFC-0863 (Networking): General-Purpose Network Integration — `octo-transport` — Phase 1: Wire sync as first consumer

## Dependencies

Missions that must be completed before this one:

- 0863a (must be completed) — provides `NetworkSender`, `PlatformAdapterBridge`, `AdapterFactory`
- 0863b (should be completed) — provides `NodeTransport` with `broadcast()` and `send_best()`

## Summary

Wire the sync engine as the first consumer of `octo-transport`, proving the integration pattern works end-to-end. Update the `stoolap-node` binary to accept `--adapter` flags and load platform adapters dynamically. Add L4 cross-transport E2E tests that exercise sync over QUIC and Webhook simultaneously.

## Design

### 1. Sync consumer wiring

The `SyncSessionManager` currently broadcasts via in-memory channels only. This mission adds an optional `NodeTransport` that sync can use to send WAL chunks over platform adapters.

```rust
// In the sync consumer code (stoolap-node or sync engine):
let registry = AdapterRegistry::new(plugin_dirs);
registry.discover_and_load();

let senders = AdapterFactory::from_registry(&registry, default_domain);
let transport = NodeTransport::new(senders);

// Sync engine now has a transport layer
transport.broadcast(wal_chunk_bytes, &send_ctx).await;
```

### 2. Stoolap-node `--adapter` flags

Update `sync-e2e-tests/stoolap-node/src/main.rs` to accept adapter flags:

```
stoolap-node --dsn file://... --listen 3333 --adapter p2p --adapter webhook
```

Each `--adapter` flag:

1. Maps adapter name to `PlatformType` via a hardcoded lookup table:

```rust
fn adapter_name_to_platform_type(name: &str) -> Option<PlatformType> {
    match name.to_lowercase().as_str() {
        "telegram" => Some(PlatformType::Telegram),
        "discord" => Some(PlatformType::Discord),
        "matrix" => Some(PlatformType::Matrix),
        "whatsapp" => Some(PlatformType::WhatsApp),
        "webhook" => Some(PlatformType::Webhook),
        "p2p" | "nativep2p" => Some(PlatformType::NativeP2P),
        "quic" => Some(PlatformType::Quic),
        "signal" => Some(PlatformType::Signal),
        "irc" => Some(PlatformType::Irc),
        "slack" => Some(PlatformType::Slack),
        "nostr" => Some(PlatformType::Nostr),
        "bluesky" => Some(PlatformType::Bluesky),
        "twitter" => Some(PlatformType::Twitter),
        "reddit" => Some(PlatformType::Reddit),
        "wechat" => Some(PlatformType::WeChat),
        "dingtalk" => Some(PlatformType::DingTalk),
        "lark" => Some(PlatformType::Lark),
        "qq" => Some(PlatformType::Qq),
        "bluetooth" => Some(PlatformType::Bluetooth),
        "lora" => Some(PlatformType::LoRa),
        "webrtc" => Some(PlatformType::WebRtc),
        _ => None,
    }
}
```

2. Looks up the adapter in `AdapterRegistry` by `PlatformType` (`registry.get(platform_type as u16)`)
3. Wraps it in `PlatformAdapterBridge`
4. Adds it to `NodeTransport`

### 3. L4 cross-transport E2E tests

Add tests in `sync-e2e-tests/tests/` that exercise sync over multiple transports:

| Test                          | What It Verifies                                        |
| ----------------------------- | ------------------------------------------------------- |
| `l4_quic_transport`           | Sync via QUIC adapter (two `stoolap-node` processes)    |
| `l4_webhook_transport`        | Sync via Webhook adapter (two `stoolap-node` processes) |
| `l4_multi_transport_failover` | QUIC fails → fallback to Webhook                        |
| `l4_plugin_adapter`           | `.so` adapter loaded at runtime, used for sync          |

### What this mission does NOT implement

- `NetworkSender` trait (0863a)
- `PlatformAdapterBridge` (0863a)
- `NodeTransport` (0863b)
- `NetworkReceiver` / DotGateway fan-out (0863d)
- DGP integration (covered by existing mission 0862j)

## Acceptance Criteria

- [ ] `stoolap-node` accepts `--adapter <name>` flags
- [ ] `--adapter` flag loads adapter from `AdapterRegistry` and wraps in `PlatformAdapterBridge`
- [ ] Multiple `--adapter` flags create a `NodeTransport` with multiple senders
- [ ] Sync engine can broadcast WAL chunks via `NodeTransport`
- [ ] L4 test: sync over QUIC adapter (two processes, real TCP/QUIC)
- [ ] L4 test: sync over Webhook adapter (two processes, real HTTP)
- [ ] L4 test: failover from QUIC to Webhook on failure
- [ ] L4 test: plugin-loaded adapter used for sync
- [ ] All existing L3/L4/L5 tests still pass
- [ ] Clippy clean: `cargo clippy -p sync-e2e-tests -- -D warnings`
- [ ] `cargo fmt --check` passes

## Type Coverage

| RFC Type                                  | Implemented By |
| ----------------------------------------- | -------------- |
| `NodeTransport` in production use         | This mission   |
| `PlatformAdapterBridge` in production use | This mission   |
| `AdapterFactory` in production use        | This mission   |
| `--adapter` CLI flags                     | This mission   |
| L4 cross-transport E2E tests              | This mission   |

## Complexity

Medium (~400-600 lines). Stoolap-node updates + E2E test infrastructure + 4 new L4 tests.

## Implementation Notes

- Stoolap-node already has `--peer` flags for TCP. The `--adapter` flags follow the same pattern but load from `AdapterRegistry` instead of raw TCP.
- L4 tests spawn `stoolap-node` child processes with different `--adapter` flags and verify sync convergence.
- The `AdapterFactory::from_registry` method maps adapter names (e.g., "p2p", "webhook") to `PlatformType` enum values for lookup via `adapter_name_to_platform_type()`.
- **QUIC test setup:** Use self-signed TLS certs generated at test time. `QuicConfig` supports `auth_mode: SelfSigned` with temp cert/key files.
- **Webhook test setup:** `WebhookConfig` has a `listen_port` field — use `0` for auto-assign, read the actual port from the adapter after startup. The sender webhook URL points to `http://127.0.0.1:{port}/dot/v1/envelope`.
- **Plugin test setup:** Build a minimal test adapter as a `.so` using the C ABI exports (`adapter_version`, `platform_type`, `create_adapter`, `destroy_adapter`). Place in a temp directory and pass via `--adapter-dirs`.
- This mission proves the pattern for all 27+ use cases. If sync works over QUIC/Webhook via `NodeTransport`, any consumer can do the same.
