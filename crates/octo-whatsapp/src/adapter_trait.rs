//! `OctoWhatsAppAdapter` — the trait abstraction for the runtime layer.
//!
//! The trait surfaces the 18 Phase 2 inherent methods on
//! `WhatsAppWebAdapter` (plus 10 `_checked` size-gated wrappers) so that
//! the runtime daemon (`DaemonHandle::adapter()`) returns an
//! `Arc<dyn OctoWhatsAppAdapter>` and can swap in a `MockAdapter` for
//! tests without instantiating a live WhatsApp Web session.
//!
//! ## Why this lives in `octo-whatsapp` (not `octo-adapter-whatsapp`)
//!
//! The trait is consumed by the runtime layer (handlers, daemon,
//! integration tests) and produced by the adapter layer
//! (`WhatsAppWebAdapter`). Putting it in `octo-adapter-whatsapp` would
//! require the adapter crate to depend on `octo-whatsapp`, which would
//! invert the existing dependency graph
//! (`octo-whatsapp -> octo-adapter-whatsapp`) and create a cycle.
//!
//! ## Object-safety
//!
//! The trait uses `#[async_trait::async_trait]` so all `async fn`
//! methods satisfy object-safety (`Send + Sync` bounds + `&self`
//! receiver + no generics on the trait method itself). All 28 methods
//! dispatch through `&self` and only borrow `&Path` / `&str` / `&[String]`,
//! so the trait object `dyn OctoWhatsAppAdapter` can sit behind an `Arc`
//! in `DaemonInner`.
//!
//! ## Capabilities delegation
//!
//! `capabilities()` is non-async (matches `PlatformAdapter::capabilities`
//! from `octo-network`) so the runtime's `capabilities` RPC handler can
//! query the bound adapter without awaiting. The `WhatsAppWebAdapter`
//! impl forwards to its own `PlatformAdapter::capabilities` impl.

use std::path::Path;

use async_trait::async_trait;

use octo_adapter_whatsapp::{ChatInfo, MessageHit};
use octo_network::dot::adapters::CapabilityReport;
use octo_network::dot::error::PlatformAdapterError;

/// Runtime-facing adapter abstraction for the WhatsApp Web transport.
///
/// All 18 inbound Phase 2 methods from `octo-adapter-whatsapp` are
/// exposed here as unchecked + `_checked` variants. The
/// `WhatsAppWebAdapter` impl in `adapter_impl.rs` (in this crate)
/// delegates to the legacy inherent methods exposed by
/// `octo-adapter-whatsapp` (visibility widened to `pub(crate)` so the
/// delegation does not cross the crate boundary).
#[async_trait]
pub trait OctoWhatsAppAdapter: Send + Sync {
    // ── Group A: outbound media (file-based, Group A floor) ──

    /// Send an image with optional caption. Returns
    /// `(message_id, media_ref_token)`.
    async fn send_image(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Send a video with optional caption.
    async fn send_video(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Send an audio file.
    async fn send_audio(
        &self,
        to_jid: &str,
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Send a voice note.
    async fn send_voice(
        &self,
        to_jid: &str,
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Send a sticker.
    async fn send_sticker(
        &self,
        to_jid: &str,
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError>;

    // ── Group B: outbound non-media (text/payload, no file) ──

    /// React to a message with an emoji. Returns the new message id.
    async fn send_reaction(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<String, PlatformAdapterError>;

    /// Send a poll. Returns the new message id.
    async fn send_poll(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        multi: bool,
    ) -> Result<String, PlatformAdapterError>;

    /// Send a vCard contact file. Returns the new message id.
    async fn send_contact(
        &self,
        to_jid: &str,
        vcard_path: &Path,
    ) -> Result<String, PlatformAdapterError>;

    /// Send a location pin. Returns the new message id.
    async fn send_location(
        &self,
        to_jid: &str,
        lat: f64,
        lon: f64,
        name: &str,
    ) -> Result<String, PlatformAdapterError>;

    // ── Group C: message lifecycle ──

    /// Edit the text of a previously-sent message.
    async fn edit_message(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
    ) -> Result<(), PlatformAdapterError>;

    /// Delete a previously-sent message.
    async fn delete_message(&self, to_jid: &str, msg_id: &str) -> Result<(), PlatformAdapterError>;

    /// Mark all messages in a peer up to (and including) `up_to_msg_id`
    /// as read.
    async fn mark_read(
        &self,
        peer_jid: &str,
        up_to_msg_id: &str,
    ) -> Result<(), PlatformAdapterError>;

    // ── Group D: search + chat metadata ──

    /// Search messages matching `query`, optionally scoped to a peer.
    async fn message_search(
        &self,
        query: &str,
        peer_jid: Option<&str>,
    ) -> Result<Vec<MessageHit>, PlatformAdapterError>;

    /// Fetch metadata for the chat identified by `jid`. Returns `None`
    /// if the chat is unknown.
    async fn chat_info(&self, jid: &str) -> Result<Option<ChatInfo>, PlatformAdapterError>;

    // ── Group E: chat ops (mutations) ──

    /// Pin or unpin a chat.
    async fn set_chat_pinned(&self, jid: &str, pinned: bool) -> Result<(), PlatformAdapterError>;

    /// Mute a chat until `until_epoch_secs` (UNIX timestamp). Pass `0`
    /// to unmute.
    async fn set_chat_muted(
        &self,
        jid: &str,
        until_epoch_secs: i64,
    ) -> Result<(), PlatformAdapterError>;

    /// Archive or unarchive a chat.
    async fn set_chat_archived(
        &self,
        jid: &str,
        archived: bool,
    ) -> Result<(), PlatformAdapterError>;

    /// Delete a chat entirely from this device.
    async fn delete_chat(&self, jid: &str) -> Result<(), PlatformAdapterError>;

    // ── Group F: presence ──

    /// Set the typing indicator (composing / paused) on a peer.
    async fn send_typing(&self, jid: &str, is_typing: bool) -> Result<(), PlatformAdapterError>;

    // ── Group G: size-gated wrappers (size ceiling first, then unchecked) ──

    /// Size-gated wrapper for `send_image`. Rejects when the file
    /// exceeds `max_bytes`; otherwise delegates to `send_image`.
    async fn send_image_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Size-gated wrapper for `send_video`.
    async fn send_video_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Size-gated wrapper for `send_audio`.
    async fn send_audio_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Size-gated wrapper for `send_voice`.
    async fn send_voice_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Size-gated wrapper for `send_sticker`.
    async fn send_sticker_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError>;

    /// Size-gated wrapper for `send_reaction`.
    async fn send_reaction_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError>;

    /// Size-gated wrapper for `send_poll`.
    async fn send_poll_checked(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        multi: bool,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError>;

    /// Size-gated wrapper for `send_contact`.
    async fn send_contact_checked(
        &self,
        to_jid: &str,
        vcard_path: &Path,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError>;

    /// Size-gated wrapper for `send_location`.
    async fn send_location_checked(
        &self,
        to_jid: &str,
        lat: f64,
        lon: f64,
        name: &str,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError>;

    /// Size-gated wrapper for `edit_message`.
    async fn edit_message_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
        max_bytes: usize,
    ) -> Result<(), PlatformAdapterError>;

    // ── Non-async (matches `PlatformAdapter::capabilities` shape) ──

    /// Static capability snapshot — same shape as
    /// `octo_network::PlatformAdapter::capabilities`. Non-async so the
    /// RPC handler can call it without `await`.
    fn capabilities(&self) -> CapabilityReport;

    /// Download a previously-uploaded media payload via a media-ref
    /// token (RFC-0850 §8.6).
    ///
    /// Lives here (not on the inner `*_inner` surface) because the
    /// `messages.download` RPC handler calls it through the trait
    /// object — adding it to `OctoWhatsAppAdapter` is the only way to
    /// keep the handler compile-error-free once the daemon stores its
    /// adapter as `dyn OctoWhatsAppAdapter`.
    async fn download_media(&self, media_ref_token: &str) -> Result<Vec<u8>, PlatformAdapterError>;
}

// ===========================================================================
// Trait impl for `WhatsAppWebAdapter`
// ===========================================================================
//
// **Why the impl lives in `octo-whatsapp` (not `octo-adapter-whatsapp`):**
//
// The trait is consumed by the runtime layer and produced by the adapter
// layer. Adding an `octo-whatsapp` dependency to `octo-adapter-whatsapp`
// would invert the existing dependency direction
// (`octo-whatsapp -> octo-adapter-whatsapp`) and create a cycle. The
// impl therefore lives in this crate, where both the trait and the
// `WhatsAppWebAdapter` type are visible.
//
// **Why the bodies delegate to `*_inner` helpers:**
//
// The actual implementations need access to `WhatsAppWebAdapter`'s
// `pub(crate)` fields (`self.client`, `self.store`) and to crate-internal
// helpers in `octo-adapter-whatsapp` (`upload_to_cdn`, `encode_base64url`,
// `MediaRef::from_upload_response`, `epoch_millis`). Those are not visible
// from this crate, so each unchecked method delegates to a `pub async fn
// *_inner(...)` helper living next to the type. The bodies in those
// helpers are the exact bodies that used to live as inherent methods on
// `WhatsAppWebAdapter`.
//
// **Delegating via `&dyn`:**
//
// The trait impl receives `&self` typed as `&WhatsAppWebAdapter`
// (concrete type). Calls to `self.send_image(...)` resolve to the
// inherent method on `WhatsAppWebAdapter` directly — there is no
// ambiguity because `*_inner` is inherent-only.

#[async_trait::async_trait]
impl OctoWhatsAppAdapter for octo_adapter_whatsapp::WhatsAppWebAdapter {
    // ── Unchecked: forward to `_inner` helpers ──

    async fn send_image(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_image(to_jid, file_path, caption).await
    }
    async fn send_video(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_video(to_jid, file_path, caption).await
    }
    async fn send_audio(
        &self,
        to_jid: &str,
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_audio(to_jid, file_path).await
    }
    async fn send_voice(
        &self,
        to_jid: &str,
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_voice(to_jid, file_path).await
    }
    async fn send_sticker(
        &self,
        to_jid: &str,
        file_path: &Path,
    ) -> Result<(String, String), PlatformAdapterError> {
        self.send_sticker(to_jid, file_path).await
    }
    async fn send_reaction(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
    ) -> Result<String, PlatformAdapterError> {
        self.send_reaction(to_jid, msg_id, emoji).await
    }
    async fn send_poll(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        multi: bool,
    ) -> Result<String, PlatformAdapterError> {
        self.send_poll(to_jid, question, options, multi).await
    }
    async fn send_contact(
        &self,
        to_jid: &str,
        vcard_path: &Path,
    ) -> Result<String, PlatformAdapterError> {
        self.send_contact(to_jid, vcard_path).await
    }
    async fn send_location(
        &self,
        to_jid: &str,
        lat: f64,
        lon: f64,
        name: &str,
    ) -> Result<String, PlatformAdapterError> {
        self.send_location(to_jid, lat, lon, name).await
    }
    async fn edit_message(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.edit_message(to_jid, msg_id, new_text).await
    }
    async fn delete_message(&self, to_jid: &str, msg_id: &str) -> Result<(), PlatformAdapterError> {
        self.delete_message(to_jid, msg_id).await
    }
    async fn mark_read(
        &self,
        peer_jid: &str,
        up_to_msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.mark_read(peer_jid, up_to_msg_id).await
    }
    async fn message_search(
        &self,
        query: &str,
        peer_jid: Option<&str>,
    ) -> Result<Vec<MessageHit>, PlatformAdapterError> {
        self.message_search(query, peer_jid).await
    }
    async fn chat_info(&self, jid: &str) -> Result<Option<ChatInfo>, PlatformAdapterError> {
        self.chat_info(jid).await
    }
    async fn set_chat_pinned(&self, jid: &str, pinned: bool) -> Result<(), PlatformAdapterError> {
        self.set_chat_pinned(jid, pinned).await
    }
    async fn set_chat_muted(
        &self,
        jid: &str,
        until_epoch_secs: i64,
    ) -> Result<(), PlatformAdapterError> {
        self.set_chat_muted(jid, until_epoch_secs).await
    }
    async fn set_chat_archived(
        &self,
        jid: &str,
        archived: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.set_chat_archived(jid, archived).await
    }
    async fn delete_chat(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        self.delete_chat(jid).await
    }
    async fn send_typing(&self, jid: &str, is_typing: bool) -> Result<(), PlatformAdapterError> {
        self.send_typing(jid, is_typing).await
    }

    // ── Checked wrappers: replicate the size-gate from inherent.rs `_checked` ──
    //
    // Each `_checked` reads the file (or computes a payload size for
    // text-based senders), enforces `max_bytes`, then calls the unchecked
    // via the trait. The bodies are character-for-character copies of
    // the `_checked` wrappers in `inherent.rs` minus the call to
    // `self.send_*` (which was renamed to `self.send_*_inner`); the
    // unchecked call now goes through the trait method, so the test path
    // goes: `_checked` -> `*_inner` (inherent) — keeping the existing
    // size-gate guarantees intact.

    async fn send_image_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("read {file_path:?}: {e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_image(to_jid, file_path, caption).await
    }
    async fn send_video_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        caption: Option<&str>,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_video(to_jid, file_path, caption).await
    }
    async fn send_audio_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_audio(to_jid, file_path).await
    }
    async fn send_voice_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_voice(to_jid, file_path).await
    }
    async fn send_sticker_checked(
        &self,
        to_jid: &str,
        file_path: &Path,
        max_bytes: usize,
    ) -> Result<(String, String), PlatformAdapterError> {
        let data =
            tokio::fs::read(file_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_sticker(to_jid, file_path).await
    }
    async fn send_reaction_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        emoji: &str,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let payload_size = msg_id.len() + emoji.len() + 16;
        if payload_size > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: payload_size,
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_reaction(to_jid, msg_id, emoji).await
    }
    async fn send_poll_checked(
        &self,
        to_jid: &str,
        question: &str,
        options: &[String],
        multi: bool,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let payload_size = question.len() + options.iter().map(|o| o.len()).sum::<usize>() + 32;
        if payload_size > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: payload_size,
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_poll(to_jid, question, options, multi).await
    }
    async fn send_contact_checked(
        &self,
        to_jid: &str,
        vcard_path: &Path,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let data =
            tokio::fs::read(vcard_path)
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{e}"),
                })?;
        if data.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_contact(to_jid, vcard_path).await
    }
    async fn send_location_checked(
        &self,
        to_jid: &str,
        lat: f64,
        lon: f64,
        name: &str,
        max_bytes: usize,
    ) -> Result<String, PlatformAdapterError> {
        let payload_size = name.len() + 64;
        if payload_size > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: payload_size,
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.send_location(to_jid, lat, lon, name).await
    }
    async fn edit_message_checked(
        &self,
        to_jid: &str,
        msg_id: &str,
        new_text: &str,
        max_bytes: usize,
    ) -> Result<(), PlatformAdapterError> {
        if new_text.len() > max_bytes {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: new_text.len(),
                max: max_bytes,
                platform: "whatsapp".into(),
            });
        }
        self.edit_message(to_jid, msg_id, new_text).await
    }

    // ── Non-async capabilities delegation ──

    fn capabilities(&self) -> CapabilityReport {
        use octo_network::dot::PlatformAdapter;
        // Delegate to the existing `PlatformAdapter::capabilities` impl
        // living in `adapter.rs`. The same numbers (`max_payload_bytes`,
        // `media_capabilities.max_upload_bytes`, ...) flow through so the
        // RPC report stays in lockstep.
        <Self as PlatformAdapter>::capabilities(self)
    }

    // ── Download-media delegation ──

    async fn download_media(&self, media_ref_token: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        use octo_network::dot::PlatformAdapter;
        // Same delegation pattern as `capabilities`: the actual wire
        // logic lives on `WhatsAppWebAdapter`'s `PlatformAdapter` impl
        // (in `adapter.rs`), so this trait wrapper just forwards.
        <Self as PlatformAdapter>::download_media(self, media_ref_token).await
    }
}
