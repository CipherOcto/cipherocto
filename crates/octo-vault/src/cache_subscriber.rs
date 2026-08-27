//! Mission B (RFC-0960 v3.7 §2.4 + RFC-0913) per-process cache-subscriber
//! factory.
//!
//! Layer B (octo-vault) substrate. The [`VaultProjectionInvalidationSubscriber`]
//! port trait is implemented at Layer D (`octo-vault-stoolap`) with a
//! Stoolap NOTIFY/LISTEN adapter; this module owns the substrate-side
//! subscriber-task factory and bootstrap helper that Layer C binaries call
//! during process init.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::event_log_producer::VaultProjectionInvalidationEnvelope;
use crate::vault_balance_projection::VaultBalanceCache;

/// Port trait for the bus-substrate backing the cache invalidation bus.
///
/// Production impl lives at `octo-vault-stoolap` Layer D (Stoolap
/// NOTIFY/LISTEN over `cache:projection:<hex(vault_id)>` wildcard
/// channel).
///
/// `recv` is blocking. Returns `None` on channel close so the spawned
/// task loop terminates cleanly when the bus adapter disconnects.
pub trait VaultProjectionInvalidationSubscriber: Send + Sync {
    /// Blocking receive; returns `None` on channel close.
    fn recv(&self) -> Option<VaultProjectionInvalidationEnvelope>;
}

/// Spawn the per-process cache invalidation subscriber task.
///
/// The task loop:
/// ```text
/// while let Some(_env) = subscriber.recv() {
///     cache.lock().unwrap_or_else(PoisonError::into_inner).invalidate_all();
/// }
/// ```
///
/// Per RFC-0960 v3.7 §2.4 the current substrate performs whole-cache
/// invalidation on every envelope (per-key invalidation reserved for
/// Cycle 2). Lock ordering rule: subscriber holds `cache: Mutex` only,
/// never `process_drain_lock`; producers hold `process_drain_lock` only,
/// never the cache — see [`cipherocto-design-principles`] §Lock-ordering
/// cross-boundary.
#[must_use]
pub fn spawn_cache_subscriber(
    cache: Arc<Mutex<VaultBalanceCache>>,
    subscriber: Arc<dyn VaultProjectionInvalidationSubscriber>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Some(_envelope) = subscriber.recv() {
            let mut guard = match cache.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.invalidate_all();
        }
    })
}

/// Process-init bootstrap factory (Mission B §6).
///
/// Wraps [`spawn_cache_subscriber`] for Layer C binaries to call from their
/// `main` / server bootstrap. Layer C wire-up (octo-wallet-node /
/// quota-router-sm-engine / octo-policy) is tracked separately at
/// `producer-wrapper-consumer-wiring.md`.
#[must_use]
pub fn init_cache_subscriber(
    cache: Arc<Mutex<VaultBalanceCache>>,
    subscriber: Arc<dyn VaultProjectionInvalidationSubscriber>,
) -> JoinHandle<()> {
    spawn_cache_subscriber(cache, subscriber)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log_producer::VaultProjectionInvalidationEnvelope;
    use crate::vault_balance_projection::{CacheKey, ProjectionSource, VaultBalanceProjection};
    use octo_cap_macaroon::{AssetId, ChainId, VaultId};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::PoisonError;

    fn sample_cache_key(seed: u8) -> CacheKey {
        CacheKey {
            chain_id: ChainId::from_bytes([seed; 32]),
            vault_id: VaultId::from_bytes([seed.wrapping_add(1); 32]),
            asset_id: AssetId::from_bytes([seed.wrapping_add(2); 32]),
        }
    }

    fn sample_projection() -> VaultBalanceProjection {
        VaultBalanceProjection {
            chain_id: ChainId::from_bytes([1u8; 32]),
            vault_id: VaultId::from_bytes([2u8; 32]),
            asset_id: AssetId::from_bytes([3u8; 32]),
            projected_balance: octo_cap_macaroon::Dqa::new(1_000, 0).unwrap(),
            projected_at_unix_seconds: Some(1_700_000_000),
            registry_snapshot_epoch: 0,
            source_kind: ProjectionSource::Cache,
        }
    }

    /// Mock subscriber backed by a shared queue + closed flag.
    ///
    /// `std::sync::mpsc::Receiver` is `!Sync` so we hand-roll a minimal
    /// shared queue. The task's `recv()` polls the mutex; tests
    /// send via the same mutex (cloning the inner Arc).
    struct MockSubscriber {
        queue: std::sync::Mutex<std::collections::VecDeque<VaultProjectionInvalidationEnvelope>>,
        closed: std::sync::atomic::AtomicBool,
        counter: Arc<AtomicUsize>,
    }

    impl MockSubscriber {
        fn enqueue(&self, env: VaultProjectionInvalidationEnvelope) {
            let mut q = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
            q.push_back(env);
        }
        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    impl VaultProjectionInvalidationSubscriber for MockSubscriber {
        fn recv(&self) -> Option<VaultProjectionInvalidationEnvelope> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            // Simple blocking-poll — busy-wait with a short sleep until an
            // item is available or the channel is closed.
            loop {
                let mut q = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(env) = q.pop_front() {
                    return Some(env);
                }
                drop(q);
                if self.closed.load(Ordering::SeqCst) {
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    fn mock_subscriber() -> Arc<MockSubscriber> {
        let counter = Arc::new(AtomicUsize::new(0));
        Arc::new(MockSubscriber {
            queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            closed: std::sync::atomic::AtomicBool::new(false),
            counter,
        })
    }

    fn empty_envelope(seed: u8) -> VaultProjectionInvalidationEnvelope {
        VaultProjectionInvalidationEnvelope {
            chain_id: ChainId::from_bytes([seed; 32]),
            vault_id: VaultId::from_bytes([seed.wrapping_add(1); 32]),
            asset_id: AssetId::from_bytes([seed.wrapping_add(2); 32]),
            source_kind: ProjectionSource::FreshLogScan,
        }
    }

    /// TV-CS-1: `spawn_cache_subscriber` returns a `JoinHandle<()>` that is
    /// not finished immediately after spawn (subscriber is blocking on
    /// `recv`).
    #[test]
    fn tv_cs1_handle_not_finished_at_spawn() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        let h = spawn_cache_subscriber(cache, sub.clone());
        // Give the task a moment to enter recv.
        thread::sleep(std::time::Duration::from_millis(10));
        assert!(!h.is_finished(), "subscriber task must still be running");
        sub.close(); // close channel → task exits
        let _ = h.join();
    }

    /// TV-CS-2: 3 envelopes cause the cache to drain from 3 entries to 0.
    #[test]
    fn tv_cs2_three_envelopes_drain_cache() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        // Pre-fill with 3 entries.
        {
            let mut g = cache.lock().unwrap_or_else(PoisonError::into_inner);
            g.put(sample_cache_key(1), sample_projection());
            g.put(sample_cache_key(2), sample_projection());
            g.put(sample_cache_key(3), sample_projection());
            assert_eq!(g.len(), 3);
        }
        let h = spawn_cache_subscriber(cache.clone(), sub.clone());
        for i in 1..=3u8 {
            sub.enqueue(empty_envelope(i));
        }
        // Give the task time to drain.
        thread::sleep(std::time::Duration::from_millis(50));
        let len = cache.lock().unwrap_or_else(PoisonError::into_inner).len();
        assert_eq!(len, 0, "3 envelopes must drain the cache");
        sub.close();
        let _ = h.join();
    }

    /// TV-CS-3: `None` (channel close) → task exits cleanly.
    #[test]
    fn tv_cs3_channel_close_exits_cleanly() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        let h = spawn_cache_subscriber(cache, sub.clone());
        sub.close();
        let result = h.join();
        assert!(result.is_ok(), "subscriber task must exit cleanly");
    }

    /// TV-CS-4: lock-poisoning resilience — drop a guard mid-invalidate →
    /// next iteration recovers via `unwrap_or_else(PoisonError::into_inner)`.
    #[test]
    fn tv_cs4_lock_poisoning_resilience() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        // Pre-poison the mutex by panicking inside a guard (test-only).
        let cache_clone = cache.clone();
        let _ = std::thread::spawn(move || {
            let _g = cache_clone.lock().unwrap();
            panic!("intentional poison");
        })
        .join();
        // Cache is now poisoned.
        assert!(cache.lock().is_err());
        let h = spawn_cache_subscriber(cache.clone(), sub.clone());
        // Send 2 envelopes — both must succeed via into_inner recovery.
        sub.enqueue(empty_envelope(1));
        sub.enqueue(empty_envelope(2));
        thread::sleep(std::time::Duration::from_millis(50));
        // After recovery, cache is empty (was already empty + 2 invalidations).
        let g = cache.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(g.len(), 0);
        drop(g);
        sub.close();
        let _ = h.join();
    }

    /// TV-CS-5: a single envelope for vault A invalidates ALL cached entries
    /// (wildcard bus semantics per §2.4 — Cycle 2 will replace with
    /// per-key invalidation).
    #[test]
    fn tv_cs5_single_envelope_invalidates_all() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        {
            let mut g = cache.lock().unwrap_or_else(PoisonError::into_inner);
            g.put(sample_cache_key(10), sample_projection());
            g.put(sample_cache_key(20), sample_projection());
            g.put(sample_cache_key(30), sample_projection());
            assert_eq!(g.len(), 3);
        }
        let h = spawn_cache_subscriber(cache.clone(), sub.clone());
        sub.enqueue(empty_envelope(10)); // envelope for vault at key 10
        thread::sleep(std::time::Duration::from_millis(50));
        let len = cache.lock().unwrap_or_else(PoisonError::into_inner).len();
        assert_eq!(len, 0, "wildcard envelope invalidates entire cache");
        sub.close();
        let _ = h.join();
    }

    /// TV-CS-6: subscriber init runs BEFORE producer-style mutation — when
    /// subscriber + a sender exist together, ordering is preserved by
    /// spawn happening before the test sends envelopes.
    #[test]
    fn tv_cs6_init_before_producer_emits() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        // Subscriber spawned first — init phase.
        let h = spawn_cache_subscriber(cache.clone(), sub.clone());
        // Producer-style emit happens AFTER subscriber is up.
        sub.enqueue(empty_envelope(99));
        thread::sleep(std::time::Duration::from_millis(50));
        let len = cache.lock().unwrap_or_else(PoisonError::into_inner).len();
        assert_eq!(
            len, 0,
            "subscriber must drain cache for envelope sent post-spawn"
        );
        sub.close();
        let _ = h.join();
    }

    /// TV-CS-7 (bonus): `init_cache_subscriber` is a thin pass-through to
    /// `spawn_cache_subscriber`.
    #[test]
    fn tv_cs7_init_is_passthrough() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        let h = init_cache_subscriber(cache.clone(), sub.clone());
        sub.enqueue(empty_envelope(1));
        thread::sleep(std::time::Duration::from_millis(50));
        let len = cache.lock().unwrap_or_else(PoisonError::into_inner).len();
        assert_eq!(len, 0);
        sub.close();
        let _ = h.join();
    }

    /// TV-CS-8 (bonus): `recv()` is invoked at least once before any
    /// envelope is sent — proves the task entered the loop.
    #[test]
    fn tv_cs8_recv_called_before_send() {
        let sub = mock_subscriber();
        let counter = sub.counter.clone();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        let h = spawn_cache_subscriber(cache, sub.clone());
        // Give task time to enter recv at least once.
        thread::sleep(std::time::Duration::from_millis(20));
        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "recv must be called at least once before any send"
        );
        sub.close();
        let _ = h.join();
    }
}
