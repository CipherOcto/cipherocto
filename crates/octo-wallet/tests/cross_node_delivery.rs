//! Cross-node delivery test (mission 0959-c2, TV7).
//!
//! Asserts the full pipeline: seller builds `MarketDeliveryEnvelope`,
//! `gossip_envelope_to_buyer` retries via `CapabilityCatalog`, the envelope
//! bytes land in the buyer's inbox, the buyer deserializes, looks the
//! envelope up via `StoolapHolderRegistry::lookup_by_ask(ask_id)`, and
//! confirms the persisted record's `holder_did` matches the buyer DID
//! stamped on the envelope.
//!
//! ## Production-wiring gap (deferred)
//!
//! In production, `CapabilityCatalog::gossip_to_buyer` delegates to the
//! canonical `octo_transport::NodeTransport` (per RFC-0862 gossip binding
//! called out by mission `0959-c2` AC "RFC-0862 gossip binding"). Wallet
//! cannot import `octo-transport` directly (avoids dep inversion per
//! [[stoolap-general-purpose-db]]); the production wiring is a future
//! `CapabilityCatalog` impl that holds an `Arc<NodeTransport>` and
//! implements `gossip_to_buyer` as `transport.broadcast(...)`. That wiring
//! is tracked by follow-up `0959-c3-octo-transport-wiring` (per
//! [[deferred-vs-unspecified]] named-owner rule).
//!
//! This test exercises the retry loop + envelope serialization + buyer-side
//! registry lookup end-to-end against an in-process harness that simulates
//! the cross-node gossip channel. The harness is hermetic (no real network)
//! and CI-deterministic.

use std::sync::{Arc, Mutex};

use octo_wallet::capability::gossip::gossip_envelope_to_buyer;
use octo_wallet::capability::macaroon::{CapabilityCatalog, CatalogGossipError};
use octo_wallet::capability::market_delivery::{
    DealSettled, DealSettledPayload, MarketDeliveryEnvelope, RoleTag,
};

use quota_router_storage::holder_kind::HolderKind;
use quota_router_storage::holder_record::{CapabilityClass, CapabilityTokenLike, HolderRecord};
use quota_router_storage::holder_registry::HolderRegistry;
use quota_router_storage::stoolap_holder_registry::StoolapHolderRegistry;

/// Buyer DID used throughout the test (canonical RFC-0968 DID format).
fn buyer_did() -> String {
    octo_ident::test_helpers::sample_did(9)
}

/// Seller DID used throughout the test.
fn seller_did() -> String {
    octo_ident::test_helpers::sample_did(14)
}

/// Build a deterministic `MarketDeliveryEnvelope` with a fixed `ask_id`
/// for `lookup_by_ask` testing.
fn build_envelope() -> MarketDeliveryEnvelope {
    MarketDeliveryEnvelope {
        envelope_id: [0xAA; 32],
        bearer: octo_wallet::capability::bearer_capsule_re_export::BearerCapsule::new(
            [0x42; 32],
            vec![],
            [0x55; 64],
        ),
        capability_token: String::new(),
        deal_settled: DealSettled {
            event_hash: [0x11; 32],
            payload: DealSettledPayload {
                prev_chain_hash: [0; 32],
                buyer_did: buyer_did(),
                seller_did: seller_did(),
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

/// In-process catalog that simulates the cross-node gossip channel.
///
/// On `gossip_to_buyer`, pushes the serialized envelope into a shared
/// inbox that the buyer side drains. This is the test-harness equivalent
/// of `octo_transport::NodeTransport::broadcast()` — the production
/// `CapabilityCatalog` impl will hold an `Arc<NodeTransport>` instead of
/// the inbox (per `0959-c3` follow-up).
struct InProcessDeliveryCatalog {
    inbox: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl CapabilityCatalog for InProcessDeliveryCatalog {
    fn lookup(&self, _id: &[u8; 32]) -> Option<octo_wallet::capability::macaroon::Macaroon> {
        None
    }

    fn gossip_to_buyer_sync(&self, _buyer_did: &str, env: &[u8]) -> Result<(), CatalogGossipError> {
        self.inbox
            .lock()
            .expect("poisoned inbox mutex")
            .push(env.to_vec());
        Ok(())
    }
}

/// **TV7 (mission 0959-c2):** seller builds envelope, gossips via
/// `CapabilityCatalog`, buyer drains inbox, deserializes envelope, looks
/// up the `HolderRecord` via `StoolapHolderRegistry::lookup_by_ask(ask_id)`,
/// and asserts the persisted record's `holder_did` matches the buyer DID
/// stamped on the envelope (canonical end-to-end: sync engine detects →
/// gossip retries → buyer inbox → deserialize → registry lookup).
#[test]
fn tv7_cross_node_delivery_envelope_to_registry_lookup() {
    let env = build_envelope();
    let ask_id = env.deal_settled.payload.ask_id;
    let buyer = buyer_did();

    // Shared inbox = the cross-node gossip channel.
    let inbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let catalog = InProcessDeliveryCatalog {
        inbox: inbox.clone(),
    };

    // Step 1 — seller gossips the envelope to the buyer (bounded retry
    // loop; catalog always succeeds in this harness so first attempt).
    let result = gossip_envelope_to_buyer(&env, &buyer, &catalog);
    assert!(
        result.is_ok(),
        "expected Ok(()) on first gossip attempt, got {result:?}"
    );

    // Step 2 — buyer drains the inbox.
    let raw = inbox
        .lock()
        .expect("poisoned inbox mutex")
        .pop()
        .expect("inbox must contain one envelope");
    assert!(
        !raw.is_empty(),
        "envelope bytes must not be empty (RFC-0959-A1 §Phase 3 wire format)"
    );

    // Step 3 — buyer deserializes the envelope.
    let received: MarketDeliveryEnvelope =
        serde_json::from_slice(&raw).expect("MarketDeliveryEnvelope JSON round-trip must succeed");

    // TV7 AC: canonical bytes identical (envelope_id, ask_id, holder_did
    // stamp). The seller's envelope must reach the buyer byte-for-byte.
    assert_eq!(received.envelope_id, env.envelope_id);
    assert_eq!(
        received.deal_settled.payload.ask_id, ask_id,
        "ask_id must round-trip across gossip"
    );
    assert_eq!(
        received.deal_settled.payload.buyer_did, buyer,
        "buyer_did must round-trip across gossip"
    );
    assert_eq!(
        received.deal_settled.payload.seller_did,
        seller_did(),
        "seller_did must round-trip across gossip"
    );

    // Step 4 — buyer stores the envelope and looks it up via the
    // canonical HolderRegistry. The persisted record's holder_did is
    // stamped from the buyer's perspective (the audience for a
    // Bearer/V1 capability).
    let registry = StoolapHolderRegistry::open_in_memory().expect("in-memory registry");
    let token_like = CapabilityTokenLike {
        cap_root_hash: received.deal_settled.payload.cap_root_hash,
        class: CapabilityClass::V1,
    };
    let buyer_holder_pub = [0xEE; 32]; // buyer's holder public key (test sentinel)
    let record = HolderRecord::from_capability(
        &token_like,
        &buyer_holder_pub,
        &buyer,
        Some(ask_id),
        1_700_000_000_000,
    );
    registry
        .insert(record.clone())
        .expect("insert HolderRecord must succeed");

    // Step 5 — buyer looks up by ask_id; canonical lookup path matches.
    let looked_up = registry
        .lookup_by_ask(&ask_id, HolderKind::V1)
        .expect("lookup_by_ask must succeed");
    let looked_up = looked_up.expect("HolderRecord must exist for the ask_id we just inserted");

    // TV7 AC: lookup result matches what the seller built.
    assert_eq!(looked_up.cap_root_hash, record.cap_root_hash);
    assert_eq!(looked_up.ask_id, Some(ask_id));
    assert_eq!(
        looked_up.holder_did, buyer,
        "holder_did MUST be the buyer DID stamped on the envelope"
    );
    assert_eq!(looked_up.holder_pub, buyer_holder_pub);
    assert_eq!(looked_up.ttl_millis_unix, 1_700_000_000_000);
}

/// TV7 extension: seller retries on `Transient` errors. The harness
/// flips a switch after the first failure so the second attempt
/// succeeds. Verifies the bounded retry loop + cross-node resilience
/// (the canonical Finding A11 fix from `0959-c1-gossip-error-variants`).
struct FlakyDeliveryCatalog {
    inbox: Arc<Mutex<Vec<Vec<u8>>>>,
    fail_count: Mutex<u32>,
    initial_failures: u32,
}

impl CapabilityCatalog for FlakyDeliveryCatalog {
    fn lookup(&self, _id: &[u8; 32]) -> Option<octo_wallet::capability::macaroon::Macaroon> {
        None
    }

    fn gossip_to_buyer_sync(&self, _buyer_did: &str, env: &[u8]) -> Result<(), CatalogGossipError> {
        let mut count = self.fail_count.lock().expect("poisoned fail_count mutex");
        if *count < self.initial_failures {
            *count += 1;
            return Err(CatalogGossipError::Transient("network blip".into()));
        }
        self.inbox
            .lock()
            .expect("poisoned inbox mutex")
            .push(env.to_vec());
        Ok(())
    }
}

#[test]
fn tv7_cross_node_delivery_survives_transient_retry() {
    let env = build_envelope();
    let buyer = buyer_did();
    let inbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let catalog = FlakyDeliveryCatalog {
        inbox: inbox.clone(),
        fail_count: Mutex::new(0),
        initial_failures: 2, // fails twice, succeeds on 3rd attempt
    };

    // First 2 attempts return Transient → bounded retry with backoff.
    // 3rd attempt succeeds → envelope lands in buyer inbox.
    let result = gossip_envelope_to_buyer(&env, &buyer, &catalog);
    assert!(result.is_ok(), "expected Ok(()) after transient retry");

    let raw = inbox
        .lock()
        .expect("poisoned inbox mutex")
        .pop()
        .expect("inbox must contain one envelope");
    let received: MarketDeliveryEnvelope = serde_json::from_slice(&raw).unwrap();
    assert_eq!(received.envelope_id, env.envelope_id);
    assert_eq!(received.deal_settled.payload.buyer_did, buyer);
}

/// TV7 cross-check: the canonical `lookup_by_ask` path on the buyer
/// side resolves a record whose `ask_id` matches the envelope's
/// `deal_settled.payload.ask_id`. Mirrors the `0957-c-holder-registry`
/// substrate contract end-to-end.
#[test]
fn tv7_lookup_by_ask_resolves_envelope_ask_id() {
    let env = build_envelope();
    let ask_id = env.deal_settled.payload.ask_id;

    let registry = StoolapHolderRegistry::open_in_memory().unwrap();
    let token_like = CapabilityTokenLike {
        cap_root_hash: env.deal_settled.payload.cap_root_hash,
        class: CapabilityClass::V1,
    };
    let record = HolderRecord::from_capability(
        &token_like,
        &[0xEE; 32],
        &buyer_did(),
        Some(ask_id),
        1_700_000_000_000,
    );
    registry.insert(record.clone()).unwrap();

    let resolved = registry
        .lookup_by_ask(&ask_id, HolderKind::V1)
        .expect("lookup_by_ask must succeed")
        .expect("HolderRecord must exist");

    assert_eq!(
        resolved.cap_root_hash,
        env.deal_settled.payload.cap_root_hash
    );
    assert_eq!(resolved.ask_id, Some(ask_id));
}

/// TV7 negative: a different `ask_id` does NOT resolve. Confirms the
/// buyer-side registry is keyed canonically (no false-positive hits).
#[test]
fn tv7_lookup_by_ask_rejects_unrelated_ask_id() {
    let registry = StoolapHolderRegistry::open_in_memory().unwrap();
    let stored_ask_id = [0x33; 32];
    let other_ask_id = [0xFF; 32];

    let token_like = CapabilityTokenLike {
        cap_root_hash: [0x77; 32],
        class: CapabilityClass::V1,
    };
    let record = HolderRecord::from_capability(
        &token_like,
        &[0xEE; 32],
        &buyer_did(),
        Some(stored_ask_id),
        1_700_000_000_000,
    );
    registry.insert(record).unwrap();

    let resolved_other = registry
        .lookup_by_ask(&other_ask_id, HolderKind::V1)
        .unwrap();
    assert!(
        resolved_other.is_none(),
        "unrelated ask_id MUST NOT resolve; canonical keying"
    );
}
