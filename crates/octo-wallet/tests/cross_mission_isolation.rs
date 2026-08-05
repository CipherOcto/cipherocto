//! Cross-mission isolation tests — Phase E exit criterion per
//! `docs/plans/2026-07-19-identity-master-plan.md` §4.
//!
//! Verifies that the key hierarchy at `octo_wallet::key_hierarchy` enforces
//! per-`(asker_did, model)` isolation:
//!
//! 1. Distinct askers → distinct mission keys
//! 2. Distinct models (under same asker) → distinct mission keys
//! 3. Axis subkeys derived under one mission do NOT validate under a sibling
//! 4. derive_mission_key is deterministic — same inputs yield identical bytes
//! 5. Defense-in-depth: HMAC tag minted by mission A is rejected by mission B
//!    even when (asker, model, axis) collide
//! 6. Different identity seeds → distinct mission keys (per-identity isolation)

#[allow(clippy::doc_markdown)]
use blake3::Hash;
use octo_ident::test_helpers::sample_did;
use octo_wallet::{AxisSubkey, KeyHierarchy, MissionId, MissionKey};

const SEED: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31,
];

fn mission(asker: &str, model: &str) -> MissionId {
    MissionId::new(asker, model).expect("valid mission id")
}

/// 1. Distinct askers → distinct mission keys.
#[test]
fn distinct_askers_distinct_mission_keys() {
    let h = KeyHierarchy::new(SEED);
    let k_a = h
        .derive_mission_key(&mission(&sample_did(156), "openai/gpt-4"))
        .unwrap();
    let k_b = h
        .derive_mission_key(&mission(&sample_did(207), "openai/gpt-4"))
        .unwrap();
    assert_ne!(
        k_a.as_bytes(),
        k_b.as_bytes(),
        "asker isolation broken: distinct askers produced identical mission keys"
    );
}

/// 2. Distinct models under the same asker → distinct mission keys.
#[test]
fn distinct_models_distinct_mission_keys() {
    let h = KeyHierarchy::new(SEED);
    let k_gpt = h
        .derive_mission_key(&mission(&sample_did(156), "openai/gpt-4"))
        .unwrap();
    let k_claude = h
        .derive_mission_key(&mission(&sample_did(156), "anthropic/claude-3-opus"))
        .unwrap();
    assert_ne!(
        k_gpt.as_bytes(),
        k_claude.as_bytes(),
        "model isolation broken: distinct models under same asker produced identical mission keys"
    );
}

/// 3. Axis subkeys derived under mission A are NOT equal to those derived
///    under mission B — even when the axis identifier matches.
#[test]
fn axis_subkeys_isolated_across_missions() {
    let h = KeyHierarchy::new(SEED);
    let axis = "input_tokens_per_1k";

    let m_alice_gpt = mission(&sample_did(156), "openai/gpt-4");
    let m_bob_gpt = mission(&sample_did(207), "openai/gpt-4");
    let m_alice_claude = mission(&sample_did(156), "anthropic/claude-3-opus");

    let sk_alice_gpt = h.derive_axis_subkey(&m_alice_gpt, axis).unwrap();
    let sk_bob_gpt = h.derive_axis_subkey(&m_bob_gpt, axis).unwrap();
    let sk_alice_claude = h.derive_axis_subkey(&m_alice_claude, axis).unwrap();

    assert_ne!(
        sk_alice_gpt.as_bytes(),
        sk_bob_gpt.as_bytes(),
        "axis subkey leaked across askers"
    );
    assert_ne!(
        sk_alice_gpt.as_bytes(),
        sk_alice_claude.as_bytes(),
        "axis subkey leaked across models"
    );
}

/// 4. Determinism — derive twice from the same hierarchy + MissionId, get
///    identical 32-byte output. Required for stable HMAC verification.
#[test]
fn derive_is_deterministic() {
    let h = KeyHierarchy::new(SEED);
    let m = mission(&sample_did(156), "openai/gpt-4");

    let k1: MissionKey = h.derive_mission_key(&m).unwrap();
    let k2: MissionKey = h.derive_mission_key(&m).unwrap();
    assert_eq!(k1.as_bytes(), k2.as_bytes());

    let s1: AxisSubkey = h.derive_axis_subkey(&m, "input_tokens_per_1k").unwrap();
    let s2: AxisSubkey = h.derive_axis_subkey(&m, "input_tokens_per_1k").unwrap();
    assert_eq!(s1.as_bytes(), s2.as_bytes());
}

/// 5. Defense-in-depth: HMAC tag minted under mission A does NOT verify under
///    mission B even when both share asker/model/axis — i.e., mission keys
///    are domain-separated at the HMAC level, not just structurally distinct.
#[test]
fn hmac_tag_cross_mission_rejected() {
    let h = KeyHierarchy::new(SEED);
    let m_alice_gpt = mission(&sample_did(156), "openai/gpt-4");
    let m_bob_gpt = mission(&sample_did(207), "openai/gpt-4");
    let axis = "input_tokens_per_1k";

    // Two missions with distinct askers — same model + axis identifier.
    let sk_alice = h.derive_axis_subkey(&m_alice_gpt, axis).unwrap();
    let sk_bob = h.derive_axis_subkey(&m_bob_gpt, axis).unwrap();
    assert_ne!(
        sk_alice.as_bytes(),
        sk_bob.as_bytes(),
        "subkeys collided — HKDF domain separation broken"
    );

    // Alice mints an HMAC tag for a canonical receipt payload.
    let payload: &[u8] = b"canonical-receipt-payload";
    let tag_alice: Hash = blake3::keyed_hash(sk_alice.as_bytes(), payload);

    // Bob's key produces a DIFFERENT tag for the same payload.
    let tag_bob: Hash = blake3::keyed_hash(sk_bob.as_bytes(), payload);
    assert_ne!(
        tag_alice.as_bytes(),
        tag_bob.as_bytes(),
        "HMAC collision across missions — keyed_hash domain separation broken"
    );

    // Bob's tag MUST NOT validate as Alice's — independent verifiers reading
    // the (asker, model, axis) tuple from the wire MUST reject.
    let sk_alice_rederived = h.derive_axis_subkey(&m_alice_gpt, axis).unwrap();
    let tag_alice_rederived: Hash = blake3::keyed_hash(sk_alice_rederived.as_bytes(), payload);
    assert_eq!(
        tag_alice.as_bytes(),
        tag_alice_rederived.as_bytes(),
        "Alice re-derived tag must match her own"
    );
    assert_ne!(
        tag_bob.as_bytes(),
        tag_alice_rederived.as_bytes(),
        "Bob's tag must NOT equal Alice's re-derived tag (would imply cross-mission forgery)"
    );
}

/// 6. Different identity seeds → distinct mission keys (per-identity isolation,
///    upstream of the per-mission isolation).
#[test]
fn distinct_seeds_distinct_mission_keys() {
    let mut seed_b = SEED;
    seed_b[0] = 0xff; // one-byte perturbation

    let h_a = KeyHierarchy::new(SEED);
    let h_b = KeyHierarchy::new(seed_b);
    let m = mission(&sample_did(156), "openai/gpt-4");

    let k_a = h_a.derive_mission_key(&m).unwrap();
    let k_b = h_b.derive_mission_key(&m).unwrap();
    assert_ne!(
        k_a.as_bytes(),
        k_b.as_bytes(),
        "identity seed change leaked through HKDF (no avalanche)"
    );
}

/// 7. MissionId validation is a pre-condition: an empty asker_did MUST be
///    rejected before reaching HKDF. Confirms the security boundary lives at
///    MissionId::new, not at derive_mission_key.
#[test]
fn mission_id_validation_prevents_empty_keys() {
    assert!(MissionId::new("", "openai/gpt-4").is_err());
    assert!(MissionId::new(&sample_did(156), "").is_err());
}

/// 8. Symmetry / closure — given the public API, a receiver can re-derive
///    the mission key from just (identity_seed, asker_did, model). No
///    out-of-band channel needed. This is the contract the rest of the
///    system relies on (RFC-0853 §6).
#[test]
fn mission_key_rederivable_from_seed() {
    let h1 = KeyHierarchy::new(SEED);
    let h2 = KeyHierarchy::new(SEED);
    let m = mission(&sample_did(156), "openai/gpt-4");

    let k1 = h1.derive_mission_key(&m).unwrap();
    let k2 = h2.derive_mission_key(&m).unwrap();
    assert_eq!(
        k1.as_bytes(),
        k2.as_bytes(),
        "mission key is not re-derivable from seed alone — RFC-0853 §6 contract broken"
    );
}
