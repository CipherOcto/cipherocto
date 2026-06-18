//! Deep coverage tests for DOT — transport, gateway federation, envelope privacy,
//! fragment reassembly, route selection, domain variants.

use std::time::Duration;

use octo_network::dot::adapters::{CapabilityReport, MediaCapabilities};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::{
    DeterministicEnvelope, MessageType, ObfuscatedEnvelope, PrivacyConfig, SealedEnvelope,
};
use octo_network::dot::fragment::{
    fragment_envelope, fragments_complete, reassemble_fragments, EnvelopeFragment, PlatformLimit,
    ReassemblyState,
};
use octo_network::dot::gateway::{
    FederationPeer, FederationState, GatewayCapacity, GatewayClass, GatewayIdentity,
};
use octo_network::dot::route::{
    compute_gateway_sequence_hash, compute_route_score, handle_partition, select_best_route,
    GatewayRoute, PartitionEvent, RouteCommitment, RouteWeights,
};
use octo_network::dot::transport::{
    b64url_decode, b64url_encode, decode_fragment_ref, decode_native_ref, decode_text_ref,
    detect_mode, encode_fragment_ref, encode_native_ref, encode_text_ref, select_mode,
    select_mode_with_max_text, TransportMode,
};

fn make_envelope(id_byte: u8) -> DeterministicEnvelope {
    DeterministicEnvelope {
        version: 1,
        network_id: 1,
        message_type: MessageType::Message as u16,
        envelope_id: [id_byte; 32],
        mission_id: [0u8; 32],
        source_peer: [0x01; 32],
        origin_gateway: [0x01; 32],
        logical_timestamp: 1000,
        ttl_hops: 10,
        payload_hash: blake3::hash(b"test").into(),
        route_trace_root: [0u8; 32],
        flags: 0,
        signature: [0u8; 64],
    }
}

fn make_gateway(id_byte: u8, class: GatewayClass) -> GatewayIdentity {
    let mut pk = [0u8; 32];
    pk[0] = id_byte;
    GatewayIdentity::new(pk, 1, class, 100)
}

// ── PlatformType comprehensive coverage ──

#[test]
fn test_platform_type_all_variants() {
    let cases = [
        (0x0001, PlatformType::Telegram),
        (0x0002, PlatformType::Discord),
        (0x0003, PlatformType::Matrix),
        (0x0004, PlatformType::Nostr),
        (0x0005, PlatformType::Signal),
        (0x0006, PlatformType::IRC),
        (0x0007, PlatformType::Slack),
        (0x0008, PlatformType::WhatsApp),
        (0x0009, PlatformType::Webhook),
        (0x000A, PlatformType::NativeP2P),
        (0x000B, PlatformType::Bluetooth),
        (0x000C, PlatformType::LoRa),
        (0x000D, PlatformType::WebRTC),
        (0x000E, PlatformType::Bluesky),
        (0x000F, PlatformType::Twitter),
        (0x0010, PlatformType::Reddit),
        (0x0011, PlatformType::WeChat),
        (0x0012, PlatformType::DingTalk),
        (0x0013, PlatformType::Lark),
        (0x0014, PlatformType::QQ),
        (0x0015, PlatformType::Quic),
    ];
    for (val, expected) in cases {
        assert_eq!(PlatformType::from_u16(val), Some(expected), "0x{:04x}", val);
    }
    assert!(PlatformType::from_u16(0x0000).is_none());
    assert!(PlatformType::from_u16(0x0016).is_none());
}

// ── BroadcastDomainId comprehensive ──

#[test]
fn test_broadcast_domain_all_platforms() {
    // Each platform produces a different domain hash
    let platforms = [
        PlatformType::Telegram,
        PlatformType::Discord,
        PlatformType::Matrix,
        PlatformType::Nostr,
        PlatformType::Signal,
        PlatformType::IRC,
        PlatformType::NativeP2P,
        PlatformType::Quic,
    ];
    let hashes: Vec<[u8; 32]> = platforms
        .iter()
        .map(|p| BroadcastDomainId::new(*p, "test").domain_hash)
        .collect();

    // All should be unique
    for i in 0..hashes.len() {
        for j in (i + 1)..hashes.len() {
            assert_ne!(
                hashes[i], hashes[j],
                "platforms {:?} and {:?} collide",
                platforms[i], platforms[j]
            );
        }
    }
}

#[test]
fn test_broadcast_domain_case_insensitive() {
    let d1 = BroadcastDomainId::new(PlatformType::Telegram, "MyChannel");
    let d2 = BroadcastDomainId::new(PlatformType::Telegram, "mychannel");
    assert_eq!(d1, d2);
}

#[test]
fn test_broadcast_domain_trimmed() {
    let d1 = BroadcastDomainId::new(PlatformType::Telegram, "  test  ");
    let d2 = BroadcastDomainId::new(PlatformType::Telegram, "test");
    assert_eq!(d1, d2);
}

// ── Envelope privacy ──

#[test]
fn test_envelope_set_privacy_all_flags() {
    let mut env = make_envelope(0xAA);
    assert_eq!(env.flags, 0);

    env.set_privacy(&PrivacyConfig {
        e2e_encryption: true,
        metadata_minimization: true,
        transport_obfuscation: true,
    });

    assert!(env.is_encrypted());
    assert!(env.is_sealed());
    assert!(env.is_obfuscated());
    assert!(env.is_e2e());
    assert!(!env.is_stealth()); // stealth is separate
}

#[test]
fn test_envelope_stealth_flag() {
    let mut env = make_envelope(0xAA);
    assert!(!env.is_stealth());
    env.flags |= 0x0010; // STEALTH
    assert!(env.is_stealth());
}

#[test]
fn test_sealed_envelope_lifecycle() {
    let env = make_envelope(0xAA);
    let sealed = SealedEnvelope::new(env.clone(), vec![0xCD; 128], [0xAB; 12], [0xEF; 32]);

    assert_eq!(sealed.envelope.envelope_id, env.envelope_id);
    assert_eq!(sealed.nonce, [0xAB; 12]);
    assert_eq!(sealed.sender_ephemeral, [0xEF; 32]);
    assert_eq!(sealed.encrypted_payload.len(), 128);
}

#[test]
fn test_sealed_envelope_derive_decryption_key() {
    let k1 = SealedEnvelope::derive_decryption_key(&[0xAA; 32], &[0xBB; 32]);
    let k2 = SealedEnvelope::derive_decryption_key(&[0xAA; 32], &[0xBB; 32]);
    assert_eq!(k1, k2);
    assert_ne!(k1, [0u8; 32]);

    // Different secret → different key
    let k3 = SealedEnvelope::derive_decryption_key(&[0xCC; 32], &[0xBB; 32]);
    assert_ne!(k1, k3);
}

#[test]
fn test_sealed_envelope_verify_payload_hash() {
    let payload = b"secret data";
    let hash = blake3::hash(payload).into();

    let mut env = make_envelope(0xAA);
    env.payload_hash = hash;

    let sealed = SealedEnvelope::new(env, payload.to_vec(), [0u8; 12], [0u8; 32]);
    assert!(sealed.verify_payload_hash());

    // Tamper
    let mut tampered = sealed.clone();
    tampered.encrypted_payload[0] ^= 0xFF;
    assert!(!tampered.verify_payload_hash());
}

#[test]
fn test_obfuscated_envelope_lifecycle() {
    let wire = vec![0xAB; 100];
    let obf = ObfuscatedEnvelope::from_wire(wire.clone());

    assert_eq!(obf.wire_bytes, wire);
    assert_ne!(obf.envelope_hash, [0u8; 32]);
    assert_eq!(obf.dedup_key(), obf.envelope_hash);

    // Deterministic
    let obf2 = ObfuscatedEnvelope::from_wire(wire);
    assert_eq!(obf.dedup_key(), obf2.dedup_key());
}

// ── Fragment reassembly edge cases ──

#[test]
fn test_fragment_empty_reassembly() {
    let result = reassemble_fragments(&[]);
    assert!(result.is_err());
}

#[test]
fn test_fragment_zero_total() {
    let frag = EnvelopeFragment {
        envelope_hash: [0u8; 32],
        envelope_id: [0u8; 32],
        fragment_index: 0,
        fragment_total: 0,
        payload: vec![],
    };
    let result = reassemble_fragments(&[frag]);
    assert!(result.is_err());
}

#[test]
fn test_fragment_index_out_of_range() {
    let frag = EnvelopeFragment {
        envelope_hash: [0u8; 32],
        envelope_id: [0u8; 32],
        fragment_index: 5,
        fragment_total: 3,
        payload: vec![],
    };
    let result = reassemble_fragments(&[frag]);
    assert!(result.is_err());
}

#[test]
fn test_fragment_integrity_hash_mismatch() {
    let payload = vec![0xAB; 100];
    let correct_hash = *blake3::hash(&payload).as_bytes();

    let mut fragments =
        fragment_envelope(correct_hash, [0xAA; 32], &payload, PlatformLimit::Irc).unwrap();
    // Tamper one fragment's envelope_hash
    fragments[0].envelope_hash[0] ^= 0xFF;

    let result = reassemble_fragments(&fragments);
    assert!(result.is_err());
}

#[test]
fn test_fragment_incomplete_set() {
    let payload = vec![0xAB; 1000];
    let hash = *blake3::hash(&payload).as_bytes();
    let fragments = fragment_envelope(hash, [0xAA; 32], &payload, PlatformLimit::Irc).unwrap();

    // Drop last fragment
    let incomplete = &fragments[..fragments.len() - 1];
    let result = reassemble_fragments(incomplete);
    assert!(result.is_err());
}

#[test]
fn test_fragments_complete_check() {
    let payload = vec![0xAB; 1000];
    let hash = *blake3::hash(&payload).as_bytes();
    let fragments = fragment_envelope(hash, [0xAA; 32], &payload, PlatformLimit::Irc).unwrap();

    assert!(fragments_complete(&fragments));
    assert!(!fragments_complete(&fragments[..2]));
    assert!(!fragments_complete(&[]));
}

#[test]
fn test_reassembly_state_lifecycle() {
    let payload = vec![0xAB; 1000];
    let hash = *blake3::hash(&payload).as_bytes();
    let fragments = fragment_envelope(hash, [0xAA; 32], &payload, PlatformLimit::Irc).unwrap();

    let mut state = ReassemblyState::new(fragments[0].clone(), 100);
    assert_eq!(state.received_count(), 1);
    assert_eq!(state.fragment_total, 3);

    // Add second fragment
    let complete = state.add_fragment(fragments[1].clone()).unwrap();
    assert!(!complete);
    assert_eq!(state.received_count(), 2);

    // Add third fragment — should be complete
    let complete = state.add_fragment(fragments[2].clone()).unwrap();
    assert!(complete);

    // Finalize
    let reassembled = state.finalize().unwrap();
    assert_eq!(reassembled, payload);
}

#[test]
fn test_reassembly_state_total_mismatch() {
    let payload = vec![0xAB; 1000];
    let hash = *blake3::hash(&payload).as_bytes();
    let fragments = fragment_envelope(hash, [0xAA; 32], &payload, PlatformLimit::Irc).unwrap();

    let mut state = ReassemblyState::new(fragments[0].clone(), 100);

    let mut wrong_total = fragments[1].clone();
    wrong_total.fragment_total = 99;
    let result = state.add_fragment(wrong_total);
    assert!(result.is_err());
}

#[test]
fn test_reassembly_state_expiry() {
    let payload = vec![0xAB; 100];
    let hash = *blake3::hash(&payload).as_bytes();
    let fragments = fragment_envelope(hash, [0xAA; 32], &payload, PlatformLimit::Irc).unwrap();

    let state = ReassemblyState::new(fragments[0].clone(), 100);
    assert!(!state.is_expired(150, Duration::from_secs(60)));
    assert!(state.is_expired(170, Duration::from_secs(60)));
}

// ── Gateway Federation ──

#[test]
fn test_federation_state_lifecycle() {
    let local = make_gateway(0x01, GatewayClass::Relay);
    let mut fed = FederationState::new(local);

    assert_eq!(fed.active_peer_count(), 0);

    // Add peers
    let peer1 = FederationPeer {
        identity: make_gateway(0x02, GatewayClass::Edge),
        capacity: GatewayCapacity::default(),
        domains: vec![[0xAA; 32]],
        last_seen: 100,
        active: true,
    };
    let peer2 = FederationPeer {
        identity: make_gateway(0x03, GatewayClass::Consensus),
        capacity: GatewayCapacity::default(),
        domains: vec![[0xAA; 32], [0xBB; 32]],
        last_seen: 200,
        active: true,
    };

    assert!(fed.upsert_peer(peer1));
    assert!(fed.upsert_peer(peer2));
    assert_eq!(fed.active_peer_count(), 2);

    // Update existing peer
    let peer1_updated = FederationPeer {
        identity: make_gateway(0x02, GatewayClass::Edge),
        capacity: GatewayCapacity::default(),
        domains: vec![[0xAA; 32]],
        last_seen: 300,
        active: true,
    };
    assert!(!fed.upsert_peer(peer1_updated)); // not new

    // Peers for domain
    let aa_peers = fed.peers_for_domain(&[0xAA; 32]);
    assert_eq!(aa_peers.len(), 2);
    let bb_peers = fed.peers_for_domain(&[0xBB; 32]);
    assert_eq!(bb_peers.len(), 1);

    // Connected domains
    let domains = fed.connected_domains();
    assert_eq!(domains.len(), 2);

    // Evict stale (cutoff = 500 - 100 = 400)
    // peer1_updated.last_seen=300 <= 400, peer2.last_seen=200 <= 400 → both evicted
    let evicted = fed.evict_stale_peers(500, 100);
    assert_eq!(evicted, 2);

    // Remove peer
    let gw2_id = make_gateway(0x02, GatewayClass::Edge).gateway_id;
    assert!(fed.remove_peer(&gw2_id));
    assert!(!fed.remove_peer(&gw2_id));
}

#[test]
fn test_federation_survive_partition() {
    let local = make_gateway(0x01, GatewayClass::Relay);
    let mut fed = FederationState::new(local);

    fed.upsert_peer(FederationPeer {
        identity: make_gateway(0x02, GatewayClass::Edge),
        capacity: GatewayCapacity::default(),
        domains: vec![[0xAA; 32]],
        last_seen: 100,
        active: true,
    });
    fed.upsert_peer(FederationPeer {
        identity: make_gateway(0x03, GatewayClass::Consensus),
        capacity: GatewayCapacity::default(),
        domains: vec![[0xBB; 32]],
        last_seen: 100,
        active: true,
    });

    // Partitioning AA still leaves peer on BB
    assert!(fed.can_survive_partition(&[0xAA; 32]));

    // If all peers only on AA, partition kills federation
    let mut fed2 = FederationState::new(make_gateway(0x01, GatewayClass::Relay));
    fed2.upsert_peer(FederationPeer {
        identity: make_gateway(0x02, GatewayClass::Edge),
        capacity: GatewayCapacity::default(),
        domains: vec![[0xAA; 32]],
        last_seen: 100,
        active: true,
    });
    assert!(!fed2.can_survive_partition(&[0xAA; 32]));
}

// ── Route selection ──

#[test]
fn test_select_best_route() {
    let routes = vec![
        GatewayRoute {
            gateway_id: [0x01; 32],
            domain_hashes: vec![],
            weights: RouteWeights::default(),
            score: 500,
            commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
            active: true,
        },
        GatewayRoute {
            gateway_id: [0x02; 32],
            domain_hashes: vec![],
            weights: RouteWeights::default(),
            score: 900,
            commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
            active: true,
        },
        GatewayRoute {
            gateway_id: [0x03; 32],
            domain_hashes: vec![],
            weights: RouteWeights::default(),
            score: 900,
            commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
            active: false, // inactive
        },
    ];

    let best = select_best_route(&routes).unwrap();
    assert_eq!(best.gateway_id, [0x02; 32]); // highest score, active

    assert!(select_best_route(&[]).is_none());
}

#[test]
fn test_handle_partition() {
    let routes = vec![
        GatewayRoute {
            gateway_id: [0x01; 32],
            domain_hashes: vec![[0xAA; 32]],
            weights: RouteWeights::default(),
            score: 500,
            commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
            active: true,
        },
        GatewayRoute {
            gateway_id: [0x02; 32],
            domain_hashes: vec![[0xBB; 32]],
            weights: RouteWeights::default(),
            score: 900,
            commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
            active: true,
        },
    ];

    let event = PartitionEvent {
        domain_hash: [0xAA; 32],
        detected_epoch: 100,
        remaining_carriers: vec![],
    };

    let remaining = handle_partition(&routes, &event);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].gateway_id, [0x02; 32]);
}

#[test]
fn test_compute_gateway_sequence_hash() {
    let ids = vec![[0x01; 32], [0x02; 32], [0x03; 32]];
    let h1 = compute_gateway_sequence_hash(&ids);
    let h2 = compute_gateway_sequence_hash(&ids);
    assert_eq!(h1, h2);
    assert_ne!(h1, [0u8; 32]);

    // Different order → different hash
    let ids_rev = vec![[0x03; 32], [0x02; 32], [0x01; 32]];
    let h3 = compute_gateway_sequence_hash(&ids_rev);
    assert_ne!(h1, h3);
}

#[test]
fn test_route_weights_default() {
    let w = RouteWeights::default();
    assert_eq!(w.trust_weight, 400);
    assert_eq!(w.bandwidth_weight, 300);
    assert_eq!(w.censorship_weight, 200);
    assert_eq!(w.cost_weight, 100);
}

// ── Transport mode ──

#[test]
fn test_detect_mode() {
    assert_eq!(detect_mode("DOT/1/base64data"), Some(TransportMode::Text));
    assert_eq!(detect_mode("DOT/2/msg123"), Some(TransportMode::Native));
    assert_eq!(detect_mode("DOT/F/fragdata"), Some(TransportMode::Fragment));
    assert_eq!(detect_mode("random stuff"), None);
}

#[test]
fn test_encode_decode_text_ref() {
    let data = b"hello world";
    let encoded = encode_text_ref(data);
    assert!(encoded.starts_with("DOT/1/"));

    let decoded = decode_text_ref(&encoded).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_decode_text_ref_errors() {
    assert!(decode_text_ref("invalid").is_err());
    assert!(decode_text_ref("DOT/1/").is_err()); // empty
}

#[test]
fn test_b64url_roundtrip() {
    let data = b"Hello, CipherOcto! This is a test of base64url encoding.";
    let encoded = b64url_encode(data);
    // base64url should not contain +, /, or =
    assert!(!encoded.contains('+'));
    assert!(!encoded.contains('/'));
    assert!(!encoded.contains('='));

    let decoded = b64url_decode(&encoded).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_b64url_empty() {
    let encoded = b64url_encode(b"");
    assert!(encoded.is_empty());
    let decoded = b64url_decode("").unwrap();
    assert!(decoded.is_empty());
}

#[test]
fn test_b64url_invalid_char() {
    assert!(b64url_decode("ab!d").is_err());
}

#[test]
fn test_payload_too_large_error_display() {
    let caps = CapabilityReport {
        max_payload_bytes: 100,
        supports_fragmentation: false,
        supports_encryption: false,
        supports_raw_binary: false,
        rate_limit_per_second: 10,
        media_capabilities: None,
    };
    let err = select_mode(5000, &caps).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("5000"));
    assert!(msg.contains("100"));
}

#[test]
fn test_payload_too_large_is_error() {
    // Verify it implements std::error::Error
    let caps = CapabilityReport {
        max_payload_bytes: 100,
        supports_fragmentation: false,
        supports_encryption: false,
        supports_raw_binary: false,
        rate_limit_per_second: 10,
        media_capabilities: None,
    };
    let err = select_mode(5000, &caps).unwrap_err();
    let _: &dyn std::error::Error = &err;
}
