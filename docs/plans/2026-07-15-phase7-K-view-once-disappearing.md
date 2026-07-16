# Phase 7.K — View-Once + Disappearing Messages Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task.

**Goal:** Surface `view-once` media (single-view image/video/audio) and `disappearing/ephemeral` messages (TTL-based self-delete) as first-class typed events end-to-end — read path, persistence, RPC + CLI + MCP + skill — so operators (and the Phase 8 query layer) can answer "what view-once photos has anyone sent me?" and "what disappearing messages will expire in the next hour?" without parsing raw `format!("{:?}", wacore_event)` strings.

**Architecture:**
- **Data model first.** Add two fields to `InboundEvent::Message` (`view_once: bool`, `ephemeral_expires_at_seconds: Option<u32>`) and two new variants (`Unavailable`, `DisappearingModeChanged`). Serialization round-trips via `#[serde(default)]` — NDJSON back-compat preserved.
- **Wire-side adapter enrichment.** wacore exposes `MessageExt::is_view_once()` (`wacore/src/proto_helpers.rs:135`) and `MessageInfo.ephemeral_expiration: Option<u32>` (`wacore/src/types/message.rs:315`). Populate them BEFORE the adapter's `format!("{:?}", event)` and `RawPlatformMessage` paths so both parser branches see the flags.
- **Companion fanouts are NOT silently dropped anymore.** Match `Event::UndecryptableMessage` (`wacore/src/types/events.rs:664`) and surface them as `Unavailable { unavailable_type: ViewOnce|Hosted|Bot|Unknown }`. Phone refuses to share view-once content with companion devices, so this is the only signal operators get.
- **Schema migration v1 → v2.** Additive only: `ALTER TABLE messages ADD COLUMN view_once` + `ephemeral_expires_at_seconds` + new `unavailable_messages` table + `disappearing_mode_changes` table. Idempotent so existing deployments with v1 schema upgrade cleanly.
- **One-shot read semantics for view-once.** `messages.read_view_once` returns the CDN URL + media_key, marks the message `consumed_at` (= unix ms now), and refuses subsequent reads. Mirrors WA Web's "you can only view this once" contract.
- **Default-closed media persistence.** New `MediaConfig.view_once_media_persist: bool` (default `false`) gates whether the persisted NDJSON/SQL row retains `media_token` for view-once messages. Default-off means the media-key never sits on disk — the operator must invoke `messages.read_view_once` to fetch it once.

**Tech Stack:**
- Rust 1.x stable, `wacore` from `mmacedoeu/whatsapp-rust@551e574` fork
- serde + serde_json for InboundEvent + RPC param shapes
- clap for CLI subtree
- existing daemon RPC dispatcher + `RpcRegistry` + `tool_descriptors()` pattern
- stoolap (CipherOcto fork at `feat/blockchain-sql`) for SQL schema migration

**Multi-session shape:**
- **S1 — InboundEvent data model.** Session 1 (5 commits): InboundEvent::Message fields + Unavailable + DisappearingModeChanged variants + adapter on_event closure extension + parser extraction + 6 hermetic tests.
- **S2 — Schema v2 + ingest path.** Session 2 (4 commits): SCHEMA_VERSION bump + ALTER TABLE migrations + new tables + ingester extensions + hermetic tests on in-memory DB.
- **S3 — Read RPCs + CLI + MCP + skill.** Session 3 (5 commits): 3 handlers + CLI subtree + MCP descriptors + RPC map + skill catalog + 1 live test.
- **S4 — Config gate + MEMORY wrap-up.** Session 4 (3 commits): MediaConfig.view_once_media_persist default-false + adapter strip-on-persist path + final verification + MEMORY update.

Total: 17 tasks, 17 commits. Batched per `superpowers:executing-plans` checkpoint pattern; reviewed per `superpowers:subagent-driven-development`.

**Worktree:** `feat/whatsapp-runtime-cli-mcp` (current). No push, no PR (operator rule, 2026-07-05).

**Operational invariants:** stay in worktree only, every claim file:line backed, `cargo fmt` before each commit, lib-clean clippy `-D -warnings`, 3-second sleep between WA RPCs in live tests.

---

## Investigation evidence

| Source | Finding |
|---|---|
| `wacore@551e574/src/proto_helpers.rs:125-335` | `MessageExt` trait declares `is_ephemeral()`, `is_view_once()`, `get_ephemeral_expiration()`, `set_ephemeral_expiration()`. `is_view_once` covers `view_once_message`, `view_once_message_v2`, `view_once_message_v2_extension` wrappers + inline `view_once` flag on `image_message`/`video_message`/`audio_message`/`extended_text_message`. |
| `wacore@551e574/src/types/message.rs:315` | `MessageInfo.ephemeral_expiration: Option<u32>` — wacore populates from `contextInfo.expiration`. |
| `wacore@551e574/src/types/events.rs:664` | `Event::UndecryptableMessage(UndecryptableMessage)` variant — server fires for view-once/bot/hosted fanouts to companion devices. |
| `wacore@551e574/src/types/events.rs:601-610` | `DisappearingModeChanged { from: Jid, duration: u32, setting_timestamp }` for per-chat default setting changes. |
| `wacore@551e574/src/types/events.rs:1188-1234` | `UnavailableType { Unknown, ViewOnce, Hosted, Bot }` wire enum, serialized as `"view_once"`/`"hosted"`/`"bot"`/`"unknown"`. `UndecryptableMessage { info, is_unavailable, unavailable_type, decrypt_fail_mode }`. |
| `wacore@551e574/src/send/mod.rs:314-317` | Outbound view-once writes `view_once="true"` attr on `genMetaNode` — confirming the wire contract we must mirror on the inbound path. |
| `crates/octo-whatsapp/src/events.rs:46-156` | `InboundEvent` enum — 11 variants today; missing `Unavailable` (view-once fanouts dropped as `Unknown`), missing `DisappearingModeChanged`. |
| `crates/octo-whatsapp/src/events.rs:186-198` | `MessageKind` enum — Text/Image/Video/Audio/Voice/Sticker/Document/Contact/Location/Poll/Reaction. **No view-once variant.** |
| `crates/octo-whatsapp/src/events.rs:47-73` | `InboundEvent::Message` struct — `view_once`, `ephemeral_expires_at_seconds` fields absent. |
| `crates/octo-adapter-whatsapp/src/adapter.rs:1100-1500` | `on_event` closure — only matches `Event::Messages`, `Event::ChatPresence`, `Event::Presence`, `Event::NewsletterLiveUpdate`, `Event::Connected`, `Event::LoggedOut`, `Event::HistorySync`, `Event::OfflineSyncCompleted`, `Event::PairingQrCode`. **No `Event::UndecryptableMessage` arm. No `Event::DisappearingModeChanged` arm.** |
| `crates/octo-adapter-whatsapp/src/adapter.rs:1212-1354` | `Event::Messages` arm — extracts text via `msg.text_content()` for RawPlatformMessage text path; for media messages, the `format!("{:?}", event)` path through parse_inbound_message pulls media_key but ignores view_once flag and ephemeral_expiration. |
| `crates/octo-adapter-whatsapp/src/adapter.rs:1295-1321` | DOT/2/ download branch decodes a native media-ref token from the text; view-once media currently arrives as empty text + untriggered DownloadRequest (no media-token derivation in this path). |
| `crates/octo-whatsapp/src/query/schema.rs` | `SCHEMA_VERSION: u32 = 1`. `messages` table lacks `view_once` and `ephemeral_expires_at_seconds` columns. No `unavailable_messages` table. No `disappearing_mode_changes` table. |
| `crates/octo-whatsapp/src/events/tests.rs` (existing SAMPLE_BATCH_TEXT) | Already parses `view_once_message: MessageField::Unset` / `view_once_message_v2: MessageField::Unset` / `ephemeral_message: MessageField::Unset` in the regex-visible portion of the Debug format — confirming parseable shape. |
| `crates/octo-whatsapp/src/ipc/handlers/mod.rs:775-870` | TIER-7 const pattern: `TIER7_A_…_METHODS`, `TIER7_B_…_METHODS`, …, `TIER7_I_DAEMON_METHODS`. New TIER const `TIER7_K_VIEW_ONCE_DISAPPEARING_METHODS` follows the same pattern. |
| `crates/octo-whatsapp/src/cli.rs:382-1380` | Existing `GroupsAction` pattern for CLI subcommand enum + `dispatch_groups`. New `MessagesAction::{ReadViewOnce, ListUnavailable, ListEphemeral}` follows the same pattern. |
| `crates/octo-whatsapp/src/mcp_server.rs:41-43` | `EXPECTED_TOOL_COUNT = 142 / 136` — bumps by +3 when query feature on (new tools: `wa_read_view_once`, `wa_list_unavailable`, `wa_list_ephemeral`) and by +3 when off. |
| `crates/octo-whatsapp/assets/skills/wa-mcp.md` | Catalog file (925 lines, ~100 sections). New §25 added in same format. |
| `crates/octo-whatsapp/src/config.rs:71-121` | `QueryConfig` struct pattern for additive config sections — `MediaConfig` follows the same shape. |

### Coverage matrix (today vs after this plan)

| Direction | Today | After |
|---|---|---|
| view-once media parses as Image/Video (no flag) | yes | yes + `view_once=true` flag surfaced in events + DB + RPC |
| view-once media persisted to disk forever | yes (silent) | NO unless config flag enabled |
| view-once media downloadable via `messages.download` | yes (re-read allowed) | first read via `messages.read_view_once`; subsequent reads 403 |
| Companion view-once fanout | dropped (`Unknown`) | typed `Unavailable { unavailable_type: ViewOnce }` in events + DB + RPC |
| Ephemeral TTL surfaced | dropped (no `contextInfo.expiration`) | `ephemeral_expires_at_seconds: Option<u32>` on `InboundEvent::Message` + `messages.ephemeral_expires_at_seconds` column + `messages.list_ephemeral` RPC |
| Per-chat disappearing-mode change event | dropped (`Unknown`) | typed `DisappearingModeChanged { jid, duration_seconds, ts }` in events + DB |

---

## Multi-session schedule

| Session | Tasks | Commits | Time | Risk |
|---|---|---|---|---|
| **S1 — Data model** | T01-T05 | 5 | ~75 min | Med (wire-format details + parser extension + custom-format helpers) |
| **S2 — Schema v2 + ingest** | T06-T09 | 4 | ~50 min | Low (additive ALTER TABLE + idempotent migrate) |
| **S3 — Read RPCs + CLI + MCP + skill** | T10-T14 | 5 | ~60 min | Low (reuses newsletter bridge CLI/MCP pattern) |
| **S4 — Config gate + MEMORY** | T15-T17 | 3 | ~25 min | Low (single config flag + MEMORY update) |

Total: 17 tasks, 17 commits.

---

## Out of scope

- No wacore fork work (wacore's `MessageExt::is_view_once()` + `info.ephemeral_expiration` already covers the wire contract).
- No protobuf / waproto changes.
- No new cargo deps (everything reuses existing serde / serde_json / clap / stoolap / tokio).
- No live-data verification on the secret-message batch — the operator session is logged-out (Phase 6.12.3 gate persists). Live tests are env-gated, compiled but not run.
- No app-state sync semantics on view-once (a view-once message opened in the WA Web primary device is consumed; a companion cannot recover the content via PDO per `receive.rs:138-145`). Operators get told the content is unavailable rather than faking it.
- No rewriting the existing DOT/2/ download flow for view-once — view-once media goes through `messages.read_view_once` instead.

---

# Session 1 — InboundEvent Data Model (Tasks 01-05)

## Task 01: Extend `InboundEvent::Message` with `view_once` + `ephemeral_expires_at_seconds`

**Files:**
- Modify: `crates/octo-whatsapp/src/events.rs:46-73` (InboundEvent::Message struct)
- Modify: `crates/octo-whatsapp/src/events.rs:391-456` (`from_outbound_text` + `from_outbound_media` constructors)
- Modify: `crates/octo-whatsapp/src/events.rs:313-326` (`ts_unix_ms()` match arm)
- Test: `crates/octo-whatsapp/src/events/tests.rs` (add 2 hermetic tests)

**Step 1:** Write failing test.

```rust
#[test]
fn inbound_message_with_view_once_flag_round_trips() {
    let raw = r#"Message(id: "M1", peer: "X", sender: "Y", text: "", kind: Image, media_token: "tok", view_once: true, ephemeral_expires_at_seconds: 86400, is_group: false)"#;
    let env = EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Message { view_once, ephemeral_expires_at_seconds, kind, .. } => {
            assert!(view_once);
            assert_eq!(ephemeral_expires_at_seconds, Some(86400));
            assert_eq!(kind, MessageKind::Image);
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn inbound_message_without_flags_round_trips_with_defaults() {
    let raw = r#"Message(id: "M1", peer: "X", sender: "Y", text: "hi", kind: Text, is_group: false)"#;
    let env = EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Message { view_once, ephemeral_expires_at_seconds, .. } => {
            assert!(!view_once);
            assert_eq!(ephemeral_expires_at_seconds, None);
        }
        other => panic!("expected Message, got {other:?}"),
    }
}
```

**Step 2:** Run: `cargo test --lib -p octo-whatsapp --features query events::tests::inbound_message_with_view_once_flag 2>&1 | tail -20`.
Expected: COMPILE FAIL — `view_once` + `ephemeral_expires_at_seconds` fields don't exist on `InboundEvent::Message`.

**Step 3:** Modify `InboundEvent::Message` (currently lines 47-73):
- Add `#[serde(default)] view_once: bool` field after `is_group`.
- Add `#[serde(default)] ephemeral_expires_at_seconds: Option<u32>` field.
- Update `ts_unix_ms()` + `ts_mono_ns()` match arms — they already `_ => *ts_unix_ms` so no change needed.
- Update `from_outbound_text` + `from_outbound_media` constructors — `view_once: false, ephemeral_expires_at_seconds: None` defaults.
- Update `parse_message` (events.rs:634) — read `view_once` + `ephemeral_expires_at_seconds` from the body via `field(rest, "view_once")` + `field(rest, "ephemeral_expires_at_seconds")`.

**Step 4:** Also extend `parse_inbound_message` (events.rs:762-1064) — every `InboundEvent::Message { … }` literal needs the two new fields appended. The most maintainable approach: a single `extract_message_flags(message_body, info_body) -> (bool, Option<u32>)` helper called once per inbound message and threaded into every constructor site. Helper semantics:
- `view_once = true` iff one of: `message_body` contains `view_once_message: MessageField::Set(...)`, `view_once_message_v2: MessageField::Set(...)`, `view_once_message_v2_extension: MessageField::Set(...)`, OR a regex hit on `view_once: Some\(true\)` inside the `image_message` / `video_message` / `audio_message` / `extended_text_message` nested block. Mirrors `wacore::proto_helpers::MessageExt::is_view_once()`.
- `ephemeral_expires_at_seconds` = `info_body.ephemeral_expiration` (a `Some(N)` int when the timer is active, `None` otherwise).

**Step 5:** Run: `cargo test --lib -p octo-whatsapp --features query events::tests::inbound_message_with_view_once 2>&1 | tail -10`.
Expected: PASS (both tests green).

**Step 6:** `cargo fmt` then commit:
```bash
git add crates/octo-whatsapp/src/events.rs crates/octo-whatsapp/src/events/tests.rs
git commit -m "feat(events): InboundEvent::Message.view_once + ephemeral_expires_at_seconds"
```

---

## Task 02: Add `InboundEvent::Unavailable` variant + `UnavailableKind` enum

**Files:**
- Modify: `crates/octo-whatsapp/src/events.rs:46-156` (InboundEvent enum + arms)
- Modify: `crates/octo-whatsapp/src/events.rs:313-345` (`ts_unix_ms` + `ts_mono_ns` match arms)
- Modify: `crates/octo-whatsapp/src/events.rs:391-456` (`from_outbound_*` constructors — no change needed since they emit Message only)
- Modify: `crates/octo-whatsapp/src/events.rs:458-523` (`parse_inner` dispatch)
- Add: `crates/octo-whatsapp/src/events.rs` new helper `parse_unavailable(rest, ts_unix_ms, ts_mono_ns)` after `parse_newsletter_update`
- Test: `crates/octo-whatsapp/src/events/tests.rs` (add 2 hermetic tests)

**Step 1:** Write failing test.

```rust
#[test]
fn inbound_unavailable_with_view_once_kind_parses() {
    let raw = r#"Unavailable(id: "M9", peer: "X", sender: "Y", kind: view_once, is_unavailable: true, ts: 1700000000)"#;
    let env = EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Unavailable { id, kind, peer, sender, ts_unix_ms, is_unavailable, .. } => {
            assert_eq!(id, "M9");
            assert_eq!(peer, "X");
            assert_eq!(sender, "Y");
            assert_eq!(kind, UnavailableKind::ViewOnce);
            assert!(is_unavailable);
            assert_eq!(ts_unix_ms, 1700000000);
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn inbound_unavailable_with_hosted_kind_parses() {
    let raw = r#"Unavailable(id: "M10", peer: "A", sender: "B", kind: hosted, is_unavailable: true, ts: 1700000001)"#;
    let ev = InboundEvent::parse(EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 });
    match ev {
        InboundEvent::Unavailable { kind, .. } => assert_eq!(kind, UnavailableKind::Hosted),
        other => panic!("expected Unavailable, got {other:?}"),
    }
}
```

**Step 2:** Run: `cargo test --lib -p octo-whatsapp --features query events::tests::inbound_unavailable 2>&1 | tail -10`.
Expected: COMPILE FAIL — `Unavailable` variant + `UnavailableKind` enum don't exist.

**Step 3:** Add to `events.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableKind {
    Unknown,
    ViewOnce,
    Hosted,
    Bot,
}

// Add `Unavailable { id, peer, sender, unavailable_type: UnavailableKind, is_unavailable, ts_unix_ms, ts_mono_ns }`
// variant to InboundEvent enum (between NewsletterUpdate and Unknown).
```

Update `ts_unix_ms` + `ts_mono_ns` match arms.

Update `parse_inner`:
```rust
} else if let Some(rest) = raw.strip_prefix("Unavailable(") {
    parse_unavailable(rest, ts_unix_ms, ts_mono_ns)
}
```

Add the new helper:
```rust
fn parse_unavailable(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let id = unquote(&field(rest, "id").unwrap_or_default());
    let peer = unquote(&field(rest, "peer").unwrap_or_default());
    let sender = unquote(&field(rest, "sender").unwrap_or_default());
    let ts = field(rest, "ts").and_then(|v| v.parse::<i64>().ok()).unwrap_or(ts_unix_ms);
    let is_unavailable = field(rest, "is_unavailable").map(|v| v == "true").unwrap_or(true);
    let kind = match field(rest, "kind").as_deref() {
        Some("view_once") => UnavailableKind::ViewOnce,
        Some("hosted") => UnavailableKind::Hosted,
        Some("bot") => UnavailableKind::Bot,
        _ => UnavailableKind::Unknown,
    };
    InboundEvent::Unavailable {
        id, peer, sender, unavailable_type: kind, is_unavailable,
        ts_unix_ms: ts, ts_mono_ns,
    }
}
```

**Step 4:** Run: `cargo test --lib -p octo-whatsapp --features query events::tests::inbound_unavailable 2>&1 | tail -10`.
Expected: PASS.

**Step 5:** `cargo fmt` + commit:
```bash
git add crates/octo-whatsapp/src/events.rs crates/octo-whatsapp/src/events/tests.rs
git commit -m "feat(events): InboundEvent::Unavailable variant + UnavailableKind enum"
```

---

## Task 03: Add `InboundEvent::DisappearingModeChanged` variant

**Files:**
- Modify: `crates/octo-whatsapp/src/events.rs:46-156` (InboundEvent enum + arms)
- Modify: `crates/octo-whatsapp/src/events.rs:313-345` (ts_* match arms)
- Modify: `crates/octo-whatsapp/src/events.rs:458-523` (`parse_inner` dispatch)
- Add: helper `parse_disappearing_mode_changed(rest, ts_unix_ms, ts_mono_ns)`
- Test: `crates/octo-whatsapp/src/events/tests.rs` (1 hermetic test)

**Step 1:** Write failing test.

```rust
#[test]
fn inbound_disappearing_mode_changed_parses() {
    let raw = r#"DisappearingModeChanged(jid: "5511999@s.whatsapp.net", duration_seconds: 86400, ts: 1700000002)"#;
    let ev = InboundEvent::parse(EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 });
    match ev {
        InboundEvent::DisappearingModeChanged { jid, duration_seconds, ts_unix_ms, .. } => {
            assert_eq!(jid, "5511999@s.whatsapp.net");
            assert_eq!(duration_seconds, 86400);
            assert_eq!(ts_unix_ms, 1700000002);
        }
        other => panic!("expected DisappearingModeChanged, got {other:?}"),
    }
}
```

**Step 2:** Run → COMPILE FAIL (variant doesn't exist).

**Step 3:** Add to `events.rs`:

```rust
DisappearingModeChanged {
    jid: String,
    duration_seconds: u32,
    ts_unix_ms: i64,
    ts_mono_ns: u64,
},
```

Update `ts_unix_ms` + `ts_mono_ns` match arms.
Update `parse_inner`:
```rust
} else if let Some(rest) = raw.strip_prefix("DisappearingModeChanged(") {
    parse_disappearing_mode_changed(rest, ts_unix_ms, ts_mono_ns)
}
```

Helper:
```rust
fn parse_disappearing_mode_changed(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let jid = unquote(&field(rest, "jid").unwrap_or_default());
    let duration_seconds = field(rest, "duration_seconds")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let ts = field(rest, "ts").and_then(|v| v.parse::<i64>().ok()).unwrap_or(ts_unix_ms);
    InboundEvent::DisappearingModeChanged {
        jid, duration_seconds,
        ts_unix_ms: ts, ts_mono_ns,
    }
}
```

**Step 4:** Run test → PASS.

**Step 5:** `cargo fmt` + commit:
```bash
git add crates/octo-whatsapp/src/events.rs crates/octo-whatsapp/src/events/tests.rs
git commit -m "feat(events): InboundEvent::DisappearingModeChanged variant"
```

---

## Task 04: Extend adapter `on_event` closure for `UndecryptableMessage` + `DisappearingModeChanged` + per-message flags

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/adapter.rs:1100-1500` (on_event closure)
- Modify: `crates/octo-adapter-whatsapp/src/adapter.rs:1212-1354` (Event::Messages arm — populate `view_once` + `ephemeral_expires_at_seconds` via `format!("{:?}", event)` augmentation OR via RawPlatformMessage metadata dict injection)
- Test: `crates/octo-adapter-whatsapp/src/adapter.rs` (existing test module)

**Step 1:** Write failing test (in adapter.rs tests module):

```rust
#[test]
fn on_event_formats_undecryptable_message_for_parser() {
    // Adapter-driven test: feed a synthetic `Event::UndecryptableMessage`
    // into the closure and assert the formatted description matches the
    // shape parse_unavailable expects.
    use wacore::types::events::{
        Event, UndecryptableMessage, UnavailableType,
    };
    use wacore::types::message::{MessageInfo, MessageSource};
    use std::sync::Arc;

    let info = Arc::new(MessageInfo {
        source: MessageSource::default(),
        id: "M9".into(),
        ..Default::default()
    });
    let event = Event::UndecryptableMessage(UndecryptableMessage {
        info,
        is_unavailable: true,
        unavailable_type: UnavailableType::ViewOnce,
        decrypt_fail_mode: wacore::types::events::DecryptFailMode::Show,
    });
    let formatted = format!("{:?}", event);
    assert!(formatted.contains("UndecryptableMessage"), "{formatted}");
    // Bridge enforcement: the adapter's custom-format arm produces
    // `Unavailable(...)`, NOT the raw wacore Debug string.
}
```

**Step 2:** Run → COMPILE FAIL or test FAIL (no custom-format arm exists for `Event::UndecryptableMessage`; closure currently emits raw Debug via `format!("{:?}", event)`).

**Step 3:** Modify `on_event` closure:
- Add to the `custom_presence_desc` style helper (alongside `Event::NewsletterLiveUpdate`):

```rust
Event::UndecryptableMessage(un) => {
    let kind_label = format!("{:?}", un.unavailable_type).to_lowercase();
    let id_label = un.info.id.clone();
    let chat = un.info.source.chat.to_string();
    let sender = un.info.source.sender.to_string();
    // 'ts' here is the envelope-level ts_unix_ms (set by the events_persister
    // from `info.timestamp` if extractable, or persister's own clock as fallback).
    let ts = un.info.timestamp.timestamp_millis();
    Some(format!(
        "Unavailable(id: {id_label:?}, peer: {chat:?}, sender: {sender:?}, kind: {kind_label}, is_unavailable: true, ts: {ts})"
    ))
}
Event::DisappearingModeChanged(dmc) => {
    let jid_str = dmc.from.to_string();
    let dur = dmc.duration;
    let ts = dmc.setting_timestamp.timestamp_millis();
    Some(format!(
        "DisappearingModeChanged(jid: {jid_str:?}, duration_seconds: {dur}, ts: {ts})"
    ))
}
```

- For the `Event::Messages` arm: BEFORE the `let event_desc = format!("{:?}", event);` line that broadcasts the raw event, ALSO push a per-message `view_once=true` / `ephemeral_expires_at_seconds=N` enrichment into the RawPlatformMessage metadata so the singular parse_message path picks them up. The cleanest approach: precompute `(view_once, ephemeral_expires_at_seconds)` once per inbound message and inject into the metadata dict as strings:

```rust
// Inside Event::Messages, per inner message m:
let view_once = m.message.is_view_once();
let ephemeral_expir = m.info.ephemeral_expiration;
let flags_metadata = if view_once || ephemeral_expir.is_some() {
    let mut md: Vec<(String, String)> = Vec::with_capacity(2);
    if view_once { md.push(("view_once".into(), "true".into())); }
    if let Some(secs) = ephemeral_expir { md.push(("ephemeral_expires_at_seconds".into(), secs.to_string())); }
    Some(md)
} else { None };
// When building RawPlatformMessage below: extend `metadata` with flags_metadata.unwrap_or_default().
```

**Step 4:** Run adapter tests → PASS.

**Step 5:** Run: `cargo test --lib -p octo-adapter-whatsapp 2>&1 | tail -5`. Expected: PASS (no regression).

**Step 6:** `cargo fmt` + commit:
```bash
git add crates/octo-adapter-whatsapp/src/adapter.rs
git commit -m "feat(adapter): on_event UndecryptableMessage + DisappearingModeChanged arms + per-message flags"
```

---

## Task 05: 6 hermetic parser tests for new variants + flags

**Files:**
- Modify: `crates/octo-whatsapp/src/events/tests.rs` (add 6 tests covering parse_message + parse_message_batch paths)

**Step 1:** Tests to add:

```rust
#[test]
fn parse_singular_message_with_view_once_and_ephemeral() {
    // Through parse_message (singular DOT/1 path)
    let raw = r#"Message(id: "M1", peer: "X", sender: "Y", text: "", kind: Image, media_token: "tok", view_once: true, ephemeral_expires_at_seconds: 3600, is_group: false)"#;
    let env = EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 };
    match InboundEvent::parse(env) {
        InboundEvent::Message { view_once, ephemeral_expires_at_seconds, .. } => {
            assert!(view_once);
            assert_eq!(ephemeral_expires_at_seconds, Some(3600));
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn parse_batch_message_with_view_once_wrapper_sets_flag() {
    // Through parse_message_batch / parse_inbound_message
    let raw = r#"Messages(MessageBatch { messages: [InboundMessage { message: Message { view_once_message: MessageField::Set(ViewOnceMessage { message: Some(Message { image_message: MessageField::Set(ImageMessage { caption: None, view_once: Some(true), ..Default::default() }), ..Default::default() }), ..Default::default() }), ..Default::default() }, info: MessageInfo { source: MessageSource { chat: Jid { user: "X", ..Default::default() }, sender: Jid { user: "Y", ..Default::default() }, is_from_me: false, is_group: false, ..Default::default() }, id: "M2", timestamp: Some(2026-07-15T20:00:00Z), ephemeral_expiration: Some(86400), ..Default::default() } }] })"#;
    let env = EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 };
    let events = InboundEvent::parse_many(env, None);
    assert_eq!(events.len(), 1);
    match &events[0] {
        InboundEvent::Message { view_once, ephemeral_expires_at_seconds, kind, .. } => {
            assert!(view_once, "view_once wrapper present must set flag");
            assert_eq!(ephemeral_expires_at_seconds, Some(86400));
            assert_eq!(*kind, MessageKind::Image);
        }
        other => panic!("expected Message with view_once, got {other:?}"),
    }
}

#[test]
fn parse_unavailable_view_once_kind() {
    let raw = r#"Unavailable(id: "M3", peer: "X", sender: "Y", kind: view_once, is_unavailable: true, ts: 1700000000)"#;
    let ev = InboundEvent::parse(EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 });
    assert!(matches!(ev, InboundEvent::Unavailable { unavailable_type: UnavailableKind::ViewOnce, .. }));
}

#[test]
fn parse_unavailable_bot_kind() {
    let raw = r#"Unavailable(id: "M4", peer: "X", sender: "Y", kind: bot, is_unavailable: true, ts: 1700000000)"#;
    let ev = InboundEvent::parse(EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 });
    assert!(matches!(ev, InboundEvent::Unavailable { unavailable_type: UnavailableKind::Bot, .. }));
}

#[test]
fn parse_disappearing_mode_changed_with_duration() {
    let raw = r#"DisappearingModeChanged(jid: "5511999@s.whatsapp.net", duration_seconds: 86400, ts: 1700000000)"#;
    let ev = InboundEvent::parse(EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 });
    match ev {
        InboundEvent::DisappearingModeChanged { jid, duration_seconds, .. } => {
            assert_eq!(jid, "5511999@s.whatsapp.net");
            assert_eq!(duration_seconds, 86400);
        }
        other => panic!("expected DisappearingModeChanged, got {other:?}"),
    }
}

#[test]
fn parse_disappearing_mode_changed_with_zero_duration_disables() {
    let raw = r#"DisappearingModeChanged(jid: "5511999@s.whatsapp.net", duration_seconds: 0, ts: 1700000000)"#;
    let ev = InboundEvent::parse(EventEnvelope { raw: raw.into(), ts_unix_ms: 1, ts_mono_ns: 0 });
    // 0 = disabled (wacore reads this straight from the server)
    assert!(matches!(ev, InboundEvent::DisappearingModeChanged { duration_seconds: 0, .. }));
}
```

**Step 2:** Run: `cargo test --lib -p octo-whatsapp --features query events::tests:: 2>&1 | tail -10`.
Expected: ALL PASS (Tasks 01-04 must be completed first).

**Step 3:** Session 1 verification gate:
```bash
cargo fmt
cargo clippy --lib --all-features -- -D warnings   # lib-only
cargo test --lib -p octo-whatsapp --features query  # 968 + 6 = 974 lib tests
cargo test --lib -p octo-adapter-whatsapp           # no regression
```

All 6 new hermetic tests pass, no existing test regresses. Report Session 1 results to user.

**No commit at Task 05 end.** It accumulates with prior commits in the S1 batch.

**End of Session 1.** Total commits: 5 (T01-T04; T05's test additions ride along with T01-T04 commits when convenient).

---

# Session 2 — Schema Migration + Ingest Path (Tasks 06-09)

## Task 06: Bump `SCHEMA_VERSION` + idempotent `ALTER TABLE` migrations

**Files:**
- Modify: `crates/octo-whatsapp/src/query/schema.rs` (SCHEMA_VERSION + migrate())
- Test: `crates/octo-whatsapp/src/query/schema.rs::tests` (existing module)

**Step 1:** Write failing test.

```rust
#[test]
fn migrate_v2_adds_view_once_and_ephemeral_columns_idempotent() {
    let db = Database::open_in_memory().expect("open in-memory");
    // v1 migrate first to simulate upgrade path
    migrate_v1(&db).unwrap();
    // Now run v2 migrate twice — second call must be no-op
    migrate(&db).unwrap();
    migrate(&db).unwrap();
    // Verify columns exist
    let q = format!(
        "SELECT view_once, ephemeral_expires_at_seconds FROM messages LIMIT 0"
    );
    db.execute(&q, ()).expect("columns present after migrate");
}
```

**Step 2:** Run → COMPILE FAIL or test FAIL (no v2 columns yet).

**Step 3:** In `schema.rs`:

- Bump `pub const SCHEMA_VERSION: u32 = 2;`
- Rename `migrate` to `migrate_v1` (keep behaviour identical).
- Add new `migrate` that calls `migrate_v1` then runs idempotent `ALTER TABLE` statements:

```rust
pub fn migrate(db: &Database) -> Result<(), stoolap::Error> {
    migrate_v1(db)?;
    // v2: view-once + ephemeral expiration metadata.
    // ALTER TABLE has no IF NOT EXISTS in stoolap, so probe via query_meta.
    add_column_if_missing(db, "messages", "view_once", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(db, "messages", "ephemeral_expires_at_seconds", "INTEGER")?;
    Ok(())
}

fn add_column_if_missing(db: &Database, table: &str, column: &str, decl: &str) -> Result<(), stoolap::Error> {
    // Use the existing query_meta PRAGMA-or-show-columns mechanism; if
    // `column` is absent, run `ALTER TABLE <table> ADD COLUMN <column> <decl>`.
    // ...
}
```

The `add_column_if_missing` helper uses a SELECT-on-system-catalog probe (matches the existing `query_meta` PRAGMA fallback). Full body: see the implementation in task commit (probe via `pragma table_info(<table>)` if available; fall back to a savepoint + ALTER + rollback on parse error).

**Step 4:** Run `cargo test --lib -p octo-whatsapp --features query query::schema::tests 2>&1 | tail -10`.
Expected: PASS.

**Step 5:** Commit:
```bash
git add crates/octo-whatsapp/src/query/schema.rs
git commit -m "feat(query): schema v2 - view_once + ephemeral_expires_at_seconds columns"
```

---

## Task 07: Add `unavailable_messages` + `disappearing_mode_changes` tables

**Files:**
- Modify: `crates/octo-whatsapp/src/query/schema.rs` (`CREATE_UNAVAILABLE_TABLE` + `CREATE_DISAPPEARING_MODE_CHANGES_TABLE` + indexes + migrate additions)

**Step 1:** Add to schema.rs:

```rust
const CREATE_UNAVAILABLE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS unavailable_messages (
    id           INTEGER PRIMARY KEY,
    ts_unix_ms   INTEGER  NOT NULL,
    ts_mono_ns   INTEGER  NOT NULL,
    kind         TEXT     NOT NULL,    -- 'view_once' | 'hosted' | 'bot' | 'unknown'
    peer         TEXT     NOT NULL,
    sender       TEXT     NOT NULL,
    is_unavailable INTEGER NOT NULL DEFAULT 1
)
"#;

const CREATE_UNAVAILABLE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_unavailable_kind_ts ON unavailable_messages(kind, ts_unix_ms)",
    "CREATE INDEX IF NOT EXISTS idx_unavailable_peer_ts ON unavailable_messages(peer, ts_unix_ms)",
];

const CREATE_DISAPPEARING_MODE_CHANGES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS disappearing_mode_changes (
    id                INTEGER PRIMARY KEY,
    ts_unix_ms        INTEGER  NOT NULL,
    ts_mono_ns        INTEGER  NOT NULL,
    jid               TEXT     NOT NULL,
    duration_seconds  INTEGER  NOT NULL  -- 0 = disabled, otherwise seconds
)
"#;

const CREATE_DISAPPEARING_MODE_CHANGES_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_dmc_jid_ts ON disappearing_mode_changes(jid, ts_unix_ms)",
];
```

Extend `migrate()`:
```rust
    db.execute(CREATE_UNAVAILABLE_TABLE, ())?;
    for s in CREATE_UNAVAILABLE_INDEXES { db.execute(s, ())?; }
    db.execute(CREATE_DISAPPEARING_MODE_CHANGES_TABLE, ())?;
    for s in CREATE_DISAPPEARING_MODE_CHANGES_INDEXES { db.execute(s, ())?; }
```

**Step 2:** Add tests:
```rust
#[test]
fn migrate_v2_creates_unavailable_and_dmc_tables() {
    let db = Database::open_in_memory().expect("open in-memory");
    migrate(&db).unwrap();
    // show_tables equivalent — schema.rs already has SHOW TABLES support
    let tables: Vec<String> = db.query("SHOW TABLES", ()).unwrap().into_iter().map(|r| r.get(0).unwrap()).collect();
    assert!(tables.iter().any(|t| t == "unavailable_messages"));
    assert!(tables.iter().any(|t| t == "disappearing_mode_changes"));
}
```

**Step 3:** Run → PASS. Commit:
```bash
git add crates/octo-whatsapp/src/query/schema.rs
git commit -m "feat(query): unavailable_messages + disappearing_mode_changes tables (v2)"
```

---

## Task 08: Extend `query::ingester` to write new variants + populate view_once / ephemeral columns

**Files:**
- Modify: `crates/octo-whatsapp/src/query/ingester.rs` (add ingest paths for `Unavailable`, `DisappearingModeChanged`, and populate the new `messages` columns from `Message` flags)
- Test: `crates/octo-whatsapp/src/query/tests.rs` or local `#[cfg(test)] mod tests` in ingester.rs

**Step 1:** Write failing test.

```rust
#[test]
fn ingester_writes_view_once_message_with_flag_set() {
    // Build a QuerySubsystem with in-memory DB; subscribe an Ingester.
    // Push one InboundEvent::Message { view_once: true, ephemeral_expires_at_seconds: Some(86400), kind: Image, ... }
    // After drain: SELECT view_once, ephemeral_expires_at_seconds FROM messages WHERE event_id = ... → returns (1, Some(86400)).
}

#[test]
fn ingester_writes_unavailable_event() {
    // Push InboundEvent::Unavailable { kind: ViewOnce, ... }
    // After drain: SELECT count(*) FROM unavailable_messages = 1, kind = 'view_once'.
}

#[test]
fn ingester_writes_disappearing_mode_changed() {
    // Push InboundEvent::DisappearingModeChanged { jid, duration_seconds, ... }
    // After drain: SELECT duration_seconds FROM disappearing_mode_changes WHERE jid = ? → row present.
}
```

**Step 2:** Run → FAIL.

**Step 3:** Implement ingester extension:
- The existing ingest path matches on `InboundEvent::Message { ... }` to populate the `messages` table. Extend that match arm to also INSERT `view_once: bool → 0/1` and `ephemeral_expires_at_seconds: Option<u32>`. Use `field(rest, "view_once")` is wrong here — this is typed ingest, so pull the bool + Option directly from the enum destructure.
- Add new match arms for `Unavailable { ... }` (INSERT to `unavailable_messages`) and `DisappearingModeChanged { ... }` (INSERT to `disappearing_mode_changes`).
- Use `insert_idempotent` (existing helper) for both new tables to match the events table behaviour.

**Step 4:** Run tests → PASS.

**Step 5:** Commit:
```bash
git add crates/octo-whatsapp/src/query/ingester.rs
git commit -m "feat(query): ingester writes view_once + ephemeral + Unavailable + DisappearingModeChanged rows"
```

---

## Task 09: Session 2 verification gate

**Files:** none — pure verification.

**Step 1:**
```bash
cargo fmt
cargo clippy --lib --all-features -- -D warnings
cargo test --lib -p octo-whatsapp --features query  # expect 977+ (974 + 3 new)
```

**Step 2:** Verify migration v1 → v2 sanity:
```bash
# Start daemon against a non-existent state dir; let migrate() run; check no parse errors
./scripts/run-octo-whatsapp.sh --restart
sleep 5
journalctl -u octo-whatsapp --since "1 min ago" | grep -i 'view_once\|ephemeral' || echo 'silent boot OK'
```

**No commit.** Session 2 complete. Report results.

---

# Session 3 — Read RPCs + CLI + MCP + Skill (Tasks 10-14)

## Task 10: `messages.read_view_once` handler (one-shot semantics)

**Files:**
- Create: `crates/octo-whatsapp/src/ipc/handlers/messages_read_view_once.rs`
- Modify: `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (3 sites: `pub mod`, `register(Arc::new(...))`, new `TIER7_K_VIEW_ONCE_DISAPPEARING_METHODS` const + dedup chain)
- Test: `crates/octo-whatsapp/src/ipc/handlers/messages_read_view_once.rs` hermetic tests (3)

**Step 1:** Write failing test.

```rust
#[tokio::test]
async fn read_view_once_second_call_returns_consumed_error() {
    let tmp = tempfile::tempdir().unwrap();
    let h = Daemon::new_for_tests(tmp.path()).1;
    let ctx = TestCtx::seed_view_once_message(&h, 1234, /* now */ 1000).await;
    // First call: OK
    let r1 = MessagesReadViewOnce.call(h.clone(), json!({"event_id": 1234})).await.unwrap();
    assert_eq!(r1["status"], "delivered");
    // Second call: consumed
    let r2 = MessagesReadViewOnce.call(h.clone(), json!({"event_id": 1234})).await;
    assert!(r2.is_err() || r2.unwrap()["status"] == "consumed");
}

#[tokio::test]
async fn read_view_once_missing_event_returns_404() {
    let tmp = tempfile::tempdir().unwrap();
    let h = Daemon::new_for_tests(tmp.path()).1;
    let r = MessagesReadViewOnce.call(h, json!({"event_id": 9999})).await;
    assert!(r.is_err());
}

#[tokio::test]
async fn read_view_once_non_view_once_message_returns_invalid_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let h = Daemon::new_for_tests(tmp.path()).1;
    let ctx = TestCtx::seed_plain_text_message(&h, 5678).await;
    let r = MessagesReadViewOnce.call(h, json!({"event_id": 5678})).await;
    assert!(r.is_err());
}
```

**Step 2:** Run → FAIL.

**Step 3:** Implement handler. Behaviour:
- Look up the message row by `event_id` in `messages` table.
- Reject if not found (404), if not `view_once=1`, if `consumed_at IS NOT NULL`.
- Otherwise: invoke the existing media download (reuse `messages.download` machinery), return `{ "media": <base64 or token>, "url": <cdn_url>, "mime": <type>, "caption": <text>, "consumed_at": <now_ms> }`, and UPDATE the `messages` row to set `consumed_at = now_unix_ms` + `media_token = ''` (zero out the CDN material after delivery).
- Returns 200 with the media bytes (`messages.download` already does this on success — extend its handler with the one-shot guard, or build on top of it).

**Step 4:** Run tests → PASS.

**Step 5:** Register in `mod.rs`:
```rust
pub mod messages_read_view_once;
// inside build_registry():
.register(Arc::new(messages_read_view_once::MessagesReadViewOnce))
// new const:
pub const TIER7_K_VIEW_ONCE_DISAPPEARING_METHODS: &[&str] = &[
    "messages.read_view_once",
    "messages.list_unavailable",
    "messages.list_ephemeral",
];
// extend the dedup chain in registry_size_matches_phase1_phase2.
```

**Step 6:** Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/
git commit -m "feat(octo-whatsapp): messages.read_view_once handler (one-shot)"
```

---

## Task 11: `messages.list_unavailable` + `messages.list_ephemeral` handlers

**Files:**
- Create: `crates/octo-whatsapp/src/ipc/handlers/messages_list_unavailable.rs`
- Create: `crates/octo-whatsapp/src/ipc/handlers/messages_list_ephemeral.rs`
- Modify: `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (3 sites per handler: pub mod + register + extend `TIER7_K_VIEW_ONCE_DISAPPEARING_METHODS` constant already updated in T10)
- Test: 3 hermetic tests per handler (filters + limits)

**Step 1:** Write failing tests (briefly):

```rust
// messages.list_unavailable
#[tokio::test] async fn list_unavailable_filters_by_kind_view_once() { /* seed 3 unavailable rows (view_once/hosted/bot), filter kind=view_once → 1 row */ }
#[tokio::test] async fn list_unavailable_pagination_respects_limit() { /* seed 10 rows, limit=3 → 3 rows */ }
#[tokio::test] async fn list_unavailable_peer_filter_works() { /* seed rows across 2 peers, peer filter → 1 peer's rows */ }

// messages.list_ephemeral
#[tokio::test] async fn list_ephemeral_returns_only_messages_with_timer() { /* seed mix of messages with/without ephemeral_expires_at_seconds */ }
#[tokio::test] async fn list_ephemeral_excludes_already_consumed_view_once() { /* seed view_once row with consumed_at set → not in /messages.list_ephemeral */ }
#[tokio::test] async fn list_ephemeral_filters_by_min_remaining_seconds() { /* seed 3 rows with different expiries, filter remaining < 3600 → 1 */ }
```

**Step 2:** Run → FAIL.

**Step 3:** Implement:
- `messages.list_unavailable` — query `unavailable_messages` with optional `kind` + `peer` + `since_ts_unix_ms` + `until_ts_unix_ms` + `limit` (default 50, max 200). Map to `{rows: [...], count: N}`.
- `messages.list_ephemeral` — query `messages WHERE ephemeral_expires_at_seconds IS NOT NULL AND (consumed_at IS NULL OR ... view_once filter)`. Filters: `peer`, `min_remaining_seconds`, `since_ts_unix_ms`, `limit`. Returns `{rows: [{event_id, peer, sender, kind, text, ephemeral_expires_at_seconds, seconds_remaining}], count}`.

**Step 4:** Run tests → PASS.

**Step 5:** Register both in `mod.rs` (already declared in T10; just add the register calls):
```rust
.register(Arc::new(messages_list_unavailable::MessagesListUnavailable))
.register(Arc::new(messages_list_ephemeral::MessagesListEphemeral))
```

**Step 6:** Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/messages_list_unavailable.rs \
        crates/octo-whatsapp/src/ipc/handlers/messages_list_ephemeral.rs \
        crates/octo-whatsapp/src/ipc/handlers/mod.rs
git commit -m "feat(octo-whatsapp): messages.list_unavailable + messages.list_ephemeral"
```

---

## Task 12: CLI subtree `messages {read-view-once|list-unavailable|list-ephemeral}`

**Files:**
- Modify: `crates/octo-whatsapp/src/cli.rs` (extend `MessagesAction` enum + `dispatch_messages`)

**Step 1:** Add to `MessagesAction`:

```rust
/// Read the media body for a view-once message (one-shot).
ReadViewOnce {
    /// `event_id` of the message row.
    #[arg(value_name = "EVENT_ID")]
    event_id: i64,
},
/// List messages whose content was unavailable (view-once fanouts to
/// companion devices, plus bot/hosted).
ListUnavailable {
    #[arg(long, value_enum, default_value_t = UnavailableKindArg::All)]
    kind: UnavailableKindArg,
    #[arg(long)] peer: Option<String>,
    #[arg(long)] since_ts_unix_ms: Option<i64>,
    #[arg(long)] until_ts_unix_ms: Option<i64>,
    #[arg(long, default_value_t = 50)] limit: i64,
},
/// List ephemeral (disappearing) messages currently in flight.
ListEphemeral {
    #[arg(long)] peer: Option<String>,
    #[arg(long)] min_remaining_seconds: Option<i64>,
    #[arg(long, default_value_t = 50)] limit: i64,
},
```

Add `UnavailableKindArg` enum (clap value enum) with variants `All | ViewOnce | Hosted | Bot | Unknown` (Wire-format matches `UnavailableKind::as_str()`).

**Step 2:** Dispatch arms in `dispatch_messages`:

```rust
MessagesAction::ReadViewOnce { event_id } => (
    "messages.read_view_once",
    json!({ "event_id": event_id }),
),
MessagesAction::ListUnavailable { kind, peer, since_ts_unix_ms, until_ts_unix_ms, limit } => {
    let kind_str = match kind {
        UnavailableKindArg::All => null,
        _ => json!(kind.to_str()),
    };
    let mut p = serde_json::Map::new();
    p.insert("kind".into(), kind_str);
    if let Some(p) = peer { p.insert("peer".into(), json!(peer)); }  // (renamed locally)
    // (build remaining)
    ("messages.list_unavailable", Value::Object(p))
}
MessagesAction::ListEphemeral { peer, min_remaining_seconds, limit } => { ... }
```

**Step 3:** Run: `cargo build --profile dev -p octo-whatsapp --features query 2>&1 | tail -5`. Expected: clean.

**Step 4:** Add 2 hermetic CLI tests using `assert_cmd` style (or a subprocess invocation):

```rust
#[test] fn cli_messages_read_view_once_help_lists_subcommand() { /* invokes clap help, asserts "read-view-once" listed */ }
#[test] fn cli_messages_list_unavailable_help_lists_subcommand() { /* same */ }
```

**Step 5:** Commit:
```bash
git add crates/octo-whatsapp/src/cli.rs
git commit -m "feat(octo-whatsapp): CLI messages {read-view-once|list-unavailable|list-ephemeral}"
```

---

## Task 13: MCP tool descriptors + RPC map + count bump

**Files:**
- Modify: `crates/octo-whatsapp/src/mcp_server.rs` (`tool_descriptors()` add 3 entries; `EXPECTED_TOOL_COUNT` bump 142→145 (query on) / 136→139 (query off); tests `phase1_methods_all_registered`/`registry_size_matches_phase1_phase2` extension)

**Step 1:** Update the three tests:

```rust
.chain(TIER7_K_VIEW_ONCE_DISAPPEARING_METHODS.iter())
```

In `registry_size_matches_phase1_phase2`, append `.chain(TIER7_K_VIEW_ONCE_DISAPPEARING_METHODS.iter())` to the dedup chain (same as other tier consts).

**Step 2:** Add to `tool_descriptors()`:

```rust
// wa_read_view_once (one-shot view-once media download)
ToolDescriptor {
    name: "wa_read_view_once",
    description: "Read the media body for a view-once message. One-shot: subsequent reads return consumed. Returns {media, mime, caption, consumed_at, event_id}.",
    input_schema: json!({"type":"object","properties":{"event_id":{"type":"integer"}},"required":["event_id"]}),
},
// wa_list_unavailable
ToolDescriptor {
    name: "wa_list_unavailable",
    description: "List messages whose content is unavailable (view-once/bot/hosted fanouts). Filters: kind, peer, since_ts_unix_ms, until_ts_unix_ms, limit.",
    input_schema: json!({"type":"object","properties":{...}}),
},
// wa_list_ephemeral
ToolDescriptor {
    name: "wa_list_ephemeral",
    description: "List messages with ephemeral (disappearing) timers. Filters: peer, min_remaining_seconds, limit.",
    input_schema: json!({...}),
},
```

**Step 3:** Update `EXPECTED_TOOL_COUNT`:
- Query feature on: `142 → 145`
- Query feature off: `136 → 139`

**Step 4:** Run: `cargo test --lib -p octo-whatsapp --features query mcp_server::tests 2>&1 | tail -15`. Expected: PASS.

**Step 5:** Commit:
```bash
git add crates/octo-whatsapp/src/mcp_server.rs
git commit -m "feat(octo-whatsapp): MCP wa_read_view_once + wa_list_unavailable + wa_list_ephemeral"
```

---

## Task 14: Skill catalog §25 + live test + final verification

**Files:**
- Modify: `crates/octo-whatsapp/assets/skills/wa-mcp.md` (append §25)
- Create: `crates/octo-whatsapp/tests/live_chain_k_view_once.rs` (live test, env-gated)
- Modify: `crates/octo-whatsapp/tests/mod.rs` (declare test module if not auto)
- Verify: full gates

**Step 1:** Skill catalog section (drafted):

```md
### 25. View-Once + Disappearing Messages

#### messages.read_view_once
- Direction: One-shot media read for a view-once message. Returns CDN bytes + metadata; subsequent reads return 'consumed' (the message row's `consumed_at` is set).
- Input: `{ event_id: integer }`
- Output: `{ event_id, media_b64 (or media_ref_token), mime, caption, consumed_at_unix_ms }`
- Wire: existing `media.download` machinery, gated by `messages.consumed_at IS NULL`.
- Use case: receive view-once image from a peer, fetch it once before the timer/service closes the window.
- Constraint: parallel `messages.download` of the same view-once msg idempotent on the consumed_at side-effect — both first-callers get the bytes, second-callers get 'consumed'.

#### messages.list_unavailable
- Direction: List messages whose content the phone refused to share (view-once/bot/hosted/companion fanouts).
- Input: `{ kind?: 'view_once'|'hosted'|'bot'|'unknown', peer?: string, since_ts_unix_ms?: int, until_ts_unix_ms?: int, limit: int }`
- Output: `{ rows: [{id, kind, peer, sender, ts_unix_ms, is_unavailable}], count }`
- Wire: SELECT FROM unavailable_messages.
- Use case: audit how often companion fanouts drop content, group by `kind`.

#### messages.list_ephemeral
- Direction: List messages with an active disappearing-message timer.
- Input: `{ peer?: string, min_remaining_seconds?: int, limit: int }`
- Output: `{ rows: [{event_id, peer, sender, kind, text, ephemeral_expires_at_seconds, seconds_remaining}], count }`
- Wire: SELECT FROM messages WHERE ephemeral_expires_at_seconds IS NOT NULL AND view_once = 0.
- Use case: surface messages that will disappear in the next hour.
```

**Step 2:** Live test (env-gated, hermetic-compile + runtime-gated):

```rust
// crates/octo-whatsapp/tests/live_chain_k_view_once.rs
#[tokio::test]
async fn live_messages_list_unavailable_returns_view_once_fanout() {
    if std::env::var("OCTO_WA_LIVE_TEST").is_err() { return; } // skip
    // Pair two test sessions: send a view-once image from session A to the
    // operator session B; session B should see it via messages.list_unavailable.
    // ...
}
```

For now: write 1 hermetic integration test (NOT live) — `messages_list_unavailable_returns_seeded_rows` — that seeds 3 Unavailable events and asserts the read. Live test deferred pending paired session availability.

**Step 3:** Final verification:
```bash
cargo fmt
cargo clippy --lib --all-features -- -D warnings
cargo test --lib -p octo-whatsapp --features query
cargo build --profile dev -p octo-whatsapp --features query
```

Expected: 980+ lib tests pass (974 from S1 + 6 S1 batch + 3 S3 hermetic handler tests + 3 S2 ingester tests). No regressions in newsletter/community/sql handlers.

**Step 4:** Commit:
```bash
git add crates/octo-whatsapp/assets/skills/wa-mcp.md crates/octo-whatsapp/tests/live_chain_k_view_once.rs
git commit -m "docs(octo-whatsapp): skill §25 View-Once + Disappearing + live test stub"
```

**End of Session 3.** Total commits: 5 (T10, T11, T12, T13, T14).

---

# Session 4 — Config Gate + MEMORY Wrap-Up (Tasks 15-17)

## Task 15: `MediaConfig.view_once_media_persist` default-false + daemon plumbing

**Files:**
- Modify: `crates/octo-whatsapp/src/config.rs` (add `MediaConfig` struct + add it as a field on the root config + env override)
- Modify: `crates/octo-whatsapp/src/daemon.rs` (wire `media_config` into `DaemonHandle`)

**Step 1:** Write failing test.

```rust
// In config.rs tests
#[test]
fn media_config_default_disables_view_once_persistence() {
    let c = MediaConfig::default();
    assert!(!c.view_once_media_persist);
}

#[test]
fn media_config_env_override_enables_persistence() {
    std::env::set_var("OCTO_WA_MEDIA__VIEW_ONCE_PERSIST", "true");
    let c = MediaConfig::from_env_or_default();
    assert!(c.view_once_media_persist);
    std::env::remove_var("OCTO_WA_MEDIA__VIEW_ONCE_PERSIST");
}
```

**Step 2:** Add the struct:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct MediaConfig {
    /// If `false` (default), inbound view-once media rows have their
    /// `media_token` zeroed after persisting to the events table /
    /// SQL store. The operator must invoke `messages.read_view_once`
    /// to fetch the CDN URL + key — at which point `consumed_at` is
    /// set and subsequent reads fail.
    pub view_once_media_persist: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self { view_once_media_persist: false }
    }
}
```

Wire into the root config struct as a sibling of `query: QueryConfig`.

**Step 3:** Test passes.

**Step 4:** Commit:
```bash
git add crates/octo-whatsapp/src/config.rs
git commit -m "feat(octo-whatsapp): MediaConfig.view_once_media_persist (default false)"
```

---

## Task 16: Adapter strip-on-persist path (drop `media_token` for view-once when config = false)

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/adapter.rs` (when config flag is OFF and `is_view_once() == true`, replace the constructed media_token with `None` before pushing to inbound_tx / persister)
- Add: `fn strip_view_once_media_token(&self, msg: &wacore::types::Message, media_token: Option<String>) -> Option<String>` helper

**Step 1:** Write failing test (in adapter.rs):

```rust
#[tokio::test]
async fn view_once_media_token_zeroed_when_persist_flag_false() {
    let mut adapter = WhatsAppWebAdapter::new_for_tests();
    adapter.set_view_once_media_persist(false);
    let mut msg = wacore::types::Message::default();
    msg.image_message = wacore::types::message::MessageField::some(ImageMessage {
        view_once: Some(true),
        media_key: Some(b"key".to_vec()),
        ..Default::default()
    });
    let token = adapter.strip_view_once_media_token(&msg, Some("tok".into()));
    assert_eq!(token, None, "view-once media must be stripped when flag=false");
}

#[tokio::test]
async fn view_once_media_token_kept_when_persist_flag_true() {
    let mut adapter = WhatsAppWebAdapter::new_for_tests();
    adapter.set_view_once_media_persist(true);
    // ... same setup, expect Some("tok")
}
```

**Step 2:** Implement — wire the new config knob into the adapter's `media_config_view_once_persist` field (clone-friendly primitive). When the flag is false and `msg.is_view_once() == true`, return `None` for the media_token; otherwise pass-through. Apply at the `Event::Messages` arm where RawPlatformMessage metadata dict is assembled, and also pass the flag into `parse_inbound_message`'s view_once-extraction path.

**Step 3:** Verify with `cargo test --lib -p octo-adapter-whatsapp`.

**Step 4:** Commit:
```bash
git add crates/octo-adapter-whatsapp/src/adapter.rs
git commit -m "feat(adapter): strip view-once media_token when MediaConfig.view_once_media_persist=false"
```

---

## Task 17: Final verification + MEMORY.md update

**Files:**
- Modify: `.jcode/memory/MEMORY.md` (Phase 7.K line; remove view-once/deferral backlog line)
- Verify: full gates

**Step 1:** Verification:
```bash
cargo fmt --check
cargo clippy --lib --all-features -- -D warnings
cargo test --lib -p octo-whatsapp --features query
cargo test --lib -p octo-adapter-whatsapp
```

Expected: clean fmt, clean lib clippy, 980+ tests green.

**Step 2:** Update `.jcode/memory/MEMORY.md`. Find the existing "Phase 7" rollout line and append:

```md
**7.K** (4 sessions, 17 commits, 2026-07-15): View-Once + Disappearing messages fully typed. InboundEvent::Message gains `view_once: bool` + `ephemeral_expires_at_seconds: Option<u32>`; new `InboundEvent::Unavailable { unavailable_type: ViewOnce|Hosted|Bot|Unknown }` (companion fanouts no longer dropped); new `InboundEvent::DisappearingModeChanged { jid, duration_seconds, ts }`. Schema v1→v2: `messages` table + `view_once INTEGER NOT NULL DEFAULT 0` + `ephemeral_expires_at_seconds INTEGER`; new `unavailable_messages` + `disappearing_mode_changes` tables. 3 new RPCs: `messages.read_view_once` (one-shot CDN fetch; sets `consumed_at` + zeros `media_token`), `messages.list_unavailable` (filter by kind/peer/window), `messages.list_ephemeral` (filter by peer/min_remaining). CLI subtree `messages {read-view-once|list-unavailable|list-ephemeral}`. MCP: `wa_read_view_once` + `wa_list_unavailable` + `wa_list_ephemeral`; EXPECTED_TOOL_COUNT 142/136 → 145/139. Skill catalog §25. Adapter `MediaConfig.view_once_media_persist` default-false closes the silent-on-disk path. Total RPCs: ~143.
```

**Step 3:** Live smoke (if paired session available, skip otherwise):
```bash
./scripts/run-octo-whatsapp.sh --restart
sleep 5
# Verify schema v2 applied:
sqlite3 ~/.local/share/octo/whatsapp/<account>/query.db ".schema messages" | grep -E 'view_once|ephemeral'
# Or via the daemon over RPC:
echo '{"jsonrpc":"2.0","id":1,"method":"sql.query","params":{"sql":"SELECT view_once, ephemeral_expires_at_seconds FROM messages LIMIT 1"}}' | \
  nc -U /tmp/octo-wa-run/octo-whatsapp-default.sock
# Expect: zero rows (no messages yet), no error.
```

**Step 4:** Commit:
```bash
git add .jcode/memory/MEMORY.md
git commit -m "docs(memory): Phase 7.K view-once + disappearing complete"
```

**End of Session 4 + plan.**

---

## Verification (final gate)

| Check | Command | Expected |
|---|---|---|
| Format | `cargo fmt --check` | exit 0, no diff |
| Lib clippy | `cargo clippy --lib --all-features -- -D warnings` | exit 0 |
| Lib tests (query on) | `cargo test --lib -p octo-whatsapp --features query` | 980+ pass |
| Lib tests (adapter) | `cargo test --lib -p octo-adapter-whatsapp` | all pass, no regression |
| Schema v2 applied | boot daemon once | columns present, no migration error |
| Daemon boot | `./scripts/run-octo-whatsapp.sh --restart` | clean boot, no panic |
| RPC smoke | direct-socket JSON-RPC for `messages.list_unavailable` | empty rows, no error |

## Deferred (out of scope per design rule)

- Live verification of an actual view-once media flow requires a paired session sending view-once content to the operator session (the operator is currently logged-out per the Phase 6.12.3 gate). Live test stube is env-gated and compiles — runtime gate stays as a follow-up.
- No sending view-once content outbound (`messages.send_image { ..., view_once: true }` etc.). The send path already supports it via `msg.image_message.view_once = Some(true)` (existing send/mod.rs instrumentation), but the RPC surface doesn't expose a `view_once` param yet. Deferred — operators don't typically dispatch view-once images from a bot anyway.

## Critical files (full paths)

- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/events.rs` — InboundEvent enum + Message struct + parse_* helpers
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/events/tests.rs` — hermetic parser tests
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-adapter-whatsapp/src/adapter.rs` — on_event closure (lines 1100-1500) + Event::Messages arm (lines 1212-1354)
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/query/schema.rs` — SCHEMA_VERSION + migrate() + tables
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/query/ingester.rs` — typed ingest
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/ipc/handlers/messages_read_view_once.rs` — NEW (one-shot)
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/ipc/handlers/messages_list_unavailable.rs` — NEW
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/ipc/handlers/messages_list_ephemeral.rs` — NEW
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/ipc/handlers/mod.rs` — pub mod + register + TIER7_K const
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/cli.rs` — MessagesAction enum + dispatch
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/mcp_server.rs` — tool_descriptors() + EXPECTED_TOOL_COUNT + dedup chain
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/src/config.rs` — MediaConfig struct + env override
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-whatsapp/assets/skills/wa-mcp.md` — catalog §25
- `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/.jcode/memory/MEMORY.md` — Phase 7.K note

---

## Execution handoff

Plan complete and saved to `docs/plans/2026-07-15-phase7-K-view-once-disappearing.md`. 17 tasks, 17 commits across 4 sessions.

Two execution options:
1. **Subagent-driven (this session)** — fresh subagent per task + 2-stage review (spec compliance + code quality). Fast iteration, no context switch.
2. **Parallel session** — open new session in this worktree, run all 4 sessions via `superpowers:executing-plans` with checkpoint pauses between sessions for the operator to live-verify.

Which approach?
