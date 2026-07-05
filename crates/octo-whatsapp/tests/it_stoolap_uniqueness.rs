//! Invariant: the runtime crate MUST NOT directly depend on stoolap.
//! All stoolap access goes via `Arc<StoolapStore>` cloned from
//! `octo-adapter-whatsapp` at startup. This test enforces that by greping
//! the source tree for forbidden patterns.

use std::fs;
use std::path::Path;

#[test]
fn no_direct_stoolap_dependency() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    for entry in walkdir(&src) {
        if entry.extension().map(|x| x == "rs").unwrap_or(false) {
            let content = fs::read_to_string(&entry).unwrap();
            for (lineno, line) in content.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                // Look for Rust-level references only (imports, types,
                // function calls, path segments). String literals like
                // `"stoolap_persist_queue_depth"` in stub JSON keys are
                // allowed — they don't pull the crate.
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
        }
    }
    assert!(
        bad.is_empty(),
        "octo-whatsapp src/ must not reference stoolap as a Rust dependency; offenders:\n{}",
        bad.join("\n"),
    );
}

fn walkdir(p: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if p.is_dir() {
        for entry in fs::read_dir(p).unwrap() {
            let e = entry.unwrap().path();
            if e.is_dir() {
                out.extend(walkdir(&e));
            } else if e.extension().map(|x| x == "rs").unwrap_or(false) {
                out.push(e);
            }
        }
    }
    out
}
