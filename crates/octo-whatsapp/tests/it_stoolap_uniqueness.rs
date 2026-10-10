//! Invariant: the runtime crate must keep the stoolap dependency
//! scoped to its legitimate boundaries.
//!
//! **History:**
//!
//! - **Phase 5 (2026-07-05)**: the original test was stricter —
//!   "no `use stoolap` anywhere in `octo-whatsapp/src/`". All
//!   storage went through `Arc<StoolapStore>` from
//!   `octo-adapter-whatsapp`.
//! - **Phase 8 (2026-07-11)**: the query layer was added inside
//!   `octo-whatsapp/src/query/`. It needs direct stoolap access
//!   for the `VECTOR` type + dynamic DDL/DML via `sql.*` RPCs.
//!   The original invariant was therefore deliberately
//!   narrowed.
//! - **Phase 9 (2026-07-13)**: the `sql.{execute,query,tables}`
//!   RPCs were added in `src/ipc/handlers/sql.rs`. They are the
//!   dynamic-SQL boundary — every consumer of the runtime that
//!   touches the SQL engine goes through `sql.*`, never directly.
//!
//! The current invariant therefore asserts:
//!
//! 1. `src/query/**` MAY reference stoolap (it's the new
//!    storage boundary for derived views).
//! 2. `src/ipc/handlers/sql.rs` MAY reference stoolap (it's the
//!    dynamic-SQL boundary for external callers).
//! 3. Everything ELSE in `src/**` MUST NOT touch stoolap. A
//!    violation here means a new handler / module is reaching
//!    past the documented boundary instead of routing through
//!    `query_subsystem()` / `query_service()` / `sql.*`.

use std::fs;
use std::path::Path;

#[test]
fn no_direct_stoolap_dependency() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(src.is_dir(), "expected octo-whatsapp src/ at {:?}", src);
    let entries = walkdir(&src);
    let mut bad = Vec::new();
    for entry in &entries {
        if entry.extension().map(|x| x == "rs").unwrap_or(false) {
            // Legitimate boundaries: see module docstring.
            let path_str = entry.to_string_lossy().replace('\\', "/");
            if path_str.contains("/query/")
                || path_str.ends_with("/query/mod.rs")
                || path_str.ends_with("/ipc/handlers/sql.rs")
            {
                continue;
            }
            let content = fs::read_to_string(&entry).unwrap();
            // Skip lines inside `#[cfg(test)]` blocks — those
            // are not compiled into the production binary so
            // they cannot violate the runtime's coupling to
            // stoolap. We track brace depth inside the cfg(test)
            // region; lines inside that region are ignored
            // until depth returns to zero.
            //
            // The depth tracker uses a single counter that
            // combines `#[cfg(test)]` entry events with raw
            // `{` / `}` counts. Any line that contains
            // `#[cfg(test)]` (including the `cfg(all(test, …))`
            // variant) increments the counter. Any line that
            // contains `{` / `}` adjusts the counter by the net
            // delta. When the counter returns to zero, we're
            // outside the cfg(test) region again.
            let mut cfg_test_depth: u32 = 0;
            for (lineno, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[cfg(all(test, ") {
                    cfg_test_depth += 1;
                    continue;
                }
                if cfg_test_depth > 0 {
                    let opens = trimmed.matches('{').count() as u32;
                    let closes = trimmed.matches('}').count() as u32;
                    // `mod tests {` opens 1; the matching `}`
                    // closes it. Until the counter returns to 0
                    // we're inside the cfg(test) region.
                    cfg_test_depth = cfg_test_depth.saturating_add(opens).saturating_sub(closes);
                    // Safety: if the file ends inside a
                    // cfg(test) region, reset on the next file.
                    continue;
                }
                // Look for Rust-level references only (imports,
                // types, function calls, path segments). String
                // literals like `"stoolap_persist_queue_depth"` in
                // stub JSON keys are allowed — they don't pull the
                // crate.
                let bad_patterns = [
                    "use stoolap",
                    "stoolap::",
                    "<Stoolap",
                    ": Stoolap",
                    "stoolap.",
                    " stoolap {",
                    "\tstoolap {",
                ];
                for needle in &bad_patterns {
                    if line.contains(needle) {
                        bad.push(format!("{}:{}: {}", entry.display(), lineno + 1, line));
                    }
                }
            }
            // Reset for safety on file boundaries (the per-file
            // counter never carries across files in the test).
        }
    }
    // Anti-vacuity: `walkdir` returns an empty Vec when its root is not a
    // directory, and `bad.is_empty()` is then trivially true. Without this
    // floor a moved or renamed `src/` turns the guard green while it inspects
    // nothing — the exact "found nothing wrong" vs "looked at nothing"
    // confusion this guard exists to rule out. The bound is deliberately far
    // below today's count so ordinary file churn never trips it; its job is
    // to catch a collapsed walk, not to track the tree.
    assert!(
        entries.len() >= 100,
        "only {} files walked under {} — the scan saw nothing, so a green \
         result here means nothing was inspected",
        entries.len(),
        src.display()
    );
    assert!(
        bad.is_empty(),
        "octo-whatsapp src/ must not reference stoolap outside `query/` and `ipc/handlers/sql.rs`; offenders:\n{}",
        bad.join("\n"),
    );
}

fn walkdir(p: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(p).unwrap() {
        let e = entry.unwrap().path();
        if e.is_dir() {
            out.extend(walkdir(&e));
        } else if e.extension().map(|x| x == "rs").unwrap_or(false) {
            out.push(e);
        }
    }
    out
}
