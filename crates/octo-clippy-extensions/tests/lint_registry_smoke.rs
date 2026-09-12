//! Lint registry smoke tests (RFC-0012-v3 §FW2 + RFC-0014-v3 §FW4).
//!
//! Asserts the Layer E per-extension registry pattern is wired: the
//! audit-side `no_raw_format_err` + settlement-side `no_raw_to_string`
//! modules each register their metadata. Lookups are by NAME (not
//! positional) so adding future lint modules to the registry is
//! non-breaking. Activation posture stays off-by-default at v1.0.
//!
//! Run with:
//!   cargo test -p octo-clippy-extensions --test lint_registry_smoke

use octo_clippy_extensions::{all_modules, LintModule};

const AUDIT_SIDE_MODULE_NAME: &str = "no_raw_format_err";
const SETTLEMENT_SIDE_MODULE_NAME: &str = "no_raw_to_string";

#[test]
fn registry_includes_both_audit_and_settlement_modules() {
    let modules = all_modules();
    let names: Vec<&str> = modules.iter().map(|m| m.name).collect();
    assert!(
        names.contains(&AUDIT_SIDE_MODULE_NAME),
        "registry must include audit-side module `{}`; got {:?}",
        AUDIT_SIDE_MODULE_NAME,
        names
    );
    assert!(
        names.contains(&SETTLEMENT_SIDE_MODULE_NAME),
        "registry must include settlement-side module `{}`; got {:?}",
        SETTLEMENT_SIDE_MODULE_NAME,
        names
    );
}

#[test]
fn registry_audit_side_module_metadata() {
    let m = find_module(AUDIT_SIDE_MODULE_NAME);
    assert_eq!(m.name, AUDIT_SIDE_MODULE_NAME);
    assert_eq!(m.gate, "lint-no-raw-format-err");
    assert!(m.scope.contains("storage"), "{}", m.scope);
    assert!(
        m.description.contains("scrub_adapter_error_with"),
        "{}",
        m.description
    );
}

#[test]
fn registry_settlement_side_module_metadata() {
    let m = find_module(SETTLEMENT_SIDE_MODULE_NAME);
    assert_eq!(m.name, SETTLEMENT_SIDE_MODULE_NAME);
    assert_eq!(m.gate, "lint-no-raw-to-string");
    assert!(
        m.scope.contains("quota-router-sm-engine") || m.scope.contains("octo-settlement"),
        "{}",
        m.scope
    );
    assert!(
        m.description.contains("scrub_adapter_error_with"),
        "{}",
        m.description
    );
}

#[test]
fn registry_gates_are_distinct() {
    let gates: Vec<&str> = all_modules().iter().map(|m| m.gate).collect();
    let unique: std::collections::HashSet<&str> = gates.iter().copied().collect();
    assert_eq!(
        unique.len(),
        gates.len(),
        "lint module gates must be distinct; got {:?}",
        gates
    );
}

#[test]
fn registry_module_names_are_unique() {
    // R2.5 fix: replace tautological name-uniqueness check with a
    // meaningful registry-count assertion. The hardcoded `vec![...]`
    // construction at lib.rs makes the per-pair name uniqueness
    // structurally guaranteed — a regression requires editing BOTH
    // the `name` field AND the registry call site simultaneously.
    // The count assertion catches an off-by-one or accidental
    // duplicate registration (e.g., vec![m, m] from a copy-paste
    // mistake in the aggregator).
    let modules = all_modules();
    let count = modules.len();
    assert!(
        count >= 2,
        "registry must include at least the audit-side + settlement-side \
         modules; got {} entries",
        count
    );
    let names: Vec<&str> = modules.iter().map(|m| m.name).collect();
    let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "lint module names must be distinct; got {:?}",
        names
    );
}

#[test]
fn lint_module_struct_partial_eq_round_trip() {
    // Replace the tautological round-trip with a structural invariant
    // check: the struct satisfies Clone + PartialEq + Eq + Debug so
    // consumers can build sets + indexes keyed by name.
    let m = find_module(AUDIT_SIDE_MODULE_NAME);
    let cloned = m.clone();
    assert_eq!(m, cloned);
    let debug = format!("{:?}", m);
    assert!(
        debug.contains(AUDIT_SIDE_MODULE_NAME),
        "Debug must include the module name: {}",
        debug
    );
}

fn find_module(name: &str) -> LintModule {
    all_modules()
        .into_iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("registry must contain module `{}`", name))
}
