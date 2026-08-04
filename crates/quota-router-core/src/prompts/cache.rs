//! LRU cache for resolved prompts (RFC-0948 AC: LRU prompt caching).
//!
//! Bounded by `cache_size` from `PromptConfig`. Each entry has a TTL
//! driven by `cache_ttl`; expired entries are treated as misses.
//!
//! The cache is keyed by `(prompt_id, version_string, request_id)` so
//! A/B test variants do not bleed across requests.

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cached render result keyed by `(prompt_id, version, request_id)`.
#[derive(Debug, Clone)]
pub struct CachedPrompt {
    pub prompt_id: String,
    pub version: String,
    pub request_id: String,
    pub rendered: String,
    pub inserted_at: Instant,
}

impl CachedPrompt {
    /// Returns true when the entry has outlived `ttl`.
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.inserted_at.elapsed() > ttl
    }
}

/// Bounded LRU cache for resolved prompts.
pub struct PromptCache {
    inner: Arc<RwLock<LruCache<CacheKey, CachedPrompt>>>,
    ttl: Duration,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    prompt_id: String,
    version: String,
    request_id: String,
}

impl PromptCache {
    /// Build a cache with the given max entries and per-entry TTL.
    pub fn new(cache_size: usize, ttl: Duration) -> Self {
        let cap = NonZeroUsize::new(cache_size.max(1)).expect("cache_size >= 1");
        Self {
            inner: Arc::new(RwLock::new(LruCache::new(cap))),
            ttl,
        }
    }

    pub fn get(&self, prompt_id: &str, version: &str, request_id: &str) -> Option<String> {
        let key = CacheKey {
            prompt_id: prompt_id.to_owned(),
            version: version.to_owned(),
            request_id: request_id.to_owned(),
        };
        let mut guard = self.inner.write().ok()?;
        let entry = guard.get(&key)?.clone();
        if entry.is_expired(self.ttl) {
            // Treat expired entry as a miss + drop it.
            guard.pop(&key);
            return None;
        }
        Some(entry.rendered)
    }

    pub fn put(&self, prompt_id: &str, version: &str, request_id: &str, rendered: &str) {
        let entry = CachedPrompt {
            prompt_id: prompt_id.to_owned(),
            version: version.to_owned(),
            request_id: request_id.to_owned(),
            rendered: rendered.to_owned(),
            inserted_at: Instant::now(),
        };
        let key = CacheKey {
            prompt_id: prompt_id.to_owned(),
            version: version.to_owned(),
            request_id: request_id.to_owned(),
        };
        if let Ok(mut guard) = self.inner.write() {
            guard.put(key, entry);
        }
    }

    pub fn invalidate(&self, prompt_id: &str) {
        if let Ok(mut guard) = self.inner.write() {
            let keys: Vec<CacheKey> = guard
                .iter()
                .map(|(k, _)| k.clone())
                .filter(|k| k.prompt_id == prompt_id)
                .collect();
            for k in keys {
                guard.pop(&k);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.inner.read().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ttl() -> Duration {
        Duration::from_secs(60)
    }

    #[test]
    fn lru_cache_hit_returns_rendered() {
        let cache = PromptCache::new(8, ttl());
        cache.put("p1", "1.0.0", "req-1", "rendered-text");
        assert_eq!(
            cache.get("p1", "1.0.0", "req-1"),
            Some("rendered-text".to_owned())
        );
    }

    #[test]
    fn lru_cache_miss_returns_none() {
        let cache = PromptCache::new(8, ttl());
        assert_eq!(cache.get("missing", "1.0.0", "req-1"), None);
    }

    #[test]
    fn lru_cache_key_includes_request_id_for_ab_test() {
        let cache = PromptCache::new(8, ttl());
        cache.put("p1", "1.0.0", "req-A", "render-A");
        cache.put("p1", "1.0.0", "req-B", "render-B");
        assert_eq!(
            cache.get("p1", "1.0.0", "req-A"),
            Some("render-A".to_owned())
        );
        assert_eq!(
            cache.get("p1", "1.0.0", "req-B"),
            Some("render-B".to_owned())
        );
    }

    #[test]
    fn lru_cache_evicts_oldest_when_full() {
        let cache = PromptCache::new(2, ttl());
        cache.put("p1", "1.0.0", "r", "a");
        cache.put("p2", "1.0.0", "r", "b");
        cache.put("p3", "1.0.0", "r", "c");
        // First entry must be evicted (LRU policy).
        assert!(cache.get("p1", "1.0.0", "r").is_none());
        assert_eq!(cache.get("p2", "1.0.0", "r"), Some("b".to_owned()));
        assert_eq!(cache.get("p3", "1.0.0", "r"), Some("c".to_owned()));
    }

    #[test]
    fn lru_cache_invalidate_removes_prompt_versions() {
        let cache = PromptCache::new(8, ttl());
        cache.put("p1", "1.0.0", "r", "a");
        cache.put("p1", "2.0.0", "r", "b");
        cache.put("p2", "1.0.0", "r", "c");
        cache.invalidate("p1");
        assert!(cache.get("p1", "1.0.0", "r").is_none());
        assert!(cache.get("p1", "2.0.0", "r").is_none());
        assert_eq!(cache.get("p2", "1.0.0", "r"), Some("c".to_owned()));
    }

    #[test]
    fn lru_cache_treats_expired_entry_as_miss() {
        let cache = PromptCache::new(8, Duration::from_millis(10));
        cache.put("p1", "1.0.0", "r", "a");
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(cache.get("p1", "1.0.0", "r"), None);
    }
}
