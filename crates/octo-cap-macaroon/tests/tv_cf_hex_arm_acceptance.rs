//! Mission 0011-caveat-form-amendment — enumeration of the `Caveat` arms whose
//! canonical form emits 64-char hex, and whether the enum's `Deserialize`
//! accepts that same hex in the position its input form uses.
//!
//! Found by adversarial review round 2 of RFC-0011 §Caveat Form Amendment.
//!
//! ## Why this file exists
//!
//! The amendment's premise is that the emitter produces hex while the input
//! form accepted only a byte array, and that closing that gap is the point.
//! It closed the gap for `AmountMax` (16-byte payload) and `Vault` (32-byte
//! payload). Four further arms emit hex from `canonical_ser` and still reject
//! it on input: `wrapped_only`, `redemption_context`, `invocation_hash_bind`,
//! and `ask_binding`. The drift audit the amendment cites as its context
//! already named all six hex-rendering arms, so the set was enumerated in
//! prose somewhere and nowhere in code.
//!
//! ## A first attempt at this vector was itself blind
//!
//! The obvious way to ask "does this arm accept its own canonical hex" is to
//! feed `canonical_ser` output back through `Deserialize`. That measures
//! nothing useful here. `Caveat` is adjacently tagged
//! (`tag = "type"`, `content = "value"`), so a struct-variant arm expects an
//! OBJECT under `value`, while `canonical_ser` emits a bare hex STRING. That
//! input can never parse, for reasons that have nothing to do with hex versus
//! array, so the check reads `false` no matter what the adapter does.
//!
//! Adopting `hex_id_32` on `wrapped_only` was tried against that version and
//! the vector stayed green. A check that cannot fail for the claim it names
//! is worse than no check, because it reads as evidence. So the property
//! measured here is narrower and honest: given the arm's CORRECT input
//! envelope, does the canonical hex value parse in the payload position?
//!
//! ## Completeness boundary
//!
//! This enumerates the hex-emitting set, not every `Caveat` arm. A NEW arm
//! that emits hex would not be caught here and must be added to
//! `HEX_EMITTING_ARMS` when it lands. Both directions that are covered are
//! covered: an arm that stops emitting hex fails, and an arm that changes
//! whether it accepts hex fails.
//!
//! If a vector here fails, that is not automatically a bug in the code. An arm
//! changing sides means RFC-0011 §Caveat Form Amendment §Known deviations must
//! be updated in the same commit. Fixing an arm is deliberately awkward:
//! adopting the adapter widens INPUT acceptance to both forms for free, but
//! also changes the EMITTED form from a 32-element array to a hex string,
//! which breaks every consumer that reads the array form.

use octo_cap_macaroon::Caveat;
use serde_json::{json, Value};

/// The hex-emitting set: arm name, and whether the input form accepts the
/// canonical hex value.
///
/// Written as a literal rather than derived by running the code, so the test
/// cannot quietly agree with whatever the code does.
const HEX_EMITTING_ARMS: [(&str, bool); 5] = [
    // arm name, accepts hex on input
    ("vault", true),
    ("wrapped_only", false),
    ("redemption_context", false),
    ("invocation_hash_bind", false),
    ("ask_binding", false),
];

/// One arm under test: the caveat itself, plus the input envelope built the
/// way each arm's `Deserialize` actually expects it.
///
/// Newtype-variant arms carry the payload directly under `value`.
/// Struct-variant arms carry an object of named fields under `value`, and the
/// field name is the Rust identifier because the enum renames variants but
/// not fields.
struct ArmCase {
    name: &'static str,
    caveat: Caveat,
    /// Correct-shape envelope whose payload is the 64-char hex value.
    input_hex: Value,
    /// Correct-shape envelope whose payload is the 32-element array.
    input_array: Value,
}

fn arms() -> Vec<ArmCase> {
    let h = |b: u8| hex::encode([b; 32]);
    let a = |b: u8| json!(vec![b; 32]);

    vec![
        ArmCase {
            name: "vault",
            caveat: Caveat::Vault([0x11u8; 32]),
            input_hex: json!({"type": "vault", "value": h(0x11)}),
            input_array: json!({"type": "vault", "value": a(0x11)}),
        },
        ArmCase {
            name: "wrapped_only",
            caveat: Caveat::WrappedOnly {
                parent_capability: [0x22u8; 32],
            },
            input_hex: json!({"type": "wrapped_only", "value": {"parent_capability": h(0x22)}}),
            input_array: json!({"type": "wrapped_only", "value": {"parent_capability": a(0x22)}}),
        },
        ArmCase {
            name: "redemption_context",
            caveat: Caveat::RedemptionContext {
                context_hash: [0x33u8; 32],
            },
            input_hex: json!({"type": "redemption_context", "value": {"context_hash": h(0x33)}}),
            input_array: json!({"type": "redemption_context", "value": {"context_hash": a(0x33)}}),
        },
        ArmCase {
            name: "invocation_hash_bind",
            caveat: Caveat::InvocationHashBind([0x44u8; 32]),
            input_hex: json!({"type": "invocation_hash_bind", "value": h(0x44)}),
            input_array: json!({"type": "invocation_hash_bind", "value": a(0x44)}),
        },
        ArmCase {
            name: "ask_binding",
            caveat: Caveat::AskBinding([0x55u8; 32]),
            input_hex: json!({"type": "ask_binding", "value": h(0x55)}),
            input_array: json!({"type": "ask_binding", "value": a(0x55)}),
        },
    ]
}

/// True when the arm's canonical form really does emit a 64-char hex value.
fn emits_hex(caveat: &Caveat) -> bool {
    let canonical = caveat.canonical_ser();
    let value: Value = serde_json::from_slice(&canonical).expect("canonical is valid json");
    value["value"]
        .as_str()
        .map(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or(false)
}

#[test]
fn tv_cf_20_hex_emitting_arms_acceptance_set_is_explicit() {
    let cases = arms();
    assert_eq!(
        cases.len(),
        HEX_EMITTING_ARMS.len(),
        "the expectation list and the arm list have drifted apart in length"
    );

    let mut problems: Vec<String> = Vec::new();

    for (case, (expected_name, expected_accepts)) in cases.iter().zip(HEX_EMITTING_ARMS.iter()) {
        assert_eq!(
            case.name, *expected_name,
            "arm order drifted between the expectation list and the arm list"
        );

        // These are all supposed to be hex emitters. If one stops emitting
        // hex it is no longer this table's subject and the list needs pruning.
        if !emits_hex(&case.caveat) {
            problems.push(format!(
                "{}: expected a 64-char hex canonical value, found something else",
                case.name
            ));
            continue;
        }

        let accepts = serde_json::from_value::<Caveat>(case.input_hex.clone()).is_ok();
        if accepts != *expected_accepts {
            problems.push(format!(
                "{}: expected accepts_hex={}, observed {} for {}",
                case.name, expected_accepts, accepts, case.input_hex
            ));
        }

        // Every arm in this set must at least accept the legacy array form,
        // since that is the form the CLI has always accepted and the amendment
        // promises not to break existing consumers.
        let array_ok = serde_json::from_value::<Caveat>(case.input_array.clone()).is_ok();
        if !array_ok {
            problems.push(format!(
                "{}: rejected the legacy 32-element array form {}, which would \
                 break a consumer the amendment promises to leave working",
                case.name, case.input_array
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "the hex-emitting arm acceptance set changed:\n  {}\n\n\
         Update HEX_EMITTING_ARMS and RFC-0011 §Caveat Form Amendment \
         §Known deviations together, and state the wire-format cost of \
         adopting the adapter for any arm that changed sides.",
        problems.join("\n  ")
    );
}

/// The four arms that still reject their own canonical hex must keep doing so
/// for a STATED reason, and the reason lives next to the pin.
///
/// Separate from the enumeration above on purpose: that one says which arms
/// are asymmetric, this one says the asymmetry is deliberate and unresolved,
/// so a future reader who trips the pin finds the explanation rather than
/// assuming the pin is stale.
#[test]
fn tv_cf_21_known_asymmetric_arms_still_reject_their_own_canonical_hex() {
    for case in arms().iter().filter(|c| c.name != "vault") {
        let parsed: Result<Caveat, _> = serde_json::from_value(case.input_hex.clone());
        assert!(
            parsed.is_err(),
            "{} now accepts the hex value it emits. That is progress, not a \
             broken test. Landing it means updating RFC-0011 §Caveat Form \
             Amendment §Known deviations in the same commit, and weighing the \
             emitted-form change: the input widens to both forms for free, but \
             the output flips from a 32-element array to a hex string, which \
             breaks every consumer reading the array form.",
            case.name
        );

        // The array form must still work, or the "progress" above would have
        // been a regression rather than a fix.
        let array_ok = serde_json::from_value::<Caveat>(case.input_array.clone()).is_ok();
        assert!(
            array_ok,
            "{} lost the legacy array form while gaining hex; that is a \
             breaking change, not a widening",
            case.name
        );
    }
}
