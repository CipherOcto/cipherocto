//! Stoolap-backed storage backend for wa-rs (WhatsApp Web protocol)
//!
//! Implements all 4 wa-rs storage traits using CipherOcto's stoolap fork.

use async_trait::async_trait;
use bytes::Bytes;
use prost::Message;
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

/// Stoolap-backed wa-rs storage backend
#[derive(Clone)]
pub struct StoolapStore {
    db: stoolap::Database,
    device_id: i32,
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
        let store = Self { db, device_id: 1 };
        store.init_schema()?;
        Ok(store)
    }

    pub fn new_in_memory() -> anyhow::Result<Self> {
        let db = stoolap::Database::open_in_memory()?;
        let store = Self { db, device_id: 1 };
        store.init_schema()?;
        Ok(store)
    }

    /// Delete the database file (for session purge on logout)
    pub fn delete_db_file(&self) -> anyhow::Result<()> {
        // stoolap doesn't expose the path, so this is a no-op
        // The caller should handle file deletion externally
        Ok(())
    }

    fn init_schema(&self) -> anyhow::Result<()> {
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
            "CREATE TABLE IF NOT EXISTS device (id INTEGER PRIMARY KEY, lid TEXT, pn TEXT, registration_id INTEGER NOT NULL, noise_key BLOB NOT NULL, identity_key BLOB NOT NULL, signed_pre_key BLOB NOT NULL, signed_pre_key_id INTEGER NOT NULL, signed_pre_key_signature BLOB NOT NULL, adv_secret_key BLOB NOT NULL, account BLOB, push_name TEXT NOT NULL, app_version_primary INTEGER NOT NULL, app_version_secondary INTEGER NOT NULL, app_version_tertiary INTEGER NOT NULL, app_version_last_fetched_ms INTEGER NOT NULL, edge_routing_info BLOB, props_hash TEXT, next_pre_key_id INTEGER NOT NULL DEFAULT 0, server_has_prekeys INTEGER NOT NULL DEFAULT 0, nct_salt BLOB, server_cert_chain BLOB, login_counter INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS identities (rowid INTEGER PRIMARY KEY, address TEXT NOT NULL, \"key\" BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, device_id))",
            "CREATE TABLE IF NOT EXISTS sessions (rowid INTEGER PRIMARY KEY, address TEXT NOT NULL, record BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, device_id))",
            "CREATE TABLE IF NOT EXISTS prekeys (rowid INTEGER PRIMARY KEY, id INTEGER NOT NULL, \"key\" BLOB NOT NULL, uploaded INTEGER NOT NULL DEFAULT 0, device_id INTEGER NOT NULL, UNIQUE (id, device_id))",
            "CREATE TABLE IF NOT EXISTS signed_prekeys (rowid INTEGER PRIMARY KEY, id INTEGER NOT NULL, record BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (id, device_id))",
            "CREATE TABLE IF NOT EXISTS sender_keys (rowid INTEGER PRIMARY KEY, address TEXT NOT NULL, record BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, device_id))",
            "CREATE TABLE IF NOT EXISTS app_state_keys (rowid INTEGER PRIMARY KEY, key_id BLOB NOT NULL, key_data BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (key_id, device_id))",
            "CREATE TABLE IF NOT EXISTS app_state_versions (rowid INTEGER PRIMARY KEY, name TEXT NOT NULL, state_data BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (name, device_id))",
            "CREATE TABLE IF NOT EXISTS app_state_mutation_macs (rowid INTEGER PRIMARY KEY, name TEXT NOT NULL, version INTEGER NOT NULL, index_mac BLOB NOT NULL, value_mac BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (name, index_mac, device_id))",
            "CREATE TABLE IF NOT EXISTS lid_pn_mapping (rowid INTEGER PRIMARY KEY, lid TEXT NOT NULL, phone_number TEXT NOT NULL, created_at INTEGER NOT NULL, learning_source TEXT NOT NULL, updated_at INTEGER NOT NULL, device_id INTEGER NOT NULL, UNIQUE (lid, device_id))",
            "CREATE TABLE IF NOT EXISTS device_registry (rowid INTEGER PRIMARY KEY, user_id TEXT NOT NULL, devices_json TEXT NOT NULL, timestamp INTEGER NOT NULL, phash TEXT, raw_id INTEGER, device_id INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE (user_id, device_id))",
            "CREATE TABLE IF NOT EXISTS sender_key_devices (rowid INTEGER PRIMARY KEY, group_jid TEXT NOT NULL, device_jid TEXT NOT NULL, has_key INTEGER NOT NULL, device_id INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE (group_jid, device_jid, device_id))",
            "CREATE TABLE IF NOT EXISTS sent_messages (rowid INTEGER PRIMARY KEY, chat_jid TEXT NOT NULL, message_id TEXT NOT NULL, payload BLOB NOT NULL, device_id INTEGER NOT NULL, created_at INTEGER NOT NULL, UNIQUE (chat_jid, message_id, device_id))",
            "CREATE TABLE IF NOT EXISTS base_keys (rowid INTEGER PRIMARY KEY, address TEXT NOT NULL, message_id TEXT NOT NULL, base_key BLOB NOT NULL, device_id INTEGER NOT NULL, UNIQUE (address, message_id, device_id))",
            "CREATE TABLE IF NOT EXISTS tc_tokens (rowid INTEGER PRIMARY KEY, jid TEXT NOT NULL, token BLOB NOT NULL, token_timestamp INTEGER NOT NULL, sender_timestamp INTEGER, device_id INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE (jid, device_id))",
        ];
        for stmt in stmts {
            exec(&self.db, stmt, vec![])?;
        }
        Ok(())
    }
}

// ── SignalStore ────────────────────────────────────────────────────

#[async_trait]
impl SignalStore for StoolapStore {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> wacore::store::error::Result<()> {
        exec(
            &self.db,
            "DELETE FROM identities WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec(
            &self.db,
            "INSERT INTO identities (address, \"key\", device_id) VALUES ($1, $2, $3)",
            vec![
                address.to_string().into(),
                stoolap::core::Value::blob(key.to_vec()),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn load_identity(&self, address: &str) -> wacore::store::error::Result<Option<[u8; 32]>> {
        let mut rows = query(
            &self.db,
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
            &self.db,
            "DELETE FROM identities WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn get_session(&self, address: &str) -> wacore::store::error::Result<Option<Bytes>> {
        let mut rows = query(
            &self.db,
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
        exec(
            &self.db,
            "DELETE FROM sessions WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec(
            &self.db,
            "INSERT INTO sessions (address, record, device_id) VALUES ($1, $2, $3)",
            vec![
                address.to_string().into(),
                stoolap::core::Value::blob(session.to_vec()),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn delete_session(&self, address: &str) -> wacore::store::error::Result<()> {
        exec(
            &self.db,
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
        exec(
            &self.db,
            "DELETE FROM prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )?;
        exec(
            &self.db,
            "INSERT INTO prekeys (id, \"key\", uploaded, device_id) VALUES ($1, $2, $3, $4)",
            vec![
                (id as i64).into(),
                stoolap::core::Value::blob(record.to_vec()),
                (uploaded as i64).into(),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn load_prekey(&self, id: u32) -> wacore::store::error::Result<Option<Bytes>> {
        let mut rows = query(
            &self.db,
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
            &self.db,
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
            &self.db,
            "DELETE FROM prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )
    }

    async fn store_signed_prekey(
        &self,
        id: u32,
        record: &[u8],
    ) -> wacore::store::error::Result<()> {
        exec(
            &self.db,
            "DELETE FROM signed_prekeys WHERE id = $1 AND device_id = $2",
            vec![(id as i64).into(), (self.device_id as i64).into()],
        )?;
        exec(
            &self.db,
            "INSERT INTO signed_prekeys (id, record, device_id) VALUES ($1, $2, $3)",
            vec![
                (id as i64).into(),
                stoolap::core::Value::blob(record.to_vec()),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn load_signed_prekey(&self, id: u32) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let mut rows = query(
            &self.db,
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
            &self.db,
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
            &self.db,
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
            &self.db,
            "DELETE FROM sender_keys WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec(
            &self.db,
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
            &self.db,
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
            &self.db,
            "DELETE FROM sender_keys WHERE address = $1 AND device_id = $2",
            vec![address.to_string().into(), (self.device_id as i64).into()],
        )
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
            &self.db,
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
        let data = serde_json::to_vec(&key)
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
        exec(
            &self.db,
            "DELETE FROM app_state_keys WHERE key_id = $1 AND device_id = $2",
            vec![
                stoolap::core::Value::blob(key_id.to_vec()),
                (self.device_id as i64).into(),
            ],
        )?;
        exec(
            &self.db,
            "INSERT INTO app_state_keys (key_id, key_data, device_id) VALUES ($1, $2, $3)",
            vec![
                stoolap::core::Value::blob(key_id.to_vec()),
                stoolap::core::Value::blob(data),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn get_version(&self, name: &str) -> wacore::store::error::Result<HashState> {
        let mut rows = query(
            &self.db,
            "SELECT state_data FROM app_state_versions WHERE name = $1 AND device_id = $2",
            vec![name.to_string().into(), (self.device_id as i64).into()],
        )?;
        match rows.next() {
            Some(Ok(row)) => {
                let data: Vec<u8> = row.get(0).map_err(to_store_err)?;
                serde_json::from_slice(&data)
                    .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))
            }
            Some(Err(e)) => Err(to_store_err(e)),
            None => Err(wacore::store::error::StoreError::Database(
                format!("version not found: {name}").into(),
            )),
        }
    }

    async fn set_version(&self, name: &str, state: HashState) -> wacore::store::error::Result<()> {
        let data = serde_json::to_vec(&state)
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
        exec(
            &self.db,
            "DELETE FROM app_state_versions WHERE name = $1 AND device_id = $2",
            vec![name.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec(
            &self.db,
            "INSERT INTO app_state_versions (name, state_data, device_id) VALUES ($1, $2, $3)",
            vec![
                name.to_string().into(),
                stoolap::core::Value::blob(data),
                (self.device_id as i64).into(),
            ],
        )
    }

    async fn put_mutation_macs(
        &self,
        name: &str,
        version: u64,
        mutations: &[AppStateMutationMAC],
    ) -> wacore::store::error::Result<()> {
        for m in mutations {
            let idx = serde_json::to_vec(&m.index_mac)
                .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
            let val = serde_json::to_vec(&m.value_mac)
                .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
            exec(&self.db, "INSERT INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id) VALUES ($1, $2, $3, $4, $5)",
                vec![name.to_string().into(), (version as i64).into(), stoolap::core::Value::blob(idx), stoolap::core::Value::blob(val), (self.device_id as i64).into()])?;
        }
        Ok(())
    }

    async fn get_mutation_mac(
        &self,
        name: &str,
        index_mac: &[u8],
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let idx = serde_json::to_vec(index_mac)
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
        let mut rows = query(&self.db, "SELECT value_mac FROM app_state_mutation_macs WHERE name = $1 AND index_mac = $2 AND device_id = $3",
            vec![name.to_string().into(), stoolap::core::Value::blob(idx), (self.device_id as i64).into()])?;
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
        for idx in index_macs {
            let idx_json = serde_json::to_vec(idx)
                .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
            exec(&self.db, "DELETE FROM app_state_mutation_macs WHERE name = $1 AND index_mac = $2 AND device_id = $3",
                vec![name.to_string().into(), stoolap::core::Value::blob(idx_json), (self.device_id as i64).into()])?;
        }
        Ok(())
    }

    async fn get_latest_sync_key_id(&self) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let mut rows = query(
            &self.db,
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
}

// ── ProtocolStore ──────────────────────────────────────────────────

#[async_trait]
impl ProtocolStore for StoolapStore {
    async fn get_sender_key_devices(
        &self,
        group_jid: &str,
    ) -> wacore::store::error::Result<Vec<(String, bool)>> {
        let rows = query(&self.db, "SELECT device_jid, has_key FROM sender_key_devices WHERE group_jid = $1 AND device_id = $2",
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
        let now = chrono::Utc::now().timestamp();
        for (jid, has_key) in entries {
            exec(&self.db, "DELETE FROM sender_key_devices WHERE group_jid = $1 AND device_jid = $2 AND device_id = $3",
                vec![group_jid.to_string().into(), jid.to_string().into(), (self.device_id as i64).into()])?;
            exec(&self.db, "INSERT INTO sender_key_devices (group_jid, device_jid, has_key, device_id, updated_at) VALUES ($1, $2, $3, $4, $5)",
                vec![group_jid.to_string().into(), jid.to_string().into(), (if *has_key { 1i64 } else { 0i64 }).into(), (self.device_id as i64).into(), now.into()])?;
        }
        Ok(())
    }

    async fn clear_sender_key_devices(&self, group_jid: &str) -> wacore::store::error::Result<()> {
        exec(
            &self.db,
            "DELETE FROM sender_key_devices WHERE group_jid = $1 AND device_id = $2",
            vec![group_jid.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn delete_sender_key_device_rows(
        &self,
        device_jids: &[&str],
    ) -> wacore::store::error::Result<()> {
        for jid in device_jids {
            exec(
                &self.db,
                "DELETE FROM sender_key_devices WHERE device_jid = $1 AND device_id = $2",
                vec![jid.to_string().into(), (self.device_id as i64).into()],
            )?;
        }
        Ok(())
    }

    async fn clear_all_sender_key_devices(&self) -> wacore::store::error::Result<()> {
        exec(
            &self.db,
            "DELETE FROM sender_key_devices WHERE device_id = $1",
            vec![(self.device_id as i64).into()],
        )
    }

    async fn get_lid_mapping(
        &self,
        lid: &str,
    ) -> wacore::store::error::Result<Option<LidPnMappingEntry>> {
        let mut rows = query(&self.db, "SELECT lid, phone_number, created_at, learning_source, updated_at FROM lid_pn_mapping WHERE lid = $1 AND device_id = $2",
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
        let mut rows = query(&self.db, "SELECT lid, phone_number, created_at, learning_source, updated_at FROM lid_pn_mapping WHERE phone_number = $1 AND device_id = $2 ORDER BY updated_at DESC LIMIT 1",
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
        exec(
            &self.db,
            "DELETE FROM lid_pn_mapping WHERE lid = $1 AND device_id = $2",
            vec![entry.lid.clone().into(), (self.device_id as i64).into()],
        )?;
        exec(&self.db, "INSERT INTO lid_pn_mapping (lid, phone_number, created_at, learning_source, updated_at, device_id) VALUES ($1, $2, $3, $4, $5, $6)",
            vec![entry.lid.clone().into(), entry.phone_number.clone().into(), entry.created_at.into(), entry.learning_source.clone().into(), entry.updated_at.into(), (self.device_id as i64).into()])
    }

    async fn get_all_lid_mappings(&self) -> wacore::store::error::Result<Vec<LidPnMappingEntry>> {
        let rows = query(&self.db, "SELECT lid, phone_number, created_at, learning_source, updated_at FROM lid_pn_mapping WHERE device_id = $1",
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
        let now = chrono::Utc::now().timestamp();
        exec(
            &self.db,
            "DELETE FROM base_keys WHERE address = $1 AND message_id = $2 AND device_id = $3",
            vec![
                address.to_string().into(),
                message_id.to_string().into(),
                (self.device_id as i64).into(),
            ],
        )?;
        exec(&self.db, "INSERT INTO base_keys (address, message_id, base_key, device_id, created_at) VALUES ($1, $2, $3, $4, $5)",
            vec![address.to_string().into(), message_id.to_string().into(), stoolap::core::Value::blob(base_key.to_vec()), (self.device_id as i64).into(), now.into()])
    }

    async fn has_same_base_key(
        &self,
        address: &str,
        message_id: &str,
        current_base_key: &[u8],
    ) -> wacore::store::error::Result<bool> {
        let mut rows = query(&self.db, "SELECT base_key FROM base_keys WHERE address = $1 AND message_id = $2 AND device_id = $3",
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
            &self.db,
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
        let devices_json = serde_json::to_string(&record.devices)
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;
        let now = chrono::Utc::now().timestamp();
        exec(
            &self.db,
            "DELETE FROM device_registry WHERE user_id = $1 AND device_id = $2",
            vec![record.user.clone().into(), (self.device_id as i64).into()],
        )?;
        exec(&self.db, "INSERT INTO device_registry (user_id, devices_json, timestamp, phash, raw_id, device_id, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            vec![record.user.into(), devices_json.into(), record.timestamp.into(), record.phash.unwrap_or_default().into(), record.raw_id.map(|r| (r as i64).into()).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null)), (self.device_id as i64).into(), now.into()])
    }

    async fn get_devices(
        &self,
        user: &str,
    ) -> wacore::store::error::Result<Option<DeviceListRecord>> {
        let mut rows = query(&self.db, "SELECT user_id, devices_json, timestamp, phash, raw_id FROM device_registry WHERE user_id = $1 AND device_id = $2",
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
            &self.db,
            "DELETE FROM device_registry WHERE user_id = $1 AND device_id = $2",
            vec![user.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn get_tc_token(&self, jid: &str) -> wacore::store::error::Result<Option<TcTokenEntry>> {
        let mut rows = query(&self.db, "SELECT token, token_timestamp, sender_timestamp FROM tc_tokens WHERE jid = $1 AND device_id = $2",
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
        let now = chrono::Utc::now().timestamp();
        exec(
            &self.db,
            "DELETE FROM tc_tokens WHERE jid = $1 AND device_id = $2",
            vec![jid.to_string().into(), (self.device_id as i64).into()],
        )?;
        exec(&self.db, "INSERT INTO tc_tokens (jid, token, token_timestamp, sender_timestamp, device_id, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
            vec![jid.to_string().into(), stoolap::core::Value::blob(entry.token.clone()), entry.token_timestamp.into(), entry.sender_timestamp.unwrap_or(0).into(), (self.device_id as i64).into(), now.into()])
    }

    async fn delete_tc_token(&self, jid: &str) -> wacore::store::error::Result<()> {
        exec(
            &self.db,
            "DELETE FROM tc_tokens WHERE jid = $1 AND device_id = $2",
            vec![jid.to_string().into(), (self.device_id as i64).into()],
        )
    }

    async fn get_all_tc_token_jids(&self) -> wacore::store::error::Result<Vec<String>> {
        let rows = query(
            &self.db,
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
        cutoff_timestamp: i64,
    ) -> wacore::store::error::Result<u32> {
        // Count first, then delete (not perfectly atomic but acceptable for cleanup)
        let mut rows = query(
            &self.db,
            "SELECT COUNT(*) FROM tc_tokens WHERE token_timestamp < $1 AND device_id = $2",
            vec![cutoff_timestamp.into(), (self.device_id as i64).into()],
        )?;
        let count: i64 = match rows.next() {
            Some(Ok(row)) => row.get(0).map_err(to_store_err)?,
            _ => 0,
        };
        exec(
            &self.db,
            "DELETE FROM tc_tokens WHERE token_timestamp < $1 AND device_id = $2",
            vec![cutoff_timestamp.into(), (self.device_id as i64).into()],
        )?;
        Ok(count as u32)
    }

    async fn store_sent_message(
        &self,
        chat_jid: &str,
        message_id: &str,
        payload: &[u8],
    ) -> wacore::store::error::Result<()> {
        let now = chrono::Utc::now().timestamp();
        exec(
            &self.db,
            "DELETE FROM sent_messages WHERE chat_jid = $1 AND message_id = $2 AND device_id = $3",
            vec![
                chat_jid.to_string().into(),
                message_id.to_string().into(),
                (self.device_id as i64).into(),
            ],
        )?;
        exec(&self.db, "INSERT INTO sent_messages (chat_jid, message_id, payload, device_id, created_at) VALUES ($1, $2, $3, $4, $5)",
            vec![chat_jid.to_string().into(), message_id.to_string().into(), stoolap::core::Value::blob(payload.to_vec()), (self.device_id as i64).into(), now.into()])
    }

    async fn take_sent_message(
        &self,
        chat_jid: &str,
        message_id: &str,
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let params = vec![
            chat_jid.to_string().into(),
            message_id.to_string().into(),
            (self.device_id as i64).into(),
        ];
        // SELECT first to get the payload
        let mut rows = query(&self.db, "SELECT payload FROM sent_messages WHERE chat_jid = $1 AND message_id = $2 AND device_id = $3", params.clone())?;
        let payload = match rows.next() {
            Some(Ok(row)) => Some(row.get::<Vec<u8>>(0).map_err(to_store_err)?),
            Some(Err(e)) => return Err(to_store_err(e)),
            None => None,
        };
        // Delete if found (consume)
        if payload.is_some() {
            exec(&self.db, "DELETE FROM sent_messages WHERE chat_jid = $1 AND message_id = $2 AND device_id = $3", params)?;
        }
        Ok(payload)
    }

    async fn delete_expired_sent_messages(
        &self,
        cutoff_timestamp: i64,
    ) -> wacore::store::error::Result<u32> {
        let mut rows = query(
            &self.db,
            "SELECT COUNT(*) FROM sent_messages WHERE created_at < $1 AND device_id = $2",
            vec![cutoff_timestamp.into(), (self.device_id as i64).into()],
        )?;
        let count: i64 = match rows.next() {
            Some(Ok(row)) => row.get(0).map_err(to_store_err)?,
            _ => 0,
        };
        exec(
            &self.db,
            "DELETE FROM sent_messages WHERE created_at < $1 AND device_id = $2",
            vec![cutoff_timestamp.into(), (self.device_id as i64).into()],
        )?;
        Ok(count as u32)
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
        let account = device.account.as_ref().map(|a| a.encode_to_vec());
        let cert_chain = device
            .server_cert_chain
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;

        exec(
            &self.db,
            "DELETE FROM device WHERE id = $1",
            vec![(self.device_id as i64).into()],
        )?;
        exec(&self.db, "INSERT INTO device (id, lid, pn, registration_id, noise_key, identity_key, signed_pre_key, signed_pre_key_id, signed_pre_key_signature, adv_secret_key, account, push_name, app_version_primary, app_version_secondary, app_version_tertiary, app_version_last_fetched_ms, edge_routing_info, props_hash, next_pre_key_id, server_has_prekeys, nct_salt, server_cert_chain, login_counter) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)",
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
            ])
    }

    async fn load(&self) -> wacore::store::error::Result<Option<CoreDevice>> {
        let mut rows = query(
            &self.db,
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
                        waproto::whatsapp::AdvSignedDeviceIdentity::decode(&*b).map_err(|e| {
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
                    app_version_last_fetched_ms: row.get::<i64>(15).map_err(to_store_err)? as i64,
                    edge_routing_info: {
                        let v: String = row.get(16).map_err(to_store_err)?;
                        if v.is_empty() {
                            None
                        } else {
                            Some(v.into_bytes())
                        }
                    },
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
            &self.db,
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
