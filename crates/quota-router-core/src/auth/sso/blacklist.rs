//! Token Blacklist (RFC-0949)
//!
//! Token blacklist for cross-instance revocation using shared storage.

use super::SsoError;
use chrono::{DateTime, Utc};
use std::sync::Arc;

// ============================================================================
// TokenBlacklistStorage Trait
// ============================================================================

/// Trait for token blacklist storage backend
#[async_trait::async_trait]
pub trait TokenBlacklistStorage: Send + Sync {
    /// Add token to blacklist with expiration
    async fn add(&self, token_id: &str, expires_at: DateTime<Utc>) -> Result<(), SsoError>;
    /// Check if token is blacklisted
    async fn contains(&self, token_id: &str) -> Result<bool, SsoError>;
    /// Remove expired entries (background cleanup)
    async fn cleanup_expired(&self) -> Result<u64, SsoError>;
}

// ============================================================================
// TokenBlacklist
// ============================================================================

pub struct TokenBlacklist {
    storage: Arc<dyn TokenBlacklistStorage>,
}

impl TokenBlacklist {
    pub fn new(storage: Arc<dyn TokenBlacklistStorage>) -> Self {
        Self { storage }
    }

    /// Revoke a token
    pub async fn revoke(&self, token_id: &str, expires_at: DateTime<Utc>) -> Result<(), SsoError> {
        self.storage.add(token_id, expires_at).await
    }

    /// Check if a token is revoked
    pub async fn is_revoked(&self, token_id: &str) -> Result<bool, SsoError> {
        self.storage.contains(token_id).await
    }

    /// Run cleanup of expired entries
    pub async fn cleanup(&self) -> Result<u64, SsoError> {
        self.storage.cleanup_expired().await
    }
}

// ============================================================================
// In-Memory Implementation (for testing)
// ============================================================================

pub struct InMemoryBlacklistStorage {
    entries: std::sync::RwLock<std::collections::HashMap<String, DateTime<Utc>>>,
}

impl InMemoryBlacklistStorage {
    pub fn new() -> Self {
        Self {
            entries: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryBlacklistStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TokenBlacklistStorage for InMemoryBlacklistStorage {
    async fn add(&self, token_id: &str, expires_at: DateTime<Utc>) -> Result<(), SsoError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| SsoError::ProviderError(format!("lock error: {}", e)))?;
        entries.insert(token_id.to_string(), expires_at);
        Ok(())
    }

    async fn contains(&self, token_id: &str) -> Result<bool, SsoError> {
        let entries = self
            .entries
            .read()
            .map_err(|e| SsoError::ProviderError(format!("lock error: {}", e)))?;
        Ok(entries.contains_key(token_id))
    }

    async fn cleanup_expired(&self) -> Result<u64, SsoError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| SsoError::ProviderError(format!("lock error: {}", e)))?;
        let now = Utc::now();
        let before = entries.len();
        entries.retain(|_, expires_at| *expires_at > now);
        Ok((before - entries.len()) as u64)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[tokio::test]
    async fn test_blacklist_add_contains() {
        let storage = Arc::new(InMemoryBlacklistStorage::new());
        let blacklist = TokenBlacklist::new(storage);

        let expires_at = Utc::now() + Duration::hours(1);
        blacklist.revoke("token-123", expires_at).await.unwrap();

        assert!(blacklist.is_revoked("token-123").await.unwrap());
        assert!(!blacklist.is_revoked("token-456").await.unwrap());
    }

    #[tokio::test]
    async fn test_blacklist_cleanup() {
        let storage = Arc::new(InMemoryBlacklistStorage::new());
        let blacklist = TokenBlacklist::new(storage);

        // Add expired token
        let expired = Utc::now() - Duration::hours(1);
        blacklist.revoke("expired-token", expired).await.unwrap();

        // Add valid token
        let valid = Utc::now() + Duration::hours(1);
        blacklist.revoke("valid-token", valid).await.unwrap();

        let cleaned = blacklist.cleanup().await.unwrap();
        assert_eq!(cleaned, 1);
        assert!(!blacklist.is_revoked("expired-token").await.unwrap());
        assert!(blacklist.is_revoked("valid-token").await.unwrap());
    }
}
