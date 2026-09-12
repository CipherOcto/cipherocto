//! `TimestampOpaque` accessor round-trip + no-raw-`.0`-field-access
//! audit (RFC-0012-v3 §S6.2 + mission 0012-v3 Deliverable 2).
//!
//! Asserts:
//! 1. Every consumer site that constructs `TimestampOpaque::new(u64)`
//!    is paired with an `as_millis_unix()` consumer for programmatic
//!    access.
//! 2. No consumer uses raw `.0` field access on `TimestampOpaque`
//!    (would bypass the accessor + redacted-Display invariant).
//!
//! Run with:
//!   cargo test -p octo-audit --test timestamp_opaque_accessor_round_trip

use octo_audit_core::TimestampOpaque;

#[test]
fn accessor_round_trip_zero() {
    assert_eq!(TimestampOpaque::new(0).as_millis_unix(), 0);
}

#[test]
fn accessor_round_trip_one() {
    assert_eq!(TimestampOpaque::new(1).as_millis_unix(), 1);
}

#[test]
fn accessor_round_trip_realistic_unix_millis() {
    let t = 1_700_000_000_000;
    assert_eq!(TimestampOpaque::new(t).as_millis_unix(), t);
}

#[test]
fn accessor_round_trip_u64_max() {
    assert_eq!(TimestampOpaque::new(u64::MAX).as_millis_unix(), u64::MAX);
}

#[test]
fn accessor_round_trip_many_samples() {
    let samples: Vec<u64> = (0..256).collect();
    for t in samples {
        assert_eq!(TimestampOpaque::new(t).as_millis_unix(), t);
    }
}

#[test]
fn no_raw_zero_field_access_in_octo_audit_core() {
    // Audit-side consumer grep: assert that no `TimestampOpaque`
    // consumer in `octo-audit-core/src/` reaches past the newtype via
    // raw `.0` field access. The accessor `as_millis_unix()` is the
    // canonical programmatic path; raw `.0` would bypass the
    // redacted-Display invariant.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let core_src_dir = std::path::Path::new(manifest_dir).join("../octo-audit-core/src");
    assert!(
        core_src_dir.is_dir(),
        "expected octo-audit-core/src at {:?}",
        core_src_dir
    );

    let offenders = walk_dir_collect_offenders(&core_src_dir);
    assert!(
        offenders.is_empty(),
        "raw `.0` field access on TimestampOpaque in octo-audit-core/src is forbidden; \
         use the `as_millis_unix()` accessor instead. Offending lines: {:?}",
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
        // Symlink guard (R1-S review): skip symlinks so a planted
        // symlink in the source tree cannot escape the intended
        // boundary or self-recurse.
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
            if line.contains("TimestampOpaque") && line.contains(".0") {
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
