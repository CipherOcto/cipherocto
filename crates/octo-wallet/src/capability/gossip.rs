// RFC-0959-A1 §Phase 3: gossip retry loop + cross-node delivery.
//
// Wraps `CapabilityCatalog::gossip_to_buyer` in a bounded retry loop to
// handle Finding A11 (gossip partition → envelope not received).
// Exhaustion emits `DeliveryError::GossipFailed { attempts }`.

// **Mission 0959-c3:** the original `CapabilityCatalog::gossip_to_buyer`
// was split into a separate `CapabilityGossip` async trait so the primary
// catalog stays object-safe (`&dyn CapabilityCatalog` is used elsewhere
// for caveat attenuation + macaroon storage). The bounded retry loop in
// this module dispatches via the sync shim `gossip_to_buyer_sync` on the
// primary trait; catalogs that implement async gossip return
// `Err(Unsupported)` from the sync shim and instead expose the async
// `CapabilityGossip::gossip_to_buyer` for runtime callers. The 0959-c2
// in-process harness uses the sync path (no tokio runtime required);
// production `TransportDeliveryCatalog` uses the async path directly.

use std::thread;
use std::time::Duration;

use super::macaroon::{CapabilityCatalog, CapabilityGossip, CatalogGossipError};
use super::market_delivery::{DeliveryError, MarketDeliveryEnvelope};

/// Maximum gossip retry attempts (RFC-0959-A1 §Future Work F5).
pub const MAX_GOSSIP_ATTEMPTS: u32 = 5;

/// Initial backoff delay (doubles each retry, capped at MAX_BACKOFF).
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(50);
pub const MAX_BACKOFF: Duration = Duration::from_secs(2);

/// Compute exponential backoff for the given attempt (1-indexed):
/// `min(INITIAL_BACKOFF * 2^(attempt-1), MAX_BACKOFF)`.
///
/// Exposed for tests; the `gossip_envelope_to_buyer` loop uses it
/// internally on `Transient` errors.
#[must_use]
pub fn backoff_for_attempt(attempt: u32) -> Duration {
    // attempt is 1-indexed; saturate shift to avoid overflow on large
    // attempt values (still bounded by MAX_GOSSIP_ATTEMPTS = 5 in practice).
    let exp = attempt.saturating_sub(1).min(31);
    let factor = 1u32 << exp;
    let candidate = INITIAL_BACKOFF.saturating_mul(factor);
    if candidate > MAX_BACKOFF {
        MAX_BACKOFF
    } else {
        candidate
    }
}

/// Gossip `MarketDeliveryEnvelope` to the buyer via `CapabilityCatalog`.
/// Bounded retry with exponential backoff; exhaustion → `GossipFailed`.
///
/// **Retry policy (RFC-0959-A1 §Phase 3 + mission `0959-c1-gossip-error-variants`):**
/// - `Ok(())` → return `Ok(())`.
/// - `Err(Unsupported)` → fail-fast (catalog lacks gossip substrate OR
///   only implements async `CapabilityGossip`); return
///   `GossipFailed { attempts }` immediately (no retry, no backoff).
/// - `Err(Permanent(_))` → fail-fast (schema/capability mismatch); return
///   `GossipFailed { attempts }` immediately (no retry).
/// - `Err(Transient(_))` → backoff per `backoff_for_attempt(attempt)` then
///   retry up to `MAX_GOSSIP_ATTEMPTS`. Exhaustion → `GossipFailed { attempts: 5 }`.
///
/// **Mission 0959-c3:** this function delegates to the sync shim
/// `catalog.gossip_to_buyer_sync(...)`. Catalogs that implement only
/// the async `CapabilityGossip` trait (e.g.,
/// `TransportDeliveryCatalog`) return `Unsupported` from the sync shim
/// and require the async dispatch path
/// `gossip_envelope_to_buyer_async`. The 0959-c2 in-process harness
/// keeps using this sync function; production wiring uses the new
/// async function.
pub fn gossip_envelope_to_buyer(
    env: &MarketDeliveryEnvelope,
    buyer_did: &str,
    catalog: &dyn CapabilityCatalog,
) -> Result<(), DeliveryError> {
    // **Round 6 (F35 fix):** `unwrap_or_default()` previously swallowed
    // serialization errors, gossiping empty bytes silently. Empty payload
    // would deserialize downstream to a different error mode (the buyer's
    // JSON parser would fail, not the gossiper's). Surfacing the error
    // here makes the failure mode visible to operators and prevents
    // "phantom successful gossip" where the seller thinks the envelope
    // arrived but the buyer received garbage.
    let payload = serde_json::to_vec(env).map_err(|e| DeliveryError::SerializationError {
        reason: format!("envelope serialization failed: {e}"),
    })?;
    for attempt in 1..=MAX_GOSSIP_ATTEMPTS {
        match catalog.gossip_to_buyer_sync(buyer_did, &payload) {
            Ok(()) => return Ok(()),
            // `Unsupported` (catalog lacks gossip substrate) and `Permanent`
            // (non-retryable schema/capability mismatch) both fail-fast.
            Err(CatalogGossipError::Unsupported | CatalogGossipError::Permanent(_)) => {
                return Err(DeliveryError::GossipFailed { attempts: attempt });
            }
            Err(CatalogGossipError::Transient(_)) => {
                if attempt < MAX_GOSSIP_ATTEMPTS {
                    // Sleep before next retry; final attempt does NOT
                    // sleep (no point; will exit loop next iteration).
                    thread::sleep(backoff_for_attempt(attempt));
                }
            }
        }
    }
    // Exhaustion: only reachable via 5x Transient (Unsupported/Permanent
    // return early). Returned explicitly to keep the API contract
    // (`attempts = MAX_GOSSIP_ATTEMPTS`) documented in source.
    Err(DeliveryError::GossipFailed {
        attempts: MAX_GOSSIP_ATTEMPTS,
    })
}

/// **Mission 0959-c3:** async variant of `gossip_envelope_to_buyer` that
/// drives the async `CapabilityCatalog::gossip_to_buyer` (now routed
/// through `CapabilityGossip::gossip_to_buyer`) using a tokio runtime
/// budget. Same retry policy as the sync variant; replaces
/// `thread::sleep` with `tokio::time::sleep` so the runtime can drive
/// other futures concurrently.
///
/// Production callers (e.g., the wallet's market-delivery pipeline)
/// should prefer this function when the catalog advertises async
/// gossip (`CapabilityCatalog::implements_gossip()`). The bounded
/// retry loop awaits each attempt + backoff interval before re-driving.
pub async fn gossip_envelope_to_buyer_async(
    env: &MarketDeliveryEnvelope,
    buyer_did: &str,
    catalog: &dyn CapabilityGossip,
) -> Result<(), DeliveryError> {
    // **Round 6 (F35 fix):** see `gossip_envelope_to_buyer` — surfacing
    // serialization error rather than silently gossiping empty bytes.
    let payload = serde_json::to_vec(env).map_err(|e| DeliveryError::SerializationError {
        reason: format!("envelope serialization failed: {e}"),
    })?;
    for attempt in 1..=MAX_GOSSIP_ATTEMPTS {
        match catalog.gossip_to_buyer(buyer_did, &payload).await {
            Ok(()) => return Ok(()),
            Err(CatalogGossipError::Unsupported | CatalogGossipError::Permanent(_)) => {
                return Err(DeliveryError::GossipFailed { attempts: attempt });
            }
            Err(CatalogGossipError::Transient(_)) => {
                if attempt < MAX_GOSSIP_ATTEMPTS {
                    tokio::time::sleep(backoff_for_attempt(attempt)).await;
                }
            }
        }
    }
    Err(DeliveryError::GossipFailed {
        attempts: MAX_GOSSIP_ATTEMPTS,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::super::bearer_capsule_re_export::BearerCapsule;
    use super::super::macaroon::{CapabilityCatalog, Macaroon};
    use super::super::market_delivery::{
        DealSettled, DealSettledPayload, MarketDeliveryEnvelope, RoleTag,
    };
    use super::*;

    struct AlwaysFailCatalog;
    impl CapabilityCatalog for AlwaysFailCatalog {
        fn lookup(&self, _id: &[u8; 32]) -> Option<Macaroon> {
            None
        }
        fn gossip_to_buyer_sync(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            Err(CatalogGossipError::Unsupported)
        }
    }

    struct AlwaysOkCatalog;
    impl CapabilityCatalog for AlwaysOkCatalog {
        fn lookup(&self, _id: &[u8; 32]) -> Option<Macaroon> {
            None
        }
        fn gossip_to_buyer_sync(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            Ok(())
        }
    }

    /// Permanent catalog: every call returns `Permanent`.
    struct AlwaysPermanentCatalog;
    impl CapabilityCatalog for AlwaysPermanentCatalog {
        fn lookup(&self, _id: &[u8; 32]) -> Option<Macaroon> {
            None
        }
        fn gossip_to_buyer_sync(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            Err(CatalogGossipError::Permanent("schema mismatch".into()))
        }
    }

    /// Always-transient catalog: every call returns `Transient`.
    /// Tracks call count for the exhaustion assertion.
    struct AlwaysTransientCatalog {
        calls: AtomicU32,
    }
    impl CapabilityCatalog for AlwaysTransientCatalog {
        fn lookup(&self, _id: &[u8; 32]) -> Option<Macaroon> {
            None
        }
        fn gossip_to_buyer_sync(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(CatalogGossipError::Transient("network unreachable".into()))
        }
    }
    impl AlwaysTransientCatalog {
        fn new() -> Self {
            Self {
                calls: AtomicU32::new(0),
            }
        }
        fn call_count(&self) -> u32 {
            self.calls.load(Ordering::SeqCst)
        }
    }

    fn empty_envelope() -> MarketDeliveryEnvelope {
        MarketDeliveryEnvelope {
            envelope_id: [0xAA; 32],
            bearer: BearerCapsule::new([0x42; 32], vec![], [0x55; 64]),
            capability_token: String::new(),
            deal_settled: DealSettled {
                event_hash: [0x11; 32],
                payload: DealSettledPayload {
                    prev_chain_hash: [0; 32],
                    buyer_did: octo_ident::test_helpers::sample_did(9),
                    seller_did: octo_ident::test_helpers::sample_did(14),
                    ask_id: [0x33; 32],
                    bearer_capsule_hash: [0x42; 32],
                    cap_root_hash: [0x77; 32],
                    settled_at_unix: 1_700_000_000_000,
                    role_tag: RoleTag::TokenIssuer,
                },
                seller_signature: [0x99; 64],
            },
            created_at_unix: 1_700_000_000_000,
        }
    }

    #[test]
    fn gossip_succeeds_on_first_attempt() {
        let env = empty_envelope();
        let result = gossip_envelope_to_buyer(
            &env,
            &octo_ident::test_helpers::sample_did(9),
            &AlwaysOkCatalog,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn gossip_fails_fast_on_unsupported() {
        let env = empty_envelope();
        let result = gossip_envelope_to_buyer(
            &env,
            &octo_ident::test_helpers::sample_did(9),
            &AlwaysFailCatalog,
        );
        assert!(matches!(
            result,
            Err(DeliveryError::GossipFailed { attempts: 1 })
        ));
    }

    #[test]
    fn backoff_for_attempt_caps_at_max() {
        // attempt 1 → INITIAL_BACKOFF (50ms)
        assert_eq!(backoff_for_attempt(1), Duration::from_millis(50));
        // attempt 2 → 100ms
        assert_eq!(backoff_for_attempt(2), Duration::from_millis(100));
        // attempt 3 → 200ms
        assert_eq!(backoff_for_attempt(3), Duration::from_millis(200));
        // attempt 4 → 400ms
        assert_eq!(backoff_for_attempt(4), Duration::from_millis(400));
        // attempt 5 → 800ms (still under MAX_BACKOFF = 2s)
        assert_eq!(backoff_for_attempt(5), Duration::from_millis(800));
        // attempt 100 → saturate to MAX_BACKOFF (2s)
        assert_eq!(backoff_for_attempt(100), MAX_BACKOFF);
    }

    /// **TV4 (RFC-0959-A1):** transient retry — fails first 2 attempts,
    /// succeeds on attempt 3. Verifies backoff consumption + bounded retry.
    #[test]
    fn tv4_transient_retry_succeeds_at_attempt_3() {
        let env = empty_envelope();
        let catalog = AlwaysTransientThenOk::new(2);
        let start = std::time::Instant::now();
        let result =
            gossip_envelope_to_buyer(&env, &octo_ident::test_helpers::sample_did(9), &catalog);
        let elapsed = start.elapsed();
        assert!(
            result.is_ok(),
            "expected Ok(()) on attempt 3, got {result:?}"
        );
        // Calls: 1 (sleep 50ms) + 2 (sleep 100ms) + 3 (Ok, no sleep)
        // = at least 150ms of backoff. Allow generous slack for CI.
        assert!(
            elapsed >= Duration::from_millis(150),
            "expected at least 150ms of backoff; got {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "expected under MAX_BACKOFF total; got {elapsed:?}"
        );
        assert_eq!(catalog.call_count(), 3, "exactly 3 calls expected");
    }

    /// Exhaustion: 5 transient failures → `GossipFailed { attempts: 5 }`.
    /// Verifies the bounded retry + backoff consumption + exhaustion arm.
    #[test]
    fn gossip_exhausts_after_max_transient_attempts() {
        let env = empty_envelope();
        let catalog = AlwaysTransientCatalog::new();
        let start = std::time::Instant::now();
        let result =
            gossip_envelope_to_buyer(&env, &octo_ident::test_helpers::sample_did(9), &catalog);
        let elapsed = start.elapsed();
        assert!(matches!(
            result,
            Err(DeliveryError::GossipFailed { attempts: 5 })
        ));
        assert_eq!(catalog.call_count(), 5, "exactly 5 attempts expected");
        // 4 sleeps between 5 attempts: 50+100+200+400 = 750ms minimum.
        assert!(
            elapsed >= Duration::from_millis(750),
            "expected at least 750ms of backoff; got {elapsed:?}"
        );
        // Last attempt (5th) does NOT sleep; total budget < 50+100+200+400+800 = 1550ms.
        assert!(
            elapsed < Duration::from_secs(2),
            "expected under 2s total; got {elapsed:?}"
        );
    }

    /// Permanent error fails fast at attempt 1 (no retry, no backoff).
    #[test]
    fn permanent_fails_fast_no_retry() {
        let env = empty_envelope();
        let result = gossip_envelope_to_buyer(
            &env,
            &octo_ident::test_helpers::sample_did(9),
            &AlwaysPermanentCatalog,
        );
        assert!(matches!(
            result,
            Err(DeliveryError::GossipFailed { attempts: 1 })
        ));
    }

    /// Manual Debug redacts `Transient` + `Permanent` reason strings.
    #[test]
    fn debug_redacts_transient_and_permanent_reasons() {
        let transient = CatalogGossipError::Transient("peer leaked did:abc:123".into());
        let permanent = CatalogGossipError::Permanent("schema drift v2 vs v3".into());
        let t_dbg = format!("{transient:?}");
        let p_dbg = format!("{permanent:?}");
        assert!(
            !t_dbg.contains("peer leaked did:abc:123"),
            "transient reason MUST be redacted; got {t_dbg}"
        );
        assert!(
            t_dbg.contains("[REDACTED reason]"),
            "transient marker MUST appear; got {t_dbg}"
        );
        assert!(
            !p_dbg.contains("schema drift v2 vs v3"),
            "permanent reason MUST be redacted; got {p_dbg}"
        );
        assert!(
            p_dbg.contains("[REDACTED reason]"),
            "permanent marker MUST appear; got {p_dbg}"
        );
    }

    // ---- helpers ----

    /// Test catalog that fails the first `initial_fail_count` calls with
    /// `Transient`, then succeeds. Uses atomic counter; concurrency-safe.
    struct AlwaysTransientThenOk {
        counter: AtomicU32,
        initial_fail_count: u32,
    }
    impl AlwaysTransientThenOk {
        fn new(initial_fail_count: u32) -> Self {
            Self {
                counter: AtomicU32::new(0),
                initial_fail_count,
            }
        }
        fn call_count(&self) -> u32 {
            self.counter.load(Ordering::SeqCst)
        }
    }
    impl CapabilityCatalog for AlwaysTransientThenOk {
        fn lookup(&self, _id: &[u8; 32]) -> Option<Macaroon> {
            None
        }
        fn gossip_to_buyer_sync(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            if n < self.initial_fail_count {
                Err(CatalogGossipError::Transient("network blip".into()))
            } else {
                Ok(())
            }
        }
    }
}
