//! Production TokenBlacklistStorage via stoolap (RFC-0949 Phase 5).
//!
//! Provides persistent, cross-instance token revocation by storing revoked
//! token IDs in a shared SQLite/PostgreSQL database.

use super::{SsoError, TokenBlacklistStorage};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Production token blacklist storage backed by stoolap.
pub struct StoolapTokenBlacklistStorage {
    db: stoolap::Database,
}

impl StoolapTokenBlacklistStorage {
    /// Create a new StoolapTokenBlacklistStorage.
    pub fn new(db: stoolap::Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl TokenBlacklistStorage for StoolapTokenBlacklistStorage {
    /// Add a token to the blacklist with expiration time.
    async fn add(&self, token_id: &str, expires_at: DateTime<Utc>) -> Result<(), SsoError> {
        let now = Utc::now().timestamp();
        // Try INSERT first; on conflict (duplicate token_id), UPDATE expires_at
        let result = self.db.execute(
            "INSERT INTO token_blacklist (token_id, expires_at, created_at) VALUES ($1, $2, $3)",
            vec![token_id.into(), expires_at.timestamp().into(), now.into()],
        );
        match result {
            Ok(_) => Ok(()),
            Err(stoolap::Error::UniqueConstraint { .. }) => {
                // Token already blacklisted — update expiration
                self.db
                    .execute(
                        "UPDATE token_blacklist SET expires_at = $1 WHERE token_id = $2",
                        vec![expires_at.timestamp().into(), token_id.into()],
                    )
                    .map_err(|e| {
                        SsoError::ProviderError(format!("blacklist update failed: {}", e))
                    })?;
                Ok(())
            }
            Err(e) => Err(SsoError::ProviderError(format!(
                "blacklist insert failed: {}",
                e
            ))),
        }
    }

    /// Check if a token is in the blacklist (and not expired).
    async fn contains(&self, token_id: &str) -> Result<bool, SsoError> {
        let now = Utc::now().timestamp();
        let mut rows = self
            .db
            .query(
                "SELECT 1 FROM token_blacklist WHERE token_id = $1 AND expires_at > $2",
                vec![token_id.into(), now.into()],
            )
            .map_err(|e| SsoError::ProviderError(format!("blacklist query failed: {}", e)))?;
        Ok(rows.next().is_some())
    }

    /// Remove all expired entries. Returns count of removed entries.
    async fn cleanup_expired(&self) -> Result<u64, SsoError> {
        let now = Utc::now().timestamp();
        let rows_affected = self
            .db
            .execute(
                "DELETE FROM token_blacklist WHERE expires_at <= $1",
                vec![now.into()],
            )
            .map_err(|e| SsoError::ProviderError(format!("blacklist cleanup failed: {}", e)))?;
        Ok(rows_affected as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[tokio::test]
    async fn test_blacklist_add_and_contains() {
        let db = stoolap::Database::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE token_blacklist (token_id TEXT NOT NULL, expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL)",
            (),
        )
        .unwrap();
        let storage = StoolapTokenBlacklistStorage::new(db);

        let expires = Utc::now() + Duration::hours(1);
        storage.add("token-123", expires).await.unwrap();

        assert!(storage.contains("token-123").await.unwrap());
        assert!(!storage.contains("unknown-token").await.unwrap());
    }

    #[tokio::test]
    async fn test_blacklist_expired_not_contained() {
        let db = stoolap::Database::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE token_blacklist (token_id TEXT NOT NULL, expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL)",
            (),
        )
        .unwrap();
        let storage = StoolapTokenBlacklistStorage::new(db);

        // Add an already-expired token
        let expired = Utc::now() - Duration::hours(1);
        storage.add("expired-token", expired).await.unwrap();

        // Should not be contained because it's expired
        assert!(!storage.contains("expired-token").await.unwrap());
    }

    #[tokio::test]
    async fn test_blacklist_cleanup() {
        let db = stoolap::Database::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE token_blacklist (token_id TEXT NOT NULL, expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL)",
            (),
        )
        .unwrap();
        let storage = StoolapTokenBlacklistStorage::new(db);

        // Add expired token
        let expired = Utc::now() - Duration::hours(1);
        storage.add("expired-token", expired).await.unwrap();

        // Add valid token
        let valid = Utc::now() + Duration::hours(1);
        storage.add("valid-token", valid).await.unwrap();

        let cleaned = storage.cleanup_expired().await.unwrap();
        assert_eq!(cleaned, 1);
        assert!(!storage.contains("expired-token").await.unwrap());
        assert!(storage.contains("valid-token").await.unwrap());
    }
}
