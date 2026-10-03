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

/// PINS THE REMAINING ASYMMETRY. `Caveat::Payment`'s canonical form is a
/// deliberate partial projection, not a lossless serialization.
///
/// `canonical_ser` emits only `caveat_name`, `budget`, `model`, and
/// `expires_at_unix_ms`. It drops `asset_id`, `registry_snapshot_epoch`, and
/// `nonce`, so feeding the canonical form back into the `Caveat` enum's
/// `Deserialize` fails with a missing-field error. The `AmountMax` and
/// `Vault` arms are single-payload, so for them the canonical form IS
/// lossless and does round-trip (`tv_cf_07`, `tv_cf_08`).
///
/// This is asserted, not tolerated silently. A future change that makes the
/// `Payment` canonical form lossless would change `caveat_body_hash`, and
/// therefore every payment capability's id — so it is a decision with a
/// stated cost, not a cleanup. Anyone reading a failure here should treat it
/// as "the projection changed, re-examine the capability-id contract", not
/// as a broken test.
///
/// The three dropped fields are not incidental:
/// `asset_id` is the asset binding that stops a USDC budget being spent
/// against an OCTO-W query, `registry_snapshot_epoch` is the staleness
/// guard, and `nonce` is the anti-replay token. Nothing in the CLI may treat
/// `canonical_ser` output as re-input for this arm.
#[test]
fn tv_cf_18_payment_canonical_form_is_a_partial_projection() {
    let caveat = Caveat::Payment(sample_payment());
    let canonical = caveat.canonical_ser();
    let parsed: Result<Caveat, _> = serde_json::from_slice(&canonical);

    assert!(
        parsed.is_err(),
        "canonical_ser(Payment) reparsed successfully. If this now passes, the \
         projection became lossless, which changes caveat_body_hash and every \
         payment capability id — update RFC-0011 §Caveat Form Amendment clause 1 \
         and the capability-id contract deliberately, do not just relax this pin. \
         canonical was: {}",
        String::from_utf8_lossy(&canonical)
    );

    // Name the dropped fields so the deviation is legible in the failure
    // output of a related test, not only here.
    let text = String::from_utf8_lossy(&canonical);
    for dropped in ["asset_id", "registry_snapshot_epoch", "nonce"] {
        assert!(
            !text.contains(dropped),
            "canonical_ser(Payment) now emits {dropped}, so the projection is \
             no longer the one this vector pins"
        );
    }

    // The fields it DOES carry must be the canonical hex budget, which is
    // the reason the projection exists: PaymentCaveat::attenuate Gate 3
    // rejects a budget whose scale differs from the parent's, so the scale
    // has to survive into the hashed form.
    let value: serde_json::Value =
        serde_json::from_slice(&canonical).expect("canonical is valid json");
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
