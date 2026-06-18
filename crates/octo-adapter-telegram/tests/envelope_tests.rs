//! Round-trip 282-byte envelope test.
//! Mission AC line 107: "envelope_tests.rs - round-trip 282-byte envelope"
//! Mission AC line 129: "send_envelope() writes the 282-byte envelope via sendMessage"
//!
//! R6 TEST-C3: Also tests decode-envelope error paths (bad length, bad base64, non-UTF8).

use base64::Engine;
use octo_adapter_telegram::envelope::{decode_envelope, encode_envelope, ENVELOPE_WIRE_LENGTH};
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

// =============================================================================
// R6 TEST-C3: Decode envelope error paths
// =============================================================================

/// Payload too short (not 282 bytes) should be rejected with Envelope error.
#[test]
fn test_decode_envelope_too_short() {
    let short_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(b"too short");
    let result = decode_envelope(&short_payload);
    assert!(result.is_err(), "should reject short payload");
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("length mismatch"), "error should mention length mismatch: {}", msg);
}

/// Payload too long (more than 282 bytes) should be rejected with Envelope error.
#[test]
fn test_decode_envelope_too_long() {
    let long_data = vec![0x42u8; ENVELOPE_WIRE_LENGTH + 10];
    let long_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&long_data);
    let result = decode_envelope(&long_payload);
    assert!(result.is_err(), "should reject long payload");
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("length mismatch"), "error should mention length mismatch: {}", msg);
}

/// Invalid base64 input should be rejected with Envelope error.
#[test]
fn test_decode_envelope_invalid_base64() {
    let result = decode_envelope("!!!not-valid-base64!!!");
    assert!(result.is_err(), "should reject invalid base64");
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("base64"), "error should mention base64: {}", msg);
}

/// Empty string should be rejected.
#[test]
fn test_decode_envelope_empty() {
    let result = decode_envelope("");
    assert!(result.is_err(), "should reject empty string");
}

/// A valid-length but wrong-content payload (not a real envelope) still passes
/// base64 and length check but is returned as bytes. The `canonicalize` path
/// then calls `DeterministicEnvelope::from_wire_bytes` which validates structure.
/// This test asserts the length gate works — it does NOT attempt structural
/// validation (that's `from_wire_bytes`'s job).
#[test]
fn test_decode_envelope_exact_length_random_bytes() {
    let random_data: Vec<u8> = (0..ENVELOPE_WIRE_LENGTH).map(|i| (i ^ 0xAB) as u8).collect();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&random_data);
    let result = decode_envelope(&encoded);
    assert!(result.is_ok(), "exact-length random bytes should decode successfully");
    assert_eq!(result.unwrap(), random_data);
}
