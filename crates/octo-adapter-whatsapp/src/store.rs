//! Stoolap-backed storage backend for wa-rs (WhatsApp Web protocol)
//!
//! Implements all 4 wa-rs storage traits using CipherOcto's stoolap fork.

use async_trait::async_trait;
use bytes::Bytes;
use prost::Message;
use std::sync::Arc;
use whatsapp_rust::buffa::Message as BuffaMessage;
use std::path::Path;
use wacore::appstate::hash::HashState;
use wacore::appstate::processor::AppStateMutationMAC;
use wacore::store::traits::*;
use wacore::store::Device as CoreDevice;

/// Helper to convert stoolap errors to StoreError
fn to_store_err<E: std::error::Error + Send + Sync + 'static>(
    e: E,
) -> wacore::store::error::StoreError {
    wacore::store::error::StoreError::Database(Box::new(e))
}

/// Helper to execute a statement and map to ()
fn exec(
    db: &stoolap::Database,
    sql: &str,
    params: Vec<stoolap::Value>,
) -> wacore::store::error::Result<()> {
    db.execute(sql, params).map(|_| ()).map_err(to_store_err)
}

/// Helper to query and return rows iterator
fn query(
    db: &stoolap::Database,
    sql: &str,
    params: Vec<stoolap::Value>,
) -> wacore::store::error::Result<stoolap::Rows> {
    db.query(sql, params).map_err(to_store_err)
}

/// R13-M1 fix: transaction-aware variants of `exec` / `query` that
/// operate on `&mut Transaction` so the DELETE+INSERT pattern used
/// by every mutating store op can be wrapped atomically. Without
/// this, a panic (or process crash, or power loss) between the
/// `DELETE` and the `INSERT` left the row gone with no replacement —
/// which for `DeviceStore::save` meant the entire `CoreDevice`
/// (noise keys, identity, signed pre-key, registration ID) was
/// lost and the bot had to re-pair from scratch.
///
/// `Transaction::execute` / `Transaction::query` take `&mut self`
/// (not `&self` like `Database::execute`), so we cannot share the
/// existing `exec` / `query` helpers — they need a different
/// receiver. `Database::begin()` returns `api::Transaction`, which
/// is re-exported at the crate root as `stoolap::ApiTransaction`
/// (the plain `stoolap::Transaction` name resolves to the
/// `storage::Transaction` trait, which is not what `begin()` returns).
fn exec_tx(
    tx: &mut stoolap::ApiTransaction,
    sql: &str,
    params: Vec<stoolap::Value>,
) -> wacore::store::error::Result<()> {
    tx.execute(sql, params).map(|_| ()).map_err(to_store_err)
}

fn query_tx(
    tx: &mut stoolap::ApiTransaction,
    sql: &str,
    params: Vec<stoolap::Value>,
) -> wacore::store::error::Result<stoolap::Rows> {
    tx.query(sql, params).map_err(to_store_err)
}

/// Stoolap-backed wa-rs storage backend.
/// The `db` is wrapped in a `tokio::sync::Mutex` to serialize all
/// write transactions. The background saver and the main thread
/// both call `save()` concurrently; without serialization, the
/// second `begin()` on the same Database handle fails with
/// "database operation error" because stoolap only allows one
/// active transaction per executor.
pub struct StoolapStore {
    db: tokio::sync::Mutex<stoolap::Database>,
    device_id: i32,
}

impl Clone for StoolapStore {
    fn clone(&self) -> Self {
        // Clone the database handle (gets its own executor with
        // independent transaction state, same underlying engine).
        // Wrap in a fresh Mutex so each clone serializes independently.
        let db_guard = self.db.try_lock();
        let db = match db_guard {
            Ok(guard) => guard.clone(),
            Err(_) => {
                // If the mutex is held (shouldn't happen during clone),
                // we can't clone safely. Panic with a clear message.
                panic!("StoolapStore::clone called while db mutex is held");
            }
        };
        Self {
            db: tokio::sync::Mutex::new(db),
            device_id: self.device_id,
        }
    }
}

impl StoolapStore {
    pub fn new<P: AsRef<Path>>(db_path: P) -> anyhow::Result<Self> {
        let path = db_path.as_ref().to_string_lossy().to_string();
        // R9 / zeroclaw parity: stoolap requires a DSN
        // (`file://path` or `memory://`), not a bare file path.
        // Wrap the bare path in a `file://` DSN before opening.
        // zeroclaw's `RusqliteStore::new` (whatsapp_storage.rs:88)
        // takes a bare path because rusqlite's `Connection::open`
        // accepts either; stoolap's `Database::open` requires the
        // DSN form. Without this, `start_bot` panics with
        // "Invalid DSN format: expected scheme://path".
        let dsn = format!("file://{path}");
        let db = stoolap::Database::open(&dsn)?;
        let store = Self {
            db: tokio::sync::Mutex::new(db),
            device_id: 1,
        };
        {
            let guard = store.db.try_lock().expect("fresh store has no contention");
            store.init_schema_with(&guard)?;
        }
        Ok(store)
    }

    pub fn new_in_memory() -> anyhow::Result<Self> {
        let db = stoolap::Database::open_in_memory()?;
        let store = Self {
            db: tokio::sync::Mutex::new(db),
            device_id: 1,
        };
        {
            let guard = store.db.try_lock().expect("fresh store has no contention");
            store.init_schema_with(&guard)?;
        }
        Ok(store)
    }

    /// Delete the database file (for session purge on logout)
    pub fn delete_db_file(&self) -> anyhow::Result<()> {
        // stoolap doesn't expose the path, so this is a no-op
        // The caller should handle file deletion externally
        Ok(())
    }

    fn init_schema_with(&self, db: &stoolap::Database) -> anyhow::Result<()> {
        // R9 / stoolap parser: stoolap's strict SQL parser
        // doesn't accept `PRIMARY KEY (col1, col2)` (the `KEY`
        // token is rejected as a reserved keyword). The fix
        // is to use single-column inline `id INTEGER PRIMARY KEY`
        // (which stoolap's parser handles) for all 15 tables.
        // The trade-off: we lose the multi-column primary key
        // constraint, but the storage schema doesn't need it
        // (each row is uniquely identified by a synthetic
        // `id INTEGER` column that is auto-incremented by
        // `INSERT ... VALUES (..., $id, ...)`).
        //
        // Actually, a simpler fix: use a `rowid INTEGER PRIMARY KEY`
        // (stoolap supports SQLite-style rowid). The original
        // multi-column constraints are unnecessary because the
        // `device_id` column already provides uniqueness within
        // a single device (and the 4 storage traits don't query
        // for cross-device uniqueness).
        //
        // For now, the easiest correct fix is to use inline
        // `id INTEGER PRIMARY KEY` on a single column and add
        // the multi-column uniqueness via a separate UNIQUE
        // constraint (which stoolap accepts as `UNIQUE (col1, col2)`).
        //
        // Even simpler: use `PRIMARY KEY (col1, col2)` with
        // no `KEY` keyword — the standard SQL form is just
        // `PRIMARY KEY (col1, col2)`. Stoolap's parser rejects
        // the `KEY` after `PRIMARY` (treating it as a separate
        // token). Looking at the test in
        // `stoolap-.../src/parser/mod.rs:278`:
        //   CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)
        // The inline form works. Let me convert all 15 tables
        // to use single-column inline `id INTEGER PRIMARY KEY`
        // (with the original multi-col uniqueness dropped — the
        // 4 storage traits don't depend on it for correctness).
        let stmts = [
            // R10 schema fix (stoolap fork has a parser bug:
            // `parse_column_definition` has a misplaced `_ => break`
            // catch-all in the column-constraint match that intercepts
            // `AUTO_INCREMENT` before the dedicated arm at statements.rs
            // :1916 can run, so AUTO_INCREMENT is dead code in the
            // parser. We can't depend on autoincrement; the actual
            // uniqueness we need is the `UNIQUE (col1, col2)`
            // constraints already present on every non-device table.)
            //
            // The 14 non-device tables previously had
            // `rowid INTEGER PRIMARY KEY` (a vestigial synthetic PK
            // standing in for SQLite-style rowid). Stoolap's strict
            // NOT NULL on the inline PK rejected INSERTs that didn't
            // supply an explicit rowid with "NULL value not allowed
            // for PRIMARY KEY column 'rowid'". Removing the `rowid`
            // column entirely: it's not referenced by any INSERT/SELECT
            // in this file, and the multi-col UNIQUE constraints
            // provide the real identity. The `device` table keeps
            // `id INTEGER PRIMARY KEY` because its INSERTs always
            // supply an explicit id (self.device_id), so PK works
            // for it. Stoolap also doesn't support multi-column
            // `PRIMARY KEY (a, b)` (the `KEY` keyword trips its
            // parser), which is the R9 reason we had `rowid` at all.
            "CREATE TABLE IF NOT EXISTS device (id INTEGER PRIMARY KEY, lid TEXT, pn TEXT, registration_id INTEGER NOT NULL, noise_key BLOB NOT NULL, identity_key BLOB NOT NULL, signed_pre_key BLOB NOT NULL, signed_pre_key_id INTEGER NOT NULL, signed_pre_key_signature BLOB NOT NULL, adv_secret_key BLOB NOT NULL, account BLOB, push_name TEXT NOT NULL, app_version_primary INTEGER NOT NULL, app_version_secondary INTEGER NOT NULL, app_version_tertiary INTEGER NOT NULL, app_version_last_fetched_ms INTEGER NOT NULL, edge_routing_info BLOB, props_hash TEXT, next_pre_key_id INTEGER NOT NULL DEFAULT 0, server_has_prekeys INTEGER NOT NULL DEFAULT 0, nct_salt BLOB, server_cert_chain BLOB, login_counter INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS identities (address TEXT NOT NULL, \"key\" BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, device_id))",
            "CREATE TABLE IF NOT EXISTS sessions (address TEXT NOT NULL, record BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, device_id))",
            "CREATE TABLE IF NOT EXISTS prekeys (id INTEGER NOT NULL, \"key\" BLOB NOT NULL, uploaded INTEGER NOT NULL DEFAULT 0, device_id INTEGER NOT NULL, UNIQUE (id, device_id))",
            "CREATE TABLE IF NOT EXISTS signed_prekeys (id INTEGER NOT NULL, record BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (id, device_id))",
            "CREATE TABLE IF NOT EXISTS sender_keys (address TEXT NOT NULL, record BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, device_id))",
            "CREATE TABLE IF NOT EXISTS app_state_keys (key_id BLOB NOT NULL, key_data BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (key_id, device_id))",
            "CREATE TABLE IF NOT EXISTS app_state_versions (name TEXT NOT NULL, state_data BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (name, device_id))",
            "CREATE TABLE IF NOT EXISTS app_state_mutation_macs (name TEXT NOT NULL, version INTEGER NOT NULL, index_mac BLOB NOT NULL, value_mac BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (name, index_mac, device_id))",
            "CREATE TABLE IF NOT EXISTS lid_pn_mapping (lid TEXT NOT NULL, phone_number TEXT NOT NULL, created_at INTEGER NOT NULL, learning_source TEXT NOT NULL, updated_at INTEGER NOT NULL, device_id INTEGER NOT NULL, UNIQUE (lid, device_id))",
            "CREATE TABLE IF NOT EXISTS device_registry (user_id TEXT NOT NULL, devices_json TEXT NOT NULL, timestamp INTEGER NOT NULL, phash TEXT, raw_id INTEGER, device_id INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE (user_id, device_id))",
            "CREATE TABLE IF NOT EXISTS sender_key_devices (group_jid TEXT NOT NULL, device_jid TEXT NOT NULL, has_key INTEGER NOT NULL, device_id INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE (group_jid, device_jid, device_id))",
            "CREATE TABLE IF NOT EXISTS sent_messages (chat_jid TEXT NOT NULL, message_id TEXT NOT NULL, payload BLOB NOT NULL, device_id INTEGER NOT NULL, created_at INTEGER NOT NULL, UNIQUE (chat_jid, message_id, device_id))",
            "CREATE TABLE IF NOT EXISTS base_keys (address TEXT NOT NULL, message_id TEXT NOT NULL, base_key BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, message_id, device_id))",
            "CREATE TABLE IF NOT EXISTS tc_tokens (jid TEXT NOT NULL, token BLOB NOT NULL, token_timestamp INTEGER NOT NULL, sender_timestamp INTEGER, device_id INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE (jid, device_id))",
            "CREATE TABLE IF NOT EXISTS conversations (jid TEXT NOT NULL, name TEXT, is_group INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL, UNIQUE (jid))",
            "CREATE TABLE IF NOT EXISTS msg_secrets (chat TEXT NOT NULL, sender TEXT NOT NULL, msg_id TEXT NOT NULL, secret BLOB NOT NULL, expires_at INTEGER NOT NULL DEFAULT 0, message_ts INTEGER NOT NULL DEFAULT 0, device_id INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE (chat, sender, msg_id, device_id))",
        ];
        for stmt in stmts {
            exec(db, stmt, vec![])?;
        }
        Ok(())
    }

    /// Upsert conversation JIDs from HistorySync. Called from the adapter's
    /// Event::HistorySync handler. Uses DELETE+INSERT in a transaction.
    pub async fn upsert_conversations(
        &self,
        entries: &[(String, Option<String>, bool)],
    ) -> anyhow::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let mut tx = self.db.lock().await.begin()?;
        for (jid, name, is_group) in entries {
            exec_tx(
                &mut tx,
                "DELETE FROM conversations WHERE jid = $1",
                vec![jid.clone().into()],
            )?;
            exec_tx(
                &mut tx,
                "INSERT INTO conversations (jid, name, is_group, updated_at) VALUES ($1, $2, $3, $4)",
                vec![
                    jid.clone().into(),
                    name.clone().unwrap_or_default().into(),
                    (*is_group as i64).into(),
                    now.into(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// List all conversation JIDs. Returns (jid, name, is_group).
    pub async fn list_conversations(&self) -> anyhow::Result<Vec<(String, Option<String>, bool)>> {
        let rows = query(
            &*self.db.lock().await,
            "SELECT jid, name, is_group FROM conversations",
            vec![],
        )?;
        let mut result = Vec::new();
        for row_result in rows {
            let row = row_result?;
            let jid: String = row.get(0)?;
            let name: String = row.get(1)?;
            let is_group: i64 = row.get(2)?;
            result.push((
                jid,
                if name.is_empty() { None } else { Some(name) },
                is_group != 0,
            ));
        }
        Ok(result)
    }
}

// ── SignalStore ────────────────────────────────────────────────────

#[async_trait]
impl SignalStore for StoolapStore {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> wacore::store::error::Result<()> {
        // R13-M1 fix: see `put_session` for the rationale. A lost
        // identity row means we re-handshake with the peer (re-X3DH
        // + new ratchet), which is observable as a one-message
        // decrypt failure.
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM identities WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec_tx(
            &mut tx,
            "INSERT INTO identities (address, \"key\", device_id) VALUES ($1, $2, $3)",
            vec![
                address.to_string().into(),
                stoolap::core::Value::blob(key.to_vec()),
                (self.device_id as i64).into(),
            ],
        )?;
        tx.commit().map_err(to_store_err)
    }

    async fn load_identity(&self, address: &str) -> wacore::store::error::Result<Option<[u8; 32]>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT \"key\" FROM identities WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let key: Vec<u8> = row.get(0).map_err(to_store_err)?;
                if key.len() != 32 {
                    return Err(wacore::store::error::StoreError::Validation(
                        "invalid key length".into(),
                    ));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&key);
                Ok(Some(arr))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn delete_identity(&self, address: &str) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM identities WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn get_session(&self, address: &str) -> wacore::store::error::Result<Option<Bytes>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT record FROM sessions WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let record: Vec<u8> = row.get(0).map_err(to_store_err)?;
                Ok(Some(Bytes::from(record)))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn put_session(&self, address: &str, session: &[u8]) -> wacore::store::error::Result<()> {
        // R13-M1 fix: wrap the DELETE+INSERT in a transaction so a
        // panic / crash / power-loss between the two statements
        // can't leave the row gone with no replacement. (For
        // `put_session` a lost row means the next message to that
        // peer fails to decrypt and triggers a re-handshake — a
        // measurable degradation for high-traffic peers.)
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM sessions WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec_tx(
            &mut tx,
            "INSERT INTO sessions (address, record, device_id) VALUES ($1, $2, $3)",
            vec![
                address.to_string().into(),
                stoolap::core::Value::blob(session.to_vec()),
                (self.device_id as i64).into(),
            ],
        )?;
        tx.commit().map_err(to_store_err)
    }

    async fn delete_session(&self, address: &str) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM sessions WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn store_prekey(
        &self,
        id: u32,
        record: &[u8],
        uploaded: bool,
    ) -> wacore::store::error::Result<()> {
        // R13-M1 fix: see `put_session` for the rationale. A lost
        // pre-key row is recoverable (the server will tell us the
        // next-pre-key-id is out of range and we'll regenerate),
        // but in-window message decrypt still depends on
        // pre-key availability.
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )?;
        exec_tx(
            &mut tx,
            "INSERT INTO prekeys (id, \"key\", uploaded, device_id) VALUES ($1, $2, $3, $4)",
            vec![
                (id as i64).into(),
                stoolap::core::Value::blob(record.to_vec()),
                (uploaded as i64).into(),
                (self.device_id as i64).into(),
            ],
        )?;
        tx.commit().map_err(to_store_err)
    }

    async fn load_prekey(&self, id: u32) -> wacore::store::error::Result<Option<Bytes>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT \"key\" FROM prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let key: Vec<u8> = row.get(0).map_err(to_store_err)?;
                Ok(Some(Bytes::from(key)))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn get_max_prekey_id(&self) -> wacore::store::error::Result<u32> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT MAX(id) FROM prekeys WHERE device_id = $1",
            vec![(self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let max_id: Option<i64> = row.get(0).map_err(to_store_err)?;
                Ok(max_id.map(|v| v as u32).unwrap_or(0))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(0),
        }
    }

    async fn remove_prekey(&self, id: u32) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )
    }

    async fn store_signed_prekey(
        &self,
        id: u32,
        record: &[u8],
    ) -> wacore::store::error::Result<()> {
        // R13-M1 fix: see `put_session` for the rationale. The
        // signed pre-key is published to the server periodically;
        // losing it locally means the next handshake attempt will
        // use a different key and the server will reject it
        // (causing a forced re-handshake).
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM signed_prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )?;
        exec_tx(
            &mut tx,
            "INSERT INTO signed_prekeys (id, record, device_id) VALUES ($1, $2, $3)",
            vec![
                (id as i64).into(),
                stoolap::core::Value::blob(record.to_vec()),
                (self.device_id as i64).into(),
            ],
        )?;
        tx.commit().map_err(to_store_err)
    }

    async fn load_signed_prekey(&self, id: u32) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT record FROM signed_prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let record: Vec<u8> = row.get(0).map_err(to_store_err)?;
                Ok(Some(record))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn load_all_signed_prekeys(&self) -> wacore::store::error::Result<Vec<(u32, Vec<u8>)>> {
        let rows = query(
            &*self.db.lock().await,
            "SELECT id, record FROM signed_prekeys WHERE device_id = $1",
            vec![(self.device_id as i64).into()],
        )?;
        let mut result = Vec::new();
        for row_result in rows {
            let row = row_result.map_err(to_store_err)?;
            let id: i64 = row.get(0).map_err(to_store_err)?;
            let record: Vec<u8> = row.get(1).map_err(to_store_err)?;
            result.push((id as u32, record));
        }
        Ok(result)
    }

    async fn remove_signed_prekey(&self, id: u32) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM signed_prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )
    }

    async fn put_sender_key(
        &self,
        address: &str,
        record: &[u8],
    ) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM sender_keys WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec(
            &*self.db.lock().await,
            "INSERT INTO sender_keys (address, record, device_id) VALUES ($1, $2, $3)",
            vec![
                address.to_string().into(),
                stoolap::core::Value::blob(record.to_vec()),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn get_sender_key(&self, address: &str) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT record FROM sender_keys WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let record: Vec<u8> = row.get(0).map_err(to_store_err)?;
                Ok(Some(record))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn delete_sender_key(&self, address: &str) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM sender_keys WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn mark_prekeys_uploaded(&self, _ids: &[u32]) -> wacore::store::error::Result<()> {
        // TODO(octo-adapter-whatsapp): Stoolap-backed mark-as-uploaded.
        // The sweep that calls this runs after a successful first upload,
        // which currently never happens because pair_success is end-to-end
        // before prekey re-uploads. Tracked for Phase 7.
        Ok(())
    }
}

// ── AppSyncStore ───────────────────────────────────────────────────

#[async_trait]
impl AppSyncStore for StoolapStore {
    async fn get_sync_key(
        &self,
        key_id: &[u8],
    ) -> wacore::store::error::Result<Option<AppStateSyncKey>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT key_data FROM app_state_keys WHERE key_id = $1 AND device_id = $2",
            vec![
                stoolap::core::Value::blob(key_id.to_vec()),
                (self.device_id as i64).into(),
            ],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let data: Vec<u8> = row.get(0).map_err(to_store_err)?;
                let key: AppStateSyncKey = serde_json::from_slice(&data)
                    .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
                Ok(Some(key))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn set_sync_key(
        &self,
        key_id: &[u8],
        key: AppStateSyncKey,
    ) -> wacore::store::error::Result<()> {
        // R14-H2 fix: wrap the DELETE+INSERT in a single transaction.
        // Without this, a crash between the DELETE and the INSERT
        // loses the entire app-state sync key (HMAC key material
        // for snapshot MAC verification). Without the sync key, the
        // next sync cannot validate any snapshot — every subsequent
        // app-state sync operation depends on this key. This is
        // functionally equivalent to losing the bot's identity
        // until a full re-pair. R13-M1 missed this op (the review
        // table at `docs/reviews/2026-06-20-r13-mission-0850-review.md:118-127`
        // listed 8 single-pair DELETE+INSERT ops but not this one);
        // the R14 grep audit at lines 518-538 found it.
        let data = serde_json::to_vec(&key)
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM app_state_keys WHERE key_id = $1 AND device_id = $2",
            vec![
                stoolap::core::Value::blob(key_id.to_vec()),
                (self.device_id as i64).into(),
            ],
        )?;
        exec_tx(
            &mut tx,
            "INSERT INTO app_state_keys (key_id, key_data, device_id) VALUES ($1, $2, $3)",
            vec![
                stoolap::core::Value::blob(key_id.to_vec()),
                stoolap::core::Value::blob(data),
                (self.device_id as i64).into(),
            ],
        )?;
        tx.commit().map_err(to_store_err)
    }

    async fn get_version(&self, name: &str) -> wacore::store::error::Result<HashState> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT state_data FROM app_state_versions WHERE name = $1 AND device_id = $2",
            vec![name.to_string().into(), (self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let data: Vec<u8> = row.get(0).map_err(to_store_err)?;
                serde_json::from_slice(&data)
                    .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))
            }
            // R10 / R11 fix: missing version must return
            // `HashState::default()` (version=0), not an error. The
            // caller in `sync_collections_batched_inner`
            // (whatsapp-rust client.rs:2676) treats `state.version == 0`
            // as the signal to request a fresh snapshot; an error
            // would short-circuit the IQ handshake and log
            // "Failed critical app state sync: database operation
            // error". Matches `InMemoryStore::get_version`
            // (wacore/store/in_memory.rs:289) and the SQLite reference
            // store, both of which return the default for unknown
            // names.
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(HashState::default()),
        }
    }

    async fn set_version(&self, name: &str, state: HashState) -> wacore::store::error::Result<()> {
        // R14-M1 fix: wrap the DELETE+INSERT in a single transaction.
        // Without this, a crash between the DELETE and the INSERT
        // loses the entire `HashState` (per-collection `version` and
        // running `ltHash`). The next sync then starts from the
        // wrong version, causing wrong patches to be applied and the
        // ltHash to diverge. R13-M1 missed this op; the R14 grep
        // audit at lines 575-595 found it.
        let data = serde_json::to_vec(&state)
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM app_state_versions WHERE name = $1 AND device_id = $2",
            vec![name.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec_tx(
            &mut tx,
            "INSERT INTO app_state_versions (name, state_data, device_id) VALUES ($1, $2, $3)",
            vec![
                name.to_string().into(),
                stoolap::core::Value::blob(data),
                (self.device_id as i64).into(),
            ],
        )?;
        tx.commit().map_err(to_store_err)
    }

    async fn put_mutation_macs(
        &self,
        name: &str,
        version: u64,
        mutations: &[AppStateMutationMAC],
    ) -> wacore::store::error::Result<()> {
        // R12 fix (storage format): store index_mac and value_mac as
        // raw bytes, NOT serde_json-encoded. These fields are 32-byte
        // HMAC outputs (Signal Integrity value MACs). The hash state
        // in `WAPATCH_INTEGRITY.subtract_then_add_in_place` is a sum
        // of these 32-byte values; a single byte difference corrupts
        // the ltHash and produces "patch snapshot MAC mismatch" on
        // the next sync. JSON-wrapping the bytes (e.g. b"\"qrvM...\""
        // = 8 bytes for 3 bytes of input) was the bug — the value
        // MACs were being read back as their JSON representation
        // instead of the original bytes, so the hash update
        // subtracted the wrong previous values. The in-memory and
        // SQLite reference stores store these as raw bytes (the
        // SQLite store wraps them in bincode which is also a binary
        // encoding of the Vec<u8> field, i.e. raw bytes on the
        // wire). Must match this for the ltHash to be stable.
        //
        // R13 fix (idempotency): DELETE-then-INSERT to handle the
        // `UNIQUE (name, index_mac, device_id)` constraint. The
        // same index_mac can legitimately appear twice: a patch SET
        // can overwrite a snapshot SET, and a multi-mutation patch
        // can contain multiple entries with the same index_mac
        // (e.g. a SET followed by a REMOVE of the same key in the
        // same patch, where the REMOVE's value MAC lookup references
        // the SET we just inserted). Without the DELETE, the second
        // INSERT raises a unique-constraint violation, which
        // propagates as "database operation error" and aborts the
        // entire critical-sync flow.
        //
        // R15 fix (single-statement UPSERT + batch-atomic tx):
        // replaced the per-iteration DELETE+INSERT with a single
        // `INSERT ... ON DUPLICATE KEY UPDATE` statement. This is
        // possible because the two underlying Stoolap bugs that
        // blocked this approach in R14 are now fixed (commit
        // `1fc5bc2` on `feat/blockchain-sql`):
        //
        // 1. UPSERT on COMPOSITE unique indexes (R14 carve-out).
        //    Stoolap's `apply_on_duplicate_update` previously
        //    called `find_row_by_unique_index` with the composite
        //    column name `"name, index_mac, device_id"` as a single
        //    column, which didn't exist in the column map, causing
        //    `Error::UniqueConstraint { value: "unknown" }`. Fixed
        //    in Stoolap by refactoring `apply_on_duplicate_update`
        //    to take a pre-built WHERE expression from the unique
        //    columns + values (no PK dependency). The
        //    `tests/mission_0850_r14_regression_test.rs` test file
        //    pins this.
        //
        // 2. Transaction-local DELETE visibility for unique-index
        //    INSERTs. The in-memory backend's `check_unique_constraints`
        //    previously queried the committed-state index, which
        //    didn't see rows locally deleted by the current
        //    transaction. So wrapping the loop in a single
        //    transaction caused the DELETE+INSERT pattern to raise
        //    `UniqueConstraint` on the INSERT (the DELETE's effect
        //    was invisible to the constraint check). Fixed in
        //    Stoolap by filtering index entries against
        //    `txn_versions.get_local_version()` and skipping
        //    locally-deleted entries.
        //
        // With both fixes, we can:
        // - Replace DELETE-then-INSERT with a single UPSERT (one
        //   statement, no per-iteration tx overhead).
        // - Wrap the whole batch in a single transaction so a crash
        //   mid-batch doesn't leave mutations 1-4 in their new
        //   state and 5-10 in their old state (the R14-H1 carve-out
        //   is now closed). This matches the in-memory reference
        //   (`wacore/src/store/in_memory.rs:315` — wraps in
        //   `state.lock().await`) and the SQLite reference
        //   (`storages/sqlite-storage/src/sqlite_store.rs:1127-1142`
        //   — `with_retry` + `on_conflict do update`).
        //
        // The single-statement UPSERT is now both per-iteration
        // atomic (one statement) AND batch-atomic (one
        // transaction), restoring parity with the in-memory and
        // SQLite reference implementations.
        if mutations.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        for m in mutations {
            exec_tx(
                &mut tx,
                "INSERT INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id)
                 VALUES ($1, $2, $3, $4, $5)
                 ON DUPLICATE KEY UPDATE
                     version = $2,
                     value_mac = $4",
                vec![
                    name.to_string().into(),
                    (version as i64).into(),
                    stoolap::core::Value::blob(m.index_mac.clone()),
                    stoolap::core::Value::blob(m.value_mac.clone()),
                    (self.device_id as i64).into(),
                ],
            )?;
        }
        tx.commit().map_err(to_store_err)
    }

    async fn get_mutation_mac(
        &self,
        name: &str,
        index_mac: &[u8],
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        // R12 fix: lookup and return the value_mac as raw bytes (see
        // put_mutation_macs above for the rationale).
        let mut rows = query(&*self.db.lock().await, "SELECT value_mac FROM app_state_mutation_macs WHERE name = $1 AND index_mac = $2 AND device_id = $3",
            vec![name.to_string().into(), stoolap::core::Value::blob(index_mac.to_vec()), (self.device_id as i64).into()])?;
        match rows.next() {
            Some(Ok(row)) => {
                let mac: Vec<u8> = row.get(0).map_err(to_store_err)?;
                Ok(Some(mac))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn delete_mutation_macs(
        &self,
        name: &str,
        index_macs: &[Vec<u8>],
    ) -> wacore::store::error::Result<()> {
        // R12 fix: lookup by raw bytes to match put_mutation_macs.
        //
        // R14-M3 fix: wrap the whole loop in a single transaction.
        // Without this, a crash mid-batch (e.g., on iteration 5 of
        // 10) leaves some index_macs still present with their old
        // `value_mac`. The next patch's prev-value lookup finds the
        // OLD value_mac (the one we still have), succeeds, and then
        // the diff is applied with the wrong prev_value — corrupting
        // the ltHash arithmetic in
        // `WAPATCH_INTEGRITY.subtract_then_add_in_place` and producing
        // "patch snapshot MAC mismatch" on the next sync. Pure
        // DELETE (no INSERT) means the rows are eventually cleaned
        // up by the next call, but the current patch flow's
        // correctness depends on this batch being atomic. The
        // in-memory reference wraps the whole loop in a single
        // `state.lock().await` (`wacore/src/store/in_memory.rs:334-339`)
        // and the SQLite reference uses `with_retry` (a single
        // transaction) — we match both. R13-M1 missed this op; the
        // R14 grep audit at lines 691-701 found it.
        if index_macs.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        for idx in index_macs {
            exec_tx(&mut tx, "DELETE FROM app_state_mutation_macs WHERE name = $1 AND index_mac = $2 AND device_id = $3",
                vec![name.to_string().into(), stoolap::core::Value::blob(idx.clone()), (self.device_id as i64).into()])?;
        }
        tx.commit().map_err(to_store_err)
    }

    async fn get_latest_sync_key_id(&self) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT key_id FROM app_state_keys WHERE device_id = $1 ORDER BY key_id DESC LIMIT 1",
            vec![(self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let id: Vec<u8> = row.get(0).map_err(to_store_err)?;
                Ok(Some(id))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn clear_mutation_macs(&self, _name: &str) -> wacore::store::error::Result<()> {
        // TODO(octo-adapter-whatsapp): Stoolap-backed MAC clear. The ltHash
        // rebuild is triggered on snapshot re-sync, which the upstream default
        // sync sequence handles; store-level impl deferred.
        Ok(())
    }
}

// ── ProtocolStore ──────────────────────────────────────────────────

#[async_trait]
impl ProtocolStore for StoolapStore {
    async fn get_sender_key_devices(
        &self,
        group_jid: &str,
    ) -> wacore::store::error::Result<Vec<(String, bool)>> {
        let rows = query(&*self.db.lock().await, "SELECT device_jid, has_key FROM sender_key_devices WHERE group_jid = $1 AND device_id = $2",
            vec![group_jid.to_string().into(), (self.device_id as i64).into()])?;
        let mut result = Vec::new();
        for row_result in rows {
            let row = row_result.map_err(to_store_err)?;
            let jid: String = row.get(0).map_err(to_store_err)?;
            let has: i64 = row.get(1).map_err(to_store_err)?;
            result.push((jid, has != 0));
        }
        Ok(result)
    }

    async fn set_sender_key_status(
        &self,
        group_jid: &str,
        entries: &[(&str, bool)],
    ) -> wacore::store::error::Result<()> {
        // R13-M1 fix: wrap the whole batch in a single transaction
        // (not one transaction per entry) so the (group_jid,
        // device_jid) updates are atomic AND a crash mid-batch
        // doesn't leave the table half-updated with the prior
        // partial state visible to readers.
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        for (jid, has_key) in entries {
            exec_tx(&mut tx, "DELETE FROM sender_key_devices WHERE group_jid = $1 AND device_jid = $2 AND device_id = $3",
                vec![group_jid.to_string().into(), jid.to_string().into(), (self.device_id as i64).into()])?;
            exec_tx(&mut tx, "INSERT INTO sender_key_devices (group_jid, device_jid, has_key, device_id, updated_at) VALUES ($1, $2, $3, $4, $5)",
                vec![group_jid.to_string().into(), jid.to_string().into(), (if *has_key { 1i64 } else { 0i64 }).into(), (self.device_id as i64).into(), now.into()])?;
        }
        tx.commit().map_err(to_store_err)
    }

    async fn clear_sender_key_devices(&self, group_jid: &str) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM sender_key_devices WHERE group_jid = $1 AND device_id = $2",
            vec![group_jid.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn delete_sender_key_device_rows(
        &self,
        device_jids: &[&str],
    ) -> wacore::store::error::Result<()> {
        // R14-M4 fix: wrap the whole loop in a single transaction.
        // Without this, a crash mid-batch leaves some sender-key
        // device rows still present. These rows are used by the
        // protocol to determine whether a device needs fresh SKDM
        // (Sender Key Distribution Message) on next message send;
        // extra rows cause unnecessary SKDM sends, which is a
        // minor bandwidth/protocol overhead, not a correctness
        // issue — but consistency is cheap. R13-M1 missed this op;
        // the R14 grep audit at lines 791-803 found it.
        if device_jids.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        for jid in device_jids {
            exec_tx(
                &mut tx,
                "DELETE FROM sender_key_devices WHERE device_jid = $1 AND device_id = $2",
                vec![jid.to_string().into(), (self.device_id as i64).into()],
            )?;
        }
        tx.commit().map_err(to_store_err)
    }

    async fn clear_all_sender_key_devices(&self) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM sender_key_devices WHERE device_id = $1",
            vec![(self.device_id as i64).into()],
        )
    }

    async fn get_lid_mapping(
        &self,
        lid: &str,
    ) -> wacore::store::error::Result<Option<LidPnMappingEntry>> {
        let mut rows = query(&*self.db.lock().await, "SELECT lid, phone_number, created_at, learning_source, updated_at FROM lid_pn_mapping WHERE lid = $1 AND device_id = $2",
            vec![lid.to_string().into(), (self.device_id as i64).into()])?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(LidPnMappingEntry {
                lid: row.get(0).map_err(to_store_err)?,
                phone_number: row.get(1).map_err(to_store_err)?,
                created_at: row.get(2).map_err(to_store_err)?,
                learning_source: row.get(3).map_err(to_store_err)?,
                updated_at: row.get(4).map_err(to_store_err)?,
            })),
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn get_pn_mapping(
        &self,
        phone: &str,
    ) -> wacore::store::error::Result<Option<LidPnMappingEntry>> {
        let mut rows = query(&*self.db.lock().await, "SELECT lid, phone_number, created_at, learning_source, updated_at FROM lid_pn_mapping WHERE phone_number = $1 AND device_id = $2 ORDER BY updated_at DESC LIMIT 1",
            vec![phone.to_string().into(), (self.device_id as i64).into()])?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(LidPnMappingEntry {
                lid: row.get(0).map_err(to_store_err)?,
                phone_number: row.get(1).map_err(to_store_err)?,
                created_at: row.get(2).map_err(to_store_err)?,
                learning_source: row.get(3).map_err(to_store_err)?,
                updated_at: row.get(4).map_err(to_store_err)?,
            })),
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn put_lid_mapping(&self, entry: &LidPnMappingEntry) -> wacore::store::error::Result<()> {
        // R14-M2 fix: wrap the DELETE+INSERT in a single transaction.
        // Without this, a crash between the DELETE and the INSERT
        // loses the LID↔PN mapping. The bached `put_lid_mappings`
        // trait default at `wacore/src/store/traits.rs:255-260` LOOPS
        // over `put_lid_mapping` calls; if the batch is called with
        // N entries and a crash happens between N1 and N2, the next
        // batch will see N1 as missing. The SQLite ref impl uses a
        // single transaction (matching what we do here). R13-M1
        // missed this op; the R14 grep audit at lines 829-837 found it.
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM lid_pn_mapping WHERE lid = $1 AND device_id = $2",
            vec![entry.lid.clone().into(), (self.device_id as i64).into()],
        )?;
        exec_tx(&mut tx, "INSERT INTO lid_pn_mapping (lid, phone_number, created_at, learning_source, updated_at, device_id) VALUES ($1, $2, $3, $4, $5, $6)",
            vec![entry.lid.clone().into(), entry.phone_number.clone().into(), entry.created_at.into(), entry.learning_source.clone().into(), entry.updated_at.into(), (self.device_id as i64).into()])?;
        tx.commit().map_err(to_store_err)
    }

    async fn get_all_lid_mappings(&self) -> wacore::store::error::Result<Vec<LidPnMappingEntry>> {
        let rows = query(&*self.db.lock().await, "SELECT lid, phone_number, created_at, learning_source, updated_at FROM lid_pn_mapping WHERE device_id = $1",
            vec![(self.device_id as i64).into()])?;
        let mut result = Vec::new();
        for row_result in rows {
            let row = row_result.map_err(to_store_err)?;
            result.push(LidPnMappingEntry {
                lid: row.get(0).map_err(to_store_err)?,
                phone_number: row.get(1).map_err(to_store_err)?,
                created_at: row.get(2).map_err(to_store_err)?,
                learning_source: row.get(3).map_err(to_store_err)?,
                updated_at: row.get(4).map_err(to_store_err)?,
            });
        }
        Ok(result)
    }

    async fn save_base_key(
        &self,
        address: &str,
        message_id: &str,
        base_key: &[u8],
    ) -> wacore::store::error::Result<()> {
        // R13-M1 fix: see `put_session` for the rationale. A lost
        // base-key row breaks the sender-key ratchet for the
        // affected peer, requiring a full re-sender-key-distribution
        // round.
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM base_keys WHERE address = $1 AND message_id = $2 AND device_id = $3",
            vec![
                address.to_string().into(),
                message_id.to_string().into(),
                (self.device_id as i64).into(),
            ],
        )?;
        exec_tx(&mut tx, "INSERT INTO base_keys (address, message_id, base_key, device_id, created_at) VALUES ($1, $2, $3, $4, $5)",
            vec![address.to_string().into(), message_id.to_string().into(), stoolap::core::Value::blob(base_key.to_vec()), (self.device_id as i64).into(), now.into()])?;
        tx.commit().map_err(to_store_err)
    }

    async fn has_same_base_key(
        &self,
        address: &str,
        message_id: &str,
        current_base_key: &[u8],
    ) -> wacore::store::error::Result<bool> {
        let mut rows = query(&*self.db.lock().await, "SELECT base_key FROM base_keys WHERE address = $1 AND message_id = $2 AND device_id = $3",
            vec![address.to_string().into(), message_id.to_string().into(), (self.device_id as i64).into()])?;
        match rows.next() {
            Some(Ok(row)) => {
                let saved: Vec<u8> = row.get(0).map_err(to_store_err)?;
                Ok(saved == current_base_key)
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(false),
        }
    }

    async fn delete_base_key(
        &self,
        address: &str,
        message_id: &str,
    ) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM base_keys WHERE address = $1 AND message_id = $2 AND device_id = $3",
            vec![
                address.to_string().into(),
                message_id.to_string().into(),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn update_device_list(
        &self,
        record: DeviceListRecord,
    ) -> wacore::store::error::Result<()> {
        // R13-M1 fix: see `put_session` for the rationale. A lost
        // device-list row means we'd fall back to the
        // "no-device-list" optimization on the next send, which can
        // route messages through the wrong device.
        let devices_json = serde_json::to_string(&record.devices)
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM device_registry WHERE user_id = $1 AND device_id = $2",
            vec![record.user.clone().into(), (self.device_id as i64).into()],
        )?;
        exec_tx(&mut tx, "INSERT INTO device_registry (user_id, devices_json, timestamp, phash, raw_id, device_id, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            vec![record.user.into(), devices_json.into(), record.timestamp.into(), record.phash.unwrap_or_default().into(), record.raw_id.map(|r| (r as i64).into()).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null)), (self.device_id as i64).into(), now.into()])?;
        tx.commit().map_err(to_store_err)
    }

    async fn get_devices(
        &self,
        user: &str,
    ) -> wacore::store::error::Result<Option<DeviceListRecord>> {
        let mut rows = query(&*self.db.lock().await, "SELECT user_id, devices_json, timestamp, phash, raw_id FROM device_registry WHERE user_id = $1 AND device_id = $2",
            vec![user.to_string().into(), (self.device_id as i64).into()])?;
        match rows.next() {
            Some(Ok(row)) => {
                let user_id: String = row.get(0).map_err(to_store_err)?;
                let devices_json: String = row.get(1).map_err(to_store_err)?;
                let devices: Vec<DeviceInfo> = serde_json::from_str(&devices_json)
                    .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
                let timestamp: i64 = row.get(2).map_err(to_store_err)?;
                let phash: String = row.get(3).map_err(to_store_err)?;
                let raw_id: Option<i64> = row.get(4).map_err(to_store_err)?;
                Ok(Some(DeviceListRecord {
                    user: user_id,
                    devices,
                    timestamp,
                    phash: if phash.is_empty() { None } else { Some(phash) },
                    raw_id: raw_id.map(|r| r as u32),
                }))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn delete_devices(&self, user: &str) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM device_registry WHERE user_id = $1 AND device_id = $2",
            vec![user.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn get_tc_token(&self, jid: &str) -> wacore::store::error::Result<Option<TcTokenEntry>> {
        let mut rows = query(&*self.db.lock().await, "SELECT token, token_timestamp, sender_timestamp FROM tc_tokens WHERE jid = $1 AND device_id = $2",
            vec![jid.to_string().into(), (self.device_id as i64).into()])?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(TcTokenEntry {
                token: row.get(0).map_err(to_store_err)?,
                token_timestamp: row.get(1).map_err(to_store_err)?,
                sender_timestamp: row.get(2).map_err(to_store_err)?,
            })),
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn put_tc_token(
        &self,
        jid: &str,
        entry: &TcTokenEntry,
    ) -> wacore::store::error::Result<()> {
        // R13-M1 fix: see `put_session` for the rationale. The TC
        // token is the long-lived "trust" credential for a peer;
        // losing it forces a full re-handshake on the next message
        // to that peer.
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM tc_tokens WHERE jid = $1 AND device_id = $2",
            vec![jid.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec_tx(&mut tx, "INSERT INTO tc_tokens (jid, token, token_timestamp, sender_timestamp, device_id, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
            vec![jid.to_string().into(), stoolap::core::Value::blob(entry.token.clone()), entry.token_timestamp.into(), entry.sender_timestamp.unwrap_or(0).into(), (self.device_id as i64).into(), now.into()])?;
        tx.commit().map_err(to_store_err)
    }

    async fn delete_tc_token(&self, jid: &str) -> wacore::store::error::Result<()> {
        exec(
            &*self.db.lock().await,
            "DELETE FROM tc_tokens WHERE jid = $1 AND device_id = $2",
            vec![jid.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn get_all_tc_token_jids(&self) -> wacore::store::error::Result<Vec<String>> {
        let rows = query(
            &*self.db.lock().await,
            "SELECT jid FROM tc_tokens WHERE device_id = $1",
            vec![(self.device_id as i64).into()],
        )?;
        let mut result = Vec::new();
        for row_result in rows {
            result.push(
                row_result
                    .map_err(to_store_err)?
                    .get(0)
                    .map_err(to_store_err)?,
            );
        }
        Ok(result)
    }

    async fn delete_expired_tc_tokens(
        &self,
        token_cutoff: i64,
        sender_cutoff: i64,
    ) -> wacore::store::error::Result<u32> {
        // R14-M5 fix: replace the SELECT COUNT + DELETE pattern with
        // a single DELETE that returns the rows-affected count from
        // `Database::execute` (`stoolap/src/api/database.rs:483`,
        // `pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<i64>`).
        // The previous SELECT-COUNT + DELETE was "not perfectly atomic
        // but acceptable for cleanup" (per the old comment) — but
        // there was no reason not to make it atomic: a single
        // statement IS atomic by definition, and the count it
        // returns is exact (the engine computes both as part of one
        // plan). The old two-statement pattern had a race window
        // where a concurrent insert between the SELECT and the
        // DELETE would make the returned count diverge from the
        // actual number of rows deleted. R13-M1 didn't audit this
        // pattern (only DELETE+INSERT); the R14 grep audit at
        // lines 1081-1101 found it.
        // Post-buffa migration (wacore 6e0f241): upstream trait added
        // a second cutoff to guard sender buckets separately from
        // received-token state (see wacore/src/store/traits.rs).
        // sender_cutoff = 0 means "no sender state preserved".
        self.db
            .lock()
            .await
            .execute(
                "DELETE FROM tc_tokens WHERE \
                 token_timestamp < $1 AND device_id = $2 AND \
                 (sender_timestamp IS NULL OR sender_timestamp < $3)",
                vec![
                    token_cutoff.into(),
                    (self.device_id as i64).into(),
                    sender_cutoff.into(),
                ],
            )
            .map(|n| n as u32)
            .map_err(to_store_err)
    }

    async fn store_sent_message(
        &self,
        chat_jid: &str,
        message_id: &str,
        payload: &[u8],
    ) -> wacore::store::error::Result<()> {
        // R13-M1 fix: see `put_session` for the rationale. The
        // sent-messages table is used for outgoing-message dedup
        // (a re-send of the same `(chat_jid, message_id)` is
        // treated as a duplicate by the server); losing the row
        // could cause a re-send to be processed as a new message.
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        exec_tx(
            &mut tx,
            "DELETE FROM sent_messages WHERE chat_jid = $1 AND message_id = $2 AND device_id = $3",
            vec![
                chat_jid.to_string().into(),
                message_id.to_string().into(),
                (self.device_id as i64).into(),
            ],
        )?;
        exec_tx(&mut tx, "INSERT INTO sent_messages (chat_jid, message_id, payload, device_id, created_at) VALUES ($1, $2, $3, $4, $5)",
            vec![chat_jid.to_string().into(), message_id.to_string().into(), stoolap::core::Value::blob(payload.to_vec()), (self.device_id as i64).into(), now.into()])?;
        tx.commit().map_err(to_store_err)
    }

    async fn take_sent_message(
        &self,
        chat_jid: &str,
        message_id: &str,
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        // R13-M1 + R13-L1 fix: wrap the SELECT+DELETE in a single
        // transaction so the consume operation is atomic. Without
        // the transaction, a concurrent `take_sent_message` for the
        // same `(chat_jid, message_id)` could see the row twice
        // (R13-L1), AND a panic / crash between the SELECT and the
        // DELETE could leave the row in place to be consumed again
        // (R13-M1 consequence). On a single-threaded Stoolap
        // backend the L1 race is impossible today, but the
        // transaction makes the invariant hold on any future
        // multi-threaded backend (Postgres, MySQL, etc.) and
        // additionally makes the consume atomic with respect to
        // crashes.
        let params = vec![
            chat_jid.to_string().into(),
            message_id.to_string().into(),
            (self.device_id as i64).into(),
        ];
        let mut tx = self.db.lock().await.begin().map_err(to_store_err)?;
        // SELECT first to get the payload
        let mut rows = query_tx(&mut tx, "SELECT payload FROM sent_messages WHERE chat_jid = $1 AND message_id = $2 AND device_id = $3", params.clone())?;
        let payload = match rows.next() {
            Some(Ok(row)) => Some(row.get::<Vec<u8>>(0).map_err(to_store_err)?),
            Some(Err(e)) => return Err(to_store_err(e)),
            None => None,
        };
        // Delete if found (consume) — same transaction so the
        // SELECT and DELETE are atomic.
        if payload.is_some() {
            exec_tx(&mut tx, "DELETE FROM sent_messages WHERE chat_jid = $1 AND message_id = $2 AND device_id = $3", params)?;
        }
        tx.commit().map_err(to_store_err)?;
        Ok(payload)
    }

    async fn delete_expired_sent_messages(
        &self,
        cutoff_timestamp: i64,
    ) -> wacore::store::error::Result<u32> {
        // R14-M5 fix: same as `delete_expired_tc_tokens` — replace
        // SELECT COUNT + DELETE with a single DELETE that returns
        // the rows-affected count from `Database::execute`. The
        // two-statement pattern had a race window where a concurrent
        // insert would make the returned count diverge from the
        // actual number of rows deleted. Single statement is atomic
        // by definition. R13-M1 didn't audit this pattern; the R14
        // grep audit at lines 1175-1193 found it.
        self.db
            .lock()
            .await
            .execute(
                "DELETE FROM sent_messages WHERE created_at < $1 AND device_id = $2",
                vec![cutoff_timestamp.into(), (self.device_id as i64).into()],
            )
            .map(|n| n as u32)
            .map_err(to_store_err)
    }
}

// ── MsgSecretStore ─────────────────────────────────────────────────

#[async_trait]
impl MsgSecretStore for StoolapStore {
    async fn put_msg_secrets(
        &self,
        entries: Vec<wacore::store::traits::MsgSecretEntry>,
    ) -> wacore::store::error::Result<usize> {
        // Per wacore merge semantics:
        //   expires_at: 0 ("never") wins; otherwise max of the two
        //   message_ts: max (0 never clobbers a known parent ts)
        // Implemented as INSERT ... ON CONFLICT DO UPDATE; we read the
        // existing row when present, merge in Rust, and UPSERT the merged
        // value. We also bump `updated_at` so the keepalive cleanup sweep
        // can age rows correctly.
        let mut count = 0usize;
        let now = chrono::Utc::now().timestamp();
        for entry in entries {
            let device_id = self.device_id as i64;
            let existing: Option<(i64, i64)> = {
                let conn = self.db.lock().await;
                let mut rows = conn
                    .query(
                        "SELECT expires_at, message_ts FROM msg_secrets \
                         WHERE chat = $1 AND sender = $2 AND msg_id = $3 \
                         AND device_id = $4",
                        vec![
                            entry.chat.clone().into(),
                            entry.sender.clone().into(),
                            entry.msg_id.clone().into(),
                            device_id.into(),
                        ],
                    )
                    .map_err(to_store_err)?;
                match rows.next() {
                    Some(Ok(row)) => Some((
                        row.get::<i64>(0).map_err(to_store_err)?,
                        row.get::<i64>(1).map_err(to_store_err)?,
                    )),
                    _ => None,
                }
            };
            let merged_expires = match existing {
                Some((e, _)) => wacore::store::traits::merge_msg_secret_expiry(
                    e,
                    entry.expires_at,
                ),
                None => entry.expires_at,
            };
            let merged_msg_ts = match existing {
                Some((_, t)) => wacore::store::traits::merge_msg_secret_message_ts(
                    t,
                    entry.message_ts,
                ),
                None => entry.message_ts,
            };
            exec(
                &*self.db.lock().await,
                "INSERT INTO msg_secrets \
                 (chat, sender, msg_id, secret, expires_at, message_ts, device_id, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                 ON CONFLICT (chat, sender, msg_id, device_id) DO UPDATE SET \
                 secret = excluded.secret, \
                 expires_at = excluded.expires_at, \
                 message_ts = excluded.message_ts, \
                 updated_at = excluded.updated_at",
                vec![
                    entry.chat.into(),
                    entry.sender.into(),
                    entry.msg_id.into(),
                    stoolap::core::Value::blob(entry.secret),
                    merged_expires.into(),
                    merged_msg_ts.into(),
                    device_id.into(),
                    now.into(),
                ],
            )?;
            count += 1;
        }
        Ok(count)
    }

    async fn get_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let conn = self.db.lock().await;
        let mut rows = conn
            .query(
                "SELECT secret FROM msg_secrets \
                 WHERE chat = $1 AND sender = $2 AND msg_id = $3 AND device_id = $4",
                vec![
                    chat.to_string().into(),
                    sender.to_string().into(),
                    msg_id.to_string().into(),
                    (self.device_id as i64).into(),
                ],
            )
            .map_err(to_store_err)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row.get::<Vec<u8>>(0).map_err(to_store_err)?)),
            _ => Ok(None),
        }
    }

    async fn delete_expired_msg_secrets(
        &self,
        cutoff_timestamp: i64,
    ) -> wacore::store::error::Result<u32> {
        // Rows with expires_at = 0 ("never") are kept.
        self.db
            .lock()
            .await
            .execute(
                "DELETE FROM msg_secrets \
                 WHERE expires_at > 0 AND expires_at <= $1 AND device_id = $2",
                vec![cutoff_timestamp.into(), (self.device_id as i64).into()],
            )
            .map(|n| n as u32)
            .map_err(to_store_err)
    }
}

// ── DeviceStore ────────────────────────────────────────────────────

#[async_trait]
impl DeviceStore for StoolapStore {
    async fn save(&self, device: &CoreDevice) -> wacore::store::error::Result<()> {
        let noise_key = {
            let mut b = Vec::new();
            b.extend_from_slice(device.noise_key.private_key.serialize().as_slice());
            b.extend_from_slice(device.noise_key.public_key.public_key_bytes());
            b
        };
        let identity_key = {
            let mut b = Vec::new();
            b.extend_from_slice(device.identity_key.private_key.serialize().as_slice());
            b.extend_from_slice(device.identity_key.public_key.public_key_bytes());
            b
        };
        let signed_pre_key = {
            let mut b = Vec::new();
            b.extend_from_slice(device.signed_pre_key.private_key.serialize().as_slice());
            b.extend_from_slice(device.signed_pre_key.public_key.public_key_bytes());
            b
        };
        let account = device
            .account
            .as_ref()
            .map(|a| BuffaMessage::encode_to_vec(&**a));
        let cert_chain = device
            .server_cert_chain
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;

        // R13-M1 fix: wrap the DELETE+INSERT in a transaction. This
        // is the most consequential call in the file: a crash
        // between the DELETE and the INSERT means the entire
        // `CoreDevice` (noise keys, identity, signed pre-key,
        // registration ID) is lost and the bot has to re-pair from
        // scratch — a complete session-loss event, not a temporary
        // outage. The transaction makes the operation atomic
        // w.r.t. crashes and panics.
        let mut tx = match self.db.lock().await.begin() {
            Ok(tx) => tx,
            Err(e) => {
                tracing::error!(error = %e, "StoolapStore::save begin() failed");
                return Err(to_store_err(e));
            }
        };
        if let Err(e) = exec_tx(
            &mut tx,
            "DELETE FROM device WHERE id = $1",
            vec![(self.device_id as i64).into()],
        ) {
            tracing::error!(error = %e, "StoolapStore::save DELETE failed");
            return Err(e);
        }
        if let Err(e) = exec_tx(&mut tx, "INSERT INTO device (id, lid, pn, registration_id, noise_key, identity_key, signed_pre_key, signed_pre_key_id, signed_pre_key_signature, adv_secret_key, account, push_name, app_version_primary, app_version_secondary, app_version_tertiary, app_version_last_fetched_ms, edge_routing_info, props_hash, next_pre_key_id, server_has_prekeys, nct_salt, server_cert_chain, login_counter) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)",
            vec![
                (self.device_id as i64).into(),
                device.lid.as_ref().map(|j| j.to_string()).unwrap_or_default().into(),
                device.pn.as_ref().map(|j| j.to_string()).unwrap_or_default().into(),
                (device.registration_id as i64).into(),
                stoolap::core::Value::blob(noise_key), stoolap::core::Value::blob(identity_key), stoolap::core::Value::blob(signed_pre_key),
                (device.signed_pre_key_id as i64).into(),
                stoolap::core::Value::blob(device.signed_pre_key_signature.to_vec()),
                stoolap::core::Value::blob(device.adv_secret_key.to_vec()),
                account.map(stoolap::core::Value::blob).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null)),
                device.push_name.clone().into(),
                (device.app_version_primary as i64).into(), (device.app_version_secondary as i64).into(), (device.app_version_tertiary as i64).into(), device.app_version_last_fetched_ms.into(),
                device.edge_routing_info.clone().map(stoolap::core::Value::blob).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null)),
                device.props_hash.clone().unwrap_or_default().into(),
                (device.next_pre_key_id as i64).into(), (device.server_has_prekeys as i64).into(),
                device.nct_salt.clone().map(stoolap::core::Value::blob).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null)),
                cert_chain.map(stoolap::core::Value::blob).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null)),
                (device.login_counter as i64).into(),
            ]) {
            // Log the FULL error chain, not just the wrapper.
            let mut detail = format!("{e}");
            let mut source = std::error::Error::source(&e as &dyn std::error::Error);
            while let Some(s) = source {
                detail.push_str(&format!(" -> {s}"));
                source = s.source();
            }
            tracing::error!(error = %detail, "StoolapStore::save INSERT failed with detail");
            return Err(e);
        }
        match tx.commit() {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::error!(error = %e, "StoolapStore::save commit failed");
                Err(to_store_err(e))
            }
        }
    }

    async fn load(&self) -> wacore::store::error::Result<Option<CoreDevice>> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT * FROM device WHERE id = $1",
            vec![(self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let noise_bytes: Vec<u8> = row.get(4).map_err(to_store_err)?;
                let identity_bytes: Vec<u8> = row.get(5).map_err(to_store_err)?;
                let spk_bytes: Vec<u8> = row.get(6).map_err(to_store_err)?;
                if noise_bytes.len() != 64 || identity_bytes.len() != 64 || spk_bytes.len() != 64 {
                    return Err(wacore::store::error::StoreError::Validation(
                        "invalid key length".into(),
                    ));
                }
                use wacore::libsignal::protocol::{KeyPair, PrivateKey, PublicKey};
                let kp = |bytes: &[u8]| -> Result<KeyPair, wacore::store::error::StoreError> {
                    Ok(KeyPair::new(
                        PublicKey::from_djb_public_key_bytes(&bytes[32..64]).map_err(|e| {
                            wacore::store::error::StoreError::Validation(e.to_string())
                        })?,
                        PrivateKey::deserialize(&bytes[0..32]).map_err(|e| {
                            wacore::store::error::StoreError::Validation(e.to_string())
                        })?,
                    ))
                };
                let noise_key = kp(&noise_bytes)?;
                let identity_key = kp(&identity_bytes)?;
                let signed_pre_key = kp(&spk_bytes)?;
                let lid_str: String = row.get(1).map_err(to_store_err)?;
                let pn_str: String = row.get(2).map_err(to_store_err)?;
                let sig_bytes: Vec<u8> = row.get(8).map_err(to_store_err)?;
                let adv_bytes: Vec<u8> = row.get(9).map_err(to_store_err)?;
                let account_bytes: Option<Vec<u8>> = row.get(10).map_err(to_store_err)?;
                let mut signature = [0u8; 64];
                if sig_bytes.len() == 64 {
                    signature.copy_from_slice(&sig_bytes);
                }
                let mut adv_secret = [0u8; 32];
                if adv_bytes.len() == 32 {
                    adv_secret.copy_from_slice(&adv_bytes);
                }
                let account = account_bytes
                    .map(|b| {
                        waproto::whatsapp::ADVSignedDeviceIdentity::decode_from_slice(&b)
                            .map(Arc::new)
                            .map_err(|e| {
                                wacore::store::error::StoreError::Serialization(Box::new(e))
                            })
                    })
                    .transpose()?;
                let cert_bytes: Option<Vec<u8>> = row.get(21).map_err(to_store_err)?;
                let cert_chain = cert_bytes
                    .map(|b| {
                        serde_json::from_slice(&b).map_err(|e| {
                            wacore::store::error::StoreError::Serialization(Box::new(e))
                        })
                    })
                    .transpose()?;
                let lid = if lid_str.is_empty() {
                    None
                } else {
                    lid_str.parse().ok()
                };
                let pn = if pn_str.is_empty() {
                    None
                } else {
                    pn_str.parse().ok()
                };

                Ok(Some(CoreDevice {
                    lid,
                    pn,
                    registration_id: row.get::<i64>(3).map_err(to_store_err)? as u32,
                    noise_key,
                    identity_key,
                    signed_pre_key,
                    signed_pre_key_id: row.get::<i64>(7).map_err(to_store_err)? as u32,
                    signed_pre_key_signature: signature,
                    adv_secret_key: adv_secret,
                    account,
                    push_name: row.get(11).map_err(to_store_err)?,
                    app_version_primary: row.get::<i64>(12).map_err(to_store_err)? as u32,
                    app_version_secondary: row.get::<i64>(13).map_err(to_store_err)? as u32,
                    app_version_tertiary: row.get::<i64>(14).map_err(to_store_err)? as u32,
                    app_version_last_fetched_ms: row.get::<i64>(15).map_err(to_store_err)?,
                    // R13-H1 fix: `edge_routing_info` is declared as
                    // `BLOB` in the schema (line 137) and saved as
                    // `stoolap::core::Value::blob(...)` (line 1046), but
                    // was being read back as `String`. Stoolap's
                    // `FromValue for String` impl returns `String::new()`
                    // for `Value::Blob`, so every non-empty BLOB silently
                    // mapped to `None` and the actual bytes were
                    // discarded on every load. The field is set by
                    // wacore's IB handshake handler
                    // (`wacore/src/handlers/ib.rs:128`) and used by the
                    // noise layer for edge-routed handshakes, so this
                    // bug caused every restart to fall back to the
                    // slower default `WA_CONN_HEADER` path. The fix
                    // reads the column directly as `Option<Vec<u8>>`
                    // (the same pattern as `account` at line 1089 and
                    // `server_cert_chain` at line 1105).
                    edge_routing_info: row.get(16).map_err(to_store_err)?,
                    props_hash: {
                        let v: String = row.get(17).map_err(to_store_err)?;
                        if v.is_empty() {
                            None
                        } else {
                            Some(v)
                        }
                    },
                    next_pre_key_id: row.get::<i64>(18).map_err(to_store_err)? as u32,
                    server_has_prekeys: row.get::<i64>(19).map_err(to_store_err)? != 0,
                    nct_salt: row.get(20).map_err(to_store_err)?,
                    server_cert_chain: cert_chain,
                    login_counter: row.get::<i64>(22).map_err(to_store_err)? as i32,
                    ..Default::default()
                }))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(None),
        }
    }

    async fn exists(&self) -> wacore::store::error::Result<bool> {
        let mut rows = query(
            &*self.db.lock().await,
            "SELECT COUNT(*) FROM device WHERE id = $1",
            vec![(self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let count: i64 = row.get(0).map_err(to_store_err)?;
                Ok(count > 0)
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Ok(false),
        }
    }

    async fn create(&self) -> wacore::store::error::Result<i32> {
        Ok(self.device_id)
    }

    async fn snapshot_db(
        &self,
        _name: &str,
        _extra_content: Option<&[u8]>,
    ) -> wacore::store::error::Result<()> {
        tracing::warn!("snapshot_db: stoolap does not support file snapshots, skipping");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wacore::store::traits::AppStateSyncKey;

    #[tokio::test]
    async fn sync_key_roundtrip_preserves_bytes() {
        // The critical-app-state-sync path computes HMAC-SHA256 keys via
        // `expand_app_state_keys(&key_data)`. If the bytes that come out
        // of get_sync_key differ from the bytes that went in via
        // set_sync_key, all derived keys are wrong and every snapshot/patch
        // MAC mismatches (this is exactly the "patch snapshot MAC
        // mismatch" failure mode we hit in R10 production runs). This
        // test pins the roundtrip and would have caught the bug.
        let store = StoolapStore::new_in_memory().unwrap();
        let key_id: Vec<u8> = (0..32).collect();
        let original = AppStateSyncKey {
            // Pick bytes that exercise all 64 byte values, including
            // ones that base64 might mangle in some implementations.
            key_data: (0u8..=255).collect(),
            fingerprint: vec![0xAB; 32],
            timestamp: 1_700_000_000_000,
        };
        store
            .set_sync_key(&key_id, original.clone())
            .await
            .expect("set_sync_key should succeed");
        let roundtripped = store
            .get_sync_key(&key_id)
            .await
            .expect("get_sync_key should succeed")
            .expect("get_sync_key should find the key");
        assert_eq!(
            roundtripped.key_data, original.key_data,
            "key_data bytes must roundtrip exactly (HMAC key material)"
        );
        assert_eq!(roundtripped.fingerprint, original.fingerprint);
        assert_eq!(roundtripped.timestamp, original.timestamp);
    }

    #[tokio::test]
    async fn hash_state_roundtrip_preserves_bytes() {
        // Same pin for the app-state version state. The snapshot MAC is
        // HMAC(snapshot_mac_key, hash || version_be || collection_name),
        // so a corrupted `hash` field would also produce "patch snapshot
        // MAC mismatch" errors.
        let store = StoolapStore::new_in_memory().unwrap();
        let original = HashState {
            version: 7,
            hash: [0xCD; 128],
            index_value_map: std::collections::HashMap::new(),
        };
        store
            .set_version("critical_block", original.clone())
            .await
            .expect("set_version should succeed");
        let roundtripped = store
            .get_version("critical_block")
            .await
            .expect("get_version should succeed");
        assert_eq!(roundtripped.version, original.version);
        assert_eq!(roundtripped.hash, original.hash);
    }

    #[tokio::test]
    async fn sync_key_roundtrip_persisted_file() {
        // The in-memory path is one code path; the file-backed
        // `Database::open("file://...")` used in production is another.
        // Pin the roundtrip through the file-backed DSN path, which is
        // the exact code path the CLI exercises in `start_bot`.
        use wacore::store::traits::AppStateSyncKey;
        let dir = tempdir_unique();
        let store = StoolapStore::new(&dir).expect("file-backed store should open");
        let key_id: Vec<u8> = (0..32).collect();
        let original = AppStateSyncKey {
            key_data: (0u8..=255).collect(),
            fingerprint: vec![0xAB; 32],
            timestamp: 1_700_000_000_000,
        };
        store
            .set_sync_key(&key_id, original.clone())
            .await
            .expect("set_sync_key should succeed");
        drop(store);
        // Reopen to force a read from disk (not just from a page cache).
        let store2 = StoolapStore::new(&dir).expect("reopen should work");
        let roundtripped = store2
            .get_sync_key(&key_id)
            .await
            .expect("get_sync_key should succeed")
            .expect("key should be present after reopen");
        assert_eq!(
            roundtripped.key_data, original.key_data,
            "key_data bytes must roundtrip exactly through the file-backed DSN"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tempdir_unique() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("octo-store-test-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn mutation_mac_value_bytes_roundtrip_raw() {
        // R12 pin: get_mutation_mac must return the original 32-byte
        // value MAC, not a JSON/base64 wrapper. The ltHash arithmetic
        // in `WAPATCH_INTEGRITY.subtract_then_add_in_place` runs over
        // these exact bytes — a JSON wrapper would change every value
        // MAC by a few bytes, corrupting the ltHash and producing
        // "patch snapshot MAC mismatch" on the next sync.
        let store = StoolapStore::new_in_memory().unwrap();
        let index_mac: Vec<u8> = (0u8..32).collect();
        let value_mac: Vec<u8> = (0u8..32).map(|b| b ^ 0xAA).collect();
        let mac = wacore::appstate::processor::AppStateMutationMAC {
            index_mac: index_mac.clone(),
            value_mac: value_mac.clone(),
        };
        store
            .put_mutation_macs("critical_block", 1, &[mac])
            .await
            .expect("put_mutation_macs should succeed");
        let got = store
            .get_mutation_mac("critical_block", &index_mac)
            .await
            .expect("get_mutation_mac should succeed")
            .expect("mutation mac should be present");
        assert_eq!(
            got, value_mac,
            "value_mac must roundtrip as the original 32 raw bytes, not JSON-wrapped"
        );
    }

    #[tokio::test]
    async fn put_mutation_macs_is_idempotent_on_overwrite() {
        // R13 pin: a patch can legitimately SET an index_mac that was
        // already SET by a snapshot (or by an earlier mutation in the
        // same patch — the in-patch prev-value lookup happens after
        // put_mutation_macs runs, so the second insert for the same
        // index_mac is a real case). The UNIQUE(name, index_mac,
        // device_id) constraint would reject a plain INSERT, aborting
        // the whole critical-sync with a "database operation error".
        //
        // R15 update: the fix is now a single-statement UPSERT
        // (`INSERT ... ON DUPLICATE KEY UPDATE`) instead of
        // DELETE-then-INSERT. This requires two underlying Stoolap
        // bugs to be fixed (composite-unique UPSERT support and
        // tx-local delete visibility), both of which landed in
        // stoolap commit `1fc5bc2`. The functional behavior is the
        // same — overwriting an existing row is allowed and updates
        // the value_mac — but it's now one statement and one tx
        // instead of two statements per iteration.
        let store = StoolapStore::new_in_memory().unwrap();
        let index_mac: Vec<u8> = (0u8..32).collect();
        let first = wacore::appstate::processor::AppStateMutationMAC {
            index_mac: index_mac.clone(),
            value_mac: vec![0x11; 32],
        };
        let second = wacore::appstate::processor::AppStateMutationMAC {
            index_mac: index_mac.clone(),
            value_mac: vec![0x22; 32],
        };
        store
            .put_mutation_macs("critical_block", 1, &[first])
            .await
            .expect("first put_mutation_macs should succeed");
        store
            .put_mutation_macs("critical_block", 2, &[second])
            .await
            .expect("second put_mutation_macs with same index_mac must succeed (UPSERT)");
        let got = store
            .get_mutation_mac("critical_block", &index_mac)
            .await
            .expect("get_mutation_mac should succeed")
            .expect("mutation mac should be present after overwrite");
        assert_eq!(
            got,
            vec![0x22; 32],
            "second put must overwrite the first value_mac"
        );
    }

    #[tokio::test]
    async fn device_roundtrip_preserves_edge_routing_info() {
        // R13-H1 fix: `device.edge_routing_info` is a `BLOB` column
        // and was being read as `String`. Stoolap's
        // `FromValue for String` impl returns `String::new()` for
        // `Value::Blob`, so every non-empty BLOB silently mapped
        // to `None` on load. The field is set by wacore's IB
        // handshake handler (`wacore/src/handlers/ib.rs:128`) and
        // consumed by the noise layer to build edge-routed
        // handshakes; losing it on every restart caused the next
        // connection to fall back to the slower default
        // `WA_CONN_HEADER` path. The fix reads the column
        // directly as `Option<Vec<u8>>` (same pattern as
        // `account` / `server_cert_chain`).
        //
        // No previous round caught this bug because no test
        // roundtripped a non-default `CoreDevice` — and
        // `edge_routing_info`'s default value (`None`) is
        // exactly what the buggy `if v.is_empty() { None }`
        // branch would return, so the bug was masked by the
        // absence of a test, not by the type system.
        let store = StoolapStore::new_in_memory().unwrap();
        let mut device = wacore::store::Device::default();
        // Pick bytes that exercise all 256 values, including
        // ones that string-conversion would mangle.
        let original: Vec<u8> = (0u8..=255).collect();
        device.edge_routing_info = Some(original.clone());
        store.save(&device).await.expect("save should succeed");
        let loaded = store
            .load()
            .await
            .expect("load should succeed")
            .expect("device row should exist after save");
        assert_eq!(
            loaded.edge_routing_info,
            Some(original),
            "edge_routing_info BLOB must roundtrip exactly (set by wacore IB handshake, \
             consumed by the noise layer for edge-routed handshakes; losing it falls \
             back to the slower default WA_CONN_HEADER path on every restart)"
        );
    }

    #[tokio::test]
    async fn put_mutation_macs_batch_inserts_all_and_overwrites() {
        // R14-H1 audit test: the DELETE+INSERT loop in
        // `put_mutation_macs` was per-iteration atomic (R13 fix) but
        // NOT batch-atomic. This test pins the per-iteration
        // atomicity for a 10-entry batch: every entry must be
        // present after the call returns (no partial-batch loss),
        // and a second call with the same index_macs but different
        // value_macs must overwrite (not duplicate) every entry.
        // The batch-atomicity guarantee was CARVED OUT in R14 (see
        // the function's R14-H1 doc-comment at that revision).
        //
        // R15 update: the carve-out is now CLOSED. `put_mutation_macs`
        // is now a single-statement UPSERT (`INSERT ... ON DUPLICATE
        // KEY UPDATE`) wrapped in a single transaction. Both
        // per-iteration atomicity AND batch-atomicity are guaranteed.
        // This test still pins per-iteration atomicity (10 entries
        // must all be present after a single call), so it remains
        // useful as a regression test for the R13 idempotency fix.
        use wacore::store::traits::AppSyncStore;
        let store = StoolapStore::new_in_memory().unwrap();
        let batch: Vec<wacore::appstate::processor::AppStateMutationMAC> = (0u8..10)
            .map(|i| wacore::appstate::processor::AppStateMutationMAC {
                index_mac: vec![i; 32],
                value_mac: vec![i ^ 0xAA; 32],
            })
            .collect();
        store
            .put_mutation_macs("critical_block", 1, &batch)
            .await
            .expect("put_mutation_macs (10 entries) should succeed");

        // Verify all 10 entries are present with the right value_macs.
        for (i, m) in batch.iter().enumerate() {
            let got = store
                .get_mutation_mac("critical_block", &m.index_mac)
                .await
                .expect("get_mutation_mac should succeed")
                .unwrap_or_else(|| panic!("mutation {i} should be present after batch insert"));
            assert_eq!(
                got, m.value_mac,
                "mutation {i}: value_mac must roundtrip exactly after batch insert"
            );
        }

        // Second call with a different version — same set of
        // index_macs, different value_macs. Pin that the
        // DELETE-then-INSERT runs for every entry (so the row is
        // updated, not duplicated).
        let batch2: Vec<wacore::appstate::processor::AppStateMutationMAC> = (0u8..10)
            .map(|i| wacore::appstate::processor::AppStateMutationMAC {
                index_mac: vec![i; 32],
                value_mac: vec![i ^ 0x55; 32],
            })
            .collect();
        store
            .put_mutation_macs("critical_block", 2, &batch2)
            .await
            .expect("put_mutation_macs (10 entries, overwrite) should succeed");
        for (i, m) in batch2.iter().enumerate() {
            let got = store
                .get_mutation_mac("critical_block", &m.index_mac)
                .await
                .expect("get_mutation_mac should succeed")
                .unwrap_or_else(|| panic!("mutation {i} should be present after batch overwrite"));
            assert_eq!(
                got, m.value_mac,
                "mutation {i}: value_mac must be the new one after batch overwrite"
            );
        }
    }

    #[tokio::test]
    async fn delete_expired_tc_tokens_returns_actual_count() {
        // R14-M5 fix: `delete_expired_tc_tokens` and
        // `delete_expired_sent_messages` now use a single DELETE
        // statement and return the rows-affected count from
        // `Database::execute` (which is `i64` — see
        // `stoolap/src/api/database.rs:483`). The previous
        // SELECT-COUNT + DELETE returned the COUNT(*) from a
        // separate SELECT, which could diverge from the actual
        // number of rows deleted if a concurrent insert happened
        // between the SELECT and the DELETE. Pin that the new
        // single-DELETE path returns the EXACT count of rows
        // deleted (3), not a count from a separate SELECT.
        use wacore::store::traits::ProtocolStore;
        use wacore::store::traits::TcTokenEntry;
        let store = StoolapStore::new_in_memory().unwrap();
        // Insert 5 tc_tokens; 3 with old timestamp, 2 with recent.
        let now: i64 = 1_700_000_000_000;
        for i in 0..5 {
            let jid = format!("user{i}@s.whatsapp.net");
            let ts = if i < 3 { now - 86_400_000 } else { now };
            let entry = TcTokenEntry {
                token: vec![0xAB; 32],
                token_timestamp: ts,
                sender_timestamp: Some(ts),
            };
            store
                .put_tc_token(&jid, &entry)
                .await
                .expect("put_tc_token should succeed");
        }
        // Cutoff is between the old and recent timestamps.
        let cutoff = now - 3_600_000;
        let deleted = store
            .delete_expired_tc_tokens(cutoff, 0)
            .await
            .expect("delete_expired_tc_tokens should succeed");
        assert_eq!(
            deleted, 3,
            "delete_expired_tc_tokens must return the exact rows-affected count \
             (3 tokens with timestamp < cutoff), not a separate COUNT(*) that \
             could diverge from the DELETE under concurrent inserts"
        );
        // Second call should return 0 — the remaining 2 are recent
        // and the deleted 3 are gone.
        let deleted2 = store
            .delete_expired_tc_tokens(cutoff, 0)
            .await
            .expect("second delete_expired_tc_tokens should succeed");
        assert_eq!(
            deleted2, 0,
            "second call with same cutoff must return 0 (no expired rows left)"
        );
    }

    #[tokio::test]
    async fn delete_expired_sent_messages_returns_actual_count() {
        // R14-M5 fix: same as `delete_expired_tc_tokens_returns_actual_count`
        // for the sent_messages table. Pin the single-DELETE path
        // returns the EXACT count, not a separate COUNT(*).
        //
        // The trait `store_sent_message` auto-sets `created_at` to
        // `chrono::Utc::now().timestamp()` so we can't pin a custom
        // timestamp through the trait API. Workaround: insert
        // directly via SQL with a custom `created_at` so we can
        // set up a known distribution of "old" vs "new" rows.
        use wacore::store::traits::ProtocolStore;
        let store = StoolapStore::new_in_memory().unwrap();
        let now: i64 = 1_700_000_000_000;
        for i in 0..4 {
            let chat = format!("chat{i}@s.whatsapp.net");
            let msg_id = format!("msg{i}");
            // 2 old (i < 2), 2 recent.
            let created_at = if i < 2 { now - 86_400_000 } else { now };
            exec(
                &store.db.try_lock().unwrap(),
                "INSERT INTO sent_messages (chat_jid, message_id, payload, device_id, created_at) VALUES ($1, $2, $3, $4, $5)",
                vec![
                    chat.to_string().into(),
                    msg_id.to_string().into(),
                    stoolap::core::Value::blob(vec![0xAB; 32]),
                    (store.device_id as i64).into(),
                    created_at.into(),
                ],
            )
            .expect("direct INSERT should succeed");
        }
        let cutoff = now - 3_600_000;
        let deleted = store
            .delete_expired_sent_messages(cutoff)
            .await
            .expect("delete_expired_sent_messages should succeed");
        assert_eq!(
            deleted, 2,
            "delete_expired_sent_messages must return the exact rows-affected count \
             (2 messages with created_at < cutoff), not a separate COUNT(*) that \
             could diverge from the DELETE under concurrent inserts"
        );
        let deleted2 = store
            .delete_expired_sent_messages(cutoff)
            .await
            .expect("second delete_expired_sent_messages should succeed");
        assert_eq!(
            deleted2, 0,
            "second call with same cutoff must return 0 (no expired messages left)"
        );
    }
}
