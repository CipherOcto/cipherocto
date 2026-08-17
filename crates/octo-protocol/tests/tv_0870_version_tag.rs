//! RFC-0870 §NodeEnvelope Version Tag — TV-0870-01 byte-exact wire form.
//!
//! Pins the `version_tag: u8` field placement and value (V2 = `0xA1`) in the
//! canonical borsh serialization of `NodeEnvelope`. Verifies:
//!
//! 1. `NodeEnvelope::build(..., VERSION_TAG_V2)` accepts V2 and writes the
//!    byte `0xA1` at the canonical position (after `envelope_id`, before
//!    `from_did`).
//! 2. `NodeEnvelope::build(..., VERSION_TAG_V1)` accepts V1 (legacy path)
//!    and writes the byte `0xA0` at the same position.
//! 3. `NodeEnvelope::build(..., 0xFF)` is REJECTED at construction with
//!    `ProtocolError::UnsupportedVersion(0xFF)`.
//! 4. `NodeEnvelope::verify_version` returns `Ok(())` for V2 and
//!    `Err(UnsupportedVersion(v))` for V1 and any other value (RFC-0870
//!    §14.1: V1 hard-rejected at verify post-cutover).
//! 5. V1 and V2 receipts of identical (other) inputs produce DISTINCT
//!    `envelope_id` values — the `envelope_id` derivation includes
//!    `version_tag`. (NB: since V1 receipts are hard-rejected at
//!    `verify_version`, this is the version_tag-participates-in-hash
//!    invariant, not a literal V1-replay-defense assertion. Future TV
//!    can pin replay detection directly.)
//! 6. The runtime gate (`verify_version`) rejects V1 and unknown
//!    `version_tag` values even when the envelope is constructed via
//!    struct literal (bypassing `NodeEnvelope::build`).
//! 7. The `version_tag` field sits at byte offset 32 of the canonical
//!    serialization (immediately after the 32-byte `envelope_id`).
//! 8. Absent `version_tag` field at deserialization (truncated bytes
//!    before offset 32) is rejected by `borsh::from_slice` — the field
//!    is mandatory in canonical encoding, not silently defaulted.
//!
//! Pattern mirrors `crates/octo-protocol/tests/tv8_borsh_parity.rs` — byte-
//! exact canonical_ser + BLAKE3-256 envelope_id pinning.

use ed25519_dalek::SigningKey;
use octo_protocol::envelope::{NodeEnvelope, VERSION_TAG_V1, VERSION_TAG_V2};
use octo_protocol::error::ProtocolError;
use octo_protocol::payload_kind::IDENTITY_RESOLVE;
use octo_protocol::recipient::RecipientRef;

const TV_0870_NONCE: [u8; 32] = [0x42; 32];
const TV_0870_EXPIRES_AT_UNIX_MS: u64 = 1_735_689_600_000;
const TV_0870_RECIPIENT: [u8; 32] = [0x07; 32];

fn build_with_version_tag(version_tag: u8) -> NodeEnvelope {
    let seed = [0xAB; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    NodeEnvelope::build(
        from_did,
        RecipientRef::Direct(TV_0870_RECIPIENT),
        IDENTITY_RESOLVE,
        vec![0x01, 0x02, 0x03],
        vec![],
        TV_0870_NONCE,
        TV_0870_EXPIRES_AT_UNIX_MS,
        version_tag,
    )
    .expect("TV-0870 envelope build")
}

#[test]
fn tv_0870_01_v2_build_accepts_and_round_trips() {
    let env = build_with_version_tag(VERSION_TAG_V2);
    assert_eq!(env.version_tag, VERSION_TAG_V2);
    assert_eq!(env.version_tag, 0xA1);

    // Round-trip preserves version_tag.
    let bytes = borsh::to_vec(&env).expect("borsh serialize");
    let back: NodeEnvelope = borsh::from_slice(&bytes).expect("borsh deserialize");
    assert_eq!(
        back.version_tag, VERSION_TAG_V2,
        "TV-0870 round-trip must preserve V2 version_tag"
    );
}

#[test]
fn tv_0870_01_v1_build_accepts_legacy_path() {
    // V1 is hard-rejected at verify post-cutover, but `build` still
    // accepts the byte at construction time so historical fixtures can
    // replay. The verify-time gate lives in `verify_version` /
    // operational rejection paths.
    let env = build_with_version_tag(VERSION_TAG_V1);
    assert_eq!(env.version_tag, VERSION_TAG_V1);
    assert_eq!(env.version_tag, 0xA0);
}

#[test]
fn tv_0870_01_unknown_tag_rejected_at_build() {
    let seed = [0xAB; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let err = NodeEnvelope::build(
        from_did,
        RecipientRef::Direct(TV_0870_RECIPIENT),
        IDENTITY_RESOLVE,
        vec![0x01, 0x02, 0x03],
        vec![],
        TV_0870_NONCE,
        TV_0870_EXPIRES_AT_UNIX_MS,
        0xFF, // unknown tag
    )
    .expect_err("unknown version_tag must be rejected at build");
    assert!(
        matches!(err, ProtocolError::UnsupportedVersion(0xFF)),
        "expected UnsupportedVersion(0xFF), got {err:?}"
    );
}

#[test]
fn tv_0870_01_verify_version_gate() {
    let v2 = build_with_version_tag(VERSION_TAG_V2);
    assert!(v2.verify_version().is_ok(), "V2 verify_version must pass");

    let v1 = build_with_version_tag(VERSION_TAG_V1);
    // V1 is REJECTED at verify per RFC-0870 §14.1 (operational rejection
    // is part of `verify_version` itself, not deferred to the caller).
    assert!(
        matches!(
            v1.verify_version(),
            Err(ProtocolError::UnsupportedVersion(VERSION_TAG_V1))
        ),
        "V1 receipts MUST be rejected at verify per RFC-0870 §14.1"
    );

    let bad = NodeEnvelope {
        version_tag: 0x42,
        ..build_with_version_tag(VERSION_TAG_V2)
    };
    assert!(
        matches!(
            bad.verify_version(),
            Err(ProtocolError::UnsupportedVersion(0x42))
        ),
        "unknown version_tag must fail verify_version"
    );
}

#[test]
fn tv_0870_01_v1_and_v2_envelope_ids_differ() {
    // `envelope_id` derivation includes `version_tag`, so V1 and V2
    // receipts of the same logical payload produce distinct IDs. Since
    // V1 receipts are hard-rejected at `verify_version`, this pins the
    // version_tag-participates-in-hash invariant rather than a literal
    // V1-replay-defense assertion.
    let v1 = build_with_version_tag(VERSION_TAG_V1);
    let v2 = build_with_version_tag(VERSION_TAG_V2);
    assert_ne!(
        v1.envelope_id, v2.envelope_id,
        "TV-0870: V1 and V2 receipts MUST have distinct envelope_id (version_tag is hashed)"
    );
}

#[test]
fn tv_0870_01_byte_position_pin() {
    // Pins the wire-form byte position of `version_tag` per RFC-0870 §NodeEnvelope
    // Version Tag: after the 32-byte `envelope_id`, before `from_did`. Borsh
    // serializes struct fields in declaration order; this test fails fast if
    // the field order ever drifts and silently changes the wire form.
    let env = build_with_version_tag(VERSION_TAG_V2);
    let bytes = borsh::to_vec(&env).expect("borsh serialize");
    // 32 bytes for envelope_id, then version_tag.
    assert_eq!(
        bytes[32], 0xA1,
        "TV-0870 byte position: version_tag MUST sit at offset 32 (immediately after envelope_id)"
    );
    let env_v1 = build_with_version_tag(VERSION_TAG_V1);
    let bytes_v1 = borsh::to_vec(&env_v1).expect("borsh serialize v1");
    assert_eq!(
        bytes_v1[32], 0xA0,
        "TV-0870 byte position: V1 version_tag byte MUST be 0xA0 at offset 32"
    );
}

#[test]
fn tv_0870_01_runtime_gate_rejects_bypassed_unknown_tag() {
    // Regression pin for HIGH-3: even if a future refactor removes the
    // build-time guard and relies on a post-construction `validate_*`
    // step, the runtime gate MUST still reject `version_tag = 0xFF` when
    // the envelope is constructed via struct literal (bypassing
    // `NodeEnvelope::build`). This test fails fast if the runtime gate
    // ever drifts.
    let env_v2 = build_with_version_tag(VERSION_TAG_V2);
    let bypassed_v1 = NodeEnvelope {
        version_tag: VERSION_TAG_V1,
        ..env_v2.clone()
    };
    assert!(
        matches!(
            bypassed_v1.verify_version(),
            Err(ProtocolError::UnsupportedVersion(VERSION_TAG_V1))
        ),
        "runtime gate MUST reject V1 even when struct-literal-bypassed"
    );
    let bypassed_bad = NodeEnvelope {
        version_tag: 0xFF,
        ..env_v2
    };
    assert!(
        matches!(
            bypassed_bad.verify_version(),
            Err(ProtocolError::UnsupportedVersion(0xFF))
        ),
        "runtime gate MUST reject unknown tag even when struct-literal-bypassed"
    );
}

#[test]
fn tv_0870_01_absent_version_tag_field_rejected() {
    // Per RFC-0870 §NodeEnvelope Version Tag Verify contract: "Absent
    // `version_tag` field at deserialization is also rejected (the field
    // is mandatory in canonical encoding; missing byte = borsh decode
    // error, not silent zero)." Truncate the canonical bytes before
    // `version_tag` and assert `borsh::from_slice` returns Err. A
    // regression that adds `#[borsh(default)]` or `pub version_tag: u8
    // = 0` would silently default to 0 and this test would fail loud.
    let env = build_with_version_tag(VERSION_TAG_V2);
    let mut bytes = borsh::to_vec(&env).expect("borsh serialize");
    assert!(
        bytes.len() > 32,
        "sanity: serialized envelope must be longer than envelope_id"
    );
    bytes.truncate(32); // drop version_tag + everything after
    let r: Result<NodeEnvelope, _> = borsh::from_slice(&bytes);
    assert!(
        r.is_err(),
        "missing version_tag byte MUST borsh-decode-fail, not silently default to 0 (got {:?})",
        r.map(|e| e.version_tag)
    );
}
