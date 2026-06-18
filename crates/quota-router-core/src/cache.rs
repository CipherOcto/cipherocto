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
                team_id: old_key.team_id,
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
                    metadata: None,
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

/// Cache invalidation handler with dual-write (broadcast + WAL).
///
/// Follows RFC-0913 architecture:
/// - Local EventBus for same-process cache invalidation
/// - WalPubSub for cross-process cache invalidation via shared WAL file
/// - Dual-write: every invalidation event goes to both EventBus and WAL
/// - Idempotency tracking via event_id deduplication
pub struct CacheInvalidation {
    event_bus: stoolap::pubsub::EventBus,
    wal_pubsub: Option<stoolap::pubsub::WalPubSub>,
    cache: KeyCache,
}

impl CacheInvalidation {
    /// Create a new CacheInvalidation with EventBus only (no WAL).
    pub fn new(cache: KeyCache) -> Self {
        Self {
            event_bus: stoolap::pubsub::EventBus::new(),
            wal_pubsub: None,
            cache,
        }
    }

    /// Create with both EventBus and WalPubSub (dual-write mode).
    pub fn with_wal(cache: KeyCache, wal_path: std::path::PathBuf) -> Self {
        Self {
            event_bus: stoolap::pubsub::EventBus::new(),
            wal_pubsub: Some(stoolap::pubsub::WalPubSub::new(wal_path)),
            cache,
        }
    }

    /// Publish a key invalidation event (dual-write: broadcast + WAL).
    ///
    /// Returns the event_id from the WAL write (canonical ID for idempotency).
    pub fn invalidate_key(
        &self,
        key_hash: Vec<u8>,
        reason: stoolap::pubsub::InvalidationReason,
        rpm_limit: Option<u32>,
        tpm_limit: Option<u32>,
    ) -> Result<[u8; 32], String> {
        // Placeholder event_id — will be replaced by WAL's computed ID
        let placeholder_id = [0u8; 32];
        let event = stoolap::pubsub::DatabaseEvent::KeyInvalidated {
            key_hash: key_hash.clone(),
            reason: reason.clone(),
            rpm_limit,
            tpm_limit,
            event_id: placeholder_id,
        };

        // Dual-write: WAL first (computes canonical event_id)
        let event_id = if let Some(ref wal) = self.wal_pubsub {
            let id = wal
                .write(&event)
                .map_err(|e| format!("WAL write failed: {}", e))?;
            // Re-publish to EventBus with correct event_id
            let event_with_id = stoolap::pubsub::DatabaseEvent::KeyInvalidated {
                key_hash,
                reason,
                rpm_limit,
                tpm_limit,
                event_id: id,
            };
            self.event_bus
                .publish(event_with_id)
                .map_err(|e| format!("EventBus publish failed: {}", e))?;
            id
        } else {
            // No WAL — just EventBus with generated ID
            let id = stoolap::pubsub::generate_event_id();
            let event_with_id = stoolap::pubsub::DatabaseEvent::KeyInvalidated {
                key_hash,
                reason,
                rpm_limit,
                tpm_limit,
                event_id: id,
            };
            self.event_bus
                .publish(event_with_id)
                .map_err(|e| format!("EventBus publish failed: {}", e))?;
            id
        };

        Ok(event_id)
    }

    /// Start background polling for WAL events.
    /// Returns a JoinHandle for the polling task.
    pub fn start_polling(
        wal_pubsub: stoolap::pubsub::WalPubSub,
        cache: KeyCache,
        interval_ms: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut last_lsn: u64 = 0;
            let interval = std::time::Duration::from_millis(interval_ms);

            loop {
                tokio::time::sleep(interval).await;

                match wal_pubsub.read_from_lsn(last_lsn) {
                    Ok(entries) => {
                        for entry in &entries {
                            // Skip duplicates
                            if wal_pubsub.idempotency().is_duplicate(entry.event_id) {
                                continue;
                            }

                            // Parse and handle event
                            if let Ok(event) =
                                stoolap::pubsub::wal_pubsub::parse_event(&entry.payload)
                            {
                                Self::handle_event(&cache, &event).await;
                                wal_pubsub.idempotency().mark_seen(entry.event_id);
                            }
                        }
                        last_lsn = wal_pubsub.current_lsn();
                    }
                    Err(e) => {
                        tracing::error!("WAL poll error: {}", e);
                    }
                }
            }
        })
    }

    /// Handle a single invalidation event by updating the cache.
    async fn handle_event(cache: &KeyCache, event: &stoolap::pubsub::DatabaseEvent) {
        match event {
            stoolap::pubsub::DatabaseEvent::KeyInvalidated { key_hash, .. } => {
                cache.invalidate(key_hash).await;
                tracing::debug!("Cache invalidated for key hash {:?}", key_hash);
            }
            stoolap::pubsub::DatabaseEvent::TableModified { .. }
            | stoolap::pubsub::DatabaseEvent::SchemaChanged { .. }
            | stoolap::pubsub::DatabaseEvent::TransactionCommited { .. } => {
                // Table/schema events not relevant for key cache
            }
        }
    }

    /// Get a reference to the EventBus for subscribing.
    pub fn event_bus(&self) -> &stoolap::pubsub::EventBus {
        &self.event_bus
    }

    /// Get a reference to the WalPubSub if configured.
    pub fn wal_pubsub(&self) -> Option<&stoolap::pubsub::WalPubSub> {
        self.wal_pubsub.as_ref()
    }

    /// Get a reference to the key cache.
    pub fn cache(&self) -> &KeyCache {
        &self.cache
    }
}

// =============================================================================
// Budget types (RFC-0914, Mission 0914-a)
// =============================================================================

/// Budget period for spend tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BudgetPeriod {
    Daily,
    Weekly,
    Monthly,
    Total,
}

impl BudgetPeriod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Total => "total",
        }
    }

    pub fn parse_period(s: &str) -> Option<Self> {
        match s {
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            "total" => Some(Self::Total),
            _ => None,
        }
    }

    /// Compute next reset timestamp from last_reset
    pub fn next_reset(&self, last_reset: i64) -> Option<i64> {
        match self {
            Self::Daily => Some(last_reset + 86400),
            Self::Weekly => Some(last_reset + 604800),
            Self::Monthly => Some(last_reset + 2592000), // 30 days
            Self::Total => None,                         // never resets
        }
    }
}

/// Entity type for budget and rate limit tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntityType {
    Key,
    User,
    Team,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::User => "user",
            Self::Team => "team",
        }
    }

    pub fn parse_entity(s: &str) -> Option<Self> {
        match s {
            "key" => Some(Self::Key),
            "user" => Some(Self::User),
            "team" => Some(Self::Team),
            _ => None,
        }
    }
}

// =============================================================================
// StoolapCache trait (RFC-0914, Mission 0914-a)
// =============================================================================

/// Generic cache interface for secret manager (RFC-0935).
/// Interim implementation uses in-memory HashMap.
/// Stoolap-backed implementation is a future phase.
#[async_trait::async_trait]
pub trait StoolapCache: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), String>;
    async fn delete(&self, key: &str) -> Result<(), String>;
}

/// In-memory cache implementation (interim until stoolap-backed).
pub struct InMemoryCache {
    entries: std::sync::RwLock<std::collections::HashMap<String, (String, Instant)>>,
}

impl InMemoryCache {
    pub fn new() -> Self {
        Self {
            entries: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl StoolapCache for InMemoryCache {
    async fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().unwrap();
        if let Some((value, cached_at)) = entries.get(key) {
            // TTL check is done by caller; return value if present
            let _ = cached_at; // stored for future TTL support
            Some(value.clone())
        } else {
            None
        }
    }

    async fn set(&self, key: &str, value: &str, _ttl_secs: u64) -> Result<(), String> {
        let mut entries = self.entries.write().unwrap();
        entries.insert(key.to_string(), (value.to_string(), Instant::now()));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), String> {
        let mut entries = self.entries.write().unwrap();
        entries.remove(key);
        Ok(())
    }
}

// =============================================================================
// Response Cache (RFC-0906)
// =============================================================================

/// Response cache for avoiding redundant API calls.
///
/// Caches responses based on request hash (model + messages + params).
/// Uses TTL-based expiration.
pub struct ResponseCache {
    entries: std::sync::RwLock<std::collections::HashMap<String, ResponseCacheEntry>>,
    ttl: Duration,
}

struct ResponseCacheEntry {
    response: String,
    cached_at: Instant,
}

impl ResponseCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: std::sync::RwLock::new(std::collections::HashMap::new()),
            ttl,
        }
    }

    /// Generate cache key from request parameters
    pub fn cache_key(
        model: &str,
        messages: &[crate::shared_types::Message],
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        model.hash(&mut hasher);
        for msg in messages {
            msg.role.hash(&mut hasher);
            msg.content.hash(&mut hasher);
        }
        temperature.map(|t| t.to_bits()).hash(&mut hasher);
        max_tokens.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get cached response
    pub fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().unwrap();
        if let Some(entry) = entries.get(key) {
            if entry.cached_at.elapsed() < self.ttl {
                return Some(entry.response.clone());
            }
        }
        None
    }

    /// Store response in cache
    pub fn set(&self, key: String, response: String) {
        let mut entries = self.entries.write().unwrap();
        entries.insert(
            key,
            ResponseCacheEntry {
                response,
                cached_at: Instant::now(),
            },
        );
    }

    /// Clear expired entries
    pub fn cleanup(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.retain(|_, entry| entry.cached_at.elapsed() < self.ttl);
    }
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(300)) // 5 minute default TTL
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
    fn test_cache_invalidation_new() {
        let cache = KeyCache::new();
        let ci = CacheInvalidation::new(cache);
        assert_eq!(ci.event_bus().subscriber_count(), 0);
        assert!(ci.wal_pubsub().is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidation_dual_write() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");
        let cache = KeyCache::new();
        let ci = CacheInvalidation::with_wal(cache, wal_path);

        // Subscribe to EventBus
        let rx = ci.event_bus().subscribe();

        // Publish invalidation event
        let event_id = ci
            .invalidate_key(
                vec![1, 2, 3],
                stoolap::pubsub::InvalidationReason::Revoke,
                None,
                None,
            )
            .unwrap();

        // Verify EventBus received the event
        let event = rx.recv().unwrap();
        match event {
            stoolap::pubsub::DatabaseEvent::KeyInvalidated { key_hash, .. } => {
                assert_eq!(key_hash, vec![1, 2, 3]);
            }
            _ => panic!("Expected KeyInvalidated event"),
        }

        // Verify WAL was written
        let wal = ci.wal_pubsub().unwrap();
        let entries = wal.read_from_lsn(0).unwrap();
        assert!(!entries.is_empty());
        assert_eq!(entries[0].event_id, event_id);
    }

    #[tokio::test]
    async fn test_cache_invalidation_idempotency() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");
        let cache = KeyCache::new();
        let ci = CacheInvalidation::with_wal(cache, wal_path.clone());

        // Write two different events
        let event_id1 = ci
            .invalidate_key(
                vec![4, 5, 6],
                stoolap::pubsub::InvalidationReason::Revoke,
                None,
                None,
            )
            .unwrap();

        let event_id2 = ci
            .invalidate_key(
                vec![7, 8, 9],
                stoolap::pubsub::InvalidationReason::Revoke,
                None,
                None,
            )
            .unwrap();

        // Different payloads → different event_ids
        assert_ne!(event_id1, event_id2);

        // WalPubSub::write() automatically marks events as seen
        let wal = ci.wal_pubsub().unwrap();
        assert!(wal.idempotency().is_duplicate(event_id1));
        assert!(wal.idempotency().is_duplicate(event_id2));

        // WAL should have both entries
        let entries = wal.read_from_lsn(0).unwrap();
        assert!(entries.len() >= 2);

        // Create a new reader simulating cross-process — fresh idempotency tracker
        let wal_b = stoolap::pubsub::WalPubSub::new(wal_path);
        let entries_b = wal_b.read_from_lsn(0).unwrap();
        assert!(entries_b.len() >= 2);

        // Before marking: not duplicates in fresh tracker
        assert!(!wal_b.idempotency().is_duplicate(event_id1));
        assert!(!wal_b.idempotency().is_duplicate(event_id2));

        // Mark first as seen
        wal_b.idempotency().mark_seen(event_id1);
        assert!(wal_b.idempotency().is_duplicate(event_id1));
        assert!(!wal_b.idempotency().is_duplicate(event_id2));
    }

    #[tokio::test]
    async fn test_wal_polling_cross_process() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("shared.wal");

        // Process A: writes to WAL
        let cache_a = KeyCache::new();
        let ci_a = CacheInvalidation::with_wal(cache_a, wal_path.clone());
        ci_a.invalidate_key(
            vec![7, 8, 9],
            stoolap::pubsub::InvalidationReason::Revoke,
            None,
            None,
        )
        .unwrap();

        // Process B: reads from same WAL (simulating cross-process)
        let wal_b = stoolap::pubsub::WalPubSub::new(wal_path);
        let entries = wal_b.read_from_lsn(0).unwrap();
        assert!(!entries.is_empty());

        // Verify the event content
        let event = stoolap::pubsub::wal_pubsub::parse_event(&entries[0].payload).unwrap();
        match event {
            stoolap::pubsub::DatabaseEvent::KeyInvalidated { key_hash, .. } => {
                assert_eq!(key_hash, vec![7, 8, 9]);
            }
            _ => panic!("Expected KeyInvalidated event"),
        }
    }
}
