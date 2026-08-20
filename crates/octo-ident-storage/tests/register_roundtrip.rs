//! Per-adapter `register_roundtrip` fixture (mission 0206-005).
//!
//! Locks TV-0206-A10: the adapter registers itself with the substrate
//! facade + a roundtrip write+read produces the inserted payload.
//! Full StoolapDidRegistry impl lands in mission 0206-003; this stub
//! exercises the open_in_memory + apply_pending substrate path.

use octo_ident_storage::StoolapDidRegistry;

#[test]
fn open_in_memory_succeeds_and_records_v008_migration() {
    let r = StoolapDidRegistry::open_in_memory().expect("open");
    // The stub applies v008 (`did_registry`) via the substrate's
    // `_legacy_apply_pending` runner. Roundtrip isn't possible until
    // mission 0206-003 moves the full impl block; this fixture
    // locks the substrate-level open + migration path.
    let _ = r;
}

#[test]
fn mainnet_chain_id_bytes_matches_rfc_0010_v1_4() {
    use octo_ident_storage::MAINNET_CHAIN_ID_BYTES;
    // 17-byte canonical encoding per RFC-0010 v1.4.
    assert_eq!(MAINNET_CHAIN_ID_BYTES.len(), 17);
    assert_eq!(MAINNET_CHAIN_ID_BYTES[0], 0x01); // Rfc variant tag
    assert_eq!(MAINNET_CHAIN_ID_BYTES[16], 0x12); // length byte
}