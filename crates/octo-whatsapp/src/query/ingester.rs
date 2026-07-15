//! Write-path: mirrors `InboundEvent` into the `events` + `messages`
//! tables. Uses `INSERT OR IGNORE` everywhere so replaying the NDJSON
//! canonical log at boot is safe — duplicates collapse on `events.id`
//! PK.
//!
//! `QueryIngester` is fed from the existing `events_persister`
//! broadcast bus (`PersisterIngress::tx_clone`), so the live sender
//! path adds no new `.await`. See
//! `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 1.
//!
//! Gated behind the `query` cargo feature.

use std::result::Result as StdResult;
use thiserror::Error;

use crate::events::{InboundEvent, MessageKind};
use stoolap::{Database, Value};

/// Errors the ingester can surface to the broadcast-loop driver.
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("stoolap error: {0}")]
    Stoolap(#[from] stoolap::Error),
    #[error("serialize event: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub type Result<T> = StdResult<T, QueryError>;

/// Owns a handle to the embedded SQL DB and exposes a single
/// synchronous `ingest` entry point. Cheap to share via `Arc`.
pub struct QueryIngester {
    db: Database,
}

impl std::fmt::Debug for QueryIngester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryIngester").finish_non_exhaustive()
    }
}

impl QueryIngester {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Borrow the underlying database. Useful for `QueryService` /
    /// boot-time rebuilders that need direct read access.
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Mirror one `InboundEvent` into `events` (always) and `messages`
    /// (when the variant is `Message`). `id` is the same monotonic id
    /// the `EventsBuffer` assigned; using it as the PK makes replay
    /// safe via `INSERT OR IGNORE`.
    ///
    /// `recorded_at = (ts_unix_ms, ts_mono_ns)` is the wall-clock /
    /// monotonic pair from the time the event was *observed* by the
    /// query subsystem — either the live broadcast loop (uses
    /// `now()`) or the NDJSON replay path (uses
    /// `PersistedEvent.ts_unix_ms/ts_mono_ns` set at first write).
    /// When the event-internal ts is zero (Receipt / Presence /
    /// Unknown variants — WA doesn't ship timestamps for those), we
    /// fall back to `recorded_at` so the SQL row carries a
    /// meaningful chronological value instead of 0. This keeps
    /// `ORDER BY ts_unix_ms DESC` and `since_ts_unix_ms` filters
    /// useful for operators.
    pub fn ingest(&self, id: u64, recorded_at: (i64, u64), ev: &InboundEvent) -> Result<()> {
        let (ev_ts_unix_ms, ev_ts_mono_ns) = event_ts(ev);
        let ts_unix_ms = if ev_ts_unix_ms > 0 {
            ev_ts_unix_ms
        } else {
            recorded_at.0
        };
        let ts_mono_ns = if ev_ts_mono_ns > 0 {
            ev_ts_mono_ns
        } else {
            // recorded_at.1 is u64; stoolap::Value::from doesn't
            // accept u64, so narrow to i64 — the value still fits
            // for any wall-clock mono_ns we'll ever observe
            // (Process startup ⇒ ~10^12 ns ⇒ well under 2^63).
            recorded_at.1 as i64
        };
        let kind = event_kind_tag(ev);
        let variant = event_variant(ev);
        let (peer, sender, chat_jid) = event_denorm(ev);
        let payload = serde_json::to_string(ev)?;

        self.insert_idempotent(
            "INSERT INTO events \
             (id, ts_unix_ms, ts_mono_ns, kind, variant, peer, sender, chat_jid, payload) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                Value::from(id as i64),
                Value::from(ts_unix_ms),
                Value::from(ts_mono_ns),
                Value::from(kind),
                opt_text(variant),
                opt_text(peer),
                opt_text(sender),
                opt_text(chat_jid),
                Value::from(payload),
            ],
        )?;

        if let Some(msg) = message_row(ev) {
            self.insert_idempotent(
                "INSERT INTO messages \
                 (event_id, peer, sender, ts_unix_ms, kind, text, media_token, from_me, is_group) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                vec![
                    Value::from(id as i64),
                    Value::from(msg.peer),
                    Value::from(msg.sender),
                    Value::from(ts_unix_ms),
                    Value::from(msg.kind_str),
                    Value::from(msg.text),
                    opt_text(msg.media_token),
                    bool_i64(msg.from_me),
                    bool_i64(msg.is_group),
                ],
            )?;
        }
        Ok(())
    }

    /// Run an `INSERT` and swallow idempotency errors so the boot-time
    /// NDJSON replay collapses duplicates on the events.id PK without
    /// aborting the rest of the rebuild. Stoolap surfaces both
    /// `PrimaryKeyConstraint` (PK violation) and `UniqueConstraint`
    /// (secondary unique index) as distinct variants.
    fn insert_idempotent(&self, sql: &str, params: Vec<Value>) -> Result<()> {
        match self.db.execute(sql, params) {
            Ok(_) => Ok(()),
            Err(stoolap::Error::PrimaryKeyConstraint { .. })
            | Err(stoolap::Error::UniqueConstraint { .. }) => Ok(()),
            Err(e) => Err(QueryError::from(e)),
        }
    }

    /// Hermetic helper for tests: truncate the derived views without
    /// touching NDJSON. Production code must never call this.
    #[doc(hidden)]
    pub fn reset_for_tests(&self) -> Result<()> {
        self.db.execute("DELETE FROM events", ())?;
        self.db.execute("DELETE FROM messages", ())?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

struct MediaRow {
    peer: String,
    sender: String,
    kind_str: &'static str,
    text: String,
    media_token: Option<String>,
    from_me: bool,
    is_group: bool,
}

fn message_row(ev: &InboundEvent) -> Option<MediaRow> {
    match ev {
        InboundEvent::Message {
            peer,
            sender,
            kind,
            text,
            media_token,
            from_me,
            is_group,
            ..
        } => Some(MediaRow {
            peer: peer.clone(),
            sender: sender.clone(),
            kind_str: message_kind_str(*kind),
            text: text.clone(),
            media_token: media_token.clone(),
            from_me: *from_me,
            is_group: *is_group,
        }),
        _ => None,
    }
}

fn event_ts(ev: &InboundEvent) -> (i64, i64) {
    match ev {
        InboundEvent::Message {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::Reaction {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::GroupChange {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::Receipt {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::Call {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::Story {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::Connection {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::CommunityUpdate {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::NewsletterUpdate {
            ts_unix_ms,
            ts_mono_ns,
            ..
        }
        | InboundEvent::Unknown {
            ts_unix_ms,
            ts_mono_ns,
            ..
        } => (*ts_unix_ms, *ts_mono_ns as i64),
        // Presence carries only `last_seen` (wall-clock). Pass that
        // through; if the daemon never observed the peer, it's `None`
        // and the caller falls back to `recorded_at`.
        InboundEvent::Presence { last_seen, .. } => (last_seen.unwrap_or(0), 0),
    }
}

fn event_kind_tag(ev: &InboundEvent) -> &'static str {
    // Mirrors the serde tag `event` with rename_all="snake_case"
    // declared on `InboundEvent`.
    match ev {
        InboundEvent::Message { .. } => "message",
        InboundEvent::Reaction { .. } => "reaction",
        InboundEvent::GroupChange { .. } => "group_change",
        InboundEvent::Receipt { .. } => "receipt",
        InboundEvent::Presence { .. } => "presence",
        InboundEvent::Connection { .. } => "connection",
        InboundEvent::Call { .. } => "call",
        InboundEvent::Story { .. } => "story",
        InboundEvent::CommunityUpdate { .. } => "community_update",
        InboundEvent::NewsletterUpdate { .. } => "newsletter_update",
        InboundEvent::Unknown { .. } => "unknown",
    }
}

fn event_variant(ev: &InboundEvent) -> Option<String> {
    match ev {
        InboundEvent::Message { kind, .. } => Some(message_kind_str(*kind).to_string()),
        InboundEvent::Receipt { kind, .. } => Some(receipt_kind_str(*kind).to_string()),
        InboundEvent::GroupChange { kind, .. } => Some(group_change_kind_str(*kind).to_string()),
        InboundEvent::Presence { kind, .. } => Some(presence_kind_str(*kind).to_string()),
        InboundEvent::Connection { kind, .. } => Some(connection_kind_str(*kind).to_string()),
        InboundEvent::Call { kind, state, .. } => Some(format!(
            "{}_{}",
            call_kind_str(*kind),
            call_state_str(*state)
        )),
        InboundEvent::Story { kind, .. } => Some(story_kind_str(*kind).to_string()),
        InboundEvent::CommunityUpdate { kind, .. } => {
            Some(community_update_kind_str(*kind).to_string())
        }
        InboundEvent::NewsletterUpdate { kind, .. } => {
            Some(newsletter_update_kind_str(*kind).to_string())
        }
        InboundEvent::Reaction { .. } | InboundEvent::Unknown { .. } => None,
    }
}

fn event_denorm(ev: &InboundEvent) -> (Option<String>, Option<String>, Option<String>) {
    match ev {
        InboundEvent::Message { peer, sender, .. } => {
            (Some(peer.clone()), Some(sender.clone()), Some(peer.clone()))
        }
        InboundEvent::Reaction { peer, from, .. } => {
            (Some(peer.clone()), Some(from.clone()), Some(peer.clone()))
        }
        InboundEvent::Receipt { peer, .. } => (Some(peer.clone()), None, Some(peer.clone())),
        InboundEvent::GroupChange { group_jid, .. } => {
            (Some(group_jid.clone()), None, Some(group_jid.clone()))
        }
        InboundEvent::Call { peer, .. } => (Some(peer.clone()), None, Some(peer.clone())),
        InboundEvent::Story { peer, .. } => (Some(peer.clone()), None, Some(peer.clone())),
        InboundEvent::CommunityUpdate { jid, .. } => (Some(jid.clone()), None, Some(jid.clone())),
        InboundEvent::NewsletterUpdate { jid, .. } => (Some(jid.clone()), None, Some(jid.clone())),
        InboundEvent::Presence { jid, .. } => {
            (Some(jid.clone()), Some(jid.clone()), Some(jid.clone()))
        }
        InboundEvent::Connection { .. } | InboundEvent::Unknown { .. } => (None, None, None),
    }
}

pub fn message_kind_str(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Text => "text",
        MessageKind::Image => "image",
        MessageKind::Video => "video",
        MessageKind::Audio => "audio",
        MessageKind::Voice => "voice",
        MessageKind::Sticker => "sticker",
        MessageKind::Document => "document",
        MessageKind::Contact => "contact",
        MessageKind::Location => "location",
        MessageKind::Poll => "poll",
        MessageKind::Reaction => "reaction",
    }
}

fn receipt_kind_str(k: crate::events::ReceiptKind) -> &'static str {
    match k {
        crate::events::ReceiptKind::Read => "read",
        crate::events::ReceiptKind::Delivered => "delivered",
        crate::events::ReceiptKind::Played => "played",
    }
}

fn group_change_kind_str(k: crate::events::GroupChangeKind) -> &'static str {
    match k {
        crate::events::GroupChangeKind::Join => "join",
        crate::events::GroupChangeKind::Leave => "leave",
        crate::events::GroupChangeKind::Promote => "promote",
        crate::events::GroupChangeKind::Demote => "demote",
        crate::events::GroupChangeKind::Subject => "subject",
        crate::events::GroupChangeKind::Icon => "icon",
        crate::events::GroupChangeKind::Description => "description",
    }
}

fn presence_kind_str(k: crate::events::PresenceKind) -> &'static str {
    match k {
        crate::events::PresenceKind::Available => "available",
        crate::events::PresenceKind::Unavailable => "unavailable",
        crate::events::PresenceKind::Typing => "typing",
        crate::events::PresenceKind::Recording => "recording",
    }
}

fn connection_kind_str(k: crate::events::ConnectionKind) -> &'static str {
    match k {
        crate::events::ConnectionKind::Connected => "connected",
        crate::events::ConnectionKind::Disconnected => "disconnected",
        crate::events::ConnectionKind::Replaced => "replaced",
        crate::events::ConnectionKind::LoggedOut => "logged_out",
        crate::events::ConnectionKind::Synced => "synced",
        crate::events::ConnectionKind::ClockSkewDetected => "clock_skew_detected",
    }
}

fn call_kind_str(k: crate::events::CallKind) -> &'static str {
    match k {
        crate::events::CallKind::Voice => "voice",
        crate::events::CallKind::Video => "video",
    }
}

fn call_state_str(s: crate::events::CallState) -> &'static str {
    match s {
        crate::events::CallState::Offered => "offered",
        crate::events::CallState::Accepted => "accepted",
        crate::events::CallState::Rejected => "rejected",
        crate::events::CallState::Terminated => "terminated",
    }
}

fn story_kind_str(k: crate::events::StoryKind) -> &'static str {
    match k {
        crate::events::StoryKind::Posted => "posted",
        crate::events::StoryKind::Viewed => "viewed",
    }
}

fn community_update_kind_str(k: crate::events::CommunityUpdateKind) -> &'static str {
    match k {
        crate::events::CommunityUpdateKind::Created => "created",
        crate::events::CommunityUpdateKind::Deactivated => "deactivated",
        crate::events::CommunityUpdateKind::Linked => "linked",
        crate::events::CommunityUpdateKind::Unlinked => "unlinked",
    }
}

fn newsletter_update_kind_str(k: crate::events::NewsletterUpdateKind) -> &'static str {
    match k {
        crate::events::NewsletterUpdateKind::Subscribed => "subscribed",
        crate::events::NewsletterUpdateKind::Unsubscribed => "unsubscribed",
        crate::events::NewsletterUpdateKind::MessageReceived => "message_received",
        crate::events::NewsletterUpdateKind::PictureChanged => "picture_changed",
        crate::events::NewsletterUpdateKind::NameChanged => "name_changed",
        crate::events::NewsletterUpdateKind::StateChanged => "state_changed",
    }
}

fn opt_text(v: Option<String>) -> Value {
    match v {
        Some(s) => Value::from(s),
        None => Value::null_unknown(),
    }
}

fn bool_i64(b: bool) -> Value {
    Value::from(if b { 1i64 } else { 0i64 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventEnvelope;
    use crate::query::schema::migrate;

    fn synth_message(id: u64, peer: &str, text: &str, ts: i64) -> InboundEvent {
        InboundEvent::parse(EventEnvelope {
            raw: format!(
                "Message(id: \"M{id}\", peer: \"{peer}\", sender: \"{peer}\", text: \"{text}\", kind: Text, is_group: false)"
            ),
            ts_unix_ms: ts,
            ts_mono_ns: 0,
        })
    }

    fn row_count(db: &Database, table: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let mut rows = db.query(&sql, ()).expect("count query");
        let row = rows.next().expect("at least one row").expect("ok row");
        row.get::<i64>(0).expect("i64")
    }

    fn row_count_where(db: &Database, table: &str, where_clause: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}");
        let mut rows = db.query(&sql, ()).expect("count query");
        let row = rows.next().expect("at least one row").expect("ok row");
        row.get::<i64>(0).expect("i64")
    }

    #[test]
    fn message_round_trip_through_events_and_messages() {
        let db = Database::open_in_memory().expect("open");
        migrate(&db).expect("migrate");
        let ingester = QueryIngester::new(db);
        for i in 0..3 {
            ingester
                .ingest(
                    10 + i,
                    (1000 + i as i64, 0),
                    &synth_message(10 + i, "peer_a", "hello", 1000 + i as i64),
                )
                .expect("ingest message");
        }
        assert_eq!(row_count(ingester.db(), "events"), 3);
        assert_eq!(row_count(ingester.db(), "messages"), 3);
        // denormalized columns populated
        assert_eq!(
            row_count_where(ingester.db(), "messages", "peer = 'peer_a'"),
            3
        );
        assert_eq!(
            row_count_where(ingester.db(), "events", "kind = 'message'"),
            3
        );
    }

    #[test]
    fn receipt_inserts_only_events_row() {
        let db = Database::open_in_memory().expect("open");
        migrate(&db).expect("migrate");
        let ingester = QueryIngester::new(db);
        let ev = InboundEvent::parse(EventEnvelope {
            raw: "Receipt(msg_id: \"M42\", peer: \"peer_b\", type: Sender, state: delivered)"
                .to_string(),
            ts_unix_ms: 2000,
            ts_mono_ns: 0,
        });
        ingester.ingest(42, (2000, 0), &ev).expect("ingest receipt");
        assert_eq!(row_count(ingester.db(), "events"), 1);
        assert_eq!(row_count(ingester.db(), "messages"), 0);
        assert_eq!(
            row_count_where(ingester.db(), "events", "kind = 'receipt'"),
            1
        );
        assert_eq!(
            row_count_where(ingester.db(), "events", "variant = 'delivered'"),
            1
        );
    }

    #[test]
    fn replay_same_event_id_is_idempotent() {
        let db = Database::open_in_memory().expect("open");
        migrate(&db).expect("migrate");
        let ingester = QueryIngester::new(db);
        let ev = synth_message(99, "peer_c", "replay me", 5000);
        ingester.ingest(99, (5000, 0), &ev).expect("ingest 1");
        ingester
            .ingest(99, (5000, 0), &ev)
            .expect("ingest 2 (replay)");
        ingester
            .ingest(99, (5000, 0), &ev)
            .expect("ingest 3 (replay)");
        assert_eq!(row_count(ingester.db(), "events"), 1);
        assert_eq!(row_count(ingester.db(), "messages"), 1);
    }

    #[test]
    fn variant_subkinds_extract_correctly() {
        let db = Database::open_in_memory().expect("open");
        migrate(&db).expect("migrate");
        let ingester = QueryIngester::new(db);
        // Receipt(Delivered) — parser reads `type` field (CamelCase
        // values: Delivered/Read/Played/Sender).
        ingester
            .ingest(
                1,
                (1, 0),
                &InboundEvent::parse(EventEnvelope {
                    raw: "Receipt(msg_id: \"x\", peer: \"p\", type: Delivered)".into(),
                    ts_unix_ms: 1,
                    ts_mono_ns: 0,
                }),
            )
            .unwrap();
        // Receipt(Read)
        ingester
            .ingest(
                2,
                (2, 0),
                &InboundEvent::parse(EventEnvelope {
                    raw: "Receipt(msg_id: \"y\", peer: \"p\", type: Read)".into(),
                    ts_unix_ms: 2,
                    ts_mono_ns: 0,
                }),
            )
            .unwrap();
        // GroupChange(subject)
        ingester
            .ingest(
                3,
                (3, 0),
                &InboundEvent::parse(EventEnvelope {
                    raw: "GroupChange(group_jid: \"g\", kind: Subject, after: \"name\", actor: \"x\")".into(),
                    ts_unix_ms: 3,
                    ts_mono_ns: 0,
                }),
            )
            .unwrap();
        assert_eq!(
            row_count_where(ingester.db(), "events", "variant = 'delivered'"),
            1
        );
        assert_eq!(
            row_count_where(ingester.db(), "events", "variant = 'read'"),
            1
        );
        assert_eq!(
            row_count_where(ingester.db(), "events", "variant = 'subject'"),
            1
        );
    }

    /// Receipts / Presence / Unknown variants carry `ts_unix_ms = 0`
    /// because the WA websocket doesn't include one. The ingester
    /// must fall back to the caller-supplied `recorded_at` so the
    /// SQL mirror has a useful chronological value (this matters
    /// for `ORDER BY ts_unix_ms DESC` and `since_ts_unix_ms`
    /// filters). Before 2026-07-12 every receipt row landed with
    /// `ts_unix_ms = 0`, breaking both.
    #[test]
    fn recorded_at_fallback_when_event_ts_is_zero() {
        let db = Database::open_in_memory().expect("open");
        migrate(&db).expect("migrate");
        let ingester = QueryIngester::new(db);
        let ev = InboundEvent::parse(EventEnvelope {
            // Receipt parser produces ts_unix_ms = 0 because the
            // raw payload doesn't carry one.
            raw: "Receipt(msg_id: \"M\", peer: \"p\", type: Delivered)".into(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        });
        let recorded_at: (i64, u64) = (1_783_887_512_357, 999);
        ingester
            .ingest(7, recorded_at, &ev)
            .expect("ingest zero-ts receipt");
        let mut rows = ingester
            .db()
            .query("SELECT ts_unix_ms, ts_mono_ns FROM events WHERE id = 7", ())
            .expect("q");
        let row = rows.next().expect("row").expect("ok");
        let ts: i64 = row.get::<i64>(0).unwrap();
        let mono: i64 = row.get::<i64>(1).unwrap();
        assert_eq!(
            ts, recorded_at.0,
            "ingester falls back to recorded_at when event ts is 0"
        );
        assert_eq!(mono, recorded_at.1 as i64);
    }

    /// When the event-internal ts is non-zero, the ingester must
    /// use it directly (don't overwrite with `recorded_at`). The
    /// recorded_at fallback is only for the zero case.
    #[test]
    fn event_internal_ts_takes_precedence_over_recorded_at() {
        let db = Database::open_in_memory().expect("open");
        migrate(&db).expect("migrate");
        let ingester = QueryIngester::new(db);
        let _ev = InboundEvent::parse(EventEnvelope {
            raw: "Receipt(msg_id: \"M\", peer: \"p\", type: Delivered)".into(),
            ts_unix_ms: 0, // parser sets to 0 anyway; we override below via Message
            ts_mono_ns: 0,
        });
        // Use a real Message variant so the event ts is non-zero.
        let ev = synth_message(8, "peer_x", "hi", 1234);
        let recorded_at: (i64, u64) = (9_999_999, 7);
        ingester.ingest(8, recorded_at, &ev).expect("ingest");
        let mut rows = ingester
            .db()
            .query("SELECT ts_unix_ms FROM events WHERE id = 8", ())
            .expect("q");
        let row = rows.next().expect("row").expect("ok");
        let ts: i64 = row.get::<i64>(0).unwrap();
        assert_eq!(
            ts, 1234,
            "event-internal ts (1234) is used, not recorded_at"
        );
    }
}
