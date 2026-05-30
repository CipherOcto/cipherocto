# Mission: Add shutdown() to DOT Adapters

## Status

Open

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.4

## Summary

Implement `shutdown()` override on DOT adapters that have persistent connections or background tasks. The default `shutdown()` is a no-op — adapters that hold connections, WebSocket streams, or bot handles need explicit cleanup.

## Why

Without `shutdown()`, adapters may leak connections, leave background tasks running, or fail to flush pending messages when the gateway stops. This causes resource leaks and potential message loss.

## Acceptance Criteria

### Telegram (`crates/octo-adapter-telegram/`)

- [ ] Abort long-polling task if running
- [ ] Flush any pending outbound messages
- [ ] Clear cached state (last_update_id)

### Discord (`crates/octo-adapter-discord/`)

- [ ] Close WebSocket gateway connection if open
- [ ] Flush any pending outbound messages
- [ ] Clear cached state

### Matrix (`crates/octo-adapter-matrix/`)

- [ ] Stop sync loop if running
- [ ] Flush any pending outbound messages
- [ ] Clear sync token cache

### Slack (`crates/octo-adapter-slack/`)

- [ ] Stop polling loop if running
- [ ] Flush any pending outbound messages
- [ ] Clear cached state (last_ts)

### IRC (`crates/octo-adapter-irc/`)

- [ ] Send QUIT command to IRC server
- [ ] Close TCP/TLS connection
- [ ] Clear cached state

### Signal (`crates/octo-adapter-signal/`)

- [ ] Stop signal-cli subprocess if spawned
- [ ] Flush any pending outbound messages

### Nostr (`crates/octo-adapter-nostr/`)

- [ ] Close relay WebSocket connections
- [ ] Unsubscribe from active subscriptions
- [ ] Clear cached state

### WhatsApp (`crates/octo-adapter-whatsapp/`)

- [ ] Already implemented (abort bot handle, clear client)

### Webhook (`crates/octo-adapter-webhook/`)

- [ ] No-op (stateless, no persistent connections)

### Bluetooth (`crates/octo-adapter-bluetooth/`)

- [ ] Close BLE connection if open
- [ ] Clear cached state

### LoRa (`crates/octo-adapter-lora/`)

- [ ] Close serial connection if open
- [ ] Clear cached state

## Design Reference

- **ZeroClaw pattern**: No explicit shutdown in ZeroClaw channels (they rely on Drop)
- **CipherOcto trait**: `async fn shutdown(&self) -> Result<(), PlatformAdapterError>` in `crates/octo-network/src/dot/adapters/mod.rs`
- **CipherOcto WhatsApp**: Already implements shutdown in `crates/octo-adapter-whatsapp/src/adapter.rs`

## Implementation Notes

- Use `Arc<Mutex<Option<JoinHandle>>>` pattern for background tasks (same as WhatsApp)
- Set a shutdown flag to prevent new sends during shutdown
- Flush pending messages before closing connections
- Log shutdown progress for debugging

## Location

- `crates/octo-adapter-telegram/src/lib.rs`
- `crates/octo-adapter-discord/src/lib.rs`
- `crates/octo-adapter-matrix/src/lib.rs`
- `crates/octo-adapter-slack/src/lib.rs`
- `crates/octo-adapter-irc/src/lib.rs`
- `crates/octo-adapter-signal/src/lib.rs`
- `crates/octo-adapter-nostr/src/lib.rs`

## Complexity

Medium

## Prerequisites

None
