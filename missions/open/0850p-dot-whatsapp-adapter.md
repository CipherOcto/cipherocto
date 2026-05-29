# Mission: DOT WhatsApp Adapter

## Status

Rewrite (migrating from Cloud Business API to native WhatsApp Web protocol via `whatsapp-rust`)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Rewrite the WhatsApp DOT adapter to use the native WhatsApp Web protocol via the `whatsapp-rust` crate (oxidezap/whatsapp-rust), replacing the previous WhatsApp Business Cloud API approach. This eliminates the dependency on Meta Business verification and provides end-to-end encryption, QR/pair-code linking, and full WhatsApp Web capabilities (groups, media, presence, reactions).

The design follows ZeroClaw's proven `WhatsAppWebChannel` pattern (see `zeroclaw/crates/zeroclaw-channels/src/whatsapp_web.rs` and `whatsapp_storage.rs`), adapted for DOT envelope transport.

## Why the Rewrite

- **No Meta Business verification required** — the Cloud API requires a verified Meta Business account; whatsapp-rust uses the WhatsApp Web protocol directly
- **E2E encryption** — whatsapp-rust uses the Signal Protocol; the Cloud API has no E2E between bot and user
- **No API token / phone_number_id** — authentication is via QR code or pair code linking
- **Full feature parity** — groups, media, presence, reactions, typing indicators, editing/deletion
- **Battle-tested in ZeroClaw** — ZeroClaw's `WhatsAppWebChannel` has production usage with the same crates

## External Crates (from oxidezap/whatsapp-rust)

Pin to the same revision as ZeroClaw for stability:

```toml
[dependencies]
# WhatsApp Web protocol (oxidezap/whatsapp-rust)
whatsapp-rust = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "9734fb2ec544e22b7055147aa3e73b6889e3ff0d", features = ["tokio-runtime"] }
wacore = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "9734fb2ec544e22b7055147aa3e73b6889e3ff0d" }
wacore-binary = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "9734fb2ec544e22b7055147aa3e73b6889e3ff0d" }
waproto = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "9734fb2ec544e22b7055147aa3e73b6889e3ff0d" }
whatsapp-rust-tokio-transport = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "9734fb2ec544e22b7055147aa3e73b6889e3ff0d" }
whatsapp-rust-ureq-http-client = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "9734fb2ec544e22b7055147aa3e73b6889e3ff0d" }
qrcode = "0.14"
serde-big-array = "0.5"
bytes = "1"

# Shared dependencies (also used by other adapter code)
tokio = { version = "1", features = ["sync", "time", "fs", "rt-multi-thread"] }
async-trait = "0.1"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
blake3 = "1"
base64 = "0.22"
chrono = { version = "0.4", features = ["clock"] }
parking_lot = "0.12"
# SQL storage (CipherOcto stoolap fork — same as quota-router-core)
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
shellexpand = "3"
prost = "0.14"

# DOT types from sibling crate
octo-network = { path = "../octo-network" }

[dev-dependencies]
tempfile = "3"
```

## Acceptance Criteria

### Core Adapter

- [ ] Replace `reqwest`-based Cloud API client with `whatsapp-rust` Bot
- [ ] Implements `PlatformAdapter` trait with ALL required methods:
  - [ ] `send_envelope()` — sends DOT envelopes as WhatsApp Web messages via `Client::send_message()`
  - [ ] `receive_messages()` — drains internal message buffer populated by `Bot::on_event()` (see Architecture)
  - [ ] `canonicalize()` — extracts DOT envelope from WhatsApp message text (strips `DOT/1/` prefix, base64-decodes)
  - [ ] `capabilities()` — returns CapabilityReport
  - [ ] `domain_id()` — returns `BroadcastDomainId(0x0008, BLAKE3(group_id))`
  - [ ] `platform_type()` — returns `PlatformType::WhatsApp`
  - [ ] `replay_protection()` — default (handled by gateway)
  - [ ] `health_check()` — checks bot_handle is alive + client is connected
  - [ ] `shutdown()` — aborts bot handle, flushes pending messages, closes session
  - [ ] `self_handle()` — returns bot's own phone number (resolved from device store on connect) for relay loop prevention

### Session & Storage

- [ ] Session persistence via `stoolap` (CipherOcto's SQL fork, same as quota-router-core)
- [ ] Storage must implement all 4 wa-rs traits: `SignalStore`, `AppSyncStore`, `ProtocolStore`, `DeviceStore`
- [ ] Schema includes: device, identities, sessions, prekeys, signed_prekeys, sender_keys, app_state_keys, app_state_versions, app_state_mutation_macs, lid_pn_mapping, device_registry, sender_key_devices, sent_messages, base_keys, tc_tokens
- [ ] Open database with `stoolap::Database::open(session_path)` — no `Arc<Mutex<>>` needed (stoolap is thread-safe)
- [ ] Parameter placeholders: `$1, $2, ...` (not rusqlite's `?1, ?2, ...`)
- [ ] Parameter values: `Vec<stoolap::Value>` using `stoolap::core::Value::blob()`, `.into()` for primitives, `stoolap::Value::Null(stoolap::DataType::Null)` for NULLs
- [ ] Upsert pattern: INSERT + catch `stoolap::Error::UniqueConstraint` + UPDATE (or DELETE+INSERT)
- [ ] Transactions: `db.begin()` → `tx.query()` / `tx.execute()` → `tx.commit()`
- [ ] Row access: `row.get::<T>(column_index)` with `stoolap::Error` → `wacore::store::error::StoreError::Database(Box::new(e))`
- [ ] No PRAGMA / WAL mode (stoolap handles concurrency internally)
- [ ] Migration support for wacore 0.6 columns (next_pre_key_id, server_has_prekeys, nct_salt, server_cert_chain, login_counter)

### Authentication & Pairing

- [ ] QR code pairing: terminal-rendered QR via `qrcode` crate for initial linking
- [ ] Pair code linking: configurable `pair_phone` in config for code-based linking
- [ ] Optional `pair_code` for custom pair code
- [ ] Session reuse: if session DB exists, load device and reconnect without re-pairing
- [ ] Session purge on `Event::LoggedOut` (remove session database file; stoolap does not use WAL/SHM sidecars)

### Message Handling

- [ ] Extract text from incoming WhatsApp messages using `wacore::proto_helpers::MessageExt::text_content()`
- [ ] Decode DOT envelope from `DOT/1/{base64}` prefix in message text
- [ ] Group JID format: `groupid@g.us` (WhatsApp group suffix)
- [ ] Bot identity resolution: on `Event::Connected`, resolve bot phone from device store (`device.pn`) for `self_handle()`
- [ ] Internal message buffer: `Bot::on_event()` pushes to `tokio::sync::mpsc` channel; `receive_messages()` drains it with timeout

### Configuration

- [ ] `WhatsAppConfig`: `session_path`, `pair_phone` (optional), `pair_code` (optional), `ws_url` (optional, for test/proxy), `groups`
- [ ] `CapabilityReport`: max_payload=65536, supports_encryption=true (Signal Protocol), supports_fragmentation=false, rate_limit_per_second=20
- [ ] `DevicePropsOverride`: os="CipherOcto", platform_type=Desktop

### Reconnection & Resilience

- [ ] Reconnect with exponential backoff (3s base, 300s cap, 10 retries)
- [ ] `Event::LoggedOut` triggers session purge + reconnect
- [ ] `Event::StreamError` logged but does not trigger reconnect
- [ ] Retry counter resets on `Event::Connected`

### Tests

- [ ] Cloud API code (`reqwest`-based) fully removed from the crate
- [ ] Unit tests: domain hash, encode/decode, config, capabilities, JID normalization, reconnect delay, retry counter, session file paths, health check disconnected

## Location

`crates/octo-adapter-whatsapp/`

## Complexity

High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Design Reference

- **ZeroClaw channel**: `zeroclaw/crates/zeroclaw-channels/src/whatsapp_web.rs` (2427 lines, 30+ tests)
- **ZeroClaw storage**: `zeroclaw/crates/zeroclaw-channels/src/whatsapp_storage.rs` (1757 lines, 4 storage traits)
- **ZeroClaw config**: `zeroclaw/crates/zeroclaw-config/src/schema.rs` (WhatsAppConfig, WhatsAppWebMode, WhatsAppChatPolicy)
- **DOT trait**: `crates/octo-network/src/dot/adapters/mod.rs` (PlatformAdapter with 10 methods)

## Implementation Notes

### Architecture

The `PlatformAdapter` trait is pull-based (`receive_messages()` returns `Vec<RawPlatformMessage>`), but whatsapp-rust is event-driven (`Bot::on_event()`). Bridge this gap with an internal channel:

```rust
pub struct WhatsAppWebAdapter {
    // ... config fields ...
    /// Bot handle for shutdown
    bot_handle: Arc<Mutex<Option<whatsapp_rust::bot::BotHandle>>>,
    /// Client for sending messages
    client: Arc<Mutex<Option<Arc<whatsapp_rust::Client>>>>,
    /// Internal message buffer: on_event() pushes, receive_messages() drains
    inbound_rx: Arc<Mutex<tokio::sync::mpsc::Receiver<RawPlatformMessage>>>,
    inbound_tx: tokio::sync::mpsc::Sender<RawPlatformMessage>,
    /// Bot's own phone number (resolved on connect)
    self_phone: Arc<Mutex<Option<String>>>,
}
```

**Sending**: `client.send_message(jid, waproto::whatsapp::Message { conversation: Some(encoded_envelope), .. })` returns `SendResult { message_id, to }`. Map to DOT's `DeliveryReceipt`:
```rust
let send_result = Box::pin(client.send_message(jid, outgoing)).await
    .map_err(|e| PlatformAdapterError::Unreachable {
        platform: "whatsapp".into(),
        reason: format!("send_message failed: {e}"),
    })?;
Ok(DeliveryReceipt {
    platform_message_id: send_result.message_id,
    delivered_at: epoch_millis(),
})
```

**Receiving**: `on_event` closure matches `Event::Message(msg, info)`, extracts text via `msg.text_content()`, wraps in `RawPlatformMessage`, pushes to `inbound_tx`. `receive_messages()` drains `inbound_rx` with a short timeout (100ms).

**Session**: Backed by `stoolap` (CipherOcto's SQL fork, same as quota-router-core). Implements all 4 wa-rs storage traits (SignalStore, AppSyncStore, ProtocolStore, DeviceStore). See ZeroClaw's `whatsapp_storage.rs` for the trait implementations — port from rusqlite to stoolap:

```rust
pub struct StoolapStore {
    db: stoolap::Database,  // thread-safe, no Mutex needed
    device_id: i32,
}

// Error mapping macro (adapting ZeroClaw's to_store_err! pattern)
macro_rules! to_store_err {
    ($expr:expr) => {
        $expr.map_err(|e| wacore::store::error::StoreError::Database(
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        ))
    };
}

// Parameter syntax: $1, $2, ... (not ?1, ?2, ...)
// Upsert: INSERT + catch UniqueConstraint + UPDATE (not INSERT OR REPLACE)
// Transactions: db.begin() → tx.query()/tx.execute() → tx.commit()
```

**JID handling**:
- Phone numbers: `wacore_binary::jid::Jid::pn(digits)` → `digits@s.whatsapp.net`
- Groups: `groupid@g.us` format
- Parse incoming: `chat.split_once('@').map(|(user, domain)| ...)`

**DOT envelope encoding**: Keep existing `DOT/1/{base64}` prefix scheme. Send as WhatsApp text message body. Receive by checking text starts with `DOT/1/`.

**Error mapping**: All whatsapp-rust errors must map to `PlatformAdapterError`:
- Send failures → `PlatformAdapterError::Unreachable { platform: "whatsapp", reason }`
- Bot not connected → `PlatformAdapterError::Unreachable { platform: "whatsapp", reason: "client not connected" }`
- Canonicalize failures → `PlatformAdapterError::ApiError { code: 400, message }`
- Health check failures → `PlatformAdapterError::Unreachable { platform: "whatsapp", reason }`

**Bot lifecycle**:
1. `Bot::builder().with_backend(storage).with_transport_factory(transport).with_http_client(http).with_runtime(TokioRuntime).with_device_props(DevicePropsOverride::new().os("CipherOcto").platform(Desktop)).on_event(handler).build().await`
2. If `pair_phone` set: `builder.with_pair_code(PairCodeOptions { phone_number, custom_code, .. })`
3. `bot.run().await` returns `BotHandle`
4. On `Event::Connected`: resolve `device.pn` for `self_handle()`
5. On `Event::LoggedOut`: set flag, signal reconnect loop

**Reconnect loop** (mirrors ZeroClaw pattern):
```
loop {
    // Build + run bot
    // Wait for logout signal or ctrl-c
    // If logged out: purge session files, increment retry, sleep backoff
    // If ctrl-c: break
    // Reset retry on Event::Connected
}
```

### Key differences from ZeroClaw

- ZeroClaw is a chat agent (messages are natural language); this adapter transports DOT envelopes (binary payloads encoded as base64 text)
- ZeroClaw has DM/group policy filtering (WhatsAppChatPolicy, WhatsAppWebMode); this adapter filters by configured group IDs only
- ZeroClaw supports voice/TTS/STT; this adapter is text-only (DOT envelope transport)
- ZeroClaw has mention patterns and self-chat mode; this adapter has no such filtering
- ZeroClaw resolves LID→phone for reply targeting; this adapter uses group JIDs directly

### Migration path

1. Remove `reqwest` dependency and Cloud API code
2. Add whatsapp-rust dependencies
3. Port storage backend from ZeroClaw's `whatsapp_storage.rs` — rewrite from rusqlite to `stoolap` using the same patterns as `quota-router-core/src/storage.rs`:
   - Replace `Arc<Mutex<Connection>>` with `stoolap::Database` (thread-safe)
   - Replace `?1, ?2, ...` params with `$1, $2, ...`
   - Replace `INSERT OR REPLACE` with INSERT + catch `UniqueConstraint` + UPDATE
   - Replace `conn.transaction()` with `db.begin()` → `tx.commit()`
   - Replace `conn.execute_batch()` with individual `db.execute()` calls
   - Remove all `PRAGMA` statements (stoolap handles internally)
4. Implement `WhatsAppWebAdapter` with Bot-based send/receive
5. Keep existing `domain_hash()`, `encode_envelope()`, `decode_envelope()` helpers
6. Update `WhatsAppConfig` to session-based auth
7. Implement `self_handle()` and `shutdown()` trait methods
