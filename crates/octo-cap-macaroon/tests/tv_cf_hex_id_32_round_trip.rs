use octo_cap_macaroon::hex_id_32;
use serde::{Deserialize, Serialize};

/// Wrapper that applies the hex_id_32 adapter. `[u8; 32]` is the
/// substrate id type; the wrapper forces serde to route through the
/// adapter's `serialize_str` / `deserialize_any`.
#[derive(Debug, Serialize, Deserialize)]
struct Id32(#[serde(with = "hex_id_32")] [u8; 32]);

#[test]
fn tv_cf_04_hex_id_32_round_trip_string() {
    let bytes = [0xab; 32];
    let json = serde_json::to_string(&Id32(bytes)).expect("serialize");
    // `hex_id_32::serialize` emits a 64-char hex string. (Without the
    // adapter, the derived form would emit a 32-element array; this
    // assertion pins the adapter.)
    assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
    let restored: Id32 = serde_json::from_str(&json).expect("round trip");
    assert_eq!(restored.0, bytes);
}

#[test]
fn tv_cf_05_hex_id_32_accepts_legacy_array_form() {
    let bytes = [0xab; 32];
    // The legacy 32-element array form must still parse through the
    // adapter. (Pin so a future "hex only" change surfaces immediately.)
    let arr: Vec<u8> = bytes.to_vec();
    let json = serde_json::to_string(&arr).expect("array serialize");
    let restored: Id32 = serde_json::from_str(&json).expect("legacy form parses");
    assert_eq!(restored.0, bytes);
}

#[test]
fn tv_cf_06_hex_id_32_rejects_short_hex() {
    let e: Result<Id32, _> = serde_json::from_str("\"abcd\"");
    assert!(e.is_err());
}

/// Position sensitivity of the shared adapter.
///
/// `tv_cf_04` / `tv_cf_05` both use `[0xab; 32]`, and the Phase 2
/// newtype vectors `tv_cf_10..13` each use a single repeated byte too. A
/// constant array is a **fixed point of every byte permutation**, so those
/// vectors stay green under a byte-order, reversal, or rotation defect in
/// the adapter. This vector uses a ramp (byte `i` holds `base + i`) so
/// every position is distinguishable, and asserts the encoded string
/// against an independently written literal rather than a value computed
/// through the adapter under test.
///
/// Not feature-gated: `Caveat::Vault` carries an unconditional
/// `#[serde(with = "crate::hex_id_32")]`, so the adapter is on the default
/// feature path and this vector runs in the ordinary CI test gate. It
/// therefore covers the `Caveat::Vault` application as well as the
/// feature-gated newtype applications.
#[test]
fn tv_cf_14_hex_id_32_preserves_byte_positions() {
    let bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
    let json = serde_json::to_string(&Id32(bytes)).expect("serialize");

    // Independently written expectation. Reversed would be
    // 1f1e1d..00; rotated would be 000102..1e1f00 -- neither matches.
    assert_eq!(
        json, "\"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\"",
        "adapter must encode byte i at hex position i"
    );

    let restored: Id32 = serde_json::from_str(&json).expect("round trip");
    assert_eq!(restored.0, bytes);

    // A one-byte rotation of the input must NOT decode to the original.
    let mut rotated = bytes;
    rotated.rotate_left(1);
    let rotated_json = serde_json::to_string(&Id32(rotated)).expect("serialize rotated");
    assert_ne!(
        rotated_json, json,
        "a rotated byte pattern must encode differently"
    );
}

/// The legacy array form is length-checked, not merely drained.
///
/// `tv_cf_05` only ever feeds a well-formed 32-element array, so dropping
/// the `visit_seq` length guard would leave it green. This vector pins the
/// rejection of a short array. Without the guard the adapter reaches
/// `copy_from_slice` and panics on the length mismatch, so the vector
/// fails either way -- the point is that the rejection is asserted rather
/// than incidental.
#[test]
fn tv_cf_15_hex_id_32_rejects_short_array_form() {
    let short: Vec<u8> = (0u8..31).collect();
    let json = serde_json::to_string(&short).expect("array serialize");
    let e: Result<Id32, _> = serde_json::from_str(&json);
    assert!(e.is_err(), "31-element array must not be accepted");

    let long: Vec<u8> = (0u8..33).collect();
    let json = serde_json::to_string(&long).expect("array serialize");
    let e: Result<Id32, _> = serde_json::from_str(&json);
    assert!(e.is_err(), "33-element array must not be accepted");
}

/// Hex case leniency is deliberate, not incidental.
///
/// The canonical wire form is 64-char **lowercase** hex (that is what
/// `serialize` emits, pinned by `tv_cf_14`). On the way in, `hex::decode`
/// also accepts uppercase. This vector pins that leniency as a conscious
/// migration affordance so a future tightening is a decision someone
/// makes rather than a behaviour that changes by accident.
#[test]
fn tv_cf_16_hex_id_32_accepts_uppercase_hex_input() {
    let uppercase = "\"000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F\"";
    let restored: Id32 = serde_json::from_str(uppercase).expect("uppercase hex accepted");
    assert_eq!(restored.0, core::array::from_fn(|i| i as u8));
}
