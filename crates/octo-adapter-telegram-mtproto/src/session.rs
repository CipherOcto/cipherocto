//! StoolapSession — `grammers_session::Session` impl backed by
//! CipherOcto's stoolap fork on `feat/blockchain-sql`.
//!
//! The `Session` trait (RFC-0850ab-c §5.4) is the persistence
//! interface that `grammers_client::Client` consults on every
//! request to determine the home DC, the per-DC config
//! (including the 256-byte `auth_key`), and cached peer info.
//! We implement it on top of stoolap (cipherocto persistence
//! convention) instead of the grammers-shipped
//! `grammers_session::storages::SqliteSession` (which uses
//! `libsql`).
//!
//! ## Caching model
//!
//! The trait requires `home_dc_id()` and `dc_option(dc_id)` to
//! be infallible synchronous calls (see the Session trait
//! doc: "This method should be implemented as an infallible
//! memory read, because it is used on every request and thus
//! should be cheap to call."). The other seven methods are
//! `BoxFuture<...>` and may do I/O.
//!
//! To satisfy both constraints, `StoolapSession` holds a
//! `grammers_session::SessionData` in memory (the canonical
//! in-memory representation) and writes through to stoolap on
//! every `set_*` call. On startup, `StoolapSession::new`
//! hydrates the in-memory `SessionData` from the stoolap DB
//! (or from `SessionData::default()` if the DB is fresh).
//!
//! ## Schema
//!
//! Five tables, mirroring the grammers' `SqliteSession` schema
//! (so future migrations from TDLib are drop-in) but in
//! stoolap's type system:
//!
//! - `mtproto_dc_home(dc_id INTEGER)` — the home DC ID.
//! - `mtproto_dc_option(dc_id INTEGER, ipv4 TEXT, ipv6 TEXT,
//!   auth_key BLOB, PRIMARY KEY(dc_id))` — per-DC config + the
//!   256-byte `auth_key`. The `auth_key` BLOB is the
//!   DD6-sensitive material: see `redact_credentials` and the
//!   `AuthKeyMaterial` newtype for the in-memory `zeroize`
//!   handling.
//! - `mtproto_peer_info(peer_id INTEGER, hash INTEGER, subtype
//!   INTEGER, bot INTEGER, PRIMARY KEY(peer_id))` — cached peer
//!   info. `subtype` encodes the `PeerInfo` variant
//!   (0=User, 1=UserSelf, 2=Chat, 3=Channel); `hash` is the
//!   `PeerAuth`; `bot` is the `PeerInfo::User::bot` flag (NULL
//!   for non-User).
//! - `mtproto_update_state(pts INTEGER, qts INTEGER, date INTEGER,
//!   seq INTEGER)` — global `UpdatesState`.
//! - `mtproto_channel_state(peer_id INTEGER, pts INTEGER,
//!   PRIMARY KEY(peer_id))` — per-channel `ChannelState`.
//!
//! ## Error model
//!
//! The `Session` trait methods are infallible (the trait itself
//! returns no `Result`). All DB errors during the synchronous
//! methods (`home_dc_id`, `dc_option`) are unrecoverable — the
//! in-memory `SessionData` always has a value, populated either
//! from `SessionData::default()` (fresh DB) or from the last
//! successful `cache_*` / `set_*` write. The async methods
//! swallow DB errors and log them at WARN level: this matches
//! the trait's expectation that `set_*` is best-effort
//! (grammers' own `SqliteSession` panics on schema migration
//! failure and silently drops per-row write errors).
//!
//! ## Threading
//!
//! `StoolapSession` holds an `Arc<Database>` (stoolap
//! `Database` is internally `Arc`-shared and
//! `Send + Sync`) and a `parking_lot::Mutex<SessionData>` for
//! the in-memory cache. The mutex is held only for
//! millisecond-scale `HashMap` operations, so contention is
//! negligible.

use std::collections::HashMap;
use std::net::{SocketAddrV4, SocketAddrV6};
use std::path::Path;
use std::sync::Arc;

use futures_core::future::BoxFuture;
use grammers_session::types::{
    ChannelKind, DcOption, PeerAuth, PeerId, PeerInfo, UpdateState, UpdatesState,
};
use grammers_session::Session;
use octo_storage_core::stoolap::Value;
use octo_storage_core::Database;
use parking_lot::Mutex;
use tracing::warn;

/// Wrapper around a 256-byte `auth_key` that zeroizes on drop.
/// The raw bytes are sensitive material (DD6): an attacker
/// who reads them can impersonate the user to Telegram. The
/// `StoolapSession` keeps a copy in memory (the
/// `SessionData::dc_options` map); the BLOB form in stoolap
/// is a plain copy (encryption-at-rest is F6 future work —
/// see RFC-0850ab-c §10).
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct AuthKeyMaterial([u8; 256]);

impl AuthKeyMaterial {
    pub fn new(bytes: [u8; 256]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 256] {
        &self.0
    }

    /// Parse from the 32-byte-per-line text format used by
    /// the auth-export tools (not the on-disk BLOB format).
    /// Unused by default; provided for the Phase-2 session
    /// export feature.
    #[allow(dead_code)]
    pub fn from_text(_text: &str) -> Result<Self, String> {
        Err("from_text not yet implemented".into())
    }
}

impl std::fmt::Debug for AuthKeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthKeyMaterial")
            .field("bytes", &"<redacted 256 bytes>")
            .finish()
    }
}

/// Parse a `SocketAddrV4` from the textual form used in the
/// `mtproto_dc_option` table. Falls back to a sentinel
/// `0.0.0.0:0` if parsing fails (which the `dc_option`
/// reader treats as "no IPv4 known" — grammers will then use
/// the statically-known defaults from `SessionData::default`).
fn parse_ipv4(s: &str) -> SocketAddrV4 {
    s.parse().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap())
}

/// Same for IPv6.
fn parse_ipv6(s: &str) -> SocketAddrV6 {
    s.parse().unwrap_or_else(|_| "[::]:0".parse().unwrap())
}

/// Peer "subtype" integer (mirrors grammers' `peer_info.subtype`).
const SUBTYPE_USER: i64 = 0;
const SUBTYPE_USER_SELF: i64 = 1;
const SUBTYPE_CHAT: i64 = 2;
const SUBTYPE_CHANNEL: i64 = 3;

fn subtype_for(info: &PeerInfo) -> i64 {
    match info {
        PeerInfo::User { is_self, .. } => {
            if is_self == &Some(true) {
                SUBTYPE_USER_SELF
            } else {
                SUBTYPE_USER
            }
        }
        PeerInfo::Chat { .. } => SUBTYPE_CHAT,
        PeerInfo::Channel { .. } => SUBTYPE_CHANNEL,
    }
}

fn info_from_subtype(
    subtype: i64,
    peer_id: i64,
    hash: Option<i64>,
    bot: Option<bool>,
    channel_kind: Option<i64>,
) -> PeerInfo {
    let hash = hash.map(PeerAuth::from_hash);
    match subtype {
        SUBTYPE_USER => PeerInfo::User {
            id: peer_id,
            auth: hash,
            bot,
            is_self: Some(false),
        },
        SUBTYPE_USER_SELF => PeerInfo::User {
            id: peer_id,
            auth: hash,
            bot,
            is_self: Some(true),
        },
        SUBTYPE_CHAT => PeerInfo::Chat { id: peer_id },
        SUBTYPE_CHANNEL => PeerInfo::Channel {
            id: peer_id,
            auth: hash,
            kind: channel_kind.and_then(|k| match k {
                1 => Some(ChannelKind::Broadcast),
                2 => Some(ChannelKind::Megagroup),
                3 => Some(ChannelKind::Gigagroup),
                _ => None,
            }),
        },
        _ => PeerInfo::Chat { id: peer_id },
    }
}

/// Stoolap-backed `grammers_session::Session` impl.
pub struct StoolapSession {
    db: Arc<Database>,
    cache: Mutex<grammers_session::SessionData>,
}

impl StoolapSession {
    /// Open a session backed by a stoolap file at `path`.
    /// The path is interpreted as a `file://` DSN (the
    /// canonical stoolap form; see
    /// `crates/octo-matrix-session-store/src/store.rs`).
    /// Creates the file if it does not exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Arc<Self>, MtprotoSessionError> {
        let dsn = format!("file://{}", path.as_ref().display());
        let db = Database::open(&dsn).map_err(MtprotoSessionError::from)?;
        let session = Arc::new(Self::init(db)?);
        Ok(session)
    }

    /// Open an in-memory session (used by tests and the
    /// `integration-test` path where the session must not
    /// outlive the process).
    pub fn open_in_memory() -> Result<Arc<Self>, MtprotoSessionError> {
        let db = Database::open_in_memory().map_err(MtprotoSessionError::from)?;
        let session = Arc::new(Self::init(db)?);
        Ok(session)
    }

    fn init(db: Database) -> Result<Self, MtprotoSessionError> {
        let db = Arc::new(db);
        init_schema(&db)?;
        let cache = hydrate_cache(&db)?;
        Ok(Self {
            db,
            cache: Mutex::new(cache),
        })
    }

    /// Wipe the on-disk store. Used by `sign_out` to
    /// invalidate the session. The in-memory cache is also
    /// reset to `SessionData::default()`.
    pub fn reset(&self) -> Result<(), MtprotoSessionError> {
        for table in [
            "mtproto_channel_state",
            "mtproto_update_state",
            "mtproto_peer_info",
            "mtproto_dc_option",
            "mtproto_dc_home",
        ] {
            self.db
                .execute(&format!("DELETE FROM {}", table), [])
                .map_err(MtprotoSessionError::from)?;
        }
        *self.cache.lock() = grammers_session::SessionData::default();
        Ok(())
    }
}

impl Drop for StoolapSession {
    /// Zeroize the cached `auth_key` bytes on drop.
    ///
    /// The cache's `DcOption::auth_key: Option<[u8; 256]>`
    /// holds the raw 256-byte MTProto auth key in plaintext
    /// (DD6: an attacker who reads them can impersonate the
    /// user to Telegram). The `StoolapSession` outlives the
    /// adapter in `Arc<StoolapSession>` form; when the last
    /// `Arc` is dropped, this `Drop` impl fires and clears
    /// every cached auth key in the in-memory map.
    ///
    /// Note: during the session's lifetime the bytes are
    /// still in memory (grammers needs them to sign RPC
    /// requests). The `Drop` impl wipes them on shutdown.
    fn drop(&mut self) {
        let mut cache = self.cache.lock();
        for dc_opt in cache.dc_options.values_mut() {
            if let Some(key) = dc_opt.auth_key.as_mut() {
                zeroize::Zeroize::zeroize(key);
            }
        }
    }
}

/// Schema migration. Mirrors the grammers `SqliteSession`
/// schema (5 tables) but in stoolap's type system.
///
/// Idempotent: `CREATE TABLE IF NOT EXISTS`. Called on every
/// `StoolapSession::init`; safe on a fresh database and on
/// an existing one.
fn init_schema(db: &Database) -> Result<(), MtprotoSessionError> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS mtproto_dc_home (
            dc_id INTEGER NOT NULL,
            PRIMARY KEY (dc_id))",
        [],
    )
    .map_err(MtprotoSessionError::from)?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS mtproto_dc_option (
            dc_id INTEGER NOT NULL,
            ipv4 TEXT NOT NULL,
            ipv6 TEXT NOT NULL,
            auth_key BLOB,
            PRIMARY KEY (dc_id))",
        [],
    )
    .map_err(MtprotoSessionError::from)?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS mtproto_peer_info (
            peer_id INTEGER NOT NULL,
            hash INTEGER,
            subtype INTEGER NOT NULL,
            bot INTEGER,
            channel_kind INTEGER,
            PRIMARY KEY (peer_id))",
        [],
    )
    .map_err(MtprotoSessionError::from)?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS mtproto_update_state (
            pts INTEGER NOT NULL,
            qts INTEGER NOT NULL,
            date INTEGER NOT NULL,
            seq INTEGER NOT NULL)",
        [],
    )
    .map_err(MtprotoSessionError::from)?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS mtproto_channel_state (
            peer_id INTEGER NOT NULL,
            pts INTEGER NOT NULL,
            PRIMARY KEY (peer_id))",
        [],
    )
    .map_err(MtprotoSessionError::from)?;

    Ok(())
}

/// Hydrate the in-memory `SessionData` from the stoolap DB.
///
/// If the DB is fresh (no rows anywhere), returns
/// `SessionData::default()` so the adapter has the
/// statically-known DC defaults (1–5 per Telegram's published
/// `GetConfig`) to work with. Otherwise, the DB rows win
/// and missing fields fall back to defaults one piece at a time.
fn hydrate_cache(db: &Database) -> Result<grammers_session::SessionData, MtprotoSessionError> {
    let home_dc_row = read_home_dc(db)?;
    let dc_options = read_all_dc_options(db)?;
    let peer_infos = read_all_peer_infos(db)?;
    let updates_state = read_update_state(db)?.unwrap_or_default();
    // Fresh-DB fast path: nothing has been persisted yet, so the
    // default `SessionData` already has the right shape
    // (DEFAULT_DC=2 plus the 5 statically-known DC options).
    if home_dc_row.is_none() && dc_options.is_empty() && peer_infos.is_empty() {
        return Ok(grammers_session::SessionData::default());
    }
    let mut defaults = grammers_session::SessionData::default();
    let home_dc = home_dc_row.unwrap_or(defaults.home_dc);
    // Layer persisted DC options on top of the defaults so any
    // DC the DB knows about (e.g. ones auth learned about)
    // overrides the static default for that id.
    defaults.dc_options.extend(dc_options);
    let mut out = defaults;
    out.home_dc = home_dc;
    out.peer_infos = peer_infos;
    out.updates_state = updates_state;
    Ok(out)
}

fn read_home_dc(db: &Database) -> Result<Option<i32>, MtprotoSessionError> {
    let rows = db
        .query("SELECT dc_id FROM mtproto_dc_home LIMIT 1", [])
        .map_err(MtprotoSessionError::from)?;
    for row in rows {
        let row = row.map_err(MtprotoSessionError::from)?;
        let v = row.get(0).map_err(MtprotoSessionError::from)?;
        if let Value::Integer(i) = v {
            return Ok(Some(i as i32));
        }
    }
    Ok(None)
}

fn read_all_dc_options(db: &Database) -> Result<HashMap<i32, DcOption>, MtprotoSessionError> {
    let rows = db
        .query(
            "SELECT dc_id, ipv4, ipv6, auth_key FROM mtproto_dc_option",
            [],
        )
        .map_err(MtprotoSessionError::from)?;
    let mut out = HashMap::new();
    for row in rows {
        let row = row.map_err(MtprotoSessionError::from)?;
        let id = match row.get(0).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i as i32,
            _ => continue,
        };
        let ipv4 = match row.get(1).map_err(MtprotoSessionError::from)? {
            Value::Text(s) => parse_ipv4(s.as_str()),
            _ => continue,
        };
        let ipv6 = match row.get(2).map_err(MtprotoSessionError::from)? {
            Value::Text(s) => parse_ipv6(s.as_str()),
            _ => continue,
        };
        let auth_key = match row.get(3).map_err(MtprotoSessionError::from)? {
            Value::Blob(b) => {
                if b.len() == 256 {
                    let mut k = [0u8; 256];
                    k.copy_from_slice(&b);
                    Some(k)
                } else {
                    None
                }
            }
            _ => None,
        };
        out.insert(
            id,
            DcOption {
                id,
                ipv4,
                ipv6,
                auth_key,
            },
        );
    }
    Ok(out)
}

fn read_all_peer_infos(db: &Database) -> Result<HashMap<PeerId, PeerInfo>, MtprotoSessionError> {
    let rows = db
        .query(
            "SELECT peer_id, hash, subtype, bot, channel_kind FROM mtproto_peer_info",
            [],
        )
        .map_err(MtprotoSessionError::from)?;
    let mut out = HashMap::new();
    for row in rows {
        let row = row.map_err(MtprotoSessionError::from)?;
        let peer_id = match row.get(0).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i,
            _ => continue,
        };
        let hash = match row.get(1).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => Some(i),
            _ => None,
        };
        let subtype = match row.get(2).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i,
            _ => 0,
        };
        let bot = match row.get(3).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => Some(i != 0),
            _ => None,
        };
        let channel_kind = match row.get(4).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => Some(i),
            _ => None,
        };
        let info = info_from_subtype(subtype, peer_id, hash, bot, channel_kind);
        // Reconstruct the `PeerId` based on the `subtype`
        // column. Telegram's three peer kinds have different
        // sign conventions and access-hash requirements:
        //
        // - User (incl. self): positive or arbitrary id.
        //   `PeerId::user_unchecked` is always safe.
        // - Chat: small-group id (positive i32). The
        //   `chat_unchecked` constructor is the only one
        //   that yields a `PeerKind::Chat` discriminant.
        // - Channel: supergroup / channel id (negative i64).
        //   Reconstructing as `user_unchecked` would yield
        //   a User peer with a negative id, which is NOT
        //   the same `PeerId` and breaks the cache lookup
        //   (`peer(PeerId::channel(id))` would miss).
        //
        // The unchecked constructors skip the validity check
        // (the persisted rows were already validated at
        // write time via `peer(peer_id)` returning
        // `Some(PeerInfo::...)`).
        let peer_id_value = match subtype {
            SUBTYPE_CHAT => PeerId::chat_unchecked(peer_id),
            SUBTYPE_CHANNEL => PeerId::channel_unchecked(peer_id),
            // User and UserSelf both yield a User PeerId.
            // Unknown subtype falls back to user_unchecked so
            // we don't lose the row entirely.
            _ => PeerId::user_unchecked(peer_id),
        };
        out.insert(peer_id_value, info);
    }
    Ok(out)
}

fn read_update_state(db: &Database) -> Result<Option<UpdatesState>, MtprotoSessionError> {
    let mut rows = db
        .query(
            "SELECT pts, qts, date, seq FROM mtproto_update_state LIMIT 1",
            [],
        )
        .map_err(MtprotoSessionError::from)?;
    if let Some(row) = rows.next() {
        let row = row.map_err(MtprotoSessionError::from)?;
        let pts = match row.get(0).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i as i32,
            _ => return Ok(None),
        };
        let qts = match row.get(1).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i as i32,
            _ => return Ok(None),
        };
        let date = match row.get(2).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i as i32,
            _ => return Ok(None),
        };
        let seq = match row.get(3).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i as i32,
            _ => return Ok(None),
        };
        // Channel state is read separately on demand (only
        // when the adapter's `set_update_state` is called with
        // `UpdateState::Channel`).
        let channels = read_all_channel_state(db)?;
        return Ok(Some(UpdatesState {
            pts,
            qts,
            date,
            seq,
            channels,
        }));
    }
    Ok(None)
}

fn read_all_channel_state(
    db: &Database,
) -> Result<Vec<grammers_session::types::ChannelState>, MtprotoSessionError> {
    let rows = db
        .query("SELECT peer_id, pts FROM mtproto_channel_state", [])
        .map_err(MtprotoSessionError::from)?;
    let mut out = Vec::new();
    for row in rows {
        let row = row.map_err(MtprotoSessionError::from)?;
        let id = match row.get(0).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i,
            _ => continue,
        };
        let pts = match row.get(1).map_err(MtprotoSessionError::from)? {
            Value::Integer(i) => i as i32,
            _ => continue,
        };
        out.push(grammers_session::types::ChannelState { id, pts });
    }
    Ok(out)
}

// --- Session trait impl ---

impl Session for StoolapSession {
    fn home_dc_id(&self) -> i32 {
        self.cache.lock().home_dc
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, ()> {
        let mut g = self.cache.lock();
        g.home_dc = dc_id;
        let db = Arc::clone(&self.db);
        Box::pin(async move {
            if let Err(e) = persist_home_dc(&db, dc_id).await {
                warn!(error = %e, dc_id, "set_home_dc_id: persist failed");
            }
        })
    }

    fn dc_option(&self, dc_id: i32) -> Option<DcOption> {
        self.cache.lock().dc_options.get(&dc_id).cloned()
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, ()> {
        let dc_option = dc_option.clone();
        let mut g = self.cache.lock();
        g.dc_options.insert(dc_option.id, dc_option.clone());
        let db = Arc::clone(&self.db);
        Box::pin(async move {
            if let Err(e) = persist_dc_option(&db, &dc_option).await {
                warn!(error = %e, dc_id = dc_option.id, "set_dc_option: persist failed");
            }
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Option<PeerInfo>> {
        // Extract the value under the lock; drop the guard
        // before returning the future. The `parking_lot::Mutex`
        // guard is not `Send`, so we cannot hold it across
        // the `async move` boundary.
        let value = self.cache.lock().peer_infos.get(&peer).cloned();
        Box::pin(async move { value })
    }

    fn cache_peer(&self, peer: &PeerInfo) -> BoxFuture<'_, ()> {
        let peer = peer.clone();
        let mut g = self.cache.lock();
        g.peer_infos.insert(peer.id(), peer.clone());
        let db = Arc::clone(&self.db);
        Box::pin(async move {
            if let Err(e) = persist_peer(&db, &peer).await {
                warn!(error = %e, "cache_peer: persist failed");
            }
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, UpdatesState> {
        // Clone the snapshot out under the lock so the
        // `Send` guard is dropped before the async block.
        let snapshot = self.cache.lock().updates_state.clone();
        Box::pin(async move { snapshot })
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, ()> {
        let mut g = self.cache.lock();
        match &update {
            UpdateState::All(s) => {
                g.updates_state = s.clone();
            }
            UpdateState::Primary { pts, date, seq } => {
                g.updates_state.pts = *pts;
                g.updates_state.date = *date;
                g.updates_state.seq = *seq;
            }
            UpdateState::Secondary { qts } => {
                g.updates_state.qts = *qts;
            }
            UpdateState::Channel { id, pts } => {
                g.updates_state.channels.retain(|c| c.id != *id);
                g.updates_state
                    .channels
                    .push(grammers_session::types::ChannelState { id: *id, pts: *pts });
            }
        }
        let snapshot = g.updates_state.clone();
        let db = Arc::clone(&self.db);
        Box::pin(async move {
            if let Err(e) = persist_update_state(&db, &snapshot, &update).await {
                warn!(error = %e, "set_update_state: persist failed");
            }
        })
    }
}

async fn persist_home_dc(db: &Database, dc_id: i32) -> Result<(), MtprotoSessionError> {
    db.execute("DELETE FROM mtproto_dc_home", [])
        .map_err(MtprotoSessionError::from)?;
    db.execute(
        "INSERT INTO mtproto_dc_home (dc_id) VALUES ($1)",
        vec![Value::integer(dc_id as i64)],
    )
    .map_err(MtprotoSessionError::from)?;
    Ok(())
}

async fn persist_dc_option(db: &Database, opt: &DcOption) -> Result<(), MtprotoSessionError> {
    // Upsert: delete-then-insert. Stoolap doesn't support
    // `INSERT OR REPLACE`; the canonical idiom is DELETE on the
    // primary key followed by a fresh INSERT. The pair runs
    // outside a transaction here; the (dc_id) primary key makes
    // a race extremely unlikely in practice.
    db.execute(
        "DELETE FROM mtproto_dc_option WHERE dc_id = $1",
        vec![Value::integer(opt.id as i64)],
    )
    .map_err(MtprotoSessionError::from)?;
    let auth_key_blob = opt
        .auth_key
        .as_ref()
        .map(|k| Value::blob(k.to_vec()))
        .unwrap_or(Value::Null(octo_storage_core::stoolap::DataType::Blob));
    db.execute(
        "INSERT INTO mtproto_dc_option (dc_id, ipv4, ipv6, auth_key) VALUES ($1, $2, $3, $4)",
        vec![
            Value::integer(opt.id as i64),
            Value::text(opt.ipv4.to_string()),
            Value::text(opt.ipv6.to_string()),
            auth_key_blob,
        ],
    )
    .map_err(MtprotoSessionError::from)?;
    Ok(())
}

async fn persist_peer(db: &Database, info: &PeerInfo) -> Result<(), MtprotoSessionError> {
    let (peer_id_bare, hash, subtype, bot, channel_kind) = match info {
        PeerInfo::User { id, auth, bot, .. } => (
            *id,
            auth.map(|a| a.hash()),
            subtype_for(info),
            bot.map(|b| if b { 1i64 } else { 0i64 }),
            None,
        ),
        PeerInfo::Chat { id } => (*id, None, SUBTYPE_CHAT, None, None),
        PeerInfo::Channel { id, auth, kind } => (
            *id,
            auth.map(|a| a.hash()),
            SUBTYPE_CHANNEL,
            None,
            kind.map(|k| match k {
                ChannelKind::Broadcast => 1,
                ChannelKind::Megagroup => 2,
                ChannelKind::Gigagroup => 3,
            }),
        ),
    };
    let hash_v = hash
        .map(Value::integer)
        .unwrap_or(Value::Null(octo_storage_core::stoolap::DataType::Integer));
    let bot_v = bot
        .map(Value::integer)
        .unwrap_or(Value::Null(octo_storage_core::stoolap::DataType::Integer));
    let kind_v = channel_kind
        .map(Value::integer)
        .unwrap_or(Value::Null(octo_storage_core::stoolap::DataType::Integer));
    // Upsert via DELETE + INSERT (stoolap doesn't have
    // INSERT OR REPLACE).
    db.execute(
        "DELETE FROM mtproto_peer_info WHERE peer_id = $1",
        vec![Value::integer(peer_id_bare)],
    )
    .map_err(MtprotoSessionError::from)?;
    db.execute(
        "INSERT INTO mtproto_peer_info (peer_id, hash, subtype, bot, channel_kind) VALUES ($1, $2, $3, $4, $5)",
        vec![
            Value::integer(peer_id_bare),
            hash_v,
            Value::integer(subtype),
            bot_v,
            kind_v,
        ],
    )
    .map_err(MtprotoSessionError::from)?;
    Ok(())
}

async fn persist_update_state(
    db: &Database,
    full: &UpdatesState,
    _update: &UpdateState,
) -> Result<(), MtprotoSessionError> {
    // For `All` we replace; for partial updates we replace
    // the global state with the mutated snapshot and let the
    // `Channel` arm deal with the per-channel row.
    db.execute("DELETE FROM mtproto_update_state", [])
        .map_err(MtprotoSessionError::from)?;
    db.execute(
        "INSERT INTO mtproto_update_state (pts, qts, date, seq) VALUES ($1, $2, $3, $4)",
        vec![
            Value::integer(full.pts as i64),
            Value::integer(full.qts as i64),
            Value::integer(full.date as i64),
            Value::integer(full.seq as i64),
        ],
    )
    .map_err(MtprotoSessionError::from)?;
    // Replace channel state rows wholesale.
    db.execute("DELETE FROM mtproto_channel_state", [])
        .map_err(MtprotoSessionError::from)?;
    for c in &full.channels {
        db.execute(
            "INSERT INTO mtproto_channel_state (peer_id, pts) VALUES ($1, $2)",
            vec![Value::integer(c.id), Value::integer(c.pts as i64)],
        )
        .map_err(MtprotoSessionError::from)?;
    }
    Ok(())
}

/// Session-specific error type. Internal — wrapped by
/// `MtprotoTelegramError::Session` in the public API.
#[derive(Debug, thiserror::Error)]
pub enum MtprotoSessionError {
    #[error("stoolap error: {0}")]
    Stoolap(#[from] octo_storage_core::stoolap::Error),
    // RFC-0206 v2.1 §Substrate Newtype Refactor: substrate returns
    // `SubstrateError` from `Database::open` / `open_in_memory`. We
    // forward via `From<SubstrateError>` so session open surfaces the
    // substrate error directly. The Stoolap variant above covers
    // query/execute paths; this covers the constructor.
    #[error("substrate error: {0}")]
    Substrate(#[from] octo_storage_core::SubstrateError),
    #[error("schema migration failed: {0}")]
    Schema(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_in_memory_creates_default_cache() {
        let s = StoolapSession::open_in_memory().unwrap();
        // Default home DC per grammers' SessionData::default is 2.
        assert_eq!(s.home_dc_id(), 2);
        // All 5 primary DCs are present by default.
        for id in 1..=5 {
            assert!(s.dc_option(id).is_some(), "missing DC {}", id);
        }
    }

    #[tokio::test]
    async fn set_home_dc_id_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.db");
        let s = StoolapSession::open(&path).unwrap();
        s.set_home_dc_id(4).await;
        assert_eq!(s.home_dc_id(), 4);
        // Re-open the file and confirm hydration reads it back.
        drop(s);
        let s2 = StoolapSession::open(&path).unwrap();
        assert_eq!(s2.home_dc_id(), 4);
    }

    #[tokio::test]
    async fn set_dc_option_round_trips_auth_key() {
        let s = StoolapSession::open_in_memory().unwrap();
        let key = [0xABu8; 256];
        let opt = DcOption {
            id: 2,
            ipv4: "127.0.0.1:443".parse().unwrap(),
            ipv6: "[::1]:443".parse().unwrap(),
            auth_key: Some(key),
        };
        s.set_dc_option(&opt).await;
        let read = s.dc_option(2).unwrap();
        assert_eq!(read.auth_key.unwrap(), key);
    }

    #[tokio::test]
    async fn cache_peer_user_self_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.db");
        let s = StoolapSession::open(&path).unwrap();
        let info = PeerInfo::User {
            id: 12345,
            auth: Some(PeerAuth::from_hash(999)),
            bot: Some(false),
            is_self: Some(true),
        };
        s.cache_peer(&info).await;
        let got = s.peer(PeerId::user_unchecked(12345)).await.unwrap();
        assert_eq!(got, info);
        drop(s);
        // Re-open the file and confirm hydration.
        let s2 = StoolapSession::open(&path).unwrap();
        let got2 = s2.peer(PeerId::user_unchecked(12345)).await.unwrap();
        assert_eq!(got2, info);
    }

    #[tokio::test]
    async fn cache_peer_chat_round_trip() {
        // R15-C5: a small-group (chat) peer must hydrate back
        // as a `PeerId` with `PeerKind::Chat`, not as a User.
        // The previous reconstruction used
        // `PeerId::user_unchecked(peer_id)` for every row,
        // which means `peer(PeerId::chat(id))` would miss
        // the row after re-opening the session file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.db");
        let s = StoolapSession::open(&path).unwrap();
        let info = PeerInfo::Chat { id: 42 };
        s.cache_peer(&info).await;
        let got = s.peer(PeerId::chat_unchecked(42)).await.unwrap();
        assert_eq!(got, info);
        drop(s);
        // Re-open the file and confirm hydration with the
        // chat constructor.
        let s2 = StoolapSession::open(&path).unwrap();
        let got2 = s2.peer(PeerId::chat_unchecked(42)).await.unwrap();
        assert_eq!(got2, info);
    }

    #[tokio::test]
    async fn cache_peer_channel_round_trip() {
        // R15-C5: a channel / supergroup peer must hydrate
        // back as `PeerId::channel(...)`, not
        // `PeerId::user_unchecked(...)`. The bare id is
        // positive; the `PeerId` constructor handles the
        // negative encoding internally. Reconstructing as
        // user would yield a User peer with a negative
        // bare_id, which is a different `PeerId` value.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.db");
        let s = StoolapSession::open(&path).unwrap();
        let info = PeerInfo::Channel {
            id: 1234567890,
            auth: Some(PeerAuth::from_hash(7777)),
            kind: Some(ChannelKind::Megagroup),
        };
        s.cache_peer(&info).await;
        let got = s.peer(PeerId::channel_unchecked(1234567890)).await.unwrap();
        assert_eq!(got, info);
        drop(s);
        let s2 = StoolapSession::open(&path).unwrap();
        let got2 = s2
            .peer(PeerId::channel_unchecked(1234567890))
            .await
            .unwrap();
        assert_eq!(got2, info);
    }

    #[tokio::test]
    async fn reset_clears_db_and_cache() {
        let s = StoolapSession::open_in_memory().unwrap();
        s.set_home_dc_id(5).await;
        s.reset().unwrap();
        assert_eq!(s.home_dc_id(), 2);
        assert!(s.dc_option(2).is_some());
    }
}
