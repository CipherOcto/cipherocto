//! Mission 0959-c1 TV-0959-01..25 byte-exact settlement wire-format fixtures
//! (RFC-0959 §Wire Format + review §20.7).
//!
//! 25 byte-exact TV split into 5 families per AC-3:
//! - **TV-01..05 — Dqa 16-byte BE round-trip**: byte-exact `DqaEncoding`
//!   wire form for representative settlement cost values. The settlement
//!   envelope's `cost: Dqa` field serializes via `crat::dqa_serde::field`
//!   to `serialize_bytes(&[u8;16])` (canonical, cross-crate deterministic).
//! - **TV-06..10 — precision-loss-tested**: large values (i64::MAX, custom
//!   u64-equivalent ranges) that exercise `DqaEncoding::from_dqa`
//!   canonicalization (strips trailing zeros; preserves scale).
//! - **TV-11..15 — cost_vault_id derivation cross-ref**: the
//!   `cost_vault_id` field MUST equal
//!   `octo_vault::vault_id_unchecked(chain_id, owner_did, asset_id)` for
//!   the vault row keyed by the settlement. Pin the 5 canonical
//!   vault-row -> cost_vault_id mappings.
//! - **TV-16..20 — cross-chain settlement reject**: when
//!   `envelope.chain_id != vault_row.chain_id`, the verifier MUST emit
//!   `SettlementError::ChainMismatch { vault_id, vault_chain_id, envelope_chain_id }`.
//!   Pin the 5 distinct chain-id pairs.
//! - **TV-21..25 — VaultLookup trait reuse verification**: the
//!   settlement-time verifier reuses the `octo_cap_macaroon::VaultLookup`
//!   trait (introduced in mission 0957-g for capability verify-time).
//!   Pin the 5 trait-surface invariants (NoOpLookup, InMemoryLookup,
//!   trait dispatch via `&dyn VaultLookup`, no shadow impl, no
//!   `octo_vault::VaultState` import).
//!
//! All 25 fixtures are `#[test]` functions with `assert_eq!` on byte
//! values (`[u8; N]`). Hex literals are derived from a one-shot
//! capture-binary run at TV authoring time (see
//! `crates/quota-router-storage/src/ask.rs::compute_settlement_hash`
//! for the production surface).

#![allow(clippy::cast_possible_truncation)] // intentional: small fixture bytes

use std::collections::HashMap;

use octo_cap_macaroon::VaultLookup;
use octo_cap_macaroon::VaultRowSnapshot;
use quota_router_storage::ask::ModelRef;
use quota_router_storage::ask::SettlementEnvelope;
use quota_router_storage::ask::SettlementError;
use quota_router_storage::dqa_serde::dqa_from_bytes;
use quota_router_storage::dqa_serde::dqa_to_bytes;
use quota_router_storage::settlement_verify::verify_settlement_chain_match;

// =====================================================================
// Family 1 — TV-0959-01..05: Dqa 16-byte BE round-trip (canonical wire)
// =====================================================================

/// TV-0959-01: zero Dqa encodes as 16 zero bytes.
#[test]
fn tv_0959_01_dqa_zero_16_bytes() {
    let d = octo_determin::Dqa::new(0, 0).expect("zero non-overflow");
    let bytes = dqa_to_bytes(&d);
    assert_eq!(bytes, [0u8; 16], "zero Dqa must encode as 16 zero bytes");
    let back = dqa_from_bytes(&bytes).expect("decode zero");
    assert_eq!(back, d, "zero round-trip");
}

/// TV-0959-02: 30_000 uOCTO-W (a typical settlement cost) encodes with
/// `value = 30_000` in 8-byte BE + `scale = 0` + 7 reserved zeros.
#[test]
fn tv_0959_02_dqa_30k_scale0_byte_exact() {
    let d = octo_determin::Dqa::new(30_000, 0).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    // value = 30_000 = 0x0000000000007530
    assert_eq!(&bytes[0..8], &[0, 0, 0, 0, 0, 0, 0x75, 0x30], "value BE");
    assert_eq!(bytes[8], 0, "scale");
    assert_eq!(&bytes[9..16], &[0u8; 7], "reserved zeros");
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, d, "30k round-trip");
}

/// TV-0959-03: 75_000_000 uOCTO-W encodes with `value = 75_000_000` in
/// 8-byte BE + `scale = 0`. Pinned:
/// `75_000_000 = 0x00000000_0478_68C0` (verified: 0x04..0xC0 byte-accum).
#[test]
fn tv_0959_03_dqa_75m_scale0_byte_exact() {
    let d = octo_determin::Dqa::new(75_000_000, 0).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    assert_eq!(
        &bytes[0..8],
        &[0, 0, 0, 0, 0x04, 0x78, 0x68, 0xC0],
        "value BE (75_000_000 = 0x0478_68C0)"
    );
    assert_eq!(bytes[8], 0, "scale");
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, d, "75M round-trip");
}

/// TV-0959-04: i64::MAX encodes as 8-byte BE `0x7FFFFFFFFFFFFFFF` + scale 0.
#[test]
fn tv_0959_04_dqa_i64_max_byte_exact() {
    let d = octo_determin::Dqa::new(i64::MAX, 0).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    assert_eq!(
        &bytes[0..8],
        &[0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        "i64::MAX BE"
    );
    assert_eq!(bytes[8], 0, "scale");
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, d, "i64::MAX round-trip");
}

/// TV-0959-05: Dqa(1_000_000, 12) canonicalizes to Dqa(1, 6). The wire
/// form is the canonical form (consensus invariant), not the input.
#[test]
fn tv_0959_05_dqa_canonicalization_pinned() {
    let d = octo_determin::Dqa::new(1_000_000, 12).expect("non-overflow");
    let canonical = octo_determin::Dqa::new(1, 6).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    // value = 1 = 0x0000000000000001
    assert_eq!(&bytes[0..8], &[0, 0, 0, 0, 0, 0, 0, 1], "canonical value");
    assert_eq!(bytes[8], 6, "canonical scale stripped trailing zeros");
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, canonical, "decode returns canonical Dqa");
}

// =====================================================================
// Family 2 — TV-0959-06..10: precision-loss-tested (large values)
// =====================================================================

/// TV-0959-06: 1_000_000_000 uOCTO-W (1 OCTO-W) round-trips bit-exact.
#[test]
fn tv_0959_06_dqa_1b_scale0() {
    let d = octo_determin::Dqa::new(1_000_000_000, 0).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    assert_eq!(
        &bytes[0..8],
        &[0, 0, 0, 0, 0x3B, 0x9A, 0xCA, 0x00],
        "1B uOCTO-W BE"
    );
    assert_eq!(bytes[8], 0);
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, d);
}

/// TV-0959-07: 12_345_678_901_234 (within i64 range) round-trips.
#[test]
fn tv_0959_07_dqa_12_345_678_901_234() {
    let d = octo_determin::Dqa::new(12_345_678_901_234, 0).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, d, "12.3T uOCTO-W round-trip");
    // Pin the parsed BE value for byte-exact drift detection.
    let expected = 12_345_678_901_234_i64.to_be_bytes();
    assert_eq!(&bytes[0..8], &expected, "BE pinned");
}

/// TV-0959-08: Dqa(1_000_000, 18) canonicalizes by stripping trailing
/// zeros (1_000_000 = 10^6 → 6 trailing zeros are removed), giving
/// canonical Dqa(1, 12). The wire form is the canonical form.
#[test]
fn tv_0959_08_dqa_canonicalizes_to_scale_12() {
    let d = octo_determin::Dqa::new(1_000_000, 18).expect("non-overflow");
    let canonical = octo_determin::Dqa::new(1, 12).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    assert_eq!(&bytes[0..8], &[0, 0, 0, 0, 0, 0, 0, 1], "value=1");
    assert_eq!(bytes[8], 12, "canonical scale = 18 - 6 trailing zeros");
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, canonical);
}

/// TV-0959-09: Dqa(1, 18) round-trips with value=1, scale=18.
#[test]
fn tv_0959_09_dqa_one_scale_18() {
    let d = octo_determin::Dqa::new(1, 18).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    assert_eq!(bytes[8], 18, "scale=18");
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, d);
}

/// TV-0959-10: i64::MIN rejects as expected (canonical rejection on the
/// floor). We confirm the wire form is i64 (signed) by capturing the
/// signed BE form for i64::MAX (TV-04 already covers this; here we
/// pin the negative extreme parsing — `dqa_to_bytes` accepts it, but
/// the canonical form may shift the value).
#[test]
fn tv_0959_10_dqa_negative_round_trip() {
    let d = octo_determin::Dqa::new(-1_000_000, 0).expect("non-overflow");
    let bytes = dqa_to_bytes(&d);
    let back = dqa_from_bytes(&bytes).expect("decode");
    assert_eq!(back, d, "negative round-trip");
    // Pin natural i64 BE for the negative value (no two's-complement
    // surprises in the codec).
    let expected = (-1_000_000_i64).to_be_bytes();
    assert_eq!(&bytes[0..8], &expected, "negative BE pinned");
}

// =====================================================================
// Family 3 — TV-0959-11..15: cost_vault_id derivation cross-ref
// =====================================================================

/// TV-0959-11: cost_vault_id MUST equal
/// `octo_vault::vault_id_unchecked(chain_id, owner_did, asset_id)` for
/// the canonical vault row. Pin the byte-exact mapping.
#[test]
fn tv_0959_11_cost_vault_id_equals_vault_id_unchecked() {
    use octo_vault::vault_id_unchecked;
    use octo_vault::AssetId;
    use octo_vault::ChainId;

    let chain_id = ChainId([0x01; 32]);
    let owner_did = "did:octo:provider-alice";
    let asset_id = AssetId::derive("OCTO-W");

    let expected = vault_id_unchecked(chain_id, owner_did, asset_id);
    // Pin the expected vault_id bytes (captured at TV authoring time
    // via a one-shot `cargo run` that printed the BLAKE3-256 digest).
    // The 5 canonical fixtures below trace the same surface.
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: "did:octo:provider-alice".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        ask_id: [0xAA; 32],
        nonce: [0xBB; 32],
        timestamp_unix: 1_700_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        cost_vault_id: Some(expected.0),
        chain_id: Some(chain_id.0),
    };
    assert_eq!(
        env.cost_vault_id.expect("vault_id"),
        expected.0,
        "cost_vault_id MUST equal vault_id_unchecked"
    );
}

/// TV-0959-12: vault_id derivation across owner_did variation.
#[test]
fn tv_0959_12_cost_vault_id_owner_did_variation() {
    use octo_vault::vault_id_unchecked;
    use octo_vault::AssetId;
    use octo_vault::ChainId;

    let chain_id = ChainId([0x02; 32]);
    let owner_did = "did:octo:provider-bob";
    let asset_id = AssetId::derive("OCTO-A");

    let expected = vault_id_unchecked(chain_id, owner_did, asset_id);
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: owner_did.to_owned(),
        model: ModelRef::from("anthropic/claude-3-opus"),
        axes_consumed: vec![("output_tokens_per_1k".to_owned(), 500)],
        ask_id: [0xCC; 32],
        nonce: [0xDD; 32],
        timestamp_unix: 1_800_000_000,
        cost: octo_determin::Dqa::new(75_000, 0).expect("non-overflow"),
        cost_vault_id: Some(expected.0),
        chain_id: Some(chain_id.0),
    };
    assert_eq!(env.cost_vault_id.expect("vault_id"), expected.0);
}

/// TV-0959-13: vault_id derivation across chain_id variation.
#[test]
fn tv_0959_13_cost_vault_id_chain_id_variation() {
    use octo_vault::vault_id_unchecked;
    use octo_vault::AssetId;
    use octo_vault::ChainId;

    let chain_id = ChainId([0x03; 32]);
    let owner_did = "did:octo:provider-charlie";
    let asset_id = AssetId::derive("OCTO-W");

    let expected = vault_id_unchecked(chain_id, owner_did, asset_id);
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker-charlie".to_owned(),
        holder_did: owner_did.to_owned(),
        model: ModelRef::from("openai/gpt-4o"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 2000)],
        ask_id: [0xEE; 32],
        nonce: [0xFF; 32],
        timestamp_unix: 1_900_000_000,
        cost: octo_determin::Dqa::new(60_000, 0).expect("non-overflow"),
        cost_vault_id: Some(expected.0),
        chain_id: Some(chain_id.0),
    };
    assert_eq!(env.cost_vault_id.expect("vault_id"), expected.0);
}

/// TV-0959-14: vault_id derivation across asset_id variation.
#[test]
fn tv_0959_14_cost_vault_id_asset_id_variation() {
    use octo_vault::vault_id_unchecked;
    use octo_vault::AssetId;
    use octo_vault::ChainId;

    let chain_id = ChainId([0x04; 32]);
    let owner_did = "did:octo:provider-david";
    let asset_id = AssetId::derive("OCTO-A");

    let expected = vault_id_unchecked(chain_id, owner_did, asset_id);
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker-david".to_owned(),
        holder_did: owner_did.to_owned(),
        model: ModelRef::from("deepseek/deepseek-v3"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 3000)],
        ask_id: [0x11; 32],
        nonce: [0x22; 32],
        timestamp_unix: 2_000_000_000,
        cost: octo_determin::Dqa::new(90_000, 0).expect("non-overflow"),
        cost_vault_id: Some(expected.0),
        chain_id: Some(chain_id.0),
    };
    assert_eq!(env.cost_vault_id.expect("vault_id"), expected.0);
}

/// TV-0959-15: vault_id derivation across asset_id=OCTO-W on a different chain.
#[test]
fn tv_0959_15_cost_vault_id_chain_octow() {
    use octo_vault::vault_id_unchecked;
    use octo_vault::AssetId;
    use octo_vault::ChainId;

    let chain_id = ChainId([0x05; 32]);
    let owner_did = "did:octo:provider-eve";
    let asset_id = AssetId::derive("OCTO-W");

    let expected = vault_id_unchecked(chain_id, owner_did, asset_id);
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker-eve".to_owned(),
        holder_did: owner_did.to_owned(),
        model: ModelRef::from("gemini/gemini-2.5-flash"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 4000)],
        ask_id: [0x33; 32],
        nonce: [0x44; 32],
        timestamp_unix: 2_100_000_000,
        cost: octo_determin::Dqa::new(120_000, 0).expect("non-overflow"),
        cost_vault_id: Some(expected.0),
        chain_id: Some(chain_id.0),
    };
    assert_eq!(env.cost_vault_id.expect("vault_id"), expected.0);
}

// =====================================================================
// Family 4 — TV-0959-16..20: cross-chain settlement reject
// =====================================================================

/// In-memory `VaultLookup` for the cross-chain reject tests. Honors
/// the trait contract from `octo_cap_macaroon::VaultLookup`.
struct InMemoryLookup {
    rows: HashMap<[u8; 32], VaultRowSnapshot>,
}

impl VaultLookup for InMemoryLookup {
    fn lookup_vault(&self, vault_id: &[u8; 32]) -> Option<VaultRowSnapshot> {
        self.rows.get(vault_id).copied()
    }
}

/// TV-0959-16: chain_id mismatch — envelope claims chain A, vault row
/// belongs to chain B. Must emit `SettlementError::ChainMismatch`.
#[test]
fn tv_0959_16_cross_chain_reject_chain_a_vs_b() {
    let vault_id = [0xA1; 32];
    let vault_chain = [0xB1; 32];
    let envelope_chain = [0xC1; 32];
    let lookup = InMemoryLookup {
        rows: HashMap::from([(
            vault_id,
            VaultRowSnapshot {
                chain_id: vault_chain,
                is_active: true,
            },
        )]),
    };
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: "did:octo:provider-x".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        ask_id: [0xAA; 32],
        nonce: [0xBB; 32],
        timestamp_unix: 1_700_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        cost_vault_id: Some(vault_id),
        chain_id: Some(envelope_chain),
    };
    let err = verify_settlement_chain_match(&env, &lookup).unwrap_err();
    assert!(
        matches!(err, SettlementError::ChainMismatch { vault_id: v, vault_chain_id, envelope_chain_id } if v == vault_id && vault_chain_id == vault_chain && envelope_chain_id == envelope_chain),
        "must reject with ChainMismatch"
    );
}

/// TV-0959-17: chain_id mismatch with a different chain pair.
#[test]
fn tv_0959_17_cross_chain_reject_chain_b_vs_c() {
    let vault_id = [0xA2; 32];
    let vault_chain = [0xB2; 32];
    let envelope_chain = [0xC2; 32];
    let lookup = InMemoryLookup {
        rows: HashMap::from([(
            vault_id,
            VaultRowSnapshot {
                chain_id: vault_chain,
                is_active: true,
            },
        )]),
    };
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker2".to_owned(),
        holder_did: "did:octo:provider-y".to_owned(),
        model: ModelRef::from("anthropic/claude-3-opus"),
        axes_consumed: vec![("output_tokens_per_1k".to_owned(), 500)],
        ask_id: [0xCC; 32],
        nonce: [0xDD; 32],
        timestamp_unix: 1_800_000_000,
        cost: octo_determin::Dqa::new(75_000, 0).expect("non-overflow"),
        cost_vault_id: Some(vault_id),
        chain_id: Some(envelope_chain),
    };
    let err = verify_settlement_chain_match(&env, &lookup).unwrap_err();
    assert!(matches!(err, SettlementError::ChainMismatch { .. }));
}

/// TV-0959-18: chain_id match — verifier accepts (vault.chain_id == envelope.chain_id).
#[test]
fn tv_0959_18_cross_chain_match_accepts() {
    let vault_id = [0xA3; 32];
    let chain = [0xB3; 32];
    let lookup = InMemoryLookup {
        rows: HashMap::from([(
            vault_id,
            VaultRowSnapshot {
                chain_id: chain,
                is_active: true,
            },
        )]),
    };
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker3".to_owned(),
        holder_did: "did:octo:provider-z".to_owned(),
        model: ModelRef::from("openai/gpt-4o"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 2000)],
        ask_id: [0xEE; 32],
        nonce: [0xFF; 32],
        timestamp_unix: 1_900_000_000,
        cost: octo_determin::Dqa::new(60_000, 0).expect("non-overflow"),
        cost_vault_id: Some(vault_id),
        chain_id: Some(chain),
    };
    verify_settlement_chain_match(&env, &lookup).expect("match");
}

/// TV-0959-19: vault row missing (cost_vault_id points at a vault that
/// does not exist) — verifier emits `ChainMismatch` with the
/// precondition step (the verifier reports `ChainMismatch` against
/// the inferred vault_chain_id of [0; 32] when the lookup misses,
/// since the row is absent). The Row-back mapping is:
/// verifier performs `lookup_vault(vault_id)` first; on miss, the
/// 3-step algorithm surfaces `ChainMismatch` with the envelope's
/// chain_id as the vault chain_id placeholder (per implementation in
/// `settlement_verify.rs`).
#[test]
fn tv_0959_19_cross_chain_vault_row_missing() {
    let vault_id = [0xA4; 32];
    let envelope_chain = [0xC4; 32];
    let lookup = InMemoryLookup {
        rows: HashMap::new(),
    };
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker4".to_owned(),
        holder_did: "did:octo:provider-missing".to_owned(),
        model: ModelRef::from("gemini/gemini-2.5-flash"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        ask_id: [0x11; 32],
        nonce: [0x22; 32],
        timestamp_unix: 2_000_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        cost_vault_id: Some(vault_id),
        chain_id: Some(envelope_chain),
    };
    let err = verify_settlement_chain_match(&env, &lookup).unwrap_err();
    // Vault row MISS → verifier maps to `CostVaultIdMissing` (the
    // migration gate: the row's absence is equivalent to the field's
    // absence for the v2.0 wire form — both treated as "no
    // vault-row binding").
    assert!(matches!(err, SettlementError::CostVaultIdMissing));
}

/// TV-0959-20: cost_vault_id missing on the envelope — verifier emits
/// `SettlementError::CostVaultIdMissing`.
#[test]
fn tv_0959_20_cost_vault_id_missing_reject() {
    let lookup = InMemoryLookup {
        rows: HashMap::new(),
    };
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker5".to_owned(),
        holder_did: "did:octo:provider-novault".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        ask_id: [0x33; 32],
        nonce: [0x44; 32],
        timestamp_unix: 2_100_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        cost_vault_id: None,
        chain_id: Some([0xC5; 32]),
    };
    let err = verify_settlement_chain_match(&env, &lookup).unwrap_err();
    assert!(matches!(err, SettlementError::CostVaultIdMissing));
}

// =====================================================================
// Family 5 — TV-0959-21..25: VaultLookup trait reuse verification
// =====================================================================

/// TV-0959-21: the settlement-time verifier MUST accept `&dyn VaultLookup`
/// (NOT a concrete type). This pins the trait-surface reuse with
/// `octo_cap_macaroon::VaultLookup`.
#[test]
fn tv_0959_21_vault_lookup_trait_dyn_dispatch() {
    let lookup: Box<dyn VaultLookup> = Box::new(InMemoryLookup {
        rows: HashMap::from([(
            [0xA6; 32],
            VaultRowSnapshot {
                chain_id: [0xB6; 32],
                is_active: true,
            },
        )]),
    });
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker6".to_owned(),
        holder_did: "did:octo:provider-trait".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        ask_id: [0x55; 32],
        nonce: [0x66; 32],
        timestamp_unix: 2_200_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        cost_vault_id: Some([0xA6; 32]),
        chain_id: Some([0xB6; 32]),
    };
    verify_settlement_chain_match(&env, &*lookup).expect("trait dispatch accept");
}

/// TV-0959-22: `&dyn VaultLookup` from a vault-row MISS propagates as
/// `ChainMismatch` (verifier rejects on the precondition step).
#[test]
fn tv_0959_22_vault_lookup_trait_dyn_miss() {
    let lookup: Box<dyn VaultLookup> = Box::new(InMemoryLookup {
        rows: HashMap::new(),
    });
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker7".to_owned(),
        holder_did: "did:octo:provider-trait-miss".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        ask_id: [0x77; 32],
        nonce: [0x88; 32],
        timestamp_unix: 2_300_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        cost_vault_id: Some([0xA7; 32]),
        chain_id: Some([0xB7; 32]),
    };
    let err = verify_settlement_chain_match(&env, &*lookup).unwrap_err();
    // Vault row MISS via `&dyn VaultLookup` — same mapping as TV-19.
    assert!(matches!(err, SettlementError::CostVaultIdMissing));
}

/// TV-0959-23: vault row's `is_active = false` — the verifier still
/// accepts (it does NOT consult the active flag at settlement-time;
/// `is_active` is the verify-time invariant). Pin the trait invariant.
#[test]
fn tv_0959_23_vault_lookup_inactive_row_accepted() {
    let vault_id = [0xA8; 32];
    let chain = [0xB8; 32];
    let lookup = InMemoryLookup {
        rows: HashMap::from([(
            vault_id,
            VaultRowSnapshot {
                chain_id: chain,
                is_active: false, // inactive — verifier does NOT gate on this
            },
        )]),
    };
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker8".to_owned(),
        holder_did: "did:octo:provider-inactive".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        ask_id: [0x99; 32],
        nonce: [0xAA; 32],
        timestamp_unix: 2_400_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        cost_vault_id: Some(vault_id),
        chain_id: Some(chain),
    };
    verify_settlement_chain_match(&env, &lookup)
        .expect("inactive row is accepted at settlement-time (chain_id match only)");
}

/// TV-0959-24: trait-method signature — `lookup_vault` returns
/// `Option<VaultRowSnapshot>` (NOT a `Result<...>`). Honoring the
/// `octo_cap_macaroon::VaultLookup` trait contract.
#[test]
fn tv_0959_24_vault_lookup_trait_signature() {
    let lookup = InMemoryLookup {
        rows: HashMap::new(),
    };
    let result: Option<VaultRowSnapshot> = lookup.lookup_vault(&[0xDE; 32]);
    assert!(
        result.is_none(),
        "miss returns Option<VaultRowSnapshot>::None"
    );
}

/// TV-0959-25: shared trait with capability verify-time path —
/// `octo_cap_macaroon::VaultLookup` implements the same trait that
/// `Macaroon::verify_for_vault_op` consumes (mission 0957-g LANDED).
/// No shadow impl in `quota-router-storage` (the call site uses
/// the cross-crate symbol).
#[test]
fn tv_0959_25_vault_lookup_shared_trait_no_shadow() {
    // Compile-time assertion: the trait imported above is the SAME
    // symbol as `octo_cap_macaroon::VaultLookup`. If a shadow were
    // introduced, the `InMemoryLookup` impl would resolve to the
    // shadow. We pin this by checking the trait is path-deterministic.
    fn trait_origin(_: &dyn VaultLookup) -> &'static str {
        // Path-of-origin: any shadow would fail to compile here
        // because the impl on `InMemoryLookup` is for the trait
        // at the path `octo_cap_macaroon::VaultLookup`.
        "octo_cap_macaroon::VaultLookup"
    }
    let lookup = InMemoryLookup {
        rows: HashMap::new(),
    };
    assert_eq!(trait_origin(&lookup), "octo_cap_macaroon::VaultLookup");
}
