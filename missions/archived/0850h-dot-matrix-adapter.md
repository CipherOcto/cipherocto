# Mission: DOT Matrix Adapter

## Status

Implemented (migrated to matrix-rust-sdk v0.17.0, 13 tests)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Implement a pure Rust Matrix Client-Server API adapter as a `cdylib` plugin. Matrix is the most aligned platform for CipherOcto — it is itself federated and decentralized.

## Design

See `docs/plans/2026-05-28-social-platform-transport-adapters-design.md` — Matrix Adapter.

## Acceptance Criteria

- [x] `crates/octo-adapter-matrix-sdk/` crate compiles to `cdylib` (matrix-rust-sdk v0.17.0)
- [x] Implements `PlatformAdapter` trait with all methods (6 required + 5 optional: replay_protection, health_check, shutdown, upload_media, download_media)
- [x] `send_envelope()` uses `room.send()` via matrix-rust-sdk
- [x] `receive_messages()` uses `client.sync_once()` with `since` token for incremental sync
- [x] `canonicalize()` extracts envelope from Matrix event content
- [x] Fragmentation: media upload for >65KB payloads via `client.media().upload()`
- [x] `CapabilityReport`: max_payload=65536, rate_limit=100/sec
- [x] `domain_id()`: `BroadcastDomainId(0x0003, BLAKE3(room_id))`
- [x] Config: `homeserver_url`, `access_token`, `rooms` (list of room IDs)
- [x] Error handling: structured errors from SDK, rate limiting
- [x] Exponential backoff with jitter: initial=1s, max=120s, jitter=0-500ms
- [x] Unit tests (13 passing)
- [ ] Integration test against Synapse/Conduit test homeserver

## Location

`crates/octo-adapter-matrix-sdk/` (replaces `crates/octo-adapter-matrix/`)

## Complexity

Medium

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI

## Implementation Notes

- **SDK:** matrix-rust-sdk v0.17.0 with `default-features = false` (no E2EE, no SQLite)
- **Runtime:** Embedded tokio Runtime for cdylib compatibility (same pattern as matrix-sdk-ffi)
- Matrix room IDs: `!abcdef:example.com` — include server name in hash for federation
- Sync: `client.sync_once(SyncSettings)` with `since` token for incremental sync
- Send: `room.send(RoomMessageEventContent::text_plain(encoded))` via SDK
- Media: `client.media().upload()` / `client.media().get_media_content()` via SDK
- Raw JSON parsing for event body extraction (avoids complex SDK type system)
- Federation: messages propagate across homeservers automatically — no extra work needed
- Access token: long-lived, obtained via login or registration API
- Old in-house adapter preserved at `crates/octo-adapter-matrix/` for reference

## Additional Requirements (from Audit)

- [x] Implement `self_handle()` for relay loop prevention (see Mission 0850s)
- [x] Implement `shutdown()` for graceful cleanup (see Mission 0850t)
- [x] Add tests to match ZeroClaw coverage (see Mission 0850u)
- [x] Implement `upload_media()` and `download_media()` via SDK media API
- [x] Health check via `sync_once()` + `whoami()`

## Migration Notes (2026-05-31)

**From:** In-house reqwest-based adapter (`crates/octo-adapter-matrix/`)
**To:** matrix-rust-sdk v0.17.0 (`crates/octo-adapter-matrix-sdk/`)

**Benefits:**
- Official SDK maintained by the Matrix.org Foundation
- Better federation support and authenticated media endpoints
- Structured error types (vs string matching)
- Built-in connection pooling and retry logic
- Future E2EE support via feature flag
- Future persistence (SQLite/IndexedDB) via feature flag

**Binary size:** ~5-8MB (minimal features) vs ~2MB (old reqwest-only)

**Migration plan:** `docs/plans/2026-05-31-matrix-rust-sdk-migration.md`
