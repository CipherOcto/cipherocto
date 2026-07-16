# WhatsApp Runtime CLI + MCP — Phase 2.5 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development in-session) to implement this plan task-by-task.

**Goal:** Wire the 18 Phase 2 stubbed inherent methods on `WhatsAppWebAdapter` to real wacore/whatsapp-rust calls. Each method currently returns `Err(Unreachable { reason: "wacore wiring deferred" })`. Replace those returns with the actual waproto message construction + `client.send_message(...)` calls (or equivalent `upload_to_cdn` + send for media).

**Architecture:** Two-layer dispatch in `crates/octo-adapter-whatsapp/src/inherent.rs`. Each method follows the existing `send_document` pattern: clone the `whatsapp_rust::Client` out of the mutex, build the `waproto::whatsapp::Message` variant, dispatch via `client.send_message(jid, outgoing)`, return the message-id (and media_ref_token for media methods). Pure control methods (mark_read, pin, mute, archive, typing, search, info) construct and send the right waproto message without media upload.

**Tech Stack:** `whatsapp-rust` 0.6.0 (existing), `waproto` (existing), `wacore-binary` for JID types (existing).

**Pre-requisites:**
- Branch: `feat/whatsapp-runtime-cli-mcp` (stack on top — Phase 1+2 + cleanup commits)
- Worktree: `.worktrees/whatsapp-runtime-cli-mcp`
- Phase 2 status: 274 tests passing, clippy clean, fmt clean
- All 18 inherent methods exist in `crates/octo-adapter-whatsapp/src/inherent.rs` returning `Err(PlatformAdapterError::Unreachable { reason: "wacore wiring deferred" })`
- Existing `send_document` (adapter.rs:2399) is the canonical reference for media-upload pattern
- Existing `send_message` (adapter.rs:2007) is the canonical reference for text-message pattern

**Acceptance gates:**
- 30 tasks complete (3 batches × 10 tasks each)
- `cargo test -p octo-adapter-whatsapp --lib inherent` green — all 18 methods now have wacore-wired bodies that compile
- All `_checked` wrappers still reject over-size inputs (existing tests pass)
- All other Phase 2 tests still pass (no regressions)
- `cargo clippy -p octo-adapter-whatsapp --all-targets --all-features -- -D warnings` clean
- `cargo fmt -p octo-adapter-whatsapp` clean
- Live-WhatsApp test suite compiles under `--features live-whatsapp` (existing + 6 new live tests added)
- No push, no PR (per user decision 2026-07-05)
- Coverage gate stays at its current level (71.18% lines) — wacore wiring adds *uncovered* code paths because hermetic tests can't drive real WhatsApp calls; the gain is real wiring, not test coverage

**YAGNI:**
- Do NOT add new error variants for wacore-specific failures — `PlatformAdapterError::Unreachable` + reason string is enough
- Do NOT add new event emissions — Phase 3 owns event router
- Do NOT touch the runtime side (`octo-whatsapp`) — Phase 2 handlers already exist and work; only the adapter-side stubs need filling

---

## Part A — Text + control methods (Tasks 1-10)

### Task 1: Wire `send_reaction` to `waproto::whatsapp::Message::reaction_message`

The `ReactionMessage` proto carries `(key: MessageKey, text: String, sender_timestamp_ms: i64)`. Build a `MessageKey { remote_jid: Some(jid), from_me: Some(false), id: Some(msg_id.into()) }`. Reaction text is the emoji (or empty for retraction). `sender_timestamp_ms` = current epoch ms.

### Task 2: Wire `send_poll` to `PollCreationMessage` + `Message::poll_creation_message`

`PollCreationMessage` proto: `{ name: question, options: Vec<PollOption>, selectable_options_count: u32, context_info: Option<ContextInfo> }`. `PollOption { name: option_text }`. `selectable_options_count = 1` for single-vote, `options.len()` for multi.

### Task 3: Wire `send_location` to `LocationMessage`

`LocationMessage { degrees_latitude: f64, degrees_longitude: f64, name: Option<String>, address: Option<String> }`. Build and send.

### Task 4: Wire `edit_message` (text only) to `Message::edited_message`

WhatsApp edits are sent via a separate `MessageKey` + `Message::edited_message` wrapping the new `Message::conversation(Some(new_text))`. Verify waproto field path (likely `edited_message: Some(EditedMessage { message: Some(Message { conversation: Some(text), .. }), ... })`).

### Task 5: Wire `delete_message` to `ProtocolMessage::REVOKE`

`ProtocolMessage { protocol_type: Some(REVOKE), key: Some(MessageKey { remote_jid, from_me, id }) }`. Send as a regular `Message { protocol_message: Some(...) }`.

### Task 6: Wire `mark_read` via `ReceiptMessage`

The wacore surface exposes `client.mark_read(jid, msg_ids)` directly — no message construction needed. Use that path. (If unavailable, fall back to `Message::recipient_message` / `ReadReceiptMessage`.)

### Task 7: Wire `message_search` via StoolapStore (read-only path)

`adapter.message_search(query, peer)` returns `Vec<MessageHit>`. The StoolapStore layer needs a `search_messages(query, peer_jid, limit)` method. Phase 2 ships without event-router persistence so this returns empty Vec unless we wire stoolap. For Phase 2.5, add a stub that scans the in-memory `list_persisted_conversations()` cache (returns the snapshot) and matches `text.contains(query)` (case-insensitive). Return up to 50 hits.

### Task 8: Wire `chat_info` via StoolapStore + metadata lookup

`adapter.chat_info(jid)` returns `Option<ChatInfo>`. Use `list_persisted_conversations()` for DM info. For groups, use the existing `group_metadata` (which calls wacore) if jid is a group; else return None.

### Task 9: Wire chat settings — `set_chat_pinned`, `set_chat_muted`, `set_chat_archived`, `delete_chat`, `send_typing`

- `set_chat_pinned(jid, pinned)`: no wacore pin API — set via `user_settings` config endpoint (out of scope here; leave the stub for Phase 5 hardening). For Phase 2.5: still return `Err(Unreachable)` but with reason `"chat pinning not yet supported by wacore 0.6"`.
- `set_chat_muted(jid, until_epoch_secs)`: same — no wacore API. Leave stub.
- `set_chat_archived(jid, archived)`: same — leave stub.
- `delete_chat(jid)`: client-side operation (no wacore call). Returns `Ok(())` and logs `tracing::info!("chat {jid} cleared locally")`.
- `send_typing(jid, is_typing)`: wacore exposes `client.send_chat_presence(jid, ChatPresence::Composing | ChatPresence::Paused)`. Wire it.

### Task 10: Tests for text + control methods

For each wired method, add a test that:
- Calls the method on a disconnected adapter → expects `Err(Unreachable { reason: "client not connected" })` (the standard "no client" path)
- Validates the proto construction compiles (covered by the wired body itself)

These tests don't drive real wacore calls — they confirm the methods dispatch correctly when no client is bound. Live tests in Part C will exercise the success path.

---

## Part B — Media-upload methods (Tasks 11-20)

### Task 11: Wire `send_image` to `Message::image_message`

Build `ImageMessage { caption: Option<String>, url, mimetype, file_length, file_sha256, media_key, ... }` from `upload_to_cdn(client, data, MediaType::Image, ...)`. Send via `client.send_message`.

### Task 12: Wire `send_video` to `Message::video_message`

Same shape as image, but `MediaType::Video` and `VideoMessage { caption, gif_playback: Some(false), ... }`.

### Task 13: Wire `send_audio` to `Message::audio_message`

`AudioMessage { mimetype: Some("audio/mpeg".into()), file_length, ... }`. Note: voice notes are different — they use `AudioMessage` with `ptt: Some(true)` and a specific Opus mimetype (Phase 2.5 only handles audio files, not voice notes — voice is Task 14).

### Task 14: Wire `send_voice` to `Message::audio_message` (PTT flag)

Same as audio but with `ptt: Some(true)` and `mimetype: Some("audio/ogg; codecs=opus".into())`.

### Task 15: Wire `send_sticker` to `Message::sticker_message`

`StickerMessage { mimetype: Some("image/webp".into()), is_animated: Some(false), ... }`. Sticker is always webp and 1 MiB max (already enforced by `_checked`).

### Task 16: Wire `send_contact` to `Message::contact_message`

`ContactMessage { display_name: Some(...), vcard: Some(vcard_text), ... }`. Read the vcard file's text contents and embed inline (no media upload needed).

### Task 17: Tests for media methods

For each wired method:
- Call on disconnected adapter → `Err(Unreachable)`
- Verify `_checked` wrapper still rejects over-size
- Verify size ceiling constants from `octo_whatsapp::limits` are respected at the `_checked` layer

### Task 18: Add a media-construction unit test

Add a test in `inherent.rs` that constructs each `waproto::whatsapp::Message` directly and verifies the proto shape (using `serde_json::to_value` or `protobuf::Message::write_to_bytes`). Confirms the wacore type usage is correct without needing a live client.

### Task 19: Integration test — wiring compiles end-to-end

Add `crates/octo-adapter-whatsapp/tests/inherent_smoke.rs`:
- Construct a `WhatsAppWebAdapter::new_unconnected_for_tests()`
- Call each of the 18 wired methods
- Expect every call to return `Err(PlatformAdapterError::Unreachable { reason: "client not connected" })`
- Total: 18 test cases

### Task 20: Final compile check across all wacore paths

Run `cargo check -p octo-adapter-whatsapp --tests --all-features` — must compile cleanly. Run `cargo test -p octo-adapter-whatsapp --lib` — must pass.

---

## Part C — Live-WhatsApp tests (Tasks 21-30)

### Task 21-26: Live tests under `live-whatsapp` feature

Create `crates/octo-adapter-whatsapp/tests/live_2_5_wiring.rs` with `#![cfg(feature = "live-whatsapp")]` and tests for:
- `live_send_image_succeeds` — login, send a 1KB image to a test peer, expect non-empty message_id
- `live_send_video_succeeds` — same pattern with a tiny video
- `live_send_audio_succeeds` — send a small audio file
- `live_send_voice_succeeds` — send a PTT voice note
- `live_send_sticker_succeeds` — send a webp sticker
- `live_send_reaction_succeeds` — send emoji reaction to the previous test message

These tests use `assert_cmd::Command` + the existing live test patterns (see `live_e2e_group_setup_test.rs:379`).

### Task 27: Update Cargo.toml `live-whatsapp` CI comment

Update the comment in `Cargo.toml:30-34` to mention the new live wiring test path.

### Task 28: Live build check

Run `cargo check -p octo-adapter-whatsapp --features live-whatsapp --tests` — must compile cleanly.

### Task 29: Final pre-merge verification

Run:
```bash
cargo fmt --all
cargo clippy -p octo-adapter-whatsapp --all-targets --all-features -- -D warnings
cargo test -p octo-adapter-whatsapp --lib
cargo check -p octo-adapter-whatsapp --features live-whatsapp --tests
```

All must pass.

### Task 30: Update handoff memory + design doc status

Append a Phase 2.5 entry to `whatsapp-phase2-handoff.md` and update `MEMORY.md` index line.

---

## Appendix A — File paths quick reference

**Modified:**
- `crates/octo-adapter-whatsapp/src/inherent.rs` (replace 18 stubs with wacore calls)
- `crates/octo-adapter-whatsapp/Cargo.toml` (live test comment update)
- `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` (status update)
- `memory/whatsapp-phase2-handoff.md` (Phase 2.5 entry)
- `memory/MEMORY.md` (index update)

**Created:**
- `crates/octo-adapter-whatsapp/tests/inherent_smoke.rs` (hermetic wiring smoke)
- `crates/octo-adapter-whatsapp/tests/live_2_5_wiring.rs` (live tests)

## Appendix B — Coverage impact

Phase 2.5 INCREASES the absolute number of uncovered lines (each new wacore-wired method adds ~20-30 lines of proto construction that no hermetic test can drive). Line coverage % will stay flat or drop slightly. This is expected: wacore wiring is a runtime-only verification path; live tests exercise it under `--features live-whatsapp`, gated from the default CI gate (matches design §1114: "`live-whatsapp` test paths are excluded from the gate").

## Appendix C — Backward compatibility

- All `_checked` wrappers unchanged in signature or semantics
- All `*_unchecked` methods now actually call wacore (no more `Unreachable` from "wacore wiring deferred")
- The runtime-side RPC handlers (octo-whatsapp) are unchanged
- Live tests are opt-in via `--features live-whatsapp`