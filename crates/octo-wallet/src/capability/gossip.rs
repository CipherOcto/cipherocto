// RFC-0959-A1 §Phase 3: gossip retry loop + cross-node delivery.
//
// Wraps `CapabilityCatalog::gossip_to_buyer` in a bounded retry loop to
// handle Finding A11 (gossip partition → envelope not received).
// Exhaustion emits `DeliveryError::GossipFailed { attempts }`.

use std::time::Duration;

use super::market_delivery::{DeliveryError, MarketDeliveryEnvelope};
use super::macaroon::{CapabilityCatalog, CatalogGossipError};

/// Maximum gossip retry attempts (RFC-0959-A1 §Future Work F5).
pub const MAX_GOSSIP_ATTEMPTS: u32 = 5;

/// Initial backoff delay (doubles each retry, capped at MAX_BACKOFF).
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(50);
pub const MAX_BACKOFF: Duration = Duration::from_secs(2);

/// Gossip `MarketDeliveryEnvelope` to the buyer via `CapabilityCatalog`.
/// Bounded retry with exponential backoff; exhaustion → `GossipFailed`.
pub fn gossip_envelope_to_buyer(
    env: &MarketDeliveryEnvelope,
    buyer_did: &str,
    catalog: &dyn CapabilityCatalog,
) -> Result<(), DeliveryError> {
    let payload = serde_json::to_vec(env).unwrap_or_default();
    let mut backoff = INITIAL_BACKOFF;
    for attempt in 1..=MAX_GOSSIP_ATTEMPTS {
        match catalog.gossip_to_buyer(buyer_did, &payload) {
            Ok(()) => return Ok(()),
            Err(CatalogGossipError::Unsupported) => {
                // Catalog doesn't support gossip — fail fast (no retry).
                return Err(DeliveryError::GossipFailed { attempts: attempt });
            }
        }
        if attempt < MAX_GOSSIP_ATTEMPTS {
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }
    Err(DeliveryError::GossipFailed {
        attempts: MAX_GOSSIP_ATTEMPTS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::macaroon::CapabilityCatalog;
    use super::super::market_delivery::{
        DealSettled, DealSettledPayload, MarketDeliveryEnvelope, RoleTag,
    };
    use super::super::bearer_capsule_re_export::BearerCapsule;

    struct AlwaysFailCatalog;
    impl CapabilityCatalog for AlwaysFailCatalog {
        fn get(&self, _id: &[u8; 32]) -> Option<&super::super::macaroon::Macaroon> {
            None
        }
        fn gossip_to_buyer(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            Err(CatalogGossipError::Unsupported)
        }
    }

    struct AlwaysOkCatalog;
    impl CapabilityCatalog for AlwaysOkCatalog {
        fn get(&self, _id: &[u8; 32]) -> Option<&super::super::macaroon::Macaroon> {
            None
        }
        fn gossip_to_buyer(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            Ok(())
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
                    buyer_did: "did:octo:buyer".into(),
                    seller_did: "did:octo:seller".into(),
                    ask_id: [0x33; 32],
                    bearer_capsule_hash: [0x42; 32],
                    cap_root_hash: [0x77; 32],
                    settled_at_unix: 1_700_000_000_000,
                    role_tag: RoleTag::Seller,
                },
                seller_signature: [0x99; 64],
            },
            created_at_unix: 1_700_000_000_000,
        }
    }

    #[test]
    fn gossip_succeeds_on_first_attempt() {
        let env = empty_envelope();
        let result = gossip_envelope_to_buyer(&env, "did:octo:buyer", &AlwaysOkCatalog);
        assert!(result.is_ok());
    }

    #[test]
    fn gossip_fails_fast_on_unsupported() {
        let env = empty_envelope();
        let result = gossip_envelope_to_buyer(&env, "did:octo:buyer", &AlwaysFailCatalog);
        assert!(matches!(
            result,
            Err(DeliveryError::GossipFailed { attempts: 1 })
        ));
    }
}
