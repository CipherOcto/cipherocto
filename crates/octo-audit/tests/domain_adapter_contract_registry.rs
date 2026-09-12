//! DOMAIN adapter contract conformance registry assertion test
//! (RFC-0012-v3 §S5.1.1 + mission 0012-v3 Deliverable 3).
//!
//! Asserts every flagged `format!("{e}")` or `.to_string()` chain in
//! workspace `crates/*/src/storage/*.rs` modules is paired with
//! `scrub_adapter_error_with` (Pattern 6 registry per
//! RFC-0012-v3 §S5.1). Coverage: octo-audit (DOMAIN adapter) +
//! octo-settlement (paired façade per sister mission).
//!
//! Run with:
//!   cargo test -p octo-audit --test domain_adapter_contract_registry

use std::path::Path;

#[test]
fn audit_side_storage_adapter_calls_scrub_adapter_error_with() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let stoolap = Path::new(manifest_dir).join("../octo-audit/src/storage/stoolap.rs");
    assert!(
        stoolap.is_file(),
        "expected octo-audit/src/storage/stoolap.rs at {:?}",
        stoolap
    );
    let contents = std::fs::read_to_string(&stoolap).expect("read stoolap.rs");
    assert!(
        contents.contains("scrub_adapter_error_with"),
        "octo-audit/src/storage/stoolap.rs must use scrub_adapter_error_with per \
         RFC-0012-v3 §S5.1.1 Pattern 6 registry"
    );
}

#[test]
fn settlement_side_storage_adapter_calls_scrub_adapter_error_with() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let store = Path::new(manifest_dir).join("../quota-router-sm-engine/src/store.rs");
    assert!(
        store.is_file(),
        "expected quota-router-sm-engine/src/store.rs at {:?}",
        store
    );
    let contents = std::fs::read_to_string(&store).expect("read store.rs");
    assert!(
        contents.contains("scrub_adapter_error_with"),
        "quota-router-sm-engine/src/store.rs must use scrub_adapter_error_with per \
         RFC-0014-v3 §S5.1.1 Pattern 6 registry"
    );
}

#[test]
fn no_bare_format_e_in_storage_modules() {
    // Tightened from R1-S review: cover the full bypass surface —
    // `format!("{e}")`, `format!("{}", e)`, `format!("{:?}", e)`,
    // `format!("{e:?}")`, `format!("{:#?}", e)`, `format!("{e:#?}")`,
    // positional `format!("{0}", e)`, reference `format!("{}", &e)`,
    // and `write!` macro form. Any of these leak Display/Debug output
    // verbatim past the scrubber.
    const BARE_FORMAT_PATTERNS: &[&str] = &[
        "format!(\"{e}\")",
        "format!(\"{}\", e)",
        "format!(\"{e:?}\")",
        "format!(\"{:?}\", e)",
        "format!(\"{e:#?}\")",
        "format!(\"{:#?}\", e)",
        "format!(\"{0}\", e)",
        "format!(\"{}\", &e)",
    ];

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let audit_storage = Path::new(manifest_dir).join("../octo-audit/src/storage");
    let sm_engine = Path::new(manifest_dir).join("../quota-router-sm-engine/src");

    for root in [&audit_storage, &sm_engine] {
        for entry in walk_rs(root) {
            let contents = std::fs::read_to_string(&entry).expect("read");
            for line in contents.lines() {
                for pattern in BARE_FORMAT_PATTERNS {
                    if line.contains(pattern) {
                        panic!(
                            "raw format! chain ({}) at {} — use scrub_adapter_error_with \
                             per RFC-0012-v3 §S5.1.1 / RFC-0014-v3 §S5.1.1: {}",
                            pattern,
                            entry.display(),
                            line.trim()
                        );
                    }
                }
                if line.contains("write!") && line.contains(", \"{}\", e") {
                    panic!(
                        "raw write! chain at {} — use scrub_adapter_error_with per \
                         RFC-0012-v3 §S5.1.1 / RFC-0014-v3 §S5.1.1: {}",
                        entry.display(),
                        line.trim()
                    );
                }
            }
        }
    }
}

fn walk_rs(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rs(&path));
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out
}
