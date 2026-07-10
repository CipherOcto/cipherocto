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
use octo_network::dot::adapters::coordinator_admin::{
    AddMemberOutput, AdminCapabilityReport, CoordinatorAdmin, GroupHandle, GroupId,
    GroupMemberSpec, GroupMetadata, GroupModeFlags, GroupProfilePictureSnapshot, InviteRef, PeerId,
    SetGroupProfilePictureResponse,
};
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
    /// Inner `CoordinatorAdmin` mock (Phase 6.12) — exposed via
    /// `as_coordinator_admin` so hermetic tests can exercise the
    /// membership / mode / admin RPC surface without a live WhatsApp
    /// session.
    pub coord_admin: MockCoordinatorAdmin,
}

impl MockAdapter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::default())),
            coord_admin: MockCoordinatorAdmin::new(),
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

    async fn send_text(
        &self,
        _to_jid: &str,
        _text: &str,
        _reply_to: Option<&str>,
        _mentions: &[String],
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(&self.state, "send_text", Ok("fake-text-msg-id".into()))
    }

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
        _is_quiz: bool,
        _correct_option_index: Option<usize>,
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

    async fn pin_message(
        &self,
        _peer_jid: &str,
        _msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "pin_message")
    }

    async fn unpin_message(
        &self,
        _peer_jid: &str,
        _msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "unpin_message")
    }

    async fn forward_message(
        &self,
        _peer_jid: &str,
        _original_msg_id: &str,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(&self.state, "forward_message", Ok("fake-fwd-msg-id".into()))
    }

    async fn edit_message_encrypted(
        &self,
        _peer_jid: &str,
        _msg_id: &str,
        _message_secret_b64: &str,
        _new_text: &str,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(
            &self.state,
            "edit_message_encrypted",
            Ok("fake-encrypted-edit-msg-id".into()),
        )
    }

    async fn fetch_sticker_pack(
        &self,
        _pack_id: &str,
        _locale: &str,
    ) -> Result<octo_adapter_whatsapp::StickerPackSnapshot, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("fetch_sticker_pack").or_insert(0) += 1;
        Ok(octo_adapter_whatsapp::StickerPackSnapshot {
            sticker_pack_id: Some("fake-pack-id".into()),
            name: Some("Fake Pack".into()),
            publisher: Some("Fake Publisher".into()),
            description: None,
            file_size: None,
            image_data_hash: None,
            stickers: Vec::new(),
            animated: 0,
            lottie: 0,
            preview_image_ids: Vec::new(),
            tray_image_id: None,
            tray_image_preview: None,
        })
    }

    async fn vote_poll(
        &self,
        _peer_jid: &str,
        _poll_msg_id: &str,
        _poll_creator_jid: &str,
        _message_secret_b64: &str,
        _selected_options: &[String],
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(&self.state, "vote_poll", Ok("fake-poll-vote-msg-id".into()))
    }

    async fn aggregate_poll_votes(
        &self,
        poll_options: &[String],
        _votes: &[(String, Vec<u8>, Vec<u8>)],
        _message_secret_b64: &str,
        _poll_msg_id: &str,
        _poll_creator_jid: &str,
    ) -> Result<Vec<octo_adapter_whatsapp::PollOptionResultSnapshot>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("aggregate_poll_votes").or_insert(0) += 1;
        Ok(poll_options
            .iter()
            .map(|name| octo_adapter_whatsapp::PollOptionResultSnapshot {
                name: name.clone(),
                voters: Vec::new(),
            })
            .collect())
    }

    async fn respond_event(
        &self,
        _peer_jid: &str,
        _event_msg_id: &str,
        _event_creator_jid: &str,
        _message_secret_b64: &str,
        _response: octo_adapter_whatsapp::waproto::whatsapp::message::event_response_message::EventResponseType,
        _extra_guest_count: Option<i32>,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(
            &self.state,
            "respond_event",
            Ok("fake-event-respond-msg-id".into()),
        )
    }

    // ── Tier 7.C: WA status / broadcast story ──────────────────

    async fn send_status_text(
        &self,
        _text: &str,
        _background_argb: u32,
        _font: &str,
        _privacy: &str,
        _recipients: &[String],
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(
            &self.state,
            "send_status_text",
            Ok("fake-status-text-msg-id".into()),
        )
    }

    async fn send_status_image(
        &self,
        _file_path: &Path,
        _caption: Option<&str>,
        _thumbnail_b64: Option<&str>,
        _privacy: &str,
        _recipients: &[String],
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(
            &self.state,
            "send_status_image",
            Ok("fake-status-image-msg-id".into()),
        )
    }

    async fn send_status_video(
        &self,
        _file_path: &Path,
        _caption: Option<&str>,
        _thumbnail_b64: Option<&str>,
        _duration_seconds: u32,
        _privacy: &str,
        _recipients: &[String],
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(
            &self.state,
            "send_status_video",
            Ok("fake-status-video-msg-id".into()),
        )
    }

    async fn revoke_status(
        &self,
        _message_id: &str,
        _privacy: &str,
        _recipients: &[String],
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(
            &self.state,
            "revoke_status",
            Ok("fake-status-revoke-msg-id".into()),
        )
    }

    // ── Tier 7.D: profile pictures + business profile + runtime config ──

    async fn set_profile_picture(&self, _image_data_b64: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_profile_picture")
    }

    async fn remove_profile_picture(&self) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "remove_profile_picture")
    }

    async fn get_business_profile(
        &self,
        _jid: &str,
    ) -> Result<Option<octo_adapter_whatsapp::BusinessProfile>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("get_business_profile").or_insert(0) += 1;
        Ok(Some(octo_adapter_whatsapp::BusinessProfile::default()))
    }

    async fn set_client_profile(
        &self,
        _platform: &str,
        _os_version: Option<&str>,
        _manufacturer: Option<&str>,
        _locale_language: Option<&str>,
        _locale_country: Option<&str>,
        _passive_login: Option<bool>,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_client_profile")
    }

    async fn set_passive(&self, _passive: bool) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_passive")
    }

    async fn set_force_active_delivery_receipts(
        &self,
        _active: bool,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_force_active_delivery_receipts")
    }

    // ── Tier 7.E: newsletter + TcToken ────────────────────────────

    async fn create_newsletter(
        &self,
        _name: &str,
        _description: Option<&str>,
    ) -> Result<octo_adapter_whatsapp::NewsletterMetadataSnapshot, PlatformAdapterError> {
        use octo_adapter_whatsapp::NewsletterMetadataSnapshot;
        let mut s = self.state.lock();
        *s.call_counts.entry("create_newsletter").or_insert(0) += 1;
        Ok(NewsletterMetadataSnapshot {
            jid: "1234567890@newsletter".into(),
            name: "Fake Newsletter".into(),
            description: None,
            subscriber_count: 0,
            state: "active".into(),
            picture_url: None,
            preview_url: None,
            invite_code: None,
            role: None,
            creation_time: None,
        })
    }

    async fn join_newsletter(
        &self,
        jid: &str,
    ) -> Result<octo_adapter_whatsapp::NewsletterMetadataSnapshot, PlatformAdapterError> {
        use octo_adapter_whatsapp::NewsletterMetadataSnapshot;
        let mut s = self.state.lock();
        *s.call_counts.entry("join_newsletter").or_insert(0) += 1;
        Ok(NewsletterMetadataSnapshot {
            jid: jid.into(),
            name: "Fake Newsletter".into(),
            description: None,
            subscriber_count: 0,
            state: "active".into(),
            picture_url: None,
            preview_url: None,
            invite_code: None,
            role: None,
            creation_time: None,
        })
    }

    async fn newsletter_send_reaction(
        &self,
        _jid: &str,
        _server_id: u64,
        _reaction: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "newsletter_send_reaction")
    }

    async fn newsletter_edit_message(
        &self,
        _jid: &str,
        _message_id: &str,
        _new_text: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "newsletter_edit_message")
    }

    async fn newsletter_revoke_message(
        &self,
        _jid: &str,
        _message_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "newsletter_revoke_message")
    }

    async fn issue_tc_tokens(
        &self,
        _jids: &[String],
    ) -> Result<Vec<octo_adapter_whatsapp::ReceivedTcTokenSnapshot>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("issue_tc_tokens").or_insert(0) += 1;
        Ok(Vec::new())
    }

    async fn get_tc_token(
        &self,
        _jid: &str,
    ) -> Result<Option<octo_adapter_whatsapp::TcTokenEntryValue>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("get_tc_token").or_insert(0) += 1;
        Ok(None)
    }

    async fn prune_expired_tc_tokens(&self) -> Result<u32, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("prune_expired_tc_tokens").or_insert(0) += 1;
        Ok(0)
    }

    async fn get_all_tc_token_jids(&self) -> Result<Vec<String>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("get_all_tc_token_jids").or_insert(0) += 1;
        Ok(Vec::new())
    }

    // ── Tier 7.F: passkey (response + confirmation) only ────────────

    async fn send_passkey_response(
        &self,
        _assertion_json_b64: &str,
        _credential_id_b64: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "send_passkey_response")
    }

    async fn send_passkey_confirmation(&self) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "send_passkey_confirmation")
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

    // ── Tier 4: contact + presence — unit-result ─────────────────────

    async fn is_on_whatsapp(&self, _jid: &str) -> Result<bool, PlatformAdapterError> {
        record_unit_call(&self.state, "is_on_whatsapp")?;
        Ok(true)
    }
    async fn get_profile_picture_url(
        &self,
        _jid: &str,
        _preview: bool,
    ) -> Result<Option<String>, PlatformAdapterError> {
        record_unit_call(&self.state, "get_profile_picture_url")?;
        Ok(Some("https://example.invalid/p.jpg".into()))
    }
    async fn block_contact(&self, _jid: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "block_contact")
    }
    async fn unblock_contact(&self, _jid: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "unblock_contact")
    }
    async fn subscribe_presence(&self, _jid: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "subscribe_presence")
    }
    async fn unsubscribe_presence(&self, _jid: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "unsubscribe_presence")
    }
    async fn set_presence_available(&self) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_presence_available")
    }
    async fn set_presence_unavailable(&self) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_presence_unavailable")
    }

    // ── Tier 6: profile + contact-enrichment — unit-result ─────────

    async fn set_push_name(&self, _name: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_push_name")
    }
    async fn set_status_text(&self, _text: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_status_text")
    }
    async fn get_user_info(
        &self,
        _jid: &str,
    ) -> Result<Option<octo_adapter_whatsapp::UserInfoSnapshot>, PlatformAdapterError> {
        use octo_adapter_whatsapp::UserInfoSnapshot;
        record_unit_call(&self.state, "get_user_info")?;
        Ok(Some(UserInfoSnapshot {
            jid: _jid.to_string(),
            lid: None,
            status: Some("mock status".into()),
            picture_id: None,
            is_business: false,
            verified_name: None,
            devices: vec![0],
        }))
    }

    // ── Tier 6.1: privacy + blocklist queries — unit-result ────────

    async fn fetch_privacy_settings(
        &self,
    ) -> Result<Vec<octo_adapter_whatsapp::PrivacySettingSnapshot>, PlatformAdapterError> {
        use octo_adapter_whatsapp::PrivacySettingSnapshot;
        record_unit_call(&self.state, "fetch_privacy_settings")?;
        Ok(vec![
            PrivacySettingSnapshot {
                category: "last".into(),
                value: "all".into(),
            },
            PrivacySettingSnapshot {
                category: "profile".into(),
                value: "contacts".into(),
            },
            PrivacySettingSnapshot {
                category: "readreceipts".into(),
                value: "all".into(),
            },
        ])
    }
    async fn set_privacy_setting(
        &self,
        _category: &str,
        _value: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "set_privacy_setting")
    }
    async fn get_blocklist(&self) -> Result<Vec<String>, PlatformAdapterError> {
        record_unit_call(&self.state, "get_blocklist")?;
        Ok(vec!["mock-blocked@s.whatsapp.net".to_string()])
    }
    async fn is_blocked(&self, _jid: &str) -> Result<bool, PlatformAdapterError> {
        record_unit_call(&self.state, "is_blocked")?;
        Ok(false)
    }

    // ── Tier 6.2: labels + star — unit-result / string-id ─────────

    async fn create_label(
        &self,
        _label_id: &str,
        _name: &str,
        _color: i32,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "create_label")
    }
    async fn delete_label(&self, _label_id: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "delete_label")
    }
    async fn add_chat_label(
        &self,
        _label_id: &str,
        _chat_jid: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "add_chat_label")
    }
    async fn remove_chat_label(
        &self,
        _label_id: &str,
        _chat_jid: &str,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "remove_chat_label")
    }
    async fn star_message(
        &self,
        _peer: &str,
        _msg_id: &str,
        _from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "star_message")
    }
    async fn unstar_message(
        &self,
        _peer: &str,
        _msg_id: &str,
        _from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "unstar_message")
    }

    // ── Tier 6.3: mark_as_played / clear_chat / delete_for_me / save_contact ─

    async fn mark_as_played(
        &self,
        _chat: &str,
        _msg_ids: &[String],
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "mark_as_played")
    }
    async fn clear_chat(
        &self,
        _jid: &str,
        _delete_starred: bool,
        _delete_media: bool,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "clear_chat")
    }
    async fn delete_message_for_me(
        &self,
        _chat: &str,
        _msg_id: &str,
        _from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "delete_message_for_me")
    }
    async fn save_contact(&self, _jid: &str, _full_name: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "save_contact")
    }

    // ── Tier 6.4: identity (Option<String> / bool returns) ───────

    async fn get_pn(&self) -> Result<Option<String>, PlatformAdapterError> {
        record_unit_call(&self.state, "get_pn")?;
        Ok(Some("15551234567@s.whatsapp.net".to_string()))
    }
    async fn get_lid(&self) -> Result<Option<String>, PlatformAdapterError> {
        record_unit_call(&self.state, "get_lid")?;
        Ok(Some("100000000000001@lid".to_string()))
    }
    async fn is_lid_migrated(&self) -> Result<bool, PlatformAdapterError> {
        record_unit_call(&self.state, "is_lid_migrated")?;
        Ok(true)
    }

    // ── Tier 6.5: newsletter + events ────────────────────────────

    async fn list_subscribed_newsletters(
        &self,
    ) -> Result<Vec<octo_adapter_whatsapp::NewsletterMetadataSnapshot>, PlatformAdapterError> {
        use octo_adapter_whatsapp::NewsletterMetadataSnapshot;
        record_unit_call(&self.state, "list_subscribed_newsletters")?;
        Ok(vec![NewsletterMetadataSnapshot {
            jid: "100000000000001@newsletter".to_string(),
            name: "mock-newsletter".to_string(),
            description: None,
            subscriber_count: 1,
            state: "Active".to_string(),
            picture_url: None,
            preview_url: None,
            invite_code: Some("ABCD1234".to_string()),
            role: Some("Subscriber".to_string()),
            creation_time: None,
        }])
    }
    async fn get_newsletter_metadata(
        &self,
        _jid: &str,
    ) -> Result<octo_adapter_whatsapp::NewsletterMetadataSnapshot, PlatformAdapterError> {
        use octo_adapter_whatsapp::NewsletterMetadataSnapshot;
        record_unit_call(&self.state, "get_newsletter_metadata")?;
        Ok(NewsletterMetadataSnapshot {
            jid: _jid.to_string(),
            name: "mock-newsletter".to_string(),
            description: None,
            subscriber_count: 1,
            state: "Active".to_string(),
            picture_url: None,
            preview_url: None,
            invite_code: None,
            role: Some("Subscriber".to_string()),
            creation_time: None,
        })
    }
    async fn leave_newsletter(&self, _jid: &str) -> Result<(), PlatformAdapterError> {
        record_unit_call(&self.state, "leave_newsletter")
    }
    async fn create_event(
        &self,
        _to_jid: &str,
        _name: &str,
        _start_time_unix: i64,
        _description: Option<&str>,
    ) -> Result<String, PlatformAdapterError> {
        record_single_call(&self.state, "create_event", Ok("fake-event-msg-id".into()))
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
        is_quiz: bool,
        correct_option_index: Option<usize>,
        _max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        self.send_poll(
            to_jid,
            question,
            options,
            multi,
            is_quiz,
            correct_option_index,
        )
        .await
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

    // ── CoordinatorAdmin probe (Phase 6.12) ──────────────────────────────

    fn as_coordinator_admin(&self) -> Option<&dyn CoordinatorAdmin> {
        Some(&self.coord_admin)
    }
}

// ===========================================================================
// MockCoordinatorAdmin — Phase 6.12
// ===========================================================================
//
// In-memory `CoordinatorAdmin` paired with `MockAdapter` so hermetic
// tests can exercise the membership / mode / admin handler surface
// without a live WhatsApp session. Mirrors `MockState` for the
// adapter: per-method counters + canned response maps.

/// Inner state for `MockCoordinatorAdmin`.
#[derive(Debug, Default)]
struct MockCoordState {
    /// Method name (static str) -> number of calls.
    call_counts: HashMap<&'static str, usize>,
    /// Per-method response override for `GroupHandle`-returning methods.
    canned_handles: HashMap<&'static str, GroupHandle>,
    /// Per-id canned metadata returned by `get_group_metadata`.
    canned_metadata: HashMap<String, GroupMetadata>,
    /// Per-method response override for unit-result (`()`) methods.
    /// Consumed on next call (single-shot).
    canned_unit_err: HashMap<&'static str, PlatformAdapterError>,
}

/// Mock `CoordinatorAdmin` — in-memory, `Send + Sync`, `Arc`-shareable.
#[derive(Debug, Clone)]
pub struct MockCoordinatorAdmin {
    state: Arc<Mutex<MockCoordState>>,
}

impl MockCoordinatorAdmin {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockCoordState::default())),
        }
    }

    /// Returns the number of times `method` has been invoked.
    pub fn call_count(&self, method: &'static str) -> usize {
        self.state
            .lock()
            .call_counts
            .get(method)
            .copied()
            .unwrap_or(0)
    }

    /// Pre-seed an error to be returned on the NEXT call to `method`.
    /// Subsequent calls return `Ok(())` again unless re-seeded.
    pub fn set_canned_err(&self, method: &'static str, e: PlatformAdapterError) {
        self.state.lock().canned_unit_err.insert(method, e);
    }

    /// Pre-seed a `GroupHandle` returned by `create_group` (and similar).
    pub fn set_canned_handle(&self, method: &'static str, h: GroupHandle) {
        self.state.lock().canned_handles.insert(method, h);
    }

    /// Pre-seed a `GroupMetadata` returned by `get_group_metadata`
    /// when the id matches `id`.
    pub fn set_canned_metadata(&self, id: &str, m: GroupMetadata) {
        self.state.lock().canned_metadata.insert(id.to_string(), m);
    }
}

impl Default for MockCoordinatorAdmin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CoordinatorAdmin for MockCoordinatorAdmin {
    fn admin_capabilities(&self) -> AdminCapabilityReport {
        AdminCapabilityReport {
            can_create: true,
            can_join_by_id: true,
            can_join_by_invite: true,
            can_leave: true,
            can_destroy: true,
            can_add_member: true,
            can_remove_member: true,
            can_ban: true,
            can_promote: true,
            can_demote: true,
            can_approve_join: true,
            can_rename: true,
            can_describe: true,
            can_lock: true,
            can_announce: true,
            can_set_ephemeral: true,
            can_require_approval: true,
            can_list_own_groups: true,
            can_get_metadata: true,
            can_resolve_invite: true,
            can_transfer_ownership: true,
            can_get_invite_link: true,
            can_update_member_label: true,
            can_get_profile_pictures: true,
            can_set_profile_picture: true,
            can_remove_profile_picture: true,
        }
    }

    async fn create_group(
        &self,
        subject: &str,
        members: &[GroupMemberSpec],
    ) -> Result<GroupHandle, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("create_group").or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("create_group") {
            return Err(e);
        }
        if let Some(h) = s.canned_handles.get("create_group").cloned() {
            return Ok(h);
        }
        Ok(GroupHandle {
            id: GroupId::new("mock-create@g.us"),
            subject: Some(subject.to_string()),
            invite_url: Some("https://chat.whatsapp.com/MOCK".into()),
            is_admin: true,
            member_count: Some(members.len() as u32 + 1),
            mode_flags: None,
            initial_admins_promoted: true,
        })
    }

    async fn leave_group(&self, _id: &GroupId) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("leave_group")
    }
    async fn destroy_group(&self, _id: &GroupId) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("destroy_group")
    }

    async fn add_member(
        &self,
        _id: &GroupId,
        _m: &GroupMemberSpec,
    ) -> Result<AddMemberOutput, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("add_member").or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("add_member") {
            return Err(e);
        }
        Ok(AddMemberOutput {
            added: true,
            promoted: None,
        })
    }

    async fn remove_member(&self, _id: &GroupId, _p: &PeerId) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("remove_member")
    }
    async fn ban_member(
        &self,
        _id: &GroupId,
        _p: &PeerId,
        _d: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("ban_member")
    }
    async fn promote_to_admin(
        &self,
        _id: &GroupId,
        _p: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("promote_to_admin")
    }
    async fn demote_from_admin(
        &self,
        _id: &GroupId,
        _p: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("demote_from_admin")
    }
    async fn approve_join_request(
        &self,
        _id: &GroupId,
        _p: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("approve_join_request")
    }
    async fn rename_group(
        &self,
        _id: &GroupId,
        _subject: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("rename_group")
    }
    async fn set_group_description(
        &self,
        _id: &GroupId,
        _desc: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("set_group_description")
    }
    async fn set_locked(&self, _id: &GroupId, _locked: bool) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("set_locked")
    }
    async fn set_announce(
        &self,
        _id: &GroupId,
        _announce: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("set_announce")
    }
    async fn set_ephemeral(
        &self,
        _id: &GroupId,
        _ttl: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("set_ephemeral")
    }
    async fn set_require_approval(
        &self,
        _id: &GroupId,
        _require: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("set_require_approval")
    }
    async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        self.record_unit_handle_vec("list_own_groups")
    }
    async fn list_own_groups_with_invites(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        self.record_unit_handle_vec("list_own_groups_with_invites")
    }
    async fn get_group_metadata(
        &self,
        id: &GroupId,
    ) -> Result<GroupMetadata, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("get_group_metadata").or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("get_group_metadata") {
            return Err(e);
        }
        if let Some(m) = s.canned_metadata.get(id.as_str()).cloned() {
            return Ok(m);
        }
        Ok(GroupMetadata {
            id: id.clone(),
            subject: Some("mock".into()),
            description: Some("mock description".into()),
            members: vec![PeerId::new("mock-member")],
            admins: vec![PeerId::new("mock-admin")],
            invite_url: Some("https://chat.whatsapp.com/MOCK".into()),
            mode_flags: GroupModeFlags::default(),
        })
    }
    async fn resolve_invite(&self, _inv: &InviteRef) -> Result<GroupHandle, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("resolve_invite").or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("resolve_invite") {
            return Err(e);
        }
        Ok(GroupHandle {
            id: GroupId::new("resolved@g.us"),
            subject: Some("resolved".into()),
            invite_url: None,
            is_admin: false,
            member_count: Some(42),
            mode_flags: None,
            initial_admins_promoted: false,
        })
    }
    async fn join_by_invite(&self, _inv: &InviteRef) -> Result<GroupHandle, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: "mock".into(),
            action: "join_by_invite".into(),
        })
    }
    async fn join_by_id(&self, _id: &GroupId) -> Result<GroupHandle, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: "mock".into(),
            action: "join_by_id".into(),
        })
    }
    async fn transfer_ownership(
        &self,
        _id: &GroupId,
        _p: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("transfer_ownership")
    }

    // ── Session 7.H: group gap list (invite link / member labels / profile pic) ──

    async fn get_invite_link(
        &self,
        _id: &GroupId,
        _reset: bool,
    ) -> Result<String, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("get_invite_link").or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("get_invite_link") {
            return Err(e);
        }
        Ok("https://chat.whatsapp.com/MOCKINV".into())
    }

    async fn update_member_label(
        &self,
        _id: &GroupId,
        _label: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.record_unit_call("update_member_label")
    }

    async fn get_profile_pictures(
        &self,
        ids: &[GroupId],
        _preview: bool,
    ) -> Result<Vec<GroupProfilePictureSnapshot>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry("get_profile_pictures").or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("get_profile_pictures") {
            return Err(e);
        }
        Ok(ids
            .iter()
            .map(|id| GroupProfilePictureSnapshot {
                group_jid: id.as_str().to_string(),
                url: Some("https://mock.example/pic".into()),
                direct_path: None,
                photo_id: Some("MOCKPIC".into()),
            })
            .collect())
    }

    async fn set_profile_picture(
        &self,
        _id: &GroupId,
        _image_data_b64: &str,
    ) -> Result<SetGroupProfilePictureResponse, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts
            .entry("set_group_profile_picture")
            .or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("set_group_profile_picture") {
            return Err(e);
        }
        Ok(SetGroupProfilePictureResponse {
            id: "MOCKPICID".into(),
        })
    }

    async fn remove_profile_picture(
        &self,
        _id: &GroupId,
    ) -> Result<SetGroupProfilePictureResponse, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts
            .entry("remove_group_profile_picture")
            .or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove("remove_group_profile_picture") {
            return Err(e);
        }
        Ok(SetGroupProfilePictureResponse { id: "0".into() })
    }

    fn platform_name(&self) -> String {
        "mock".into()
    }
}

impl MockCoordinatorAdmin {
    fn record_unit_call(&self, method: &'static str) -> Result<(), PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry(method).or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove(method) {
            return Err(e);
        }
        Ok(())
    }

    fn record_unit_handle_vec(
        &self,
        method: &'static str,
    ) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        let mut s = self.state.lock();
        *s.call_counts.entry(method).or_insert(0) += 1;
        if let Some(e) = s.canned_unit_err.remove(method) {
            return Err(e);
        }
        Ok(vec![GroupHandle {
            id: GroupId::new("mock-list@g.us"),
            subject: Some("mock".into()),
            invite_url: Some("https://chat.whatsapp.com/MOCK".into()),
            is_admin: true,
            member_count: Some(2),
            mode_flags: None,
            initial_admins_promoted: true,
        }])
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
            .send_poll_checked("jid", "q", &[], false, false, None, 1024)
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

    // ── Phase 6.12: CoordinatorAdmin probe ───────────────────────────────

    #[tokio::test]
    async fn mock_as_coordinator_admin_returns_some() {
        let m = MockAdapter::new();
        let coord = m.as_coordinator_admin();
        assert!(coord.is_some(), "MockAdapter must expose CoordinatorAdmin");
    }

    #[tokio::test]
    async fn mock_coord_admin_capabilities_all_true() {
        let m = MockAdapter::new();
        let coord = m.as_coordinator_admin().expect("some");
        let caps = coord.admin_capabilities();
        assert!(caps.can_create);
        assert!(caps.can_add_member);
        assert!(caps.can_remove_member);
        assert!(caps.can_ban);
        assert!(caps.can_promote);
        assert!(caps.can_demote);
        assert!(caps.can_rename);
        assert!(caps.can_lock);
        assert!(caps.can_announce);
        assert!(caps.can_set_ephemeral);
        assert!(caps.can_require_approval);
        assert!(caps.can_list_own_groups);
        assert!(caps.can_get_metadata);
        assert!(caps.can_resolve_invite);
        assert!(caps.can_join_by_id);
        assert!(caps.can_join_by_invite);
        assert!(caps.can_transfer_ownership);
    }

    #[tokio::test]
    async fn mock_coord_admin_unit_methods_record_and_override() {
        let m = MockAdapter::new();
        // Call through the trait-object surface (the public API), but
        // observe counts via the concrete `MockCoordinatorAdmin` field
        // (which owns the counter state).
        let coord_trait = m.as_coordinator_admin().expect("some");
        coord_trait
            .leave_group(&GroupId::new("g@g.us"))
            .await
            .unwrap();
        coord_trait
            .rename_group(&GroupId::new("g@g.us"), "new")
            .await
            .unwrap();
        assert_eq!(m.coord_admin.call_count("leave_group"), 1);
        assert_eq!(m.coord_admin.call_count("rename_group"), 1);

        m.coord_admin.set_canned_err(
            "rename_group",
            PlatformAdapterError::Unreachable {
                platform: "mock".into(),
                reason: "override".into(),
            },
        );
        let r = coord_trait.rename_group(&GroupId::new("g@g.us"), "x").await;
        assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
        // The single-shot error is consumed — next call returns Ok.
        let r2 = coord_trait.rename_group(&GroupId::new("g@g.us"), "x").await;
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn mock_coord_admin_add_member_default_ok() {
        let m = MockAdapter::new();
        let coord = m.as_coordinator_admin().expect("some");
        let spec = GroupMemberSpec::new("+15555550100");
        let out = coord
            .add_member(&GroupId::new("g@g.us"), &spec)
            .await
            .unwrap();
        assert!(out.added);
        assert!(out.promoted.is_none());
        assert_eq!(m.coord_admin.call_count("add_member"), 1);
    }

    #[tokio::test]
    async fn mock_coord_admin_create_group_default_handle() {
        let m = MockAdapter::new();
        let coord = m.as_coordinator_admin().expect("some");
        let h = coord.create_group("subj", &[]).await.unwrap();
        assert_eq!(h.subject.as_deref(), Some("subj"));
        assert!(h.is_admin);
        assert!(h.initial_admins_promoted);
        assert_eq!(m.coord_admin.call_count("create_group"), 1);
    }
}
