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
    /// Echo of a community create / deactivate / link / unlink operation.
    /// Emitted by the daemon-side broadcast hook when a community RPC
    /// succeeds (the actual outbound IQ itself is what `events.rs`
    /// observes).
    CommunityUpdate {
        jid: String,
        kind: CommunityUpdateKind,
        ts_unix_ms: i64,
        ts_mono_ns: u64,
    },
    /// Echo of a newsletter (channel) lifecycle / state change.
    /// Distinct from `CommunityUpdate` (subgroup structure) — this is
    /// per-channel: subscribe/unsubscribe/message/profile/admin actions.
    /// Emitted by the daemon-side broadcast hook when a newsletter RPC
    /// succeeds or the upstream fires a live channel event.
    NewsletterUpdate {
        jid: String,
        kind: NewsletterUpdateKind,
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
pub enum CommunityUpdateKind {
    Created,
    Deactivated,
    Linked,
    Unlinked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NewsletterUpdateKind {
    /// Caller subscribed to the channel (live_updates + initial join).
    Subscribed,
    /// Caller unsubscribed / left.
    Unsubscribed,
    /// A new message arrived in the channel.
    MessageReceived,
    /// Channel profile picture changed.
    PictureChanged,
    /// Channel display name changed.
    NameChanged,
    /// Channel state changed (Active <-> Suspended by admin or server).
    StateChanged,
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

    /// Parse an envelope into one or more [`InboundEvent`]s.
    /// `Messages(MessageBatch { messages: [...] })` produces one
    /// event per inner message (so a single history-sync backfill of
    /// 50 messages lands as 50 events instead of one opaque
    /// `Unknown`). Every other envelope produces exactly one event,
    /// matching [`Self::parse`].
    ///
    /// `now_unix_ms` (when supplied) is used to flag future-skewed
    /// timestamps on the envelope's own `ts_unix_ms` (the same
    /// `untrusted` semantics as [`Self::parse_with_now`]).
    pub fn parse_many(env: EventEnvelope, now_unix_ms: Option<i64>) -> Vec<Self> {
        let raw = env.raw.trim();
        if let Some(rest) = raw.strip_prefix("Messages(") {
            parse_message_batch(rest, env.ts_unix_ms, env.ts_mono_ns, now_unix_ms)
        } else {
            vec![Self::parse_inner(
                raw,
                env.ts_unix_ms,
                env.ts_mono_ns,
                now_unix_ms,
            )]
        }
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
            | Self::CommunityUpdate { ts_unix_ms, .. }
            | Self::NewsletterUpdate { ts_unix_ms, .. }
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
            | Self::CommunityUpdate { ts_mono_ns, .. }
            | Self::NewsletterUpdate { ts_mono_ns, .. }
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
        } else if let Some(rest) = raw.strip_prefix("ServerAck(") {
            // wacore emits `ServerAck(class="message", id, from, timestamp)`
            // for the immediate server-side acknowledge of an outbound
            // dispatch. The 8-variant model collapses this into
            // `Receipt { kind: Delivered }` so the Tier-3 canary and
            // every downstream consumer can assert on a uniform Receipt
            // event. Class="receipt" and other non-message ServerAck
            // classes (which are themselves peer-device delivery
            // confirmations) are kept as typed Receipts as well — they
            // describe the same "server confirmed delivery" semantics
            // from the daemon's point of view. Non-message-class
            // ServerAcks are rare (only for ack-of-ack chains) and we
            // route them through the same parser for consistency.
            parse_server_ack(rest, ts_unix_ms, ts_mono_ns)
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
    // Match either `, key:` (a typical Rust Debug field separator),
    // ` { key:` (the *first* field of a struct whose outer braces
    // were stripped), or `key:` at byte 0. This avoids tripping on
    // field names whose suffixes contain `key` (e.g.
    // `message_context_info` shares `info` with `info:`).
    let after = if let Some(s) = body.find(&format!(", {key}: ")) {
        s + format!(", {key}: ").len()
    } else if let Some(s) = body.find(&format!("{{ {key}: ")) {
        s + format!("{{ {key}: ").len()
    } else if let Some(stripped) = body.strip_prefix(&format!("{key}: ")) {
        body.len() - stripped.len()
    } else {
        return None;
    };
    let rest = &body[after..];
    // Find the end of the value, tracking both `(` / `)` and `{` / `}`
    // depth so that values like `MessageField::Set(VideoMessage { ... })`
    // or `Jid { user: "...", ... }` are kept intact (a bare `,` or `}`
    // inside such a wrapper must not terminate the value).
    //
    // A `)` that returns paren_depth from >0 to 0 is the closing
    // paren of an enclosing `Some(...)` etc.; we INCLUDE it so that
    // downstream `unwrap_some` can find the matching paren. A `)`
    // from 0 to -1 is an outer-wrapper closing that should be
    // excluded; we cut BEFORE it.
    let bytes = rest.as_bytes();
    let mut paren_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut end = bytes.len();
    let mut cut_at: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => paren_depth += 1,
            b')' => {
                if paren_depth == 0 {
                    // Extra closing paren (e.g. outer envelope's `)`).
                    cut_at = Some(i);
                    break;
                }
                paren_depth -= 1;
                if paren_depth == 0 {
                    end = i + 1;
                    cut_at = Some(end);
                    break;
                }
            }
            b'{' => brace_depth += 1,
            b'}' => brace_depth -= 1,
            b',' if paren_depth == 0 && brace_depth == 0 => {
                cut_at = Some(i);
                break;
            }
            _ => {}
        }
    }
    if let Some(c) = cut_at {
        end = c;
    }
    Some(rest[..end].trim().to_string())
}

/// Strip a `Some(value)` wrapper that Rust's `Debug` emits for
/// `Option<T>`. Returns the inner value with no paren-depth loss
/// (handles nested tuples / struct literals). Used by the
/// `MessageBatch` parser for fields like `conversation: Some("hi")`,
/// `id: Some("abc")`, `stanza_id: Some("xyz")`, etc. Returns owned
/// `String` so callers don't have to wrangle `field()`'s borrowed
/// lifetime.
fn unwrap_some(s: &str) -> Option<String> {
    let s = s.trim();
    let rest = s.strip_prefix("Some(")?;
    let bytes = rest.as_bytes();
    let mut depth = 1i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(rest[..i].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
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

/// Parse a `Messages(MessageBatch { messages: [InboundMessage { ... }], origin: ..., ... })`
/// envelope into one [`InboundEvent`] per inner message.
///
/// Why this exists: the WA WS delivers group messages as `MessageBatch`
/// envelopes that wrap an array of `InboundMessage`s, each carrying its
/// own `info: MessageInfo { source: MessageSource { chat, sender }, ...,
/// timestamp }` and `message: Message { conversation, ... }`. Before
/// 2026-07-12 every MessageBatch fell through to `Unknown` because the
/// parser only knew `Message(...)` (singular). The single-event buffer +
/// SQL mirror couldn't see group conversations at all, so they were
/// invisible to FTS, semantic search, and the events-find RPC.
///
/// The parser is conservative — it regex-scans top-level key/value pairs
/// and walks a state machine for nested `MessageField::Set(...)` blocks.
/// When extraction fails (reactions inside a batch, unrecognised
/// sub-messages, etc.) the inner message is dropped silently rather than
/// emitting a noisy `Unknown` — the envelope-level `Unknown` would be
/// misleading because the *envelope* was recognised, just not every
/// inner entry.
fn parse_message_batch(
    rest: &str,
    fallback_ts_unix_ms: i64,
    fallback_ts_mono_ns: u64,
    now_unix_ms: Option<i64>,
) -> Vec<InboundEvent> {
    // Locate the `messages: [ ... ]` array body. We scan for the
    // outermost `[...]` that follows `messages:`.
    let array_body = match extract_top_level_array_after(rest, "messages") {
        Some(b) => b,
        None => {
            // Malformed envelope — return one Unknown so the
            // persister writes the raw envelope rather than dropping
            // it on the floor.
            let untrusted = match now_unix_ms {
                Some(now) => fallback_ts_unix_ms > now.saturating_add(SKEW_TOLERANCE_MS),
                None => false,
            };
            return vec![InboundEvent::Unknown {
                raw: format!("Messages({rest})"),
                ts_unix_ms: fallback_ts_unix_ms,
                ts_mono_ns: fallback_ts_mono_ns,
                untrusted,
            }];
        }
    };

    // Split the array into per-element bodies at the
    // `InboundMessage { ... }` boundary. Each element is a brace
    // block at depth 1 inside the array.
    let elements = split_top_level_braces(array_body);
    let mut out = Vec::with_capacity(elements.len());
    for elem in elements {
        if let Some(ev) =
            parse_inbound_message(elem, fallback_ts_unix_ms, fallback_ts_mono_ns, now_unix_ms)
        {
            out.push(ev);
        }
        // None ⇒ silently dropped (noisy / unsupported inner message)
    }
    if out.is_empty() {
        // No usable inner messages — emit one Unknown so the
        // envelope is at least recorded (matches the single-event
        // parser's fallback semantics).
        let untrusted = match now_unix_ms {
            Some(now) => fallback_ts_unix_ms > now.saturating_add(SKEW_TOLERANCE_MS),
            None => false,
        };
        out.push(InboundEvent::Unknown {
            raw: format!("Messages({rest})"),
            ts_unix_ms: fallback_ts_unix_ms,
            ts_mono_ns: fallback_ts_mono_ns,
            untrusted,
        });
    }
    out
}

/// Parse one `InboundMessage { message: Message { ... }, info: MessageInfo { ... } }`
/// body into an [`InboundEvent`]. Returns `None` for inner messages we
/// don't know how to surface yet (e.g. encrypted reactions whose body
/// uses a MessageField variant the regex doesn't recognise).
fn parse_inbound_message(
    body: &str,
    fallback_ts_unix_ms: i64,
    fallback_ts_mono_ns: u64,
    now_unix_ms: Option<i64>,
) -> Option<InboundEvent> {
    // info: MessageInfo { source: MessageSource { chat: Jid { ... }, sender: ... },
    //                     id, timestamp, is_from_me, is_group, ... }
    let info_body = extract_nested_block(body, "info").unwrap_or("");
    let source_body = extract_nested_block(info_body, "source").unwrap_or("");
    // Rust `Debug` renders Option<T> as `Some(value)` and Jid as
    // `Jid { user: "...", ... }` (no `Some` wrapper). For peer/sender
    // we want the bare `user: "..."` string from inside the Jid block.
    let peer = extract_jid_user(source_body, "chat");
    let sender = extract_jid_user(source_body, "sender");
    let id = unquote(&field(info_body, "id").unwrap_or_default());
    let timestamp_raw = field(info_body, "timestamp")
        .and_then(|v| unwrap_some(&v))
        .unwrap_or_default();
    let ts_unix_ms = parse_iso8601_ms(&timestamp_raw).unwrap_or(fallback_ts_unix_ms);
    let is_group = field(info_body, "is_group")
        .map(|v| v == "true")
        .unwrap_or(false);
    let from_me = field(info_body, "is_from_me")
        .map(|v| v == "true")
        .unwrap_or(false);
    let _untrusted = match now_unix_ms {
        Some(now) => ts_unix_ms > now.saturating_add(SKEW_TOLERANCE_MS),
        None => false,
    };
    // The `info.timestamp` is `2026-07-12T20:49:20Z` and the
    // envelope-level ts is the persister's now(). Prefer the
    // message-internal timestamp when it's parseable (it usually
    // is — MessageInfo always carries one for inbound traffic);
    // fall back to envelope ts otherwise (sentinel for "when WA
    // didn't bother" — usually for system-side inner messages).
    let effective_ts_unix_ms = if timestamp_raw.is_empty() {
        fallback_ts_unix_ms
    } else {
        ts_unix_ms
    };
    let ts_mono_ns = fallback_ts_mono_ns;

    // message: Message { conversation, extended_text_message, image_message, ... }
    let message_body = extract_nested_block(body, "message")?;

    // Pull the first `MessageField::Set(<InnerType> { ... text fields ... })`
    // we recognise. Earlier fields win so plain `conversation` beats
    // `extended_text_message.text` if both are present (which they
    // never are, but order is explicit anyway).
    if let Some(text) = field(message_body, "conversation")
        .as_deref()
        .and_then(unwrap_some)
        .filter(|v| *v != "None" && !v.is_empty())
        .map(|v| unquote(&v))
    {
        return Some(InboundEvent::Message {
            id,
            peer: peer.clone(),
            sender: sender.clone(),
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Text,
            text: InboundEvent::bound_text(text),
            media_token: None,
            reply_to: extract_context_reply_to(message_body),
            mentions: extract_mentions(message_body),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    if let Some(ext) =
        field(message_body, "extended_text_message").and_then(|s| unwrap_message_field_set(&s))
    {
        if let Some(text) = extract_first_some_text_in_block(&ext) {
            return Some(InboundEvent::Message {
                id,
                peer: peer.clone(),
                sender: sender.clone(),
                ts_unix_ms: effective_ts_unix_ms,
                ts_mono_ns,
                kind: MessageKind::Text,
                text: InboundEvent::bound_text(text),
                media_token: None,
                reply_to: extract_context_reply_to(message_body),
                mentions: extract_mentions(message_body),
                mentions_truncated: false,
                from_me,
                is_group,
            });
        }
    }
    // Image
    if let Some(img) =
        field(message_body, "image_message").and_then(|s| unwrap_message_field_set(&s))
    {
        let caption = extract_first_some_string_in_block(img.as_str(), "caption");
        return Some(InboundEvent::Message {
            id,
            peer: peer.clone(),
            sender: sender.clone(),
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Image,
            text: InboundEvent::bound_text(caption.unwrap_or_default()),
            media_token: extract_first_some_string_in_block(img.as_str(), "media_key"),
            reply_to: extract_context_reply_to(message_body),
            mentions: extract_mentions(message_body),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Video
    if let Some(vid) =
        field(message_body, "video_message").and_then(|s| unwrap_message_field_set(&s))
    {
        let caption = extract_first_some_string_in_block(vid.as_str(), "caption");
        return Some(InboundEvent::Message {
            id,
            peer: peer.clone(),
            sender: sender.clone(),
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Video,
            text: InboundEvent::bound_text(caption.unwrap_or_default()),
            media_token: extract_first_some_string_in_block(vid.as_str(), "media_key"),
            reply_to: extract_context_reply_to(message_body),
            mentions: extract_mentions(message_body),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Document
    if let Some(doc) =
        field(message_body, "document_message").and_then(|s| unwrap_message_field_set(&s))
    {
        let caption = extract_first_some_string_in_block(doc.as_str(), "caption");
        return Some(InboundEvent::Message {
            id,
            peer: peer.clone(),
            sender: sender.clone(),
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Document,
            text: InboundEvent::bound_text(caption.unwrap_or_default()),
            media_token: extract_first_some_string_in_block(doc.as_str(), "media_key"),
            reply_to: extract_context_reply_to(message_body),
            mentions: extract_mentions(message_body),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Audio (no caption field — just media_key)
    if field(message_body, "audio_message")
        .and_then(|s| unwrap_message_field_set(&s))
        .is_some()
    {
        return Some(InboundEvent::Message {
            id,
            peer: peer.clone(),
            sender: sender.clone(),
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Audio,
            text: String::new(),
            media_token: None,
            reply_to: extract_context_reply_to(message_body),
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Voice (ptv_message or legacy voice fields)
    if let Some(ptv) = field(message_body, "ptv_message").and_then(|s| unwrap_message_field_set(&s))
    {
        return Some(InboundEvent::Message {
            id,
            peer: peer.clone(),
            sender: sender.clone(),
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Voice,
            text: String::new(),
            media_token: extract_first_some_string_in_block(ptv.as_str(), "media_key"),
            reply_to: extract_context_reply_to(message_body),
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Sticker
    if field(message_body, "sticker_message")
        .and_then(|s| unwrap_message_field_set(&s))
        .is_some()
    {
        return Some(InboundEvent::Message {
            id,
            peer,
            sender,
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Sticker,
            text: String::new(),
            media_token: None,
            reply_to: extract_context_reply_to(message_body),
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Contact
    if field(message_body, "contact_message")
        .and_then(|s| unwrap_message_field_set(&s))
        .is_some()
    {
        return Some(InboundEvent::Message {
            id,
            peer,
            sender,
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Contact,
            text: String::new(),
            media_token: None,
            reply_to: extract_context_reply_to(message_body),
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Location
    if field(message_body, "location_message")
        .and_then(|s| unwrap_message_field_set(&s))
        .is_some()
    {
        return Some(InboundEvent::Message {
            id,
            peer,
            sender,
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Location,
            text: String::new(),
            media_token: None,
            reply_to: extract_context_reply_to(message_body),
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Poll
    if field(message_body, "poll_creation_message_v3")
        .or_else(|| field(message_body, "poll_creation_message"))
        .and_then(|s| unwrap_message_field_set(&s))
        .is_some()
    {
        return Some(InboundEvent::Message {
            id,
            peer,
            sender,
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
            kind: MessageKind::Poll,
            text: String::new(),
            media_token: None,
            reply_to: extract_context_reply_to(message_body),
            mentions: Vec::new(),
            mentions_truncated: false,
            from_me,
            is_group,
        });
    }
    // Reaction (nested in batch — synthesize a Reaction event)
    if let Some(react) =
        field(message_body, "reaction_message").and_then(|s| unwrap_message_field_set(&s))
    {
        let key_body = field(react.as_str(), "key")
            .and_then(|s| unwrap_message_field_set(&s))
            .unwrap_or_default();
        let target = unquote(&field(&key_body, "id").unwrap_or_default());
        let emoji = unquote(&field(react.as_str(), "text").unwrap_or_default());
        return Some(InboundEvent::Reaction {
            id,
            target_msg_id: target,
            emoji,
            from: sender,
            peer,
            ts_unix_ms: effective_ts_unix_ms,
            ts_mono_ns,
        });
    }
    // Encrypted reaction / unknown — drop silently.
    let _ = message_body;
    None
}

/// Find the body of an array `[ ... ]` that immediately follows
/// `key:` at the same nesting depth (depth 0 inside `rest`). Returns
/// the inside of the brackets (without the surrounding `[` `]`).
fn extract_top_level_array_after<'a>(rest: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}: [");
    let start = rest.find(&needle)? + needle.len();
    // Walk forward, tracking `[`/`]` depth, until we close depth 0.
    let bytes = rest.as_bytes();
    let mut depth: i32 = 1;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[start..i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Split `[a, b, c]` into top-level elements `["a", "b", "c"]`,
/// respecting brace/paren/bracket nesting. The caller is responsible
/// for the surrounding `[` `]` having been stripped — `s` is the inner
/// content of the array.
fn split_top_level_braces(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut brace = 0i32;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => brace += 1,
            b'}' => {
                brace -= 1;
                if brace == 0 && paren == 0 && bracket == 0 {
                    // Trim trailing comma if present.
                    let end = if i + 1 < bytes.len() && bytes[i + 1] == b',' {
                        i + 1
                    } else {
                        i
                    };
                    let trimmed = s[start..=end].trim();
                    // Skip empty entries (", ,") and bare commas.
                    if !trimmed.is_empty() && trimmed != "," {
                        out.push(trimmed);
                    }
                    // Next element starts after the comma (if any).
                    start = end + 1;
                    // Consume trailing whitespace.
                    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
                        start += 1;
                    }
                    i = start;
                    continue;
                }
            }
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'[' => bracket += 1,
            b']' => bracket -= 1,
            _ => {}
        }
        i += 1;
    }
    // Trailing element without a closing brace (shouldn't happen for
    // well-formed Debug output, but guard against losing the last
    // element).
    let tail = s[start..].trim();
    if !tail.is_empty() && tail != "," && !out.iter().any(|e| e.trim() == tail) {
        out.push(tail);
    }
    out
}

/// Extract the body inside `key: <block>` where `<block>` is a
/// `{...}`-delimited struct at the *current* nesting level. Returns
/// the body inside the braces (without surrounding `{` `}`).
///
/// The caller is expected to call this with the **enclosing struct
/// body** (e.g. `InboundMessage { ... }` stripped of its outer
/// braces) so that the inner `info: MessageInfo { ... }` can be
/// found at depth 0.
///
/// To avoid matching fields whose name ends in `_key` (e.g.
/// `message_context_info` contains the substring `info`), we search
/// for `, key: ` (a preceding comma + space is the canonical
/// Rust Debug field separator at depth 0).
fn extract_nested_block<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    // Field can appear after `, ` (a typical Rust Debug field
    // separator), after ` { ` (the *first* field of a struct that
    // opens with `Name { ... }`), after `{{ ` (depth-2 nested open),
    // or at byte 0 (the body itself starts with `key:`).
    let after = if let Some(s) = body.find(&format!(", {key}: ")) {
        s + format!(", {key}: ").len()
    } else if let Some(s) = body.find(&format!("{{ {key}: ")) {
        s + format!("{{ {key}: ").len()
    } else if let Some(s) = body.find(&format!("{{{{ {key}: ")) {
        // Nested-open brace sequence (depth-2).
        s + format!("{{{{ {key}: ").len()
    } else if let Some(stripped) = body.strip_prefix(&format!("{key}: ")) {
        body.len() - stripped.len()
    } else {
        return None;
    };
    let bytes = body.as_bytes();
    // Skip whitespace AND a Rust type-name prefix like `MessageInfo `
    // — Debug format emits `key: TypeName { ... }`. Walk past any
    // identifier characters, then whitespace, until we find `{`.
    let mut i = after;
    // First skip whitespace.
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    // Then skip a Rust identifier (type name): letter/underscore,
    // then letters/digits/underscores.
    if i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
        i += 1;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
    }
    // Then skip whitespace again.
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'{' {
        return None;
    }
    let open = i + 1;
    let mut depth: i32 = 1;
    let mut j = open;
    while j < bytes.len() {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(body[open..j].trim_start());
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// Within `block: { ... }`, find the first field of the form
/// `key: Some("...")` and return the unquoted string. Used to pull
/// `caption` out of image/video/document sub-messages whose nested
/// schema is `caption: Some("...")`.
/// Pull the `user: "..."` value from a `Jid { user: "...", ... }`
/// block attached to a field. Used by `parse_inbound_message` to get
/// peer/sender JID users without parsing the whole Jid struct. Returns
/// empty string if the field or its `user` subfield is missing.
fn extract_jid_user(body: &str, field_name: &str) -> String {
    let block = match extract_nested_block(body, field_name) {
        Some(b) => b,
        None => return String::new(),
    };
    let raw = match field(block, "user") {
        Some(v) => v,
        None => return String::new(),
    };
    unquote(&raw)
}

fn extract_first_some_string_in_block(block: &str, key: &str) -> Option<String> {
    let needle = format!("{key}: Some(");
    let start = block.find(&needle)? + needle.len();
    // Read until matching `)` (only one level deep — `Some(String)`
    // doesn't have nested parens for simple strings).
    let bytes = block.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == b')' {
            return Some(unquote(&block[start..i]));
        }
        i += 1;
    }
    None
}

/// Strip a `MessageField::Set(<InnerType> { ... })` wrapper, returning
/// the inner `{ ... }` body. Used by `extract_context_reply_to` and
/// the message-field arms so callers can use `field()` /
/// `extract_first_some_string_in_block` on the inner struct directly.
/// Returns `None` for `MessageField::Unset` / `None` / missing.
fn unwrap_message_field_set(block: &str) -> Option<String> {
    let prefix = "MessageField::Set(";
    let start = block.find(prefix)? + prefix.len();
    let bytes = block.as_bytes();
    let mut depth = 1i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(block[start..i].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Within `block: { ... }`, find the first field of the form
/// `key: Some("...")` for the *nested text field* — used for
/// `extended_text_message.text: Some("...")`. The block contains
/// `text: Some("...")`, so we search for the text key.
fn extract_first_some_text_in_block(block: &str) -> Option<String> {
    extract_first_some_string_in_block(block, "text")
}

/// Parse `2026-07-12T20:49:20Z` → `Some(1783890560000)`. Returns
/// `None` for any other shape — caller falls back to envelope ts.
fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let s = s.trim().trim_matches('"');
    // Expected: YYYY-MM-DDTHH:MM:SSZ  (sometimes with millis, sometimes
    // without). Minimal manual parser — no chrono dep.
    if s.len() < 20 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let minute: i64 = s.get(14..16)?.parse().ok()?;
    let second: i64 = s.get(17..19)?.parse().ok()?;
    // Days in month (leap year aware). Conservative: Feb = 28.
    let days_before_month = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mut day_of_year = days_before_month[(month - 1) as usize] + (day - 1);
    if month > 2 && is_leap {
        day_of_year += 1;
    }
    // Years since 1970.
    let mut y = year - 1970;
    let mut days = 0i64;
    while y > 0 {
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        days += if leap { 366 } else { 365 };
        y -= 1;
    }
    days += day_of_year;
    Some(((days * 24 + hour) * 60 + minute) * 60 * 1000 + second * 1000)
}

/// Walk `message.context_info: ... { ..., stanza_id: ..., quoted_message: ... }`
/// and pull `stanza_id` (the WA msg_id of the quoted message) if the
/// quoted message is present. Returns `None` when the field is absent
/// or `stanza_id: None`.
fn extract_context_reply_to(message_body: &str) -> Option<String> {
    let raw = field(message_body, "context_info")?;
    let ctx = unwrap_message_field_set(&raw)?;
    field(ctx.as_str(), "quoted_message").and_then(|s| unwrap_message_field_set(&s))?;
    let id = field(ctx.as_str(), "stanza_id").and_then(|v| unwrap_some(&v))?;
    if id == "None" || id.is_empty() {
        return None;
    }
    Some(unquote(&id))
}

/// Walk `message.context_info.mentions` and pull each as a JID
/// string. Bounded by `MAX_INLINE_MENTIONS` via
/// [`InboundEvent::bound_mentions`].
fn extract_mentions(message_body: &str) -> Vec<String> {
    let raw = match field(message_body, "context_info") {
        Some(c) => c,
        None => return Vec::new(),
    };
    let ctx = match unwrap_message_field_set(&raw) {
        Some(c) => c,
        None => return Vec::new(),
    };
    // mentions: [Jid { ... }, Jid { ... }] — same regex strategy as
    // split_top_level_braces but we just want the user string from
    // each entry.
    let arr = match extract_top_level_array_after(&ctx, "mentions") {
        Some(a) => a,
        None => return Vec::new(),
    };
    let elems = split_top_level_braces(arr);
    let mut out = Vec::with_capacity(elems.len());
    for elem in elems {
        if let Some(user) = field(elem, "user").and_then(|v| unwrap_some(&v)) {
            out.push(unquote(&user));
        }
    }
    InboundEvent::bound_mentions(out).0
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
    // wacore emits `Receipt { type: ReceiptType, source: MessageSource { chat, sender, ... },
    // message_ids: ["3EB0..."], timestamp, offline }`. The Receipt
    // struct field is declared `r#type` (Rust reserved-word escape)
    // but the Debug impl on this wacore version prints it as `type`
    // (no raw-identifier escape). There is NO `id:` field — message
    // ids live inside the `message_ids: Vec<MessageId>` array. Reading
    // the first id from that array gives us the canonical message id
    // that consumers correlate against.
    //
    // The 8-variant model collapses `Read` and `Played` directly into
    // the matching `ReceiptKind`; everything else (Sent, Sender,
    // Retry, EncRekeyRetry, ReadSelf, PlayedSelf, ServerError,
    // Inactive, PeerMsg, HistorySync, Other) falls back to Delivered
    // so consumers always see a typed Receipt.
    //
    // We check `type` first (current wacore Debug) then fall back to
    // `r#type` (older wacore versions that did render the raw
    // identifier escape) and finally `kind` (legacy compat path).
    let kind = match field(rest, "type")
        .or_else(|| field(rest, "r#type"))
        .or_else(|| field(rest, "kind"))
        .as_deref()
    {
        Some("Read") | Some("ReadSelf") => ReceiptKind::Read,
        Some("Played") | Some("PlayedSelf") => ReceiptKind::Played,
        Some("Delivered") => ReceiptKind::Delivered,
        _ => ReceiptKind::Delivered,
    };
    // Extract the first id from `message_ids: ["3EB0...", ...]`.
    let msg_id = extract_message_ids_first(rest).unwrap_or_default();
    // The "peer" for an inbound receipt is the JID the receipt was
    // sent to — `source.chat` in wacore's MessageSource. Use that
    // field, falling back to `from` then `to` for older wacore
    // variants or server-originated receipts without an acker.
    let peer = extract_source_chat(rest)
        .or_else(|| extract_field(rest, "from"))
        .or_else(|| extract_field(rest, "to"))
        .unwrap_or_default();
    InboundEvent::Receipt {
        msg_id,
        peer,
        kind,
        ts_unix_ms,
        ts_mono_ns,
    }
}

/// Extract the first message id from a wacore `Receipt` Debug body's
/// `message_ids: Vec<String>` field. The Debug format prints the array
/// as `["3EB0...", ...]`. Returns the first quoted string, or `None`
/// if the field is missing or empty.
fn extract_message_ids_first(body: &str) -> Option<String> {
    let needle = "message_ids: [";
    let start = body.find(needle)? + needle.len();
    let rest = &body[start..];
    let after_quote = rest.strip_prefix('"')?;
    let end = after_quote.find('"')?;
    Some(after_quote[..end].to_string())
}

/// Extract the JID from `source.chat` inside a wacore
/// `MessageSource`. The Debug body looks like:
/// `source: MessageSource { chat: Some(Jid { ... }), sender: ..., ... }`.
/// The value follows `chat: ` and is either `Some(Jid { ... })` or
/// bare `Jid { ... }`. We extract the user+server directly since the
/// Jid shape is known.
fn extract_source_chat(body: &str) -> Option<String> {
    let needle = "source: MessageSource { chat: ";
    let start = body.find(needle)? + needle.len();
    let rest = &body[start..];
    // Strip the optional `Some(...)` wrapper. Find the Jid's user and
    // server fields directly — same regex-free approach we use for
    // ServerAck's `from` field.
    let rest = rest.strip_prefix("Some(").unwrap_or(rest).trim_start();
    let after_jid = rest.strip_prefix("Jid ")?;
    let user = {
        let u_needle = "user: \"";
        let u_start = after_jid.find(u_needle)? + u_needle.len();
        let after_u = &after_jid[u_start..];
        let u_end = after_u.find('"')?;
        &after_u[..u_end]
    };
    let s_needle = "server: ";
    let s_start = after_jid.find(s_needle)? + s_needle.len();
    let after_s = &after_jid[s_start..];
    let s_end = after_s.find([',', ' ', '}']).unwrap_or(after_s.len());
    let server_raw = after_s[..s_end].trim();
    let server = match server_raw {
        "Pn" => "s.whatsapp.net",
        "Lid" => "lid",
        other => other,
    };
    let device = after_jid
        .find("device: ")
        .map(|d| {
            let rest = &after_jid[d + "device: ".len()..];
            let end = rest.find([',', ' ', '}']).unwrap_or(rest.len());
            rest[..end].trim().parse::<u16>().unwrap_or(0)
        })
        .unwrap_or(0);
    Some(if device > 0 {
        format!("{user}:{device}@{server}")
    } else {
        format!("{user}@{server}")
    })
}

/// Parse wacore's `ServerAck` debug-formatted body. The shape is:
///
/// ```text
/// ServerAck { id: "3EB0...", class: Some("message"), from: Some(Jid { ... }),
///             timestamp: Some(2026-07-11T12:06:47Z), error: None }
/// ```
///
/// We collapse the message-class server-ack into a typed
/// `Receipt { kind: Delivered }` so consumers can assert on a single
/// uniform event shape. The original `Unknown(ServerAck(...))` body is
/// preserved in the events table only for the raw wacore variant when
/// we don't recognise the class — that path is exercised by `class =
/// None` (e.g. presence-class acks), which we drop into `Unknown`
/// rather than fabricating a Receipt we can't substantiate.
///
/// The helper `field()` terminates at the first `,`, `}`, or `)` — too
/// aggressive for `Some("...")` and `Some(Jid { ... })` values that
/// appear as Rust Debug-wrapped enums. We hand-roll the extraction
/// here: locate the quoted inner string directly.
fn parse_server_ack(rest: &str, ts_unix_ms: i64, ts_mono_ns: u64) -> InboundEvent {
    let class = extract_field(rest, "class");
    let id = extract_field(rest, "id").unwrap_or_default();
    let from = extract_field(rest, "from").unwrap_or_default();
    if !matches!(class.as_deref(), Some("message")) {
        return InboundEvent::Unknown {
            raw: format!("ServerAck({rest})"),
            ts_unix_ms,
            ts_mono_ns,
            untrusted: false,
        };
    }
    InboundEvent::Receipt {
        msg_id: id,
        peer: from,
        kind: ReceiptKind::Delivered,
        ts_unix_ms,
        ts_mono_ns,
    }
}

/// Find `key: ...` in a Rust Debug dump body and return a usable
/// string for the value. Three shapes are recognised:
///
///   - `key: "value"` — bare quoted string
///   - `key: Some("value")` — enum-wrapped primitive
///   - `key: Some(Jid { user: "X", server: Pn, ... })` — enum-wrapped
///     Jid Debug form; this helper unwraps to the canonical `X@Pn`
///     JID string so consumers don't have to re-parse Debug output.
///
/// Returns `None` if the field is absent or its shape doesn't match
/// any of the above. The `field()` helper used elsewhere in this
/// module terminates at the first `,`, `}`, or `)` — too aggressive
/// for `Some(...)` and `Some(Jid { ... })` values. This helper
/// unwraps correctly.
fn extract_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("{key}: ");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let rest = rest.trim_start();
    // Bare quoted string: "value"
    if let Some(after_quote) = rest.strip_prefix('"') {
        let end = after_quote.find('"')?;
        return Some(after_quote[..end].to_string());
    }
    // Some("value")
    if let Some(after_some) = rest.strip_prefix("Some(\"") {
        let end = after_some.find('"')?;
        return Some(after_some[..end].to_string());
    }
    // Some(Jid { user: "X", server: Pn, device: 25, ... }) -> "X:25@Pn"
    // when device > 0, otherwise "X@Pn". The device suffix is required
    // by the WA wire protocol for multi-device self-echo (without it,
    // the dispatch lands on the primary slot instead of the linked
    // session — see the Tier 2 self-send diagnosis in
    // commit bdf2e81a).
    if let Some(after_jid) = rest.strip_prefix("Some(Jid ") {
        let u_needle = "user: \"";
        let u_start = after_jid.find(u_needle)? + u_needle.len();
        let after_u = &after_jid[u_start..];
        let u_end = after_u.find('"')?;
        let user = &after_u[..u_end];
        let s_needle = "server: ";
        let s_start = after_jid.find(s_needle)? + s_needle.len();
        let after_s = &after_jid[s_start..];
        let s_end = after_s.find([',', ' ', '}']).unwrap_or(after_s.len());
        let server_raw = after_s[..s_end].trim();
        // Map wacore's Debug repr of the server enum to the canonical
        // WA JID domain string. Pn (phone number) and Lid (long-form
        // identity) are the two values in the wire protocol.
        let server = match server_raw {
            "Pn" => "s.whatsapp.net",
            "Lid" => "lid",
            other => other,
        };
        // Optional device suffix.
        let device = after_jid
            .find("device: ")
            .map(|d| {
                let rest = &after_jid[d + "device: ".len()..];
                let end = rest.find([',', ' ', '}']).unwrap_or(rest.len());
                rest[..end].trim().parse::<u8>().unwrap_or(0)
            })
            .unwrap_or(0);
        let canonical = if device > 0 {
            format!("{user}:{device}@{server}")
        } else {
            format!("{user}@{server}")
        };
        return Some(canonical);
    }
    None
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
