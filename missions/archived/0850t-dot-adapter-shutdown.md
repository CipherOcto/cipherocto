# Mission: Add shutdown() to DOT Adapters

## Status

Implemented (9 adapters: telegram=9, discord=9, matrix=11, slack=13, irc=24, signal=8, nostr=13, bluetooth=11, lora=16 tests)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.4

## Summary

Implement `shutdown()` override on DOT adapters that have persistent connections or background tasks. The default `shutdown()` is a no-op — adapters that hold connections, WebSocket streams, or bot handles need explicit cleanup.

## Why

Without `shutdown()`, adapters may leak connections, leave background tasks running, or fail to flush pending messages when the gateway stops. This causes resource leaks and potential message loss.

## Claimant

@agent (Jcode)

## Acceptance Criteria

### Telegram (`crates/octo-adapter-telegram/`)

- [x] Abort long-polling task if running
- [x] Flush any pending outbound messages
- [x] Clear cached state (last_update_id)

### Discord (`crates/octo-adapter-discord/`)

- [x] Close WebSocket gateway connection if open
- [x] Flush any pending outbound messages
- [x] Clear cached state

### Matrix (`crates/octo-adapter-matrix/`)

- [x] Stop sync loop if running
- [x] Flush any pending outbound messages
- [x] Clear sync token cache

### Slack (`crates/octo-adapter-slack/`)

- [x] Stop polling loop if running
- [x] Flush any pending outbound messages
- [x] Clear cached state (last_ts)

### IRC (`crates/octo-adapter-irc/`)

- [x] Send QUIT command to IRC server
- [x] Close TCP/TLS connection
- [x] Clear cached state

### Signal (`crates/octo-adapter-signal/`)

- [x] Stop signal-cli subprocess if spawned
- [x] Flush any pending outbound messages

### Nostr (`crates/octo-adapter-nostr/`)

- [x] Close relay WebSocket connections
- [x] Unsubscribe from active subscriptions
- [x] Clear cached state

### WhatsApp (`crates/octo-adapter-whatsapp/`)

- [x] Already implemented (abort bot handle, clear client)

### Webhook (`crates/octo-adapter-webhook/`)

- [x] No-op (stateless, no persistent connections)

### Bluetooth (`crates/octo-adapter-bluetooth/`)

- [x] Close BLE connection if open
- [x] Clear cached state

### LoRa (`crates/octo-adapter-lora/`)

- [x] Close serial connection if open
- [x] Clear cached state

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
