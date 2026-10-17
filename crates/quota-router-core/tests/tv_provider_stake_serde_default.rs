//! Mission 0900-d2 — TV for `ProviderStake` serde-default hardening.
//!
//! Locks:
//! - JSON missing `chain_id` field deserializes with
//!   `chain_id = [0_u8; 32]` (the production `DEFAULT_CHAIN_ID` sentinel).
//! - JSON with explicit `chain_id` round-trips byte-exact.
//! - JSON missing any OTHER field still errors (default is
//!   per-attribute, not struct-wide).
//!
//! All three tests are byte-exact TV that lock the serde contract
//! against accidental regression. Layer-A substrate concern (no
//! randomness, no time, no I/O).

use octo_determin::Dqa;
use quota_router_core::marketplace::slashing::ProviderStake;

/// TV-1: missing `chain_id` defaults to all-zeros (`DEFAULT_CHAIN_ID`).
#[test]
fn provider_stake_json_without_chain_id_defaults_to_zero_sentinel() {
    // Build the canonical pre-0900-d1 payload (no chain_id field).
    // `Dqa` wire form is 16-byte BE per `DqaEncoding` (see
    // `crates/quota-router-storage/src/dqa_serde.rs::dqa_to_bytes`):
    //   bytes[0..8]  = i64 value BE
    //   bytes[8]     = scale (u8)
    //   bytes[9..16] = _reserved (must be all-zero per
    //                  `DqaError::InvalidEncoding` rejection)
    // For `Dqa::new(1, 0)`: value=1 → [0,0,0,0,0,0,0,1], scale=0,
    //   reserved=zero.
    // For `Dqa::new(2, 0)`: value=2 → [0,0,0,0,0,0,0,2], scale=0,
    //   reserved=zero.
    let json = r#"{
        "provider_id": "alice",
        "stake_micro_octo_w": [0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0],
        "initial_stake_micro_octo_w": [0,0,0,0,0,0,0,2,0,0,0,0,0,0,0,0],
        "offense_count": 0,
        "cumulative_loss_pct": 0.0
    }"#;
    let stake: ProviderStake = serde_json::from_str(json).expect("missing chain_id must default");
    assert_eq!(
        stake.chain_id, [0_u8; 32],
        "missing chain_id must default to DEFAULT_CHAIN_ID sentinel"
    );
    assert_eq!(stake.provider_id, "alice");
    assert_eq!(
        stake.stake_micro_octo_w,
        Dqa::new(1, 0).expect("non-overflow")
    );
    assert_eq!(
        stake.initial_stake_micro_octo_w,
        Dqa::new(2, 0).expect("non-overflow")
    );
    assert_eq!(stake.offense_count, 0);
    assert_eq!(stake.cumulative_loss_pct, 0.0);
}

/// TV-2: explicit `chain_id` round-trips byte-exact (no defaulting
/// regression).
#[test]
fn provider_stake_json_with_chain_id_round_trips_byte_exact() {
    let mut chain = [0_u8; 32];
    chain[0] = 0x01;
    chain[31] = 0xff;
    let stake = ProviderStake {
        chain_id: chain,
        provider_id: "bob".to_string(),
        stake_micro_octo_w: Dqa::new(1_000_000, 0).expect("non-overflow"),
        initial_stake_micro_octo_w: Dqa::new(1_000_000, 0).expect("non-overflow"),
        offense_count: 1,
        cumulative_loss_pct: 0.1,
    };
    let serialized = serde_json::to_string(&stake).expect("serialize");
    let deserialized: ProviderStake =
        serde_json::from_str(&serialized).expect("explicit chain_id round-trip");
    assert_eq!(stake, deserialized);
}

/// TV-3: missing `provider_id` (NOT decorated) still errors — locks
/// that `#[serde(default)]` is per-attribute, not struct-wide.
#[test]
fn provider_stake_json_missing_other_field_still_errors() {
    let json = r#"{
        "chain_id": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        "stake_micro_octo_w": "0000000000000001",
        "initial_stake_micro_octo_w": "0000000000000002",
        "offense_count": 0,
        "cumulative_loss_pct": 0.0
    }"#;
    let result: Result<ProviderStake, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "missing provider_id (un-decorated) must still error; got Ok"
    );
}
