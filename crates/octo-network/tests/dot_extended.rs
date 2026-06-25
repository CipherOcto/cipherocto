//! Integration tests for DOT gaps — fragment, gateway, envelope APIs,
//! domain enums, route, transport mode selection.

use octo_network::dot::adapters::{CapabilityReport, MediaCapabilities};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::{DeterministicEnvelope, MessageType, PrivacyConfig};
use octo_network::dot::fragment::{fragment_envelope, reassemble_fragments, PlatformLimit};
use octo_network::dot::gateway::{FederationPeer, GatewayCapacity, GatewayClass, GatewayIdentity};
use octo_network::dot::route::{compute_route_score, RouteCommitment, RouteWeights};
use octo_network::dot::transport::{
    decode_fragment_ref, decode_native_ref, encode_fragment_ref, encode_native_ref, select_mode,
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

// ── PlatformType enum ──

#[test]
fn test_platform_type_from_u16() {
    assert_eq!(PlatformType::from_u16(0x0001), Some(PlatformType::Telegram));
    assert_eq!(PlatformType::from_u16(0x0008), Some(PlatformType::WhatsApp));
    assert_eq!(
        PlatformType::from_u16(0x000A),
        Some(PlatformType::NativeP2P)
    );
    assert_eq!(PlatformType::from_u16(0x0015), Some(PlatformType::Quic));
    assert!(PlatformType::from_u16(0x00FF).is_none());
}

// ── BroadcastDomainId ──

#[test]
fn test_broadcast_domain_id_canonical_roundtrip() {
    let domain = BroadcastDomainId::new(PlatformType::Telegram, "test_channel");
    let bytes = domain.to_canonical_bytes();
    let recovered = BroadcastDomainId::from_canonical_bytes(&bytes).unwrap();
    assert_eq!(domain, recovered);
}

#[test]
fn test_broadcast_domain_id_from_bytes_too_short() {
    assert!(BroadcastDomainId::from_canonical_bytes(&[0u8; 10]).is_err());
}

#[test]
fn test_broadcast_domain_id_different_platforms_different_hash() {
    let d1 = BroadcastDomainId::new(PlatformType::Telegram, "test");
    let d2 = BroadcastDomainId::new(PlatformType::Quic, "test");
    assert_ne!(d1.to_canonical_bytes(), d2.to_canonical_bytes());
}

// ── Envelope APIs ──

#[test]
fn test_derive_envelope_id_deterministic() {
    let env = make_envelope(0xAA);
    let id1 = env.derive_envelope_id();
    let id2 = env.derive_envelope_id();
    assert_eq!(id1, id2);
}

#[test]
fn test_envelope_wire_roundtrip() {
    let env = make_envelope(0xAA);
    let wire = env.to_wire_bytes();
    let recovered = DeterministicEnvelope::from_wire_bytes(&wire).unwrap();
    assert_eq!(env.envelope_id, recovered.envelope_id);
    assert_eq!(env.network_id, recovered.network_id);
    assert_eq!(env.version, recovered.version);
}

#[test]
fn test_envelope_privacy_config() {
    let config = PrivacyConfig {
        e2e_encryption: true,
        metadata_minimization: false,
        transport_obfuscation: false,
    };
    assert!(config.e2e_encryption);
    assert!(!config.metadata_minimization);
}

#[test]
fn test_envelope_is_encrypted_flag() {
    let mut env = make_envelope(0xAA);
    assert!(!env.is_encrypted());
    // Set encrypted flag
    env.flags |= 0x0001;
    assert!(env.is_encrypted());
}

#[test]
fn test_envelope_is_sealed_flag() {
    let mut env = make_envelope(0xAA);
    assert!(!env.is_sealed());
    env.flags |= 0x0002;
    assert!(env.is_sealed());
}

#[test]
fn test_envelope_is_e2e_flag() {
    let mut env = make_envelope(0xAA);
    assert!(!env.is_e2e());
    env.flags |= 0x0008; // E2E = 0x0008
    assert!(env.is_e2e());
}

#[test]
fn test_envelope_derive_sealing_key() {
    let k1 = DeterministicEnvelope::derive_sealing_key(&[0xAA; 32], &[0xBB; 32]);
    let k2 = DeterministicEnvelope::derive_sealing_key(&[0xAA; 32], &[0xBB; 32]);
    assert_eq!(k1, k2);
    assert_ne!(k1, [0u8; 32]);
}

#[test]
fn test_envelope_compute_wire_hash_deterministic() {
    let h1 = DeterministicEnvelope::compute_wire_hash(b"signing", b"encrypted");
    let h2 = DeterministicEnvelope::compute_wire_hash(b"signing", b"encrypted");
    assert_eq!(h1, h2);
    assert_ne!(h1, [0u8; 32]);
}

// ── Fragment ──

#[test]
fn test_fragment_and_reassemble_irc() {
    let payload = vec![0xAB; 1000];
    let envelope_hash = blake3::hash(&payload).into();
    let envelope_id = [0xAA; 32];

    let fragments =
        fragment_envelope(envelope_hash, envelope_id, &payload, PlatformLimit::Irc).unwrap();

    // IRC: 512 - 68 = 444 bytes per fragment, ceil(1000/444) = 3
    assert_eq!(fragments.len(), 3);
    assert_eq!(fragments[0].fragment_index, 0);
    assert_eq!(fragments[0].fragment_total, 3);
    assert_eq!(fragments[2].fragment_index, 2);

    // Reassemble
    let reassembled = reassemble_fragments(&fragments).unwrap();
    assert_eq!(reassembled, payload);
}

#[test]
fn test_fragment_and_reassemble_lora() {
    let payload = vec![0xCD; 500];
    let envelope_hash = blake3::hash(&payload).into();

    let fragments =
        fragment_envelope(envelope_hash, [0xBB; 32], &payload, PlatformLimit::Lora).unwrap();

    // LoRa: 256 - 68 = 188 bytes per fragment, ceil(500/188) = 3
    assert_eq!(fragments.len(), 3);

    let reassembled = reassemble_fragments(&fragments).unwrap();
    assert_eq!(reassembled, payload);
}

#[test]
fn test_fragment_single_fragment() {
    let payload = vec![0xAB; 100];
    let envelope_hash = blake3::hash(&payload).into();

    let fragments =
        fragment_envelope(envelope_hash, [0xCC; 32], &payload, PlatformLimit::Telegram).unwrap();

    // Telegram: 4096 - 68 = 4028, 100 fits in one fragment
    assert_eq!(fragments.len(), 1);

    let reassembled = reassemble_fragments(&fragments).unwrap();
    assert_eq!(reassembled, payload);
}

#[test]
fn test_fragment_empty_payload() {
    let envelope_hash = [0u8; 32];
    let fragments =
        fragment_envelope(envelope_hash, [0xDD; 32], &[], PlatformLimit::Telegram).unwrap();
    assert_eq!(fragments.len(), 1);
    assert!(fragments[0].payload.is_empty());
}

#[test]
fn test_fragment_custom_limit() {
    let payload = vec![0xAB; 200];
    let envelope_hash = blake3::hash(&payload).into();

    // Custom limit: 100 bytes total, 100 - 68 = 32 bytes per fragment
    let fragments = fragment_envelope(
        envelope_hash,
        [0xEE; 32],
        &payload,
        PlatformLimit::Custom(100),
    )
    .unwrap();

    assert!(fragments.len() > 1);

    let reassembled = reassemble_fragments(&fragments).unwrap();
    assert_eq!(reassembled, payload);
}

#[test]
fn test_fragment_integrity_mismatch() {
    let payload = vec![0xAB; 100];
    let envelope_hash = blake3::hash(&payload).into();

    let mut fragments =
        fragment_envelope(envelope_hash, [0xFF; 32], &payload, PlatformLimit::Irc).unwrap();

    // Tamper with fragment payload
    if !fragments[0].payload.is_empty() {
        fragments[0].payload[0] ^= 0xFF;
    }

    let result = reassemble_fragments(&fragments);
    assert!(result.is_err());
}

// ── Gateway ──

#[test]
fn test_gateway_identity_deterministic() {
    let id1 = GatewayIdentity::new([0x42; 32], 1, GatewayClass::Edge, 100);
    let id2 = GatewayIdentity::new([0x42; 32], 1, GatewayClass::Edge, 100);
    assert_eq!(id1.gateway_id, id2.gateway_id);
}

#[test]
fn test_gateway_identity_builder() {
    let gw = GatewayIdentity::new([0x42; 32], 1, GatewayClass::Relay, 100)
        .with_platforms(0x0001 | 0x0002)
        .with_capabilities(0x0001 | 0x0004);

    assert_eq!(gw.supported_platforms, 0x0003);
    assert_eq!(gw.capabilities, 0x0005);
    assert_eq!(gw.gateway_class, GatewayClass::Relay);
}

#[test]
fn test_gateway_capacity_default() {
    let cap = GatewayCapacity::default();
    assert_eq!(cap.max_throughput, 1000);
    assert_eq!(cap.domain_count, 0);
}

#[test]
fn test_federation_peer_creation() {
    let identity = GatewayIdentity::new([0x42; 32], 1, GatewayClass::Edge, 100);
    let peer = FederationPeer {
        identity: identity.clone(),
        capacity: GatewayCapacity::default(),
        domains: vec![[0xAA; 32]],
        last_seen: 500,
        active: true,
    };

    assert!(peer.active);
    assert_eq!(peer.identity.gateway_id, identity.gateway_id);
}

// ── Route ──

#[test]
fn test_route_commitment_compute_and_verify() {
    let rc = RouteCommitment::compute([0xAA; 32], [0xBB; 32], 100);
    assert!(rc.verify());

    let mut tampered = rc.clone();
    tampered.epoch = 101;
    assert!(!tampered.verify());
}

#[test]
fn test_route_commitment_deterministic() {
    let rc1 = RouteCommitment::compute([0xAA; 32], [0xBB; 32], 100);
    let rc2 = RouteCommitment::compute([0xAA; 32], [0xBB; 32], 100);
    assert_eq!(rc1.commitment, rc2.commitment);
}

#[test]
fn test_compute_route_score() {
    let weights = RouteWeights::default();
    let score = compute_route_score(&weights, 100, 200, 50, 10);
    assert!(score > 0);

    // Higher trust → higher score
    let score_high = compute_route_score(&weights, 500, 200, 50, 10);
    assert!(score_high > score);
}

// ── Transport mode selection ──

#[test]
fn test_transport_mode_raw_binary() {
    let caps = CapabilityReport {
        max_payload_bytes: 65536,
        supports_fragmentation: true,
        supports_encryption: true,
        supports_raw_binary: true,
        rate_limit_per_second: 1000,
        media_capabilities: None,
        ..Default::default()
    };

    assert_eq!(select_mode(100, &caps).unwrap(), TransportMode::Raw);
    assert_eq!(select_mode(100_000, &caps).unwrap(), TransportMode::Raw);
}

#[test]
fn test_transport_mode_text_small() {
    let caps = CapabilityReport {
        max_payload_bytes: 4096,
        supports_fragmentation: false,
        supports_encryption: false,
        supports_raw_binary: false,
        rate_limit_per_second: 10,
        media_capabilities: None,
        ..Default::default()
    };

    assert_eq!(select_mode(100, &caps).unwrap(), TransportMode::Text);
}

#[test]
fn test_transport_mode_native_with_media() {
    let caps = CapabilityReport {
        max_payload_bytes: 4096,
        supports_fragmentation: false,
        supports_encryption: false,
        supports_raw_binary: false,
        rate_limit_per_second: 10,
        media_capabilities: Some(MediaCapabilities {
            max_upload_bytes: 50_000_000,
            supported_mime_types: vec![],
        }),
        ..Default::default()
    };

    assert_eq!(select_mode(5000, &caps).unwrap(), TransportMode::Native);
}

#[test]
fn test_transport_mode_fragment() {
    let caps = CapabilityReport {
        max_payload_bytes: 4096,
        supports_fragmentation: true,
        supports_encryption: false,
        supports_raw_binary: false,
        rate_limit_per_second: 10,
        media_capabilities: None,
        ..Default::default()
    };

    assert_eq!(select_mode(5000, &caps).unwrap(), TransportMode::Fragment);
}

#[test]
fn test_transport_mode_too_large_no_fragment() {
    let caps = CapabilityReport {
        max_payload_bytes: 100,
        supports_fragmentation: false,
        supports_encryption: false,
        supports_raw_binary: false,
        rate_limit_per_second: 10,
        media_capabilities: None,
        ..Default::default()
    };

    let result = select_mode(5000, &caps);
    assert!(result.is_err());
}

#[test]
fn test_transport_mode_custom_max_text() {
    let caps = CapabilityReport {
        max_payload_bytes: 512,
        supports_fragmentation: true,
        supports_encryption: false,
        supports_raw_binary: false,
        rate_limit_per_second: 10,
        media_capabilities: None,
        ..Default::default()
    };

    // With custom max_text_bytes = 100, a 50-byte payload fits in text
    assert_eq!(
        select_mode_with_max_text(50, &caps, 100).unwrap(),
        TransportMode::Text
    );
    // 150 bytes > 100 max_text, falls through to fragment
    assert_eq!(
        select_mode_with_max_text(150, &caps, 100).unwrap(),
        TransportMode::Fragment
    );
}

// ── Wire format encoding ──

#[test]
fn test_encode_decode_native_ref() {
    let encoded = encode_native_ref("msg_12345");
    assert_eq!(encoded, "DOT/2/msg_12345");

    let decoded = decode_native_ref(&encoded);
    assert_eq!(decoded, Some("msg_12345"));
}

#[test]
fn test_decode_native_ref_invalid() {
    assert!(decode_native_ref("invalid").is_none());
    assert!(decode_native_ref("DOT/2/").is_none()); // empty id
}

#[test]
fn test_encode_decode_fragment_ref() {
    let encoded = encode_fragment_ref("YWJjZA==");
    assert_eq!(encoded, "DOT/F/YWJjZA==");

    let decoded = decode_fragment_ref(&encoded);
    assert_eq!(decoded, Some("YWJjZA=="));
}

#[test]
fn test_decode_fragment_ref_invalid() {
    assert!(decode_fragment_ref("invalid").is_none());
}
