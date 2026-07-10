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

use octo_adapter_whatsapp::{
    ChatInfo, MessageHit, NewsletterMetadataSnapshot, PrivacySettingSnapshot, UserInfoSnapshot,
};
use octo_network::dot::adapters::coordinator_admin::CoordinatorAdmin;
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

    /// Send a plain-text message. Returns the new message id.
    ///
    /// `reply_to` is the message id being quoted (None for plain text).
    /// `mentions` is a list of JIDs to ping (empty for none).
    async fn send_text(
        &self,
        to_jid: &str,
        text: &str,
        reply_to: Option<&str>,
        mentions: &[String],
    ) -> Result<String, PlatformAdapterError>;

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

    /// Pin a message in a chat for all participants (7-day default).
    async fn pin_message(&self, peer_jid: &str, msg_id: &str) -> Result<(), PlatformAdapterError>;

    /// Unpin a previously pinned message.
    async fn unpin_message(&self, peer_jid: &str, msg_id: &str)
        -> Result<(), PlatformAdapterError>;

    /// Forward a previously-sent text message to a new peer.
    /// Returns the new message id.
    async fn forward_message(
        &self,
        peer_jid: &str,
        original_msg_id: &str,
    ) -> Result<String, PlatformAdapterError>;

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

    // ── Group F2: contact + presence (queries) ────────────────────────
    //
    // Tier 4 of the live coverage matrix. Each wraps a thin slice of the
    // WA crate's `Contacts`, `Blocking`, and `Presence` features; the IPC
    // handlers under `ipc/handlers/` translate them into JSON-RPC.

    /// Check whether a phone-number JID is registered on WhatsApp.
    /// Returns `true` if the user has an account, `false` otherwise.
    /// Maps to `wacore::Client::contacts().is_on_whatsapp(...)` for one
    /// input JID.
    async fn is_on_whatsapp(&self, jid: &str) -> Result<bool, PlatformAdapterError>;

    /// Fetch the profile-picture URL for a peer. `preview = true` asks
    /// for the thumbnail; `false` asks for the full image. Maps to
    /// `wacore::Client::contacts().get_profile_picture(...)`. Returns
    /// `Ok(None)` when the peer has no profile picture (or it is hidden
    /// by privacy settings).
    async fn get_profile_picture_url(
        &self,
        jid: &str,
        preview: bool,
    ) -> Result<Option<String>, PlatformAdapterError>;

    /// Add the peer to the local blocklist. The WA server propagates
    /// the block to all our linked devices.
    async fn block_contact(&self, jid: &str) -> Result<(), PlatformAdapterError>;

    /// Remove the peer from the local blocklist.
    async fn unblock_contact(&self, jid: &str) -> Result<(), PlatformAdapterError>;

    /// Subscribe to the peer's presence updates. Sends a
    /// `<presence type="subscribe">` stanza. After this returns,
    /// inbound `InboundEvent::Presence { peer, kind }` events will fire
    /// whenever the peer goes online/offline.
    async fn subscribe_presence(&self, jid: &str) -> Result<(), PlatformAdapterError>;

    /// Unsubscribe from the peer's presence updates.
    async fn unsubscribe_presence(&self, jid: &str) -> Result<(), PlatformAdapterError>;

    /// Broadcast our presence as `available` (online). All peers that
    /// have subscribed to us see the change within ~1s.
    async fn set_presence_available(&self) -> Result<(), PlatformAdapterError>;

    /// Broadcast our presence as `unavailable` (offline).
    async fn set_presence_unavailable(&self) -> Result<(), PlatformAdapterError>;

    // ── Group F3: profile (Tier 6) ─────────────────────────────────
    //
    // Updates that touch our OWN profile. Each maps 1:1 to a thin
    // wacore call. The IPC handlers under `ipc/handlers/profile_*`
    // translate them into JSON-RPC.

    /// Set our push name (the display name peers see in chats and
    /// groups). Maps to `Client::profile().set_push_name(name)`.
    /// Propagates cross-device via app-state sync.
    async fn set_push_name(&self, name: &str) -> Result<(), PlatformAdapterError>;

    /// Set our "About" status text (the persistent profile status —
    /// NOT the ephemeral text status). Maps to
    /// `Client::profile().set_status_text(text)`.
    async fn set_status_text(&self, text: &str) -> Result<(), PlatformAdapterError>;

    // ── Group F4: contact enrichment (Tier 6) ──────────────────────

    /// Fetch rich user info for a single JID (status, picture_id,
    /// business flag, verified name, linked device IDs). Returns
    /// `Ok(None)` when the WA server reports no record for the JID.
    /// Maps to `Client::contacts().get_user_info(&[Jid])`.
    async fn get_user_info(
        &self,
        jid: &str,
    ) -> Result<Option<UserInfoSnapshot>, PlatformAdapterError>;

    // ── Group F5: privacy + blocklist queries (Tier 6.1) ───────────
    //
    // Privacy: thin wrappers over `Client::fetch_privacy_settings`
    // + `Client::set_privacy_setting`. Blocklist queries: thin
    // wrappers over `Client::blocking().get_blocklist / is_blocked`.

    /// Fetch all current privacy settings as a list of
    /// `{category, value}` pairs (wire string forms). Maps to
    /// `Client::fetch_privacy_settings()`.
    async fn fetch_privacy_settings(
        &self,
    ) -> Result<Vec<PrivacySettingSnapshot>, PlatformAdapterError>;

    /// Set one privacy setting. `category` and `value` are the wire
    /// strings (`"last"`, `"profile"`, `"contacts"`, `"all"`, `"none"`,
    /// etc.). Maps to `Client::set_privacy_setting(...)`.
    async fn set_privacy_setting(
        &self,
        category: &str,
        value: &str,
    ) -> Result<(), PlatformAdapterError>;

    /// Return the current local blocklist as a list of JID strings.
    /// Maps to `Client::blocking().get_blocklist()`.
    async fn get_blocklist(&self) -> Result<Vec<String>, PlatformAdapterError>;

    /// Check whether a single JID is currently on our blocklist.
    /// Maps to `Client::blocking().is_blocked(jid)`.
    async fn is_blocked(&self, jid: &str) -> Result<bool, PlatformAdapterError>;

    // ── Group F6: labels + polls + star (Tier 6.2) ────────────────
    //
    // Cross-cutting mutators that touch app-state sync on the WA
    // server. Most return the new label/poll id directly; star
    // returns nothing (just an IQ ACK).

    /// Create a new label. `label_id` is caller-assigned (upsert: WA
    /// allows renaming/recoloring an existing label by reissuing
    /// with the same id). Maps to
    /// `Client::labels().create_label(label_id, name, color)`.
    async fn create_label(
        &self,
        label_id: &str,
        name: &str,
        color: i32,
    ) -> Result<(), PlatformAdapterError>;

    /// Delete a label by id. Maps to
    /// `Client::labels().delete_label(label_id)`.
    async fn delete_label(&self, label_id: &str) -> Result<(), PlatformAdapterError>;

    /// Attach a label to a chat. Maps to
    /// `Client::labels().add_chat_label(label_id, chat_jid)`.
    async fn add_chat_label(
        &self,
        label_id: &str,
        chat_jid: &str,
    ) -> Result<(), PlatformAdapterError>;

    /// Remove a label from a chat. Maps to
    /// `Client::labels().remove_chat_label(label_id, chat_jid)`.
    async fn remove_chat_label(
        &self,
        label_id: &str,
        chat_jid: &str,
    ) -> Result<(), PlatformAdapterError>;

    /// Star a message. `peer` is the chat JID; `msg_id` is the
    /// message id (use `from_me = true` for outbound, `false` for
    /// inbound messages). Maps to
    /// `Client::chatstate().star_message(...)`.
    async fn star_message(
        &self,
        peer: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError>;

    /// Unstar a message. Same shape as `star_message`.
    async fn unstar_message(
        &self,
        peer: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError>;

    // ── Group F7: messages.mark_as_played (Tier 6.3) ──────────────
    //
    // Receipt variant — voice/video "played" ack.

    /// Send a `played` receipt for one or more messages. Maps to
    /// `Client::mark_as_played(chat, sender, message_ids)`.
    async fn mark_as_played(
        &self,
        chat: &str,
        msg_ids: &[String],
    ) -> Result<(), PlatformAdapterError>;

    // ── Group F8: chats.clear (Tier 6.3) ─────────────────────────
    //
    // Clear all messages in a chat but keep the chat entry. Distinct
    // from `chats.delete` which removes the chat entirely.

    /// Clear all messages in a chat. `delete_starred` also removes
    /// starred messages; `delete_media` also removes downloaded
    /// media. Maps to `Client::chat_actions().clear_chat(...)`.
    async fn clear_chat(
        &self,
        jid: &str,
        delete_starred: bool,
        delete_media: bool,
    ) -> Result<(), PlatformAdapterError>;

    // ── Group F9: messages.delete_for_me (Tier 6.3) ──────────────

    /// Local-only delete (not for everyone). Maps to
    /// `Client::chat_actions().delete_message_for_me(...)`.
    async fn delete_message_for_me(
        &self,
        chat: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError>;

    // ── Group F10: contacts.save_contact (Tier 6.3) ──────────────

    /// Save or rename a contact. Maps to
    /// `Client::chat_actions().save_contact(jid, full_name, first_name,
    /// save_on_primary_addressbook)`. The jid must be a phone-number
    /// JID (LIDs rejected by the WA server).
    async fn save_contact(&self, jid: &str, full_name: &str) -> Result<(), PlatformAdapterError>;

    // ── Group F11: identity (Tier 6.4) ────────────────────────────
    //
    // Local-state reads — no WA server roundtrip needed (PN / LID
    // come from the in-memory `persistence_manager`; identity_tags
    // are the user's local device tag set).

    /// Return our PN (phone-number) JID as a string, or `None` if
    /// the device is not signed in. Maps to `Client::get_pn()`.
    async fn get_pn(&self) -> Result<Option<String>, PlatformAdapterError>;

    /// Return our LID (local identifier) JID as a string, or `None`
    /// if migration has not occurred. Maps to `Client::get_lid()`.
    async fn get_lid(&self) -> Result<Option<String>, PlatformAdapterError>;

    /// Return `true` if the device has completed LID migration.
    /// Maps to `Client::is_lid_migrated()`.
    async fn is_lid_migrated(&self) -> Result<bool, PlatformAdapterError>;

    // ── Group F12: newsletter (Tier 6.5) ─────────────────────────
    //
    // Newsletter = WA's broadcast channel feature (one-to-many,
    // followers-only). The runtime exposes list/get/leave; create /
    // join remain operator-driven for now (admin gates).

    /// List all newsletters this account is subscribed to. Maps to
    /// `Client::newsletter().list_subscribed()`.
    async fn list_subscribed_newsletters(
        &self,
    ) -> Result<Vec<NewsletterMetadataSnapshot>, PlatformAdapterError>;

    /// Fetch metadata for one newsletter by its JID. Maps to
    /// `Client::newsletter().get_metadata(jid)`.
    async fn get_newsletter_metadata(
        &self,
        jid: &str,
    ) -> Result<NewsletterMetadataSnapshot, PlatformAdapterError>;

    /// Leave a newsletter. Maps to
    /// `Client::newsletter().leave(jid)`.
    async fn leave_newsletter(&self, jid: &str) -> Result<(), PlatformAdapterError>;

    // ── Group F13: events (Tier 6.5) ────────────────────────────
    //
    // WhatsApp's calendar / event feature (separate from the
    // chat-bot messages). `events.create` sends a new event;
    // `events.respond` is RSVP, gated behind operator setup
    // (the event + secret must originate from a real creation).

    /// Create a WA calendar event with `name`, `start_time_unix`, and
    /// optional `description`. Maps to
    /// `Client::events().create(jid, params)`. The `message_secret`
    /// returned by wacore is internal to the protocol and not
    /// surfaced to the runtime.
    async fn create_event(
        &self,
        to_jid: &str,
        name: &str,
        start_time_unix: i64,
        description: Option<&str>,
    ) -> Result<String, PlatformAdapterError>;

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

    /// Runtime-side escape hatch to reach the
    /// [`CoordinatorAdmin`](octo_network::dot::adapters::coordinator_admin::CoordinatorAdmin)
    /// trait object. Default: `None`.
    ///
    /// `WhatsAppWebAdapter` overrides to `Some(self)`. `MockAdapter`
    /// overrides to `Some(&self.coord_admin)` so hermetic tests can
    /// exercise the membership/mode/admin handler surface (Phase 6.12).
    ///
    /// Default-`None` is safe for adapters that don't implement
    /// `CoordinatorAdmin` (e.g. Matrix, Telegram adapters). Callers
    /// must handle the `None` arm by returning a `NotConnected`-style
    /// RpcError to the RPC caller.
    fn as_coordinator_admin(&self) -> Option<&dyn CoordinatorAdmin> {
        None
    }

    /// Runtime-side escape hatch to subscribe to the adapter's raw
    /// lifecycle event stream. Returns the `format!("{:?}", event)`
    /// strings the underlying SDK emits (`Event::Connected(_)`, etc.).
    ///
    /// Default: `None`. `WhatsAppWebAdapter` overrides to forward to
    /// its internal `broadcast::Sender` (`adapter.rs:500`). `MockAdapter`
    /// keeps the default — hermetic tests don't drive a real event
    /// stream.
    ///
    /// Consumed by the daemon's connection-watcher task (Phase 6.12.4)
    /// to translate WA events into `BotStateMirror` transitions.
    /// Adapters without a single bot lifecycle (Matrix, Telegram)
    /// keep the default `None` and the watcher simply does nothing.
    fn subscribe_raw_events(&self) -> Option<tokio::sync::broadcast::Receiver<String>> {
        None
    }
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

    async fn send_text(
        &self,
        to_jid: &str,
        text: &str,
        reply_to: Option<&str>,
        mentions: &[String],
    ) -> Result<String, PlatformAdapterError> {
        self.send_text(to_jid, text, reply_to, mentions).await
    }

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
    async fn pin_message(&self, peer_jid: &str, msg_id: &str) -> Result<(), PlatformAdapterError> {
        self.pin_message(peer_jid, msg_id).await
    }
    async fn unpin_message(
        &self,
        peer_jid: &str,
        msg_id: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.unpin_message(peer_jid, msg_id).await
    }
    async fn forward_message(
        &self,
        peer_jid: &str,
        original_msg_id: &str,
    ) -> Result<String, PlatformAdapterError> {
        self.forward_message(peer_jid, original_msg_id).await
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

    // ── Tier 4: contact + presence delegation ──────────────────────────

    async fn is_on_whatsapp(&self, jid: &str) -> Result<bool, PlatformAdapterError> {
        self.is_on_whatsapp(jid).await
    }
    async fn get_profile_picture_url(
        &self,
        jid: &str,
        preview: bool,
    ) -> Result<Option<String>, PlatformAdapterError> {
        self.get_profile_picture_url(jid, preview).await
    }
    async fn block_contact(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        self.block_contact(jid).await
    }
    async fn unblock_contact(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        self.unblock_contact(jid).await
    }
    async fn subscribe_presence(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        self.subscribe_presence(jid).await
    }
    async fn unsubscribe_presence(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        self.unsubscribe_presence(jid).await
    }
    async fn set_presence_available(&self) -> Result<(), PlatformAdapterError> {
        self.set_presence_available().await
    }
    async fn set_presence_unavailable(&self) -> Result<(), PlatformAdapterError> {
        self.set_presence_unavailable().await
    }

    // ── Tier 6: profile + contact-enrichment delegation ─────────────

    async fn set_push_name(&self, name: &str) -> Result<(), PlatformAdapterError> {
        self.set_push_name(name).await
    }
    async fn set_status_text(&self, text: &str) -> Result<(), PlatformAdapterError> {
        self.set_status_text(text).await
    }
    async fn get_user_info(
        &self,
        jid: &str,
    ) -> Result<Option<UserInfoSnapshot>, PlatformAdapterError> {
        self.get_user_info(jid).await
    }

    // ── Tier 6.1: privacy + blocklist queries delegation ───────────

    async fn fetch_privacy_settings(
        &self,
    ) -> Result<Vec<PrivacySettingSnapshot>, PlatformAdapterError> {
        self.fetch_privacy_settings().await
    }
    async fn set_privacy_setting(
        &self,
        category: &str,
        value: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.set_privacy_setting(category, value).await
    }
    async fn get_blocklist(&self) -> Result<Vec<String>, PlatformAdapterError> {
        self.get_blocklist().await
    }
    async fn is_blocked(&self, jid: &str) -> Result<bool, PlatformAdapterError> {
        self.is_blocked(jid).await
    }

    // ── Tier 6.2: labels + star + polls delegation ────────────────

    async fn create_label(
        &self,
        label_id: &str,
        name: &str,
        color: i32,
    ) -> Result<(), PlatformAdapterError> {
        self.create_label(label_id, name, color).await
    }
    async fn delete_label(&self, label_id: &str) -> Result<(), PlatformAdapterError> {
        self.delete_label(label_id).await
    }
    async fn add_chat_label(
        &self,
        label_id: &str,
        chat_jid: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.add_chat_label(label_id, chat_jid).await
    }
    async fn remove_chat_label(
        &self,
        label_id: &str,
        chat_jid: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.remove_chat_label(label_id, chat_jid).await
    }
    async fn star_message(
        &self,
        peer: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.star_message(peer, msg_id, from_me).await
    }
    async fn unstar_message(
        &self,
        peer: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.unstar_message(peer, msg_id, from_me).await
    }

    // ── Tier 6.3: mark_as_played / clear_chat / delete_message_for_me / save_contact ─

    async fn mark_as_played(
        &self,
        chat: &str,
        msg_ids: &[String],
    ) -> Result<(), PlatformAdapterError> {
        self.mark_as_played(chat, msg_ids).await
    }
    async fn clear_chat(
        &self,
        jid: &str,
        delete_starred: bool,
        delete_media: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.clear_chat(jid, delete_starred, delete_media).await
    }
    async fn delete_message_for_me(
        &self,
        chat: &str,
        msg_id: &str,
        from_me: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.delete_message_for_me(chat, msg_id, from_me).await
    }
    async fn save_contact(&self, jid: &str, full_name: &str) -> Result<(), PlatformAdapterError> {
        self.save_contact(jid, full_name).await
    }

    // ── Tier 6.4: identity delegation ─────────────────────────────

    async fn get_pn(&self) -> Result<Option<String>, PlatformAdapterError> {
        self.get_pn().await
    }
    async fn get_lid(&self) -> Result<Option<String>, PlatformAdapterError> {
        self.get_lid().await
    }
    async fn is_lid_migrated(&self) -> Result<bool, PlatformAdapterError> {
        self.is_lid_migrated().await
    }

    // ── Tier 6.5: newsletter + events delegation ──────────────────

    async fn list_subscribed_newsletters(
        &self,
    ) -> Result<Vec<NewsletterMetadataSnapshot>, PlatformAdapterError> {
        self.list_subscribed_newsletters().await
    }
    async fn get_newsletter_metadata(
        &self,
        jid: &str,
    ) -> Result<NewsletterMetadataSnapshot, PlatformAdapterError> {
        self.get_newsletter_metadata(jid).await
    }
    async fn leave_newsletter(&self, jid: &str) -> Result<(), PlatformAdapterError> {
        self.leave_newsletter(jid).await
    }
    async fn create_event(
        &self,
        to_jid: &str,
        name: &str,
        start_time_unix: i64,
        description: Option<&str>,
    ) -> Result<String, PlatformAdapterError> {
        self.create_event(to_jid, name, start_time_unix, description)
            .await
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

    // ── CoordinatorAdmin probe (Phase 6.12) ──────────────────────────────

    fn as_coordinator_admin(&self) -> Option<&dyn CoordinatorAdmin> {
        // `WhatsAppWebAdapter` already implements `CoordinatorAdmin` in
        // `adapter.rs:2592`, with its `PlatformAdapter::as_coordinator_admin`
        // probe returning `Some(self)`. We forward through that probe so
        // a single source of truth decides the answer — if a future
        // change makes the live adapter conditional, the trait surface
        // tracks it automatically.
        use octo_network::dot::PlatformAdapter;
        <Self as PlatformAdapter>::as_coordinator_admin(self)
    }

    // ── Raw event stream forwarder (Phase 6.12.4) ────────────────────────

    fn subscribe_raw_events(&self) -> Option<tokio::sync::broadcast::Receiver<String>> {
        // `WhatsAppWebAdapter::subscribe_raw_events` lives on the
        // concrete type (`adapter.rs:500`) rather than on the
        // `PlatformAdapter` trait (which serves 5+ adapter impls).
        // The trait impl here is targeted specifically at the WA
        // backend, so calling the concrete method is appropriate.
        Some(octo_adapter_whatsapp::WhatsAppWebAdapter::subscribe_raw_events(self))
    }
}

// ===========================================================================
// Tests — delegation coverage
// ===========================================================================
//
// `OctoWhatsAppAdapter` is consumed by the runtime's `DaemonHandle` via an
// `Arc<dyn OctoWhatsAppAdapter>`. In production the daemon either binds a
// `MockAdapter` (in tests) or a live `WhatsAppWebAdapter` (in production),
// so the `impl OctoWhatsAppAdapter for WhatsAppWebAdapter` block above
// (330 lines) is never exercised by `DaemonHandle` test paths. Result:
// `adapter_trait.rs` shows 0% line coverage in `cargo llvm-cov ... -p
// octo-whatsapp` — the runtime always binds the mock.
//
// These tests close that gap by direct-calling each method on a
// `WhatsAppWebAdapter::new_unconnected_for_tests()` fixture (an unconnected
// adapter with no live wacore client). Each method that needs a connected
// client returns `Err(PlatformAdapterError::Unreachable { reason:
// "client not connected", .. })`; `delete_chat` returns `Ok(())` (pure
// client-side op, no client required); `capabilities()` returns a static
// `CapabilityReport`. Proving the `impl` body runs end-to-end is the goal
// — not exhaustively re-testing the inherent method bodies (those are
// already pinned by `inherent.rs`'s `mod tests`).
#[cfg(test)]
mod tests {
    use super::*;
    use octo_adapter_whatsapp::WhatsAppWebAdapter;
    use octo_network::dot::error::PlatformAdapterError;

    fn adapter() -> WhatsAppWebAdapter {
        // Inlines the same minimal config used by the adapter's own
        // `#[cfg(any(test, feature = "test-helpers"))]`
        // `new_unconnected_for_tests()` — duplicated here so this test
        // module can build against `octo-adapter-whatsapp`'s public API
        // without flipping any feature flag on the dep crate.
        // `session_path` is required by `WhatsAppConfig`; `start_bot` is
        // never called from here so the path is never opened or written.
        let cfg_json =
            br#"{"session_path":"/tmp/octo-whatsapp-trait-test.session.db","groups":[]}"#;
        WhatsAppWebAdapter::from_config_bytes(cfg_json)
            .expect("test adapter: from_config_bytes should accept the minimal JSON")
    }

    /// Build a temp file with `size` zero bytes; returns its `PathBuf`.
    fn tmp_file(name: &str, size: usize) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("octo-octowa-traits-{name}"));
        std::fs::write(&p, vec![0u8; size]).unwrap();
        p
    }

    /// A peer JID with the canonical shape `<digits>@s.whatsapp.net` —
    /// passes the `wacore_binary::Jid::parse` precondition that several
    /// inherent methods enforce before the client lock check.
    const JID: &str = "1234567890@s.whatsapp.net";

    /// Assert the inherent client-gate fired: every method that needs a
    /// live `wacore` client short-circuits with
    /// `Unreachable { reason: "client not connected" }`.
    fn assert_client_not_connected<T: std::fmt::Debug>(r: Result<T, PlatformAdapterError>) {
        match r {
            Err(PlatformAdapterError::Unreachable { reason, .. }) => {
                assert!(
                    reason.contains("client not connected"),
                    "expected reason containing 'client not connected', got {reason:?}"
                );
            }
            Err(other) => {
                panic!("expected Err(Unreachable {{ client not connected }}), got {other:?}")
            }
            Ok(v) => panic!("expected Err(Unreachable {{ client not connected }}), got Ok({v:?})"),
        }
    }

    // ── Group A: file-based send (unchecked) ──

    #[tokio::test]
    async fn delegation_send_text() {
        // Plain text does not need a client file — should still
        // short-circuit on the client-lock check.
        assert_client_not_connected(adapter().send_text(JID, "hello", None, &[]).await);
    }

    #[tokio::test]
    async fn delegation_send_image() {
        let p = tmp_file("img.jpg", 16);
        let r = adapter().send_image(JID, &p, None).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_video() {
        let p = tmp_file("vid.mp4", 16);
        let r = adapter().send_video(JID, &p, None).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_audio() {
        let p = tmp_file("aud.mp3", 16);
        let r = adapter().send_audio(JID, &p).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_voice() {
        let p = tmp_file("vo.ogg", 16);
        let r = adapter().send_voice(JID, &p).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_sticker() {
        let p = tmp_file("stk.webp", 16);
        let r = adapter().send_sticker(JID, &p).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }

    // ── Group B: payload-only send (unchecked) ──

    #[tokio::test]
    async fn delegation_send_reaction() {
        assert_client_not_connected(adapter().send_reaction(JID, "msg-1", "\u{1f44d}").await);
    }
    #[tokio::test]
    async fn delegation_send_poll() {
        let opts = vec!["A".to_string(), "B".to_string()];
        assert_client_not_connected(adapter().send_poll(JID, "Q?", &opts, false).await);
    }
    #[tokio::test]
    async fn delegation_send_contact() {
        let p = tmp_file("contact.vcf", 16);
        let r = adapter().send_contact(JID, &p).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_location() {
        assert_client_not_connected(
            adapter()
                .send_location(JID, 37.7749, -122.4194, "San Francisco")
                .await,
        );
    }

    // ── Group C: message lifecycle (unchecked) ──

    #[tokio::test]
    async fn delegation_edit_message() {
        assert_client_not_connected(adapter().edit_message(JID, "msg-1", "edited").await);
    }
    #[tokio::test]
    async fn delegation_delete_message() {
        assert_client_not_connected(adapter().delete_message(JID, "msg-1").await);
    }
    #[tokio::test]
    async fn delegation_mark_read() {
        assert_client_not_connected(adapter().mark_read(JID, "msg-1").await);
    }

    // ── Group D: search + chat metadata (unchecked) ──
    //
    // `message_search` and `chat_info` consult the local store, not the
    // client — they return `Ok` with empty / minimal data when no store
    // is bound. We assert no panic and a successful Ok to prove the trait
    // dispatch reached the inherent body.

    #[tokio::test]
    async fn delegation_message_search() {
        let r = adapter().message_search("query", Some(JID)).await;
        assert!(r.is_ok(), "message_search returned error: {r:?}");
    }
    #[tokio::test]
    async fn delegation_chat_info() {
        let r = adapter().chat_info(JID).await;
        assert!(r.is_ok(), "chat_info returned error: {r:?}");
    }

    // ── Group E: chat ops (unchecked) ──

    #[tokio::test]
    async fn delegation_set_chat_pinned() {
        // The inherent body short-circuits BEFORE the client-lock check
        // with `Unreachable { reason: "chat pinning not yet supported by
        // wacore 0.6" }`. That's still proof the trait dispatch reached
        // the inherent method — accept any `Unreachable` variant.
        match adapter().set_chat_pinned(JID, true).await {
            Err(PlatformAdapterError::Unreachable { .. }) => {}
            other => panic!("expected Err(Unreachable {{ .. }}), got {other:?}"),
        }
    }
    #[tokio::test]
    async fn delegation_set_chat_muted() {
        match adapter().set_chat_muted(JID, 0).await {
            Err(PlatformAdapterError::Unreachable { .. }) => {}
            other => panic!("expected Err(Unreachable {{ .. }}), got {other:?}"),
        }
    }
    #[tokio::test]
    async fn delegation_set_chat_archived() {
        match adapter().set_chat_archived(JID, true).await {
            Err(PlatformAdapterError::Unreachable { .. }) => {}
            other => panic!("expected Err(Unreachable {{ .. }}), got {other:?}"),
        }
    }
    #[tokio::test]
    async fn delegation_delete_chat() {
        // `delete_chat` is a pure client-side cache clear — succeeds even
        // with no client bound.
        assert_eq!(adapter().delete_chat(JID).await, Ok(()));
    }

    // ── Group F: presence (unchecked) ──

    #[tokio::test]
    async fn delegation_send_typing() {
        assert_client_not_connected(adapter().send_typing(JID, true).await);
    }

    // ── Tier 4: contact + presence (unchecked) ────────────────────────
    //
    // Each delegation test verifies that the trait method forwards
    // through to the inherent method on the same adapter. With no
    // client bound the inherent body returns `Unreachable` — exactly
    // what the trait method should propagate.

    #[tokio::test]
    async fn delegation_is_on_whatsapp() {
        assert_client_not_connected(adapter().is_on_whatsapp(JID).await);
    }
    #[tokio::test]
    async fn delegation_get_profile_picture_url() {
        assert_client_not_connected(adapter().get_profile_picture_url(JID, true).await);
    }
    #[tokio::test]
    async fn delegation_block_contact() {
        assert_client_not_connected(adapter().block_contact(JID).await);
    }
    #[tokio::test]
    async fn delegation_unblock_contact() {
        assert_client_not_connected(adapter().unblock_contact(JID).await);
    }
    #[tokio::test]
    async fn delegation_subscribe_presence() {
        assert_client_not_connected(adapter().subscribe_presence(JID).await);
    }
    #[tokio::test]
    async fn delegation_unsubscribe_presence() {
        assert_client_not_connected(adapter().unsubscribe_presence(JID).await);
    }
    #[tokio::test]
    async fn delegation_set_presence_available() {
        assert_client_not_connected(adapter().set_presence_available().await);
    }
    #[tokio::test]
    async fn delegation_set_presence_unavailable() {
        assert_client_not_connected(adapter().set_presence_unavailable().await);
    }

    // ── Tier 6: profile + contact-enrichment (unchecked) ────────────

    #[tokio::test]
    async fn delegation_set_push_name() {
        assert_client_not_connected(adapter().set_push_name("Alice").await);
    }
    #[tokio::test]
    async fn delegation_set_status_text() {
        assert_client_not_connected(adapter().set_status_text("hello").await);
    }
    #[tokio::test]
    async fn delegation_get_user_info() {
        assert_client_not_connected(adapter().get_user_info(JID).await);
    }

    // ── Tier 6.1: privacy + blocklist queries (unchecked) ──────────

    #[tokio::test]
    async fn delegation_fetch_privacy_settings() {
        assert_client_not_connected(adapter().fetch_privacy_settings().await);
    }
    #[tokio::test]
    async fn delegation_set_privacy_setting() {
        assert_client_not_connected(adapter().set_privacy_setting("last", "contacts").await);
    }
    #[tokio::test]
    async fn delegation_get_blocklist() {
        assert_client_not_connected(adapter().get_blocklist().await);
    }
    #[tokio::test]
    async fn delegation_is_blocked() {
        assert_client_not_connected(adapter().is_blocked(JID).await);
    }

    // ── Tier 6.2: labels + star + polls (unchecked) ──────────────

    #[tokio::test]
    async fn delegation_create_label() {
        assert_client_not_connected(adapter().create_label("42", "work", 0).await);
    }
    #[tokio::test]
    async fn delegation_delete_label() {
        assert_client_not_connected(adapter().delete_label("42").await);
    }
    #[tokio::test]
    async fn delegation_add_chat_label() {
        assert_client_not_connected(adapter().add_chat_label("42", JID).await);
    }
    #[tokio::test]
    async fn delegation_remove_chat_label() {
        assert_client_not_connected(adapter().remove_chat_label("42", JID).await);
    }
    #[tokio::test]
    async fn delegation_star_message() {
        assert_client_not_connected(adapter().star_message(JID, "m1", true).await);
    }
    #[tokio::test]
    async fn delegation_unstar_message() {
        assert_client_not_connected(adapter().unstar_message(JID, "m1", true).await);
    }

    // ── Tier 6.3: mark_as_played / clear_chat / delete_for_me / save_contact ─

    #[tokio::test]
    async fn delegation_mark_as_played() {
        assert_client_not_connected(adapter().mark_as_played(JID, &["m1".to_string()]).await);
    }
    #[tokio::test]
    async fn delegation_clear_chat() {
        assert_client_not_connected(adapter().clear_chat(JID, false, false).await);
    }
    #[tokio::test]
    async fn delegation_delete_message_for_me() {
        assert_client_not_connected(adapter().delete_message_for_me(JID, "m1", true).await);
    }
    #[tokio::test]
    async fn delegation_save_contact() {
        assert_client_not_connected(adapter().save_contact(JID, "Alice").await);
    }

    // ── Tier 6.4: identity (unchecked) ────────────────────────────

    #[tokio::test]
    async fn delegation_get_pn() {
        assert_client_not_connected(adapter().get_pn().await);
    }
    #[tokio::test]
    async fn delegation_get_lid() {
        assert_client_not_connected(adapter().get_lid().await);
    }
    #[tokio::test]
    async fn delegation_is_lid_migrated() {
        assert_client_not_connected(adapter().is_lid_migrated().await);
    }

    // ── Tier 6.5: newsletter + events (unchecked) ───────────────

    #[tokio::test]
    async fn delegation_list_subscribed_newsletters() {
        assert_client_not_connected(adapter().list_subscribed_newsletters().await);
    }
    #[tokio::test]
    async fn delegation_get_newsletter_metadata() {
        assert_client_not_connected(adapter().get_newsletter_metadata(JID).await);
    }
    #[tokio::test]
    async fn delegation_leave_newsletter() {
        assert_client_not_connected(adapter().leave_newsletter(JID).await);
    }
    #[tokio::test]
    async fn delegation_create_event() {
        assert_client_not_connected(
            adapter()
                .create_event(JID, "tier6-event", 1_700_000_000, None)
                .await,
        );
    }

    // ── Group G: size-gated wrappers ──
    //
    // These read the file (or compute a payload size) BEFORE calling the
    // unchecked inherent method. We pass a small file / well-sized text
    // so the size check passes and the inherent body runs (which then
    // short-circuits on the missing client).

    #[tokio::test]
    async fn delegation_send_image_checked() {
        let p = tmp_file("img-c.jpg", 16);
        let r = adapter().send_image_checked(JID, &p, None, 1024).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_video_checked() {
        let p = tmp_file("vid-c.mp4", 16);
        let r = adapter().send_video_checked(JID, &p, None, 1024).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_audio_checked() {
        let p = tmp_file("aud-c.mp3", 16);
        let r = adapter().send_audio_checked(JID, &p, 1024).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_voice_checked() {
        let p = tmp_file("vo-c.ogg", 16);
        let r = adapter().send_voice_checked(JID, &p, 1024).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_sticker_checked() {
        let p = tmp_file("stk-c.webp", 16);
        let r = adapter().send_sticker_checked(JID, &p, 1024).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_reaction_checked() {
        // payload = msg_id.len() + emoji.len() + 16 = 5 + 4 + 16 = 25; max 1024 OK.
        assert_client_not_connected(
            adapter()
                .send_reaction_checked(JID, "msg-1", "\u{1f44d}", 1024)
                .await,
        );
    }
    #[tokio::test]
    async fn delegation_send_poll_checked() {
        let opts = vec!["A".to_string()];
        // payload = 2 + 1 + 32 = 35; max 1024 OK.
        assert_client_not_connected(
            adapter()
                .send_poll_checked(JID, "Q?", &opts, false, 1024)
                .await,
        );
    }
    #[tokio::test]
    async fn delegation_send_contact_checked() {
        let p = tmp_file("cont-c.vcf", 16);
        let r = adapter().send_contact_checked(JID, &p, 1024).await;
        let _ = std::fs::remove_file(&p);
        assert_client_not_connected(r);
    }
    #[tokio::test]
    async fn delegation_send_location_checked() {
        // payload = name.len() + 64 = 13 + 64 = 77; max 1024 OK.
        assert_client_not_connected(
            adapter()
                .send_location_checked(JID, 0.0, 0.0, "Anywhere", 1024)
                .await,
        );
    }
    #[tokio::test]
    async fn delegation_edit_message_checked() {
        // payload = new_text.len() = 3; max 1024 OK.
        assert_client_not_connected(
            adapter()
                .edit_message_checked(JID, "msg-1", "new", 1024)
                .await,
        );
    }

    // ── Non-async: capabilities delegation ──

    #[test]
    fn delegation_capabilities() {
        // capabilities() returns a static CapabilityReport — must not
        // depend on client state.
        let r = adapter().capabilities();
        assert!(
            r.max_payload_bytes > 0,
            "capabilities().max_payload_bytes must be > 0, got {}",
            r.max_payload_bytes
        );
    }

    // ── Non-async: download_media delegation ──
    //
    // `download_media` decodes the base64url token first; an invalid token
    // short-circuits to `ApiError` BEFORE the client-lock check. So we
    // accept any `Err` variant here — the goal is to prove the trait
    // wrapper reached the inherent body (which it did: the inherent
    // `download_via_media_ref` ran and returned `Err`, surfaced through
    // the trait wrapper).

    #[tokio::test]
    async fn delegation_download_media() {
        let r = adapter().download_media("not-base64!!!").await;
        assert!(r.is_err(), "download_media must error on bad token, got Ok");
    }
}
