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
