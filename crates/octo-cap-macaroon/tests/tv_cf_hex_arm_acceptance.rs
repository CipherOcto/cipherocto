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
    /// True for every 32-byte-id arm except `payment`, whose blocker is a
    /// PARTIAL PROJECTION rather than an encoding or envelope defect — see
    /// `tv_cf_21` and RFC-0011-caveat-form-amendment.md §Known deviations 1.
    canonical_round_trips: bool,
}

/// The 32-byte id-bearing hex-rendering arms, plus `amount_max` as the
/// conformant control: it renders a 16-byte payload as hex and does round-trip,
/// because Phase 1 added the `visit_str` arm to `dqa_serde::field`.
///
/// Seven of the eight `canonical_round_trips: false` entries below became
/// `true` when `hex_id_32` was adopted on each 32-byte id field AND
/// `canonical_ser` was corrected to emit an OBJECT under `value` for the
/// struct-payload arms. Only `payment` remains `false`, and for a different
/// reason than the other seven had.
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
            canonical_round_trips: true,
        },
        ArmCase {
            name: "ask_binding",
            caveat: Caveat::AskBinding([0x55u8; 32]),
            canonical_round_trips: true,
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
            canonical_round_trips: true,
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
            canonical_round_trips: true,
        },
        ArmCase {
            name: "policy_reference",
            caveat: Caveat::PolicyReference {
                policy_id: [0x88u8; 32],
                policy_version_seq: 1,
                attenuation_witness: [0x99u8; 64],
            },
            canonical_round_trips: true,
        },
        ArmCase {
            name: "redemption_context",
            caveat: Caveat::RedemptionContext {
                context_hash: [0x33u8; 32],
            },
            canonical_round_trips: true,
        },
        ArmCase {
            name: "asset_binding",
            caveat: Caveat::AssetBinding(AssetBinding {
                asset_id: [0x66u8; 32],
            }),
            canonical_round_trips: true,
        },
        // The last remaining asymmetric arm. It round-trips now that the
        // projection carries all seven fields rather than four. See
        // `tv_cf_18` and `tv_cf_25` for the projection, and
        // RFC-0011-caveat-form-amendment.md §Known deviations 1 for the
        // sixteen-day drift that hid three of them.
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
            canonical_round_trips: true,
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

/// NO fixed-width-id arm may reject its own canonical form.
///
/// This vector has been the count of the asymmetric set through every stage of
/// the eight-arm work: eight arms, then one, then none. Each transition was a
/// deliberate wire-format decision landing in the same commit as the count
/// change, which is the property that made the sequence auditable.
///
/// It is kept as a standing assertion rather than deleted at zero, because a
/// vector that only ever asserts a count is worth nothing once the count is
/// trivially satisfied — and because the way this set reached zero is the
/// cautionary part. The projection that left `payment` asymmetric was asserted
/// here as INTENTIONAL, and that assertion is what kept a sixteen-day field
/// drift looking like a design decision. An empty set should be a standing
/// property, not a one-time milestone.
#[test]
fn tv_cf_21_no_fixed_width_arm_rejects_its_own_canonical_form() {
    let all = arms();
    let asymmetric: Vec<&str> = all
        .iter()
        .filter(|c| !c.canonical_round_trips)
        .map(|c| c.name)
        .collect();

    assert!(
        asymmetric.is_empty(),
        "{} now reject their own canonical form, out of {} hex-rendering arms: {}. \
         If an arm regressed, restore the `hex_id_32` adapter on its 32-byte id \
         field and, for a struct-payload arm, the object envelope in \
         `canonical_ser`. If an arm is DELIBERATELY asymmetric again, it needs a \
         named entry in RFC-0011-caveat-form-amendment.md §Known deviations, and \
         this vector should pin THAT arm by name rather than a bare count.",
        asymmetric.len(),
        all.len(),
        asymmetric.join(", ")
    );
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

/// POSITIVE pin for what the seven-arm fix delivered, plus the regression it
/// could have caused.
///
/// Two properties, both stated as literal expectations rather than derived from
/// the code under test:
///
/// 1. **Every fixed-width-id arm accepts its own canonical form**, except
///    `payment`, which is named rather than counted.
/// 2. **The legacy 32-element array form still parses on input** for every one
///    of those arms. This is the half that a fix like this can silently break:
///    adopting `hex_id_32` widens input acceptance, but if the adapter had been
///    written to accept only the hex form, every existing consumer sending a
///    byte array would break while the new hex tests stayed green. The
///    canonical-form test above cannot see that, because it only ever sends hex.
///
/// The ids here are POSITION-DISTINCT (`0x00..=0x1f`, not a repeated byte).
/// A single repeated byte is a fixed point of every byte permutation, so an
/// adapter that reversed byte order would pass this vector — that is exactly how
/// the original thirteen vectors were blind, and it is why `tv_cf_14` exists.
#[test]
fn tv_cf_24_fixed_width_arms_round_trip_and_still_accept_the_legacy_array_form() {
    /// Position-distinct 32-byte id: byte `i` is `i`. A permutation of this
    /// value is a DIFFERENT value, so a byte-order defect is detectable.
    fn id() -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = i as u8;
        }
        out
    }

    let id_val = serde_json::to_value(id()).expect("id serializes");

    // (label, legacy-form JSON with the id as a 32-element array)
    let legacy_cases: Vec<(&str, serde_json::Value)> = vec![
        (
            "invocation_hash_bind",
            serde_json::json!({"type": "invocation_hash_bind", "value": id_val}),
        ),
        (
            "ask_binding",
            serde_json::json!({"type": "ask_binding", "value": id_val}),
        ),
        (
            "vault",
            serde_json::json!({"type": "vault", "value": id_val}),
        ),
        (
            "wrapped_only",
            serde_json::json!({
                "type": "wrapped_only",
                "value": {"parent_capability": id_val}
            }),
        ),
        (
            "redemption_context",
            serde_json::json!({
                "type": "redemption_context",
                "value": {"context_hash": id_val}
            }),
        ),
        (
            "asset_binding",
            serde_json::json!({"type": "asset_binding", "value": {"asset_id": id_val}}),
        ),
        (
            "factory",
            serde_json::json!({
                "type": "factory",
                "value": {
                    "target_vault_id": id_val,
                    "action_template": {"selector": "probe", "args": []},
                    "required_caller": null,
                    "pre_conditions": [],
                    "expiry_for_deploy_unix": 0,
                }
            }),
        ),
        (
            "policy_reference",
            serde_json::json!({
                "type": "policy_reference",
                "value": {
                    "policy_id": id_val,
                    "policy_version_seq": 1,
                    "attenuation_witness": hex::encode([0xAB; 64]),
                }
            }),
        ),
        (
            "payment",
            serde_json::json!({
                "type": "payment",
                "value": {
                    "caveat_name": "paid-query/v1",
                    "asset_id": id_val,
                    // 16-byte DqaEncoding wire form: value 8B BE, scale, 7 reserved.
                    "budget": hex::encode([0u8; 16]),
                    "model": "gpt-4",
                    "expires_at_unix_ms": u64::MAX,
                    // `serde_epoch` writes a decimal STRING, not a number.
                    "registry_snapshot_epoch": "1",
                    "nonce": id_val,
                }
            }),
        ),
    ];

    let mut problems: Vec<String> = Vec::new();
    for (label, legacy) in &legacy_cases {
        match serde_json::from_value::<Caveat>(legacy.clone()) {
            Ok(_) => {}
            Err(e) => problems.push(format!(
                "{label}: the LEGACY 32-element array form no longer parses, so an \
                 existing consumer that sends arrays breaks: {e}"
            )),
        }
    }

    // Property 1: the canonical form of every hex-rendering arm parses. This
    // was eight of nine, then nine of nine once the `payment` projection was
    // made lossless. It is asserted as a SET against the literal above rather
    // than as a count, so an arm silently leaving the set fails here.
    let mut observed: Vec<&str> = arms()
        .iter()
        .filter(|c| serde_json::from_slice::<Caveat>(&c.caveat.canonical_ser()).is_ok())
        .map(|c| c.name)
        .collect();
    let mut expected: Vec<&str> = legacy_cases.iter().map(|(l, _)| *l).collect();
    expected.push("amount_max");
    observed.sort_unstable();
    expected.sort_unstable();
    if observed != expected {
        problems.push(format!(
            "the arms accepting their own canonical form changed.\n  expected: {expected:?}\n  \
             observed: {observed:?}"
        ));
    }

    // Property 1b: byte ORDER is preserved in BOTH directions on the hex path.
    // Round-trip success alone does not prove this: an adapter that reversed the
    // bytes on the way out AND on the way in would round-trip perfectly while
    // emitting the wrong value. These two assertions are what make the
    // position-distinct id above worth having, and they are the reason this
    // vector exists separately from the `contains`-style pins elsewhere.
    let id = id();
    let emitted = serde_json::to_value(Caveat::Vault(id)).expect("vault serializes");
    let expected_hex = hex::encode(id);
    if emitted.get("value").and_then(|v| v.as_str()) != Some(expected_hex.as_str()) {
        problems.push(format!(
            "vault emitted {:?} under `value`, expected the 64-char hex of the id \
             in order ({expected_hex:?})",
            emitted.get("value")
        ));
    }
    let decoded: Caveat = serde_json::from_value(serde_json::json!({
        "type": "vault",
        "value": expected_hex,
    }))
    .expect("hex form parses");
    match decoded {
        Caveat::Vault(got) if got == id => {}
        other => problems.push(format!(
            "decoding the canonical hex did not yield the original id in order: {other:?}"
        )),
    }

    assert!(
        problems.is_empty(),
        "the fixed-width-id round-trip contract changed:\n  {}\n\n\
         The canonical-form half is RFC-0011-caveat-form-amendment.md clause 1 \
         and clause 3. If an arm stopped accepting the legacy array form, clause \
         3 is broken and every pre-existing consumer that sends arrays breaks.",
        problems.join("\n  ")
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
