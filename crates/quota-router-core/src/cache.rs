// Cache module for handling invalidation events from WAL pub/sub
//
// L1 CACHE DETERMINISM DISCLAIMER:
// The L1 key cache is NOT part of the accounting/budget enforcement path.
// It is purely for performance (reducing DB lookups). Cache misses are
// handled gracefully by falling back to DB lookup. Budget enforcement
// happens atomically in record_spend_ledger() at the storage layer.

use crate::keys::{ApiKey, KeyError};
use crate::storage::KeyStorage;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// L1 key cache with LRU eviction and TTL-based expiration.
///
/// Uses Arc<ApiKey> to avoid cloning on cache hits.
pub struct KeyCache {
    cache: Arc<RwLock<lru::LruCache<Vec<u8>, CacheEntry>>>,
    ttl_secs: u64,
}

/// Cache entry wrapping ApiKey with metadata for TTL tracking.
struct CacheEntry {
    api_key: Arc<ApiKey>,
    cached_at: Instant,
}

impl CacheEntry {
    fn new(api_key: ApiKey) -> Self {
        Self {
            api_key: Arc::new(api_key),
            cached_at: Instant::now(),
        }
    }

    fn is_expired(&self, ttl_secs: u64) -> bool {
        self.cached_at.elapsed() > Duration::from_secs(ttl_secs)
    }
}

/// Cache configuration constants per RFC-0903 §L1 Cache for Fast Lookups
pub const CACHE_SIZE: usize = 10_000;
pub const CACHE_TTL_SECS: u64 = 30;

impl KeyCache {
    /// Create a new KeyCache with default configuration.
    pub fn new() -> Self {
        Self::with_capacity_and_ttl(CACHE_SIZE, CACHE_TTL_SECS)
    }

    /// Create a KeyCache with custom capacity and TTL.
    pub fn with_capacity_and_ttl(capacity: usize, ttl_secs: u64) -> Self {
        use std::num::NonZero;
        Self {
            cache: Arc::new(RwLock::new(lru::LruCache::new(
                NonZero::new(capacity).unwrap(),
            ))),
            ttl_secs,
        }
    }

    /// Get a key from cache if present and not expired.
    ///
    /// Returns `Option<Arc<ApiKey>>` - Arc avoids cloning.
    pub async fn get(&self, key_hash: &[u8]) -> Option<Arc<ApiKey>> {
        let mut cache = self.cache.write().await;
        let entry = cache.get_mut(key_hash)?;

        if entry.is_expired(self.ttl_secs) {
            cache.pop(key_hash);
            return None;
        }

        Some(entry.api_key.clone())
    }

    /// Put a key into the cache.
    ///
    /// Wraps ApiKey in Arc to avoid cloning.
    pub async fn put(&self, key_hash: Vec<u8>, api_key: ApiKey) {
        let mut cache = self.cache.write().await;
        cache.put(key_hash, CacheEntry::new(api_key));
    }

    /// Invalidate (remove) a key from the cache.
    pub async fn invalidate(&self, key_hash: &[u8]) {
        let mut cache = self.cache.write().await;
        cache.pop(key_hash);
    }

    /// Clear all entries from the cache.
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get current number of entries in cache.
    pub async fn len(&self) -> usize {
        self.cache.read().await.len()
    }

    /// Check if cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.cache.read().await.is_empty()
    }
}

impl Default for KeyCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a key with L1 cache optimization.
///
/// Flow: Check cache (TTL) → On miss: DB lookup → Validate → Add to cache → Return Arc<ApiKey>
///
/// Returns `Arc<ApiKey>` to avoid cloning on cache hits.
pub async fn validate_key_with_cache(
    db: &stoolap::Database,
    cache: &KeyCache,
    key: &str,
) -> Result<Arc<ApiKey>, KeyError> {
    use crate::keys::compute_key_hash;

    let key_hash = compute_key_hash(key);

    // Check cache first
    if let Some(cached) = cache.get(&key_hash).await {
        // Validate expiry/revoked (cheap check)
        crate::keys::validate_key(&cached)?;
        return Ok(cached);
    }

    // Cache miss - lookup in DB
    let key_hash_blob = stoolap::core::Value::blob(key_hash.to_vec());
    let mut rows = db
        .query(
            "SELECT * FROM api_keys WHERE key_hash = $1 AND revoked = 0 LIMIT 1",
            vec![key_hash_blob],
        )
        .map_err(|e| KeyError::Storage(e.to_string()))?;

    let row = rows
        .next()
        .ok_or(KeyError::NotFound)?
        .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Parse row into ApiKey using StoolapKeyStorage helper
    let storage = crate::storage::StoolapKeyStorage::new(db.clone());
    let api_key = storage
        .row_to_api_key(&row)
        .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Validate (expiry, revoked)
    crate::keys::validate_key(&api_key)?;

    // Add to cache
    cache.put(key_hash.to_vec(), api_key.clone()).await;

    Ok(Arc::new(api_key))
}

/// Check budget without locking (soft pre-flight check).
///
/// This is a non-locking check for UX improvement. It computes current spend
/// from the ledger and checks if estimated_max_cost would exceed budget.
///
/// Returns `Ok(())` if under budget, `Err(KeyError::BudgetExceeded)` if would exceed.
///
/// Note: The authoritative check happens atomically in `record_spend_ledger()`.
pub fn check_budget_soft_limit(
    db: &stoolap::Database,
    key_id: &str,
    estimated_max_cost: u64,
) -> Result<(), KeyError> {
    let key_id_value: Vec<stoolap::Value> = vec![key_id.into()];

    // Get key budget
    let mut key_rows = db
        .query(
            "SELECT budget_limit FROM api_keys WHERE key_id = $1",
            key_id_value.clone(),
        )
        .map_err(|e| KeyError::Storage(e.to_string()))?;

    let key_budget: i64 = key_rows
        .next()
        .ok_or(KeyError::NotFound)?
        .map_err(|e| KeyError::Storage(e.to_string()))?
        .get(0)
        .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Compute current spend from ledger
    let mut spend_rows = db
        .query(
            "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
            key_id_value,
        )
        .map_err(|e| KeyError::Storage(e.to_string()))?;

    let current: i64 = spend_rows
        .next()
        .ok_or(KeyError::Storage("Expected row".to_string()))?
        .map_err(|e| KeyError::Storage(e.to_string()))?
        .get(0)
        .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Check if estimated would exceed
    if current + estimated_max_cost as i64 > key_budget {
        return Err(KeyError::BudgetExceeded {
            current: current as u64,
            limit: key_budget as u64,
        });
    }

    Ok(())
}

/// Background worker for automatic key rotation.
///
/// Runs every `interval` and rotates keys where:
/// - `auto_rotate = 1` AND `expires_at < now`
///
/// Logs failures but continues processing other keys.
pub async fn rotation_worker(db: &stoolap::Database, cache: &KeyCache, interval_secs: u64) {
    use crate::keys::generate_key_id;
    use crate::keys::generate_key_string;

    let interval = Duration::from_secs(interval_secs);

    loop {
        tokio::time::sleep(interval).await;

        tracing::debug!("Running key rotation worker...");

        // Find keys to rotate
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let params: Vec<stoolap::Value> = vec![now.into()];
        let rows = match db.query(
            "SELECT * FROM api_keys WHERE auto_rotate = 1 AND expires_at < $1 AND revoked = 0",
            params,
        ) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("rotation_worker: failed to query keys: {}", e);
                continue;
            }
        };

        for row in rows {
            let row = match row {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("rotation_worker: failed to read row: {}", e);
                    continue;
                }
            };

            let storage = crate::storage::StoolapKeyStorage::new(db.clone());
            let old_key = match storage.row_to_api_key(&row) {
                Ok(k) => k,
                Err(e) => {
                    tracing::error!("rotation_worker: failed to parse key: {}", e);
                    continue;
                }
            };

            // Generate new key
            let new_key_string = generate_key_string();
            let new_key_id = generate_key_id();
            let new_key_hash = crate::keys::compute_key_hash(&new_key_string);

            // Create new key with same settings
            let new_key = ApiKey {
                key_id: new_key_id,
                key_hash: new_key_hash.to_vec(),
                key_prefix: new_key_string.chars().take(7).collect(),
                team_id: old_key.team_id.clone(),
                budget_limit: old_key.budget_limit,
                rpm_limit: old_key.rpm_limit,
                tpm_limit: old_key.tpm_limit,
                created_at: now,
                expires_at: old_key
                    .expires_at
                    .map(|e| e + old_key.rotation_interval_days.unwrap_or(30) as i64 * 86400),
                revoked: false,
                revoked_at: None,
                revoked_by: None,
                revocation_reason: Some("Auto-rotated".to_string()),
                key_type: old_key.key_type,
                allowed_routes: old_key.allowed_routes.clone(),
                auto_rotate: old_key.auto_rotate,
                rotation_interval_days: old_key.rotation_interval_days,
                description: old_key.description.clone(),
                metadata: old_key.metadata.clone(),
            };

            // Revoke old key
            if let Err(e) = storage.update_key(
                &old_key.key_id,
                &crate::keys::KeyUpdates {
                    revoked: Some(true),
                    revocation_reason: Some("Auto-rotated".to_string()),
                    budget_limit: None,
                    rpm_limit: None,
                    tpm_limit: None,
                    expires_at: None,
                    revoked_by: None,
                    key_type: None,
                    description: None,
                },
            ) {
                tracing::error!(
                    "rotation_worker: failed to revoke key {}: {}",
                    old_key.key_id,
                    e
                );
                continue;
            }

            // Create new key
            if let Err(e) = storage.create_key(&new_key) {
                tracing::error!(
                    "rotation_worker: failed to create new key for {}: {}",
                    old_key.key_id,
                    e
                );
                continue;
            }

            // Invalidate old key from cache
            cache.invalidate(&old_key.key_hash).await;

            tracing::info!(
                "rotation_worker: rotated key {} -> {}",
                old_key.key_id,
                new_key.key_id
            );
        }
    }
}

// Cache invalidation handler for WAL pub/sub events (legacy)
pub struct CacheInvalidation;

impl CacheInvalidation {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CacheInvalidation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_key_cache_basic() {
        let cache = KeyCache::new();

        let key = crate::keys::ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![1, 2, 3],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: Some(100),
            tpm_limit: Some(1000),
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: crate::keys::KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        let key_hash = vec![1, 2, 3];

        assert!(cache.get(&key_hash).await.is_none());
        cache.put(key_hash.clone(), key.clone()).await;
        assert!(cache.get(&key_hash).await.is_some());
        cache.invalidate(&key_hash).await;
        assert!(cache.get(&key_hash).await.is_none());
    }

    #[tokio::test]
    async fn test_key_cache_ttl_expiry() {
        let cache = KeyCache::with_capacity_and_ttl(100, 0);

        let key = crate::keys::ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![4, 5, 6],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: crate::keys::KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        let key_hash = vec![4, 5, 6];
        cache.put(key_hash.clone(), key).await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(cache.get(&key_hash).await.is_none());
    }

    #[tokio::test]
    async fn test_key_cache_clear() {
        let cache = KeyCache::new();

        let key = crate::keys::ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![7, 8, 9],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: crate::keys::KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        let key_hash = vec![7, 8, 9];
        cache.put(key_hash.clone(), key).await;
        assert!(!cache.is_empty().await);
        cache.clear().await;
        assert!(cache.is_empty().await);
    }

    #[test]
    fn test_cache_invalidation() {
        let _ci = CacheInvalidation::new();
    }
}
