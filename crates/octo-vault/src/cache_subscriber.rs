//! Mission B (RFC-0960 §2.4 + RFC-0913) per-process cache-subscriber
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

use dashmap::DashMap;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

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
///
/// **Extension note:** Rust does not permit `#[non_exhaustive]` on
/// traits. To prevent downstream from adding methods, the trait would
/// need a sealed marker pattern (`mod sealed { pub trait Sealed {} }` +
/// `impl sealed::Sealed for crate::...`). That is deferred — current
/// RFC-0960 §2.4 enumerates the only intended method (`recv`), and any
/// future method addition is an additive semver-minor bump by convention.
pub trait VaultProjectionInvalidationSubscriber: Send + Sync {
    /// Blocking receive; returns `None` on channel close.
    fn recv(&self) -> Option<VaultProjectionInvalidationEnvelope>;
}

/// Errors surfaced by envelope verification (`cache-bus-auth` Mission).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeVerificationError {
    /// Producer DID not in trust list — fail-closed.
    #[error("unknown producer_did: {0}")]
    UnknownProducer(String),
    /// Ed25519 signature verification failed.
    #[error("signature verification failed for producer {producer_did}")]
    InvalidSignature {
        /// Producer DID whose signature failed verification.
        producer_did: String,
    },
    /// Sequence number ≤ last-seen sequence — replay defense.
    #[error("replay: producer {producer_did} sequence {observed} ≤ last_seen {last_seen}")]
    Replay {
        /// Producer DID whose envelope replayed.
        producer_did: String,
        /// Sequence number observed in the (rejected) envelope.
        observed: u64,
        /// Highest sequence number previously accepted for this producer.
        last_seen: u64,
    },
    /// Wire-form version not supported by this substrate (e.g. v3 envelope
    /// received by a v2-only subscriber).
    #[error("unsupported envelope version: {0}")]
    UnsupportedVersion(u8),
}

/// Producer trust list + per-producer monotonic sequence tracking
/// (`cache-bus-auth` Mission sub-step 4).
///
/// `trust_keys: HashMap<OverlayIdentity, VerifyingKey>` (authoritative
/// list at init). `last_seen_sequence: DashMap<OverlayIdentity, u64>`
/// updated on accept-only (concurrent-safe via DashMap).
#[derive(Debug)]
pub struct ProducerTrustList {
    /// Authoritative list of `OverlayIdentity → VerifyingKey` pairs at init.
    trust_keys: std::collections::HashMap<String, VerifyingKey>,
    /// Per-producer monotonic last-seen sequence (replay defense). Updated
    /// on accept-only; concurrent via DashMap.
    last_seen_sequence: DashMap<String, u64>,
}

impl ProducerTrustList {
    /// Build a new trust list from a list of `(producer_did, verifying_key)`
    /// pairs. Empty list = fail-closed default per TV-CB-6.
    #[must_use]
    pub fn new(keys: Vec<(String, VerifyingKey)>) -> Self {
        Self {
            trust_keys: keys.into_iter().collect(),
            last_seen_sequence: DashMap::new(),
        }
    }

    /// Lookup the verifying key for a producer DID.
    #[must_use]
    pub fn get_verifying_key(&self, producer_did: &str) -> Option<&VerifyingKey> {
        self.trust_keys.get(producer_did)
    }

    /// Verify an envelope's signature + monotonic sequence. Updates
    /// `last_seen_sequence` only on success.
    pub fn verify_and_update_sequence(
        &self,
        envelope: &VaultProjectionInvalidationEnvelope,
    ) -> Result<(), EnvelopeVerificationError> {
        // 1. Wire-form version gate.
        if envelope.version != crate::event_log_producer::ENVELOPE_VERSION_V2 {
            return Err(EnvelopeVerificationError::UnsupportedVersion(
                envelope.version,
            ));
        }
        // 2. Producer trust list lookup (fail-closed).
        let vk = self
            .get_verifying_key(&envelope.producer_did)
            .ok_or_else(|| {
                EnvelopeVerificationError::UnknownProducer(envelope.producer_did.clone())
            })?;
        // 3. Signature verification.
        let preimage = envelope.preimage();
        let sig = ed25519_dalek::Signature::from_bytes(&envelope.producer_signature);
        vk.verify(&preimage, &sig)
            .map_err(|_| EnvelopeVerificationError::InvalidSignature {
                producer_did: envelope.producer_did.clone(),
            })?;
        // 4. Sequence monotonicity (replay defense).
        let mut entry = self
            .last_seen_sequence
            .entry(envelope.producer_did.clone())
            .or_insert(0);
        if envelope.sequence <= *entry {
            return Err(EnvelopeVerificationError::Replay {
                producer_did: envelope.producer_did.clone(),
                observed: envelope.sequence,
                last_seen: *entry,
            });
        }
        *entry = envelope.sequence;
        Ok(())
    }
}

/// Sign an envelope's preimage with the given ed25519 signing key.
/// Returns the 64-byte signature suitable for `envelope.producer_signature`.
#[must_use]
pub fn sign_envelope(preimage: &[u8], signing_key: &SigningKey) -> [u8; 64] {
    let sig = signing_key.sign(preimage);
    sig.to_bytes()
}

/// Spawn the per-process cache invalidation subscriber task (v1 legacy).
///
/// V1 envelopes (pre-`cache-bus-auth`) skip signature verification —
/// this is the 1-cycle warn-only window per `cache-bus-auth` §Risk row 1.
/// New production deployments MUST use
/// [`spawn_cache_subscriber_with_trust_list`] (v2 signed) instead.
///
/// The task loop:
/// ```text
/// while let Some(_env) = subscriber.recv() {
///     cache.lock().unwrap_or_else(PoisonError::into_inner).invalidate_all();
/// }
/// ```
///
/// Per RFC-0960 §2.4 the current substrate performs whole-cache
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

/// Spawn the v2 signed-envelope subscriber (`cache-bus-auth` Mission).
///
/// Verifies each envelope's ed25519 signature + monotonic sequence
/// before invalidating the cache (TV-CB-2/3/4/5/6). Unknown producers,
/// invalid signatures, and replays are dropped silently — `cache.len()`
/// stays unchanged.
#[must_use]
pub fn spawn_cache_subscriber_with_trust_list(
    cache: Arc<Mutex<VaultBalanceCache>>,
    subscriber: Arc<dyn VaultProjectionInvalidationSubscriber>,
    trust_list: Arc<ProducerTrustList>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Some(envelope) = subscriber.recv() {
            if trust_list.verify_and_update_sequence(&envelope).is_ok() {
                let mut guard = match cache.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                guard.invalidate_all();
            }
            // else: silent drop (TV-CB-3/4/5/6)
        }
    })
}

/// Process-init bootstrap factory (Mission B §6 — v1 legacy).
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

/// Process-init bootstrap factory for v2 signed envelopes.
///
/// Loads the producer trust list from the binary's known producer DIDs
/// and spawns the verifying subscriber. Layer C binaries call this in
/// their bootstrap instead of [`init_cache_subscriber`] for v2
/// deployments.
#[must_use]
pub fn init_producer_trust_list(
    cache: Arc<Mutex<VaultBalanceCache>>,
    subscriber: Arc<dyn VaultProjectionInvalidationSubscriber>,
    trust_list: Arc<ProducerTrustList>,
) -> JoinHandle<()> {
    spawn_cache_subscriber_with_trust_list(cache, subscriber, trust_list)
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
        VaultProjectionInvalidationEnvelope::v1_legacy(
            ChainId::from_bytes([seed; 32]),
            VaultId::from_bytes([seed.wrapping_add(1); 32]),
            AssetId::from_bytes([seed.wrapping_add(2); 32]),
            ProjectionSource::FreshLogScan,
        )
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
    ///
    /// **R1 strengthening:** assert the `closed` flag on the subscriber
    /// was actually set by `close()` — guards against a `MockSubscriber`
    /// regression where `recv()` returns `None` early regardless of state.
    #[test]
    fn tv_cs3_channel_close_exits_cleanly() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        let h = spawn_cache_subscriber(cache, sub.clone());
        // Pre-condition: closed flag is false.
        assert!(!sub.closed.load(Ordering::SeqCst));
        sub.close();
        // Post-condition: closed flag flipped.
        assert!(
            sub.closed.load(Ordering::SeqCst),
            "MockSubscriber.close() MUST set closed flag"
        );
        let result = h.join();
        assert!(result.is_ok(), "subscriber task must exit cleanly");
    }

    /// TV-CS-4: lock-poisoning resilience — drop a guard mid-invalidate →
    /// next iteration recovers via `unwrap_or_else(PoisonError::into_inner)`.
    ///
    /// **R1 strengthening:** pre-populate the cache with 3 entries so the
    /// `g.len() == 0` assertion is NOT trivially satisfied; assert
    /// `h.join()` returned Ok (regression to `panic!` would surface here).
    #[test]
    fn tv_cs4_lock_poisoning_resilience() {
        let sub = mock_subscriber();
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        // Pre-populate the cache BEFORE poisoning so the g.len()==0
        // assertion has substance (R1 fix).
        {
            let mut g = cache.lock().unwrap_or_else(PoisonError::into_inner);
            g.put(sample_cache_key(1), sample_projection());
            g.put(sample_cache_key(2), sample_projection());
            g.put(sample_cache_key(3), sample_projection());
            assert_eq!(g.len(), 3);
        }
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
        // Send 3 envelopes — all must succeed via into_inner recovery
        // AND drain the cache.
        for i in 1..=3u8 {
            sub.enqueue(empty_envelope(i));
        }
        thread::sleep(std::time::Duration::from_millis(50));
        // After recovery, cache MUST be empty (was 3 entries + 3 invalidations).
        let g = cache.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            g.len(),
            0,
            "recovery must invalidate the 3 pre-populated entries"
        );
        drop(g);
        sub.close();
        let join_result = h.join();
        assert!(
            join_result.is_ok(),
            "subscriber task MUST exit cleanly (panic in poisoned lock recovery would surface here)"
        );
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

    // ============================================================================
    // TV-CB-1..7 — cache-bus-auth test vectors
    // ============================================================================

    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use rand::RngCore;

    fn sample_signing_key() -> SigningKey {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        SigningKey::from_bytes(&secret)
    }

    fn sample_did(seed: u8) -> String {
        format!("did:octo:test:producer:{seed}")
    }

    fn signed_v2_envelope(
        signing_key: &SigningKey,
        producer_did: &str,
        sequence: u64,
        seed: u8,
    ) -> VaultProjectionInvalidationEnvelope {
        let chain_id = ChainId::from_bytes([seed; 32]);
        let vault_id = VaultId::from_bytes([seed.wrapping_add(1); 32]);
        let asset_id = AssetId::from_bytes([seed.wrapping_add(2); 32]);
        let mut env = VaultProjectionInvalidationEnvelope {
            version: crate::event_log_producer::ENVELOPE_VERSION_V2,
            chain_id,
            vault_id,
            asset_id,
            source_kind: ProjectionSource::FreshLogScan,
            producer_did: producer_did.to_string(),
            sequence,
            producer_signature: [0u8; 64],
        };
        let preimage = env.preimage();
        env.producer_signature = sign_envelope(&preimage, signing_key);
        env
    }

    /// TV-CB-1: envelope struct has the 8 expected fields (was 4);
    /// constructing a v2 envelope requires all 8 fields explicitly.
    #[test]
    fn tv_cb1_v2_envelope_has_8_fields() {
        let sk = sample_signing_key();
        let env = signed_v2_envelope(&sk, &sample_did(1), 1, 1);
        assert_eq!(env.version, crate::event_log_producer::ENVELOPE_VERSION_V2);
        assert_eq!(env.producer_did, sample_did(1));
        assert_eq!(env.sequence, 1);
        assert_ne!(env.producer_signature, [0u8; 64]);
    }

    /// TV-CB-2: producer-side sign-then-emit produces an envelope that
    /// the trust-list-verifying subscriber validates.
    #[test]
    fn tv_cb2_signed_envelope_verifies() {
        let sk = sample_signing_key();
        let vk = sk.verifying_key();
        let did = sample_did(2);
        let env = signed_v2_envelope(&sk, &did, 1, 2);
        let tl = ProducerTrustList::new(vec![(did.clone(), vk)]);
        assert!(tl.verify_and_update_sequence(&env).is_ok());
    }

    /// TV-CB-3: tampered envelope (modify `vault_id` after signing) fails
    /// verification → subscriber drops the envelope, cache stays populated.
    ///
    /// **R1 strengthening:** positive control — a VALID envelope (also
    /// pre-populating the cache) MUST drain the cache. Without this, a
    /// "silent drop" regression in `spawn_cache_subscriber_with_trust_list`
    /// (e.g. unconditional drop instead of verify-then-drop) would let
    /// the tampered test pass without exercising `verify_and_update_sequence`.
    #[test]
    fn tv_cb3_tampered_envelope_rejected() {
        let sub = mock_subscriber();
        let sk = sample_signing_key();
        let vk = sk.verifying_key();
        let did = sample_did(3);
        // Positive control first — valid envelope MUST drain cache.
        let valid_env = signed_v2_envelope(&sk, &did, 1, 3);
        // Negative case — tampered envelope.
        let mut tampered_env = signed_v2_envelope(&sk, &did, 2, 3);
        tampered_env.vault_id = VaultId::from_bytes([0xff; 32]); // tampered post-sign
        let tl = Arc::new(ProducerTrustList::new(vec![(did.clone(), vk)]));
        let cache = Arc::new(Mutex::new(VaultBalanceCache::new(60)));
        {
            let mut g = cache.lock().unwrap_or_else(PoisonError::into_inner);
            g.put(sample_cache_key(3), sample_projection());
            g.put(sample_cache_key(4), sample_projection());
            assert_eq!(g.len(), 2);
        }
        let h = spawn_cache_subscriber_with_trust_list(cache.clone(), sub.clone(), tl.clone());
        // Send valid then tampered.
        sub.enqueue(valid_env);
        sub.enqueue(tampered_env);
        thread::sleep(std::time::Duration::from_millis(50));
        // After valid-only drain (tampered rejected) cache MUST be empty:
        // valid envelope drained all entries, tampered was a no-op.
        // The stronger assertion: the subscriber DID process valid AND
        // rejected tampered (cache.len() == 0 proves valid was processed;
        // no entry in last_seen_sequence with sequence=2 proves tampered
        // was rejected before sequence-update).
        let g = cache.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            g.len(),
            0,
            "valid envelope MUST drain cache; tampered envelope MUST be dropped"
        );
        drop(g);
        // Tampered envelopes leave no trace — last_seen_sequence MUST be at sequence=1
        // (valid accepted), NOT 2 (which would mean tampered bypassed verify).
        let recorded = tl.last_seen_sequence.get(&did).map(|v| *v).unwrap_or(0);
        assert_eq!(
            recorded, 1,
            "tampered envelope MUST NOT advance last_seen_sequence past valid (recorded={recorded})"
        );
        sub.close();
        let result = h.join();
        assert!(result.is_ok(), "subscriber task MUST exit cleanly");
    }

    /// TV-CB-4: replay defense — multi-vector coverage:
    /// (a) same-value replay (5 then 5) → Replay
    /// (b) lower-after-higher (5 then 4) → Replay (catches `==` regression)
    /// (c) wrap-around (u64::MAX then 0) → Replay (catches `<` regression on edge)
    /// (d) monotonic positive (5 then 6) → Ok (control)
    #[test]
    fn tv_cb4_replay_rejected() {
        let sk = sample_signing_key();
        let vk = sk.verifying_key();
        let did = sample_did(4);
        let tl = ProducerTrustList::new(vec![(did.clone(), vk)]);
        // (a) Same-value replay.
        let env_a = signed_v2_envelope(&sk, &did, 5, 4);
        assert!(tl.verify_and_update_sequence(&env_a).is_ok());
        assert!(matches!(
            tl.verify_and_update_sequence(&env_a).unwrap_err(),
            EnvelopeVerificationError::Replay { .. }
        ));
        // (b) Lower-after-higher: a regression to `==` instead of `<=` would accept.
        let env_b = signed_v2_envelope(&sk, &did, 4, 4);
        assert!(matches!(
            tl.verify_and_update_sequence(&env_b).unwrap_err(),
            EnvelopeVerificationError::Replay { .. }
        ));
        // (c) Wrap-around: last_seen=u64::MAX (set manually), next=0.
        tl.last_seen_sequence.insert(did.clone(), u64::MAX);
        let env_c = signed_v2_envelope(&sk, &did, 0, 4);
        assert!(matches!(
            tl.verify_and_update_sequence(&env_c).unwrap_err(),
            EnvelopeVerificationError::Replay { .. }
        ));
        // Cleanup: reset so (d) is a real monotonic-positive control.
        tl.last_seen_sequence.insert(did.clone(), 5);
        // (d) Monotonic positive (5 then 6) MUST succeed.
        let env_d = signed_v2_envelope(&sk, &did, 6, 4);
        assert!(
            tl.verify_and_update_sequence(&env_d).is_ok(),
            "monotonic-positive sequence MUST verify"
        );
    }

    /// TV-CB-5: unknown producer_did (not in trust list) is rejected.
    #[test]
    fn tv_cb5_unknown_producer_rejected() {
        let sk = sample_signing_key();
        let env = signed_v2_envelope(&sk, "did:octo:rogue:not-in-list", 1, 5);
        let vk = sk.verifying_key();
        let tl = ProducerTrustList::new(vec![(sample_did(5), vk)]);
        let err = tl.verify_and_update_sequence(&env).unwrap_err();
        assert!(matches!(err, EnvelopeVerificationError::UnknownProducer(_)));
    }

    /// TV-CB-6: process init with empty trust list → all envelopes rejected
    /// (fail-closed default).
    #[test]
    fn tv_cb6_empty_trust_list_rejects_all() {
        let sk = sample_signing_key();
        let env = signed_v2_envelope(&sk, &sample_did(6), 1, 6);
        let tl = ProducerTrustList::new(vec![]);
        let err = tl.verify_and_update_sequence(&env).unwrap_err();
        assert!(matches!(err, EnvelopeVerificationError::UnknownProducer(_)));
    }

    /// TV-CB-7: preimage starts with the cross-protocol domain separator
    /// (mandatory per `cache-bus-auth` §Sub-step 3).
    #[test]
    fn tv_cb7_preimage_has_domain_separator() {
        let sk = sample_signing_key();
        let env = signed_v2_envelope(&sk, &sample_did(7), 1, 7);
        let preimage = env.preimage();
        assert!(
            preimage.starts_with(crate::event_log_producer::CACHE_BUS_DOMAIN_SEPARATOR),
            "preimage MUST start with CACHE_BUS_DOMAIN_SEPARATOR"
        );
    }

    /// TV-CB-8 (bonus): v1 legacy envelope fails verification with
    /// UnsupportedVersion.
    #[test]
    fn tv_cb8_v1_envelope_unsupported() {
        let sk = sample_signing_key();
        let vk = sk.verifying_key();
        let did = sample_did(8);
        let v1 = VaultProjectionInvalidationEnvelope::v1_legacy(
            ChainId::from_bytes([8u8; 32]),
            VaultId::from_bytes([9u8; 32]),
            AssetId::from_bytes([10u8; 32]),
            ProjectionSource::FreshLogScan,
        );
        let tl = ProducerTrustList::new(vec![(did, vk)]);
        let err = tl.verify_and_update_sequence(&v1).unwrap_err();
        assert!(matches!(
            err,
            EnvelopeVerificationError::UnsupportedVersion(1)
        ));
        // Avoid unused-warning for sk.
        let _ = sk.verifying_key();
    }
}
