//! Integration tests for Onion Relay Routing (ORR).
//!
//! Tests the full onion lifecycle: route construction → hop key derivation →
//! session key derivation → nonce/MAC computation → route commitment.

use octo_network::orr::session::{compute_hop_mac, derive_hop_nonce, derive_hop_session_key};
use octo_network::orr::types::{
    OnionRoute, RouteCommitment,
    ROUTE_FLAG_COVER, ROUTE_FLAG_MISSION_SCOPED, ROUTE_FLAG_STEALTH,
    TransportVector,
};

// ── Onion Route lifecycle ──

#[test]
fn test_onion_route_derive_id_deterministic() {
    let id1 = OnionRoute::derive_route_id(
        &[0xAA; 32], 100, 3, &[0x01; 32], &[0x02; 32], 1000,
    );
    let id2 = OnionRoute::derive_route_id(
        &[0xAA; 32], 100, 3, &[0x01; 32], &[0x02; 32], 1000,
    );
    assert_eq!(id1, id2);
    assert_ne!(id1, [0u8; 32]);
}

#[test]
fn test_onion_route_derive_id_different_params() {
    let id1 = OnionRoute::derive_route_id(
        &[0xAA; 32], 100, 3, &[0x01; 32], &[0x02; 32], 1000,
    );
    let id2 = OnionRoute::derive_route_id(
        &[0xAA; 32], 100, 4, &[0x01; 32], &[0x02; 32], 1000, // different hop count
    );
    assert_ne!(id1, id2);
}

#[test]
fn test_onion_route_layered_root() {
    let hashes = vec![[0x01; 32], [0x02; 32], [0x03; 32]];
    let root = OnionRoute::compute_layered_route_root(&hashes);
    assert_ne!(root, [0u8; 32]);

    // Different order → different root
    let hashes_rev = vec![[0x03; 32], [0x02; 32], [0x01; 32]];
    let root_rev = OnionRoute::compute_layered_route_root(&hashes_rev);
    assert_ne!(root, root_rev);
}

// ── Hop key derivation ──

#[test]
fn test_hop_session_key_deterministic() {
    let secret = [0xAA; 32];
    let route = [0xBB; 32];

    let k1 = derive_hop_session_key(&secret, &route, 0);
    let k2 = derive_hop_session_key(&secret, &route, 0);
    assert_eq!(k1, k2);
}

#[test]
fn test_hop_session_key_per_hop_isolation() {
    let secret = [0xAA; 32];
    let route = [0xBB; 32];

    let k0 = derive_hop_session_key(&secret, &route, 0);
    let k1 = derive_hop_session_key(&secret, &route, 1);
    let k2 = derive_hop_session_key(&secret, &route, 2);

    assert_ne!(k0, k1);
    assert_ne!(k1, k2);
}

#[test]
fn test_hop_session_key_per_route_isolation() {
    let secret = [0xAA; 32];

    let k_a = derive_hop_session_key(&secret, &[0xBB; 32], 0);
    let k_b = derive_hop_session_key(&secret, &[0xCC; 32], 0);
    assert_ne!(k_a, k_b);
}

// ── Hop nonce ──

#[test]
fn test_hop_nonce_deterministic() {
    let key = [0xAA; 32];
    let route = [0xBB; 32];

    let n1 = derive_hop_nonce(&key, &route, 0);
    let n2 = derive_hop_nonce(&key, &route, 0);
    assert_eq!(n1, n2);
    assert_eq!(n1.len(), 12);
}

#[test]
fn test_hop_nonce_per_hop_isolation() {
    let key = [0xAA; 32];
    let route = [0xBB; 32];

    let n0 = derive_hop_nonce(&key, &route, 0);
    let n1 = derive_hop_nonce(&key, &route, 1);
    assert_ne!(n0, n1);
}

// ── Hop MAC ──

#[test]
fn test_hop_mac_deterministic() {
    let key = [0xAA; 32];
    let frag = b"encrypted_fragment_data";
    let instr = b"encrypted_instructions";

    let m1 = compute_hop_mac(&key, frag, instr);
    let m2 = compute_hop_mac(&key, frag, instr);
    assert_eq!(m1, m2);
    assert_ne!(m1, [0u8; 32]);
}

#[test]
fn test_hop_mac_different_keys_different_mac() {
    let frag = b"fragment";
    let instr = b"instructions";

    let m1 = compute_hop_mac(&[0xAA; 32], frag, instr);
    let m2 = compute_hop_mac(&[0xBB; 32], frag, instr);
    assert_ne!(m1, m2);
}

#[test]
fn test_hop_mac_different_data_different_mac() {
    let key = [0xAA; 32];

    let m1 = compute_hop_mac(&key, b"frag1", b"instr1");
    let m2 = compute_hop_mac(&key, b"frag2", b"instr2");
    assert_ne!(m1, m2);
}

// ── Route commitment ──

#[test]
fn test_route_commitment_compute() {
    let rc = RouteCommitment::compute(
        [0xAA; 32],
        [0xBB; 32],
        [0xCC; 32],
        100,
    );

    assert_ne!(rc.commitment, [0u8; 32]);

    // Verify: recomputing with same inputs should match
    let rc2 = RouteCommitment::compute(
        [0xAA; 32],
        [0xBB; 32],
        [0xCC; 32],
        100,
    );
    assert_eq!(rc.commitment, rc2.commitment);

    // Different input → different commitment
    let rc3 = RouteCommitment::compute(
        [0xAA; 32],
        [0xBB; 32],
        [0xCC; 32],
        101, // different epoch
    );
    assert_ne!(rc.commitment, rc3.commitment);
}

#[test]
fn test_route_commitment_deterministic() {
    let rc1 = RouteCommitment::compute(
        [0xAA; 32],
        [0xBB; 32],
        [0xCC; 32],
        100,
    );
    let rc2 = RouteCommitment::compute(
        [0xAA; 32],
        [0xBB; 32],
        [0xCC; 32],
        100,
    );
    assert_eq!(rc1.commitment, rc2.commitment);
}

// ── Route flags ──

#[test]
fn test_route_flags_bitmask() {
    let flags = ROUTE_FLAG_MISSION_SCOPED | ROUTE_FLAG_COVER | ROUTE_FLAG_STEALTH;
    assert!(flags & ROUTE_FLAG_MISSION_SCOPED != 0);
    assert!(flags & ROUTE_FLAG_COVER != 0);
    assert!(flags & ROUTE_FLAG_STEALTH != 0);
}

// ── Transport Vector ──

#[test]
fn test_transport_vector_creation() {
    let tv = TransportVector {
        transport_type: 0x0001,
        domain_id: [0xAA; 32],
        priority: 100,
        bandwidth_class: 200,
        censorship_score: 150,
    };

    assert_eq!(tv.transport_type, 0x0001);
    assert_eq!(tv.bandwidth_class, 200);
}
