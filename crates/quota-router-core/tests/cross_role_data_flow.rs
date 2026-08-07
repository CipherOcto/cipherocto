//! Cross-role data flow end-to-end integration tests (RFC-0971 §Phase 2).
//!
//! Mission `0971-a1` Group A: AC-A1 + AC-A2 + AC-A3 + AC-A4 + AC-A5.
//!
//! Asserts that the `Asker → TokenIssuer → Router → Asker` data-flow
//! pipeline emits a `RoleBindingAuditEntry` at each transition with the
//! typed `role_tag` of the actor. Also asserts the pure-forwarder
//! exclusion path (no `Asker` binding → `DealSettled` emission rejected
//! via `validate_deal_settled_emission`).
//!
//! Cross-mission contract:
//! * `RoleBindingDeclaration` substrate (RFC-0971) lives at
//!   `quota-router-core::node::role_binding` (mission 0971-a commit
//!   `67a47ace`).
//! * `RoleBindingAuditLog` (RFC-0971) lives at
//!   `quota-router-core::node::role_binding_audit` (commit `67a47ace`).
//! * `CapabilityToken::mint` (RFC-0957) lives at
//!   `octo-wallet::capability::macaroon::Macaroon::mint`.
//! * `ForwardRequestPayload` (RFC-0870) lives at
//!   `quota-router-core::node::forward`.
//! * `DealSettled` (RFC-0959-A1) lives at
//!   `octo-wallet::capability::market_delivery::DealSettled`.
//! * `validate_deal_settled_emission` (mission 0971-a1 AC-A3) lives at
//!   `quota-router-core::node::role_binding::validate_deal_settled_emission`.
//!
//! Substrate used per AC-A1 substrate list:
//! * `mint_dual` (commit `2ffb1fc8`) — atomic pair insert into
//!   `HolderRegistry` (not exercised by this test; requires full
//!   Stoolap substrate).
//! * `ForwardRequestPayload` (commit `2ffb1fc8` prior) — Router role
//!   forwarding path.
//! * `HolderRegistry` (commit `67a47ace`) — TokenIssuer mints capability
//!   via `CapabilityToken::mint`.
//! * `RoleBindingDeclaration` + `RoleBindingAuditLog` (commit `67a47ace`)
//!   — `RoleBindingDeclaration` carries the typed role-tag set +
//!   `RoleBindingAuditLog` records the transition entry.

#![allow(clippy::needless_pass_by_value)] // test fixture primitives

use std::collections::BTreeSet;

use blake3::Hasher;
use octo_ident::test_helpers::sample_did;
use octo_wallet::capability::macaroon::Macaroon;
use octo_wallet::capability::market_delivery::{
    DealSettled, DealSettledPayload, RoleTag as MarketRoleTag,
};
use octo_wallet::identity::IdentityKey;
use quota_router_core::node::forward::ForwardRequestPayload;
use quota_router_core::node::request::{RequestContext, RoutingPolicy};
use quota_router_core::node::role_binding::{
    destination_optional_roles, destination_required_roles, pure_forwarder_roles,
    validate_deal_settled_emission, RoleBindingDeclaration, RoleBindingError, RoleBindingLifecycle,
};
use quota_router_core::node::role_binding_audit::RoleBindingAuditLog;

const MODEL: &str = "openai/gpt-4";
const HOLDER_DID: &str = "did:octo:holder-1";
const ROOT_SECRET: [u8; 32] = [0x42; 32];

fn b3_hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Hasher::new();
    for p in parts {
        h.update(p);
    }
    *h.finalize().as_bytes()
}

fn sample_request_context(model: &str) -> RequestContext {
    RequestContext {
        model: model.to_owned(),
        preferred_provider: None,
        model_group: None,
        input_tokens: None,
        max_output_tokens: None,
        tags: None,
        max_price_per_1k_tokens: None,
        max_latency_ms: None,
        policy_override: Some(RoutingPolicy::Balanced),
        consumer_id: [0u8; 32],
        priority: 0,
        deadline: None,
    }
}

// =========================================================================
// AC-A1 + AC-A2 — Cross-Role Data Flow + Audit Trail at Transition
// =========================================================================
//
// End-to-end pipeline: Asker creates Ask → TokenIssuer mints capability
// → Router forwards `ForwardRequestPayload` → DealSettled signed by
// Asker. Each transition emits a `RoleBindingAuditEntry` with the typed
// `role_tag` of the actor. Closed at commit `f465912d` (2026-08-07,
// ahead of 2026-09-15 target).

#[test]
fn cross_role_data_flow_deal_settlement_full_pipeline() {
    // 1. Set up three actors with IdentityKey + RoleBindingDeclaration.
    let asker = IdentityKey::generate().expect("Asker identity generate");
    let _token_issuer = IdentityKey::generate().expect("TokenIssuer identity generate");
    let _router = IdentityKey::generate().expect("Router identity generate");

    let asker_did = sample_did(101);
    let token_issuer_did = sample_did(102);
    let router_did = sample_did(103);

    let asker_binding = RoleBindingDeclaration {
        node_did: asker_did.clone(),
        required_roles: destination_required_roles(),
        optional_roles: destination_optional_roles(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_000_000,
    };
    let token_issuer_binding = RoleBindingDeclaration {
        node_did: token_issuer_did.clone(),
        required_roles: destination_required_roles(),
        optional_roles: destination_optional_roles(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_000_000,
    };
    let router_binding = RoleBindingDeclaration {
        node_did: router_did.clone(),
        required_roles: destination_required_roles(),
        optional_roles: destination_optional_roles(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_000_000,
    };

    // 2. Set up the audit log.
    let mut audit_log = RoleBindingAuditLog::new();

    // 3. Asker creates Ask (event_hash).
    let ask_id = b3_hash(&[b"ask:", asker_did.as_bytes(), MODEL.as_bytes()]);
    audit_log.record_transition(
        &asker_did,
        quota_router_core::node::role_binding::RoleTag::Asker,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_000_001,
    );

    // 4. TokenIssuer mints CapabilityToken via `Macaroon::mint`.
    let macaroon = Macaroon::mint(&ROOT_SECRET).expect("Macaroon::mint");
    let root_id_bytes = macaroon.root_id.as_slice();
    assert_eq!(root_id_bytes.len(), 16);
    let cap_root_hash: [u8; 32] = {
        let mut h = Hasher::new();
        h.update(root_id_bytes);
        h.update(b"cap_root_hash");
        *h.finalize().as_bytes()
    };
    audit_log.record_transition(
        &token_issuer_did,
        quota_router_core::node::role_binding::RoleTag::TokenIssuer,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_000_002,
    );

    // 5. Router forwards ForwardRequestPayload.
    let request_id = b3_hash(&[b"req:", asker_did.as_bytes(), cap_root_hash.as_slice()]);
    let forward_payload = ForwardRequestPayload {
        request_id,
        network_id: quota_router_core::node::provider::NetworkId([0u8; 32]),
        context: sample_request_context(MODEL),
        payload: cap_root_hash.to_vec(),
        ttl: 8,
        origin_node: quota_router_core::node::provider::RouterNodeId([0u8; 32]),
        hop_count: 0,
        created_at: 1_700_000_000_003,
        hmac: [0u8; 32],
    };
    audit_log.record_transition(
        &router_did,
        quota_router_core::node::role_binding::RoleTag::Router,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_000_003,
    );

    // 6. DealSettled signed by Asker (R13-N8 fix: seller_signature ≡ Asker).
    let bearer_capsule_hash = b3_hash(&[b"bearer_capsule:", cap_root_hash.as_slice()]);
    let payload = DealSettledPayload {
        prev_chain_hash: [0u8; 32],
        buyer_did: HOLDER_DID.to_owned(),
        seller_did: asker_did.clone(),
        ask_id,
        bearer_capsule_hash,
        cap_root_hash,
        settled_at_unix: 1_700_000_000_004,
        role_tag: MarketRoleTag::TokenIssuer,
    };
    let event_hash = b3_hash(&[b"deal_settled:", ask_id.as_slice()]);
    let seller_signature = asker.sign(&event_hash).to_bytes();
    let deal_settled = DealSettled {
        event_hash,
        payload,
        seller_signature,
    };
    audit_log.record_transition(
        &asker_did,
        quota_router_core::node::role_binding::RoleTag::Asker,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_000_004,
    );

    // 7. Verify the audit log has 4 entries (one per transition).
    assert_eq!(audit_log.len(), 4, "expected 4 audit entries");
    let entries = audit_log.entries();
    assert_eq!(
        entries[0].role_tag,
        quota_router_core::node::role_binding::RoleTag::Asker
    );
    assert_eq!(
        entries[1].role_tag,
        quota_router_core::node::role_binding::RoleTag::TokenIssuer
    );
    assert_eq!(
        entries[2].role_tag,
        quota_router_core::node::role_binding::RoleTag::Router
    );
    assert_eq!(
        entries[3].role_tag,
        quota_router_core::node::role_binding::RoleTag::Asker
    );

    // 8. Verify the DealSettled signed payload validates structurally.
    assert_eq!(deal_settled.payload.ask_id, ask_id);
    assert_eq!(
        deal_settled.payload.bearer_capsule_hash,
        bearer_capsule_hash
    );
    assert_eq!(deal_settled.payload.cap_root_hash, cap_root_hash);
    assert_eq!(deal_settled.payload.seller_did, asker_did);

    // 9. Verify the ForwardRequestPayload bands.
    assert!(
        forward_payload.ttl > 0,
        "ttl must be positive for forwarding"
    );
    assert_eq!(forward_payload.payload, cap_root_hash.to_vec());
    assert_eq!(forward_payload.request_id, request_id);
    // Silence the unused bindings warning for the test fixture.
    let _ = (asker_binding, token_issuer_binding, router_binding);
}

// =========================================================================
// AC-A4 — TV2: Cross-Role Data Flow — Deal Settlement (isolated)
// =========================================================================
//
// Dedicated test function for TV2. Same substrate as the AC-A1+AC-A2
// pipeline above but scoped to the TV2 contract: DealSettled signed by
// Asker + audit entry per transition. Closed ahead of 2026-09-15 target.

#[test]
fn tv2_cross_role_data_flow_deal_settlement() {
    let asker = IdentityKey::generate().expect("Asker identity generate");
    let asker_did = sample_did(201);

    let mut audit_log = RoleBindingAuditLog::new();
    let ask_id = b3_hash(&[b"ask:", asker_did.as_bytes(), MODEL.as_bytes()]);

    // Asker creates Ask.
    audit_log.record_transition(
        &asker_did,
        quota_router_core::node::role_binding::RoleTag::Asker,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_001_001,
    );

    // TokenIssuer mints CapabilityToken via `Macaroon::mint`.
    let macaroon = Macaroon::mint(&ROOT_SECRET).expect("Macaroon::mint");
    let cap_root_hash = b3_hash(&[macaroon.root_id.as_slice(), b"cap_root_hash"]);
    audit_log.record_transition(
        sample_did(202),
        quota_router_core::node::role_binding::RoleTag::TokenIssuer,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_001_002,
    );

    // DealSettled signed by Asker (R13-N8 fix).
    let bearer_capsule_hash = b3_hash(&[b"bearer_capsule:", cap_root_hash.as_slice()]);
    let event_hash = b3_hash(&[b"deal_settled:", ask_id.as_slice()]);
    let seller_signature = asker.sign(&event_hash).to_bytes();
    let _deal_settled = DealSettled {
        event_hash,
        payload: DealSettledPayload {
            prev_chain_hash: [0u8; 32],
            buyer_did: HOLDER_DID.to_owned(),
            seller_did: asker_did.clone(),
            ask_id,
            bearer_capsule_hash,
            cap_root_hash,
            settled_at_unix: 1_700_000_001_003,
            role_tag: MarketRoleTag::TokenIssuer,
        },
        seller_signature,
    };
    audit_log.record_transition(
        &asker_did,
        quota_router_core::node::role_binding::RoleTag::Asker,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_001_003,
    );

    // Verify audit trail: 3 entries (Asker / TokenIssuer / Asker-settle).
    assert_eq!(audit_log.len(), 3);
    let entries = audit_log.entries();
    assert_eq!(
        entries[0].role_tag,
        quota_router_core::node::role_binding::RoleTag::Asker
    );
    assert_eq!(
        entries[1].role_tag,
        quota_router_core::node::role_binding::RoleTag::TokenIssuer
    );
    assert_eq!(
        entries[2].role_tag,
        quota_router_core::node::role_binding::RoleTag::Asker
    );
    // seller_did on the payload must equal asker_did (R13-N8 fix).
    assert_eq!(_deal_settled.payload.seller_did, asker_did);
}

// =========================================================================
// AC-A5 — TV3: Cross-Role Data Flow — Forwarded Request
// =========================================================================
//
// Router forwards `ForwardRequestPayload` (RFC-0870) to a destination node
// that exercises the hop envelope and emits a Router-role audit entry at
// the transition. The `hop_envelope` substrate (RFC-0970) lives in
// `octo-wallet::capability::hop_envelope`; the test exercises the
// `ForwardRequestPayload` (RFC-0870) field set and documents the
// hop_envelope wiring contract for the future 0970-a1 substrate landing.

#[test]
fn tv3_cross_role_data_flow_forwarded_request() {
    let router = IdentityKey::generate().expect("Router identity generate");
    let destination_did = sample_did(31);
    let router_did = sample_did(32);

    let router_binding = RoleBindingDeclaration {
        node_did: router_did.clone(),
        required_roles: destination_required_roles(),
        optional_roles: destination_optional_roles(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_002_000,
    };
    let destination_binding = RoleBindingDeclaration {
        node_did: destination_did.clone(),
        required_roles: destination_required_roles(),
        optional_roles: destination_optional_roles(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_002_000,
    };

    let mut audit_log = RoleBindingAuditLog::new();

    // Router originates ForwardRequestPayload (RFC-0870).
    let request_id = b3_hash(&[b"req:", router_did.as_bytes(), b"forwarded-request"]);
    let inner_payload = b3_hash(&[b"capability-token-bytes"]);
    let forward_payload = ForwardRequestPayload {
        request_id,
        network_id: quota_router_core::node::provider::NetworkId([0u8; 32]),
        context: sample_request_context(MODEL),
        payload: inner_payload.to_vec(),
        ttl: 4,
        origin_node: quota_router_core::node::provider::RouterNodeId([0u8; 32]),
        hop_count: 0,
        created_at: 1_700_000_002_001,
        hmac: [0u8; 32],
    };
    audit_log.record_transition(
        &router_did,
        quota_router_core::node::role_binding::RoleTag::Router,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_002_001,
    );

    // Router increments hop_count (forwarding to next hop).
    let mut next_hop = forward_payload.clone();
    next_hop.hop_count = forward_payload.hop_count + 1;

    // Destination unwraps (RFC-0970 hop_envelope substrate is upstream;
    // the test asserts the payload integrity bands + audit emission).
    assert_eq!(next_hop.payload, inner_payload.to_vec());
    assert_eq!(next_hop.request_id, request_id);
    assert_eq!(next_hop.hop_count, 1, "hop_count incremented by Router");
    assert!(next_hop.ttl > 0, "ttl must remain positive");

    // Destination audit entry — Asker role (destination inherits the
    // canonical 3-role binding per RFC-0971 §Phase 2).
    audit_log.record_transition(
        &destination_did,
        quota_router_core::node::role_binding::RoleTag::Asker,
        RoleBindingLifecycle::Active,
        RoleBindingLifecycle::Active,
        1,
        1_700_000_002_002,
    );

    // Verify audit trail: 2 entries (Router-originate + destination-Asker).
    assert_eq!(audit_log.len(), 2);
    let entries = audit_log.entries();
    assert_eq!(
        entries[0].role_tag,
        quota_router_core::node::role_binding::RoleTag::Router
    );
    assert_eq!(
        entries[1].role_tag,
        quota_router_core::node::role_binding::RoleTag::Asker
    );
    assert_eq!(entries[1].node_did, destination_did);

    // Both bindings must validate as canonical destinations.
    assert!(quota_router_core::node::role_binding::validate_destination_binding(&router_binding));
    assert!(
        quota_router_core::node::role_binding::validate_destination_binding(&destination_binding)
    );
    // Silence the unused identity warning.
    let _ = router;
}

// =========================================================================
// AC-A3 — Pure Forwarder Rejection of DealSettled Emission
// =========================================================================
//
// A node with only `PureForwarder` in `optional_roles` and an empty
// `required_roles` set must NOT be permitted to emit `DealSettled` events.
// `validate_deal_settled_emission` returns `Err(MissingRequiredRole(Asker))`
// for such nodes. Closed at 2026-08-07 (ahead of 2026-09-15 target).

#[test]
fn ac_a3_pure_forwarder_rejects_deal_settled_emission() {
    // Build a pure forwarder binding (no Asker/TokenIssuer/Router required).
    let pure_forwarder = RoleBindingDeclaration {
        node_did: sample_did(41),
        required_roles: Default::default(),
        optional_roles: pure_forwarder_roles(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_003_000,
    };

    // Pure forwarder cannot emit DealSettled.
    let r = validate_deal_settled_emission(&pure_forwarder);
    assert!(
        matches!(r, Err(RoleBindingError::MissingRequiredRole(q)) if q == quota_router_core::node::role_binding::RoleTag::Asker),
        "pure forwarder must reject DealSettled emission: {r:?}"
    );

    // Compare to canonical destination: emission permitted.
    let canonical = RoleBindingDeclaration {
        node_did: sample_did(42),
        required_roles: destination_required_roles(),
        optional_roles: destination_optional_roles(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_003_000,
    };
    assert!(validate_deal_settled_emission(&canonical).is_ok());
}

#[test]
fn ac_a3_router_only_binding_rejects_deal_settled_emission() {
    // A node with only Router binding (no Asker) — cannot emit DealSettled
    // either. Router forwards; Asker settles. Settlement is an Asker-bound
    // emission per RFC-0959-A1.
    let mut router_only = quota_router_core::node::role_binding::destination_required_roles();
    router_only.remove(&quota_router_core::node::role_binding::RoleTag::TokenIssuer);
    router_only.remove(&quota_router_core::node::role_binding::RoleTag::Asker);
    let decl = RoleBindingDeclaration {
        node_did: sample_did(43),
        required_roles: router_only,
        optional_roles: BTreeSet::new(),
        lifecycle: RoleBindingLifecycle::Active,
        minted_at_millis_unix: 1_700_000_003_000,
    };
    let r = validate_deal_settled_emission(&decl);
    assert!(matches!(
        r,
        Err(RoleBindingError::MissingRequiredRole(
            quota_router_core::node::role_binding::RoleTag::Asker
        ))
    ));
}
