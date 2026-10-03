//! Mission 0011-caveat-form-amendment — the SET of `Caveat` arms that render
//! 32-byte ids through `hex::encode` in `Caveat::canonical_ser`, and which of
//! those accept their own canonical output back through `Deserialize`.
//!
//! Found by adversarial review of RFC-0011 §Caveat Form Amendment. Rounds 1-6
//! of the first review pass enumerated only five of these arms and missed four:
//! `asset_binding`, `factory`, `policy_reference`, and the distinction between
//! a bare-hex `value` and an object `value` with hex inside it. The enumeration
//! looked complete because it was compared against other documents rather than
//! against `canonical_ser` itself. `tv_cf_22` below closes that gap
//! mechanically.
//!
//! ## How the property is measured
//!
//! Each arm is measured twice, using the crate's OWN two encodings as the
//! baseline rather than a hand-built envelope:
//!
//! * **derived form** — `serde_json::to_value(&caveat)`, the form the enum's
//!   own `Serialize` emits. This always round-trips.
//! * **canonical form** — `canonical_ser()`, the form the digest and the
//!   operator-facing surface use. This is the hex-rendering one.
//!
//! An arm is *asymmetric* when its derived form round-trips but its canonical
//! form does not: the same caveat, two encodings from the same crate, one
//! accepted and one not.
//!
//! An earlier draft of this file asked whether an arm accepts its own canonical
//! hex by feeding `canonical_ser` output straight back in. That measures
//! nothing useful, because `Caveat` is adjacently tagged
//! (`tag = "type"`, `content = "value"`) and a struct-payload arm expects an
//! OBJECT under `value` where `canonical_ser` writes a bare string, so the
//! input cannot parse for reasons that have nothing to do with hex versus
//! array. Adopting `hex_id_32` on one arm was tried against that version and
//! the vector stayed green. Measuring against the derived form avoids the
//! hand-built-envelope problem entirely, and catches the shape disagreement as
//! a side effect rather than needing a separate check for it.
//!
//! If a vector here fails, that is not automatically a bug in the code. An arm
//! changing sides means RFC-0011 §Caveat Form Amendment §Known deviations must
//! be updated in the same commit. Fixing an arm widens its input acceptance to
//! both forms for free, but it also changes what `Serialize` emits for that
//! arm, which is a wire-format decision.

use octo_cap_macaroon::caveat::payment::PaymentCaveat;
use octo_cap_macaroon::{
    ActionTemplate, AssetBinding, AssetId, Caveat, Dqa, Epoch, FactoryVet, Nonce,
};

/// Every `Caveat` VARIANT whose `canonical_ser` arm renders bytes through
/// `hex::encode`.
///
/// Variant names rather than the snake_case tags `canonical_ser` writes under
/// `"type"`, because the source scan in `tv_cf_22` reads the match arms and
/// comparing like with like keeps the two from disagreeing over spelling. It
/// covers 32-byte ids, the 16-byte `Dqa` payload, and the variable-length
/// `Raw` blob, because the point of the comparison is that nothing renders hex
/// silently.
///
/// This literal is the comparison target for `tv_cf_22`, which derives the
/// same set from `canonical_ser` itself.
const HEX_RENDERING_ARMS: [&str; 11] = [
    "AmountMax",
    "InvocationHashBind",
    "AskBinding",
    "Raw",
    "Vault",
    "WrappedOnly",
    "Factory",
    "PolicyReference",
    "RedemptionContext",
    "AssetBinding",
    "Payment",
];

/// A constructible arm under test.
struct ArmCase {
    /// The `CaveatName` short tag `canonical_ser` writes under `"type"`.
    name: &'static str,
    caveat: Caveat,
    /// Whether the canonical form is accepted back by the enum's `Deserialize`.
    /// Only `vault` is, via the `hex_id_32` adapter.
    canonical_round_trips: bool,
}

/// The 32-byte id-bearing hex-rendering arms, plus `amount_max` as the
/// conformant control: it renders a 16-byte payload as hex and does round-trip,
/// because Phase 1 added the `visit_str` arm to `dqa_serde::field`.
fn arms() -> Vec<ArmCase> {
    vec![
        ArmCase {
            name: "amount_max",
            caveat: Caveat::AmountMax(Dqa::new(1_000_000, 6).expect("dqa")),
            canonical_round_trips: true,
        },
        ArmCase {
            name: "invocation_hash_bind",
            caveat: Caveat::InvocationHashBind([0x44u8; 32]),
            canonical_round_trips: false,
        },
        ArmCase {
            name: "ask_binding",
            caveat: Caveat::AskBinding([0x55u8; 32]),
            canonical_round_trips: false,
        },
        ArmCase {
            name: "vault",
            caveat: Caveat::Vault([0x11u8; 32]),
            canonical_round_trips: true,
        },
        ArmCase {
            name: "wrapped_only",
            caveat: Caveat::WrappedOnly {
                parent_capability: [0x22u8; 32],
            },
            canonical_round_trips: false,
        },
        ArmCase {
            name: "factory",
            caveat: Caveat::Factory(FactoryVet {
                target_vault_id: [0x77u8; 32],
                action_template: ActionTemplate {
                    selector: "probe".to_string(),
                    args: vec![],
                },
                required_caller: None,
                pre_conditions: vec![],
                expiry_for_deploy_unix: 0,
            }),
            canonical_round_trips: false,
        },
        ArmCase {
            name: "policy_reference",
            caveat: Caveat::PolicyReference {
                policy_id: [0x88u8; 32],
                policy_version_seq: 1,
                attenuation_witness: [0x99u8; 64],
            },
            canonical_round_trips: false,
        },
        ArmCase {
            name: "redemption_context",
            caveat: Caveat::RedemptionContext {
                context_hash: [0x33u8; 32],
            },
            canonical_round_trips: false,
        },
        ArmCase {
            name: "asset_binding",
            caveat: Caveat::AssetBinding(AssetBinding {
                asset_id: [0x66u8; 32],
            }),
            canonical_round_trips: false,
        },
        ArmCase {
            name: "payment",
            caveat: Caveat::Payment(PaymentCaveat {
                caveat_name: "paid-query/v1".to_string(),
                asset_id: AssetId::from_bytes([0x11u8; 32]),
                budget: Dqa::new(1_000_000, 6).expect("dqa"),
                model: "gpt-4".to_string(),
                expires_at_unix_ms: u64::MAX,
                registry_snapshot_epoch: Epoch::new(1),
                nonce: Nonce::from_bytes([0x22u8; 32]),
            }),
            canonical_round_trips: false,
        },
    ]
}

#[test]
fn tv_cf_20_hex_rendering_arm_acceptance_set_is_explicit() {
    let cases = arms();
    let mut problems: Vec<String> = Vec::new();

    for case in &cases {
        // The derived form is the arm's own Serialize output and must always
        // round-trip. If it does not, the arm is broken independently of
        // anything this amendment is about.
        let derived = serde_json::to_value(&case.caveat).expect("derived serialize");
        let derived_ok = serde_json::from_value::<Caveat>(derived).is_ok();
        if !derived_ok {
            problems.push(format!(
                "{}: the crate's own derived Serialize output does not round-trip, \
                 which is a defect independent of the canonical form",
                case.name
            ));
            continue;
        }

        // The canonical form must render hex, or the arm does not belong in
        // this table.
        let canonical = case.caveat.canonical_ser();
        let text = String::from_utf8_lossy(&canonical);
        if !text.contains("\"") {
            problems.push(format!("{}: canonical output is not JSON", case.name));
            continue;
        }
        let renders_hex = hex_renders_anywhere(&text);
        if !renders_hex {
            problems.push(format!(
                "{}: expected the canonical form to render bytes as hex, found {text}",
                case.name
            ));
            continue;
        }

        // The asymmetry itself.
        let canonical_ok = serde_json::from_slice::<Caveat>(&canonical).is_ok();
        if canonical_ok != case.canonical_round_trips {
            problems.push(format!(
                "{}: expected canonical_round_trips={}, observed {} for {text}",
                case.name, case.canonical_round_trips, canonical_ok
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "the hex-rendering arm acceptance set changed:\n  {}\n\n\
         Update the arm list in this file and RFC-0011 §Caveat Form Amendment \
         §Known deviations together, and state the wire-format cost of any arm \
         that changed sides.",
        problems.join("\n  ")
    );
}

/// The eight arms that render hex and still reject their own canonical output
/// must keep doing so for a STATED reason, and the reason lives next to the pin.
#[test]
fn tv_cf_21_known_asymmetric_arms_still_reject_their_own_canonical_form() {
    let all = arms();
    let asymmetric: Vec<&ArmCase> = all.iter().filter(|c| !c.canonical_round_trips).collect();

    assert_eq!(
        asymmetric.len(),
        8,
        "the asymmetric arm count changed; that is a substantive change to the \
         deviation, not a test-maintenance detail"
    );

    for case in asymmetric {
        let canonical = case.caveat.canonical_ser();
        assert!(
            serde_json::from_slice::<Caveat>(&canonical).is_err(),
            "{} now accepts its own canonical form. That is progress, not a \
             broken test. Land it as a deliberate wire-format decision, drop the \
             arm from this vector, and update RFC-0011 §Caveat Form Amendment \
             §Known deviations in the same commit.",
            case.name
        );
    }
}

/// MECHANICAL derivation of the hex-rendering set from the `canonical_ser`
/// source, compared against the literal above.
///
/// The first review pass wrote the literal by hand and then checked it against
/// other documents, which is why four arms went missing while every
/// cross-document comparison still agreed. This vector compares the literal
/// against the code that decides the answer.
///
/// A new arm that renders bytes through `hex::encode` fails this vector, which
/// is the whole point: a set nobody can add to without noticing is not a
/// completeness guarantee, it is a hope.
#[test]
fn tv_cf_22_hex_rendering_set_matches_canonical_ser_source() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join("src/caveat/mod.rs");
    let source = std::fs::read_to_string(&path)
        .expect("canonical_ser source must be readable from the test");

    // Isolate the canonical_ser function body.
    let start = source
        .find("pub fn canonical_ser")
        .expect("canonical_ser must exist");
    let body = &source[start..];

    // Match arms sit at a fixed indent inside that function. Split on them and
    // keep the arms whose body renders bytes through hex::encode.
    let mut derived: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    let mut current_renders_hex = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        let arm_start = trimmed.strip_prefix("Caveat::");
        if let Some(rest) = arm_start {
            if let Some(prev) = current.take() {
                if current_renders_hex {
                    derived.push(prev);
                }
            }
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            current = Some(name);
            current_renders_hex = false;
            // Do NOT skip the rest of this line. Several arms are written on
            // one line (`Caveat::Vault(id) => json!({ "type": "vault",
            // "value": hex::encode(id) }),`), and an earlier version of this
            // parser `continue`d here, which silently dropped every
            // single-line arm from the derived set. The scan then agreed with
            // a literal that had Vault in it and disagreed with the source.
            // A completeness check that under-reports is worse than none.
        }
        if line.contains("hex::encode") {
            current_renders_hex = true;
        }
        // Stop at the end of the match, which is the first line closing the
        // function body after the arms.
        if current.is_some() && trimmed.starts_with("};") {
            if let Some(prev) = current.take() {
                if current_renders_hex {
                    derived.push(prev);
                }
            }
            break;
        }
    }
    if let Some(prev) = current.take() {
        if current_renders_hex {
            derived.push(prev);
        }
    }

    assert!(
        !derived.is_empty(),
        "the source scan found no hex-rendering arms, so it is not actually \
         reading the match it claims to read"
    );

    // Compare as sets, since the literal is ordered for readability and the
    // scan returns source order.
    let mut from_source = derived.clone();
    let mut from_literal: Vec<String> = HEX_RENDERING_ARMS.iter().map(|s| s.to_string()).collect();
    from_source.sort();
    from_literal.sort();
    from_source.dedup();
    from_literal.dedup();

    assert_eq!(
        from_source, from_literal,
        "the arms rendering hex in canonical_ser and the arms listed in \
         HEX_RENDERING_ARMS have diverged.\n  from source: {from_source:?}\n  \
         from literal: {from_literal:?}\nA new hex-rendering arm must be added \
         to both this file and RFC-0011 §Caveat Form Amendment §Known deviations."
    );
}

/// Guards the scan in `tv_cf_22` against silent truncation.
///
/// The scan stops at the first line beginning `};` that appears while a match
/// arm is open, which today is exactly the end of `canonical_ser`'s
/// `let value = match self { ... };`. That is correct by coincidence of
/// formatting, not by construction: a future refactor that introduced a nested
/// block ending in `};` inside an arm would stop the scan early, and the
/// comparison would then hold for a SHORT list and report agreement. That is
/// the under-reporting failure this whole check exists to prevent, so it gets
/// its own guard rather than a hope.
///
/// The sentinel is the enum's LAST variant, read from the enum declaration
/// rather than hard-coded. If a new final variant lands, this fails until the
/// scan is confirmed to reach it. If the scan stops early, this fails.
#[test]
fn tv_cf_23_hex_scan_reaches_the_final_caveat_variant() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join("src/caveat/mod.rs");
    let source = std::fs::read_to_string(&path).expect("canonical_ser source readable");

    // The enum's last variant. Bounded by brace counting over the enum
    // declaration: scanning to end-of-file instead picks up `Caveat::` match
    // arms in LATER FUNCTIONS and reports one of those as the final variant,
    // which makes the sentinel wrong while still happening to sit far enough
    // down the file to catch truncation. A sentinel that is wrong for the
    // stated reason is the same class of defect as the one being guarded.
    let enum_start = source
        .find("pub enum Caveat {")
        .expect("Caveat enum must exist");
    let after_open = enum_start + "pub enum Caveat {".len();
    let mut depth = 1usize;
    let mut enum_end = after_open;
    for (offset, ch) in source[after_open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    enum_end = after_open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let enum_body = &source[after_open..enum_end];
    assert!(
        !enum_body.is_empty(),
        "could not bound the Caveat enum body, so the sentinel would be unreliable"
    );
    // Inside the enum declaration a variant is a BARE name at one indent level
    // (`AmountMax(Dqa),`, `WrappedOnly { ... },`) — there is no `Caveat::`
    // prefix. A first attempt of this guard looked for that prefix, found
    // nothing inside the enum body, and only ever "worked" because it was
    // scanning past the enum and matching later `Caveat::` match arms, which
    // made the sentinel a different variant than the one it claimed to name.
    let last_variant = enum_body
        .lines()
        .filter_map(|line| {
            let name: String = line
                .chars()
                .skip_while(|c| c.is_whitespace())
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() || !name.starts_with(|c: char| c.is_uppercase()) {
                return None;
            }
            let rest = line.trim_start().trim_start_matches(&name);
            (rest.starts_with('(') || rest.starts_with('{') || rest.starts_with(','))
                .then_some(name)
        })
        .next_back()
        .expect("the Caveat enum must declare at least one variant");

    // Re-run the same stop rule the scan uses, and check it got that far.
    let start = source
        .find("pub fn canonical_ser")
        .expect("canonical_ser must exist");
    let mut reached_last = false;
    let mut current: Option<String> = None;
    for line in source[start..].lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Caveat::") {
            current = Some(
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect(),
            );
            if current.as_deref() == Some(last_variant.as_str()) {
                reached_last = true;
            }
        }
        if current.is_some() && trimmed.starts_with("};") {
            break;
        }
    }

    assert!(
        reached_last,
        "the canonical_ser scan never reached the enum's final variant \
         ({last_variant}), so it stopped early and tv_cf_22 was comparing a \
         truncated list against the literal. Fix the scan bound rather than the \
         literal."
    );
}

/// True when any string value anywhere in the canonical JSON is a hex string
/// of 16 or 32 bytes, which is the shape of a rendered `Dqa` payload or a
/// rendered 32-byte id. Used instead of checking one specific key, so an
/// object-shaped arm whose hex lives under a field name is recognised the same
/// as a bare-hex one.
fn hex_renders_anywhere(canonical: &str) -> bool {
    let value: serde_json::Value = serde_json::from_str(canonical).expect("canonical is JSON");
    fn walk(v: &serde_json::Value) -> bool {
        match v {
            // 32 chars is a 16-byte Dqa payload, 64 chars is a 32-byte id.
            // Both are the hex-rendering shape; only the width differs.
            serde_json::Value::String(s) => {
                matches!(s.len(), 32 | 64) && s.chars().all(|c| c.is_ascii_hexdigit())
            }
            serde_json::Value::Array(items) => items.iter().any(walk),
            serde_json::Value::Object(map) => map.values().any(walk),
            _ => false,
        }
    }
    walk(&value)
}
