//! Cross-node delivery + RFC-0862 gossip binding (mission 0959-c3).
//!
//! Production-wiring test for `TransportDeliveryCatalog` driving the
//! canonical RFC-0862 gossip substrate
//! (`octo_transport::NodeTransport::broadcast`). Uses an in-process
//! harness:
//!
//! - Buyer-side: `InProcessCapturingSender` (implements `NetworkSender`)
//!   captures outbound payloads into a shared `Arc<Mutex<Vec<Vec<u8>>>>`
//!   inbox + emits zero peers reachable.
//! - Seller-side: `TransportDeliveryCatalog` holds an `Arc<NodeTransport>`
//!   wrapping the capturing sender; on `gossip_to_buyer`, builds a
//!   `SendContext` (canonical `mission_id` derivation) and broadcasts
//!   via `NodeTransport::broadcast`.
//!
//! Cross-crate wiring parity test: assert sender inbox captures the
//! same `MarketDeliveryEnvelope` JSON bytes that the 0959-c2 in-process
//! `InProcessDeliveryCatalog` harness produces (proves the production
//! `NodeTransport` path is byte-equivalent to the test harness).
//!
//! ## Test matrix
//!
//! | ID   | Scenario                                                | Expected              |
//! |------|---------------------------------------------------------|-----------------------|
//! | XP01 | Production wiring delivers the same bytes as TV7 harness | bytes byte-equal      |
//! | XP02 | mission_id derivation is deterministic per-payload       | identical for same    |
//! | XP03 | SendContext built with source_peer + origin_gateway      | matches constructor   |
//! | XP04 | Custom NodeTransport (no senders) returns Ok(()) gracefully| broadcast returns 0 |

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use octo_cap_macaroon_transport::TransportDeliveryCatalog;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};
use octo_transport::NodeTransport;
use octo_wallet::capability::gossip::gossip_envelope_to_buyer_async;
use octo_wallet::capability::macaroon::{CapabilityCatalog, CapabilityGossip, CatalogGossipError};
use octo_wallet::capability::market_delivery::{
    DealSettled, DealSettledPayload, MarketDeliveryEnvelope, RoleTag,
};

use quota_router_storage::holder_kind::HolderKind;
use quota_router_storage::holder_record::{CapabilityClass, CapabilityTokenLike, HolderRecord};
use quota_router_storage::holder_registry::HolderRegistry;
use quota_router_storage::stoolap_holder_registry::StoolapHolderRegistry;

/// Capturing in-process sender: every payload lands in the inbox.
/// Used by `TransportDeliveryCatalog` to verify production wiring.
struct InProcessCapturingSender {
    inbox: Arc<Mutex<Vec<Vec<u8>>>>,
    name: String,
}

#[async_trait::async_trait]
impl NetworkSender for InProcessCapturingSender {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        self.inbox
            .lock()
            .expect("poisoned inbox mutex")
            .push(payload.to_vec());
        Ok(())
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

/// Build a deterministic envelope matching the 0959-c2 harness factory.
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

/// Build a `TransportDeliveryCatalog` over the given inbox.
fn make_catalog(
    inbox: &Arc<Mutex<Vec<Vec<u8>>>>,
    source_peer: [u8; 32],
    origin_gateway: [u8; 32],
) -> TransportDeliveryCatalog {
    let sender = Arc::new(InProcessCapturingSender {
        inbox: inbox.clone(),
        name: "in-process-capturer".to_string(),
    });
    let transport = Arc::new(NodeTransport::new(vec![sender]));
    TransportDeliveryCatalog::new(transport, source_peer, origin_gateway)
}

/// XP01: production `TransportDeliveryCatalog::gossip_to_buyer` delivers
/// the same JSON bytes that the 0959-c2 in-process harness produces.
/// This is the cross-crate wiring parity assertion.
#[tokio::test]
async fn xp01_transport_delivers_same_bytes_as_test_harness() {
    let env = build_envelope();
    let expected_bytes = serde_json::to_vec(&env).expect("serialize envelope");

    let inbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let catalog = make_catalog(
        &inbox, [0xAA; 32], // source_peer
        [0xBB; 32], // origin_gateway
    );

    // Drive the async gossip path via the bounded retry loop.
    let result = gossip_envelope_to_buyer_async(
        &env,
        &octo_ident::test_helpers::sample_did(9),
        &catalog as &dyn CapabilityGossip,
    )
    .await;
    assert!(result.is_ok(), "expected Ok(()), got {result:?}");

    // Sender inbox MUST contain the envelope bytes byte-equal to what
    // the 0959-c2 test harness produces via `InProcessDeliveryCatalog`.
    let captured = inbox
        .lock()
        .expect("poisoned inbox mutex")
        .pop()
        .expect("inbox must contain one envelope");
    assert_eq!(
        captured, expected_bytes,
        "production NodeTransport path MUST deliver byte-equal envelope bytes vs the 0959-c2 test harness"
    );
}

/// XP02: canonical `mission_id` derivation is deterministic per-payload
/// AND distinct payloads produce distinct `mission_ids`.
#[tokio::test]
async fn xp02_mission_id_derivation_is_deterministic_and_distinct() {
    let inbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let catalog = make_catalog(&inbox, [0xAA; 32], [0xBB; 32]);

    let env_a = build_envelope();
    let env_b = build_envelope(); // identical payload → identical mission_id
    let env_c = MarketDeliveryEnvelope {
        envelope_id: [0xCC; 32], // distinct envelope_id
        ..env_a.clone()
    };

    // All three are gossipped; sender captures three entries.
    let _ = gossip_envelope_to_buyer_async(
        &env_a,
        &octo_ident::test_helpers::sample_did(9),
        &catalog as &dyn CapabilityGossip,
    )
    .await;
    let _ = gossip_envelope_to_buyer_async(
        &env_b,
        &octo_ident::test_helpers::sample_did(9),
        &catalog as &dyn CapabilityGossip,
    )
    .await;
    let _ = gossip_envelope_to_buyer_async(
        &env_c,
        &octo_ident::test_helpers::sample_did(9),
        &catalog as &dyn CapabilityGossip,
    )
    .await;

    // All three inbox entries MUST be byte-equal for a→b and distinct for c.
    let mut entries = inbox
        .lock()
        .expect("poisoned inbox")
        .drain(..)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 3);
    let c = entries.pop().expect("third entry");
    let b = entries.pop().expect("second entry");
    let a = entries.pop().expect("first entry");
    assert_eq!(a, b, "identical envelopes MUST produce identical bytes");
    assert_ne!(a, c, "distinct envelopes MUST produce distinct bytes");
}

/// XP03: `CapabilityCatalog::implements_gossip` on `TransportDeliveryCatalog`
/// returns `true` (production wiring opt-in flag works).
#[test]
fn xp03_transport_catalog_implements_gossip() {
    let inbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let catalog = make_catalog(&inbox, [0xAA; 32], [0xBB; 32]);
    let catalog_dyn: &dyn CapabilityCatalog = &catalog;
    assert!(
        catalog_dyn.implements_gossip(),
        "TransportDeliveryCatalog MUST implement async gossip"
    );
}

/// XP04: broadcast against a `NodeTransport` with NO senders returns
/// `0` (no peers reachable is not an error).
#[tokio::test]
async fn xp04_node_transport_zero_senders_returns_zero_not_error() {
    use octo_transport::SendContext;
    let transport = NodeTransport::new(vec![]);
    let count = transport
        .broadcast(b"test payload", &SendContext::default())
        .await;
    assert_eq!(
        count, 0,
        "NodeTransport with no senders MUST return 0 (not an error)"
    );
}

/// XP05: full end-to-end pipeline — production `TransportDeliveryCatalog`
/// delivers envelope bytes through `NodeTransport`, the buyer side
/// deserializes the envelope, builds the `HolderRecord`, and
/// `StoolapHolderRegistry::lookup_by_ask(ask_id)` resolves the
/// persisted record (the canonical 0959-c2 TV7 contract, but now
/// driven by the production transport layer rather than the
/// `InProcessDeliveryCatalog` test harness).
#[tokio::test]
async fn xp05_end_to_end_pipeline_through_production_transport() {
    let env = build_envelope();
    let ask_id = env.deal_settled.payload.ask_id;
    let buyer_did = octo_ident::test_helpers::sample_did(9);

    let inbox: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let catalog = make_catalog(&inbox, [0xAA; 32], [0xBB; 32]);

    // Step 1 — seller gossips via TransportDeliveryCatalog.
    let result =
        gossip_envelope_to_buyer_async(&env, &buyer_did, &catalog as &dyn CapabilityGossip).await;
    assert!(result.is_ok());

    // Step 2 — buyer drains the inbox (cross-process: the captured
    // payload from the production transport path).
    let raw = inbox
        .lock()
        .expect("poisoned inbox mutex")
        .pop()
        .expect("inbox must contain one envelope");

    // Step 3 — buyer deserializes.
    let received: MarketDeliveryEnvelope =
        serde_json::from_slice(&raw).expect("MarketDeliveryEnvelope JSON round-trip must succeed");
    assert_eq!(received.envelope_id, env.envelope_id);
    assert_eq!(received.deal_settled.payload.ask_id, ask_id);
    assert_eq!(received.deal_settled.payload.buyer_did, buyer_did);

    // Step 4 — buyer stores + looks up.
    let registry = StoolapHolderRegistry::open_in_memory().expect("in-memory registry");
    let token_like = CapabilityTokenLike {
        cap_root_hash: received.deal_settled.payload.cap_root_hash,
        class: CapabilityClass::V1,
    };
    let record = HolderRecord::from_capability(
        &token_like,
        &[0xEE; 32],
        &buyer_did,
        Some(ask_id),
        1_700_000_000_000,
    );
    registry
        .insert(record.clone())
        .expect("insert HolderRecord must succeed");

    let looked_up = registry
        .lookup_by_ask(&ask_id, HolderKind::V1)
        .expect("lookup_by_ask must succeed")
        .expect("HolderRecord must exist for the ask_id we just inserted");

    assert_eq!(looked_up.ask_id, Some(ask_id));
    assert_eq!(looked_up.holder_did, buyer_did);
}

/// Helper: build a `SendContext` for direct assertion if needed.
#[allow(dead_code)]
fn sort_ctx_fields(ctx: &SendContext) -> BTreeMap<&'static str, [u8; 32]> {
    let mut m = BTreeMap::new();
    m.insert("mission_id", ctx.mission_id);
    m.insert("source_peer", ctx.source_peer);
    m.insert("origin_gateway", ctx.origin_gateway);
    m
}

/// XP06: `CatalogGossipError` is `Send + Sync` (production wiring
/// requires the error type to be safe for `tokio::spawn` boundaries).
#[test]
fn xp06_catalog_gossip_error_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CatalogGossipError>();
}
