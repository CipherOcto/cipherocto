//! Mock `OctoWhatsAppAdapter` for hermetic handler tests.
//!
//! Each method increments a counter (`call_counts`) and returns either a
//! canned response (set via `set_return`, `set_pair_return`,
//! `set_message_search_result`, `set_chat_info_result`) or the default
//! success response. Tests override per-method behavior to exercise
//! error paths without instantiating a live WhatsApp Web session.
//!
//! The mock is `Send + Sync` — it uses `parking_lot::Mutex` internally
//! and is shared via `Arc`, so multiple clones share the same state.
//!
//! See `docs/plans/2026-07-05-whatsapp-runtime-cli-mcp-phase2.md` Phase B
//! for the full design.

#![cfg(any(test, feature = "test-helpers"))]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use octo_adapter_whatsapp::{ChatInfo, MessageHit};
use octo_network::dot::adapters::{CapabilityReport, MediaCapabilities};
use octo_network::dot::error::PlatformAdapterError;

use crate::adapter_trait::OctoWhatsAppAdapter;

/// Single-result canned response (most methods that return `String`).
pub type CannedSingleResult = Result<String, PlatformAdapterError>;

/// Pair-result canned response (media methods that return `(msg_id, token)`).
#[derive(Debug, Clone)]
pub enum CannedPairResult {
    Ok { id: String, token: String },
    Err(PlatformAdapterError),
}

#[derive(Debug, Default)]
struct MockState {
    /// Method name (static str) -> number of calls.
    call_counts: HashMap<&'static str, u64>,
    /// Per-method response override for single-result methods.
    canned_single: HashMap<&'static str, CannedSingleResult>,
    /// Per-method response override for pair-result (media) methods.
    canned_pair: HashMap<&'static str, CannedPairResult>,
    /// Per-method response override for `message_search` (returns `Vec`).
    canned_search: HashMap<&'static str, Vec<MessageHit>>,
    /// Per-method response override for `chat_info` (returns `Option`).
    canned_chat_info: HashMap<&'static str, Option<ChatInfo>>,
    /// Per-method response override for `download_media` (returns `Vec<u8>`).
    canned_download: HashMap<&'static str, Vec<u8>>,
    /// Per-method response override for unit-result (`()`) methods.
    canned_unit_err: HashMap<&'static str, PlatformAdapterError>,
}

/// `MockAdapter` — in-memory `OctoWhatsAppAdapter` used by hermetic tests.
///
/// Construct via `MockAdapter::new()` (or `MockAdapter::default()`).
/// Tests then call methods, optionally override canned responses via the
/// `set_*` setters, and verify call counts via `call_count()`.
#[derive(Debug)]
pub struct MockAdapter {
    state: Arc<Mutex<MockState>>,
}

impl MockAdapter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::default())),
        }
    }

    /// Returns the number of times `method` has been invoked on this mock.
    pub fn call_count(&self, method: &'static str) -> u64 {
        self.state
            .lock()
            .call_counts
            .get(method)
            .copied()
            .unwrap_or(0)
    }

    /// Override the canned response for a single-result (`String`) method.
    pub fn set_return(&self, method: &'static str, r: CannedSingleResult) {
        self.state.lock().canned_single.insert(method, r);
    }

    /// Override the canned response for a pair-result (`(id, token)`) method.
    pub fn set_pair_return(&self, method: &'static str, r: CannedPairResult) {
        self.state.lock().canned_pair.insert(method, r);
    }

    /// Override the canned response for `message_search`.
    pub fn set_message_search_result(&self, method: &'static str, hits: Vec<MessageHit>) {
        self.state.lock().canned_search.insert(method, hits);
    }

    /// Override the canned response for `chat_info`.
    pub fn set_chat_info_result(&self, method: &'static str, info: Option<ChatInfo>) {
        self.state.lock().canned_chat_info.insert(method, info);
    }

    /// Override the canned response for `download_media`.
    pub fn set_download_media_result(&self, method: &'static str, bytes: Vec<u8>) {
        self.state.lock().canned_download.insert(method, bytes);
    }

    /// Inject an error for a unit-result (`()`) method.
    pub fn set_unit_err(&self, method: &'static str, err: PlatformAdapterError) {
        self.state.lock().canned_unit_err.insert(method, err);
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl OctoWhatsAppAdapter for MockAdapter {
    // ── Group A: outbound media (file-based) — pair-result ──

    async fn send_image(
        &self,
        _to_jid: &str,
        _file_path: &Path,
        _caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        record_pair_call(
            &self.state,
            "send_image",
            CannedPairResult::Ok {
                id: "fake-img-msg-id".into(),
                token: "fake-img-token".into(),
            },
        )
    }

    async fn send_video(
        &self,
        _to_jid: &str,
        _file_path: &Path,
        _caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        record_pair_call(
            &self.state,
            "send_video",
            CannedPairResult::Ok {
                id: "fake-vid-msg-id".into(),
                token: "fake-vid-token".into(),
            },
        )
    }

    async fn send_audio(
        &self,
        _to_jid: &str,
        _file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        record_pair_call(
            &self.state,
            "send_audio",
            CannedPairResult::Ok {
                id: "fake-aud-msg-id".into(),
                token: "fake-aud-token".into(),
            },
        )
    }

    async fn send_voice(
        &self,
        _to_jid: &str,
        _file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        record_pair_call(
            &self.state,
            "send_voice",
            CannedPairResult::Ok {
                id: "fake-voice-msg-id".into(),
                token: "fake-voice-token".into(),
            },
        )
    }

    async fn send_sticker(
        &self,
        _to_jid: &str,
        _file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        record_pair_call(
            &self.state,
            "send_sticker",
            CannedPairResult::Ok {
                id: "fake-stk-msg-id".into(),
                token: "fake-stk-token".into(),
            },
        )
    }

    // ── Group B: outbound non-media — single-result ──

    async fn send_reaction(
        &self,
        _to_jid: &str,
        _msg_id: &str,
        _emoji: &str,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(&self.state, "send_reaction", Ok("fake-rxn-msg-id".into()))
    }

    async fn send_poll(
        &self,
        _to_jid: &str,
        _question: &str,
        _options: &[String],
        _multi: bool,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(&self.state, "send_poll", Ok("fake-poll-msg-id".into()))
    }

    async fn send_contact(
        &self,
        _to_jid: &str,
        _vcard_path: &Path,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(
            &self.state,
            "send_contact",
            Ok("fake-contact-msg-id".into()),
        )
    }

    async fn send_location(
        &self,
        _to_jid: &str,
        _lat: f64,
        _lon: f64,
        _name: &str,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(&self.state, "send_location", Ok("fake-loc-msg-id".into()))
    }

    // ── Group C: message lifecycle — unit-result ──

    async fn edit_message(
        &self,
        _to_jid: &str,
        _msg_id: &str,
        _new_text: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "edit_message")
    }

    async fn delete_message(
        &self,
        _to_jid: &str,
        _msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "delete_message")
    }

    async fn mark_read(
        &self,
        _peer_jid: &str,
        _up_to_msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "mark_read")
    }

    // ── Group D: search + chat metadata — collection/option ──

    async fn message_search(
        &self,
        _query: &str,
        _peer_jid: Option<&str>,
    ) -> Result<Vec<MessageHit>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("message_search").or_insert(0) += 1;
        Ok(s.canned_search.remove("message_search").unwrap_or_default())
    }

    async fn chat_info(&self, _jid: &str) -> Result<Option<ChatInfo>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("chat_info").or_insert(0) += 1;
        Ok(s.canned_chat_info.remove("chat_info").unwrap_or(None))
    }

    // ── Group E: chat ops — unit-result ──

    async fn set_chat_pinned(&self, _jid: &str, _pinned: bool) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_chat_pinned")
    }

    async fn set_chat_muted(
        &self,
        _jid: &str,
        _until_epoch_secs: i64,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_chat_muted")
    }

    async fn set_chat_archived(
        &self,
        _jid: &str,
        _archived: bool,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_chat_archived")
    }

    async fn delete_chat(&self, _jid: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "delete_chat")
    }

    // ── Group F: presence — unit-result ──

    async fn send_typing(&self, _jid: &str, _is_typing: bool) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "send_typing")
    }

    // ── Group G: size-gated wrappers (delegate to unchecked) ──

    async fn send_image_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        _max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_image(to_jid, file_path, caption).await
    }

    async fn send_video_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        _max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_video(to_jid, file_path, caption).await
    }

    async fn send_audio_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        _max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_audio(to_jid, file_path).await
    }

    async fn send_voice_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        _max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_voice(to_jid, file_path).await
    }

    async fn send_sticker_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        _max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_sticker(to_jid, file_path).await
    }

    async fn send_reaction_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
        _max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        self.send_reaction(to_jid, msg_id, emoji).await
    }

    async fn send_poll_checked(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        multi: bool,
        _max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        self.send_poll(to_jid, question, options, multi).await
    }

    async fn send_contact_checked(
        &self,
        to_jid: &str,
        vcard_path: &Path,
        _max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        self.send_contact(to_jid, vcard_path).await
    }

    async fn send_location_checked(
        &self,
        to_jid: &str,
        lat: f64,
        lon: f64,
        name: &str,
        _max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        self.send_location(to_jid, lat, lon, name).await
    }

    async fn edit_message_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
        _max_bytes: usize,
    ) -> Result<(), PlatformAdapterError> {
        self.edit_message(to_jid, msg_id, new_text).await
    }

    // ── Non-async capabilities / download ──

    fn capabilities(&self) -> CapabilityReport {
        // Canned `CapabilityReport` with `media_capabilities` populated so
        // the `capabilities.rs` handler covers the inner `.map()` branch.
        // The real `WhatsAppWebAdapter` impl returns richer defaults; the
        // mock only needs the shape correct enough for handler coverage.
        CapabilityReport {
            max_payload_bytes: 65_536,
            media_capabilities: Some(MediaCapabilities {
                max_upload_bytes: 100 * 1024 * 1024,
                supported_mime_types: vec![
                    "image/jpeg".into(),
                    "image/png".into(),
                    "video/mp4".into(),
                    "audio/ogg".into(),
                    "audio/mpeg".into(),
                ],
            }),
            ..CapabilityReport::default()
        }
    }

    async fn download_media(
        &self,
        _media_ref_token: &str,
    ) -> Result<Vec<u8>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("download_media").or_insert(0) += 1;
        // Default: empty bytes. Tests can override via
        // `set_download_media_result`.
        Ok(s.canned_download
            .remove("download_media")
            .unwrap_or_default())
    }
}

// === Internal helpers ===

fn record_pair_call(
    state: &Arc<Mutex<MockState>>,
    method: &'static str,
    default: CannedPairResult,
) -> Result<(String, String), PlatformAdapterError> {
    let mut s = state.lock();
    *s.call_counts.entry(method).or_insert(0) += 1;
    match s.canned_pair.remove(method).unwrap_or(default) {
        CannedPairResult::Ok { id, token } => Ok((id, token)),
        CannedPairResult::Err(e) => Err(e),
    }
}

fn record_single_call(
    state: &Arc<Mutex<MockState>>,
    method: &'static str,
    default: CannedSingleResult,
) -> Result<String, PlatformAdapterError> {
    let mut s = state.lock();
    *s.call_counts.entry(method).or_insert(0) += 1;
    s.canned_single.remove(method).unwrap_or(default)
}

fn record_unit_call(
    state: &Arc<Mutex<MockState>>,
    method: &'static str,
) -> Result<(), PlatformAdapterError> {
    let mut s = state.lock();
    *s.call_counts.entry(method).or_insert(0) += 1;
    // Allow per-method override of unit-result methods via
    // `set_unit_err`.
    if let Some(e) = s.canned_unit_err.remove(method) {
        return Err(e);
    }
    Ok(())
}

// ===========================================================================
// Unit tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn mock_records_call_counts() {
        let m = MockAdapter::new();
        let _ = m.send_image("jid", Path::new("/tmp/x"), None).await;
        let _ = m.send_image("jid", Path::new("/tmp/x"), None).await;
        assert_eq!(m.call_count("send_image"), 2);
        assert_eq!(m.call_count("send_video"), 0);
    }

    #[tokio::test]
    async fn mock_default_pair_returns_canned_ids() {
        let m = MockAdapter::new();
        let (id, token) = m
            .send_image("jid", Path::new("/tmp/x"), None)
            .await
            .unwrap();
        assert_eq!(id, "fake-img-msg-id");
        assert_eq!(token, "fake-img-token");
    }

    #[tokio::test]
    async fn mock_default_single_returns_canned_id() {
        let m = MockAdapter::new();
        let id = m.send_reaction("jid", "msg", "👍").await.unwrap();
        assert_eq!(id, "fake-rxn-msg-id");
    }

    #[tokio::test]
    async fn mock_default_unit_returns_ok() {
        let m = MockAdapter::new();
        m.delete_message("jid", "msg").await.unwrap();
        assert_eq!(m.call_count("delete_message"), 1);
    }

    #[tokio::test]
    async fn mock_override_single_returns_err() {
        let m = MockAdapter::new();
        m.set_return(
            "send_reaction",
            Err(PlatformAdapterError::Unreachable {
                platform: "mock".into(),
                reason: "test override".into(),
            }),
        );
        let r = m.send_reaction("jid", "msg", "👍").await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn mock_override_pair_returns_err() {
        let m = MockAdapter::new();
        m.set_pair_return(
            "send_image",
            CannedPairResult::Err(PlatformAdapterError::PayloadTooLarge {
                platform: "mock".into(),
                size: 99,
                max: 16,
            }),
        );
        let r = m.send_image("jid", Path::new("/tmp/x"), None).await;
        assert!(matches!(
            r,
            Err(PlatformAdapterError::PayloadTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn mock_override_pair_returns_ok() {
        let m = MockAdapter::new();
        m.set_pair_return(
            "send_video",
            CannedPairResult::Ok {
                id: "custom-id".into(),
                token: "custom-token".into(),
            },
        );
        let (id, token) = m
            .send_video("jid", Path::new("/tmp/x"), None)
            .await
            .unwrap();
        assert_eq!(id, "custom-id");
        assert_eq!(token, "custom-token");
    }

    #[tokio::test]
    async fn mock_unit_err_override() {
        let m = MockAdapter::new();
        m.set_unit_err(
            "delete_message",
            PlatformAdapterError::Unreachable {
                platform: "mock".into(),
                reason: "unit override".into(),
            },
        );
        let r = m.delete_message("jid", "msg").await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
    }

    #[tokio::test]
    async fn mock_message_search_default_empty() {
        let m = MockAdapter::new();
        let r = m.message_search("query", None).await.unwrap();
        assert!(r.is_empty());
        assert_eq!(m.call_count("message_search"), 1);
    }

    #[tokio::test]
    async fn mock_message_search_with_override() {
        let m = MockAdapter::new();
        m.set_message_search_result(
            "message_search",
            vec![MessageHit {
                msg_id: "msg-1".into(),
                peer: "jid".into(),
                ts: 123,
                snippet: "hello".into(),
            }],
        );
        let r = m.message_search("query", None).await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].snippet, "hello");
    }

    #[tokio::test]
    async fn mock_chat_info_default_none() {
        let m = MockAdapter::new();
        let r = m.chat_info("jid").await.unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn mock_chat_info_with_override() {
        let m = MockAdapter::new();
        m.set_chat_info_result(
            "chat_info",
            Some(ChatInfo {
                jid: "jid".into(),
                kind: "dm".into(),
                name: Some("Alice".into()),
                last_activity_ts: 1_700_000_000,
            }),
        );
        let r = m.chat_info("jid").await.unwrap().unwrap();
        assert_eq!(r.kind, "dm");
        assert_eq!(r.name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    async fn mock_capabilities_includes_media() {
        let m = MockAdapter::new();
        let r = m.capabilities();
        assert_eq!(r.max_payload_bytes, 65_536);
        assert!(r.media_capabilities.is_some());
        let media = r.media_capabilities.unwrap();
        assert_eq!(media.max_upload_bytes, 100 * 1024 * 1024);
        assert!(!media.supported_mime_types.is_empty());
    }

    #[tokio::test]
    async fn mock_download_media_default_empty() {
        let m = MockAdapter::new();
        let bytes = m.download_media("tok").await.unwrap();
        assert!(bytes.is_empty());
        assert_eq!(m.call_count("download_media"), 1);
    }

    #[tokio::test]
    async fn mock_download_media_with_override() {
        let m = MockAdapter::new();
        m.set_download_media_result("download_media", vec![1, 2, 3, 4]);
        let bytes = m.download_media("tok").await.unwrap();
        assert_eq!(bytes, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn mock_checked_wrappers_delegate_to_unchecked() {
        let m = MockAdapter::new();
        let _ = m
            .send_image_checked("jid", Path::new("/tmp/x"), None, 1024)
            .await
            .unwrap();
        let _ = m
            .send_video_checked("jid", Path::new("/tmp/x"), None, 1024)
            .await
            .unwrap();
        let _ = m
            .send_audio_checked("jid", Path::new("/tmp/x"), 1024)
            .await
            .unwrap();
        let _ = m
            .send_voice_checked("jid", Path::new("/tmp/x"), 1024)
            .await
            .unwrap();
        let _ = m
            .send_sticker_checked("jid", Path::new("/tmp/x"), 1024)
            .await
            .unwrap();
        let _ = m
            .send_reaction_checked("jid", "msg", "👍", 1024)
            .await
            .unwrap();
        let _ = m
            .send_poll_checked("jid", "q", &[], false, 1024)
            .await
            .unwrap();
        let _ = m
            .send_contact_checked("jid", Path::new("/tmp/x"), 1024)
            .await
            .unwrap();
        let _ = m
            .send_location_checked("jid", 0.0, 0.0, "n", 1024)
            .await
            .unwrap();
        let _ = m.edit_message_checked("jid", "msg", "t", 1024).await;

        assert_eq!(m.call_count("send_image"), 1);
        assert_eq!(m.call_count("send_video"), 1);
        assert_eq!(m.call_count("send_audio"), 1);
        assert_eq!(m.call_count("send_voice"), 1);
        assert_eq!(m.call_count("send_sticker"), 1);
        assert_eq!(m.call_count("send_reaction"), 1);
        assert_eq!(m.call_count("send_poll"), 1);
        assert_eq!(m.call_count("send_contact"), 1);
        assert_eq!(m.call_count("send_location"), 1);
        assert_eq!(m.call_count("edit_message"), 1);
    }

    #[tokio::test]
    async fn mock_lifecycle_methods_record_counts() {
        let m = MockAdapter::new();
        m.mark_read("jid", "msg").await.unwrap();
        m.set_chat_pinned("jid", true).await.unwrap();
        m.set_chat_muted("jid", 0).await.unwrap();
        m.set_chat_archived("jid", false).await.unwrap();
        m.delete_chat("jid").await.unwrap();
        m.send_typing("jid", true).await.unwrap();

        assert_eq!(m.call_count("mark_read"), 1);
        assert_eq!(m.call_count("set_chat_pinned"), 1);
        assert_eq!(m.call_count("set_chat_muted"), 1);
        assert_eq!(m.call_count("set_chat_archived"), 1);
        assert_eq!(m.call_count("delete_chat"), 1);
        assert_eq!(m.call_count("send_typing"), 1);
    }
}
