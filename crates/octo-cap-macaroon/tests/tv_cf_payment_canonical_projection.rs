//! Mission 0011-caveat-form-amendment — vectors for the `Payment` caveat arm
//! and for input-acceptance parity across the crate's 32-byte id fields.
//!
//! These two cover ground the original `tv_cf_01..17` set did not, and both
//! were found by an adversarial review of RFC-0011 §Caveat Form Amendment
//! rather than by an implementation gap:
//!
//! * `tv_cf_18` pins a **deviation** the amendment's normative clause 1
//!   asserts does not exist. The clause says the canonical form emitted for
//!   the `Payment.budget` arm must be accepted by the same enum's
//!   `Deserialize` impl. It is not: `canonical_ser` projects four of
//!   `PaymentCaveat`'s seven fields, so reparsing fails outright.
//! * `tv_cf_19` pins that every 32-byte id-bearing field in the crate accepts
//!   the same two input forms. `serde_bytes_arr32` (used by
//!   `PaymentCaveat::asset_id` and `::nonce`) used to accept hex only while
//!   `Caveat::Vault` accepted hex or array, so two 32-byte id fields in one
//!   crate disagreed on the same canonical form.

use octo_cap_macaroon::caveat::payment::PaymentCaveat;
use octo_cap_macaroon::{AssetId, Caveat, Dqa, Epoch, Nonce};

/// Position-distinctive bytes. A single repeated byte is a fixed point of
/// every byte permutation, so a constant array cannot detect a reordering
/// defect — the same trap `tv_cf_14` was written to close.
fn ramp() -> [u8; 32] {
    core::array::from_fn(|i| (i as u8).wrapping_mul(7).wrapping_add(3))
}

fn sample_payment() -> PaymentCaveat {
    PaymentCaveat {
        caveat_name: "paid-query/v1".to_string(),
        asset_id: AssetId::from_bytes(ramp()),
        budget: Dqa::new(1_000_000, 6).expect("dqa"),
        model: "gpt-4".to_string(),
        expires_at_unix_ms: u64::MAX,
        registry_snapshot_epoch: Epoch::new(1),
        nonce: Nonce::from_bytes([0x22u8; 32]),
    }
}

/// PINS THAT THE `Payment` CANONICAL FORM IS LOSSLESS.
///
/// This vector used to assert the opposite — that the projection was partial
/// and that reparsing FAILED. It was written to "pin a deviation", and in doing
/// so it enforced a maintenance drift: the projection was written on 2026-08-10
/// when `caveat_name`, `budget`, `model`, and `expires_at_unix_ms` were all of
/// `PaymentCaveat`'s fields; `asset_id`, `registry_snapshot_epoch`, and `nonce`
/// were added on 2026-08-26 by `726960ea` without touching the match arm, and
/// this assertion is what stopped anyone noticing for sixteen days.
///
/// A vector that pins a defect is not neutral. It reads as a considered
/// decision, so a later reader trusts the drift rather than seeing it. The
/// three dropped fields were the three that constrain the capability — asset
/// binding, staleness guard, anti-replay nonce — and the four that survived
/// were the four inert descriptive ones.
///
/// `tv_cf_25` now guards COMPLETENESS mechanically, so the projection cannot
/// drift from the struct again. This vector pins the behaviour.
#[test]
fn tv_cf_18_payment_canonical_form_is_lossless() {
    let caveat = Caveat::Payment(sample_payment());
    let canonical = caveat.canonical_ser();
    let text = String::from_utf8_lossy(&canonical);

    let parsed: Result<Caveat, _> = serde_json::from_slice(&canonical);
    assert!(
        parsed.is_ok(),
        "canonical_ser(Payment) no longer reparses, so the arm is asymmetric \
         again: {parsed:?}\ncanonical was: {text}"
    );
    let reparsed = parsed.expect("reparses");

    // The round-trip must be a FIXED POINT of the canonical form, not a
    // struct equality.
    //
    // A plain `assert_eq!(reparsed, caveat)` fails here, and it is right to
    // fail: `Dqa` has several representations of one value, and
    // `DqaEncoding::from_dqa` canonicalizes before encoding ("CRITICAL:
    // Canonicalizes before encoding to ensure deterministic Merkle hashes" in
    // the Layer A `determin` crate). So `Dqa { value: 1_000_000, scale: 6 }`
    // re-reads as `Dqa { value: 1, scale: 0 }` — the same economic value in the
    // canonical representation. Demanding representation equality would be
    // demanding a defect from a frozen consensus crate.
    //
    // The fixed-point form is the honest property and is strictly stronger than
    // a parse-success check: it still fails if a field is dropped, reordered
    // into a different value, or silently altered, because re-canonicalizing
    // the reparsed caveat must reproduce the identical bytes.
    assert_eq!(
        reparsed.canonical_ser(),
        canonical,
        "the Payment canonical form is not a fixed point: reparsing and \
         re-canonicalizing does not reproduce the same bytes, so the \
         projection loses or alters something.\nfirst:  {text}\nsecond: {}",
        String::from_utf8_lossy(&reparsed.canonical_ser())
    );

    // Every field the projection used to drop must be present. Named
    // explicitly, not merely counted, so a failure says which one is missing.
    let value: serde_json::Value = serde_json::from_str(&text).expect("canonical is valid json");
    for field in [
        "asset_id",
        "registry_snapshot_epoch",
        "nonce",
        "caveat_name",
        "budget",
        "model",
        "expires_at_unix_ms",
    ] {
        assert!(
            !value["value"][field].is_null(),
            "canonical_ser(Payment) does not carry {field}, so the projection \
             is partial again. Every field of PaymentCaveat must be in the \
             canonical form: it feeds caveat_body_hash, which backs both the \
             capability id and the dry-run pastejacking echo. canonical: {text}"
        );
    }

    // The budget must still be the canonical 16-byte hex carrying `scale`.
    // `PaymentCaveat::attenuate` Gate 3 rejects a budget whose scale differs
    // from the parent's, so dropping the scale would discard exactly the
    // invariant the attenuator defends. Pinned by
    // `canonical_payment_budget_carries_scale` on the substrate side too.
    let budget = value["value"]["budget"]
        .as_str()
        .expect("budget is the canonical hex string");
    assert_eq!(budget.len(), 32, "budget hex is 16 bytes");
    assert!(
        budget
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "budget must be lowercase hex, got {budget}"
    );
}

/// MECHANICAL completeness guard: every field `PaymentCaveat` declares must
/// appear in the canonical projection.
///
/// The defect this closes was invisible because a hand-written field list does
/// not break when a struct grows a field, and the only test touching it
/// asserted the drift was intentional. Deriving the field set from the struct's
/// own source means adding a field to `PaymentCaveat` without adding it to
/// `canonical_ser` fails here.
///
/// The list is read from the source rather than reflected over, so a field
/// marked `#[serde(skip)]` is correctly excluded.
#[test]
fn tv_cf_25_payment_canonical_projection_covers_every_declared_field() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join("src/caveat/payment.rs");
    let source = std::fs::read_to_string(&path).expect("payment.rs readable from the test");

    // Bound the struct body by brace counting, so a later struct in the file
    // cannot contribute fields.
    let start = source
        .find("pub struct PaymentCaveat {")
        .expect("PaymentCaveat must exist");
    let after_open = start + "pub struct PaymentCaveat {".len();
    let mut depth = 1usize;
    let mut end = after_open;
    for (offset, ch) in source[after_open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = after_open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &source[after_open..end];
    assert!(
        body.contains("pub asset_id") && body.contains("pub nonce"),
        "the PaymentCaveat body could not be bounded, so this guard would be \
         reading a different struct and reporting agreement"
    );

    let mut declared: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("pub ") {
            continue;
        }
        // A skipped field is not part of the wire form, so requiring it in the
        // projection would be wrong.
        if trimmed.contains("serde(skip") {
            continue;
        }
        let after_pub = trimmed["pub ".len()..].trim_start();
        let name: String = after_pub
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        // Must be a FIELD (`name:`), not a method (`fn name(`).
        if after_pub[name.len()..].trim_start().starts_with(':') {
            declared.push(name);
        }
    }

    assert!(
        declared.len() >= 7,
        "expected at least the seven known fields, found {declared:?}. A field \
         rename or removal should be a deliberate edit here, not a silent drop."
    );

    let value: serde_json::Value =
        serde_json::from_slice(&Caveat::Payment(sample_payment()).canonical_ser())
            .expect("canonical is valid json");
    let projected = value["value"].as_object().expect("value is an object");

    let mut missing: Vec<&String> = declared
        .iter()
        .filter(|f| !projected.contains_key(f.as_str()))
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "PaymentCaveat declares {declared:?} but canonical_ser omits {missing:?}. \
         Every declared field must be projected: the canonical form feeds \
         caveat_body_hash, which backs the capability id and the dry-run \
         pastejacking echo. A field added to the struct without being added here \
         is invisible to every other test, which is exactly how asset_id, \
         registry_snapshot_epoch, and nonce stayed out for sixteen days."
    );
}

/// Every 32-byte id-bearing field in this crate accepts the same two input
/// forms: the 64-hex canonical form and the 32-element legacy array.
///
/// This is the parity property that RFC-0011 §Caveat Form Amendment clause 2
/// asserts ("the substrate-owned serialization path for any 32-byte
/// id-bearing field"). It was false before the review: `Caveat::Vault` went
/// through `hex_id_32`, which accepts both forms, while
/// `PaymentCaveat::asset_id` and `::nonce` went through a parallel
/// `serde_bytes_arr32` that accepted hex only. Same crate, same canonical
/// form, opposite input acceptance.
///
/// The three fields are checked through their owning envelopes rather than
/// through the bare newtype, because that is the only place the divergence
/// was visible: a bare `AssetId` serialises as a 32-element array under
/// default features, while the same type as a `PaymentCaveat` field
/// serialises as hex.
#[test]
fn tv_cf_19_all_32byte_id_fields_accept_both_forms() {
    let bytes = ramp();
    let hex = hex::encode(bytes);
    let array = serde_json::json!(bytes.to_vec());

    // 1. Caveat::Vault — the reference behaviour.
    for (label, value) in [("hex", serde_json::json!(hex)), ("array", array.clone())] {
        let envelope = serde_json::json!({"type": "vault", "value": value});
        let parsed: Result<Caveat, _> = serde_json::from_value(envelope);
        assert!(
            parsed.is_ok(),
            "Caveat::Vault rejected the {label} form: {parsed:?}"
        );
        if let Ok(Caveat::Vault(id)) = parsed {
            assert_eq!(id, bytes, "Caveat::Vault {label} form changed the bytes");
        }
    }

    // 2. PaymentCaveat::asset_id and ::nonce.
    let base = sample_payment();
    for (label, value) in [("hex", serde_json::json!(hex)), ("array", array)] {
        let mut object = serde_json::to_value(&base).expect("serialise base");
        object["asset_id"] = value.clone();
        object["nonce"] = value;
        let parsed: Result<PaymentCaveat, _> = serde_json::from_value(object);
        assert!(
            parsed.is_ok(),
            "PaymentCaveat rejected the {label} form for a 32-byte id field: {parsed:?}. \
             All 32-byte id-bearing fields must accept both forms."
        );
        if let Ok(got) = parsed {
            // Both fields were overwritten with the same ramp value, so both
            // must come back as that value, byte for byte.
            assert_eq!(got.asset_id.as_bytes(), &bytes, "asset_id {label} form");
            assert_eq!(got.nonce.as_bytes(), &bytes, "nonce {label} form");
        }
    }
}
