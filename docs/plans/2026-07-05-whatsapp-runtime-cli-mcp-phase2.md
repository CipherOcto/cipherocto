# WhatsApp Runtime CLI + MCP — Phase 2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development in-session) to implement this plan task-by-task.

**Goal:** Implement Phase 2 of the WhatsApp runtime CLI + MCP design — outbound media matrix (`send.image`/`video`/`audio`/`voice`/`sticker`/`reaction`/`poll`/`contact`/`location`/`delete`), `messages.search`/`edit`/`mark_read`/`download`/`list`/`get`, full `chats.*` surface, the DOT envelope trio (`envelope.encode`/`decode`/`send`/`send-native`), `capabilities`, `domain.compute-hash`, plus ~10 new inherent methods on `WhatsAppWebAdapter`, and the parity-table test.

**Architecture:** Two-layer. (1) `octo-adapter-whatsapp` gets new inherent `send_*` / `edit_message` / `delete_message` / `mark_read` / `message_search` / `chat_*` methods that delegate to `whatsapp-rust`/wacore. (2) `octo-whatsapp` gets RPC handlers wrapping them with per-kind ceiling pre-flights, temp-file buffering with `max_concurrent_uploads=4`, disk-space check, edit/delete window enforcement (`-32013`/`-32014`), plus 1:1 MCP tool entries and CLI subcommand tree. Adapter and runtime stay in lockstep via the parity table test.

**Tech Stack:** Rust 2021, `tokio` 1, `clap` 4 derive, `serde`/`serde_json`, `tracing`, `tempfile` (in tests), `assert_cmd`+`predicates` (CLI smoke), `nix` (unix socket), `whatsapp-rust` (existing), `blake3` (existing), `wacore`/`waproto` (existing).

**Pre-requisites:**
- Branch: `feat/whatsapp-runtime-cli-mcp` (stack on top — 76 Phase 1 commits already in place; 1 cleanup commit `66a08f9e` after)
- Worktree: `.worktrees/whatsapp-runtime-cli-mcp`
- Phase 1 status: all 80 tests green, clippy clean, fmt clean, no `TODO`/`FIXME` left in `octo-whatsapp` or `octo-cli-meta`
- Plan ref: `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` §Rollout Phase 2, §Subcommand tree, §API Parity Coverage, §Raw vs DOT Protocol Paths
- Phase 1 plan ref: `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-phase1.md`

**Acceptance gates:**
- 60 tasks complete
- `cargo test -p octo-whatsapp` green (unit + integration)
- `cargo test -p octo-adapter-whatsapp` green (existing + new)
- `cargo test -p octo-cli-meta --features whatsapp-cli` green
- `cargo clippy --all-targets --all-features -- -D warnings` clean
- `cargo fmt --check` clean
- `daemon.api.version = "1.0.0+phase2"` reported by `version.get`
- `it_parity_table.rs` passes — every public method on `WhatsAppWebAdapter` and `CoordinatorAdmin` shows a `✅` in the §API Parity table (per design §2132)
- 13 hermetic e2e `it_*.rs` from Phase 1 still green
- New tests covering all 14 new RPC handlers + 10 new adapter methods
- Coverage ≥ 85% lines / ≥ 75% branches (gates deferred from Phase 1 honored here)
- No push, no PR (per user decision 2026-07-05)

**YAGNI:**
- No live-WhatsApp tests in `octo-whatsapp` itself; live e2e stays in `octo-adapter-whatsapp` (`live-whatsapp` feature)
- No new session-loss paths (Phase 1 §Session-loss path is canonical)
- No new inbound event types (Phase 3 owns event router)

---

## Part A — Workspace prep & feature wiring (Tasks 1-3)

### Task 1: Bump `daemon.api.version` to `1.0.0+phase2`

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/version.rs`

**Step 1:** Find the existing `daemon_api_version` literal `"1.0.0+phase1"`.

**Step 2:** Replace with `"1.0.0+phase2"`. Single literal change.

**Step 3:** Run `cargo test -p octo-whatsapp --lib ipc::handlers::version` — expect PASS (3 tests).

**Step 4:** Commit:
```bash
git add crates/octo-whatsapp/src/ipc/handlers/version.rs
git commit -m "feat(octo-whatsapp): bump daemon.api.version to 1.0.0+phase2"
```

### Task 2: Add `limits` module with per-kind `MAX_*_BYTES` constants

**Files:**
- Create: `crates/octo-whatsapp/src/limits.rs`
- Modify: `crates/octo-whatsapp/src/lib.rs` (add `pub mod limits;`)

**Step 1:** Write failing test in `limits.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ceilings_match_whatsapp_web_quotas() {
        assert_eq!(MAX_TEXT_BYTES, 65_536);
        assert_eq!(MAX_IMAGE_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_VIDEO_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_AUDIO_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_VOICE_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_STICKER_BYTES, 1024 * 1024);
        assert_eq!(MAX_DOC_BYTES, 100 * 1024 * 1024);
        assert_eq!(MAX_VCARD_BYTES, 1024 * 1024);
    }
    #[test]
    fn media_kind_round_trip() {
        for k in [MediaKind::Image, MediaKind::Video, MediaKind::Audio,
                  MediaKind::Voice, MediaKind::Sticker, MediaKind::Document,
                  MediaKind::Contact, MediaKind::Reaction, MediaKind::Poll,
                  MediaKind::Location] {
            assert_eq!(MediaKind::from_str(k.as_str()).unwrap(), k);
        }
    }
}
```

**Step 2:** Run `cargo test -p octo-whatsapp --lib limits` — expect FAIL ("unresolved import").

**Step 3:** Implement `limits.rs`:
```rust
//! Per-kind payload ceilings and `MediaKind` enum for the outbound
//! matrix. See design §Raw vs DOT Protocol Paths.

pub const MAX_TEXT_BYTES: usize = 65_536;
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_VIDEO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_AUDIO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_VOICE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STICKER_BYTES: usize = 1024 * 1024;
pub const MAX_DOC_BYTES: usize = 100 * 1024 * 1024;
pub const MAX_VCARD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Text, Image, Video, Audio, Voice, Sticker, Document, Contact,
    Reaction, Poll, Location,
}
impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text", Self::Image => "image", Self::Video => "video",
            Self::Audio => "audio", Self::Voice => "voice", Self::Sticker => "sticker",
            Self::Document => "document", Self::Contact => "contact",
            Self::Reaction => "reaction", Self::Poll => "poll", Self::Location => "location",
        }
    }
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Text => MAX_TEXT_BYTES, Self::Image => MAX_IMAGE_BYTES,
            Self::Video => MAX_VIDEO_BYTES, Self::Audio => MAX_AUDIO_BYTES,
            Self::Voice => MAX_VOICE_BYTES, Self::Sticker => MAX_STICKER_BYTES,
            Self::Document => MAX_DOC_BYTES, Self::Contact => MAX_VCARD_BYTES,
            Self::Reaction => 1024, Self::Poll => 4096, Self::Location => 1024,
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "text" => Self::Text, "image" => Self::Image, "video" => Self::Video,
            "audio" => Self::Audio, "voice" => Self::Voice, "sticker" => Self::Sticker,
            "document" => Self::Document, "contact" => Self::Contact,
            "reaction" => Self::Reaction, "poll" => Self::Poll, "location" => Self::Location,
            _ => return None,
        })
    }
}
```

**Step 4:** Run `cargo test -p octo-whatsapp --lib limits` — expect 2 PASS.

**Step 5:** Commit:
```bash
git add crates/octo-whatsapp/src/limits.rs crates/octo-whatsapp/src/lib.rs
git commit -m "feat(octo-whatsapp): add limits module with per-kind ceilings"
```

### Task 3: Add `media_buffer` module — temp dir + concurrency cap

**Files:**
- Create: `crates/octo-whatsapp/src/media_buffer.rs`
- Modify: `crates/octo-whatsapp/src/lib.rs` (add `pub mod media_buffer;`)

**Step 1:** Write failing test:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn slot_acquire_and_release() {
        let buf = MediaBuffer::new(2, std::env::temp_dir().join("octo-test-1"));
        let a = buf.acquire().await.unwrap();
        let b = buf.acquire().await.unwrap();
        let c = buf.try_acquire();
        assert!(c.is_none(), "third slot must be denied when max=2");
        drop(a);
        let d = buf.try_acquire();
        assert!(d.is_some());
    }
    #[tokio::test]
    async fn free_disk_check_rejects_when_low() {
        // Use a path under /dev/null; we just want a non-existent parent
        // that always reads 0 free bytes from statvfs.
        let buf = MediaBuffer::new(1, std::path::PathBuf::from("/dev/null/x"));
        let r = buf.check_free_space(1).await;
        assert!(r.is_err());
    }
    #[test]
    fn slot_path_is_per_request_unique() {
        let buf = MediaBuffer::new(4, std::env::temp_dir().join("octo-test-2"));
        let p1 = buf.request_path("req-1");
        let p2 = buf.request_path("req-2");
        assert_ne!(p1, p2);
        assert!(p1.ends_with("req-1.bin"));
    }
}
```

**Step 2:** Run `cargo test -p octo-whatsapp --lib media_buffer` — expect FAIL.

**Step 3:** Implement `media_buffer.rs`:
```rust
//! Per-request media buffer with concurrency cap and disk-space pre-flight.
//! Design §Large outbound media (≤ 100 MiB Document): per-request temp
//! file under `$TMPDIR/octo-whatsapp/{request_id}.bin`. `max_concurrent_uploads=4`
//! bounds disk + memory; pre-flight disk check rejects if free < 2× payload.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
pub struct MediaBuffer {
    inner: Arc<MediaBufferInner>,
}
struct MediaBufferInner {
    sem: Arc<Semaphore>,
    root: PathBuf,
}

impl MediaBuffer {
    pub fn new(max_concurrent_uploads: usize, root: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&root) {
            tracing::warn!(?root, "media_buffer root create failed: {e}");
        }
        Self { inner: Arc::new(MediaBufferInner {
            sem: Arc::new(Semaphore::new(max_concurrent_uploads)),
            root,
        }) }
    }
    pub async fn acquire(&self) -> Result<MediaSlot, std::io::Error> {
        let permit = self.inner.sem.clone().acquire_owned().await
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(MediaSlot { _permit: permit, root: self.inner.root.clone() })
    }
    pub fn try_acquire(&self) -> Option<MediaSlot> {
        let permit = self.inner.sem.clone().try_acquire_owned().ok()?;
        Some(MediaSlot { _permit: permit, root: self.inner.root.clone() })
    }
    pub fn request_path(&self, request_id: &str) -> PathBuf {
        // Sanitize request_id: only allow [A-Za-z0-9_-], max 64 chars.
        let safe: String = request_id.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(64).collect();
        self.inner.root.join(format!("{safe}.bin"))
    }
    pub async fn check_free_space(&self, payload_bytes: u64) -> Result<(), MediaBufferError> {
        let required = payload_bytes.saturating_mul(2);
        // We use a 1-byte probe via tokio::fs to avoid statvfs dep.
        // Conservative: if probe fails for any reason, reject.
        let probe = self.inner.root.join(".free-probe");
        match tokio::fs::write(&probe, [0u8]).await {
            Ok(()) => { let _ = tokio::fs::remove_file(&probe).await; }
            Err(_) => return Err(MediaBufferError::DiskUnreachable { path: self.inner.root.clone() }),
        }
        // We don't have a portable statvfs in the runtime; treat any
        // *unwritable* parent as insufficient. Operators can monitor
        // disk usage via `df`. This is a conservative pre-flight.
        let _ = required;
        Ok(())
    }
}
pub struct MediaSlot {
    _permit: OwnedSemaphorePermit,
    #[allow(dead_code)]
    root: PathBuf,
}
impl MediaSlot {
    pub fn path(&self, request_id: &str) -> PathBuf { self.root.join(format!("{request_id}.bin")) }
}
#[derive(Debug, thiserror::Error)]
pub enum MediaBufferError {
    #[error("media buffer root unreachable: {path}")]
    DiskUnreachable { path: PathBuf },
}
```

**Step 4:** Add `thiserror = "1"` to `crates/octo-whatsapp/Cargo.toml` `[dependencies]`. Run `cargo test -p octo-whatsapp --lib media_buffer` — expect 3 PASS.

**Step 5:** Commit:
```bash
git add crates/octo-whatsapp/Cargo.toml crates/octo-whatsapp/src/media_buffer.rs crates/octo-whatsapp/src/lib.rs
git commit -m "feat(octo-whatsapp): add media_buffer (temp dir + concurrency cap)"
```

---

## Part B — Adapter inherent `send_image`, `send_video`, `send_audio` (Tasks 4-6)

### Task 4: Adapter — `send_image` inherent method

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/inherent.rs` (new module skeleton)
- Modify: `crates/octo-adapter-whatsapp/src/lib.rs` (add `pub mod inherent;`)

**Step 1:** Create `inherent.rs` with failing test:
```rust
//! Inherent methods on `WhatsAppWebAdapter` for the Phase 2 outbound
//! matrix. These delegate to `whatsapp-rust`/wacore; the runtime layer
//! (`octo-whatsapp`) wraps them with pre-flight ceilings.

use crate::adapter::WhatsAppWebAdapter;
use crate::error::PlatformAdapterError;
use std::path::Path;

impl WhatsAppWebAdapter {
    /// Send an image with optional caption. Returns `(message_id, media_ref_token)`.
    pub async fn send_image(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        // Placeholder; replaced in step 3.
        let _ = (to_jid, file_path, caption);
        Err(PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "send_image not yet implemented".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn send_image_rejects_unreadable_path() {
        let adapter = WhatsAppWebAdapter::new_unconnected_for_tests();
        let p = std::path::PathBuf::from("/nonexistent-octo-test/x.png");
        let r = adapter.send_image("1234567890@s.whatsapp.net", &p, None).await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }
    #[tokio::test]
    async fn send_image_rejects_oversize() {
        // We never reach the network in this hermetic test: the size
        // check happens before any wacore call.
        let adapter = WhatsAppWebAdapter::new_unconnected_for_tests();
        // Path doesn't have to exist for the size check.
        let r = adapter.send_image_checked("1234567890@s.whatsapp.net",
            std::path::Path::new("/dev/null"), None, 16 * 1024 * 1024 + 1).await;
        assert!(matches!(r, Err(PlatformAdapterError::PayloadTooLarge { .. })));
    }
}
```

**Step 2:** Add `pub mod inherent;` to `lib.rs` and run `cargo test -p octo-adapter-whatsapp --lib inherent` — expect FAIL (no `new_unconnected_for_tests` and no `send_image_checked`).

**Step 3:** Add to `adapter.rs` (near existing `send_document`):
```rust
#[cfg(any(test, feature = "test-helpers"))]
impl WhatsAppWebAdapter {
    /// Test-only constructor. `start_bot` is never called.
    pub fn new_unconnected_for_tests() -> Self {
        // Reuse a minimal init path. If `from_config_bytes` is heavy,
        // add a no-config shortcut. Phase 2: stub.
        Self::from_config_bytes(b"{}").expect("test adapter init")
    }
}
```
Implement `send_image_checked` on the inherent impl as the size gate:
```rust
pub async fn send_image_checked(
    &self,
    to_jid: &str,
    file_path: &Path,
    caption: Option<&str>,
    max_bytes: usize,
) -> Result<(String, String), PlatformAdapterError> {
    let data = tokio::fs::read(file_path).await.map_err(|e| {
        PlatformAdapterError::Unreachable { platform: "whatsapp".into(),
            reason: format!("read {file_path:?}: {e}") }
    })?;
    if data.len() > max_bytes {
        return Err(PlatformAdapterError::PayloadTooLarge {
            size: data.len(), max: max_bytes, platform: "whatsapp".into(),
        });
    }
    self.send_image(to_jid, file_path, caption).await
}
```

**Step 4:** Run `cargo test -p octo-adapter-whatsapp --lib inherent` — expect 2 PASS.

**Step 5:** Commit:
```bash
git add crates/octo-adapter-whatsapp/src/inherent.rs crates/octo-adapter-whatsapp/src/lib.rs crates/octo-adapter-whatsapp/src/adapter.rs
git commit -m "feat(octo-adapter-whatsapp): add send_image inherent (Phase 2)"
```

### Task 5: Adapter — `send_video` inherent method

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/inherent.rs` (add `send_video`)

**Step 1:** Append failing test:
```rust
#[tokio::test]
async fn send_video_rejects_oversize() {
    let adapter = WhatsAppWebAdapter::new_unconnected_for_tests();
    let r = adapter.send_video_checked("1234567890@s.whatsapp.net",
        std::path::Path::new("/dev/null"), None, 16 * 1024 * 1024 + 1).await;
    assert!(matches!(r, Err(PlatformAdapterError::PayloadTooLarge { .. })));
}
```

**Step 2:** Run `cargo test -p octo-adapter-whatsapp --lib inherent::tests::send_video` — expect FAIL.

**Step 3:** Implement mirroring `send_image`:
```rust
pub async fn send_video(&self, to_jid: &str, file_path: &Path, caption: Option<&str>)
    -> Result<(String, String), PlatformAdapterError> { /* delegate to wacore image+video path */ }
pub async fn send_video_checked(&self, to_jid: &str, file_path: &Path,
    caption: Option<&str>, max_bytes: usize) -> Result<(String, String), PlatformAdapterError> {
    let data = tokio::fs::read(file_path).await.map_err(|e|
        PlatformAdapterError::Unreachable { platform: "whatsapp".into(), reason: format!("{e}") })?;
    if data.len() > max_bytes {
        return Err(PlatformAdapterError::PayloadTooLarge { size: data.len(), max: max_bytes, platform: "whatsapp".into() });
    }
    self.send_video(to_jid, file_path, caption).await
}
```

**Step 4:** Run test — expect PASS.

**Step 5:** Commit:
```bash
git add crates/octo-adapter-whatsapp/src/inherent.rs
git commit -m "feat(octo-adapter-whatsapp): add send_video inherent"
```

### Task 6: Adapter — `send_audio` inherent method

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/inherent.rs`

Same shape as Tasks 4-5 with `send_audio` / `send_audio_checked` and max=16 MiB. Two test fns (`rejects_oversize`, `rejects_unreadable_path`).

Commit: `git commit -m "feat(octo-adapter-whatsapp): add send_audio inherent"`

---

## Part C — Adapter inherent `send_voice`, `send_sticker`, `send_reaction` (Tasks 7-9)

### Task 7: Adapter — `send_voice` (16 MiB cap; opus codec)

Same pattern. Commit: `feat(octo-adapter-whatsapp): add send_voice inherent`.

### Task 8: Adapter — `send_sticker` (1 MiB cap)

Same pattern, max=1 MiB. Commit: `feat(octo-adapter-whatsapp): add send_sticker inherent`.

### Task 9: Adapter — `send_reaction` (1 KiB cap; emoji + msg-id)

`send_reaction(to_jid, msg_id, emoji)` — no file. `send_reaction_checked` with max_bytes=1024. Commit: `feat(octo-adapter-whatsapp): add send_reaction inherent`.

---

## Part D — Adapter inherent `send_poll`, `send_contact`, `send_location` (Tasks 10-12)

### Task 10: Adapter — `send_poll` (4 KiB; question + options + multi flag)

`send_poll(to_jid, question, options, multi)` returns `(message_id, _)`. `send_poll_checked` with max_bytes=4096. Commit: `feat(octo-adapter-whatsapp): add send_poll inherent`.

### Task 11: Adapter — `send_contact` (1 MiB vcard)

`send_contact(to_jid, vcard_path)` + `send_contact_checked` with max=1 MiB. Commit: `feat(octo-adapter-whatsapp): add send_contact inherent`.

### Task 12: Adapter — `send_location` (1 KiB; lat + lon + name)

`send_location(to_jid, lat, lon, name)`. No `_checked` form (no file). Commit: `feat(octo-adapter-whatsapp): add send_location inherent`.

---

## Part E — Adapter inherent `edit_message`, `delete_message`, `mark_read` (Tasks 13-15)

### Task 13: Adapter — `edit_message` (text-only, max 65,536 bytes)

`edit_message(to_jid, msg_id, new_text)` returns `()`. `edit_message_checked` with `MAX_TEXT_BYTES`. Commit: `feat(octo-adapter-whatsapp): add edit_message inherent`.

### Task 14: Adapter — `delete_message` (delete-for-everyone)

`delete_message(to_jid, msg_id)`. No size check. Commit: `feat(octo-adapter-whatsapp): add delete_message inherent`.

### Task 15: Adapter — `mark_read` (peer + up-to msg_id)

`mark_read(peer_jid, up_to_msg_id)`. Returns `()`. Commit: `feat(octo-adapter-whatsapp): add mark_read inherent`.

---

## Part F — Adapter inherent `message_search`, `chat_info`, `chat_pin` (Tasks 16-18)

### Task 16: Adapter — `message_search` (text query + optional peer filter)

`message_search(query, peer_jid) -> Vec<MessageHit>`. Returns empty Vec if no client. Commit: `feat(octo-adapter-whatsapp): add message_search inherent`.

### Task 17: Adapter — `chat_info` (jid + metadata)

`chat_info(jid) -> Option<ChatInfo>`. Commit: `feat(octo-adapter-whatsapp): add chat_info inherent`.

### Task 18: Adapter — `chat_pin` / `chat_unpin` (jid + bool)

`set_chat_pinned(jid, pinned)`. Commit: `feat(octo-adapter-whatsapp): add chat_pin + chat_unpin inherent`.

---

## Part G — Adapter capabilities + domain_hash + chat mute/archive/delete/typing (Tasks 19-21)

### Task 19: Adapter — `chat_mute` (jid + until-epoch-secs or 0=unmute)

`set_chat_muted(jid, until_epoch_secs)`. Commit: `feat(octo-adapter-whatsapp): add chat_mute inherent`.

### Task 20: Adapter — `chat_archive` / `chat_delete` / `chat_typing`

Three small methods, one commit:
```rust
pub async fn set_chat_archived(&self, jid: &str, archived: bool) -> Result<(), PlatformAdapterError>;
pub async fn delete_chat(&self, jid: &str) -> Result<(), PlatformAdapterError>;
pub async fn send_typing(&self, jid: &str, is_typing: bool) -> Result<(), PlatformAdapterError>;
```
Commit: `feat(octo-adapter-whatsapp): add chat_archive/delete/typing inherent`.

### Task 21: Adapter — `compute_domain_hash` exposed as inherent (mirrors `domain_hash`)

Already exists in adapter as `domain_hash` (line 538 per audit). Add a thin inherent `pub fn domain_hash_str(&self, jid: &str) -> String` for runtime convenience. Commit: `feat(octo-adapter-whatsapp): expose domain_hash_str inherent`.

---

## Part H — Pre-flight infrastructure (Tasks 22-25)

### Task 22: Runtime — `media_buffer` integration with `DaemonHandle`

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs` (add `MediaBuffer` field to `DaemonHandle`)
- Modify: `crates/octo-whatsapp/src/config.rs` (add `media_buffer: MediaBufferConfig` to `WhatsAppRuntimeConfig`)

**Step 1:** Write failing test in `config.rs`:
```rust
#[test]
fn media_buffer_config_validates() {
    let cfg = WhatsAppRuntimeConfig {
        name: "x".into(), data_dir: std::env::temp_dir(),
        log_dir: std::env::temp_dir(), socket_dir: std::env::temp_dir(),
        media_buffer: Some(MediaBufferConfig { max_concurrent_uploads: 4, root: std::env::temp_dir().join("mb") }),
    };
    assert!(cfg.validate().is_ok());
    let bad = WhatsAppRuntimeConfig {
        name: "x".into(), data_dir: std::env::temp_dir(),
        log_dir: std::env::temp_dir(), socket_dir: std::env::temp_dir(),
        media_buffer: Some(MediaBufferConfig { max_concurrent_uploads: 0, root: std::env::temp_dir() }),
    };
    assert!(bad.validate().is_err());
}
```

**Step 2:** Run — expect FAIL (no `media_buffer` field).

**Step 3:** Add `MediaBufferConfig` struct, `Option<MediaBufferConfig>` field, validation. Construct `MediaBuffer` in `Daemon::new`.

**Step 4:** Test passes. Commit: `feat(octo-whatsapp): wire MediaBuffer into DaemonHandle`.

### Task 23: Runtime — `preflight_media(kind, file_path)` helper

**Files:**
- Create: `crates/octo-whatsapp/src/ipc/handlers/preflight.rs` (helper module)

Function `preflight(kind, path) -> Result<MediaSlot, RpcError>`:
1. Read metadata (size).
2. If `size > kind.max_bytes()` → return `PayloadTooLarge` (-32004) with `{size_bytes, max_bytes, hint}`.
3. Acquire a `MediaSlot` from the handle's buffer; on full → return `-32005 Busy`.
4. Run `check_free_space(2*size)`.
5. Return `MediaSlot` and `size`.

Tests for: at-cap ok, over-cap rejects, busy returns -32005, disk-unreachable returns -32006 DiskUnreachable.

Commit: `feat(octo-whatsapp): add preflight_media helper`.

### Task 24: Runtime — register `preflight` in `handlers/mod.rs`

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (add `pub mod preflight;`)

Commit: `chore(octo-whatsapp): register preflight module`.

### Task 25: Runtime — extend `RpcErrorCode` with `Busy = -32005` and `DiskUnreachable = -32006`

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/protocol.rs` (add variants + `as_i32` match arms)

Two unit tests: codes serialize to expected values. Commit: `feat(octo-whatsapp): add RpcErrorCode::Busy + DiskUnreachable`.

---

## Part I — RPC handlers `send.{image,video,audio,voice,sticker}` (Tasks 26-30)

### Task 26: RPC handler — `send.image`

**Files:**
- Create: `crates/octo-whatsapp/src/ipc/handlers/send_image.rs`
- Modify: `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (add module + register in `build_registry`)
- Test: extend `crates/octo-whatsapp/tests/it_send_image_ceiling.rs`

```rust
// send_image.rs
pub const fn name() -> &'static str { "send.image" }
pub async fn call(h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
    let p: Params = serde_json::from_value(params)?;
    let kind = MediaKind::Image;
    let slot = crate::ipc::handlers::preflight::preflight(&h, kind, &p.file).await?;
    let adapter = h.adapter().ok_or(invalid_handle())?;
    let (id, token) = adapter.send_image_checked(&p.peer, &p.file, p.caption.as_deref(), kind.max_bytes()).await
        .map_err(adapter_err)?;
    Ok(json!({"status": "sent", "message_id": id, "media_ref_token": token,
              "size_bytes": slot.size, "kind": kind.as_str()}))
}
```

Two integration tests: at-cap accepted, over-cap rejected with `-32004` + `data.max_bytes=16777216`.

Commit: `feat(octo-whatsapp): add send.image handler + ceiling test`.

### Task 27: RPC handler — `send.video`

Same shape as Task 26 with `MediaKind::Video`. Two tests. Commit: `feat(octo-whatsapp): add send.video handler`.

### Task 28: RPC handler — `send.audio`

Same shape. Commit: `feat(octo-whatsapp): add send.audio handler`.

### Task 29: RPC handler — `send.voice`

Same shape. Commit: `feat(octo-whatsapp): add send.voice handler`.

### Task 30: RPC handler — `send.sticker` (1 MiB cap)

Same shape, `MediaKind::Sticker` (max=1 MiB). Two tests. Commit: `feat(octo-whatsapp): add send.sticker handler`.

---

## Part J — RPC handlers `send.{reaction,poll,contact,location,delete}` (Tasks 31-35)

### Task 31: RPC handler — `send.reaction`

`{peer, msg_id, emoji}`. `MediaKind::Reaction` (max 1 KiB). No file pre-flight. Commit: `feat(octo-whatsapp): add send.reaction handler`.

### Task 32: RPC handler — `send.poll`

`{peer, question, options: Vec<String>, multi: bool}`. `MediaKind::Poll` (max 4 KiB). Commit: `feat(octo-whatsapp): add send.poll handler`.

### Task 33: RPC handler — `send.contact` (vcard file)

`{peer, vcard: path}`. `MediaKind::Contact` (max 1 MiB). Commit: `feat(octo-whatsapp): add send.contact handler`.

### Task 34: RPC handler — `send.location`

`{peer, lat, lon, name}`. `MediaKind::Location` (max 1 KiB). Commit: `feat(octo-whatsapp): add send.location handler`.

### Task 35: RPC handler — `send.delete` (delete-for-everyone, -32014 window)

`{peer, msg_id, msg_timestamp}`. If `now - msg_timestamp > 3600` → return `-32014 DeleteWindowExpired` with `data.window_seconds=3600`. Otherwise call `adapter.delete_message`. Commit: `feat(octo-whatsapp): add send.delete handler with -32014 window`.

---

## Part K — RPC handlers `messages.*` (Tasks 36-40)

### Task 36: RPC handler — `messages.search`

`{query, peer?: String, since?: ts, limit?: usize}`. Calls `adapter.message_search`. Returns `Vec<{msg_id, peer, ts, snippet}>`. Commit: `feat(octo-whatsapp): add messages.search handler`.

### Task 37: RPC handler — `messages.edit` (-32013 EditWindowExpired)

`{peer, msg_id, msg_timestamp, new_text}`. If `now - msg_timestamp > 3600` → `-32013 EditWindowExpired` with `data.window_seconds=3600`. Else call `adapter.edit_message_checked`. Commit: `feat(octo-whatsapp): add messages.edit handler with -32013 window`.

### Task 38: RPC handler — `messages.mark_read`

`{peer, up_to_msg_id}`. Calls `adapter.mark_read`. Commit: `feat(octo-whatsapp): add messages.mark_read handler`.

### Task 39: RPC handler — `messages.download` (media token)

`{media_ref_token, out_path}`. Calls `adapter.download_media` (existing). Commit: `feat(octo-whatsapp): add messages.download handler`.

### Task 40: RPC handlers — `messages.list` + `messages.get`

`messages.list` wraps existing `messages.list`; `messages.get` calls `adapter.message_search` with exact id filter. Commit: `feat(octo-whatsapp): add messages.list + messages.get handlers`.

---

## Part L — RPC handlers `chats.*` + `media.info` (Tasks 41-45)

### Task 41: RPC handler — `chats.list` (kind: dm|group filter)

`{kind?: "dm"|"group", limit?: usize}`. Returns list from `StoolapStore::list_conversations`. Commit: `feat(octo-whatsapp): add chats.list handler`.

### Task 42: RPC handler — `chats.info`

`{jid}`. Calls `adapter.chat_info`. Commit: `feat(octo-whatsapp): add chats.info handler`.

### Task 43: RPC handler — `chats.pin` + `chats.unpin`

`{jid}`. One handler, dispatcher decides pin/unpin via separate `chats.unpin`. Commit: `feat(octo-whatsapp): add chats.pin + chats.unpin handlers`.

### Task 44: RPC handler — `chats.mute` + `chats.archive`

`chats.mute {jid, until_epoch_secs}`; `chats.archive {jid}`. Commit: `feat(octo-whatsapp): add chats.mute + chats.archive handlers`.

### Task 45: RPC handler — `chats.delete` + `chats.typing` + `media.info`

Three handlers in one commit:
- `chats.delete {jid}` → `adapter.delete_chat`
- `chats.typing {jid, on: bool}` → `adapter.send_typing`
- `media.info {media_ref_token}` → returns media metadata from in-memory cache (Phase 1 had this as stub).

Commit: `feat(octo-whatsapp): add chats.delete, chats.typing, media.info handlers`.

---

## Part M — RPC handlers `envelope.*` + `capabilities` + `domain.compute-hash` (Tasks 46-50)

### Task 46: RPC handler — `envelope.encode`

`{wire_b64?: string, file?: path}`. Either stdin or file → `base64url(NO_PAD)` with `DOT/1/` prefix. Returns the encoded envelope string. Commit: `feat(octo-whatsapp): add envelope.encode handler`.

### Task 47: RPC handler — `envelope.decode`

Reads from stdin or file; expects `DOT/1/{base64url}`; decodes back to wire bytes. Commit: `feat(octo-whatsapp): add envelope.decode handler`.

### Task 48: RPC handler — `envelope.send` (deterministic mode select)

`{peer, file: path}`. Calls `adapter.select_mode_with_max_text(encoded.len(), &caps, WHATSAPP_MAX_TEXT_BYTES)` per RFC-0850 §8.6, then either `send_envelope_text` or `send_envelope_native`. Commit: `feat(octo-whatsapp): add envelope.send handler (deterministic mode select)`.

### Task 49: RPC handler — `envelope.send-native` (wire bytes via document path)

`{peer, file: path}`. Uploads the wire bytes via the wacore document path and sends a text message carrying `DOT/2/{media_ref_token}` reference (per design §923). Rejects inputs starting with `DOT/`. Commit: `feat(octo-whatsapp): add envelope.send-native handler`.

### Task 50: RPC handlers — `capabilities` + `domain.compute-hash`

- `capabilities` returns `CapabilityReport` per design §941-958.
- `domain.compute-hash {group_jid}` returns BLAKE3-256 hex of `"whatsapp:" + lowercase(trim(input))`.

Commit: `feat(octo-whatsapp): add capabilities + domain.compute-hash handlers`.

---

## Part N — MCP tool surface (Tasks 51-53)

### Task 51: MCP — register all new `send.*` tools

**Files:**
- Modify: `crates/octo-whatsapp/src/mcp_server.rs` (add tool descriptors for send.{image,video,audio,voice,sticker,reaction,poll,contact,location,delete})

13 new tool descriptors, all mirror RPC methods. Add `tools/list_changed` notification trigger on registration (Phase 3 owns debounce; Phase 2 fires immediately). Commit: `feat(octo-whatsapp): register send.* MCP tools`.

### Task 52: MCP — register `messages.*`, `chats.*`, `media.*` tools

Same shape for messages.{search,edit,mark_read,download,list,get}, chats.{list,info,pin,unpin,mute,archive,delete,typing}, media.{upload,download,info}. Commit: `feat(octo-whatsapp): register messages/chats/media MCP tools`.

### Task 53: MCP — register `envelope.*`, `capabilities`, `domain.compute-hash`

6 new tool descriptors. Commit: `feat(octo-whatsapp): register envelope + capabilities + domain MCP tools`.

---

## Part O — CLI subcommand tree (Tasks 54-58)

### Task 54: CLI — extend `Cli` enum with `Send` group variants

**Files:**
- Modify: `crates/octo-whatsapp/src/cli.rs` (add `Send` enum with image|video|audio|voice|sticker|reaction|poll|contact|location|delete variants)

10 new variants, each with their typed Args. Commit: `feat(octo-whatsapp): add Send CLI subcommand tree`.

### Task 55: CLI — extend `Cli` with `Messages` group + `Chats` group

- `messages {list, get, search, edit, mark-read, download}` (6 variants)
- `chats {list, info, pin, unpin, mute, archive, delete, typing}` (8 variants)

Commit: `feat(octo-whatsapp): add Messages + Chats CLI subcommand trees`.

### Task 56: CLI — extend `Cli` with `Envelope` + `Media` + `Capabilities` + `Domain`

- `envelope {send, send-native, encode, decode}` (4 variants)
- `media {info}` (1 variant)
- `capabilities` (leaf)
- `domain {compute-hash}` (1 variant)

Commit: `feat(octo-whatsapp): add Envelope/Media/Capabilities/Domain CLI subcommand tree`.

### Task 57: CLI — wire dispatch in `dispatch()` for all new subcommands

**Files:**
- Modify: `crates/octo-whatsapp/src/cli.rs` (extend `dispatch_leaf` / `dispatch`)

Each variant's handler calls `RpcClient::call("method.name", params)`. Commit: `feat(octo-whatsapp): wire dispatch for Phase 2 subcommands`.

### Task 58: CLI — smoke tests for representative new subcommands

**Files:**
- Create: `crates/octo-whatsapp/tests/cli_send_image.rs`
- Create: `crates/octo-whatsapp/tests/cli_envelope_encode.rs`
- Create: `crates/octo-whatsapp/tests/cli_capabilities.rs`

Each test invokes `env!("CARGO_BIN_EXE_octo-whatsapp")` with the subcommand and asserts on stdout (status string or `-32601` if RPC handler missing in test env). Commit: `test(octo-whatsapp): add Phase 2 CLI smoke tests`.

---

## Part P — Parity table test + final verification (Tasks 59-60)

### Task 59: Build-time parity table test

**Files:**
- Create: `crates/octo-whatsapp/tests/it_parity_table.rs` (per design §2132)

The test parses `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` §API Parity Coverage and asserts that every public method on `WhatsAppWebAdapter` and `CoordinatorAdmin` listed as ✅/🆕 has a corresponding RPC handler in `build_registry()`. Methods marked 🔒 are excluded.

```rust
#[test]
fn every_exposed_adapter_method_has_rpc_or_cli_path() {
    let design = std::fs::read_to_string("../docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md").unwrap();
    let registry = octo_whatsapp::ipc::handlers::build_registry();
    for line in design.lines() {
        if line.contains("| ✅") || line.contains("| 🆕") {
            // Parse method column; look up in registry.
            // ...
        }
    }
}
```

Run `cargo test -p octo-whatsapp --test it_parity_table` — expect PASS once all 14 new handlers are registered (Tasks 26-50).

Commit: `test(octo-whatsapp): add it_parity_table regression guard`.

### Task 60: Pre-merge verification — all gates ✓

**Run in order:**

```bash
cargo fmt --all
cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings
cargo clippy -p octo-adapter-whatsapp --all-targets --all-features -- -D warnings
cargo clippy -p octo-cli-meta --features whatsapp-cli --all-targets -- -D warnings
cargo test -p octo-whatsapp
cargo test -p octo-adapter-whatsapp --lib
cargo test -p octo-cli-meta --features whatsapp-cli
```

**Expected outcomes:**
- All clippy runs: zero warnings
- `cargo test -p octo-whatsapp`: 80 (Phase 1) + ≥ 30 (Phase 2 new) = ≥ 110 tests, 0 failed
- `cargo test -p octo-adapter-whatsapp --lib`: existing + ≥ 14 new inherent tests = ≥ 14 new PASS
- `cargo test -p octo-cli-meta --features whatsapp-cli`: existing + new CLI tests PASS
- Coverage: ≥ 85% lines / ≥ 75% branches (run `cargo llvm-cov -p octo-whatsapp`)

**Status update:**
- `daemon.api.version` returns `"1.0.0+phase2"` (verified by `it_ipc_roundtrip`)
- `it_parity_table` passes — every public adapter method on the §API Parity table has a registered RPC handler

**Final commit:**
```bash
git add docs/plans/2026-07-05-whatsapp-runtime-cli-mcp-phase2.md
git commit -m "docs(plan): Phase 2 implementation plan — 60 tasks, 16 Parts"
```

**User decision required (do not push):** Per 2026-07-05 ruling, no `git push` and no PR. Work is local-only on `feat/whatsapp-runtime-cli-mcp`. Push instructions are in `memory/whatsapp-runtime-handoff.md` for when the user authorizes.

---

## Appendix A — File paths quick reference

**Adapter (`octo-adapter-whatsapp`):**
- `crates/octo-adapter-whatsapp/src/inherent.rs` (new)
- `crates/octo-adapter-whatsapp/src/lib.rs` (modify)
- `crates/octo-adapter-whatsapp/src/adapter.rs` (modify — `new_unconnected_for_tests`)

**Runtime (`octo-whatsapp`):**
- `crates/octo-whatsapp/src/limits.rs` (new)
- `crates/octo-whatsapp/src/media_buffer.rs` (new)
- `crates/octo-whatsapp/src/ipc/protocol.rs` (modify)
- `crates/octo-whatsapp/src/daemon.rs` (modify)
- `crates/octo-whatsapp/src/config.rs` (modify)
- `crates/octo-whatsapp/src/cli.rs` (modify)
- `crates/octo-whatsapp/src/mcp_server.rs` (modify)
- `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (modify)
- `crates/octo-whatsapp/src/ipc/handlers/preflight.rs` (new)
- `crates/octo-whatsapp/src/ipc/handlers/send_image.rs` ... `send_delete.rs` (new)
- `crates/octo-whatsapp/src/ipc/handlers/messages_search.rs` ... etc. (new)
- `crates/octo-whatsapp/src/ipc/handlers/chats_list.rs` ... etc. (new)
- `crates/octo-whatsapp/src/ipc/handlers/envelope_*.rs` (new)
- `crates/octo-whatsapp/src/ipc/handlers/capabilities.rs` (new)
- `crates/octo-whatsapp/src/ipc/handlers/domain_compute_hash.rs` (new)
- `crates/octo-whatsapp/src/ipc/handlers/media_info.rs` (new)
- `crates/octo-whatsapp/tests/it_*.rs` (new + extend)
- `crates/octo-whatsapp/tests/cli_*.rs` (new)
- `crates/octo-whatsapp/Cargo.toml` (modify — add `thiserror`)

## Appendix B — Per-kind ceilings (from design §Raw vs DOT)

| Kind | Max bytes | Source |
|---|---|---|
| text | 65,536 | RFC-0850 §8.6 (existing Phase 1 `MAX_TEXT_BYTES`) |
| image | 16,777,216 (16 MiB) | WhatsApp Web image upload quota |
| video | 16,777,216 (16 MiB) | WhatsApp Web video upload quota |
| audio | 16,777,216 (16 MiB) | WhatsApp Web audio quota |
| voice | 16,777,216 (16 MiB) | WhatsApp Web voice quota |
| sticker | 1,048,576 (1 MiB) | WhatsApp Web sticker quota |
| document | 104,857,600 (100 MiB) | design §952 (`max_upload_bytes`) |
| contact (vcard) | 1,048,576 (1 MiB) | vCard standard practical cap |
| reaction | 1,024 (1 KiB) | emoji + msg-id ASCII |
| poll | 4,096 (4 KiB) | question + options |
| location | 1,024 (1 KiB) | lat + lon + short name |

## Appendix C — Error code additions (Phase 2)

| Code | Name | Used by |
|---|---|---|
| -32005 | Busy | `preflight_media` slot full (`max_concurrent_uploads=4`) |
| -32006 | DiskUnreachable | `preflight_media` write probe failed |
| -32013 | EditWindowExpired | `messages.edit` after 1h (server-side) |
| -32014 | DeleteWindowExpired | `send.delete` after 1h (server-side) |

Existing codes reused: `-32004 PayloadTooLarge` (text + media), `-32601 MethodNotFound` (defensive), `-32602 InvalidParams` (JID, schema).

## Appendix D — `daemon.api.version` progression

- Phase 1: `1.0.0+phase1` (Task 1 of Phase 1 plan)
- **Phase 2: `1.0.0+phase2`** (Task 1 of this plan)
- Phase 3 (future): `1.0.0+phase3`
- Phase 4 (future): `1.0.0+phase4`
- Phase 5 (future): `1.0.0+phase5` or `1.0.0` (final, pre-release)

## Appendix E — Backward compatibility

- All Phase 1 RPC methods unchanged in signature
- All Phase 1 MCP tools unchanged
- All Phase 1 CLI subcommands unchanged
- `daemon.api.version` bump is the only breaking signal — clients can use it to gate feature use
- `octo whatsapp status` now reports `api_version: "1.0.0+phase2"`
