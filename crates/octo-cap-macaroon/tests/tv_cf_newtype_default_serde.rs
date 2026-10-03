#![cfg(not(feature = "hex-ids"))]
//! Mission 0011-vault-asset-chain-id-hex - default-feature regression
//! vector for the three 32-byte id newtypes (`AssetId`, `ChainId`,
//! `VaultId`).
//!
//! Pins AC-3: under default features (no `hex-ids`), the derived
//! `Serialize` form is unchanged - 32-element byte array. The
//! `hex-ids` capability defaults OFF per the open-limitations drift
//! audit's "no amendment warranted" verdict on the newtype change
//! (the migration is opt-in).

use octo_cap_macaroon::{AssetId, ChainId, VaultId};

#[test]
fn tv_cf_13_default_newtype_serde_is_byte_array() {
    let bytes = [0xab; 32];
    let arr_json = serde_json::to_string(&bytes.to_vec()).expect("array serialize");

    // AssetId - derived Serialize emits the 32-element byte array.
    let asset_json = serde_json::to_string(&AssetId::from_bytes(bytes)).expect("serialize");
    assert_eq!(
        asset_json, arr_json,
        "AssetId must serialise as 32-element byte array under default features"
    );
    let restored: AssetId = serde_json::from_str(&asset_json).expect("default round trip");
    assert_eq!(restored.as_bytes(), &bytes);

    // ChainId.
    let chain_json = serde_json::to_string(&ChainId::from_bytes(bytes)).expect("serialize");
    assert_eq!(
        chain_json, arr_json,
        "ChainId must serialise as 32-element byte array under default features"
    );
    let restored: ChainId = serde_json::from_str(&chain_json).expect("default round trip");
    assert_eq!(restored.as_bytes(), &bytes);

    // VaultId.
    let vault_json = serde_json::to_string(&VaultId::from_bytes(bytes)).expect("serialize");
    assert_eq!(
        vault_json, arr_json,
        "VaultId must serialise as 32-element byte array under default features"
    );
    let restored: VaultId = serde_json::from_str(&vault_json).expect("default round trip");
    assert_eq!(restored.as_bytes(), &bytes);
}
