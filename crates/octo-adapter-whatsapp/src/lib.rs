//! WhatsApp Web adapter for DOT (RFC-0850 §8.1, PlatformType::WhatsApp)
//!
//! Bridges DOT envelopes to WhatsApp groups via the native WhatsApp Web protocol
//! using whatsapp-rust. No Meta Business verification required.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "session_path": "~/.cipherocto/whatsapp-session.db",
//!   "pair_phone": "15551234567",
//!   "groups": ["120363012345678901"]
//! }
//! ```

pub mod adapter;
/// Phase 2 — 18 new inherent methods on `WhatsAppWebAdapter`
/// (`send_image`, `edit_message`, `mark_read`, ...).
pub mod inherent;
/// Session 2 of the wacore-webauthn plan (RFC-0909): `PasskeyAuthenticator`
/// trait seam + `CallbackAuthenticator` for the SHORTCAKE_PASSKEY link flow.
pub mod passkey;
/// Re-export of `PlatformAdapterError` from `octo-network::dot::error`.
/// Provides a stable import path for inherent methods and runtime code.
pub use octo_network::dot::error::PlatformAdapterError as AdapterError;
mod media_ref; // R9-M1 fix: was `pub mod media_ref;`; the spec at
               // `missions/open/0850-whatsapp-media-transport.md:224`
               // explicitly requires the module be private (it's an
               // implementation detail of the adapter's wire format,
               // not part of the public API). All `MediaRef` fields are
               // `pub(crate)` so this change doesn't break the adapter.
pub mod state;
pub mod store;

pub use adapter::{CreateGroupOutput, WhatsAppConfig, WhatsAppWebAdapter};
pub use state::{BotState, LoggedOutCause};
pub use store::StoolapStore;
// Re-export the whatsapp-rust types that the e2e group-setup test references
// directly. Keeping the re-exports centralised here means callers (and the
// test) don't need a direct `whatsapp-rust` dependency on their dev-deps
// just to spell out a `CreateGroupOutput.metadata.participants: Vec<GroupParticipant>`.
/// Re-export of the WA business-profile wire type (already
/// `serde::Serialize`) so the runtime can return it directly
/// without depending on wacore.
pub use wacore::iq::business::BusinessProfile;
/// Re-export of the protobuf crate so runtime-layer handlers can
/// reach into waproto for enum values that don't have a flat
/// snapshot equivalent (e.g. `EventResponseType`).
pub use waproto;
pub use whatsapp_rust::{GroupMetadata, GroupParticipant, ParticipantChangeResponse};

// ── Phase 2 RPC payload types ──────────────────────────────────────
//
// Defined here (not inside `inherent` / `adapter`) so RPC handlers
// (Tasks 36-50) can import them as `octo_adapter_whatsapp::MessageHit`
// etc. without depending on either implementation module.

/// A single hit returned from [`WhatsAppWebAdapter::message_search`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MessageHit {
    /// WhatsApp message ID of the hit.
    pub msg_id: String,
    /// Peer JID (`<digits>@s.whatsapp.net` or `<digits>@g.us`).
    pub peer: String,
    /// Timestamp (epoch seconds) of the hit.
    pub ts: i64,
    /// Short text snippet (truncated for transport).
    pub snippet: String,
}

/// Metadata for a chat (1:1 or group).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatInfo {
    /// Chat JID.
    pub jid: String,
    /// `"dm"` or `"group"`.
    pub kind: String,
    /// Display name (subject for groups; push name for 1:1). `None` if unknown.
    pub name: Option<String>,
    /// Last-activity timestamp (epoch seconds).
    pub last_activity_ts: i64,
}

/// Flattened snapshot of `wacore::iq::usync::UserInfo` returned by
/// the Tier-6 `contacts.get_user_info` RPC. Strips the `Jid` rich
/// type to a string and drops server-side error fields — the RPC
/// either succeeds (some fields may be `None`) or returns `Ok(None)`
/// for an unknown JID. Defined here so that the inherent
/// implementation in `inherent.rs` can build it without `octo-whatsapp`
/// needing to depend back on `octo-adapter-whatsapp` (already taken
/// care of via the dependency-graph inversion).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserInfoSnapshot {
    pub jid: String,
    pub lid: Option<String>,
    pub status: Option<String>,
    pub picture_id: Option<String>,
    pub is_business: bool,
    pub verified_name: Option<String>,
    pub devices: Vec<u16>,
}

/// One (category, value) pair from the privacy settings list.
/// Both fields carry the wire string (`"last"`, `"profile"`,
/// `"all"`, `"contacts"`, `"none"`, `"contact_blacklist"`, etc.) so
/// the runtime does not need to know the full PrivacyCategory /
/// PrivacyValue enums to round-trip a value.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PrivacySettingSnapshot {
    pub category: String,
    pub value: String,
}

/// Flattened newsletter metadata for the runtime. Mirrors the WA
/// crate's `NewsletterMetadata` (most fields) plus a wire-string
/// form of the `state` and `role` enums so the runtime does not
/// need to depend on those types.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NewsletterMetadataSnapshot {
    pub jid: String,
    pub name: String,
    pub description: Option<String>,
    pub subscriber_count: u64,
    pub state: String,
    pub picture_url: Option<String>,
    pub preview_url: Option<String>,
    pub invite_code: Option<String>,
    pub role: Option<String>,
    pub creation_time: Option<u64>,
}

/// One entry inside a first-party sticker pack. Mirrors the WA
/// crate's `wacore::sticker_pack::StickerPackItem` flattened to
/// primitive types so the runtime layer does not need to depend on
/// wacore re-exports. `media_key`, `file_hash`, and `enc_file_hash`
/// are base64-encoded on the wire (the WA crate returns raw `Vec<u8>`
/// because the bytes are CDN-encrypted blob keys).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StickerPackItemSnapshot {
    pub media_key_b64: Option<String>,
    pub file_hash_b64: Option<String>,
    pub enc_file_hash_b64: Option<String>,
    pub direct_path: Option<String>,
    pub url: Option<String>,
    pub file_size: Option<u64>,
    pub mimetype: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub emojis: Vec<String>,
    pub accessibility_text: Option<String>,
}

/// Flattened sticker pack returned by `media.fetch_sticker_pack`.
/// Mirrors `wacore::sticker_pack::StickerPack` so the runtime can
/// serialize without depending on the wacore type directly.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StickerPackSnapshot {
    pub sticker_pack_id: Option<String>,
    pub name: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub file_size: Option<String>,
    pub image_data_hash: Option<String>,
    pub stickers: Vec<StickerPackItemSnapshot>,
    pub animated: i32,
    pub lottie: i32,
    pub preview_image_ids: Vec<String>,
    pub tray_image_id: Option<String>,
    pub tray_image_preview: Option<String>,
}

/// One row of poll tally results — `name` is the option label (the
/// string the poll creator posted), `voters` is the canonical JID
/// of every voter that selected it (deduped, last-vote-wins per the
/// WA crate's `aggregate_votes` semantics).
///
/// Maps to `wacore::features::polls::PollOptionResult`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PollOptionResultSnapshot {
    pub name: String,
    pub voters: Vec<String>,
}

/// Convenience alias used by the Phase 2 RPC handlers and the inherent
/// methods in this crate. They are interchangeable — pick whichever is
/// clearer at the call site.
pub use octo_network::dot::error::PlatformAdapterError;

// (blank line kept for cargo fmt)

// ── SHORTCAKE_PASSKEY event-broadcast contract tests (Session 3) ───
//
// (See the test module at the end of this file — it sits past the
// Plugin ABI so clippy::items_after_test_module doesn't fire.)

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0008
}

/// Create a new adapter from JSON config bytes.
///
/// # Safety
///
/// `config` must point to a valid buffer of at least `config_len` bytes.
/// Returns null on invalid config. Caller must call `destroy_adapter` to free.
#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match WhatsAppWebAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Destroy an adapter created by `create_adapter`.
///
/// # Safety
///
/// `adapter` must be a pointer previously returned by `create_adapter`.
/// Must not be called more than once for the same pointer.
#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut WhatsAppWebAdapter);
    }
}

// ── SHORTCAKE_PASSKEY event-broadcast contract tests (Session 3) ───
//
// The adapter's `on_event` closure unconditionally forwards every
// `wacore::types::events::Event` to the `raw_event_tx` broadcast as a
// `format!("{:?}", event)` string (see `adapter.rs:1001-1002`). These
// tests pin the upstream `Debug` shape of the three SHORTCAKE_PASSKEY
// events so a future wacore bump that renames or reorders fields shows
// up as a compile/lint break here rather than silently breaking the
// connection-watcher's classifier arm in `octo-whatsapp`.
//
// The hermetic test asserts on the *stringification* (the contract that
// flows through the broadcast) rather than going through a full adapter
// instance — that keeps the test free of session-DB / `start_bot`
// dependencies and verifies the upstream `Debug` shape in one place.

#[cfg(test)]
mod passkey_event_broadcast_tests {
    use wacore::types::events::{
        Event, PairPasskeyConfirmation, PairPasskeyError, PairPasskeyRequest,
    };

    #[test]
    fn pair_passkey_request_debug_includes_payload_and_json() {
        let evt = Event::PairPasskeyRequest(
            PairPasskeyRequest::builder()
                .request_options_json(r#"{"challenge":"AA","rpId":"web.whatsapp.com"}"#.to_string())
                .build(),
        );
        let raw = format!("{evt:?}");

        // The event-variant + payload-struct name must both appear so the
        // existing classifier (`strip_prefix("Event::").unwrap_or(raw)` +
        // split-on-brace) extracts `ident = "PairPasskeyRequest"`.
        assert!(
            raw.contains("PairPasskeyRequest"),
            "missing variant/payload identifier: {raw}"
        );
        // The JSON payload must round-trip across the Debug boundary so
        // operators can scrape the broadcast channel and feed it to a QR
        // renderer / authenticator bridge. `Debug` escapes inner `"` to
        // `\"` (e.g. `\"challenge\":\"AA\"`) — the JSON braces, field
        // names, and values all survive, so we assert on substrings that
        // do not span an escape boundary.
        assert!(raw.contains("challenge"), "challenge field name: {raw}");
        assert!(raw.contains("AA"), "challenge value: {raw}");
        assert!(raw.contains("rpId"), "rpId field name: {raw}");
        assert!(raw.contains("web.whatsapp.com"), "rpId value: {raw}");
        assert!(
            raw.contains("request_options_json"),
            "payload field name: {raw}"
        );
    }

    #[test]
    fn pair_passkey_confirmation_debug_includes_code_and_flag() {
        let evt = Event::PairPasskeyConfirmation(
            PairPasskeyConfirmation::builder()
                .code("ABCD1234".to_string())
                .skip_handoff_ux(false)
                .build(),
        );
        let raw = format!("{evt:?}");

        assert!(raw.contains("PairPasskeyConfirmation"), "raw: {raw}");
        assert!(raw.contains("ABCD1234"), "code missing: {raw}");
        assert!(raw.contains("skip_handoff_ux"), "flag missing: {raw}");
    }

    #[test]
    fn pair_passkey_error_debug_includes_error_and_continuation() {
        let evt = Event::PairPasskeyError(
            PairPasskeyError::builder()
                .error("user_cancelled".to_string())
                .continuation(false)
                .build(),
        );
        let raw = format!("{evt:?}");

        assert!(raw.contains("PairPasskeyError"), "raw: {raw}");
        assert!(raw.contains("user_cancelled"), "error missing: {raw}");
        assert!(raw.contains("continuation"), "flag missing: {raw}");
    }
}
