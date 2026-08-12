//! Integration tests: RFC-0958 wire format v2 (4th segment = `proof_bundle_borsh`).
//!
//! AC-8: Wire format v2 parses correctly:
//! - v2 emits 4 segments when caller supplies `Some(proof_bundle)`
//! - v1 parser ignores 4th segment (forward-compat)
//! - v2 parser extracts `proof_bundle_borsh`
//! - Borsh round-trip byte-identical (canonicalization survives)
// Test doc-comment lint relaxation (see other `tests/*.rs` files).
#![allow(clippy::doc_markdown)]
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use octo_ident::test_helpers::sample_did;
use octo_wallet::capability::caveat::Caveat;
use octo_wallet::capability::{
    deserialize_wire, deserialize_wire_v2, serialize_wire, serialize_wire_v2,
};
use octo_wallet::capability::{CapabilityToken, ProofBundle, PublicInputs};
use octo_wallet::identity::IdentityKey;

fn sample_token() -> (CapabilityToken, IdentityKey) {
    let mut holder = IdentityKey::generate().expect("identity key");
    // Newly-generated identities start in `Designated` state; `mint()` requires
    // `Active` so the holder can sign the holder_msg (per RFC-0009 §Lifecycle
    // row 1: `Designated → Active`). The activation timestamp is not security-
    // sensitive in tests; use a fixed epoch.
    holder.activate(1_700_000_000).expect("activate");
    let root_secret = [0x42; 32];
    let caveats = [Caveat::Before(1_700_000_000)];
    let token =
        CapabilityToken::mint(&root_secret, &holder, &sample_did(244), &caveats).expect("mint");
    (token, holder)
}

#[test]
fn v2_emits_4_segments() {
    let (token, _holder) = sample_token();
    let pb_bytes = b"fake_proof_bundle_v2".to_vec();
    let wire = serialize_wire_v2(&token, Some(&pb_bytes)).expect("v2 serialize");
    let segments: Vec<&str> = wire.split('.').collect();
    assert_eq!(segments.len(), 4, "v2 wire must have 4 segments");

    // The 4th segment is the base64url-encoded proof_bundle.
    let pb_decoded = URL_SAFE_NO_PAD.decode(segments[3]).expect("b64 decode");
    assert_eq!(pb_decoded, pb_bytes);
}

#[test]
fn v2_with_no_proof_bundle_emits_3_segments() {
    let (token, _holder) = sample_token();
    let wire = serialize_wire_v2(&token, None).expect("v2 serialize");
    let segments: Vec<&str> = wire.split('.').collect();
    assert_eq!(segments.len(), 3, "no proof_bundle → v1 3-segment shape");
    assert_eq!(wire, serialize_wire(&token).expect("v1 serialize"));
}

#[test]
fn v1_parser_ignores_4th_segment() {
    let (token, _holder) = sample_token();
    let wire_v2 = serialize_wire_v2(&token, Some(b"sentinel_for_v2")).expect("v2 serialize");
    // v1 deserialize_wire must NOT error on the 4th segment.
    let back = deserialize_wire(&wire_v2, &token.holder_did, token.holder_pub)
        .expect("v1 parser must succeed with v2 wire");
    assert_eq!(back.macaroon.root_id, token.macaroon.root_id);
}

#[test]
fn v2_parser_extracts_proof_bundle_borsh() {
    let (token, _holder) = sample_token();
    // Synthesize a real ProofBundle so the test exercises its serde_json path
    // (the wire_v2 module's borsh_compat is serde_json today; see TODO in
    // wire.rs).
    let pb = ProofBundle {
        stark_proof: vec![0xab; 64],
        public_inputs: PublicInputs {
            ask_id: [0x55; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 42)],
            cap_root_hash: [0x66; 32],
            invocation_hash: [0x77; 32],
            holder_did: sample_did(134).clone(),
            current_unix_time: 1_700_000_300,
            output_hash: None,
            provider_slot_id: "slot-bridge-001".to_owned(),
        },
        casm_hash: [0x42; 32],
        casm_version: 1,
        security_bits: 128,
        witness_format: zk_vendor::prover_input::WitnessFormat::BytesFallback,
        capability_v2: None,
    };
    let pb_bytes =
        octo_wallet::capability::zk_mint::proof_bundle_to_wire(&pb).expect("proof_bundle_to_wire");

    let wire_v2 = serialize_wire_v2(&token, Some(&pb_bytes)).expect("v2 serialize");
    let parsed =
        deserialize_wire_v2(&wire_v2, &token.holder_did, token.holder_pub).expect("v2 deserialize");
    // 4th segment round-trips byte-identical.
    let back_bytes = parsed
        .proof_bundle
        .expect("v2 parser must extract proof_bundle");
    assert_eq!(back_bytes, pb_bytes, "proof_bundle bytes round-trip exact");

    // The extracted bytes are valid: deserialize back to ProofBundle.
    let back_pb: ProofBundle =
        octo_wallet::capability::zk_mint::proof_bundle_from_wire(&back_bytes)
            .expect("proof_bundle_from_wire");
    assert_eq!(back_pb.casm_hash, pb.casm_hash);
    assert_eq!(back_pb.security_bits, pb.security_bits);
    assert_eq!(back_pb.stark_proof, pb.stark_proof);
}

#[test]
fn v2_parser_accepts_v1_3segment_wire() {
    let (token, _holder) = sample_token();
    let wire_v1 = serialize_wire(&token).expect("v1 serialize");
    let parsed = deserialize_wire_v2(&wire_v1, &token.holder_did, token.holder_pub)
        .expect("v2 parser must accept v1 wire");
    assert!(parsed.proof_bundle.is_none(), "v1 wire has no 4th segment");
    assert_eq!(parsed.token.macaroon.root_id, token.macaroon.root_id);
}

#[test]
fn v2_rejects_wrong_segment_count() {
    let err = deserialize_wire_v2("only.one", sample_did(244), [0u8; 32])
        .expect_err("must reject < 3 segments");
    assert!(matches!(
        err,
        octo_wallet::capability::wire::WireError::SegmentCount(2)
    ));

    let err = deserialize_wire_v2("a.b.c.d.e", sample_did(244), [0u8; 32])
        .expect_err("must reject > 4 segments");
    assert!(matches!(
        err,
        octo_wallet::capability::wire::WireError::SegmentCount(5)
    ));
}

#[test]
fn v2_rejects_oversize_wire_total() {
    // DoS guard: a >2MiB wire must be rejected before split/decode.
    let huge = "a".repeat(2 * 1024 * 1024 + 1);
    let err = deserialize_wire_v2(&huge, sample_did(244), [0u8; 32])
        .expect_err("DoS guard: must reject oversize wire");
    assert!(
        matches!(
            err,
            octo_wallet::capability::wire::WireError::WireTooLong(_, _)
        ),
        "expected WireTooLong, got {err:?}"
    );
}

#[test]
fn v2_rejects_oversize_segment() {
    // 1 MiB per-segment cap. A segment slightly over the cap must be rejected.
    let oversize_seg = "b".repeat(1024 * 1024 + 1);
    let wire = format!("a.b.c.{oversize_seg}");
    let err = deserialize_wire_v2(&wire, sample_did(244), [0u8; 32])
        .expect_err("DoS guard: must reject oversize segment");
    assert!(
        matches!(
            err,
            octo_wallet::capability::wire::WireError::SegmentTooLong(3, _, _)
        ),
        "expected SegmentTooLong, got {err:?}"
    );
}

// Reference compile-clean: NodeType import is reserved for a future
// embedding test that exercises the full mint_with_zk path on each
// NodeType variant; not needed for v2 wire round-trip.
