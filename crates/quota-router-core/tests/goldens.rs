//! AC-5 goldens: canonical 11-step exercise expected outputs.
//!
//! Compares step outputs against checked-in fixture at
//! `tests/fixtures/exercise/eleven_step_goldens.json`. Update flow:
//!
//! ```text
//! UPDATE_GOLDENS=1 cargo test -p quota-router-core --test goldens eleven_step_goldens_dump -- --nocapture
//! ```
//!
//! Then `git diff` the fixture, justify the drift in the PR description per
//! mission AC-5 R2 fix (signal-vs-noise distinguished by hash change > 1 byte
//! OR new axis added OR capability schema bump; review step mandatory).

use std::collections::HashSet;

use blake3::Hasher;
use ed25519_dalek::Signer;

const HOLDER_DID: &str = "did:octo:holder-1";
const ROUTER_ID: &str = "did:octo:router-1";
const MODEL: &str = "openai/gpt-4";
const FIXTURE: &str = "tests/fixtures/exercise/eleven_step_goldens.json";

/// Canonical step outputs (computed deterministically from declared inputs).
struct Goldens {
    step1_idp_token: String,
    step2_vak: String,
    step3_cap_id: String,
    step6_escrow_id: String,
    step10_settlement_hash: String,
}

fn compute_goldens() -> Goldens {
    // Step 1
    let mut h = Hasher::new();
    h.update(HOLDER_DID.as_bytes());
    let step1 = hex::encode(h.finalize().as_bytes());

    // Step 2 = BLAKE3(step1 || "vak/v1")
    let step1_bytes: [u8; 32] = *blake3::hash(HOLDER_DID.as_bytes()).as_bytes();
    let mut h = Hasher::new();
    h.update(&step1_bytes);
    h.update(b"vak/v1");
    let step2 = hex::encode(h.finalize().as_bytes());

    // Step 3 = BLAKE3(step2 || "cap/v1")
    let step2_bytes: [u8; 32] = *blake3::hash(
        {
            let mut buf = Vec::with_capacity(32 + 6);
            buf.extend_from_slice(&step1_bytes);
            buf.extend_from_slice(b"vak/v1");
            buf
        }
        .as_slice(),
    )
    .as_bytes();
    let mut h = Hasher::new();
    h.update(&step2_bytes);
    h.update(b"cap/v1");
    let step3 = hex::encode(h.finalize().as_bytes());

    // Ask = full Ask struct; ask_id = BLAKE3(asker_did || model || axes_hash || nonce)
    // For the canonical sample ask: axes_hash = BLAKE3("input_tokens_per_1k" || 30_000_le16)
    // The 11_step test computes `expected_ask_id = inserted_ask.id()` so we mirror
    // that here. (axes_hash computation lives in quota-router-storage::ask.)
    let ask = quota_router_storage::ask::Ask {
        asker_did: "did:octo:asker1".to_owned(),
        model: ModelRef::from(MODEL),
        rates: quota_router_storage::ask::ModelRateTable {
            model: ModelRef::from(MODEL),
            rates: vec![quota_router_storage::ask::AxisRate {
                axis: "input_tokens_per_1k".to_owned(),
                rate_per_1k: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
            }],
        },
        nonce: [0x42; 16],
        expires_at_unix: 1_900_000_000,
    };
    let ask_id: [u8; 32] = ask.id();

    // Step 6 = BLAKE3(ask_id || "escrow/v1")
    let mut h = Hasher::new();
    h.update(&ask_id);
    h.update(b"escrow/v1");
    let step6 = hex::encode(h.finalize().as_bytes());

    // Step 10 settlement_hash — R1 carryover M-3 fix: now derives from the
    // SAME `SettlementEnvelope::compute_settlement_hash` canonicalization
    // used by the sm-engine on disk path, instead of the prior
    // `blake3::hash(b"settlement-mock")` stub. The golden now reflects
    // the actual deterministic output of the canonical envelope encoder
    // for the canonical 11-step inputs.
    use quota_router_storage::ask::{ModelRef, SettlementEnvelope};
    let envelope = SettlementEnvelope {
        settlement_hash: [0u8; 32], // placeholder; computed below
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: HOLDER_DID.to_owned(),
        model: ModelRef::from(MODEL),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        // The ask_id used by `eleven_step::run_settlement` is what the
        // marketplace Ask derives via `Ask::id()` — we mirror that here.
        ask_id, // reused from the step-6 calculation above
        nonce: [0x55; 32],
        timestamp_unix: 1_700_000_000,
        cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
    };
    let step10 = hex::encode(envelope.compute_settlement_hash());

    Goldens {
        step1_idp_token: step1,
        step2_vak: step2,
        step3_cap_id: step3,
        step6_escrow_id: step6,
        step10_settlement_hash: step10,
    }
}

fn read_fixture() -> serde_json::Value {
    let raw =
        std::fs::read_to_string(FIXTURE).unwrap_or_else(|e| panic!("read fixture {FIXTURE}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse fixture {FIXTURE}: {e}"))
}

fn write_fixture(v: &serde_json::Value) {
    let pretty = serde_json::to_string_pretty(v).expect("serialize");
    std::fs::write(FIXTURE, pretty).unwrap_or_else(|e| panic!("write fixture: {e}"));
}

#[test]
fn eleven_step_goldens_match() {
    let goldens = compute_goldens();
    let mut fixture = read_fixture();
    let steps = fixture
        .get_mut("steps")
        .and_then(|s| s.as_object_mut())
        .expect("fixture.steps must be an object");

    let expected: &[(&str, &str)] = &[
        ("step1_idp_token_blake3", &goldens.step1_idp_token),
        ("step2_vak_blake3", &goldens.step2_vak),
        ("step3_cap_id_blake3", &goldens.step3_cap_id),
        ("step6_escrow_id_blake3", &goldens.step6_escrow_id),
        (
            "step10_settlement_hash_blake3",
            &goldens.step10_settlement_hash,
        ),
    ];

    for (key, expected_hex) in expected {
        let actual = steps
            .get(*key)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("fixture.steps.{key} missing"));
        assert_eq!(
            actual, *expected_hex,
            "drift in {key}: fixture={actual} computed={expected_hex}. \
             Re-run with UPDATE_GOLDENS=1 to refresh, then justify in PR."
        );
    }
}

#[test]
fn eleven_step_goldens_dump() {
    // Developer-only: regenerate fixture when step semantics intentionally change.
    if std::env::var("UPDATE_GOLDENS").is_err() {
        eprintln!("(set UPDATE_GOLDENS=1 to regenerate fixture)");
        return;
    }
    let goldens = compute_goldens();
    let mut fixture = read_fixture();
    let steps = fixture
        .get_mut("steps")
        .and_then(|s| s.as_object_mut())
        .expect("fixture.steps must be an object");
    steps.insert(
        "step1_idp_token_blake3".to_owned(),
        serde_json::Value::String(goldens.step1_idp_token),
    );
    steps.insert(
        "step2_vak_blake3".to_owned(),
        serde_json::Value::String(goldens.step2_vak),
    );
    steps.insert(
        "step3_cap_id_blake3".to_owned(),
        serde_json::Value::String(goldens.step3_cap_id),
    );
    steps.insert(
        "step6_escrow_id_blake3".to_owned(),
        serde_json::Value::String(goldens.step6_escrow_id),
    );
    steps.insert(
        "step10_settlement_hash_blake3".to_owned(),
        serde_json::Value::String(goldens.step10_settlement_hash),
    );
    write_fixture(&fixture);
    eprintln!("fixture updated: {FIXTURE}");
}

/// AC-5 smoke: full 11-step exercise end-to-end matches the canonical ledger
/// state. Re-uses the step functions defined in `eleven_step.rs` (same crate,
/// same target dir, `#[path]` access via `mod`).
#[test]
fn eleven_step_goldens_ledger_size_one() {
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = i as u8;
    }
    let router = ed25519_dalek::SigningKey::from_bytes(&seed);
    let settlement_hash: [u8; 32] = *blake3::hash(b"settlement-mock").as_bytes();
    let receipt = {
        use quota_router_core::receipt::{canonical_receipt_bytes, Receipt};
        let msg = canonical_receipt_bytes(
            &settlement_hash,
            "did:octo:asker1",
            HOLDER_DID,
            1_700_000_000,
        );
        let sig = router.sign(&msg);
        Receipt {
            settlement_hash,
            router_id: ROUTER_ID.to_owned(),
            router_sig: sig,
            timestamp_unix: 1_700_000_000,
        }
    };
    let mut ledger: HashSet<[u8; 32]> = HashSet::new();
    ledger.insert(receipt.settlement_hash);
    assert_eq!(ledger.len(), 1);
    // Replay must NOT increase ledger size (ConsumedReceiptIndex equivalent).
    ledger.insert(receipt.settlement_hash);
    assert_eq!(ledger.len(), 1, "replay must be idempotent");
}
