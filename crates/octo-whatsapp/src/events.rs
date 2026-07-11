//! Typed inbound event model + parser. Phase 3: full 8-variant parser
//! for the adapter's `raw_event_tx` broadcast (lossy by design — see
//! `adapter.rs` capacity 1000 + `RecvError::Lagged(n)`).
//!
//! Source format: `format!("{:?}", ev)` produced by the adapter's
//! `on_event` closure. The parser matches variant names from
//! `wacore::types::events::Event` and routes to the typed
//! `InboundEvent` enum below. Anything unrecognised falls through to
//! `Unknown` (the design doc's canonical fallback).

use serde::{Deserialize, Serialize};

/// Build a fresh monotonic-clock sample in nanoseconds. Used by
/// `from_outbound_*` constructors and by the parser path; tests inject
/// the source sample directly so they don't depend on `Instant::now`
/// drift.
pub fn now_mono_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Maximum number of mentions kept inline. The design says "longer
/// mention lists truncate with a `mentions_truncated=true` flag."
pub const MAX_INLINE_MENTIONS: usize = 8;

/// Maximum text payload size retained in events. Full text is available
/// via `messages.get` for messages with longer bodies.
pub const MAX_INLINE_TEXT_BYTES: usize = 65_536;

/// Wall-clock skew threshold: events with `ts > now() + SKEW_TOLERANCE_MS`
/// are flagged `untrusted=true` per design §Timestamp policy.
pub const SKEW_TOLERANCE_MS: i64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventEnvelope {
    pub raw: String,
    pub ts_unix_ms: i64,
    pub ts_mono_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum InboundEvent {
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
        /// `true` for messages we dispatched ourselves (synthesized
        /// into the events table by the `send.*` IPC handlers after a
        /// successful adapter dispatch). `false` for inbound messages
        /// arriving from a peer via the WA websocket. Defaults to
        /// `false` on deserialization to preserve NDJSON back-compat
        /// with event records written before this field existed.
        #[serde(default)]
        from_me: bool,
        is_group: bool,
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
    GroupChange {
        group_jid: String,
        kind: GroupChangeKind,
        actor: Option<String>,
        target: Option<String>,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
        #[serde(default)]
        after: Option<String>,
    },
    Presence {
        jid: String,
        kind: PresenceKind,
        #[serde(default)]
        last_seen: Option<i64>,
    },
    Connection {
        kind: ConnectionKind,
        #[serde(default)]
        cause: Option<LoggedOutCause>,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    Receipt {
        msg_id: String,
        peer: String,
        kind: ReceiptKind,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    Call {
        id: String,
        peer: String,
        kind: CallKind,
        state: CallState,
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
    Unknown {
        raw: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
        #[serde(default)]
        untrusted: bool,
    },
}

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
pub enum GroupChangeKind {
    Join,
    Leave,
    Promote,
    Demote,
    Subject,
    Icon,
    Description,
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
pub enum ConnectionKind {
    Connected,
    Disconnected,
    Replaced,
    LoggedOut,
    Synced,
    ClockSkewDetected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoggedOutCause {
    UserInitiated,
    SessionReplaced,
    ProtocolError,
    Unknown,
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
pub enum CallKind {
    Voice,
    Video,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    Offered,
    Accepted,
    Rejected,
    Terminated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryKind {
    Posted,
    Viewed,
}

impl InboundEvent {
    /// Public entry point. Dispatches to the inner parser.
    pub fn parse(env: EventEnvelope) -> Self {
        Self::parse_inner(&env.raw, env.ts_unix_ms, env.ts_mono_ns, None)
    }

    /// Parser with optional skew-detection context. When
    /// `now_unix_ms` is supplied, events with `ts_unix_ms > now + SKEW_TOLERANCE_MS`
    /// get the `untrusted=true` flag (Unknown variant) or are passed through
    /// unchanged for typed variants (the consumer can check `ts_unix_ms`).
    pub fn parse_with_now(env: EventEnvelope, now_unix_ms: i64) -> Self {
        Self::parse_inner(&env.raw, env.ts_unix_ms, env.ts_mono_ns, Some(now_unix_ms))
    }

    /// Wall-clock timestamp (Unix milliseconds). Every variant carries
    /// one except `Presence`, which exposes `last_seen` instead.
    /// Falls back to `0` for `Presence` events without a `last_seen`.
    pub fn ts_unix_ms(&self) -> i64 {
        match self {
            Self::Message { ts_unix_ms, .. }
            | Self::Reaction { ts_unix_ms, .. }
            | Self::GroupChange { ts_unix_ms, .. }
            | Self::Connection { ts_unix_ms, .. }
            | Self::Receipt { ts_unix_ms, .. }
            | Self::Call { ts_unix_ms, .. }
            | Self::Story { ts_unix_ms, .. }
            | Self::Unknown { ts_unix_ms, .. } => *ts_unix_ms,
            Self::Presence { last_seen, .. } => last_seen.unwrap_or(0),
        }
    }

    /// Monotonic timestamp (nanoseconds since boot). Every variant
    /// carries one except `Presence` (which has only `last_seen` wall).
    pub fn ts_mono_ns(&self) -> Option<u64> {
        match self {
            Self::Message { ts_mono_ns, .. }
            | Self::Reaction { ts_mono_ns, .. }
            | Self::GroupChange { ts_mono_ns, .. }
            | Self::Connection { ts_mono_ns, .. }
            | Self::Receipt { ts_mono_ns, .. }
            | Self::Call { ts_mono_ns, .. }
            | Self::Story { ts_mono_ns, .. }
            | Self::Unknown { ts_mono_ns, .. } => Some(*ts_mono_ns),
            Self::Presence { .. } => None,
        }
    }

    /// True if the event's wall-clock timestamp is more than
    /// `SKEW_TOLERANCE_MS` in the future relative to `now_unix_ms`.
    pub fn is_untrusted(&self, now_unix_ms: i64) -> bool {
        let ts = self.ts_unix_ms();
        ts > now_unix_ms.saturating_add(SKEW_TOLERANCE_MS)
    }

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

    /// Build a typed `InboundEvent::Message` that represents a message
    /// we dispatched ourselves. Used by the `send.*` IPC handlers in
    /// `octo-whatsapp` to surface every outbound dispatch in the daemon's
    /// events table — independently of WA's own inbox-echo behaviour,
    /// which is unreliable for self-sends on single-device sessions and
    /// filtered (1:1 vs configured groups) by the adapter's `accept_message`
    /// for live-test fixtures. Operator mandate: every dispatched text
    /// must surface in the events table so every linked WA client can
    /// mirror the bubble.
    ///
    /// `peer` is the JID the user addressed (canonical form already
    /// resolved by the handler); `sender` is the bot's own JID. `ts_unix_ms`
    /// is the wall clock at dispatch time; `ts_mono_ns` is the same
    /// monotonic sample so subsequent inbound-echo events (if any) can
    /// be deduplicated against this one.
    pub fn from_outbound_text(
        message_id: String,
        peer: String,
        self_jid: String,
        text: String,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    ) -> Self {
        let text = Self::bound_text(text);
        InboundEvent::Message {
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
        }
    }

    /// Mirror of `from_outbound_text` for media dispatches (image/video/audio/
    /// voice/sticker/document). Same operator mandate: every dispatched media
    /// must surface in the events table — WA's inbox-echo is unreliable for
    /// self-sends and the adapter's `accept_message` filter drops 1:1 chat
    /// messages on live-test fixtures. Two functions, two data flows,
    /// isolated to `octo-whatsapp`. Inbound echos for the same message_id
    /// can be deduped by `events.list` consumers.
    ///
    /// `kind` selects the `MessageKind` variant; `caption` becomes the
    /// `text` slot (bounded to `MAX_INLINE_TEXT_BYTES`). `media_token` is
    /// the upload-ref token returned by the adapter (when the RPC exposes
    /// one — images do; voice/audio/video currently return `(id, _token)`).
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
        let text = caption.map(Self::bound_text).unwrap_or_default();
        InboundEvent::Message {
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
        }
    }

    fn parse_inner(raw: &str, ts_unix_ms: i64, ts_mono_ns: u64, now_unix_ms: Option<i64>) -> Self {
        let raw = raw.trim();
        let untrusted = match now_unix_ms {
            Some(now) => ts_unix_ms > now.saturating_add(SKEW_TOLERANCE_MS),
            None => false,
        };
        let event = if let Some(rest) = raw.strip_prefix("Message(") {
            parse_message(rest, ts_unix_ms, ts_mono_ns)
        } else if let Some(rest) = raw.strip_prefix("Reaction(") {
            parse_reaction(rest, ts_unix_ms, ts_mono_ns)
        } else if let Some(rest) = raw.strip_prefix("GroupChange(") {
            parse_group_change(rest, ts_unix_ms, ts_mono_ns)
        } else if let Some(rest) = raw.strip_prefix("Presence(") {
            parse_presence(rest)
        } else if let Some(rest) = raw.strip_prefix("Connection(") {
            parse_connection(rest, ts_unix_ms, ts_mono_ns)
        } else if let Some(rest) = raw.strip_prefix("Receipt(") {
            parse_receipt(rest, ts_unix_ms, ts_mono_ns)
        } else if let Some(rest) = raw.strip_prefix("Call(") {
            parse_call(rest, ts_unix_ms, ts_mono_ns)
        } else if let Some(rest) = raw.strip_prefix("Story(") {
            parse_story(rest, ts_unix_ms, ts_mono_ns)
        } else {
            return InboundEvent::Unknown {
                raw: raw.to_string(),
                ts_unix_ms,
                ts_mono_ns,
                untrusted,
            };
        };
        // For Unknown fallback, attach the untrusted flag.
        if untrusted {
            match event {
                InboundEvent::Unknown {
                    raw,
                    ts_unix_ms,
                    ts_mono_ns,
                    ..
                } => InboundEvent::Unknown {
                    raw,
                    ts_unix_ms,
                    ts_mono_ns,
                    untrusted: true,
                },
                other => other,
            }
        } else {
            event
        }
    }
}

/// Extract a single `key: value` field from a Debug-formatted struct
/// body. Conservative: returns `None` for missing or malformed fields.
///
/// Stops at the first top-level `,`, `}`, or `)` (the closing paren
/// of the outer `Variant(...)` tuple in `format!("{:?}", Event::...)`).
fn field(body: &str, key: &str) -> Option<String> {
    let needle = format!("{key}: ");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let end = rest.find([',', '}', ')']).unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
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

fn parse_message(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let id = unquote(&field(rest, "id").unwrap_or_default());
    let peer = unquote(&field(rest, "peer").unwrap_or_default());
    let sender = unquote(&field(rest, "sender").unwrap_or_default());
    let text = InboundEvent::bound_text(unquote(&field(rest, "text").unwrap_or_default()));
    let mentions_raw: Vec<String> = (0..MAX_INLINE_MENTIONS + 4)
        .filter_map(|i| {
            let key = format!("mentions[{i}]");
            field(rest, &key).map(|v| unquote(&v))
        })
        .collect();
    let (mentions, mentions_truncated) = InboundEvent::bound_mentions(mentions_raw);
    let is_group = field(rest, "is_group")
        .map(|v| v == "true")
        .unwrap_or(false);
    let from_me = field(rest, "from_me").map(|v| v == "true").unwrap_or(false);
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
    InboundEvent::Message {
        id,
        peer,
        sender,
        ts_unix_ms,
        ts_mono_ns,
        kind,
        text,
        media_token: field(rest, "media_token").map(|v| unquote(&v)),
        reply_to: field(rest, "reply_to").map(|v| unquote(&v)),
        mentions,
        mentions_truncated,
        from_me,
        is_group,
    }
}

fn parse_reaction(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    InboundEvent::Reaction {
        id: unquote(&field(rest, "id").unwrap_or_default()),
        target_msg_id: unquote(&field(rest, "target_msg_id").unwrap_or_default()),
        emoji: unquote(&field(rest, "emoji").unwrap_or_default()),
        from: unquote(&field(rest, "from").unwrap_or_default()),
        peer: unquote(&field(rest, "peer").unwrap_or_default()),
        ts_unix_ms,
        ts_mono_ns,
    }
}

fn parse_group_change(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Join") => GroupChangeKind::Join,
        Some("Leave") => GroupChangeKind::Leave,
        Some("Promote") => GroupChangeKind::Promote,
        Some("Demote") => GroupChangeKind::Demote,
        Some("Subject") => GroupChangeKind::Subject,
        Some("Icon") => GroupChangeKind::Icon,
        Some("Description") => GroupChangeKind::Description,
        _ => GroupChangeKind::Join,
    };
    InboundEvent::GroupChange {
        group_jid: unquote(&field(rest, "group_jid").unwrap_or_default()),
        kind,
        actor: field(rest, "actor").map(|v| unquote(&v)),
        target: field(rest, "target").map(|v| unquote(&v)),
        ts_unix_ms,
        ts_mono_ns,
        after: field(rest, "after").map(|v| unquote(&v)),
    }
}

fn parse_presence(rest: &str) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Available") => PresenceKind::Available,
        Some("Unavailable") => PresenceKind::Unavailable,
        Some("Typing") => PresenceKind::Typing,
        Some("Recording") => PresenceKind::Recording,
        _ => PresenceKind::Available,
    };
    InboundEvent::Presence {
        jid: unquote(&field(rest, "jid").unwrap_or_default()),
        kind,
        last_seen: field(rest, "last_seen").and_then(|v| v.parse().ok()),
    }
}

fn parse_connection(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Connected") => ConnectionKind::Connected,
        Some("Disconnected") => ConnectionKind::Disconnected,
        Some("Replaced") => ConnectionKind::Replaced,
        Some("LoggedOut") => ConnectionKind::LoggedOut,
        Some("Synced") => ConnectionKind::Synced,
        Some("ClockSkewDetected") => ConnectionKind::ClockSkewDetected,
        _ => ConnectionKind::Connected,
    };
    InboundEvent::Connection {
        kind,
        cause: field(rest, "cause").map(|c| match c.as_str() {
            "UserInitiated" => LoggedOutCause::UserInitiated,
            "SessionReplaced" => LoggedOutCause::SessionReplaced,
            "ProtocolError" => LoggedOutCause::ProtocolError,
            _ => LoggedOutCause::Unknown,
        }),
        ts_unix_ms,
        ts_mono_ns,
    }
}

fn parse_receipt(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Read") => ReceiptKind::Read,
        Some("Delivered") => ReceiptKind::Delivered,
        Some("Played") => ReceiptKind::Played,
        _ => ReceiptKind::Delivered,
    };
    InboundEvent::Receipt {
        msg_id: unquote(&field(rest, "msg_id").unwrap_or_default()),
        peer: unquote(&field(rest, "peer").unwrap_or_default()),
        kind,
        ts_unix_ms,
        ts_mono_ns,
    }
}

fn parse_call(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Voice") => CallKind::Voice,
        Some("Video") => CallKind::Video,
        _ => CallKind::Voice,
    };
    let state = match field(rest, "state").as_deref() {
        Some("Offered") => CallState::Offered,
        Some("Accepted") => CallState::Accepted,
        Some("Rejected") => CallState::Rejected,
        Some("Terminated") => CallState::Terminated,
        _ => CallState::Offered,
    };
    InboundEvent::Call {
        id: unquote(&field(rest, "id").unwrap_or_default()),
        peer: unquote(&field(rest, "peer").unwrap_or_default()),
        kind,
        state,
        ts_unix_ms,
        ts_mono_ns,
    }
}

fn parse_story(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let kind = match field(rest, "kind").as_deref() {
        Some("Posted") => StoryKind::Posted,
        Some("Viewed") => StoryKind::Viewed,
        _ => StoryKind::Posted,
    };
    InboundEvent::Story {
        id: unquote(&field(rest, "id").unwrap_or_default()),
        peer: unquote(&field(rest, "peer").unwrap_or_default()),
        kind,
        ts_unix_ms,
        ts_mono_ns,
    }
}

#[cfg(test)]
mod tests;
