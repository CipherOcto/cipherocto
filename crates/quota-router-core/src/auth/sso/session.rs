//! Session management for SSO (RFC-0949).
//!
//! Provides sliding-window session lifecycle with automatic expiry.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Session timeout: 30 minutes idle.
const SESSION_IDLE_TIMEOUT_MINUTES: i64 = 30;

/// An active SSO session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID (UUID)
    pub id: String,
    /// SSO user ID (subject claim)
    pub user_id: String,
    /// Identity provider that issued the session
    pub provider: String,
    /// When the session was created
    pub created_at: DateTime<Utc>,
    /// When the session was last accessed (sliding window)
    pub last_access: DateTime<Utc>,
    /// Absolute expiry (created_at + max lifetime)
    pub expires_at: DateTime<Utc>,
}

impl Session {
    /// Create a new session with the given idle timeout.
    pub fn new(id: String, user_id: String, provider: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            user_id,
            provider,
            created_at: now,
            last_access: now,
            expires_at: now + Duration::hours(8),
        }
    }

    /// Check if the session is expired (idle timeout or absolute expiry).
    pub fn is_expired(&self) -> bool {
        let now = Utc::now();
        let idle_expiry = self.last_access + Duration::minutes(SESSION_IDLE_TIMEOUT_MINUTES);
        now > idle_expiry || now > self.expires_at
    }

    /// Refresh the session's idle timer (sliding window).
    pub fn refresh(&mut self) {
        self.last_access = Utc::now();
    }
}

/// Trait for session storage backends.
pub trait SessionStorage: Send + Sync {
    /// Create a new session.
    fn create(&self, session: Session) -> Result<(), String>;

    /// Get a session by ID.
    fn get(&self, session_id: &str) -> Option<Session>;

    /// Refresh a session's idle timer.
    fn refresh(&self, session_id: &str) -> Result<(), String>;

    /// Revoke (delete) a session.
    fn revoke(&self, session_id: &str) -> Result<(), String>;

    /// Remove all expired sessions.
    fn cleanup_expired(&self) -> usize;

    /// List all active sessions for a user.
    fn list_for_user(&self, user_id: &str) -> Vec<Session>;
}

/// In-memory session storage.
pub struct InMemorySessionStorage {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl InMemorySessionStorage {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStorage for InMemorySessionStorage {
    fn create(&self, session: Session) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        sessions.insert(session.id.clone(), session);
        Ok(())
    }

    fn get(&self, session_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().ok()?;
        let session = sessions.get(session_id)?;
        if session.is_expired() {
            return None;
        }
        Some(session.clone())
    }

    fn refresh(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        if let Some(session) = sessions.get_mut(session_id) {
            if session.is_expired() {
                sessions.remove(session_id);
                return Err("Session expired".to_string());
            }
            session.refresh();
            Ok(())
        } else {
            Err("Session not found".to_string())
        }
    }

    fn revoke(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        sessions.remove(session_id);
        Ok(())
    }

    fn cleanup_expired(&self) -> usize {
        let mut sessions = match self.sessions.write() {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let before = sessions.len();
        sessions.retain(|_, s| !s.is_expired());
        before - sessions.len()
    }

    fn list_for_user(&self, user_id: &str) -> Vec<Session> {
        let sessions = match self.sessions.read() {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        sessions
            .values()
            .filter(|s| s.user_id == user_id && !s.is_expired())
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_not_expired_after_create() {
        let session = Session::new("s1".into(), "u1".into(), "okta".into());
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_refresh() {
        let mut session = Session::new("s1".into(), "u1".into(), "okta".into());
        let original = session.last_access;
        // Simulate time passing by setting last_access far in the past
        session.last_access = Utc::now() - Duration::minutes(29);
        session.refresh();
        assert!(session.last_access > original);
    }

    #[test]
    fn test_in_memory_storage_create_get() {
        let storage = InMemorySessionStorage::new();
        let session = Session::new("s1".into(), "u1".into(), "okta".into());
        storage.create(session).unwrap();
        let got = storage.get("s1").unwrap();
        assert_eq!(got.user_id, "u1");
    }

    #[test]
    fn test_in_memory_storage_revoke() {
        let storage = InMemorySessionStorage::new();
        let session = Session::new("s1".into(), "u1".into(), "okta".into());
        storage.create(session).unwrap();
        storage.revoke("s1").unwrap();
        assert!(storage.get("s1").is_none());
    }

    #[test]
    fn test_in_memory_storage_list_for_user() {
        let storage = InMemorySessionStorage::new();
        storage
            .create(Session::new("s1".into(), "u1".into(), "okta".into()))
            .unwrap();
        storage
            .create(Session::new("s2".into(), "u1".into(), "okta".into()))
            .unwrap();
        storage
            .create(Session::new("s3".into(), "u2".into(), "okta".into()))
            .unwrap();
        let user1_sessions = storage.list_for_user("u1");
        assert_eq!(user1_sessions.len(), 2);
    }

    #[test]
    fn test_session_expired_after_idle() {
        let mut session = Session::new("s1".into(), "u1".into(), "okta".into());
        // Set last_access to 31 minutes ago (exceeds 30min idle timeout)
        session.last_access = Utc::now() - Duration::minutes(31);
        assert!(session.is_expired());
    }
}
