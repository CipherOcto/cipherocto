//! First-class typed `InboundEvent` enum.
//!
//! Plan: `docs/plans/2026-07-18-whatsapp-events-first-class-overhaul.md`.
//!
//! Every wacore `Event` variant wacore itself implements is a typed
//! variant here. New wacore variants surface as
//! [`InboundEvent::Unknown`] — graceful observability, no compile
//! error. The catch-all path in `octo-adapter-whatsapp/src/adapter.rs`
//! emits `Unknown` for any wacore variant we have not yet projected.
//!
//! This module lives in `octo-adapter-whatsapp` (not `octo-whatsapp`)
//! because the adapter is the producer of typed events on the
//! `raw_event_tx` broadcast channel; the daemon crate re-exports
//! it so consumers keep using `crate::events::InboundEvent`.
//!
//! Design choices:
//! - **No Debug-string parser.** `raw_event_tx` carries
//!   `Arc<InboundEvent>` directly. Adapter match arms construct typed
//!   variants.
//! - **`serde_json::Value` for wacore payloads.** wacore types
//!   implement `Serialize` but not `Deserialize` (the upstream fork
//!   hasn't added the round-trip derive). To keep NDJSON round-trip
//!   working we store the wacore payload as `Value` and project a few
//!   indexed fields (`jid` / `peer` / `actor` / etc.) alongside it.
//!   Consumers that need typed access to a specific wacore struct can
//!   re-deserialise via `wacore`'s `serde_json::from_value` if/when
//!   `Deserialize` is added upstream.
//! - **`Unknown` is graceful.** Carries the full wacore event
//!   payload + a discriminant label so callers can route, metric,
//!   and inspect without parsing Debug output.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

// Re-export wacore's event types so consumers do not need a second
// wacore pin. Used by the adapter for `serde_json::to_value(event)` and
// by the catch-all `discriminant_label(event)` helper below.
pub use wacore::types::events as wacore_events;

/// Build a fresh monotonic-clock sample in nanoseconds.
pub fn now_mono_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Parse one NDJSON envelope into a single [`InboundEvent`]. Used by
/// the adapter-side `format!("{:?}", event)` bridge — the typed
/// adapter match arms in `octo-adapter-whatsapp/src/adapter.rs` emit
/// these Debug-formatted envelopes via `raw_event_tx.send(desc)` for
/// events we haven't yet projected into first-class variants. When
/// the projection lands, this helper (and the format-string path)
/// goes away. Until then it preserves the existing operator-visible
/// behaviour: anything we recognise routes to a typed variant;
/// anything else lands as `Unknown { wacore_event, variant_label }`
/// with the full Debug string preserved.
pub fn parse(env: EventEnvelope) -> InboundEvent {
    parse_inner(&env.raw, env.ts_unix_ms, env.ts_mono_ns)
}

/// Method-style shim for [`parse`]. Used at every test site that
/// constructs an `EventEnvelope` and feeds it through the parser.
/// Will be deleted once the adapter match arm rewrite (T05-T10)
/// lands and the Debug-string bridge is removed.
impl InboundEvent {
    /// Wrap the free function [`parse`] so callers can use
    /// `InboundEvent::parse(env)`. Inherent method takes priority
    /// over any trait-derived `parse` (clap/winnow Parser).
    pub fn parse(env: EventEnvelope) -> InboundEvent {
        parse_free(env)
    }

    /// Wrap the free function [`parse_many`].
    pub fn parse_with_now(env: EventEnvelope, now_unix_ms: i64) -> Vec<InboundEvent> {
        parse_many(env, Some(now_unix_ms))
    }
}

// Free-function alias so the inherent `InboundEvent::parse` can
// delegate without recursive recursion. Kept private to the module.
fn parse_free(env: EventEnvelope) -> InboundEvent {
    parse(env)
}

/// Parse an envelope into one or more [`InboundEvent`]s. A
/// `Messages(MessageBatch { … })` envelope fans out to one event per
/// inner message so group conversations land as searchable rows
/// instead of an opaque Unknown blob. Every other envelope produces
/// exactly one event.
pub fn parse_many(env: EventEnvelope, _now_unix_ms: Option<i64>) -> Vec<InboundEvent> {
    // The Debug-string batch path requires per-message parsing of the
    // `format!("{:?}", MessageBatch)` envelope. Until the typed
    // projection lands, we just emit a single Unknown wrapping the
    // whole batch so the messages table doesn't lose visibility —
    // the wacore batch is recovered via the HistorySync path in
    // `octo-adapter-whatsapp`.
    let raw = env.raw.trim().to_string();
    if raw.starts_with("Messages(") {
        vec![InboundEvent::Unknown {
            wacore_event: serde_json::Value::String(raw),
            variant_label: "Messages".into(),
            ts_unix_ms: env.ts_unix_ms,
            ts_mono_ns: env.ts_mono_ns,
        }]
    } else {
        vec![InboundEvent::parse(env)]
    }
}

fn parse_inner(raw: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    // Default fallback: capture the raw envelope as Unknown. The
    // adapter currently emits projections for 5 wacore variants
    // (ChatPresence → Presence, Presence → Presence, NewsletterLiveUpdate
    // → NewsletterUpdate, UndecryptableMessage → Unavailable,
    // DisappearingModeChanged → DisappearingModeChanged) so the
    // 5-prefix string match below handles those.
    let raw_owned = raw.to_string();
    // Extract the leading variant discriminant (e.g. "GroupUpdate",
    // "PictureUpdate", "PairSuccess", or a future wacore variant).
    // Falls back to "debug_fallback" when the envelope is not a
    // recognised `Variant(...)` shape (malformed input, raw node
    // passthroughs, etc.). Drives the per-variant aggregate in
    // `unknown_stats` so operators can see exactly which wacore
    // events lack typed handlers.
    let variant_label = raw_owned
        .split_once('(')
        .map(|(name, _)| name.trim().to_string())
        .unwrap_or_else(|| "debug_fallback".to_string());
    if let Some(rest) = raw.strip_prefix("Message(") {
        return parse_message(rest, ts_unix_ms, ts_mono_ns);
    }
    if let Some(rest) = raw.strip_prefix("Presence(") {
        return parse_presence(rest);
    }
    if let Some(rest) = raw.strip_prefix("Receipt(") {
        return parse_receipt(rest, ts_unix_ms, ts_mono_ns);
    }
    if let Some(rest) = raw.strip_prefix("NewsletterUpdate(") {
        return parse_newsletter_update(rest, ts_unix_ms, ts_mono_ns);
    }
    if let Some(rest) = raw.strip_prefix("Unavailable(") {
        return parse_unavailable(rest, ts_unix_ms, ts_mono_ns);
    }
    if let Some(rest) = raw.strip_prefix("DisappearingModeChanged(") {
        return parse_disappearing_mode_changed(rest, ts_unix_ms, ts_mono_ns);
    }
    InboundEvent::Unknown {
        wacore_event: serde_json::Value::String(raw_owned),
        variant_label,
        ts_unix_ms,
        ts_mono_ns,
    }
}

/// Strip surrounding quotes from a Debug-printed string literal.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && (s.starts_with('"') && s.ends_with('"') || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Find `key: value` in a Debug dump body. Conservative.
fn field(body: &str, key: &str) -> Option<String> {
    let needle = format!("{key}: ");
    body.find(&needle).map(|i| {
        let rest = &body[i + needle.len()..];
        let end = rest.find([',', '}', ')']).unwrap_or(rest.len());
        rest[..end].trim().to_string()
    })
}

fn parse_presence(rest: &str) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Available") => PresenceKind::Available,
        Some("Unavailable") => PresenceKind::Unavailable,
        Some("Typing") => PresenceKind::Typing,
        Some("Recording") => PresenceKind::Recording,
        Some("Paused") => PresenceKind::Available,
        _ => PresenceKind::Available,
    };
    InboundEvent::Presence {
        jid: unquote(&field(rest, "jid").unwrap_or_default()),
        kind,
        last_seen: field(rest, "last_seen").and_then(|v| v.parse().ok()),
    }
}

fn parse_receipt(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let kind = match field(rest, "type")
        .or_else(|| field(rest, "r#type"))
        .or_else(|| field(rest, "kind"))
        .as_deref()
    {
        Some("Read") | Some("ReadSelf") => ReceiptKind::Read,
        Some("Played") | Some("PlayedSelf") => ReceiptKind::Played,
        Some("Delivered") | Some("Sender") => ReceiptKind::Delivered,
        _ => ReceiptKind::Delivered,
    };
    let msg_id = unquote(&field(rest, "msg_id").unwrap_or_default());
    let peer = unquote(&field(rest, "peer").unwrap_or_default());
    InboundEvent::Receipt {
        msg_id,
        peer,
        kind,
        ts_unix_ms,
        ts_mono_ns,
    }
}

fn parse_newsletter_update(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Subscribed") => NewsletterUpdateKind::Subscribed,
        Some("Unsubscribed") => NewsletterUpdateKind::Unsubscribed,
        Some("MessageReceived") => NewsletterUpdateKind::MessageReceived,
        Some("PictureChanged") => NewsletterUpdateKind::PictureChanged,
        Some("NameChanged") => NewsletterUpdateKind::NameChanged,
        Some("StateChanged") => NewsletterUpdateKind::StateChanged,
        _ => NewsletterUpdateKind::Subscribed,
    };
    InboundEvent::NewsletterUpdate {
        jid: unquote(&field(rest, "jid").unwrap_or_default()),
        kind,
        ts_unix_ms,
        ts_mono_ns,
    }
}

fn parse_unavailable(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let id = unquote(&field(rest, "id").unwrap_or_default());
    let peer = unquote(&field(rest, "peer").unwrap_or_default());
    let sender = unquote(&field(rest, "sender").unwrap_or_default());
    let ts = field(rest, "ts")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(ts_unix_ms);
    let is_unavailable = field(rest, "is_unavailable")
        .map(|v| v == "true")
        .unwrap_or(true);
    let unavailable_type = match field(rest, "kind").as_deref() {
        Some("view_once") => UnavailableKind::ViewOnce,
        Some("hosted") => UnavailableKind::Hosted,
        Some("bot") => UnavailableKind::Bot,
        _ => UnavailableKind::Unknown,
    };
    InboundEvent::Unavailable {
        id,
        peer,
        sender,
        unavailable_type,
        is_unavailable,
        ts_unix_ms: ts,
        ts_mono_ns,
    }
}

fn parse_disappearing_mode_changed(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let jid = unquote(&field(rest, "jid").unwrap_or_default());
    let duration_seconds = field(rest, "duration_seconds")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let ts = field(rest, "ts")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(ts_unix_ms);
    InboundEvent::DisappearingModeChanged {
        jid,
        duration_seconds,
        ts_unix_ms: ts,
        ts_mono_ns,
    }
}

/// Parse a `Message(...)` Debug envelope into a typed `Message` event.
/// Carries the full Message fields including `view_once`, `media_token`,
/// `is_group`, `ephemeral_expires_at_seconds`. Keeps the existing
/// behaviour for keys we can parse; anything else falls into sensible
/// defaults so a partial Debug dump produces a valid event.
fn parse_message(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let id = unquote(&field(rest, "id").unwrap_or_default());
    let peer = unquote(&field(rest, "peer").unwrap_or_default());
    let sender = unquote(&field(rest, "sender").unwrap_or_default());
    let text = unquote(&field(rest, "text").unwrap_or_default());
    let kind = match field(rest, "kind").as_deref() {
        Some("Text") => MessageKind::Text,
        Some("Image") => MessageKind::Image,
        Some("Video") => MessageKind::Video,
        Some("Audio") => MessageKind::Audio,
        Some("Voice") => MessageKind::Voice,
        Some("Sticker") => MessageKind::Sticker,
        Some("Document") => MessageKind::Document,
        Some("Contact") => MessageKind::Contact,
        Some("Location") => MessageKind::Location,
        Some("Poll") => MessageKind::Poll,
        Some("Reaction") => MessageKind::Reaction,
        _ => MessageKind::Text,
    };
    let media_token = field(rest, "media_token").map(|v| unquote(&v));
    let is_group = field(rest, "is_group").as_deref() == Some("true");
    let view_once = field(rest, "view_once").as_deref() == Some("true");
    let from_me = field(rest, "from_me").as_deref() == Some("true");
    let ephemeral_expires_at_seconds =
        field(rest, "ephemeral_expires_at_seconds").and_then(|v| v.parse::<u32>().ok());
    InboundEvent::Message {
        id,
        peer,
        sender,
        ts_unix_ms,
        ts_mono_ns,
        kind,
        text,
        media_token,
        reply_to: None,
        mentions: Vec::new(),
        mentions_truncated: false,
        from_me,
        is_group,
        view_once,
        ephemeral_expires_at_seconds,
    }
}

pub const MAX_INLINE_MENTIONS: usize = 8;
pub const MAX_INLINE_TEXT_BYTES: usize = 65_536;
pub const SKEW_TOLERANCE_MS: i64 = 60_000;

/// Truncate a Debug-formatted sample to a bounded size. Used by the
/// `UnknownStats` sidecar so a runaway payload cannot blow up the
/// persistence file. 2 KiB + trailing ellipsis.
#[allow(dead_code)]
pub(crate) const UNKNOWN_SAMPLE_CAP: usize = 2048;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventEnvelope {
    pub raw: String,
    pub ts_unix_ms: i64,
    pub ts_mono_ns: u64,
}

/// First-class typed inbound event. One variant per wacore `Event`
/// variant wacore itself implements, plus `Unknown` for the catch-all
/// (graceful — never a compile error on a new wacore variant).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum InboundEvent {
    // ─── Existing projected variants (kept; rich downstream shape) ──────
    Message {
        id: String,
        peer: String,
        sender: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
        kind: MessageKind,
        #[serde(default)]
        text: String,
        #[serde(default)]
        media_token: Option<String>,
        #[serde(default)]
        reply_to: Option<String>,
        #[serde(default)]
        mentions: Vec<String>,
        #[serde(default)]
        mentions_truncated: bool,
        #[serde(default)]
        from_me: bool,
        is_group: bool,
        #[serde(default)]
        view_once: bool,
        #[serde(default)]
        ephemeral_expires_at_seconds: Option<u32>,
    },
    Reaction {
        id: String,
        target_msg_id: String,
        emoji: String,
        from: String,
        peer: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::ChatPresenceUpdate` + `Event::PresenceUpdate`.
    Presence {
        jid: String,
        kind: PresenceKind,
        #[serde(default)]
        last_seen: Option<i64>,
    },
    /// Bridged from `Event::Receipt` + `Event::ServerAck(class=message)`.
    Receipt {
        msg_id: String,
        peer: String,
        kind: ReceiptKind,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    Story {
        id: String,
        peer: String,
        kind: StoryKind,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Echo of a community RPC success.
    CommunityUpdate {
        jid: String,
        kind: CommunityUpdateKind,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::NewsletterLiveUpdate`.
    NewsletterUpdate {
        jid: String,
        kind: NewsletterUpdateKind,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::UndecryptableMessage`.
    Unavailable {
        id: String,
        peer: String,
        sender: String,
        unavailable_type: UnavailableKind,
        is_unavailable: bool,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Mirrors `Event::DisappearingModeChanged`.
    DisappearingModeChanged {
        jid: String,
        duration_seconds: u32,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::PairingQrCode`.
    PairingQrCode {
        qr_code: String,
        ref_string: String,
        timeout: u64,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::PairingCode`.
    PairingCode {
        code: String,
        timeout: u64,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::PairPasskeyRequest`.
    PairPasskeyRequest {
        auth: String,
        request_json: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::PairPasskeyConfirmation`.
    PairPasskeyConfirmation {
        auth: String,
        confirmation_json: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Bridged from `Event::PairPasskeyError`.
    PairPasskeyError {
        auth: String,
        error_json: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },

    // ─── New: 1:1 wrappers for wacore-implemented variants ──────────────
    // Each variant carries the full wacore serialised payload plus a few
    // indexed projection fields (typically JID + actor + action kind) so
    // downstream tooling can filter / group without parsing the payload.
    // Variant label is implicit in the enum discriminant.
    GroupUpdate {
        group_jid: String,
        participant: Option<String>,
        action_kind: String,
        /// Full serialised wacore `Event::GroupUpdate` payload.
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    IncomingCall {
        peer: String,
        kind: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    MissedCall {
        peer: String,
        reason: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    CallEndedElsewhere {
        peer: String,
        outcome: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// `Event::Disconnected` — websocket dropped. No payload.
    Disconnected {
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// `Event::StreamReplaced` — another device took over.
    StreamReplaced {
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    TemporaryBan {
        reason: String,
        expires_at_ms: i64,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ConnectFailure {
        reason: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    Connected {
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    LoggedOut {
        cause: Option<String>,
        on_connect: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    PictureUpdate {
        jid: String,
        author: Option<String>,
        removed: bool,
        picture_id: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    UserAboutUpdate {
        jid: String,
        status: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    PushNameUpdate {
        jid: String,
        old_push_name: String,
        new_push_name: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    SelfPushNameUpdated {
        new_push_name: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ContactUpdated {
        jid: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ContactNumberChanged {
        jid: String,
        old_phone: Option<String>,
        new_phone: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ContactSyncRequested {
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ContactUpdate {
        jid: String,
        from_full_sync: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    PinUpdate {
        jid: String,
        pinned: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    MuteUpdate {
        jid: String,
        muted: bool,
        mute_expires_at_ms: Option<i64>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ArchiveUpdate {
        jid: String,
        archived: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    StarUpdate {
        jid: String,
        msg_id: String,
        pinned: bool,
        starred: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    MarkChatAsReadUpdate {
        jid: String,
        read_until_msg_id: Option<String>,
        unread_count: Option<u32>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    DeleteChatUpdate {
        jid: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ClearChatUpdate {
        jid: String,
        msg_count: Option<u32>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    UserStatusMuteUpdate {
        jid: String,
        muted: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    DeleteMessageForMeUpdate {
        jid: String,
        msg_id: String,
        only_me: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ServerAck {
        msg_id: Option<String>,
        peer: Option<String>,
        ack_class: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    DeviceListUpdate {
        user: String,
        device_count: usize,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    IdentityChange {
        timestamp_ms: i64,
        jid: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    LabelEditUpdate {
        label_id: u32,
        name: String,
        color: Option<u32>,
        deleted: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    LabelAssociationUpdate {
        label_id: u32,
        chat_jid: String,
        labeled: bool,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    PairSuccess {
        device_id: u32,
        business_name: Option<String>,
        platform: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    PairError {
        code: i32,
        message: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    PairingCodeRefresh {
        code: String,
        timeout_seconds: u64,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    QrScannedWithoutMultidevice {
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    ClientOutdated {
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    BusinessStatusUpdate {
        jid: String,
        status_kind: String,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    MexNotification {
        payload_kind: String,
        agent_id: Option<String>,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    OfflineSyncPreview {
        total: u32,
        received: u32,
        app_data_synced: bool,
        peer_count: u32,
        payload: serde_json::Value,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },

    // ─── Unknown (graceful catch-all; never a compile error) ───────────
    /// Emitted by the adapter for wacore events we haven't projected
    /// yet. Carries the full wacore event so future code can
    /// re-extract fields when a typed variant is added. Also emitted
    /// for the two known wacore-unimplemented cases
    /// (`Event::Notification` / `Event::RawNode`).
    Unknown {
        /// Serialised wacore event (`serde_json::Value` so the variant
        /// round-trips through NDJSON without requiring every wacore
        /// type's serde bounds).
        wacore_event: serde_json::Value,
        /// Discriminant label from `wacore::types::events::Event`
        /// (`"GroupUpdate"`, `"PictureUpdate"`, …). Used by metrics +
        /// `events.unknown_stats` for sorting / display.
        variant_label: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
}

// ─── Sub-enums (preserved where projected variants still need them) ──

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Image,
    Video,
    Audio,
    Voice,
    Sticker,
    Document,
    Contact,
    Location,
    Poll,
    Reaction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresenceKind {
    Available,
    Unavailable,
    Typing,
    Recording,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    Read,
    Delivered,
    Played,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryKind {
    Posted,
    Viewed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommunityUpdateKind {
    Created,
    Deactivated,
    Linked,
    Unlinked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NewsletterUpdateKind {
    Subscribed,
    Unsubscribed,
    MessageReceived,
    PictureChanged,
    NameChanged,
    StateChanged,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableKind {
    Unknown,
    ViewOnce,
    Hosted,
    Bot,
}

impl UnavailableKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::ViewOnce => "view_once",
            Self::Hosted => "hosted",
            Self::Bot => "bot",
        }
    }
}

impl InboundEvent {
    /// Truncate mention list per design doc §InboundEvent bounding.
    pub fn bound_mentions(mentions: Vec<String>) -> (Vec<String>, bool) {
        if mentions.len() > MAX_INLINE_MENTIONS {
            let mut kept = mentions;
            kept.truncate(MAX_INLINE_MENTIONS);
            (kept, true)
        } else {
            (mentions, false)
        }
    }

    /// Truncate text payload per design doc §InboundEvent bounding.
    pub fn bound_text(text: String) -> String {
        if text.len() > MAX_INLINE_TEXT_BYTES {
            let mut t = text;
            t.truncate(MAX_INLINE_TEXT_BYTES);
            t
        } else {
            text
        }
    }

    /// Wall-clock timestamp (Unix milliseconds). Every variant except
    /// `Presence` (which exposes `last_seen` instead) carries one.
    pub fn ts_unix_ms(&self) -> i64 {
        match self {
            Self::Message { ts_unix_ms, .. }
            | Self::Reaction { ts_unix_ms, .. }
            | Self::Receipt { ts_unix_ms, .. }
            | Self::Story { ts_unix_ms, .. }
            | Self::CommunityUpdate { ts_unix_ms, .. }
            | Self::NewsletterUpdate { ts_unix_ms, .. }
            | Self::Unavailable { ts_unix_ms, .. }
            | Self::DisappearingModeChanged { ts_unix_ms, .. }
            | Self::PairingQrCode { ts_unix_ms, .. }
            | Self::PairingCode { ts_unix_ms, .. }
            | Self::PairPasskeyRequest { ts_unix_ms, .. }
            | Self::PairPasskeyConfirmation { ts_unix_ms, .. }
            | Self::PairPasskeyError { ts_unix_ms, .. }
            | Self::GroupUpdate { ts_unix_ms, .. }
            | Self::IncomingCall { ts_unix_ms, .. }
            | Self::MissedCall { ts_unix_ms, .. }
            | Self::CallEndedElsewhere { ts_unix_ms, .. }
            | Self::Disconnected { ts_unix_ms, .. }
            | Self::StreamReplaced { ts_unix_ms, .. }
            | Self::TemporaryBan { ts_unix_ms, .. }
            | Self::ConnectFailure { ts_unix_ms, .. }
            | Self::Connected { ts_unix_ms, .. }
            | Self::LoggedOut { ts_unix_ms, .. }
            | Self::PictureUpdate { ts_unix_ms, .. }
            | Self::UserAboutUpdate { ts_unix_ms, .. }
            | Self::PushNameUpdate { ts_unix_ms, .. }
            | Self::SelfPushNameUpdated { ts_unix_ms, .. }
            | Self::ContactUpdated { ts_unix_ms, .. }
            | Self::ContactNumberChanged { ts_unix_ms, .. }
            | Self::ContactSyncRequested { ts_unix_ms, .. }
            | Self::ContactUpdate { ts_unix_ms, .. }
            | Self::PinUpdate { ts_unix_ms, .. }
            | Self::MuteUpdate { ts_unix_ms, .. }
            | Self::ArchiveUpdate { ts_unix_ms, .. }
            | Self::StarUpdate { ts_unix_ms, .. }
            | Self::MarkChatAsReadUpdate { ts_unix_ms, .. }
            | Self::DeleteChatUpdate { ts_unix_ms, .. }
            | Self::ClearChatUpdate { ts_unix_ms, .. }
            | Self::UserStatusMuteUpdate { ts_unix_ms, .. }
            | Self::DeleteMessageForMeUpdate { ts_unix_ms, .. }
            | Self::ServerAck { ts_unix_ms, .. }
            | Self::DeviceListUpdate { ts_unix_ms, .. }
            | Self::IdentityChange { ts_unix_ms, .. }
            | Self::LabelEditUpdate { ts_unix_ms, .. }
            | Self::LabelAssociationUpdate { ts_unix_ms, .. }
            | Self::PairSuccess { ts_unix_ms, .. }
            | Self::PairError { ts_unix_ms, .. }
            | Self::PairingCodeRefresh { ts_unix_ms, .. }
            | Self::QrScannedWithoutMultidevice { ts_unix_ms, .. }
            | Self::ClientOutdated { ts_unix_ms, .. }
            | Self::BusinessStatusUpdate { ts_unix_ms, .. }
            | Self::MexNotification { ts_unix_ms, .. }
            | Self::OfflineSyncPreview { ts_unix_ms, .. }
            | Self::Unknown { ts_unix_ms, .. } => *ts_unix_ms,
            Self::Presence { last_seen, .. } => last_seen.unwrap_or(0),
        }
    }

    /// Monotonic timestamp (nanoseconds since boot).
    pub fn ts_mono_ns(&self) -> Option<u64> {
        match self {
            Self::Message { ts_mono_ns, .. }
            | Self::Reaction { ts_mono_ns, .. }
            | Self::Receipt { ts_mono_ns, .. }
            | Self::Story { ts_mono_ns, .. }
            | Self::CommunityUpdate { ts_mono_ns, .. }
            | Self::NewsletterUpdate { ts_mono_ns, .. }
            | Self::Unavailable { ts_mono_ns, .. }
            | Self::DisappearingModeChanged { ts_mono_ns, .. }
            | Self::PairingQrCode { ts_mono_ns, .. }
            | Self::PairingCode { ts_mono_ns, .. }
            | Self::PairPasskeyRequest { ts_mono_ns, .. }
            | Self::PairPasskeyConfirmation { ts_mono_ns, .. }
            | Self::PairPasskeyError { ts_mono_ns, .. }
            | Self::GroupUpdate { ts_mono_ns, .. }
            | Self::IncomingCall { ts_mono_ns, .. }
            | Self::MissedCall { ts_mono_ns, .. }
            | Self::CallEndedElsewhere { ts_mono_ns, .. }
            | Self::Disconnected { ts_mono_ns, .. }
            | Self::StreamReplaced { ts_mono_ns, .. }
            | Self::TemporaryBan { ts_mono_ns, .. }
            | Self::ConnectFailure { ts_mono_ns, .. }
            | Self::Connected { ts_mono_ns, .. }
            | Self::LoggedOut { ts_mono_ns, .. }
            | Self::PictureUpdate { ts_mono_ns, .. }
            | Self::UserAboutUpdate { ts_mono_ns, .. }
            | Self::PushNameUpdate { ts_mono_ns, .. }
            | Self::SelfPushNameUpdated { ts_mono_ns, .. }
            | Self::ContactUpdated { ts_mono_ns, .. }
            | Self::ContactNumberChanged { ts_mono_ns, .. }
            | Self::ContactSyncRequested { ts_mono_ns, .. }
            | Self::ContactUpdate { ts_mono_ns, .. }
            | Self::PinUpdate { ts_mono_ns, .. }
            | Self::MuteUpdate { ts_mono_ns, .. }
            | Self::ArchiveUpdate { ts_mono_ns, .. }
            | Self::StarUpdate { ts_mono_ns, .. }
            | Self::MarkChatAsReadUpdate { ts_mono_ns, .. }
            | Self::DeleteChatUpdate { ts_mono_ns, .. }
            | Self::ClearChatUpdate { ts_mono_ns, .. }
            | Self::UserStatusMuteUpdate { ts_mono_ns, .. }
            | Self::DeleteMessageForMeUpdate { ts_mono_ns, .. }
            | Self::ServerAck { ts_mono_ns, .. }
            | Self::DeviceListUpdate { ts_mono_ns, .. }
            | Self::IdentityChange { ts_mono_ns, .. }
            | Self::LabelEditUpdate { ts_mono_ns, .. }
            | Self::LabelAssociationUpdate { ts_mono_ns, .. }
            | Self::PairSuccess { ts_mono_ns, .. }
            | Self::PairError { ts_mono_ns, .. }
            | Self::PairingCodeRefresh { ts_mono_ns, .. }
            | Self::QrScannedWithoutMultidevice { ts_mono_ns, .. }
            | Self::ClientOutdated { ts_mono_ns, .. }
            | Self::BusinessStatusUpdate { ts_mono_ns, .. }
            | Self::MexNotification { ts_mono_ns, .. }
            | Self::OfflineSyncPreview { ts_mono_ns, .. }
            | Self::Unknown { ts_mono_ns, .. } => Some(*ts_mono_ns),
            Self::Presence { .. } => None,
        }
    }

    /// Stable per-event-kind label used as the
    /// `inbound_events_total{kind}` Prometheus label.
    pub fn event_kind(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
            Self::Reaction { .. } => "reaction",
            Self::Presence { .. } => "presence",
            Self::Receipt { .. } => "receipt",
            Self::Story { .. } => "story",
            Self::CommunityUpdate { .. } => "community_update",
            Self::NewsletterUpdate { .. } => "newsletter_update",
            Self::Unavailable { .. } => "unavailable",
            Self::DisappearingModeChanged { .. } => "disappearing_mode_changed",
            Self::PairingQrCode { .. } => "pairing_qr_code",
            Self::PairingCode { .. } => "pairing_code",
            Self::PairPasskeyRequest { .. } => "pair_passkey_request",
            Self::PairPasskeyConfirmation { .. } => "pair_passkey_confirmation",
            Self::PairPasskeyError { .. } => "pair_passkey_error",
            Self::GroupUpdate { .. } => "group_update",
            Self::IncomingCall { .. } => "incoming_call",
            Self::MissedCall { .. } => "missed_call",
            Self::CallEndedElsewhere { .. } => "call_ended_elsewhere",
            Self::Disconnected { .. } => "disconnected",
            Self::StreamReplaced { .. } => "stream_replaced",
            Self::TemporaryBan { .. } => "temporary_ban",
            Self::ConnectFailure { .. } => "connect_failure",
            Self::Connected { .. } => "connected",
            Self::LoggedOut { .. } => "logged_out",
            Self::PictureUpdate { .. } => "picture_update",
            Self::UserAboutUpdate { .. } => "user_about_update",
            Self::PushNameUpdate { .. } => "push_name_update",
            Self::SelfPushNameUpdated { .. } => "self_push_name_updated",
            Self::ContactUpdated { .. } => "contact_updated",
            Self::ContactNumberChanged { .. } => "contact_number_changed",
            Self::ContactSyncRequested { .. } => "contact_sync_requested",
            Self::ContactUpdate { .. } => "contact_update",
            Self::PinUpdate { .. } => "pin_update",
            Self::MuteUpdate { .. } => "mute_update",
            Self::ArchiveUpdate { .. } => "archive_update",
            Self::StarUpdate { .. } => "star_update",
            Self::MarkChatAsReadUpdate { .. } => "mark_chat_as_read_update",
            Self::DeleteChatUpdate { .. } => "delete_chat_update",
            Self::ClearChatUpdate { .. } => "clear_chat_update",
            Self::UserStatusMuteUpdate { .. } => "user_status_mute_update",
            Self::DeleteMessageForMeUpdate { .. } => "delete_message_for_me_update",
            Self::ServerAck { .. } => "server_ack",
            Self::DeviceListUpdate { .. } => "device_list_update",
            Self::IdentityChange { .. } => "identity_change",
            Self::LabelEditUpdate { .. } => "label_edit_update",
            Self::LabelAssociationUpdate { .. } => "label_association_update",
            Self::PairSuccess { .. } => "pair_success",
            Self::PairError { .. } => "pair_error",
            Self::PairingCodeRefresh { .. } => "pairing_code_refresh",
            Self::QrScannedWithoutMultidevice { .. } => "qr_scanned_without_multidevice",
            Self::ClientOutdated { .. } => "client_outdated",
            Self::BusinessStatusUpdate { .. } => "business_status_update",
            Self::MexNotification { .. } => "mex_notification",
            Self::OfflineSyncPreview { .. } => "offline_sync_preview",
            Self::Unknown { .. } => "unknown",
        }
    }

    /// `true` when the variant carries a `serde_json::Value` payload
    /// that may grow large (the `Unknown` variant + all 1:1 wrappers).
    /// Used by the persister to decide whether to truncate before
    /// writing to disk.
    pub fn has_unbounded_payload(&self) -> bool {
        matches!(
            self,
            Self::Unknown { .. }
                | Self::GroupUpdate { .. }
                | Self::IncomingCall { .. }
                | Self::MissedCall { .. }
                | Self::CallEndedElsewhere { .. }
                | Self::TemporaryBan { .. }
                | Self::ConnectFailure { .. }
                | Self::LoggedOut { .. }
                | Self::PictureUpdate { .. }
                | Self::UserAboutUpdate { .. }
                | Self::PushNameUpdate { .. }
                | Self::SelfPushNameUpdated { .. }
                | Self::ContactUpdated { .. }
                | Self::ContactNumberChanged { .. }
                | Self::ContactSyncRequested { .. }
                | Self::ContactUpdate { .. }
                | Self::PinUpdate { .. }
                | Self::MuteUpdate { .. }
                | Self::ArchiveUpdate { .. }
                | Self::StarUpdate { .. }
                | Self::MarkChatAsReadUpdate { .. }
                | Self::DeleteChatUpdate { .. }
                | Self::ClearChatUpdate { .. }
                | Self::UserStatusMuteUpdate { .. }
                | Self::DeleteMessageForMeUpdate { .. }
                | Self::ServerAck { .. }
                | Self::DeviceListUpdate { .. }
                | Self::IdentityChange { .. }
                | Self::LabelEditUpdate { .. }
                | Self::LabelAssociationUpdate { .. }
                | Self::PairSuccess { .. }
                | Self::PairError { .. }
                | Self::PairingCodeRefresh { .. }
                | Self::BusinessStatusUpdate { .. }
                | Self::MexNotification { .. }
                | Self::OfflineSyncPreview { .. }
        )
    }

    /// Build an `InboundEvent::Unknown` from a wacore `Event` (used by
    /// the adapter's catch-all arm and by the explicit Notification /
    /// RawNode arms). The `wacore_event` is serialised via
    /// `serde_json::to_value` so the full payload survives NDJSON
    /// round-trip. Falls back to a string-form `Value` if
    /// serialisation fails (rare — only for wacore types with
    /// non-Serialize fields).
    pub fn unknown_from_wacore(
        event: &wacore_events::Event,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    ) -> Self {
        let variant_label = discriminant_label(event).to_string();
        let wacore_event = serde_json::to_value(event).unwrap_or_else(|e| {
            tracing::warn!(
                variant = %variant_label,
                error = %e,
                "InboundEvent::unknown_from_wacore: serde_json::to_value failed; falling back to Debug string"
            );
            serde_json::Value::String(format!("{event:?}"))
        });
        Self::Unknown {
            wacore_event,
            variant_label,
            ts_unix_ms,
            ts_mono_ns,
        }
    }
    /// Build a typed `InboundEvent::Message` that represents a message
    /// we dispatched ourselves. Used by `send.*` IPC handlers in
    /// `octo-whatsapp` to surface every outbound dispatch in the
    /// daemon's events table — independently of WA's own inbox-echo
    /// behaviour (unreliable for self-sends on single-device sessions
    /// and filtered by `accept_message` on live-test fixtures).
    pub fn from_outbound_text(
        message_id: String,
        peer: String,
        self_jid: String,
        text: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    ) -> Self {
        let text = Self::bound_text(text);
        Self::Message {
            id: message_id,
            peer,
            sender: self_jid,
            ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Text,
            text,
            media_token: None,
            reply_to: None,
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me: true,
            is_group: false,
            view_once: false,
            ephemeral_expires_at_seconds: None,
        }
    }

    /// Mirror of [`Self::from_outbound_text`] for media dispatches.
    /// Accepts an optional `caption` (text) and `media_token` (the
    /// media-ref token returned by the adapter dispatch) so the events
    /// table carries enough metadata for downstream consumers to
    /// surface the dispatch without re-querying the adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn from_outbound_media(
        message_id: String,
        peer: String,
        self_jid: String,
        kind: MessageKind,
        caption: Option<String>,
        media_token: Option<String>,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    ) -> Self {
        let text = Self::bound_text(caption.unwrap_or_default());
        Self::Message {
            id: message_id,
            peer,
            sender: self_jid,
            ts_unix_ms,
            ts_mono_ns,
            kind,
            text,
            media_token,
            reply_to: None,
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me: true,
            is_group: false,
            view_once: false,
            ephemeral_expires_at_seconds: None,
        }
    }

    /// Construct a synthetic `InboundEvent::Unknown` from textual
    /// payload. Used by tests + a handful of internal sites that
    /// never see a real wacore event but want to push something onto
    /// the events ring for downstream consumers. `text` is anything
    /// string-like (`String`, `&&str`, `Cow<str>`) — the function
    /// converts to `String` internally.
    pub fn synthetic_unknown(label: impl Into<String>, text: impl Into<String>) -> Self {
        Self::Unknown {
            wacore_event: serde_json::Value::String(text.into()),
            variant_label: label.into(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        }
    }
}

/// Extract the wacore `Event` discriminant label (e.g. `"GroupUpdate"`,
/// `"PictureUpdate"`). Used by the catch-all unknown-event path so
/// every emission has a stable per-variant metric + per-variant
/// `unknown_stats` entry.
/// to add variants without a SemVer bump), so this match has a
/// defensive `_` arm that returns `"Unknown"` rather than `unreachable!`
/// — the wacore variant surface is observable as `Unknown` either way.
pub fn known_kinds() -> Vec<&'static str> {
    // Stable order — one entry per typed variant. `Unknown` is
    // listed last because it's the catch-all (graceful, never a
    // compile error on a new wacore variant). Total: 56 typed +
    // 1 catch-all.
    vec![
        "message",
        "reaction",
        "presence",
        "receipt",
        "story",
        "community_update",
        "newsletter_update",
        "unavailable",
        "disappearing_mode_changed",
        "pairing_qr_code",
        "pairing_code",
        "pair_passkey_request",
        "pair_passkey_confirmation",
        "pair_passkey_error",
        "group_update",
        "incoming_call",
        "missed_call",
        "call_ended_elsewhere",
        "disconnected",
        "stream_replaced",
        "temporary_ban",
        "connect_failure",
        "connected",
        "logged_out",
        "picture_update",
        "user_about_update",
        "push_name_update",
        "self_push_name_updated",
        "contact_updated",
        "contact_number_changed",
        "contact_sync_requested",
        "contact_update",
        "pin_update",
        "mute_update",
        "archive_update",
        "star_update",
        "mark_chat_as_read_update",
        "delete_chat_update",
        "clear_chat_update",
        "user_status_mute_update",
        "delete_message_for_me_update",
        "server_ack",
        "device_list_update",
        "identity_change",
        "label_edit_update",
        "label_association_update",
        "pair_success",
        "pair_error",
        "pairing_code_refresh",
        "qr_scanned_without_multidevice",
        "client_outdated",
        "business_status_update",
        "mex_notification",
        "offline_sync_preview",
        "unknown",
    ]
}
pub fn discriminant_label(event: &wacore_events::Event) -> &'static str {
    use wacore_events::Event;
    match event {
        Event::Connected(_) => "Connected",
        Event::Disconnected(_) => "Disconnected",
        Event::PairSuccess(_) => "PairSuccess",
        Event::PairError(_) => "PairError",
        Event::LoggedOut(_) => "LoggedOut",
        Event::PairingQrCode(_) => "PairingQrCode",
        Event::PairingCode(_) => "PairingCode",
        Event::PairingCodeRefresh(_) => "PairingCodeRefresh",
        Event::QrScannedWithoutMultidevice(_) => "QrScannedWithoutMultidevice",
        Event::ClientOutdated(_) => "ClientOutdated",
        Event::Messages(_) => "Messages",
        Event::Receipt(_) => "Receipt",
        Event::ServerAck(_) => "ServerAck",
        Event::UndecryptableMessage(_) => "UndecryptableMessage",
        Event::Notification(_) => "Notification",
        Event::ChatPresence(_) => "ChatPresence",
        Event::Presence(_) => "Presence",
        Event::PictureUpdate(_) => "PictureUpdate",
        Event::UserAboutUpdate(_) => "UserAboutUpdate",
        Event::ContactUpdated(_) => "ContactUpdated",
        Event::ContactNumberChanged(_) => "ContactNumberChanged",
        Event::ContactSyncRequested(_) => "ContactSyncRequested",
        Event::GroupUpdate(_) => "GroupUpdate",
        Event::ContactUpdate(_) => "ContactUpdate",
        Event::IncomingCall(_) => "IncomingCall",
        Event::MissedCall(_) => "MissedCall",
        Event::CallEndedElsewhere(_) => "CallEndedElsewhere",
        Event::PushNameUpdate(_) => "PushNameUpdate",
        Event::SelfPushNameUpdated(_) => "SelfPushNameUpdated",
        Event::PinUpdate(_) => "PinUpdate",
        Event::MuteUpdate(_) => "MuteUpdate",
        Event::ArchiveUpdate(_) => "ArchiveUpdate",
        Event::StarUpdate(_) => "StarUpdate",
        Event::MarkChatAsReadUpdate(_) => "MarkChatAsReadUpdate",
        Event::DeleteChatUpdate(_) => "DeleteChatUpdate",
        Event::ClearChatUpdate(_) => "ClearChatUpdate",
        Event::UserStatusMuteUpdate(_) => "UserStatusMuteUpdate",
        Event::DeleteMessageForMeUpdate(_) => "DeleteMessageForMeUpdate",
        Event::LabelEditUpdate(_) => "LabelEditUpdate",
        Event::LabelAssociationUpdate(_) => "LabelAssociationUpdate",
        Event::HistorySync(_) => "HistorySync",
        Event::OfflineSyncPreview(_) => "OfflineSyncPreview",
        Event::OfflineSyncCompleted(_) => "OfflineSyncCompleted",
        Event::DeviceListUpdate(_) => "DeviceListUpdate",
        Event::IdentityChange(_) => "IdentityChange",
        Event::BusinessStatusUpdate(_) => "BusinessStatusUpdate",
        Event::StreamReplaced(_) => "StreamReplaced",
        Event::TemporaryBan(_) => "TemporaryBan",
        Event::ConnectFailure(_) => "ConnectFailure",
        Event::StreamError(_) => "StreamError",
        Event::DisappearingModeChanged(_) => "DisappearingModeChanged",
        Event::NewsletterLiveUpdate(_) => "NewsletterLiveUpdate",
        Event::RawNode(_) => "RawNode",
        Event::MexNotification(_) => "MexNotification",
        Event::PairPasskeyRequest(_) => "PairPasskeyRequest",
        Event::PairPasskeyConfirmation(_) => "PairPasskeyConfirmation",
        Event::PairPasskeyError(_) => "PairPasskeyError",
        // Future-proof arm: wacore is `#[non_exhaustive]`, so any new
        // variant that lands in upstream before we project it here
        // surfaces in metrics + unknown_stats under the literal
        // label `"Unknown"` (not a panic, not an `unreachable!`).
        // When the upstream variant is mapped, add an explicit arm
        // here and remove the catch-all.
        _ => "Unknown",
    }
}

// Silences unused-import lint when `Arc` is referenced only via the
// discriminant_label helper or the call-site adapter (which lives in
// the sibling crate).
#[allow(dead_code)]
fn _arc_marker(_: Arc<()>) {}
