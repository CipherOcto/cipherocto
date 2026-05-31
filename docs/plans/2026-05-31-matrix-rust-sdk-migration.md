# Matrix Adapter Migration: In-House → matrix-rust-sdk

**Date:** 2026-05-31  
**Status:** Evaluation  
**Source:** `crates/octo-adapter-matrix/` (in-house) → `matrix-rust-sdk` v0.17.0  
**Local clone:** `/home/mmacedoeu/_w/tools/matrix-rust-sdk` (branch: `stable`, tag: `matrix-sdk-0.17.0`)

---

## 1. Current State Summary

### In-House Adapter (`crates/octo-adapter-matrix/`)

| Aspect | Current |
|--------|---------|
| Files | 2 (`Cargo.toml`, `src/lib.rs`) |
| HTTP client | `reqwest 0.12` (raw) |
| Sync | Manual `GET /_matrix/client/v3/sync` with `next_batch` token |
| Send | Manual `PUT /rooms/{roomId}/send/m.room.message/{txnId}` |
| Media | Manual `POST /_matrix/media/v3/upload` |
| Auth | Manual `Bearer` header on every request |
| Retry | String-matching on error messages ("429", "M_LIMIT_EXCEEDED") |
| Tests | 10 unit tests (pure logic, no HTTP mocks) |
| Plugin ABI | 4 exported C functions (`cdylib`) |
| Lines | ~600 |
| Dependencies | 8 (reqwest, tokio, serde, blake3, base64, uuid, async-trait, octo-network) |

**What it implements well:**
- DOT/1/ envelope encoding/decoding
- BLAKE3 domain hashing with normalization
- Plugin ABI compliance (version 1)
- Exponential backoff with jitter
- Media upload fallback for >65KB payloads

**What it lacks:**
- No connection pooling configuration
- No structured error types (all errors → `String`)
- No mock HTTP tests
- No integration tests against real homeserver
- No E2EE support
- No event handler registration
- No state store / persistence
- Manual auth header management
- String-based retry detection (fragile)

---

## 2. matrix-rust-sdk v0.17.0 Capabilities

### Relevant Crate Structure

| Crate | Purpose | Needed? |
|-------|---------|---------|
| `matrix-sdk` | Main client (Client, Room, Media, sync) | **YES** |
| `matrix-sdk-base` | State store abstractions | Maybe (transitive) |
| `matrix-sdk-common` | Shared types | Maybe (transitive) |
| `matrix-sdk-crypto` | E2EE (Olm/Megolm) | No (unless E2EE needed) |
| `matrix-sdk-sqlite` | SQLite persistence | Optional |
| `matrix-sdk-ffi` | UniFFI bindings (Kotlin/Swift) | No |

### Key APIs Mapped to PlatformAdapter

| PlatformAdapter Method | matrix-rust-sdk Equivalent | Complexity |
|------------------------|---------------------------|------------|
| `send_envelope()` | `room.send(RoomMessageEventContent::text_plain(...))` or `room.send_raw(event_type, json)` | Low |
| `receive_messages()` | `client.sync_once(SyncSettings::default().token(since))` → iterate `SyncResponse.rooms.join` | Low |
| `canonicalize()` | Parse event content body, strip `DOT/1/` prefix, base64 decode | Low (same as current) |
| `capabilities()` | Static values (unchanged) | Trivial |
| `domain_id()` | Static (unchanged) | Trivial |
| `health_check()` | `client.sync_once(SyncSettings::default().timeout(Duration::ZERO))` or `client.whoami()` | Low |
| `shutdown()` | Drop client / stop sync stream | Low |
| `upload_media()` | `client.media().upload(content_type, data, None)` | Low |
| `download_media()` | `client.media().get_media_content(request, false)` | Low |
| `self_handle()` | `client.user_id()` (cached after login) | Trivial |

### Sync Comparison

```
CURRENT (manual):
  GET /_matrix/client/v3/sync?timeout=30000&since={token}
  → parse JSON manually
  → iterate joined_rooms → timeline → events
  → decode DOT/1/ from message body

SDK:
  client.sync_once(SyncSettings::default().token(since).timeout(Duration::from_secs(30)))
  → SyncResponse.rooms.join[room_id].timeline.events
  → each event has .event.deserialize() → typed content
  → extract body, decode DOT/1/
```

### Send Comparison

```
CURRENT (manual):
  PUT /_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}
  Authorization: Bearer {token}
  Content-Type: application/json
  {"msgtype": "m.text", "body": "DOT/1/{b64}"}

SDK:
  let content = RoomMessageEventContent::text_plain(format!("DOT/1/{b64}"));
  room.send(content).await?;
  // OR for custom event type:
  room.send_raw("io.cipherocto.envelope", raw_json).await?;
```

---

## 3. Migration Effort Assessment

### What Changes

| Component | Effort | Notes |
|-----------|--------|-------|
| `Cargo.toml` | Low | Replace `reqwest` with `matrix-sdk = { version = "0.17.0", default-features = false }` |
| `MatrixConfig` | Low | Keep struct, but feed into `ClientBuilder` instead of manual headers |
| `MatrixAdapter` struct | Medium | Replace `reqwest::Client` with `matrix_sdk::Client`, add sync state |
| `send_envelope()` | Low | `room.send()` replaces manual PUT |
| `receive_messages()` | Medium | `sync_once()` replaces manual GET + JSON parsing; need to extract room from response |
| `canonicalize()` | None | Unchanged (DOT/1/ logic is protocol-level) |
| `health_check()` | Low | SDK handles `/versions` automatically |
| `upload_media()` | Low | `client.media().upload()` replaces manual POST |
| `download_media()` | Low | `client.media().get_media_content()` replaces manual GET |
| Plugin ABI (`cdylib`) | **High** | See below |
| Tests | Medium | Need to rewrite against SDK types |

### Critical Blocker: cdylib Plugin ABI

The current adapter exports 4 C functions for dynamic loading:

```rust
#[no_mangle] extern "C" fn adapter_version() -> u32
#[no_mangle] extern "C" fn platform_type() -> u16
#[no_mangle] extern "C" unsafe fn create_adapter(config: *const u8, config_len: usize) -> *mut ()
#[no_mangle] extern "C" unsafe fn destroy_adapter(adapter: *mut ())
```

**Problem:** `matrix-sdk` uses `tokio` runtime internally, `Arc<Client>` is `!Send` across FFI boundaries in some configurations, and the SDK's async runtime expectations may conflict with the plugin host's runtime.

**Solutions:**

1. **Embed tokio runtime in the adapter** — Create a `Runtime` in `create_adapter()`, keep it alive in the adapter struct. All async calls use `runtime.block_on()` or `runtime.spawn()`. This is how `matrix-sdk-ffi` works internally.

2. **Switch to rlib-only** — Drop `cdylib`, link directly. Requires changing the adapter registry to use direct Rust linkage instead of `libloading`. Simpler but loses hot-swappable plugins.

3. **Hybrid: SDK for HTTP, keep DOT logic in-house** — Use `matrix-sdk` only for authentication and HTTP transport, keep the manual DOT/1/ encoding and ABI exports. Minimal change but doesn't fully leverage the SDK.

**Recommendation:** Option 1 (embed tokio runtime) — matches how the official FFI bindings work.

---

## 4. Feature Matrix: Current vs SDK

| Feature | Current | With SDK | Delta |
|---------|---------|----------|-------|
| Send messages | Manual HTTP | SDK `room.send()` | Better error handling, auto-retry |
| Receive messages | Manual sync | SDK `sync_once()` | Structured response types |
| Media upload | Manual POST | SDK `media.upload()` | Authenticated endpoints, caching |
| Media download | Manual GET | SDK `media.get_media_content()` | Auth media fallback |
| Connection pooling | None (default reqwest) | Built-in (hyper) | Improved |
| Auth management | Manual Bearer header | SDK handles token lifecycle | Token refresh, SSO support |
| Error handling | String matching | Typed errors (`HttpError`, `RumaApiError`) | Structured |
| Retry logic | String-matched backoff | SDK built-in + our backoff layer | More robust |
| E2EE | Not supported | Optional feature flag | Future capability |
| State persistence | None | Optional SQLite/IndexedDB | Future capability |
| Event handlers | None | `add_event_handler()` pattern | Future capability |
| Sliding Sync | Not supported | Optional | Future capability |
| Test mocking | None | SDK has `matrix-sdk-test` | Better test infra |
| Binary size | ~2MB (reqwest) | ~8-15MB (SDK + deps) | Larger |
| Compile time | Fast | Slower (ruma codegen) | Notable |

---

## 5. Dependency Impact

### Current Dependencies (8)

```
reqwest, tokio, serde, serde_json, blake3, base64, uuid, async-trait, octo-network
```

### After Migration (estimated)

```
matrix-sdk (brings: hyper, tokio, reqwest, ruma, serde, serde_json, http, url, ...)
blake3, base64, uuid, async-trait, octo-network
```

**Net change:** ~6 direct deps removed, ~20 transitive deps added (via matrix-sdk). The SDK is heavy — it pulls in the full `ruma` type system, `hyper`, `tokio`, and HTTP stack.

### Binary Size Impact

| Config | Estimated Size |
|--------|---------------|
| Current (reqwest) | ~1.5-2 MB |
| SDK minimal (`default-features = false`) | ~5-8 MB |
| SDK with sqlite | ~8-12 MB |
| SDK with e2ee | ~12-18 MB |

For a `cdylib` plugin, size matters. The minimal config is acceptable.

---

## 6. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| cdylib + tokio runtime conflicts | **HIGH** | Embed runtime (matrix-sdk-ffi pattern) |
| SDK upgrade breaking changes | MEDIUM | Pin to 0.17.0, test before upgrading |
| Binary size bloat | MEDIUM | Use `default-features = false` |
| Compile time increase | LOW | Acceptable for correctness gains |
| Loss of DOT/1/ control | LOW | Keep encoding logic in-house, only replace HTTP |
| SDK bugs / upstream issues | LOW | Mature project (Element uses it in production) |

---

## 7. Implementation Plan (If Approved)

### Phase 1: Proof of Concept (1-2 days)

1. Create `crates/octo-adapter-matrix-sdk/` (new crate, keep old for reference)
2. Add `matrix-sdk = { version = "0.17.0", default-features = false }` to Cargo.toml
3. Implement `MatrixAdapter` with embedded tokio runtime
4. Implement `send_envelope()` and `receive_messages()` using SDK
5. Keep DOT/1/ encoding logic, BLAKE3 domain hashing, plugin ABI unchanged
6. Verify cdylib compiles and loads

### Phase 2: Full Implementation (2-3 days)

7. Port all `PlatformAdapter` methods to SDK
8. Port media upload/download to `client.media()`
9. Add structured error handling (SDK errors → `PlatformAdapterError`)
10. Implement `health_check()` and `shutdown()` properly
11. Write unit tests using `matrix-sdk-test` utilities
12. Write integration test against local Synapse/Conduit

### Phase 3: Validation (1 day)

13. Run existing DOT adapter test suite
14. Verify plugin ABI compatibility
15. Test with adapter registry
16. Benchmark binary size and startup time
17. Update mission document

### Phase 4: Cutover (1 day)

18. Archive `octo-adapter-matrix` (old)
19. Rename `octo-adapter-matrix-sdk` → `octo-adapter-matrix`
20. Update workspace Cargo.toml
21. Update RFC-0850 references
22. Re-run full test suite

**Total estimated effort: 5-7 days**

---

## 8. Decision Criteria

### Migrate If:

- E2EE support is planned (SDK handles it natively)
- State persistence is needed (SDK has SQLite/IndexedDB)
- Auth token lifecycle management matters (SSO, token refresh)
- Structured error handling is desired
- Future Matrix features (sliding sync, widgets) are relevant
- Integration with Element ecosystem is valuable

### Keep Current If:

- Minimal binary size is critical (plugin distribution)
- The adapter only needs basic send/receive (no E2EE, no persistence)
- Compile time is a concern
- Full control over HTTP behavior is needed
- The adapter is considered "done" and won't evolve

---

## 9. Recommendation

**Migrate to matrix-rust-sdk** using Phase 1-4 plan.

**Rationale:**
1. Matrix is "the most aligned platform for CipherOcto" (per mission doc) — invest in the integration
2. The SDK handles auth, retry, error handling, and media correctly — reduces maintenance burden
3. E2EE and persistence are natural future requirements for a privacy-focused protocol
4. The `matrix-sdk-ffi` crate proves the cdylib + tokio runtime pattern works
5. Binary size increase is acceptable for a network adapter plugin

**Risk mitigation:** Keep the old adapter archived for reference. The DOT/1/ encoding logic is protocol-level and stays in-house regardless.

---

## Appendix A: Code Mapping

### Current `send_envelope()` (~40 lines)

```rust
// Manual HTTP PUT with Bearer header
let url = format!("{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
    self.config.homeserver_url, room_id, txn_id);
let resp = self.client.put(&url)
    .header("Authorization", format!("Bearer {}", self.config.access_token))
    .json(&serde_json::json!({"msgtype": "m.text", "body": encoded}))
    .send().await?;
```

### SDK Equivalent (~5 lines)

```rust
let room = self.client.get_room(&room_id).ok_or("Room not found")?;
let content = RoomMessageEventContent::text_plain(encoded);
room.send(content).await?;
```

### Current `receive_messages()` (~60 lines)

```rust
// Manual GET /sync with JSON parsing
let url = format!("{}/_matrix/client/v3/sync?timeout=30000", ...);
// ... manual JSON traversal ...
```

### SDK Equivalent (~15 lines)

```rust
let response = self.client.sync_once(
    SyncSettings::default()
        .token(self.next_batch.lock().clone())
        .timeout(Duration::from_secs(30))
).await?;
for (room_id, joined) in &response.rooms.join {
    for event in &joined.timeline.events {
        // extract body, decode DOT/1/
    }
}
```

---

## Appendix B: matrix-rust-sdk Feature Flags

```toml
# Minimal (recommended for DOT adapter)
matrix-sdk = { version = "0.17.0", default-features = false }

# With persistence
matrix-sdk = { version = "0.17.0", default-features = false, features = ["sqlite"] }

# Full (E2EE + persistence)
matrix-sdk = { version = "0.17.0" }

# Available features:
# - e2e-encryption (default)
# - automatic-room-key-forwarding (default)
# - sqlite (default)
# - bundled-sqlite
# - indexeddb
# - markdown
# - sso-login
# - qrcode
# - uniffi
# - federation-api
# - experimental-widgets
# - experimental-search
```
