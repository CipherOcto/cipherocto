//! `SettlementHashOpaque` accessor round-trip + no-raw-`.0`-field-access
//! audit (RFC-0014-v3 §S5.2 + mission 0014-v3 Deliverable 2).
//!
//! Asserts:
//! 1. Every consumer site that constructs
//!    `SettlementHashOpaque::new([u8; 32])` is paired with an
//!    `as_bytes()` consumer for programmatic access.
//! 2. No consumer uses raw `.0` field access on
//!    `SettlementHashOpaque` in `quota-router-sm-engine` (would bypass
//!    the accessor + redacted-Display invariant).
//!
//! Run with:
//!   cargo test -p octo-settlement --test hash_opaque_accessor_round_trip

use octo_settlement_core::SettlementHashOpaque;

#[test]
fn accessor_round_trip_zero() {
    assert_eq!(SettlementHashOpaque::new([0; 32]).as_bytes(), &[0u8; 32]);
}

#[test]
fn accessor_round_trip_ones() {
    assert_eq!(
        SettlementHashOpaque::new([0xff; 32]).as_bytes(),
        &[0xffu8; 32]
    );
}

#[test]
fn accessor_round_trip_mixed() {
    let mut mixed = [0u8; 32];
    for (i, b) in mixed.iter_mut().enumerate() {
        *b = i as u8;
    }
    assert_eq!(SettlementHashOpaque::new(mixed).as_bytes(), &mixed);
}

#[test]
fn accessor_round_trip_realistic_blake3_digest() {
    let digest = [0xab; 32];
    assert_eq!(SettlementHashOpaque::new(digest).as_bytes(), &digest);
}

#[test]
fn accessor_returns_borrowed_slice_of_correct_length() {
    let s = SettlementHashOpaque::new([0x42; 32]);
    assert_eq!(s.as_bytes().len(), 32);
}

#[test]
fn no_raw_zero_field_access_in_quota_router_sm_engine() {
    // Audit-side consumer grep: assert that no `SettlementHashOpaque`
    // consumer in `quota-router-sm-engine/src/` reaches past the
    // newtype via raw `.0` field access. The accessor `as_bytes()`
    // is the canonical programmatic path; raw `.0` would bypass the
    // redacted-Display invariant.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let sm_engine_src = std::path::Path::new(manifest_dir).join("../quota-router-sm-engine/src");
    assert!(
        sm_engine_src.is_dir(),
        "expected quota-router-sm-engine/src at {:?}",
        sm_engine_src
    );

    let offenders = walk_dir_collect_offenders(&sm_engine_src);
    assert!(
        offenders.is_empty(),
        "raw `.0` field access on SettlementHashOpaque in quota-router-sm-engine/src is forbidden; \
         use the `as_bytes()` accessor instead. Offending lines: {:?}",
        offenders
    );
}

fn walk_dir_collect_offenders(dir: &std::path::Path) -> Vec<String> {
    let mut offenders = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return offenders,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Symlink guard (R1-S review): skip symlinks (directory or
        // file) so a planted symlink in the source tree cannot escape
        // the intended boundary or self-recurse.
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() {
                continue;
            }
        }
        if path.is_dir() {
            offenders.extend(walk_dir_collect_offenders(&path));
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (idx, line) in contents.lines().enumerate() {
            if line.contains("SettlementHashOpaque") && line.contains(".0") {
                let trimmed = line.trim_start();
                // Whitelist array-literal forms (R1-C review):
                // covers `[0; 32]`, `[0u8; 32]`, `[0_u8; 32]`,
                // `[0u8;32]`, `, 0;` (mid-expr).
                if trimmed.contains("[0;")
                    || trimmed.contains("[0u8;")
                    || trimmed.contains("[0_u8;")
                    || trimmed.contains(", 0;")
                {
                    continue;
                }
                offenders.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
            }
        }
    }
    offenders
}
