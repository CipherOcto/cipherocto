//! `SessionStore` trait + stoolap-backed implementation (mission 0850h-d).
//!
//! The trait surface is the direct-getter subset of EXA's
//! `element-x-android/.../SessionStore.kt`: `add_session`, `update_data`,
//! `get_session`, `get_all_sessions`, `get_latest_session`,
//! `number_of_sessions`, `set_latest_session`, `remove_session`. The
//! Flow-based observers (`loggedInStateFlow`, `sessionsFlow`) are
//! intentionally NOT adopted because a CLI does not need a reactive
//! stream.

use crate::models::{LoginType, SessionRow};
use async_trait::async_trait;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Errors the store can surface. The host process (CLI or adapter)
/// decides whether to retry, log, or fail; the store itself does
/// not panic.
#[derive(Debug, thiserror::Error)]
pub enum SessionStoreError {
    /// stoolap I/O / SQL error.
    #[error("session store: {0}")]
    Stoolap(String),
    /// A requested `(user_id, device_id)` row was not found.
    #[error("session not found: user_id={user_id}, device_id={device_id}")]
    NotFound { user_id: String, device_id: String },
    /// `add_session` refused to overwrite an existing row and the
    /// caller did not pass `force`.
    #[error(
        "session already exists for user_id={user_id}, device_id={device_id} (pass force=true to overwrite)"
    )]
    AlreadyExists { user_id: String, device_id: String },
}

pub(crate) fn stoolap_err(e: stoolap::Error) -> SessionStoreError {
    SessionStoreError::Stoolap(e.to_string())
}

/// Direct-getter session store (mission 0850h-d).
///
/// All methods are `async` to match the rest of the platform
/// (octo-network's `PlatformAdapter` is async, the SDK is async).
/// The trait has no `Display`/`Debug` bound on `Self` because the
/// store is typically held as `Arc<dyn SessionStore>`.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Insert a new session. `login_timestamp`, `last_used`, and
    /// `position` are overwritten by the store (the caller should
    /// leave them at their default values; the store derives them
    /// from the system clock and the current max `position`).
    ///
    /// Returns `AlreadyExists` if a row with the same
    /// `(user_id, device_id)` is already present and `force` is
    /// `false`. With `force = true` the existing row is overwritten
    /// (the new row gets a fresh `login_timestamp` and the next
    /// monotonic `position`).
    async fn add_session(&self, row: &SessionRow, force: bool) -> Result<(), SessionStoreError>;

    /// Update mutable fields on an existing session. Only
    /// `access_token`, `refresh_token`, `display_name`, `avatar_url`,
    /// and `last_used` may be mutated; `user_id`, `device_id`,
    /// `login_type`, `login_timestamp`, and `position` are immutable
    /// after insert.
    async fn update_data(&self, row: &SessionRow) -> Result<(), SessionStoreError>;

    /// Look up a session by `(user_id, device_id)`. Returns `None`
    /// when no row matches.
    async fn get_session(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<SessionRow>, SessionStoreError>;

    /// All sessions, ordered by `position` ascending. Stable across
    /// calls (positions are strictly monotonic on insert, so the
    /// order is the chronological multi-account insertion order).
    async fn get_all_sessions(&self) -> Result<Vec<SessionRow>, SessionStoreError>;

    /// The most-recently-used session (highest `last_used`), or
    /// `None` when the store is empty.
    async fn get_latest_session(&self) -> Result<Option<SessionRow>, SessionStoreError>;

    /// Number of sessions in the store.
    async fn number_of_sessions(&self) -> Result<i64, SessionStoreError>;

    /// Mark a session as the latest by updating its `last_used` to
    /// the current epoch seconds. Does NOT change `position` (so
    /// the chronological multi-account ordering is preserved).
    async fn set_latest_session(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), SessionStoreError>;

    /// Remove a session. Returns `NotFound` when the row does not
    /// exist (idempotent callers should match on the variant and
    /// treat it as success).
    async fn remove_session(&self, user_id: &str, device_id: &str)
        -> Result<(), SessionStoreError>;
}

/// Stoolap-backed `SessionStore` (mission 0850h-d).
///
/// The implementation uses `stoolap::Database` directly (no
/// connection pool — stoolap's embedded engine is single-process by
/// design; multi-process deployments route through the matrix-sdk's
/// own distributed locks if needed, which is out of scope for this
/// mission).
#[derive(Clone)]
pub struct StoolapSessionStore {
    db: stoolap::Database,
}

impl std::fmt::Debug for StoolapSessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapSessionStore").finish()
    }
}

impl StoolapSessionStore {
    /// Open a file-backed store at `db_path` (created if missing).
    /// Calls `init_schema` on success.
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, SessionStoreError> {
        // stoolap's `Database::open` takes a DSN like `file:///path/to.db`,
        // not a bare path. `file://` opens (or creates) an on-disk
        // engine; `memory://` is the in-memory variant.
        let path = db_path.as_ref().to_string_lossy().to_string();
        let dsn = if path.starts_with("://") || path.contains("://") {
            path
        } else {
            format!("file://{}", path)
        };
        let db = stoolap::Database::open(&dsn).map_err(stoolap_err)?;
        let store = Self { db };
        crate::schema::init_schema(&store.db)?;
        Ok(store)
    }

    /// Open an in-memory store. Useful for tests and for deployments
    /// that store sessions elsewhere (e.g., a network-mounted
    /// secret manager).
    pub fn new_in_memory() -> Result<Self, SessionStoreError> {
        let db = stoolap::Database::open_in_memory().map_err(stoolap_err)?;
        let store = Self { db };
        crate::schema::init_schema(&store.db)?;
        Ok(store)
    }
}

/// Current epoch seconds. Returns 0 if the clock is before the
/// Unix epoch (defensive — never expected in practice).
fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Convert a `stoolap::ResultRow` to a `SessionRow`. Returns
/// `SessionStoreError::Stoolap` on column-mismatch errors.
fn row_to_session(row: &stoolap::ResultRow) -> Result<SessionRow, SessionStoreError> {
    let login_type_str: String = row
        .get_by_name("login_type")
        .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?;
    let login_type: LoginType = login_type_str.parse().map_err(SessionStoreError::Stoolap)?;

    Ok(SessionRow {
        user_id: row
            .get_by_name("user_id")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        device_id: row
            .get_by_name("device_id")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        homeserver_url: row
            .get_by_name("homeserver_url")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        access_token: row
            .get_by_name("access_token")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        refresh_token: row
            .get_by_name("refresh_token")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        login_type,
        login_timestamp: row
            .get_by_name("login_timestamp")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        last_used: row
            .get_by_name("last_used")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        position: row
            .get_by_name("position")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        display_name: row
            .get_by_name("display_name")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
        avatar_url: row
            .get_by_name("avatar_url")
            .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?,
    })
}

#[async_trait]
impl SessionStore for StoolapSessionStore {
    async fn add_session(&self, row: &SessionRow, force: bool) -> Result<(), SessionStoreError> {
        // Check for an existing row. With force=true, drop it first
        // (the (user_id, device_id) primary key would block the
        // re-insert otherwise).
        let existing = self.get_session(&row.user_id, &row.device_id).await?;
        if existing.is_some() {
            if !force {
                return Err(SessionStoreError::AlreadyExists {
                    user_id: row.user_id.clone(),
                    device_id: row.device_id.clone(),
                });
            }
            // With force=true, delete the existing row before re-inserting.
            // The new row gets a fresh login_timestamp and the next
            // monotonic position.
            let del_params: Vec<stoolap::Value> =
                vec![row.user_id.clone().into(), row.device_id.clone().into()];
            self.db
                .execute(
                    "DELETE FROM sessions WHERE user_id = $1 AND device_id = $2",
                    del_params,
                )
                .map_err(stoolap_err)?;
        }

        // Compute position = max(position) + 1 (or 1 if empty).
        let next_position: i64 = {
            let mut rows = self
                .db
                .query(
                    "SELECT COALESCE(MAX(position), 0) + 1 AS next FROM sessions",
                    [],
                )
                .map_err(stoolap_err)?;
            if let Some(Ok(r)) = rows.next() {
                r.get_by_name("next")
                    .map_err(|e| SessionStoreError::Stoolap(e.to_string()))?
            } else {
                1
            }
        };

        let now = now_epoch();
        let params: Vec<stoolap::Value> = vec![
            row.user_id.clone().into(),
            row.device_id.clone().into(),
            row.homeserver_url.clone().into(),
            row.access_token.clone().into(),
            row.refresh_token.clone().into(),
            row.login_type.as_str().to_string().into(),
            now.into(),
            now.into(),
            next_position.into(),
            row.display_name.clone().into(),
            row.avatar_url.clone().into(),
        ];
        self.db
            .execute(
                "INSERT INTO sessions (
                    user_id, device_id, homeserver_url, access_token,
                    refresh_token, login_type, login_timestamp, last_used,
                    position, display_name, avatar_url
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                params,
            )
            .map_err(stoolap_err)?;
        Ok(())
    }

    async fn update_data(&self, row: &SessionRow) -> Result<(), SessionStoreError> {
        // Confirm the row exists; update_data is not an upsert.
        if self
            .get_session(&row.user_id, &row.device_id)
            .await?
            .is_none()
        {
            return Err(SessionStoreError::NotFound {
                user_id: row.user_id.clone(),
                device_id: row.device_id.clone(),
            });
        }
        let params: Vec<stoolap::Value> = vec![
            row.access_token.clone().into(),
            row.refresh_token.clone().into(),
            row.display_name.clone().into(),
            row.avatar_url.clone().into(),
            row.user_id.clone().into(),
            row.device_id.clone().into(),
        ];
        self.db
            .execute(
                "UPDATE sessions SET
                    access_token = $1,
                    refresh_token = $2,
                    display_name = $3,
                    avatar_url = $4
                 WHERE user_id = $5 AND device_id = $6",
                params,
            )
            .map_err(stoolap_err)?;
        Ok(())
    }

    async fn get_session(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<SessionRow>, SessionStoreError> {
        let params: Vec<stoolap::Value> =
            vec![user_id.to_string().into(), device_id.to_string().into()];
        let mut rows = self
            .db
            .query(
                "SELECT user_id, device_id, homeserver_url, access_token,
                        refresh_token, login_type, login_timestamp, last_used,
                        position, display_name, avatar_url
                 FROM sessions WHERE user_id = $1 AND device_id = $2 LIMIT 1",
                params,
            )
            .map_err(stoolap_err)?;
        if let Some(Ok(row)) = rows.next() {
            Ok(Some(row_to_session(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn get_all_sessions(&self) -> Result<Vec<SessionRow>, SessionStoreError> {
        let mut rows = self
            .db
            .query(
                "SELECT user_id, device_id, homeserver_url, access_token,
                        refresh_token, login_type, login_timestamp, last_used,
                        position, display_name, avatar_url
                 FROM sessions ORDER BY position ASC",
                [],
            )
            .map_err(stoolap_err)?;
        let mut out = Vec::new();
        while let Some(Ok(row)) = rows.next() {
            out.push(row_to_session(&row)?);
        }
        Ok(out)
    }

    async fn get_latest_session(&self) -> Result<Option<SessionRow>, SessionStoreError> {
        let mut rows = self
            .db
            .query(
                "SELECT user_id, device_id, homeserver_url, access_token,
                        refresh_token, login_type, login_timestamp, last_used,
                        position, display_name, avatar_url
                 FROM sessions ORDER BY last_used DESC LIMIT 1",
                [],
            )
            .map_err(stoolap_err)?;
        if let Some(Ok(row)) = rows.next() {
            Ok(Some(row_to_session(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn number_of_sessions(&self) -> Result<i64, SessionStoreError> {
        let mut rows = self
            .db
            .query("SELECT COUNT(*) AS n FROM sessions", [])
            .map_err(stoolap_err)?;
        if let Some(Ok(row)) = rows.next() {
            row.get_by_name("n")
                .map_err(|e| SessionStoreError::Stoolap(e.to_string()))
        } else {
            Ok(0)
        }
    }

    async fn set_latest_session(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), SessionStoreError> {
        let now = now_epoch();
        if self.get_session(user_id, device_id).await?.is_none() {
            return Err(SessionStoreError::NotFound {
                user_id: user_id.to_string(),
                device_id: device_id.to_string(),
            });
        }
        let params: Vec<stoolap::Value> = vec![
            now.into(),
            user_id.to_string().into(),
            device_id.to_string().into(),
        ];
        self.db
            .execute(
                "UPDATE sessions SET last_used = $1
                 WHERE user_id = $2 AND device_id = $3",
                params,
            )
            .map_err(stoolap_err)?;
        Ok(())
    }

    async fn remove_session(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), SessionStoreError> {
        if self.get_session(user_id, device_id).await?.is_none() {
            return Err(SessionStoreError::NotFound {
                user_id: user_id.to_string(),
                device_id: device_id.to_string(),
            });
        }
        let params: Vec<stoolap::Value> =
            vec![user_id.to_string().into(), device_id.to_string().into()];
        self.db
            .execute(
                "DELETE FROM sessions WHERE user_id = $1 AND device_id = $2",
                params,
            )
            .map_err(stoolap_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_store() -> StoolapSessionStore {
        StoolapSessionStore::new_in_memory().expect("open in-memory store")
    }

    fn row(user: &str, device: &str) -> SessionRow {
        SessionRow {
            user_id: user.to_string(),
            device_id: device.to_string(),
            homeserver_url: "https://matrix.example.com".to_string(),
            access_token: format!("syt_{}", user),
            refresh_token: Some(format!("syr_{}", user)),
            login_type: LoginType::Password,
            login_timestamp: 0,
            last_used: 0,
            position: 0,
            display_name: None,
            avatar_url: None,
        }
    }

    #[tokio::test]
    async fn add_and_get_session() {
        let store = fresh_store().await;
        let r = row("@a:example.com", "DEV1");
        store.add_session(&r, false).await.unwrap();
        let got = store.get_session("@a:example.com", "DEV1").await.unwrap();
        // The store overwrites login_timestamp / last_used / position
        // on insert, so we compare just the user-supplied fields.
        let got = got.unwrap();
        assert_eq!(got.user_id, r.user_id);
        assert_eq!(got.device_id, r.device_id);
        assert_eq!(got.homeserver_url, r.homeserver_url);
        assert_eq!(got.access_token, r.access_token);
        assert_eq!(got.refresh_token, r.refresh_token);
        assert_eq!(got.login_type, r.login_type);
        assert_eq!(got.display_name, r.display_name);
        assert_eq!(got.avatar_url, r.avatar_url);
        // Store-managed fields are non-zero after insert.
        assert!(got.login_timestamp > 0);
        assert!(got.last_used > 0);
        assert_eq!(got.position, 1);
    }

    #[tokio::test]
    async fn add_duplicate_refuses_without_force() {
        let store = fresh_store().await;
        let r = row("@a:example.com", "DEV1");
        store.add_session(&r, false).await.unwrap();
        let r2 = row("@a:example.com", "DEV1");
        let err = store.add_session(&r2, false).await.unwrap_err();
        assert!(matches!(err, SessionStoreError::AlreadyExists { .. }));
    }

    #[tokio::test]
    async fn add_duplicate_with_force_overwrites() {
        let store = fresh_store().await;
        let r = row("@a:example.com", "DEV1");
        store.add_session(&r, false).await.unwrap();
        let r2 = SessionRow {
            access_token: "syt_NEW".to_string(),
            ..r.clone()
        };
        store.add_session(&r2, true).await.unwrap();
        let got = store.get_session("@a:example.com", "DEV1").await.unwrap();
        assert_eq!(got.unwrap().access_token, "syt_NEW");
    }

    #[tokio::test]
    async fn position_is_strictly_monotonic() {
        let store = fresh_store().await;
        store
            .add_session(&row("@a:example.com", "DEV1"), false)
            .await
            .unwrap();
        store
            .add_session(&row("@b:example.com", "DEV1"), false)
            .await
            .unwrap();
        store
            .add_session(&row("@c:example.com", "DEV1"), false)
            .await
            .unwrap();
        let sessions = store.get_all_sessions().await.unwrap();
        assert_eq!(sessions[0].position, 1);
        assert_eq!(sessions[1].position, 2);
        assert_eq!(sessions[2].position, 3);
    }

    #[tokio::test]
    async fn set_latest_does_not_change_position() {
        let store = fresh_store().await;
        store
            .add_session(&row("@a:example.com", "DEV1"), false)
            .await
            .unwrap();
        store
            .add_session(&row("@b:example.com", "DEV1"), false)
            .await
            .unwrap();
        // Mark the older session as latest; position should stay.
        store
            .set_latest_session("@a:example.com", "DEV1")
            .await
            .unwrap();
        let sessions = store.get_all_sessions().await.unwrap();
        let a = sessions
            .iter()
            .find(|r| r.user_id == "@a:example.com")
            .unwrap();
        let b = sessions
            .iter()
            .find(|r| r.user_id == "@b:example.com")
            .unwrap();
        // a still has position 1, b still has position 2.
        assert_eq!(a.position, 1);
        assert_eq!(b.position, 2);
    }

    #[tokio::test]
    async fn login_timestamp_immutable_after_insert() {
        let store = fresh_store().await;
        let r = row("@a:example.com", "DEV1");
        let before = now_epoch();
        store.add_session(&r, false).await.unwrap();
        let after_insert = store
            .get_session("@a:example.com", "DEV1")
            .await
            .unwrap()
            .unwrap();
        let first_ts = after_insert.login_timestamp;
        assert!(
            first_ts >= before,
            "login_timestamp should be >= clock before insert"
        );
        // A set_latest_session updates last_used, not login_timestamp.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        store
            .set_latest_session("@a:example.com", "DEV1")
            .await
            .unwrap();
        let after_latest = store
            .get_session("@a:example.com", "DEV1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after_latest.login_timestamp, first_ts);
        assert!(after_latest.last_used >= first_ts);
    }

    #[tokio::test]
    async fn number_of_sessions_counts_correctly() {
        let store = fresh_store().await;
        assert_eq!(store.number_of_sessions().await.unwrap(), 0);
        store
            .add_session(&row("@a:example.com", "DEV1"), false)
            .await
            .unwrap();
        store
            .add_session(&row("@b:example.com", "DEV1"), false)
            .await
            .unwrap();
        assert_eq!(store.number_of_sessions().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn remove_session_returns_not_found_when_absent() {
        let store = fresh_store().await;
        let err = store
            .remove_session("@a:example.com", "DEV1")
            .await
            .unwrap_err();
        assert!(matches!(err, SessionStoreError::NotFound { .. }));
    }

    #[tokio::test]
    async fn remove_session_drops_row() {
        let store = fresh_store().await;
        store
            .add_session(&row("@a:example.com", "DEV1"), false)
            .await
            .unwrap();
        store
            .remove_session("@a:example.com", "DEV1")
            .await
            .unwrap();
        assert!(store
            .get_session("@a:example.com", "DEV1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_latest_returns_most_recently_used() {
        let store = fresh_store().await;
        store
            .add_session(&row("@a:example.com", "DEV1"), false)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        store
            .add_session(&row("@b:example.com", "DEV1"), false)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        store
            .set_latest_session("@a:example.com", "DEV1")
            .await
            .unwrap();
        let latest = store.get_latest_session().await.unwrap().unwrap();
        assert_eq!(latest.user_id, "@a:example.com");
    }

    #[tokio::test]
    async fn update_data_preserves_position_and_login_timestamp() {
        let store = fresh_store().await;
        store
            .add_session(&row("@a:example.com", "DEV1"), false)
            .await
            .unwrap();
        let before = store
            .get_session("@a:example.com", "DEV1")
            .await
            .unwrap()
            .unwrap();
        let mut updated = before.clone();
        updated.access_token = "syt_NEW".to_string();
        updated.display_name = Some("Alice".to_string());
        store.update_data(&updated).await.unwrap();
        let after = store
            .get_session("@a:example.com", "DEV1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.access_token, "syt_NEW");
        assert_eq!(after.display_name.as_deref(), Some("Alice"));
        assert_eq!(after.position, before.position);
        assert_eq!(after.login_timestamp, before.login_timestamp);
    }
}
