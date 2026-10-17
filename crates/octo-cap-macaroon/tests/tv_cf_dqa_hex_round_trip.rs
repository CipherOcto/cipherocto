//! Regression vectors for the caveat-form amendment.
//! Pinned by `tv_cf_01_dqa_hex_round_trip` and `tv_cf_02_dqa_hex_rejects_odd_length`.

use octo_cap_macaroon::dqa_serde::{dqa_from_bytes, dqa_to_bytes, field};
use octo_determin::Dqa;
use serde::{Deserialize, Serialize};

/// Wrapper to exercise the `field::deserialize` adapter end-to-end
/// through `serde_json::from_value`. `Dqa` itself does not derive
/// `Deserialize` (it is RFC-frozen; the substrate owns serde via
/// `dqa_serde::field`), so the test routes through a wrapper.
#[derive(Debug, Serialize, Deserialize)]
struct DqaWrap(#[serde(with = "field")] Dqa);

#[test]
fn tv_cf_01_dqa_hex_round_trip() {
    let d = Dqa::new(1_000_000, 6).expect("scale in range");
    let bytes = dqa_to_bytes(&d);
    let hex_str = hex::encode(bytes);
    // The 64-hex form (what canonical_ser emits) decodes to the same Dqa
    // as the 16-byte form, both via the substrate's dqa_from_bytes.
    let from_hex: DqaWrap =
        serde_json::from_value(serde_json::Value::String(hex_str.clone())).expect("hex decodes");
    let from_bytes: DqaWrap = serde_json::from_value(serde_json::Value::Array(
        bytes
            .iter()
            .map(|b| serde_json::Value::Number((*b).into()))
            .collect(),
    ))
    .expect("bytes decode");
    assert_eq!(
        from_hex.0, from_bytes.0,
        "hex form must decode to the same Dqa as the byte-array form"
    );
    // Both forms route through dqa_from_bytes, which canonicalises
    // trailing-zero numerators through the wire form (1_000_000 @ 6 and
    // 1 @ 0 share an encoding). The round-trip equivalence is therefore
    // pinned on the substrate's canonical-form bytes, not the input Dqa.
    let back = dqa_from_bytes(&bytes).expect("dqa");
    assert_eq!(from_hex.0, back, "hex form must match dqa_from_bytes");
    assert_eq!(
        from_bytes.0, back,
        "byte-array form must match dqa_from_bytes"
    );
}

#[test]
fn tv_cf_02_dqa_hex_rejects_odd_length() {
    let e: Result<DqaWrap, _> = serde_json::from_str("\"abc\"");
    assert!(e.is_err(), "odd-length hex must reject");
}

#[test]
fn tv_cf_03_dqa_hex_rejects_non_hex_chars() {
    let e: Result<DqaWrap, _> = serde_json::from_str(&format!("\"{}\"", "zz".repeat(32)));
    assert!(e.is_err(), "non-hex characters must reject");
}
