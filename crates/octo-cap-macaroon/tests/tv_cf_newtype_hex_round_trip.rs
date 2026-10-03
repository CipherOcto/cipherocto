#![cfg(feature = "hex-ids")]
//! Mission 0011-vault-asset-chain-id-hex - feature-gated round-trip
//! vectors for the three 32-byte id newtypes (`AssetId`, `ChainId`,
//! `VaultId`) under `--features hex-ids`.
//!
//! Pins AC-1 (each newtype serialises as 64-char lowercase hex) and
//! AC-2 (each newtype's `hex-ids` adapter accepts BOTH the 64-hex
//! canonical form AND the 32-element byte-array legacy form - dual-form
//! deserialize is delegated to `crate::hex_id_32` from the paired Layer
//! A mission 0011-caveat-form-amendment, which already exercises both
//! forms via `visit_str` / `visit_seq`).

use octo_cap_macaroon::{AssetId, ChainId, VaultId};

#[test]
fn tv_cf_10_asset_id_hex_round_trip() {
    let bytes = [0xab; 32];
    let id = AssetId::from_bytes(bytes);

    // Under --features hex-ids, AssetId serialises as 64-char lowercase hex.
    let json = serde_json::to_string(&id).expect("serialize");
    assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));

    // Round-trip via the canonical hex form.
    let restored: AssetId = serde_json::from_str(&json).expect("hex round trip");
    assert_eq!(restored.as_bytes(), &bytes);

    // Dual-form acceptance: the legacy 32-element byte-array form is
    // also accepted (delegated to hex_id_32::visit_seq).
    let arr_json = serde_json::to_string(&bytes.to_vec()).expect("array serialize");
    let restored: AssetId = serde_json::from_str(&arr_json).expect("byte-array accepts");
    assert_eq!(restored.as_bytes(), &bytes);
}

#[test]
fn tv_cf_11_chain_id_hex_round_trip() {
    let bytes = [0xcd; 32];
    let id = ChainId::from_bytes(bytes);

    let json = serde_json::to_string(&id).expect("serialize");
    assert_eq!(json, format!("\"{}\"", "cd".repeat(32)));

    let restored: ChainId = serde_json::from_str(&json).expect("hex round trip");
    assert_eq!(restored.as_bytes(), &bytes);

    let arr_json = serde_json::to_string(&bytes.to_vec()).expect("array serialize");
    let restored: ChainId = serde_json::from_str(&arr_json).expect("byte-array accepts");
    assert_eq!(restored.as_bytes(), &bytes);
}

#[test]
fn tv_cf_12_vault_id_hex_round_trip() {
    let bytes = [0xef; 32];
    let id = VaultId::from_bytes(bytes);

    let json = serde_json::to_string(&id).expect("serialize");
    assert_eq!(json, format!("\"{}\"", "ef".repeat(32)));

    let restored: VaultId = serde_json::from_str(&json).expect("hex round trip");
    assert_eq!(restored.as_bytes(), &bytes);

    let arr_json = serde_json::to_string(&bytes.to_vec()).expect("array serialize");
    let restored: VaultId = serde_json::from_str(&arr_json).expect("byte-array accepts");
    assert_eq!(restored.as_bytes(), &bytes);
}
