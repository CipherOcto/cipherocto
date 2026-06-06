//! Round-trip 282-byte envelope test.
//! Mission AC line 107: "envelope_tests.rs - round-trip 282-byte envelope"
//! Mission AC line 129: "send_envelope() writes the 282-byte envelope via sendMessage"

use octo_adapter_telegram::envelope::{decode_envelope, encode_envelope};
use octo_network::dot::envelope::DeterministicEnvelope;

#[test]
fn test_envelope_roundtrip_282_bytes() {
    // 0850f code comment: "payload contains full wire bytes (218 signing + 64 signature = 282 bytes)"
    let envelope = DeterministicEnvelope {
        version: 1,
        network_id: 42,
        message_type: 0,
        envelope_id: [1u8; 32],
        mission_id: [0u8; 32],
        source_peer: [2u8; 32],
        origin_gateway: [3u8; 32],
        logical_timestamp: 100,
        ttl_hops: 5,
        payload_hash: [4u8; 32],
        route_trace_root: [5u8; 32],
        flags: 0,
        signature: [6u8; 64],
    };

    let wire = envelope.to_wire_bytes();
    assert_eq!(
        wire.len(),
        282,
        "wire envelope should be 218 + 64 = 282 bytes"
    );

    let encoded = encode_envelope(&wire);
    let decoded = decode_envelope(&encoded).unwrap();
    assert_eq!(decoded, wire);
}

#[test]
fn test_envelope_uses_url_safe_no_pad() {
    // 0850f code (lib.rs:228): base64::engine::general_purpose::URL_SAFE_NO_PAD
    // URL_SAFE_NO_PAD replaces + with - and / with _, and omits trailing =
    let envelope = DeterministicEnvelope {
        version: 1,
        network_id: 1,
        message_type: 0,
        envelope_id: [0u8; 32],
        mission_id: [0u8; 32],
        source_peer: [0u8; 32],
        origin_gateway: [0u8; 32],
        logical_timestamp: 0,
        ttl_hops: 0,
        payload_hash: [0u8; 32],
        route_trace_root: [0u8; 32],
        flags: 0,
        signature: [0u8; 64],
    };

    let wire = envelope.to_wire_bytes();
    let encoded = encode_envelope(&wire);

    // URL_SAFE_NO_PAD: no '+' no '/' no '='
    assert!(
        !encoded.contains('+'),
        "URL_SAFE_NO_PAD should not contain '+'"
    );
    assert!(
        !encoded.contains('/'),
        "URL_SAFE_NO_PAD should not contain '/'"
    );
    assert!(
        !encoded.contains('='),
        "URL_SAFE_NO_PAD should not contain '='"
    );
}
