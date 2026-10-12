//! Regression guards for the 2026-10-04 red-team security audit.
//!
//! The audit's own summary pattern is "cryptographic verification that is
//! stubbed, decorative, or feature-gated off in production builds". Most of
//! those defects are behavioural and their guards live beside the code they
//! protect. This file holds the guards that must live *here* instead, because
//! the property being asserted is structural — about how a file is wired, not
//! about what a function returns:
//!
//! - a check that exists but whose call site was removed (H-10), which is the
//!   exact shape of the original defect and cannot be caught by testing the
//!   function in isolation, because the function passes its own tests;
//! - a security decision copied into N call sites (M-12), where the risk is
//!   that a future call site reintroduces the old value;
//! - three store implementations that must not drift apart (C-4);
//! - shell scripts whose guard lives in argument validation, which no Rust
//!   unit test reaches (C-6, M-10).
//!
//! Counting rules are stated inline next to every count, because every number
//! here is a claim that has to fail loudly when reality moves.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

/// Files that launch a headless Chrome against a logged-in WhatsApp profile.
const CHROME_LAUNCHERS: [&str; 3] = [
    "crates/whatsapp_chrome_session_extract/src",
    "crates/whatsapp_chrome_driver/src",
    "crates/whatsapp_chrome_reconnect_observer/src",
];

/// The store implementations that must all delegate signature verification to
/// the one helper. RULE: every file under `crates/octo-reputation/src/store`
/// that defines `verify_governance_suspension` and is not a pure delegation.
const REPUTATION_STORES: [&str; 2] = [
    "crates/octo-reputation/src/store/memory.rs",
    "crates/octo-reputation/src/store/stoolap.rs",
];

/// RULE: the QUIC accept loop is the `tokio::spawn` block that begins at
/// `// Spawn accept loop`; the replay check must live inside it, not in a
/// helper that only the tests call.
const QUIC_ACCEPT_LOOP_ANCHOR: &str = "// Spawn accept loop";

// ── C-6: systemd unit injection via unescaped --name ────────────────────────
//
// `run-octo-whatsapp.sh` interpolates `$NAME` and six path variables raw into
// a systemd unit heredoc. A newline in any of them injects unit directives and
// `systemctl --user start` executes them as the operator. The guard is
// behavioural because the whole point is that the *script* refuses the input:
// no Rust test can assert what a shell script does with a crafted argv.

/// Run the script and return (exit code, combined stderr+stdout).
fn run_script(rel: &str, args: &[&str]) -> (i32, String) {
    let script = workspace_root().join(rel);
    let out = Command::new("bash")
        .arg(&script)
        .args(args)
        .env("XDG_RUNTIME_DIR", "/tmp")
        .output()
        .unwrap_or_else(|e| panic!("{} must be runnable: {e}", script.display()));
    let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stdout));
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn c6_systemd_script_rejects_names_that_can_inject_a_unit_directive() {
    // The `--systemd` branch is the one that writes the heredoc. The newline
    // form is the working injection; `[`/`]`, a space, and the empty string
    // are the shapes the validation must also reject so no other directive
    // syntax survives.
    let hostile_names: [(&str, String); 4] = [
        (
            "newline + ExecStart",
            "x\n[Service]\nExecStart=/bin/sh -c id".to_string(),
        ),
        ("bracket", "a[Service]".to_string()),
        ("space", "has space".to_string()),
        ("empty", String::new()),
    ];
    for (label, name) in hostile_names {
        let arg = format!("--name={name}");
        let (code, text) = run_script("scripts/run-octo-whatsapp.sh", &[&arg, "--systemd"]);
        assert_ne!(
            code, 0,
            "C-6: {label} was accepted by --name (output: {text})"
        );
        assert!(
            text.contains("invalid --name"),
            "C-6: {label} should be refused with an explicit diagnostic, got: {text}"
        );
    }
}

#[test]
fn c6_systemd_script_rejects_non_absolute_or_bracket_paths() {
    // The heredoc also interpolates the path variables, which are derived from
    // env vars an operator or a wrapper may set. `run-octo-whatsapp.sh` is not
    // the only writer of these values.
    for (env_var, value) in [
        ("OCTO_WHATSAPP_LOG_DIR", "relative/path"),
        ("OCTO_WHATSAPP_LOG_DIR", "/tmp/ok[1]"),
        ("OCTO_WHATSAPP_SESSION_PATH", "relative/session"),
    ] {
        let out = Command::new("bash")
            .arg(workspace_root().join("scripts/run-octo-whatsapp.sh"))
            .arg("--systemd")
            .env(env_var, value)
            .env("XDG_RUNTIME_DIR", "/tmp")
            .output()
            .expect("script must run");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        assert_ne!(
            out.status.code().unwrap_or(-1),
            0,
            "C-6: {env_var}={value} was accepted (output: {text})"
        );
        assert!(
            text.contains("invalid path"),
            "C-6: {env_var}={value} should be refused as a path, got: {text}"
        );
    }
}

#[test]
fn c6_systemd_heredoc_quotes_the_interpolated_execstart_arguments() {
    // Defence in depth: even with validation, an unexpected value that slips
    // through must not be word-split by systemd. The unit file is generated,
    // so this asserts on the script's own text.
    let script = read("scripts/run-octo-whatsapp.sh");
    assert!(
        script.contains("ExecStart=$BIN --socket '$SOCKET' --name '$NAME' daemon"),
        "C-6: the generated ExecStart must single-quote the interpolated socket \
         and name so systemd does not word-split them"
    );
}

// ── M-10: SQL injection through LIMIT / OFFSET ─────────────────────────────

#[test]
fn m10_persist_contacts_rejects_non_integer_limit_and_offset() {
    // SQLite accepts the two-argument `LIMIT offset, count` form, so an
    // unvalidated value is a working injection that reaches the events DB.
    // The script validates its SOURCE table but not these two.
    //
    // An EMPTY argument is deliberately not a hostile case: the script reads
    // `LIMIT="${3:-1000}"`, so an empty positional means "use the default", not
    // "reject". That is the documented defaulting behaviour, not a bypass.
    let cases: [(&str, &str); 3] = [
        ("injection", "1,(SELECT sql FROM sqlite_master)--"),
        ("negative", "-1"),
        ("non-numeric", "abc"),
    ];
    for (label, value) in cases {
        // The script's exit code for an invalid argument is 3; it fails later
        // (exit 1, daemon unreachable) once the arguments are accepted, so
        // exit 3 specifically is what proves the validation fired.
        let (code, text) = run_script(
            "scripts/persist-contacts-bulk.sh",
            &["tbl", "80836284174444@lid", value, "0"],
        );
        assert_eq!(
            code, 3,
            "M-10: LIMIT={value} ({label}) was not rejected by validation \
             (exit {code}, output: {text})"
        );
        assert!(
            text.contains("must be a non-negative integer"),
            "M-10: expected an explicit LIMIT/OFFSET diagnostic, got: {text}"
        );
    }
}

#[test]
fn m10_persist_contacts_still_accepts_a_valid_pagination_pair() {
    // Anti-vacuity: if the guard rejected everything, the two fixes above
    // would pass. A well-formed pair must get past validation and fail later
    // for an unrelated reason (no daemon in the test environment).
    let (code, text) = run_script(
        "scripts/persist-contacts-bulk.sh",
        &["tbl", "80836284174444@lid", "1000", "0"],
    );
    assert!(
        !text.contains("must be a non-negative integer"),
        "M-10: a valid LIMIT/OFFSET pair was rejected: {text}"
    );
    assert_ne!(
        code, 3,
        "M-10: a valid pair must not be refused by validation"
    );
}

// ── H-10: QUIC replay protection must be wired, not merely defined ─────────

#[test]
fn h10_quic_accept_loop_consults_the_replay_cache() {
    // The original defect was precisely a divergence between the two: the
    // cache and its `check_and_insert` had thorough unit tests, and the accept
    // loop never called them. A test on `ReplayCache` cannot catch that.
    //
    // The body is scoped to `start_server`'s tail — the region containing the
    // spawned accept loop — and comments are stripped. Both matter: an
    // unscoped `contains` over the whole file is satisfied by the *public
    // `check_replay` wrapper* that wraps the real call, which is the same
    // "defined but not called from the receive path" shape the audit found.
    // The anchor is a comment, so it must be located in the raw text and only
    // the body it introduces is comment-stripped.
    let raw = read("crates/octo-adapter-quic/src/lib.rs");
    assert!(
        raw.contains(QUIC_ACCEPT_LOOP_ANCHOR),
        "H-10: the QUIC adapter no longer spawns an accept loop — re-check this guard"
    );
    let body = strip_comments(fn_body(&raw, QUIC_ACCEPT_LOOP_ANCHOR));

    assert!(
        body.contains(".check_and_insert("),
        "H-10: the spawned accept loop must consult the replay cache. \
         `ReplayCache`'s own tests pass whether or not it is called here, so \
         this is the only place the regression is visible."
    );
    // The dedup key must be derived from the payload, or the check is
    // vacuous (a constant key would drop every frame after the first).
    assert!(
        body.contains("dedup_key"),
        "H-10: the replay check must key on the received payload, not a constant"
    );
    assert!(
        !body.contains("check_and_insert(dedup_key, 0)"),
        "H-10: a constant epoch of 0 disables the 1-hour replay window — the \
         cache must be driven by local wall-clock time"
    );
    // The check has to gate delivery, not merely observe. Pinned as the
    // assignment-and-branch shape rather than three independent substrings,
    // because those are satisfied by a body that computes `is_replay` and then
    // ignores it — which the mutation check found was the weakest form.
    assert!(
        body.contains("let is_replay = {") && body.contains("if is_replay {"),
        "H-10: the accept loop must branch on the cache's verdict"
    );
    assert!(
        body.contains("continue;"),
        "H-10: a detected replay must drop the frame before it is forwarded"
    );
}

#[test]
fn h10_quic_replay_cache_is_reachable_from_the_accept_loop() {
    // The loop is spawned without `self`, so the cache has to be shared. If
    // someone reverts the field to a bare `Mutex` this stops compiling — which
    // is the intended failure mode, but the assertion documents why.
    let src = read("crates/octo-adapter-quic/src/lib.rs");
    assert!(
        src.contains("replay_cache: Arc<Mutex<ReplayCache>>"),
        "H-10: the replay cache must be shared into the spawned accept loop \
         (Arc<Mutex<..>>), not held as a bare Mutex that the loop cannot reach"
    );
}

// ── M-12: Chrome's renderer sandbox must stay on ───────────────────────────

#[test]
fn m12_no_chrome_launcher_disables_the_renderer_sandbox() {
    // RULE: no source file under the three Chrome packages may pass
    // `--no-sandbox` to a process. These binaries drive a profile holding a
    // live WhatsApp Web session, so a renderer compromise is full account
    // compromise. There is no environment escape hatch by design: these are
    // local desktop tools, and a sandbox-less container should be given user
    // namespaces instead.
    for pkg in CHROME_LAUNCHERS {
        let dir = workspace_root().join(pkg);
        let mut checked = 0usize;
        for entry in walk_rs(&dir) {
            if entry.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&entry)
                    .unwrap_or_else(|e| panic!("{} must be readable: {e}", entry.display()));
                assert!(
                    !text.contains("--no-sandbox"),
                    "M-12: {} passes --no-sandbox, disabling Chrome's renderer \
                     sandbox against a logged-in WhatsApp profile",
                    entry.display()
                );
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "M-12: no Rust sources found under {pkg} — the path is wrong, so \
             this guard is passing vacuously"
        );
    }

    // The list above is hand-written. Measured today it is complete: exactly
    // three crates depend on the Chrome session packages and none of the
    // others carry a Chrome, webdriver or chromedriver launcher. But a fourth
    // launcher added without touching this list would drive a profile
    // holding a live WhatsApp Web session with no sandbox check applied, and
    // this guard would still report green.
    //
    // Same lesson as C-4's store list: a hand-written list is only as good as
    // its worst entry, so the list is checked against the rule it claims.
    let mut launchers: Vec<String> = Vec::new();
    let crates_dir = workspace_root().join("crates");
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        panic!("crates/ must be readable");
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.path().join("Cargo.toml").is_file() {
            continue;
        }
        let manifest = std::fs::read_to_string(entry.path().join("Cargo.toml")).unwrap_or_default();
        let is_known = CHROME_LAUNCHERS.iter().any(|p| p.contains(&name));
        let depends_on_chrome = manifest.contains("whatsapp_chrome_");
        if depends_on_chrome && !is_known {
            launchers.push(name);
        }
    }
    assert!(
        launchers.is_empty(),
        "M-12: these crates depend on the Chrome session packages but are not \
         in CHROME_LAUNCHERS: {launchers:?}. Their sources get no renderer \
         sandbox check. Add them to the list."
    );
}

/// Minimal recursive walk; avoids pulling in a dependency for three directories.
fn walk_rs(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rs(&path));
        } else {
            out.push(path);
        }
    }
    out
}

// ── C-4: the three store impls must not drift apart ────────────────────────

/// Strip line and block comments from Rust source.
///
/// Source-scan guards that match a bare substring are satisfied by the *name*
/// appearing in a comment or in a test's own assertion string, so a real
/// regression can pass. This is the guard against that failure mode, and it is
/// applied to every source-scan below that is looking for a call rather than a
/// declaration.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut in_str = false;
    let mut in_raw = false;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = next {
                    out.push(n);
                    i += 2;
                    continue;
                }
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if in_raw {
            out.push(c);
            if c == '"' {
                in_raw = false;
            }
            i += 1;
            continue;
        }
        match (c, next) {
            ('/', Some('/')) => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            ('/', Some('*')) => {
                i += 2;
                let mut depth = 1usize;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == '/' && bytes.get(i + 1) == Some(&'*') {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == '*' && bytes.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }
            ('"', _) => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            ('r', Some('"')) | ('r', Some('#')) => {
                in_raw = true;
                out.push(c);
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Whether `haystack` contains `needle` as a **whole path segment**.
///
/// A bare `contains` is wrong for a path like `Signature::from_bytes`:
/// `GovernanceSignature::from_bytes` contains that string, so the AC#11 scan
/// flagged a governance newtype's own constructor as an Ed25519 site. That is
/// not a cosmetic false positive — it is the guard refusing to be extended,
/// because the first exception it reports is one nobody can act on.
///
/// The boundary that matters is on the left: a path segment starts after
/// `::`, whitespace, or a delimiter. Requiring the left boundary is enough,
/// since the needle itself ends on `(` or an identifier and a trailing
/// `FooSignature::from_bytes_x` would not match on the right either.
/// Every constructor that makes a line an AC#11 verification site.
///
/// `Signature::from_slice` was added to this list after a mutation planted
/// one in a file the pending table records as **zero** sites, and all
/// 47 vectors stayed green. The zeroed counts exist precisely to make a
/// new site fail the gate; this constructor walked straight past them.
///
/// The cause was two matchers for one question: the count summed two
/// constructors with a raw substring match, while the completeness scan
/// looked for three with path-boundary matching. Anything that decides
/// "is this a site, and how many" must go through `ac11_site_count` so the
/// two cannot disagree again.
const AC11_SITE_CONSTRUCTORS: [&str; 3] = [
    "VerifyingKey::from_bytes",
    "Signature::from_bytes",
    "Signature::from_slice",
];

/// How many AC#11 verification sites `body` holds.
///
/// Counting matters, not just presence: a pending entry pins a number so
/// that deleting one site and adding another in the same file still
/// changes the total.
fn ac11_site_count(body: &str) -> usize {
    AC11_SITE_CONSTRUCTORS
        .iter()
        .map(|ctor| count_path_calls(body, ctor))
        .sum()
}

/// Number of times `needle` occurs in `haystack` starting a **whole path
/// segment**. See `contains_path_call` for why the left boundary is required.
fn count_path_calls(haystack: &str, needle: &str) -> usize {
    let bytes = haystack.as_bytes();
    let n = needle.len();
    if n == 0 || n > bytes.len() {
        return 0;
    }
    bytes
        .windows(n)
        .enumerate()
        .filter(|(i, w)| *w == needle.as_bytes() && (*i == 0 || !is_ident_byte(bytes[i - 1])))
        .count()
}

fn contains_path_call(haystack: &str, needle: &str) -> bool {
    count_path_calls(haystack, needle) > 0
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Slice out the body of the function whose definition line contains `anchor`.
///
/// The indent of the anchor line is used to find the end: the body runs until
/// the next line at that same indent that opens another `fn`, or that closes
/// the block. Scanning for a fixed `"}"` string instead would stop at the first
/// *inner* brace — which, for a function that opens with an `if` guard, is
/// before any of the code under test.
fn fn_body<'a>(src: &'a str, anchor: &str) -> &'a str {
    let start = src
        .find(anchor)
        .unwrap_or_else(|| panic!("no function definition containing {anchor:?}"));
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let rest = &src[line_start..];

    // The terminator is the first line that is exactly `}` and is indented
    // LESS than the function body. Referencing the body's indent rather than
    // the `fn` keyword's is what makes this work for an `impl` block whose
    // methods sit at column 0 — and it is unambiguous, because every inner
    // block closes at or deeper than the body indent.
    let lines: Vec<&str> = rest.lines().collect();
    let body_indent = lines
        .iter()
        .skip(1)
        .find(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    let mut end = rest.len();
    for line in lines.iter().skip(1) {
        let trimmed = line.trim_start();
        let ind = line.len() - trimmed.len();
        if trimmed == "}" && ind < body_indent {
            let consumed: usize = lines
                .iter()
                .take_while(|l| **l != *line)
                .map(|l| l.len() + 1)
                .sum();
            end = consumed;
            break;
        }
    }
    &rest[..end]
}

#[test]
fn c4_every_reputation_store_verifies_the_governance_signature() {
    // The pre-fix defect was duplicated across all three implementations, each
    // checking shape/freshness/quorum and none checking the signature. Fixing
    // one and leaving the others is the drift this test exists to prevent.
    //
    // Comments are stripped first: the fix carries a comment naming
    // `auth::verify_governance_signature`, and matching on that would make
    // this test pass with the call deleted — which is exactly the mutation
    // check caught.
    for store in REPUTATION_STORES {
        let src = strip_comments(&read(store));
        // Both governance entry points, not one.
        //
        // This scanned only `verify_governance_suspension` while its own
        // failure message claimed to stop a caller slashing an arbitrary
        // recorder. It never looked at `slash_recorder`, which is a different
        // method taking a different record (`GovernanceProof`, not
        // `SuspensionAuth`) and which ran an `UPDATE ... SET slashed = 1`
        // with no signature check at all. Verifying one entry point and
        // assuming the other is exactly the drift this test exists to stop.
        for (method, record) in [
            ("async fn verify_governance_suspension", "SuspensionAuth"),
            ("async fn slash_recorder", "GovernanceProof"),
            ("async fn declare_retirement_eligible", "GovernanceProof"),
        ] {
            let body = fn_body(&src, method);
            assert!(
                body.contains("verify_signature(") || body.contains("verify_governance_signature("),
                "C-4: {store} {method} does not verify the governance \
                 signature. A {record} would suspend, slash or retire an \
                 arbitrary recorder with no key material."
            );
        }
    }

    // The list above is hand-written, and a hand-written list is only as good
    // as its worst entry. `REPUTATION_STORES` states its own rule — every
    // file under `src/store` that defines `verify_governance_suspension` and
    // is not a pure delegation — but nothing checked that the list still
    // matches the rule. A fourth store backend would have gone unscanned with
    // this test reporting green, which is the same blind spot it was extended
    // to close.
    //
    // A file is accounted for if it is in the list, or if its body only
    // forwards. Pure delegation cannot bypass anything: it has no
    // implementation to omit a check from.
    let store_dir = workspace_root().join("crates/octo-reputation/src/store");
    let mut unaccounted: Vec<String> = Vec::new();
    let mut walked = 0usize;
    let mut stack = vec![store_dir];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(rel) = path.strip_prefix(workspace_root()) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let src = strip_comments(&std::fs::read_to_string(&path).unwrap_or_default());
            // An implementation, not the trait declaration. `store/mod.rs`
            // declares the method with no body, and it owns no verification
            // logic that could omit a check.
            let anchor = src.find("async fn verify_governance_suspension");
            let implements = anchor.is_some_and(|i| {
                let tail = &src[i..];
                let stop = tail.find(';').unwrap_or(usize::MAX);
                tail[..stop.min(tail.len())].contains('{')
            });
            if !implements {
                continue;
            }
            walked += 1;
            if REPUTATION_STORES.contains(&rel.as_str()) {
                continue;
            }
            let body = fn_body(&src, "async fn verify_governance_suspension");
            let delegates = body.contains(".verify_governance_suspension(");
            if !delegates {
                unaccounted.push(rel);
            }
        }
    }
    assert!(
        walked > 0,
        "no store source under src/store defines verify_governance_suspension, \
         so this completeness check is reading nothing and reporting green"
    );
    assert!(
        unaccounted.is_empty(),
        "C-4: these files implement verify_governance_suspension and are not \
         in REPUTATION_STORES and do not purely delegate: {unaccounted:?}. Add \
         them to the list or the governance paths they own go unscanned."
    );
}

#[test]
fn c4_the_signature_verifier_is_a_single_shared_choke_point() {
    // If the ed25519 check is re-derived inside a store impl, the three
    // copies can drift again. The preimage construction must be reached only
    // through the shared helper.
    let auth = read("crates/octo-reputation/src/auth.rs");
    assert!(
        auth.contains("pub fn verify_governance_signature("),
        "C-4: auth.rs must own the single verification choke point"
    );
    // The count was on `.verify(&message, &signature)`, which pinned the
    // *implementation* (a direct dalek call) rather than the property the
    // guard exists for: one place where the check happens. Mission 0009-c
    // moved that check behind `AlgorithmRegistry`, so the count now follows
    // the dispatch call. Same intent, same failure mode if the choke point
    // is duplicated or removed — which is what the count is for.
    let calls = auth.matches("verify_dispatch(").count();
    assert_eq!(
        calls, 1,
        "C-4: expected exactly one registry dispatch site in auth.rs, found {calls}"
    );
    // And the choke point must not have grown a second algorithm inline.
    assert!(
        !auth.contains("VerifyingKey::from_bytes"),
        "C-4: auth.rs rebuilt an Ed25519 key inline. The whole point of the \
         choke point is that the algorithm is chosen once, in one place."
    );
}

// ── C-5: the authenticated settlement verifier must be usable ──────────────

#[test]
fn c5_the_integrity_only_verifier_no_longer_claims_authenticity() {
    // `verify_receipt_chain` is Layer A / frozen by RFC-0014-v2, and RFC-0014
    // §Threats explicitly delegates signature verification to the domain. So
    // the function's behaviour stays as specified — but its documentation must
    // not tell a caller it authenticates a router, which is what made C-5 read
    // as a substrate bug rather than a missing domain check.
    let raw = read("crates/octo-settlement-core/src/chain.rs");
    let src = raw.as_str();
    let anchor = src
        .find("pub fn verify_receipt_chain(")
        .expect("the frozen verifier must still exist");
    let doc = &src[..anchor];
    let doc_tail = &doc[doc.len().saturating_sub(1200)..];
    // Matched in the raw text, not the comment-stripped text: the sentence is
    // a doc comment and stripping comments would remove the very thing asserted.
    assert!(
        doc_tail.contains("not an authenticity check"),
        "C-5: verify_receipt_chain's docs must state that it is integrity-only. \
         Without this sentence a caller reads the frozen verifier as proof a \
         router signed the receipt, which is what made C-5 look like a substrate \
         bug rather than a missing domain check."
    );
    assert!(
        src.contains("pub fn verify_receipt_chain_authenticated("),
        "C-5: the additive authenticated verifier must exist for domains to adopt"
    );
}

// ── M-9: the events log must not be world-readable ─────────────────────────

#[test]
fn m9_events_log_files_are_opened_owner_only() {
    // The NDJSON holds full inbound message bodies. Asserted as a source check
    // because the permission depends on the open mode being set, and a test
    // that ran as root could not observe the mode meaningfully.
    let src = read("crates/octo-whatsapp/src/events_persister.rs");
    assert!(
        src.contains("open_append_private"),
        "M-9: the events log must be opened through the private-mode helper"
    );
    assert!(
        src.contains("opts.mode(0o600)"),
        "M-9: the append-mode open must request 0600"
    );
    assert!(
        src.contains("0o700"),
        "M-9: the containing directory must be restricted to 0700"
    );
    // Every `create_dir_all` on an events path must be followed by an explicit
    // restriction: `create_dir_all` applies the umask and never tightens an
    // existing directory.
    for (i, line) in src.lines().enumerate() {
        if line.contains("create_dir_all") && !line.trim_start().starts_with("//") {
            let window: String = src.lines().skip(i).take(4).collect::<Vec<_>>().join("\n");
            assert!(
                window.contains("restrict_dir")
                    || window.contains("ensure_private_parent")
                    || window.contains("restrict_dir_sync"),
                "M-9: line {} creates a directory without restricting its mode: {line}",
                i + 1
            );
        }
    }
}

// ── M-8: the IPC daemon's bearer default must stay fail-closed ─────────────

#[test]
fn m8_bearer_required_defaults_to_true() {
    // The daemon accepted unauthenticated *mutating* RPCs (including outbound
    // send) whenever the config key was absent, because `#[serde(default)]` on a
    // bool is `false`.
    let src = read("crates/octo-whatsapp/src/config.rs");
    assert!(
        src.contains("#[serde(default = \"default_true\")]\n    pub bearer_required: bool"),
        "M-8: bearer_required must default to true, not to serde's bool default"
    );
    assert!(
        src.contains("bearer_required: true,"),
        "M-8: SecurityConfig::default() must set bearer_required: true"
    );
}

// ── M-17: the keystore must not take KDF parameters from the file ──────────

/// The part of a source file before its `#[cfg(test)]` module.
///
/// Guards that look for a production construct must not match the crate's own
/// regression test asserting on that construct — which is the third way a
/// substring scan goes vacuous, after comments and dead sibling functions.
/// Remove all whitespace, for matching a needle that may straddle a rustfmt
/// line break or spacing choice.
///
/// `split_whitespace().join(" ")` collapses runs to a SINGLE space; it does not
/// delete them, so a needle like `nonce <= last` still needs its spaces and
/// would break the moment rustfmt reformats the expression. Deleting
/// whitespace entirely makes these guards independent of formatting.
fn dense(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn production_region(src: &str) -> &str {
    match src.find("#[cfg(test)]") {
        Some(i) => &src[..i],
        None => src,
    }
}

#[test]
fn m17_keystore_import_does_not_read_file_supplied_kdf_params() {
    // The KDF ran before any passphrase check, so a hostile `memorycost` forced
    // a multi-gigabyte allocation from the file alone. Asserted on the source
    // because the failure mode is an OOM kill, not a catchable return value.
    //
    // Scoped to the production region: this crate's own regression test asserts
    // on the message `unsupported kdfparams`, so a whole-file `contains` is
    // satisfied by the test even when the production check is deleted.
    let raw = read("crates/octo-wallet/src/keystore.rs");
    let src = strip_comments(production_region(&raw));
    assert!(
        src.contains("unsupported kdfparams"),
        "M-17: import must reject a file whose declared params do not match \
         the compile-time constants"
    );
    assert!(
        src.contains("kdf.memory_cost != ARGON2_M_COST"),
        "M-17: the comparison against the file's declared params is missing"
    );
    // The KDF itself must be built from the constants, not from the file.
    let tail = src
        .split("Params::new")
        .nth(1)
        .unwrap_or_else(|| panic!("M-17: no Argon2 Params::new call in keystore.rs"));
    // Bound to the call's own argument list. An unbounded tail would run to
    // end-of-file and pick up unrelated `file.crypto` reads (e.g. the salt),
    // failing a check that is actually satisfied.
    let params: String = tail.chars().take(300).collect();
    assert!(
        params.contains("ARGON2_M_COST"),
        "M-17: Params::new must be fed from the compile-time constants, not the file"
    );
    assert!(
        !params.contains("file.crypto"),
        "M-17: Params::new is being fed from the untrusted file: {params}"
    );
}

// ── M-15: every adapter `canonicalize` must verify, not just parse ─────────

/// Collect every workspace adapter whose `canonicalize` impl lives in
/// `src/lib.rs`.
///
/// RULE: a canonicalization surface is any `octo-adapter-*` crate that defines
/// `fn canonicalize(`. Discovered from the filesystem rather than a hardcoded
/// list — the audit named 4 adapters for M-15 and there are 22, so a hand-
/// maintained manifest would have hidden 18 of them.
fn adapter_canonicalize_files() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = walk_rs(&workspace_root().join("crates"))
        .into_iter()
        .filter(|p| {
            p.to_string_lossy().contains("/octo-adapter-")
                && p.file_name().is_some_and(|f| f == "lib.rs")
                && std::fs::read_to_string(p)
                    .map(|s| s.contains("fn canonicalize("))
                    .unwrap_or(false)
        })
        .collect();
    found.sort();
    found
}

/// Every adapter that names a concrete algorithm at its call site must say
/// why, and name the owner.
///
/// Measured: all 24 adapter call sites pass `ed25519_algorithm_id()`
/// explicitly, and none passed anything else. That is a **known and
/// documented** limitation — the adapters have no peer registry yet, so they
/// cannot know which algorithm a peer's key speaks, and binding a per-peer
/// algorithm is CD-6's work.
///
/// What was not true is that the assumption was *declared* everywhere. The
/// site in `octo-adapter-p2p` says Ed25519 is named "so the call site reads
/// as an assumption rather than an accident" — and that held at **5 of 24**
/// sites. The other 19 hardcoded Ed25519 in silence, which is the accident
/// the one note warns against.
///
/// This pins the declaration, not the algorithm. CD-6 replacing the hardcoded
/// id is the fix; until then every site states the assumption out loud, so a
/// reader (or a grep) can find all 24 of them and none is a surprise.
///
/// The floor is measured, not invented: a guard that stops matching because
/// the discovery rule changed would otherwise pass vacuously.
#[test]
fn every_adapter_naming_a_concrete_algorithm_declares_the_assumption() {
    let files = adapter_canonicalize_files();
    assert!(
        files.len() >= 20,
        "this guard examined {} adapter files, expected 20 or more",
        files.len()
    );

    let mut sites = 0usize;
    let mut undeclared: Vec<String> = Vec::new();
    for file in &files {
        let raw = std::fs::read_to_string(file).unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("ed25519_algorithm_id()") {
                continue;
            }
            sites += 1;
            // The note may sit above the statement or above the statement's own
            // leading comments, so the window reaches a little further up.
            let lo = i.saturating_sub(10);
            let window = lines[lo..(i + 1).min(lines.len())].join("\n");
            // The note must point at CD-6's *registry* work specifically.
            // CD-6 D1 binds a key to a known peer; it does not specify an
            // algorithm binding — the word does not appear in that plan. A
            // note claiming CD-6 will deliver the algorithm would be a
            // pointer to work that does not exist, and would outlive its
            // usefulness by looking like an owned debt.
            if !window.contains("registry") && !window.contains("CD-6") {
                undeclared.push(format!(
                    "{} (line {})",
                    file.file_name().unwrap_or_default().to_string_lossy(),
                    i + 1
                ));
            }
        }
    }

    assert!(
        undeclared.is_empty(),
        "M-15: these adapter sites hardcode Ed25519 without declaring that it is \
         an assumption and naming the work that would retire it. Silence is how a \
         known limitation becomes an accident: state it at the call site, as the \
         note in octo-adapter-p2p already does. Undeclared: {undeclared:?}"
    );

    // 24 measured across 20 adapters today. When CD-6 lands and these become
    // real per-peer lookups this fails loudly, which is the point: the debt
    // being paid should be visible, not silently absorbed.
    assert!(
        sites >= 24,
        "the adapter sweep found only {sites} sites naming a concrete algorithm, \
         against a measured 24. If this fires the discovery rule changed."
    );
}

#[test]
fn m15_every_adapter_canonicalize_verifies_the_envelope() {
    let files = adapter_canonicalize_files();
    // The audit reported 4 for this finding; the real count is the point of
    // the guard, so it is asserted rather than assumed.
    assert!(
        files.len() >= 20,
        "M-15: expected ~20+ adapters to canonicalize, found {} — the \
         discovery rule has probably stopped matching",
        files.len()
    );

    for file in &files {
        let raw = std::fs::read_to_string(file).unwrap();
        let src = strip_comments(&raw);
        let body = fn_body(&src, "fn canonicalize");
        assert!(
            body.contains(".verify("),
            "M-15: {} canonicalizes without verifying. Parsing accepts a \
             structurally valid frame with a forged signature or a mismatched \
             envelope_id, so this is an unauthenticated injection point.",
            file.display()
        );
        // Verify must be against the envelope's own key, not a hardcoded one.
        // Matched on whitespace-collapsed text because rustfmt wraps the call.
        //
        // Since RFC-0009 AC#11 mission `0009-c7` the call takes the algorithm
        // as its FIRST argument, so the bare `.verify(&env.source_peer)` form no
        // longer matches.
        //
        // Each `.verify(` call is checked individually rather than asking
        // whether the body contains one good call. "At least one" is what an
        // earlier version did, and it is too weak: this function has two call
        // sites, so mutating the second to a hardcoded key still passed
        // because the first still matched. Verified by mutation.
        let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
        let verify_calls: Vec<String> = flat
            .match_indices(".verify(")
            .map(|(i, _)| {
                let rest = &flat[i..];
                // Scan to the matching close paren rather than the first one:
                // the argument list contains nested calls (`ed25519_algorithm_id()`)
                // whose own `)` would truncate the text and hide the key that
                // follows it. Depth counting is the whole fix.
                let bytes = rest.as_bytes();
                let mut depth = 0i32;
                let mut end = rest.len();
                for (j, b) in bytes.iter().enumerate() {
                    match b {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = j + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                rest[..end].to_string()
            })
            .collect();
        assert!(
            !verify_calls.is_empty(),
            "M-15: {} canonicalizes without verifying. Parsing accepts a \
             structurally valid frame with a forged signature or a mismatched \
             envelope_id, so this is an unauthenticated injection point.",
            file.display()
        );
        for call in &verify_calls {
            let own_key =
                call.contains("&env.source_peer") || call.contains("&envelope.source_peer");
            assert!(
                own_key,
                "M-15: {} has a `{call}` that does not use the envelope's own \
                 source_peer. A hardcoded key would make the check theatre.",
                file.display()
            );
            // The call must carry the algorithm the caller supplied, as its
            // first argument (RFC-0009 AC#11; the signature took that shape in
            // mission `0009-c7`).
            //
            // **This check exists because its absence cost fourteen adapters.**
            // When `0009-c7` moved the algorithm to first position, it stranded
            // every caller still passing the bare `.verify(&env.source_peer)`
            // form, and this guard was edited in the same change so the bare
            // form stopped matching. The gate went green over fourteen crates
            // that did not compile. A guard edited to accommodate a half-finished
            // migration is no longer a guard.
            //
            // The test is **arity, not naming**. Matching on an
            // `_algorithm_id()` spelling would be the third name-based measure
            // in this arc to produce a wrong number, and it would fail the moment
            // a fifth algorithm appears. What must hold is structural: the
            // envelope carries no algorithm, so anything preceding the key is
            // caller-supplied by construction, and a single-argument call is
            // precisely the pre-`0009-c7` shape.
            assert!(
                count_top_level_args(call) >= 2,
                "M-15: {} has a `{call}` with no algorithm argument. Since \
                 `0009-c7` the verifier dispatches on the algorithm the CALLER \
                 names, and the envelope carries none, so a one-argument call is \
                 the pre-dispatch shape — it verifies under whatever the \
                 implementation assumes rather than what the peer speaks.",
                file.display()
            );
        }
        // The verification must guard the value the function RETURNS. Adapters
        // with two decode paths (quic, p2p, webrtc: binary + DOT/1 text
        // fallback) funnel both through one trailing verify, so the rule is
        // "verify happens on the way out", not "one verify per decode site" —
        // the former is the stronger property, since a per-site check can be
        // forgotten when a third path is added.
        let last_verify = flat.rfind(".verify(").unwrap_or_else(|| {
            panic!(
                "M-15: {} has no verification to place on the return path",
                file.display()
            )
        });
        assert!(
            flat[last_verify..].contains("Ok("),
            "M-15: {} verifies but then returns something that is not the \
             verified value — the check does not guard delivery.",
            file.display()
        );
    }
}

/// Arguments at the top level of a call, counted with paren depth tracked.
///
/// Depth matters: `.verify(&alg_id(), &env.source_peer)` contains a comma
/// inside `alg_id()`'s own parentheses, and a naive `split(',')` reports two
/// arguments for the one-argument call `.verify(f(), &k)`. Counting only
/// depth-zero commas is what makes the arity check mean what it says.
fn count_top_level_args(call: &str) -> usize {
    let open = match call.find('(') {
        Some(i) => i,
        None => return 0,
    };
    let bytes = call.as_bytes();
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut any_content = false;
    for &b in &bytes[open..] {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => commas += 1,
            b' ' | b'\n' | b'\t' | b'\r' if depth == 1 => {}
            _ if depth == 1 => any_content = true,
            _ => {}
        }
    }
    if any_content {
        commas + 1
    } else {
        0
    }
}

#[test]
fn m15_the_telegram_adapter_is_a_documented_opt_out() {
    // `octo-adapter-telegram` defines `canonicalize` in `src/adapter.rs`, not
    // `src/lib.rs`, so the discovery rule above cannot see it. It verifies only
    // `if let Some(pk) = &self.verifying_key` — i.e. opt-in, the H-13 shape.
    // It is a non-workspace member (needs TDLib), so this guard pins the
    // recorded state rather than pretending the gap is closed.
    let p = workspace_root().join("crates/octo-adapter-telegram/src/adapter.rs");
    let Ok(raw) = std::fs::read_to_string(&p) else {
        return; // crate not present in this checkout
    };
    let src = strip_comments(&raw);
    let body: String = fn_body(&src, "fn canonicalize")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        body.contains(".verify("),
        "M-15: telegram no longer verifies at all — that is a regression, not \
         the recorded opt-in state."
    );
    assert!(
        body.contains("if let Some(pk)"),
        "M-15: telegram's verification changed shape. If it is now \
         unconditional, drop this guard; if it still skips when no key is \
         configured, update §Remediation wave 1 in the audit to say so."
    );
}

// ── M-16: a stub must not report a delivery ────────────────────────────────

#[test]
fn m16_bluetooth_stub_does_not_issue_a_delivery_receipt() {
    // The adapter has no radio link. Returning `Ok(DeliveryReceipt { .. })`
    // after queueing told callers a message was delivered, and those receipts
    // feed settlement and audit.
    let src = strip_comments(&read("crates/octo-adapter-bluetooth/src/lib.rs"));
    let body = fn_body(&src, "async fn send_message");
    assert!(
        !body.contains("Ok(DeliveryReceipt"),
        "M-16: the bluetooth stub issues a DeliveryReceipt without \
         transmitting anything"
    );
    assert!(
        body.contains("Err(") && body.contains("not transmitted"),
        "M-16: send_message must say the frame was queued, not transmitted"
    );
}

// ── M-13: the CDP trace must not be world-readable session material ────────

#[test]
fn m13_cdp_trace_is_private_and_defaults_outside_tmp() {
    let raw = read("crates/whatsapp_chrome_driver/src/main.rs");
    // Scoped to the production region: the regression test itself names the
    // old default in its assertion, which would otherwise satisfy this check.
    let src = strip_comments(production_region(&raw));
    assert!(
        !src.contains("/tmp/whatsapp_chrome_driver.jsonl"),
        "M-13: the trace default is a world-readable /tmp path. That file \\
         holds CDP request headers and WebSocket frames — the live session."
    );
    assert!(
        src.contains("opts.mode(0o600)"),
        "M-13: the trace file must be opened 0600"
    );
    assert!(
        src.contains("XDG_RUNTIME_DIR"),
        "M-13: the default trace parent should be the private runtime dir"
    );
}

// ── L-5 … L-11: the remaining small findings ───────────────────────────────

#[test]
fn l5_health_server_does_not_trust_client_forwarded_for() {
    // The label reached the auth-failure metric, and the header is set by the
    // client — so a security signal became a forgeable field.
    let src = strip_comments(production_region(&read(
        "crates/octo-whatsapp/src/observability/health_server.rs",
    )));
    assert!(
        !src.contains("x-forwarded-for"),
        "L-5: the health server reads X-Forwarded-For again. There is no \\
         socket peer available here (axum::serve has no ConnectInfo), so any \\
         use of the header trusts an unverifiable client claim."
    );
}

#[test]
fn l6_unix_socket_bind_refuses_to_delete_a_non_socket() {
    // `exists()` → `remove_file()` → `bind()` is a TOCTOU window, and it also
    // deleted whatever was at the path even if it was not a socket.
    let src = strip_comments(&read("crates/octo-whatsapp/src/ipc/server.rs"));
    let body = fn_body(&src, "pub fn bind");
    assert!(
        !body.contains("if path.exists()"),
        "L-6: bind() is back to an exists() check, which both races and cannot \\
         tell a socket from a regular file."
    );
    assert!(
        body.contains("symlink_metadata") && body.contains("is_socket"),
        "L-6: bind() must lstat the path and refuse anything that is not a \\
         socket."
    );
}

#[test]
fn l7_active_symlink_swap_is_atomic() {
    // `remove_file` then `symlink` lets a same-UID process create `active`
    // itself in the gap.
    let src = strip_comments(&read(
        "crates/octo-whatsapp-onboard-core/src/multi_account.rs",
    ));
    let body = fn_body(&src, "fn use_account");
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("staging") && flat.contains("rename("),
        "L-7: use_account must build the link at a staging path and rename it \\
         into place, so `active` is never absent or attacker-controlled."
    );
    assert!(
        !flat.contains("let_=fs::remove_file(&active)"),
        "L-7: use_account still removes `active` before creating the link, \\
         which is the race this finding is about."
    );
}

#[test]
fn l8_pairing_uri_is_withheld_unless_show_secret_is_set() {
    // The raw FIDO URI is the secret the phone is about to prove possession of,
    // and terminal output is routinely captured.
    let src = strip_comments(production_region(&read(
        "crates/octo-whatsapp-onboard/src/main.rs",
    )));
    let body = fn_body(&src, "fn build_passkey_authenticator");
    let flat = dense(body);
    assert!(
        flat.contains("show_secret"),
        "L-8: build_passkey_authenticator must take the --show-secret choice."
    );
    // The URI may only be printed inside a `show_secret` branch. Assert the
    // guard shape rather than a bare absence, so the flag cannot be honoured
    // by accident while the print is unconditional.
    assert!(
        flat.contains("ifshow_secret{") && flat.contains("waswithheld"),
        "L-8: the raw URI print must sit behind `if show_secret`, with the \\
         withheld case stating why."
    );
}

#[test]
fn l9_vault_uses_the_base64_crate_not_a_hand_rolled_parser() {
    // The hand-rolled decoder accepted `QQ=A`: it saw `chunk[2] == '='` and
    // skipped the `chunk[3]` check.
    let src = strip_comments(production_region(&read("crates/octo-wallet/src/vault.rs")));
    let body = fn_body(&src, "fn base64_decode");
    assert!(
        body.contains("STANDARD.decode(") || body.contains("STANDARD\n"),
        "L-9: base64_decode must delegate to base64::engine::general_purpose::STANDARD"
    );
    assert!(
        !body.contains("fn val("),
        "L-9: the bespoke base64 alphabet decoder is back."
    );
}

#[test]
fn l10_quic_drops_a_replayed_ping_nonce() {
    // The PONG used to echo whatever nonce arrived, with no memory of what was
    // already reflected, so a captured ping replayed unbounded.
    let raw = read("crates/octo-adapter-quic/src/lib.rs");
    let flat = dense(&strip_comments(raw_ping_region(&raw)));
    assert!(
        flat.contains("last_ping_nonce"),
        "L-10: the accept loop must track the highest ping nonce it reflected."
    );
    assert!(
        flat.contains("nonce<=last"),
        "L-10: a duplicate or regressed ping nonce must be dropped rather than \\
         answered."
    );
    // And the frame must actually be dropped, rather than the comparison being
    // computed and discarded.
    assert!(
        flat.contains("is_some_and") && flat.contains("droppingreplayedquicping"),
        "L-10: the replayed ping must be dropped, not merely detected."
    );
    // The mutation sweep found the weakness in the two assertions above: the
    // guard sees text, not reachability, so rewriting `if last_ping_nonce...`
    // as `if false` left every needle in place and the guard stayed green. Pin
    // the CONDITION so the branch cannot be neutered without the guard noticing.
    assert!(
        flat.contains("iflast_ping_nonce"),
        "L-10: the replay branch must be conditioned on `last_ping_nonce`, not \
         on a constant. The earlier form of this guard could not tell \
         `if last_ping_nonce...` from `if false`."
    );
}

/// Slice the QUIC accept loop: from its spawn anchor to the end of the
/// enclosing `start_server` body.
///
/// Anchoring on `FRAME_TYPE_PING` was wrong: the first occurrence after the
/// spawn anchor is not the receive branch, so the window missed the very
/// comparison it was looking for. Scoping to the enclosing function is stable
/// regardless of how the frame constants are referenced.
fn raw_ping_region(src: &str) -> &str {
    let anchor = src
        .find(QUIC_ACCEPT_LOOP_ANCHOR)
        .expect("the QUIC adapter must still spawn an accept loop");
    let enclosing = fn_body(src, "pub async fn start_server");
    let rel = src[anchor..]
        .find(enclosing)
        .unwrap_or(0)
        .min(enclosing.len().saturating_sub(1));
    &src[anchor..anchor + rel + enclosing.len() - rel]
}

#[test]
fn l11_transport_addr_len_uses_checked_arithmetic() {
    // `addr_len` is attacker-controlled and cast from u32, so on a 32-bit target
    // `cursor + addr_len` wraps and a "4 GiB address" passes the length check.
    let src = strip_comments(&read("crates/octo-runtime/src/handle/encoding.rs"));
    assert!(
        src.contains("checked_add"),
        "L-11: the addr_len bound must use checked_add so it fails closed on \\
         overflow instead of wrapping."
    );
    assert!(
        !src.contains("bytes.len() < cursor + addr_len"),
        "L-11: the unchecked `cursor + addr_len` comparison is back."
    );
}

// ── L-12: the macaroon id derivation is pinned, not fixed ─────────────────

/// Pin the CURRENT `macaroon_id` byte form.
///
/// L-12 wants `MACAR_ID_DOMAIN` prepended, which is a real doc/implementation
/// drift — but it cannot be changed as a patch. `StoolapSpendLedger` keys
/// persisted balances on this value, and issued capabilities embed it as
/// `channel_id`, so a change orphans stored rows and invalidates issued
/// tokens. The report's suggested "re-derive stored ids during migration" is
/// impossible: the stored value IS the truncated keyed hash.
///
/// So this guard does not assert the domain-separated form. It asserts that
/// whatever the current form is, it does not change silently — which is the
/// property that actually matters while the fix is pending an owner decision.
#[test]
fn l12_macaroon_id_derivation_is_pinned() {
    let src = strip_comments(&read("crates/octo-cap-macaroon/src/lib.rs"));
    let body = fn_body(&src, "pub fn macaroon_id");

    // Either domain form is acceptable; what is not acceptable is the id being
    // derived from something other than `root_secret` and `nonce`.
    //
    // Matched on the CALL, not on a bare identifier: the mutation sweep showed
    // that asserting `body.contains("root_secret")` is satisfied by the
    // function SIGNATURE, so replacing the key in the `hmac_blake3` call with a
    // zero key left the guard green. Dense matching is used because rustfmt owns
    // the spacing inside the call.
    let dense_body = dense(body);
    assert!(
        dense_body.contains("hmac_blake3(root_secret,nonce)"),
        "L-12: macaroon_id must be derived as hmac_blake3(root_secret, nonce). \
         Asserting the identifiers appear is not enough — the signature mentions \
         both regardless of what the call actually passes."
    );
    // The truncation length is a wire constant.
    assert!(
        body.contains("id.copy_from_slice(&mac[..16])"),
        "L-12: the 16-byte truncation changed. MacaroonId is a 16-byte wire \
         type; changing the truncation is a wire-format break."
    );

    // The drift is real and must stay visible, so the constant the doc claims
    // is used cannot be quietly deleted.
    assert!(
        src.contains("pub const MACAR_ID_DOMAIN"),
        "L-12: MACAR_ID_DOMAIN was deleted instead of the derivation being \
         reconciled. Deleting the constant hides the drift instead of fixing \
         it; the constant is still the chain's domain separator."
    );
}

// ── H-4: time caveats must be evaluated at redemption ──────────────────────

#[test]
fn h4_redemption_evaluates_time_caveats_before_anything_else() {
    // The finding was that `redeem_capability` had no clock at all, so an
    // EXPIRED capability redeemed. Behavioural coverage lives in
    // `octo-wallet/tests/redemption_subgraph.rs` (7 tests); this pins the
    // wiring, because deleting the call would leave those tests failing only
    // if someone also kept them — and a whole-gate deletion is silent.
    let src = strip_comments(&read("crates/octo-wallet/src/capability/redemption.rs"));

    let at = fn_body(&src, "pub fn redeem_capability_at_with");
    let dense_at = dense(at);
    assert!(
        dense_at.contains("time_caveats_admit(cap,evaluators,now_unix)?;"),
        "H-4: `redeem_capability_at_with` must evaluate time caveats before the \
         policy lookup, so an expired capability fails for the right reason."
    );

    // The clock-aware entry point must still route through it rather than
    // gaining a second, weaker time path of its own.
    let thin = dense(fn_body(&src, "pub fn redeem_capability_at("));
    assert!(
        thin.contains(
            "redeem_capability_at_with(cap,catalog,&CaveatEvaluatorRegistry::empty(),now_unix)"
        ),
        "H-4: `redeem_capability_at` must delegate to `redeem_capability_at_with` \
         with an empty registry, so there is one time-evaluation path."
    );

    // The clock-less entry point must refuse a time-bearing capability rather
    // than wave it through.
    let clockless = dense(fn_body(&src, "pub fn redeem_capability_with"));
    assert!(
        clockless.contains("time_caveat_bounds"),
        "H-4: `redeem_capability_with` must refuse a capability carrying a time \
         caveat. Without a `now` there is no honest answer to 'is this still \
         valid?', and delegating with now=0 would accept a window spanning 0."
    );
    assert!(
        clockless.contains("evaluators.resolve("),
        "H-4: the clock-less refusal must also cover a third-party caveat with a \
         registered evaluator. The core classifier cannot read a third-party \
         caveat, so skipping it here would reopen the hole the core pass closes."
    );
}

#[test]
fn h4_every_time_bearing_caveat_variant_is_covered() {
    // The classification of "which caveats are time-bearing" now lives in
    // `Caveat::time_bounds`, in the crate that DEFINES the variants, and its
    // match is exhaustive with no wildcard arm.
    //
    // That is strictly stronger than the hand-maintained list this guard used to
    // inspect. Before, a new time-bearing variant was silently unenforced —
    // which is not hypothetical: `Caveat::Before` was in the DSL, serialized,
    // subsumed and displayed, but missing from the guard's list, so a
    // capability whose only deadline was `Before` redeemed indefinitely.
    // Exhaustiveness turns that silent skip into a compile error.
    let defs = read("crates/octo-cap-macaroon/src/caveat/mod.rs");
    let def_src = strip_comments(&defs);
    let body = fn_body(&def_src, "fn time_bounds");
    let dense_body = dense(body);

    assert!(
        dense_body.contains("Self::Before")
            && dense_body.contains("Self::ValidRange")
            && dense_body.contains("Self::ValidAfter"),
        "H-4: the time-bearing classification must handle Before, ValidRange and ValidAfter"
    );
    assert!(
        !dense_body.contains("_=>"),
        "H-4: `time_bounds` must have no wildcard arm — a wildcard is precisely \
         what lets a new time-bearing variant be silently unenforced"
    );
    // Matched on the full signature, not the bare name: unit tests named
    // `fn time_bounds_*` in the same file would otherwise inflate the count.
    assert_eq!(
        defs.matches("fn time_bounds(&self)").count(),
        1,
        "H-4: `time_bounds` must be defined exactly once, in the crate that owns the variants"
    );

    // The wallet must consume that one definition rather than keeping its own
    // list, so the refusal and the evaluation cannot drift apart.
    let raw = read("crates/octo-wallet/src/capability/redemption.rs");
    assert!(
        raw.contains("caveat.time_bounds()"),
        "H-4: the wallet must classify time bounds through `Caveat::time_bounds`, \
         not a hand-maintained list"
    );
    assert!(
        raw.matches("fn time_caveat_bounds").count() == 1,
        "H-4: `time_caveat_bounds` must be defined exactly once, so the \
         refusal and the evaluation cannot drift apart"
    );
    assert!(
        raw.contains("time_caveats_admit(cap, evaluators, now_unix)?"),
        "H-4: the clock-aware entry point must delegate to the shared evaluator"
    );
    // The extension pass must exist, or a third-party expiry caveat works in
    // the registry and is then ignored by redemption.
    assert!(
        raw.contains("evaluate_registered("),
        "H-4: redemption must consult registered third-party evaluators, or an \
         extension caveat that carries a deadline redeems unevaluated"
    );
}

// ── C-1: the stub verifier feature must never reach a production build ───────
//
// `quota-router-core` carried `zk-verifier = { ..., features =
// ["allow-stub-verifier"] }` on its production `[dependencies]` edge. Cargo
// unifies feature flags across the whole dependency graph, so that one line
// put the forgeable BLAKE3 stub verifier into every `cargo build --workspace`
// — the `Err(VerifyError::StubDisabled)` fail-closed arm never compiled in
// any real binary.
//
// RULE: `allow-stub-verifier` may appear in a `Cargo.toml` only under
//   (a) `[dev-dependencies]` — dev-dep features do not reach a release build,
//   (b) `[features]` — the declaration/passthrough itself, never inside
//       `default = [...]`.
// Anything else is a production edge and fails the guard.
//
// `#` comments are stripped before scanning: several manifests explain the
// flag in prose, and matching prose would make this guard useless.

/// Strip `#` comments from TOML text, returning (section, line) pairs.
///
/// RULE: a section header is a line whose first non-space char is `[` and
/// which ends in `]`; everything up to the first `#` is then discarded.
fn toml_section_lines(src: &str) -> Vec<(String, String)> {
    let mut section = String::new();
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_owned();
            continue;
        }
        out.push((section.clone(), line.to_owned()));
    }
    out
}

fn cargo_manifests() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(workspace_root().join("crates"))
        .expect("crates/ must be readable")
        .flatten()
    {
        let manifest = entry.path().join("Cargo.toml");
        if manifest.is_file() {
            out.push(manifest);
        }
    }
    out.sort();
    out
}

#[test]
fn c1_stub_verifier_feature_is_dev_only() {
    let manifests = cargo_manifests();
    assert!(
        !manifests.is_empty(),
        "the manifest scan must find at least one crate, else it passes vacuously"
    );

    let mut violations = Vec::new();
    for manifest in &manifests {
        let rel = manifest
            .strip_prefix(workspace_root())
            .unwrap_or(manifest)
            .display()
            .to_string();
        for (section, line) in toml_section_lines(&read(&rel)) {
            if !line.contains("allow-stub-verifier") {
                continue;
            }
            match section.as_str() {
                "dev-dependencies" => {}
                "features" => {
                    // A passthrough under `[features]` is opt-in only. If it
                    // ever lands in `default = [...]` every plain build
                    // inherits it, which is the defect all over again.
                    let is_default_array = line.starts_with("default") && line.contains('[');
                    if is_default_array {
                        violations.push(format!("{rel}: [{section}] {line}"));
                    }
                }
                _ => violations.push(format!("{rel}: [{section}] {line}")),
            }
        }
    }

    assert!(
        violations.is_empty(),
        "C-1: `allow-stub-verifier` reached a production dependency edge. Cargo \
         unifies features across the graph, so this makes the forgeable stub \
         verifier live in every workspace build and kills the fail-closed arm:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn c1_the_fail_closed_arm_still_exists() {
    // The gate is only real if rejecting is still the default outcome. Pin the
    // cfg that selects it, and the returned value inside the gated arm — not
    // merely the string `StubDisabled`, which also appears in doc comments and
    // in the enum variant's `#[error(...)]` text.
    let src = strip_comments(&read("crates/zk-verifier/src/lib.rs"));
    assert!(
        src.contains("#[cfg(not(any(test, feature = \"allow-stub-verifier\")))]"),
        "C-1: zk-verifier must keep a cfg-gated arm that fails closed"
    );

    let verify = fn_body(&src, "pub fn verify_capability_zk(");
    let dense_verify = dense(verify);
    let gated_at = dense_verify
        .find("#[cfg(not(any(test,feature=\"allow-stub-verifier\")))]")
        .unwrap_or_else(|| {
            panic!("C-1: verify_capability_zk must gate a fail-closed arm; body was:\n{verify}")
        });
    let tail = &dense_verify[gated_at..];
    assert!(
        tail.contains("Err(VerifyError::StubDisabled)"),
        "C-1: the cfg-disabled arm must return StubDisabled, never Ok(())"
    );
    assert!(
        !tail.contains("Ok(())"),
        "C-1: nothing after the fail-closed gate may return Ok(()); a stub-shaped \
         proof must never verify when the feature is off"
    );
}

// ── M-4: the FFI library path is a trust boundary ───────────────────────────
//
// After the C-1 fix the `stwo-sys` object is the ONLY sound verification path
// left, so an attacker-controllable path to it is both RCE and a way to
// install a `stwo_verify` that returns 0 for everything. The behavioural rules
// live in `crates/zk-vendor/tests/library_path.rs`; what is pinned here is the
// wiring, because a validation helper that nothing calls passes its own tests.

#[test]
fn m4_loaded_library_validates_before_dlopen() {
    let src = strip_comments(&read("crates/zk-vendor/src/lib.rs"));
    let body = fn_body(&src, "pub fn loaded_library()");
    let dense_body = dense(body);

    let validate_at = dense_body
        .find("validate_library_path(")
        .unwrap_or_else(|| {
            panic!("M-4: loaded_library must call validate_library_path; body was:\n{body}")
        });
    let load_at = dense_body.find("try_load(").unwrap_or_else(|| {
        panic!("M-4: loaded_library must still call try_load; body was:\n{body}")
    });

    assert!(
        validate_at < load_at,
        "M-4: validate_library_path must run BEFORE try_load — checking after the \
         dlopen is not a check (validate_at={validate_at}, load_at={load_at})"
    );
    assert!(
        dense_body.contains("returnNone;"),
        "M-4: a validation failure must return None (treat the library as absent), \
         not fall through to try_load"
    );
}

#[test]
fn m4_ffi_calls_are_serialized() {
    // The FFI contract says the entry points are not thread-safe with respect
    // to a ProofHandle; a bare `unsafe impl Send + Sync` contradicted it.
    let src = strip_comments(&read("crates/zk-vendor/src/lib.rs"));
    assert!(
        src.contains("call_lock"),
        "M-4: StwoSys must carry the call_lock that enforces the FFI threading contract"
    );
    for (name, marker) in [
        ("verify", "let_guard=self.ffi_call();"),
        ("prove", "let_guard=self.ffi_call();"),
        ("version", "let_guard=self.ffi_call();"),
    ] {
        let body = fn_body(&src, &format!("pub fn {name}("));
        assert!(
            dense(body).contains(marker),
            "M-4: {name} must take the FFI call lock before calling into the library"
        );
    }
}

// ── M-5: the stub commitment must cover its whole input type ─────────────────
//
// See the note in `crates/zk-verifier/src/lib.rs`: the five router-side fields
// never reach this layer, so binding them is CD-1 phase 2 work. The bounded,
// enforceable property is that `canonicalize_public` covers every field it IS
// given — asserted behaviourally in zk-verifier's own tests, because a source
// scan cannot tell whether a field is actually folded into the transcript.

// ── M-11: the session-swap script must not trust its base directory ─────────
//
// `swap_sessions.sh` renames `default.session.db` <-> `bak_main_phone.session.db`
// under `$OCTO_WHATSAPP_PERSIST_DIR`, so whoever controls that variable chooses
// which directory the renames happen in. The audit calls this "renames
// attacker-chosen `*.session.db` paths"; that overstates it — the filenames are
// hardcoded, so the primitive is "renames two fixed names wherever you point",
// which still matters when the script runs as root from a supervisor with a
// writable environment.
//
// The guards are behavioural and run the script, because the property being
// asserted is that the *script refuses*, which no Rust unit test can reach.

/// Build a persist dir that passes pre-flight, with the given mode.
fn make_persist_dir(label: &str, mode: u32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "m11-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for name in ["default", "bak_main_phone"] {
        std::fs::create_dir_all(dir.join(format!("{name}.session.db")))
            .expect("create session dir");
        std::fs::write(
            dir.join(format!("{name}.session.db.meta.json")),
            format!("{{\"phone\":\"{name}\"}}"),
        )
        .expect("write meta");
    }
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(mode))
        .expect("chmod persist dir");
    dir
}

/// Run `swap_sessions.sh` against `persist_dir`. Returns (exit code, output).
fn run_swap(persist_dir: &Path) -> (i32, String) {
    let out = Command::new("bash")
        .arg(workspace_root().join("scripts/swap_sessions.sh"))
        .env("OCTO_WHATSAPP_PERSIST_DIR", persist_dir)
        .output()
        .unwrap_or_else(|e| panic!("swap_sessions.sh must be runnable: {e}"));
    let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stdout));
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn m11_swap_sessions_refuses_an_untrusted_persist_dir() {
    // A stock 0755 directory is the operator's normal case and MUST still work;
    // the two writable cases are the ones that must be refused.
    let trusted = make_persist_dir("trusted", 0o755);
    let (code, text) = run_swap(&trusted);
    assert_eq!(
        code, 0,
        "M-11: a normal 0755 persist dir must still be accepted (got {code}: {text})"
    );
    let _ = std::fs::remove_dir_all(&trusted);

    for (label, mode) in [("world-writable", 0o777u32), ("group-writable", 0o775)] {
        let dir = make_persist_dir(label, mode);
        let (code, text) = run_swap(&dir);
        assert_ne!(
            code, 0,
            "M-11: a {label} persist dir was accepted (output: {text})"
        );
        assert!(
            text.contains("group- or world-writable"),
            "M-11: {label} should be refused with an explicit diagnostic, got: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn m11_swap_sessions_refuses_a_relative_or_symlinked_persist_dir() {
    let (code, text) = Command::new("bash")
        .arg(workspace_root().join("scripts/swap_sessions.sh"))
        .env("OCTO_WHATSAPP_PERSIST_DIR", "relative/persist/dir")
        .output()
        .map(|out| {
            (
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            )
        })
        .expect("run");
    assert_ne!(code, 0, "M-11: a relative persist dir was accepted");
    assert!(
        text.contains("must be an absolute path"),
        "M-11: relative path should be refused explicitly, got: {text}"
    );

    let real = make_persist_dir("symlink-target", 0o755);
    let link = std::env::temp_dir().join(format!("m11-link-{}", std::process::id()));
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&real, &link).expect("create symlink");
    let (code, text) = run_swap(&link);
    assert_ne!(
        code, 0,
        "M-11: a symlinked persist dir was accepted (output: {text})"
    );
    assert!(
        text.contains("must not be a symlink"),
        "M-11: symlink should be refused explicitly, got: {text}"
    );
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_dir_all(&real);
}

#[test]
fn m11_swap_sessions_verifies_that_identity_actually_moved() {
    // Existence checks pass on a mis-ordered rename. The two distinguishing
    // facts are that the meta.json contents followed their directories, and
    // that the script reported the inode comparison rather than silently
    // skipping it.
    let dir = make_persist_dir("identity", 0o700);
    let meta = |n: &str| dir.join(format!("{n}.session.db.meta.json"));
    let read = |p: &PathBuf| std::fs::read_to_string(p).unwrap_or_default();

    assert_eq!(read(&meta("default")), "{\"phone\":\"default\"}");
    assert_eq!(
        read(&meta("bak_main_phone")),
        "{\"phone\":\"bak_main_phone\"}"
    );

    let (code, text) = run_swap(&dir);
    assert_eq!(
        code, 0,
        "M-11: the swap itself must succeed (got {code}: {text})"
    );

    assert_eq!(
        read(&meta("default")),
        "{\"phone\":\"bak_main_phone\"}",
        "M-11: after the swap, `default` must hold what `bak_main_phone` held"
    );
    assert_eq!(
        read(&meta("bak_main_phone")),
        "{\"phone\":\"default\"}",
        "M-11: after the swap, `bak_main_phone` must hold what `default` held"
    );
    assert!(
        text.contains("identity verified"),
        "M-11: the post-swap inode comparison must actually run, got: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── H-6: WhatsApp session dumps must never be world-readable ────────────────
//
// The Chrome-session binaries read a profile that is logged in to WhatsApp Web
// and write out Noise private keys, `document.cookie`, localStorage `crypto`
// entries and captured AES import keys. Every one of those writes used to be a
// bare `std::fs::write`, which takes its mode from the umask — 0644 at the
// conventional 022 — into `/tmp/wa-observer` and `/tmp/idb-diff`. On a
// multi-user host any local account could then copy a live session without
// touching the profile at all.
//
// The behavioural rules live in the crate's own tests (the file really is
// 0600). What is pinned here is coverage: a *new* dump binary, or a new write
// inside an existing one, must not quietly reintroduce a umask-derived mode.

/// RULE: every Rust source in this crate that is not the shared helper module
/// must write files through `write_private_file` / `write_private_json`.
const WA_SESSION_CRATE: &str = "crates/whatsapp_chrome_session_extract/src";

#[test]
fn h6_no_session_dump_is_written_with_a_umask_derived_mode() {
    let root = workspace_root().join(WA_SESSION_CRATE);
    let mut offenders = Vec::new();
    let mut checked = 0usize;

    for entry in walk_rs(&root) {
        let rel = entry
            .strip_prefix(workspace_root())
            .unwrap_or(&entry)
            .display()
            .to_string();
        // The helper module itself contains no `fs::write`; it defines the
        // private writer. Excluding it keeps the rule about *callers*.
        if rel.ends_with("src/lib.rs") {
            continue;
        }
        let src = strip_comments(&read(&rel));
        checked += 1;
        let dense_src = dense(&src);
        for needle in ["std::fs::write(", "fs::write(&", "fs::write("] {
            if dense_src.contains(&dense(needle)) {
                offenders.push(format!("{rel}: contains `{needle}`"));
            }
        }
        // `File::create` is the other umask-respecting way to make a file.
        if dense_src.contains(&dense("File::create(")) {
            offenders.push(format!("{rel}: uses File::create (umask-derived mode)"));
        }
    }

    assert!(
        checked > 0,
        "the scan must find at least one dump source, else it passes vacuously"
    );
    assert!(
        offenders.is_empty(),
        "H-6: session material must be written through `write_private_file` / \
         `write_private_json` (0600 under a 0700 dir), never at umask 0644:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn h6_the_private_writer_is_shared_not_duplicated() {
    // Six binaries writing the same rule six ways is how this regressed in the
    // first place. There must be exactly one definition.
    let src = read("crates/whatsapp_chrome_session_extract/src/lib.rs");
    assert_eq!(
        src.matches("pub fn write_private_file(").count(),
        1,
        "write_private_file must be defined exactly once, in the crate's shared helper"
    );
    // No trailing `(`: `write_private_json` is generic, so its definition
    // reads `pub fn write_private_json<T: serde::Serialize>(...)`.
    assert_eq!(
        src.matches("pub fn write_private_json").count(),
        1,
        "write_private_json must be defined exactly once, in the crate's shared helper"
    );
    assert!(
        src.contains("0o600"),
        "the shared writer must request owner-only mode explicitly"
    );
}

// ── H-7: the session export bundle must not be world-readable ────────────────
//
// `export_bundle` writes a gzip of the session DB — which carries the device
// Noise private key — with `fs::write`, so it was born 0644 under the
// conventional umask and every local account could read it. The import side
// wrote the same material back out the same way. Encryption of the bundle is
// still open (CD-5 D1); this is the permissions half only.

#[test]
fn h7_the_session_export_is_not_written_with_a_umask_derived_mode() {
    let raw = read("crates/octo-whatsapp-onboard-core/src/multi_account.rs");
    // Scoped to the production region: the `#[cfg(test)]` module below seeds
    // fixtures with `fs::write`, which is not a disclosure — a test writing
    // fake bytes into its own tempdir has no security surface.
    let dense_src = dense(production_region(&strip_comments(&raw)));

    assert!(
        !dense_src.contains(&dense("fs::write(")),
        "H-7: multi_account.rs must not use fs::write — it takes its mode from the \
         umask. Session material (export bundle, imported session.db, config, \
         sidecar, index) must go through write_private_bytes."
    );
    assert_eq!(
        raw.matches("fn write_private_bytes(").count(),
        1,
        "the private writer must be defined exactly once in this module"
    );
    assert!(
        dense_src.contains(&dense("cipherocto_fs_perms::create_private(")),
        "H-7: write_private_bytes must delegate to the shared primitive, not \
         re-implement the mode itself"
    );
}

/// The primitive has exactly one definition in the workspace.
///
/// An earlier revision of the CDP dump path carried its own private copy of
/// this rule. That is the parallel-abstraction failure `cipherocto-fs-perms`
/// exists to prevent: the same security rule in two places, only one of which
/// gets fixed when the assumption underneath it changes.
#[test]
fn h7_private_write_primitives_have_exactly_one_definition() {
    // Deliberately NOT a within-crate occurrence count: each primitive has
    // TWO definitions by design, the `cfg(unix)` and `cfg(not(unix))`
    // platform arms with identical signatures. An "exactly once" assertion
    // cannot tell that apart from a real duplicate, and would have failed
    // here for the wrong reason. The property that matters is that no OTHER
    // crate defines it — which is what the cross-crate scan below checks.
    assert!(
        read("crates/cipherocto-fs-perms/src/lib.rs").contains("pub fn create_private("),
        "cipherocto-fs-perms must own the canonical create_private"
    );

    // Any other crate must CONSUME the primitive, never redefine it.
    let mut redefiners: Vec<String> = Vec::new();
    for rel in [
        "crates/octo-wallet/src/fs_perms.rs",
        "crates/whatsapp_chrome_session_extract/src/lib.rs",
        "crates/octo-whatsapp-onboard-core/src/multi_account.rs",
    ] {
        let src = strip_comments(&read(rel));
        let defining = src.contains("pub fn create_private(")
            || src.contains("pub fn create_dir_private(")
            || src.contains("fn create_private(")
            || src.contains("fn create_dir_private(");
        if defining && !rel.contains("cipherocto-fs-perms") {
            redefiners.push(rel.to_owned());
        }
    }
    assert!(
        redefiners.is_empty(),
        "H-7: the private-write primitives must have one canonical definition \
         (crates/cipherocto-fs-perms). These files redefine them: {redefiners:?}"
    );

    // And the wallet must be re-exporting, not reimplementing.
    let wallet = strip_comments(&read("crates/octo-wallet/src/fs_perms.rs"));
    assert!(
        wallet.contains("pub use cipherocto_fs_perms::"),
        "H-7: octo-wallet::fs_perms must re-export the shared primitives"
    );
}

// ── AC-11: no verification site hardcodes an algorithm ────────────────────

/// Crates this vector covers. Each has completed its dispatch work; the
/// two that have not are handled by the explicit allowlist below, which is
/// the point of listing them rather than omitting them.
const AC11_DISPATCHED: &[&str] = &[
    "crates/octo-wallet/src",
    "crates/octo-reputation/src",
    "crates/quota-router-sm-engine/src",
    "crates/octo-runtime/src",
    "crates/octo-wallet-node/src",
    // Mission 0009-c2: sites that needed a new registry dependency edge.
    "crates/quota-router-storage/src",
    "crates/octo-settlement-core/src",
    // Mission 0009-cb: the crate's last inline constructor was the
    // `VerifyingKey::from_bytes` in `dot/dc.rs`, which existed only because
    // `verify` took a key *type* rather than key bytes. Taking bytes removed
    // it, and with 16 zero-count pending entries retired the whole crate
    // belongs here: under the inline-rebuild scan, which is a stronger check
    // than a count, since it catches a site whatever the total is.
    "crates/octo-network/src",
    // Mission 0009-ca: the last allowlisted file, `authorization.rs`, was
    // dispatched and its entry removed, so the crate no longer needs to be
    // scanned by hand below. RFC-0871's authorization verification is under
    // this scan now, not excused from it.
    "crates/octo-protocol/src",
    // Found by `ac11_every_crate_that_verifies_a_signature_is_scanned_or_explained`,
    // not by a failing site. `verify_holder_sig` dispatches on the algorithm
    // recorded beside the signature and was never read by any gate — the crate
    // was clean by review, not by construction. Its `HolderSignature` is the
    // reference instance of this pattern: `Vec<u8>` bytes, a slice-returning
    // `to_bytes`, and a constructor that checks the length against the
    // *registered algorithm's* `signature_len()` rather than a hardcoded 64.
    "crates/octo-cap-macaroon/src",
];

/// Crates that **have** un-dispatched AC#11 sites and are known to.
///
/// Without this list the gate's silence about a crate is indistinguishable
/// from its cleanliness: the scan walks a fixed set of directories, so a
/// crate that is simply absent cannot fail it. `octo-network` sat in no list
/// at all — not dispatched, not excused — while carrying 14 sites. Proved by
/// planting a `VerifyingKey::from_bytes` in `dom/admission.rs`: all four
/// AC#11 vectors stayed green.
///
/// So this is the honest middle state between "dispatched" and "out of
/// scope": the sites are acknowledged, counted, and owned by a mission. The
/// counts are what make it a guard rather than a comment — `ac11_pending`
/// asserts each file still has exactly the number of sites listed here, so a
/// **new** site turns the gate red and a **removed** one does too. That
/// second direction is the point: dispatching a site without updating this
/// table is a failure, which is what stops the table outliving its blocker
/// the way an allowlist entry did.
///
/// The table is expected to shrink. It is recorded here rather than left to
/// memory because the whole failure mode was that nobody was holding the
/// number.
///
/// ## The blocker is NOT uniformly the DID codec, and saying so was wrong
///
/// This table originally carried one reason for every `octo-network` entry:
/// "blocked on the RFC-0010 DID codec". That was inherited from the sites
/// where it is true, and pasted onto sites where it is not — the same error
/// that made two sibling entries false blockers (`0009-c5`, `0009-c6`).
///
/// Measured 2026-10-10 by reading each site, the ten files split three ways:
///
/// - **Caller-supplied key** — `porelay/*`, `drs/route`, `dot/envelope`,
///   `gossip/reputation`, `GatewayAttestation::verify_signature`. The key
///   arrives as a `&[u8; 32]` parameter. No DID is parsed, so the codec is
///   not in the path. Dispatchable by adding an `AlgorithmId` parameter,
///   exactly as `octo-vault` was.
/// - **Key is a field on the wire struct** — `KeyRotation`, `dom/admission`,
///   `dot/dc`. The key travels inside the message
///   (`self.old_public_key`, `intent.sender_id`, `witness_id`).
/// - **Actually DID-derived** — none of these ten. The only
///   `pubkey_from_did` callers in the crate are `dc/admin_attest.rs` and
///   `octo-role`, neither of which appears in this table.
///
/// ### The second category's blocker was wrong too
///
/// The wire-field group was recorded as "carrying an algorithm here means a
/// **wire format change** under RFC-0853 — a different blocker under a
/// different RFC". That is the same inherited-blocker error one level down:
/// it presumes the algorithm must travel with the key. `verify_dispatch` takes
/// `public_key: &[u8]`, so nothing forces that, and the caller can supply the
/// algorithm for every one of these sites. All three dispatched in `0009-c9`.
///
/// The wire field would also have been the *worse* choice, not the blocked
/// one. In all three the message arrives from the party being verified, so an
/// algorithm read off the wire is chosen by the party choosing its own
/// verifier. Resolving it from the caller's registry is stricter.
///
/// And the premise itself did not survive being checked:
/// `RevocationNotice` was listed as wire-field, but it carries
/// `compromised_key_id` and `successor_key_id` — *identifiers*. Its verifying
/// key arrived as a parameter, making it a caller-supplied site all along.
///
/// The blanket reason also hid a second problem: `octo-network` does **not**
/// depend on `octo-ident` at all. `octo-ident` appears in its manifest only
/// inside a comment describing a future codec swap. So every dispatch here
/// needs a new dependency edge, which is the actual work in the first
/// category — not a codec that already landed.
const AC11_PENDING: &[(&str, usize, &str)] = &[
    (
        "crates/octo-coordinator-types/src/slashing.rs",
        0,
        "mission 0009-c6: DISPATCHED. Count of zero rather than removed, so a \
         future site here fails the gate instead of opening a new silent gap. \
         Second instance of the vault finding: the codec did not block it \
         either, because the governance key set is supplied by the caller and \
         no DID is parsed on the way to the key.",
    ),
    (
        "crates/octo-vault/src/cache_subscriber.rs",
        0,
        "mission 0009-c5: DISPATCHED. Recorded with a count of zero rather \
         than removed, so that adding a site here is a gate failure instead of \
         a silent new gap. This was the one pending site the RFC-0010 DID \
         codec did not actually block — the trust list receives its keys from \
         the caller rather than parsing them out of a DID string.",
    ),
    (
        "crates/quota-router-core/src/receipt.rs",
        4,
        "CORRECTED, because the previous text here was wrong in two \
         checkable ways. It said 'the only production one is \
         wrap_receipt_envelope'. There are THREE production sites, and only \
         two of them are placeholders. Measured: line 128 seeds \
         signature with an all-zero value for sign_envelope to overwrite, \
         line 196 does the same for router_sig, and neither is verified \
         anywhere. Line 89 is a different shape entirely and was \
         misdescribed as a placeholder: it is the serde deserialize shim, \
         which rebuilds a Signature from attacker-supplied wire bytes with \
         no algorithm involved at all. That is the shape a verifier will \
         eventually consume, and the module doc says replay nodes are meant \
         to verify this envelope. No such verifier exists in the workspace \
         yet, which is why AC#11's verification-site clause is not literally \
         triggered today. A serde impl has no caller to supply an algorithm \
         to, so it cannot be dispatched the way the other sites were; what \
         it can do is state the shape honestly so the verifier that lands \
         does not assume the signature already names its algorithm. The \
         fourth site is a test. Counted 3 to 4 when the pending count and \
         the completeness scan were merged onto one matcher in 0009-c8: the \
         old two-constructor count could not see from_slice at all. Pending \
         rather than allowlisted only because the crate is not on the \
         scanned list.",
    ),
];

/// Crates deliberately **outside** AC#11, with the reason each one is.
///
/// AC#11 reaches "holder- or capability-signature verification sites". A
/// third-party protocol's fixed key format is a different question: nostr
/// (NIP-01) specifies Ed25519 and offers no alternative, so there is
/// nothing to select and routing it through `AlgorithmRegistry` would be a
/// semantic error rather than dispatch. The registry describes algorithms
/// an *identity document* may declare; a nostr event has no identity
/// document behind it.
///
/// Listed rather than omitted, so that adding a *genuine* holder
/// signature to this crate later is a deliberate act someone has to notice.
///
/// ### `crates/octo-governance/src` is deliberately absent
///
/// It was carried for a long time as "blocked on
/// `EnvelopeSigner::algorithm_id`, which has no carrier until the
/// `governance_envelopes` table lands". That constraint is real and still is:
/// both production call sites bind the signature to `_` and discard it.
///
/// But it was filed against the wrong criterion, and putting it in this table
/// would have cemented the mistake — an entry here has to *have* a site to be
/// excused, and `ac11_every_out_of_scope_crate_is_justified_and_disjoint`
/// correctly refuses one that does not. The crate verifies nothing.
///
/// Measured over `crates/octo-governance/src`: no `verify_dispatch`, no
/// `verify` call, no `VerifyingKey`, no inline key construction. AC#11 governs
/// verification sites. A crate that only signs has nothing here to dispatch,
/// which is why it appears in no table at all.
///
/// Recorded because "absent" and "not yet examined" look identical from the
/// outside. An algorithm stamp on the signing trait is a real question and it
/// belongs to RFC-0862's envelope design — but it is not an unmet AC#11
/// obligation, and a future reader should not re-derive that from scratch.
const AC11_OUT_OF_SCOPE: &[(&str, &str)] = &[(
    "crates/octo-adapter-nostr/src",
    "verifies a nostr event signature under NIP-01, which mandates ed25519. \
     Not a holder or capability signature: no CipherOcto identity document \
     is involved and no algorithm is selectable. A new holder-signature \
     site in this crate IS in scope and must be dispatched.",
)];

/// Inline-constructor occurrences that are still legitimate, each with the
/// reason. An allowlist entry without a reason is
/// indistinguishable from the defect it hides, which is the failure this
/// vector exists to prevent.
///
/// This list is now empty, and that is the state the vector enforces.
///
/// ### `crates/quota-router-sm-engine/src/envelope.rs` was here and is not any more
///
/// Its entry excused `unsigned_canonical_ser` cloning the envelope to blank the
/// signature field, so the signed message excludes it — and the blanking was
/// `ed25519_dalek::Signature::from_bytes(&[0u8; 64])`, an inline constructor of
/// exactly the kind this vector scans for.
///
/// Widening `ExecutionEnvelope::signature` to `Vec<u8>` removed both problems at
/// once: the field no longer names an algorithm, and the blanking became
/// `vec![0u8; 64]`, which constructs no key material. The all-zero placeholder
/// the old entry described is still there and still carries no cryptographic
/// meaning — but it is now a byte vector rather than a reconstructed Ed25519
/// type, so there is no algorithm left for the scan to be confused about.
///
/// The entry was **removed**, not re-justified. An entry that outlives its site
/// exempts the file for no reason, which is a silent hole rather than a
/// documented one.
///
/// ### `crates/octo-protocol/src/authorization.rs` was here and is not any more
///
/// Its entry said the DID payload is a bare 32 bytes "so the dispatcher has
/// nothing to dispatch on", and cited the `0009-c3` DID-codec blocker — which
/// `13ced5e7` had already disproved when it was still cited.
///
/// The premise was true and the conclusion was not. `verify_dispatch` takes
/// `public_key: &[u8]`, so nothing forces the algorithm to travel with the key;
/// the caller supplies it, as at every other site in this arc. `0009-ca`
/// dispatched it and the entry was **removed**, not re-justified.
///
/// Removing it is the point. An allowlist entry exempts the whole file from
/// the scan, so this was not one dark site — it was RFC-0871 §Adversary
/// Analysis A6's enforcement point for every signed envelope, unscanned, and
/// a new inline site added beside it would not have been seen. An entry that
/// outlives its site is how a file goes dark in the first place.
///
/// The list is empty. The last entry described `unsigned_canonical_ser`
/// blanking an envelope signature field, and stopped being true when
/// `ExecutionEnvelope::signature` became bytes.
const AC11_ALLOWED: &[(&str, &str)] = &[];

/// Crates that **are** signature algorithms rather than consumers of one.
///
/// They appear in `verify_dispatch` because that function is theirs — it is
/// defined there, and their own tests exercise it. They are not AC#11 sites,
/// because a dispatch question only exists where a *caller* chooses the
/// algorithm. Listing them is what lets the coverage test below tell "this
/// crate is deliberately not scanned" apart from "this crate was never
/// looked at", which are otherwise the same silence.
const AC11_ALGORITHM_CRATES: &[(&str, &str)] = &[
    (
        "crates/octo-algo-ed25519/src",
        "Defines `verify_dispatch` and self-tests it. The one crate in the \
         workspace where naming Ed25519 is the whole point.",
    ),
    (
        "crates/octo-algo-stark-schnorr/src",
        "Same, for the second registered algorithm. Its existence is what \
         makes a one-algorithm assumption observable at a call site.",
    ),
];

/// **Named by RFC-0009 AC#11.** The criterion names this exact test as its
/// gate, and it was implemented under a different name. An auditor running
/// the RFC's stated command would have found no such test and had no way to
/// tell "not verified" from "verified under another name". The
/// implementation was correct throughout; only the name it was reachable by
/// was wrong, which is the same failure this arc keeps meeting in other
/// shapes.
#[test]
fn no_verification_site_hardcodes_an_algorithm() {
    let root = workspace_root();
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned_dirs = 0usize;
    let mut scanned_total = 0usize;

    let mut dirs: Vec<&str> = AC11_DISPATCHED.to_vec();
    // This crate is scanned *only* because of this line — it is not in
    // `AC11_DISPATCHED`.
    //
    // It was here because `token.rs` was allowlisted, and it stopped being
    // allowlisted in the same change. The justification comment went stale
    // immediately, which made this line look like leftover tidying.
    //
    // It is not. Removing it silently un-scans the entire crate. Verified:
    // with this line gone, a `VerifyingKey::from_bytes` reintroduced into
    // `token.rs` passes this gate. Do not remove it on the grounds that the
    // crate is no longer excused — the crate was never listed here to begin
    // with.
    //
    // `octo-protocol` was the second crate on this line, for the mirror-image
    // reason: scanned because it held the one allowlisted file. `0009-ca`
    // dispatched that file and removed the entry, which left the line and its
    // justification stale in exactly the way the paragraph above describes.
    // Rather than leave it, `octo-protocol` moved into `AC11_DISPATCHED`,
    // which is where a fully dispatched crate belongs.
    dirs.push("crates/octo-cap-macaroon/src");

    for dir in dirs {
        let abs = root.join(dir);
        // Existence is asserted separately from the walk below. Binding the
        // `read_dir` result here was fine when the walk consumed it, and became
        // an unused variable once the walk opened each directory itself.
        assert!(
            abs.is_dir(),
            "AC-11: {dir} is on the scanned list but does not exist. AC#11 covers \
             every crate here, so a renamed or moved crate silently drops out of \
             coverage while this gate still reports green. Repoint the entry."
        );
        // Counted before the allowlist skip, so a crate whose only sources are
        // excepted files still counts as read.
        let mut scanned = 0usize;
        // RECURSIVE. This used to be a flat `read_dir`, which read only the
        // files sitting directly in `src/`. For a crate that keeps its
        // modules in subdirectories that is almost nothing: `octo-network/src`
        // holds exactly one `.rs` file at the top level, so adding the crate
        // here scanned one file and reported success — while `dot/`, `dom/`,
        // `porelay/` and the other sixteen subdirectories went unread.
        //
        // Proved by planting a `VerifyingKey::from_bytes` in
        // `porelay/heartbeat.rs` with the entry present: all 47 vectors green.
        // Five of the seven crates already listed here have subdirectories
        // that were equally unscanned.
        //
        // The completeness guard below already walked recursively; this one
        // did not, which is how a crate could be "covered" by one vector and
        // dark to the other.
        let mut stack = vec![abs.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                scanned += 1;
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if AC11_ALLOWED.iter().any(|(allowed, _)| *allowed == rel) {
                    continue;
                }
                let src = std::fs::read_to_string(&path).unwrap();
                let body = strip_comments(&src);
                // All three constructors, through the shared matcher, so this
                // scan and the pending count cannot drift apart again.
                for ctor in AC11_SITE_CONSTRUCTORS {
                    if contains_path_call(&body, ctor) {
                        offenders.push(format!("{rel} ({ctor})"));
                    }
                }
            }
        }
        if scanned > 0 {
            scanned_dirs += 1;
        }
        scanned_total += scanned;
    }

    assert!(
        offenders.is_empty(),
        "AC-11: these files rebuild an Ed25519 verifying key inline instead \
         of dispatching through AlgorithmRegistry: {offenders:?}. If one of \
         them is a signing site, say so in AC11_ALLOWED with the reason."
    );
    assert!(
        scanned_dirs > 0,
        "AC-11: no directory on the scanned list yielded a Rust source, so this \
         gate read nothing and reported success vacuously."
    );
    // The stronger half of the same floor. `scanned_dirs > 0` is satisfied by
    // a single file, which is exactly how this scan hid: `read_dir` is not
    // recursive, so a crate whose modules live in subdirectories contributed
    // its `lib.rs` and nothing else, and every vector stayed green.
    //
    // 347 files across the ten scanned crates today; a non-recursive walk of
    // the same list reads 120, measured by disabling the recursion step. The
    // floor sits between the two, so ordinary churn cannot trip it and a
    // regression to the flat version fails here instead of passing quietly
    // for another arc.
    assert!(
        scanned_total >= 250,
        "AC-11: the inline-rebuild scan read only {scanned_total} files across \
         {scanned_dirs} directories, expected at least 250. The walk has \
         probably gone back to being non-recursive: a flat walk of the same \
         list reads 120 files, and for octo-network it meant lib.rs and none \
         of its seventeen subdirectories."
    );
}

#[test]
fn ac11_every_allowlist_entry_still_exists_and_still_has_a_reason() {
    // An allowlist that silently rots is worse than none: the path is
    // renamed or deleted, the excepted file stops being scanned, and a new
    // hardcoded site inside it passes unnoticed.
    for (path, reason) in AC11_ALLOWED {
        assert!(
            workspace_root().join(path).exists(),
            "AC-11: allowlisted path {path} no longer exists. Remove the entry \
             or repoint it; a stale exception is a silent hole."
        );
        assert!(
            reason.len() > 40 && !reason.trim().is_empty(),
            "AC-11: allowlist entry {path} needs a real reason, not a stub"
        );

        // The half that actually rots. Every entry above is written as
        // "blocked until <landed work>", so the normal life of an entry is
        // that its blocker gets fixed and the file stops hardcoding anything.
        // Nothing here noticed: existence and a reason both stay true, the
        // vector keeps skipping the file, and a NEW hardcoded site in it goes
        // unseen.
        //
        // `crates/octo-cap-macaroon/src/token.rs` sat in this state for the
        // whole of missions `0009-c4`, `0009-e5` and `0009-e10` — the blocker
        // named in its reason ("the token does not carry an algorithm") landed
        // in all three, and by the end the file rebuilt no verifying key at
        // all. The exception was pure blind spot.
        let src = std::fs::read_to_string(workspace_root().join(path)).unwrap();
        let body = strip_comments(&src);
        // Any of the constructors justifies the entry, and none surviving does NOT.
        // This used to name two of the three by hand, which is the same drift
        // `ac11_site_count` was introduced to end: an allowlisted file whose
        // only site is `Signature::from_slice` would have been told to keep an
        // exception it no longer needs, while the scan looked for three.
        assert!(
            ac11_site_count(&body) > 0,
            "AC-11: allowlisted {path} no longer contains any constructor \
             site, so the exception is superseded — remove the entry. An allowlist \
             entry outliving its blocker exempts the file from the gate for no \
             reason, which is a silent hole rather than a documented one."
        );
    }
}

#[test]
fn ac11_the_ed25519_registration_itself_is_where_keys_get_built() {
    // AC-11 excludes "the Ed25519 registration itself". If that exclusion
    // ever names a file that no longer exists, the dispatching crates lose
    // their only legal home for constructing a key.
    let reg = workspace_root().join("crates/octo-algo-ed25519/src/lib.rs");
    assert!(
        reg.exists(),
        "AC-11: the Ed25519 registration crate is gone. Either restore it or \
         amend AC-11 with the new registration's home."
    );
    let src = std::fs::read_to_string(&reg).unwrap();
    assert!(
        strip_comments(&src).contains("VerifyingKey::from_bytes"),
        "AC-11: the registration must still be where the key is constructed, \
         otherwise the dispatching crates have nowhere legal to build one."
    );
}

// ── Two algorithms, still independent ────────────────────────────────────

/// Body of one `[section]` of a Cargo.toml, or `None` if it has none.
///
/// Section-aware on purpose. `octo-algo-stark-schnorr` names
/// `octo-algo-ed25519` in `[dev-dependencies]` — its suite must be able to
/// build a real Ed25519 signature to show the two algorithms coexist — and a
/// bare `contains` over the whole manifest would read that as a runtime edge
/// and demand the dependency be deleted, which would delete the proof.
fn cargo_section(manifest: &str, section: &str) -> Option<String> {
    let header = format!("[{section}]");
    let start = manifest.lines().position(|l| l.trim() == header)?;
    Some(
        manifest
            .lines()
            .skip(start + 1)
            .take_while(|l| !l.trim_start().starts_with('['))
            .collect(),
    )
}

/// Assert a section exists before relying on its contents being empty.
fn cargo_section_present(manifest: &str, section: &str) -> String {
    cargo_section(manifest, section).unwrap_or_else(|| {
        panic!(
            "no [{section}] section in manifest — if it was renamed, amend \
             this guard rather than letting it pass on a section that does \
             not exist"
        )
    })
}

/// RFC-0009's second algorithm must be a real peer of Ed25519, not a
/// replacement or a wrapper around it.
///
/// With one registered algorithm, everything this registry exists to prevent
/// is unobservable: a verifier that returned Ed25519 for *any* identifier
/// would answer every lookup correctly, and a cross-algorithm replay would
/// look like an ordinary verification. Two algorithms are what make those
/// wrong answers visible, so the properties that make a second algorithm
/// possible are worth guarding — each one is a silent regression that leaves
/// every other test in the workspace green.
#[test]
fn the_two_algorithm_crates_stay_independent_and_additive() {
    // 1. Layer B must not know either Layer E crate. If octo-ident depended
    //    on an algorithm, the registry would be a wrapper over a hardcoded
    //    choice rather than the thing that removes one.
    let ident = read("crates/octo-ident/Cargo.toml");
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let body = cargo_section(&ident, section).unwrap_or_default();
        assert!(
            !body.contains("octo-algo-"),
            "octo-ident [{section}] names an algorithm crate. Layer B must not \
             depend on Layer E: the registry exists so no algorithm is baked \
             into the identity substrate."
        );
    }

    // 2. Neither algorithm may depend on the other at runtime. They are
    //    independently selectable per-algorithm crates; a dependency either
    //    way means a deployment cannot take one without the other, and
    //    "this deployment rejects stark-schnorr" stops being expressible.
    //
    //    `[dev-dependencies]` is deliberately NOT checked: the coexistence
    //    test needs both, and that is a test-time edge only.
    let stark = read("crates/octo-algo-stark-schnorr/Cargo.toml");
    assert!(
        !cargo_section_present(&stark, "dependencies").contains("octo-algo-ed25519"),
        "octo-algo-stark-schnorr takes a runtime dependency on \
         octo-algo-ed25519. The two must be independently installable; a \
         dependency makes 'this deployment supports only one of them' \
         unexpressible."
    );

    // 3. Both registrations must ADD to the registry rather than declare it.
    //
    //    `bootstrap` declares the deployment's whole algorithm set and is
    //    idempotent only for an identical one, so two crates calling it each
    //    get one winner and one `ConflictingBootstrap`. That is a registry of
    //    one algorithm with a deadlock at the install site. This is a
    //    one-word slip, and reverting it is invisible to any test that does
    //    not happen to install both in one process.
    for (path, why) in [
        ("crates/octo-algo-ed25519/src/lib.rs", "Ed25519"),
        (
            "crates/octo-algo-stark-schnorr/src/lib.rs",
            "STARK-curve Schnorr",
        ),
    ] {
        let src = strip_comments(&read(path));
        let installs: Vec<&str> = src
            .lines()
            .filter(|l| l.contains("vec![") && l.contains("Algorithm::shared()"))
            .collect();
        assert!(
            !installs.is_empty(),
            "{why}: no `install()` body found in {path}. If the install path \
             moved, amend this guard rather than deleting it."
        );
        for line in installs {
            assert!(
                line.contains("bootstrap_additional("),
                "{why}: install() in {path} calls `{line}`, which declares the \
                 deployment's whole algorithm set instead of adding to it. \
                 The first caller would win and every other algorithm crate \
                 would report ConflictingBootstrap."
            );
        }
    }
}

// ── Capability-token API must not name ed25519-dalek ─────────────────────

#[test]
fn no_capability_token_api_names_ed25519_dalek() {
    // RFC-0009 AC#11's requirement is stronger than "dispatch somewhere".
    // While `CapabilityToken.holder_sig` was typed
    // `ed25519_dalek::Signature`, the type itself asserted Ed25519 and no
    // amount of dispatch elsewhere could make a non-Ed25519 holder
    // signature expressible — so the mission `0009-c4` could not write this
    // guard, because it would have failed on arrival.
    //
    // Scoped to the capability substrate. `octo-cap-zk`'s `PrivateWitness`
    // is deliberately exempt and is NOT a capability-token API: its
    // `holder_sig` is circuit witness data, a 64-byte blob the STARK
    // commits to without constraining an algorithm, so carrying an
    // AlgorithmId there would change the committed witness shape and
    // require a circuit amendment.
    // Comments are stripped: the newtype's doc comment necessarily names the
    // dalek type it replaced, and a guard that matched prose would fail on
    // its own explanation.
    let token = strip_comments(&read("crates/octo-cap-macaroon/src/token.rs"));
    assert!(
        !token.contains("ed25519_dalek::Signature"),
        "AC-11: CapabilityToken must not expose an ed25519_dalek signature \
         type. The type asserts the algorithm; the registry exists so \
         nothing asserts it."
    );

    // The dispatch must actually read the algorithm off the signature.
    let body = &token;
    assert!(
        body.contains("verify_dispatch(") && body.contains("self.holder_sig.algorithm_id()"),
        "AC-11: verify_holder_sig must dispatch on the signature's own \
         algorithm, not on a hardcoded one."
    );
}

/// The pending table must be **complete for the crates it covers**.
///
/// Counting the files the table names proves those files still hold what it
/// says. It says nothing about a file the table never named, and that is not
/// hypothetical: a `VerifyingKey::from_bytes` planted in
/// `octo-network/src/mon/vdf.rs` — a file no entry mentions — left all five
/// AC-11 vectors green, while the same plant in `dot/dc.rs` (which *is*
/// listed) turned the count vector red immediately.
///
/// So this walks each covered crate and asserts that **every** file holding a
/// constructor appears in `AC11_PENDING` or `AC11_ALLOWED`. A new site in a
/// new file now fails here instead of waiting for someone to extend a table.
#[test]
fn ac11_pending_table_is_complete_for_every_crate_it_covers() {
    // Crate roots implied by the pending paths: everything up to `/src`.
    let mut roots: BTreeSet<&str> = BTreeSet::new();
    for (path, _, _) in AC11_PENDING {
        if let Some(idx) = path.find("/src/") {
            roots.insert(&path[..idx + 4]);
        }
    }
    assert!(
        !roots.is_empty(),
        "AC-11: the pending table names no crate root, so there is nothing to \
         check for completeness"
    );

    let mut unlisted: Vec<String> = Vec::new();
    for root in roots {
        let abs = workspace_root().join(root);
        assert!(
            abs.is_dir(),
            "AC-11: pending crate root {root} does not exist. Every path in \
             AC11_PENDING must sit under a real crate root for this check to \
             scan anything."
        );
        let mut stack = vec![abs.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(workspace_root())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let body = strip_comments(&std::fs::read_to_string(&path).unwrap());
                if ac11_site_count(&body) == 0 {
                    continue;
                }
                let listed = AC11_PENDING.iter().any(|(p, _, _)| *p == rel)
                    || AC11_ALLOWED.iter().any(|(p, _)| *p == rel);
                if !listed {
                    unlisted.push(rel);
                }
            }
        }
    }

    assert!(
        unlisted.is_empty(),
        "AC-11: these files hold an inline Ed25519 constructor but appear in \
         neither AC11_PENDING nor AC11_ALLOWED. They are outside the gate's \
         view, which is how octo-network went unrecorded for so long: add a \
         pending entry with its measured count, or an allowlist entry with \
         the reason it is legitimate."
    );
}

/// Every crate that verifies a signature is either scanned or accounted for.
///
/// The tests above all walk **fixed lists**. A list only speaks about the
/// crates someone put in it, so a crate holding verification code and absent
/// from every list is not "clean" — it is **unexamined**, and the two look
/// identical from outside. That is the failure this whole arc keeps meeting in
/// new forms, one level up from the site it was last found at.
///
/// It was live here. `octo-cap-macaroon` holds `verify_holder_sig`, which
/// dispatches correctly on the algorithm recorded beside the signature, and
/// which no gate had ever read. The crate turned out to be clean — its
/// `HolderSignature` stores `Vec<u8>` and validates the length against the
/// *registered algorithm's* `signature_len()`, which is the best instance of
/// this pattern in the workspace. It was clean by luck and by review, not by
/// construction, and nothing would have said so.
///
/// Adding the crate to `AC11_DISPATCHED` is what makes it scanned. This test
/// is what stops the next one from being added unexamined.
///
/// A crate reaches this check by *containing* the call, test code included. A
/// crate whose only occurrence is its own tests is still worth listing, because
/// "the product path is dispatched, the test path is not" is exactly the
/// distinction a reader cannot make from outside.
#[test]
fn ac11_every_crate_that_verifies_a_signature_is_scanned_or_explained() {
    let root = workspace_root();
    let crates_dir = root.join("crates");

    // Every crate any AC-11 list speaks about, in any form.
    let mut accounted: BTreeSet<String> = BTreeSet::new();
    for d in AC11_DISPATCHED {
        accounted.insert(d.to_string());
    }
    for (p, _, _) in AC11_PENDING {
        if let Some(idx) = p.find("/src") {
            accounted.insert(p[..=idx + 3].to_string());
        }
    }
    for (p, _) in AC11_OUT_OF_SCOPE {
        if let Some(idx) = p.find("/src") {
            accounted.insert(p[..=idx + 3].to_string());
        }
    }
    for (p, _) in AC11_ALLOWED {
        if let Some(idx) = p.find("/src") {
            accounted.insert(p[..=idx + 3].to_string());
        }
    }
    for (p, _) in AC11_ALGORITHM_CRATES {
        accounted.insert(p.to_string());
    }

    let mut scanned = 0usize;
    let mut unaccounted: Vec<String> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        panic!("cannot read {}", crates_dir.display());
    };
    for entry in entries.flatten() {
        let src = entry.path().join("src");
        if !src.is_dir() {
            continue;
        }
        let rel = format!("crates/{}/src", entry.file_name().to_string_lossy());
        let mut sites = 0usize;
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            let Ok(files) = std::fs::read_dir(&dir) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let body = strip_comments(&std::fs::read_to_string(&path).unwrap_or_default());
                sites += body.matches("verify_dispatch").count();
            }
        }
        if sites == 0 {
            continue;
        }
        scanned += 1;
        if !accounted.contains(&rel) {
            unaccounted.push(format!("{rel} ({sites} call sites)"));
        }
    }

    assert!(
        unaccounted.is_empty(),
        "AC-11: these crates call verify_dispatch and appear in no AC-11 list, so \
         the gate has never read them. A crate that is unexamined and a crate that \
         is clean are indistinguishable from outside, which is how a site stays \
         hidden: add the crate to AC11_DISPATCHED if its sites should be \
         dispatched, or to AC11_ALGORITHM_CRATES if it implements an algorithm \
         rather than consuming one. Unaccounted: {unaccounted:?}"
    );

    // Measured, not guessed. The first version of this asserted nothing about
    // how much it actually walked, so a regression that stopped the walk would
    // have gone green by finding nothing to complain about. 14 is the measured
    // count of crates containing the call today.
    assert!(
        scanned >= 14,
        "the crate-coverage walk examined only {scanned} crates containing \
         verify_dispatch, against a measured 14. If this ever fires, the walk \
         stopped covering the workspace — which would make the check above pass \
         vacuously."
    );
}

#[test]
fn ac11_pending_sites_are_counted_and_still_present() {
    // The count is the guard. An entry that names a file without pinning how
    // many sites are in it is a comment with a type, and it fails in the one
    // direction that matters: a fifteenth site in `octo-network` would be
    // invisible, exactly as the first fourteen were.
    for (path, expected, reason) in AC11_PENDING {
        let abs = workspace_root().join(path);
        assert!(
            abs.exists(),
            "AC-11: pending {path} no longer exists. Move it to AC11_DISPATCHED \
             if the sites are gone, or drop the entry — a pending path that \
             does not resolve describes nothing."
        );
        assert!(
            reason.len() > 20,
            "AC-11: pending entry {path} needs a real reason, not a stub"
        );
        // Disjointness: a file cannot be both dispatched (gated for zero
        // sites) and pending (gated for N sites). Both gates would run, and
        // whichever lost would be the one that failed for no stated reason.
        assert!(
            !AC11_DISPATCHED
                .iter()
                .any(|d| path.starts_with(d) && d.ends_with("/src")),
            "AC-11: {path} is both dispatched and pending. Pick one."
        );
        assert!(
            !AC11_ALLOWED.iter().any(|(allowed, _)| allowed == path),
            "AC-11: {path} is both pending and allowlisted. A pending file is \
             being counted; an allowlisted file is being skipped. Those are \
             opposite answers to the same question."
        );

        // One matcher, shared with the completeness scan above. These two used
        // to disagree — it is why `Signature::from_slice` reached a file
        // recorded as zero sites without turning anything red.
        let body = strip_comments(&std::fs::read_to_string(&abs).unwrap());
        let actual = ac11_site_count(&body);
        assert_eq!(
            actual, *expected,
            "AC-11: {path} has {actual} un-dispatched sites, the table says \
             {expected}.\n  \
             A new site: add it to AC11_PENDING with its count.\n  \
             A dispatched site: remove the entry, or drop the file when its \
             last site is gone — a pending entry that outlives its sites is \
             how octo-network went unrecorded in the first place."
        );
    }
}

#[test]
fn ac11_every_out_of_scope_crate_is_justified_and_disjoint() {
    // An out-of-scope list that is never read is decoration. Two things
    // have to hold for it to be honest: each entry carries a real reason,
    // and no crate is both dispatched and excused.
    for (crate_path, reason) in AC11_OUT_OF_SCOPE {
        assert!(
            !AC11_DISPATCHED.contains(crate_path),
            "AC-11: {crate_path} is both dispatched and excused. Pick one: \\
             either its sites must dispatch, or the excuse must go."
        );
        assert!(
            reason.len() > 80,
            "AC-11: out-of-scope entry {crate_path} needs a real reason, not a \\
             stub. A short one is indistinguishable from a hand-wave."
        );
        assert!(
            workspace_root().join(crate_path).exists(),
            "AC-11: out-of-scope path {crate_path} no longer exists. Remove the \\
             entry; a stale exception is a silent hole."
        );
        // And the exception must stay pinned to what it claims: the crate
        // still has the site that justifies excluding it. If that site is
        // routed through the registry or deleted, the entry describes nothing,
        // and an out-of-scope entry describes nothing *forever* unless
        // something checks.
        //
        // This used to be a hand-written assertion naming the nostr crate, so
        // it protected exactly one entry and a second entry added later would
        // have come in silently unguarded. Running it per entry is the only
        // version that survives the next entry.
        let has_site = std::fs::read_dir(workspace_root().join(crate_path))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("rs"))
                    .any(|e| {
                        std::fs::read_to_string(e.path())
                            .map(|src| strip_comments(&src).contains("VerifyingKey::from_bytes"))
                            .unwrap_or(false)
                    })
            })
            .unwrap_or(false);
        assert!(
            has_site,
            "AC-11: out-of-scope {crate_path} no longer contains a VerifyingKey::from_bytes \
             site, so the excuse describes nothing. Re-decide it rather than leaving a \
             reason for a thing that is gone."
        );
    }
}

/// Split a Rust source file into `(signature, body)` per function.
///
/// The point is scope. An earlier version of the key-shape guard matched a
/// binding name against a declaration found *anywhere in the file*, which
/// flagged a wire record field, an identity key field and two test fakes
/// whose parameter names happened to collide with a real verifier's. None of
/// those is a verification parameter, and a guard whose first report is
/// inactionable gets muted.
///
/// Scoping to a function body removes all four: those declarations are either
/// at file scope or inside a different `fn` than the `verify_dispatch` call.
fn fn_blocks(src: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = src.lines().collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            (t.starts_with("fn ")
                || t.starts_with("pub fn ")
                || t.starts_with("pub(crate) fn ")
                || t.starts_with("async fn ")
                || t.starts_with("pub async fn "))
                && l.contains("fn ")
                && !t.starts_with("//")
        })
        .map(|(i, _)| i)
        .collect();
    let mut out = Vec::new();
    for (n, &s) in starts.iter().enumerate() {
        let indent = lines[s].len() - lines[s].trim_start().len();
        let end = starts
            .get(n + 1)
            .copied()
            .unwrap_or(lines.len())
            .min(lines.len());
        // Signature: up to the first line that opens a block.
        let mut sig_end = s;
        while sig_end < end && !lines[sig_end].contains('{') {
            sig_end += 1;
        }
        let sig = lines[s..=sig_end.min(end - 1)].join("\n");
        let body = lines[(sig_end + 1).min(end)..end]
            .iter()
            .take_while(|l| {
                // Stop at a closing brace at or left of the fn's own indent.
                let t = l.trim();
                let ind = l.len() - l.trim_start().len();
                !(t == "}" && ind <= indent)
            })
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let _ = &lines;
        out.push((sig, body));
    }
    out
}

/// Split a call's argument text into top-level arguments.
fn top_level_args(inner: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in inner.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 {
            args.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(c);
    }
    args.push(cur);
    args
}

/// A verification site's key must be a slice, never a fixed-size array.
///
/// This is the *second* half of what AC#11 asks for, and it went unseen for
/// the whole of the dispatch work. Every site was converted from
/// `key: &VerifyingKey` to `key: &[u8; 32]`, which removed a **type**-level
/// Ed25519 assertion and left a **length**-level one:
/// `octo-algo-stark-schnorr` public keys are 33 bytes, so a `[u8; 32]`
/// parameter could never carry the second algorithm. The body dispatched
/// correctly and the signature still decided which algorithms were
/// expressible — `0009-cb` fixed the type, `0009-cc` the length.
///
/// Missions `0009-cc` and its follow-on found 43 sites by hand. The count is
/// worth stating because two earlier estimates were wrong in instructive ways:
/// 79 came from grepping parameter *names* and counted macaroon secrets and
/// field elements, and a first pass at 38 counted only parameters that reach
/// `verify_dispatch` directly, missing five **forwarders** that hand the key
/// to another verify function.
///
/// This vector is the automated version, and it is what makes 43 a
/// floor rather than a number somebody has to remember.
///
/// **Method.** Per function: slice signature and body, find `verify_dispatch(`
/// in the body, take its **second** argument — the first version read the
/// first comma-separated argument, which is the algorithm, so it passed with a
/// fixed-size key reinstated — and check whether that binding is declared with
/// a fixed size *in the same function*.
///
/// Scoping is what makes it usable. A file-level version of this reported six
/// hits, none actionable: a wire record field, `IdentityKey`'s own key, a slug
/// helper and two test fakes whose names collided.
///
/// **What this does NOT catch: forwarders.** A function that takes the key and
/// hands it to another *verify* function never mentions `verify_dispatch`, so
/// this vector skips it — verified by planting one, which this vector stayed
/// green through. Five exist in this workspace and were fixed by hand the same
/// day (`verify_full` in heartbeat and forwarding, `validate_bind` in witness,
/// `append` in handover, and `dot/gateway.rs`).
///
/// Catching them means following the call graph: a key that reaches
/// `verify_dispatch` through another function. That is a real analysis, and a
/// half-version of it would guess at which `verify_*` names are verifiers —
/// which is how a guard starts reporting things nobody can act on. The gap is
/// stated here rather than papered over, because a reader who assumes this is
/// total will not go looking for the forwarder class.
///
/// **And a second gap, which this one closes: aliases declared elsewhere.** The
/// alias table is built workspace-wide, not per-file, because a file that says
/// `use octo_ident::Ed25519PublicKey;` and then takes `key:
/// &Ed25519PublicKey` has exactly the same pin — one more hop away from the
/// literal spelling, and further away from a reader looking for it. A per-file
/// table finds nothing there and reports the crate clean, which is the one
/// outcome a guard must never produce by not looking.
/// Fixed-size array type aliases declared in a file: name -> length.
///
/// `pub type Ed25519PublicKey = [u8; 32];` pins a key exactly as hard as a
/// literal annotation does, one indirection away from any scan that looks for
/// the literal.
fn fixed_size_aliases(src: &str) -> std::collections::HashMap<String, usize> {
    let mut out = std::collections::HashMap::new();
    let mut i = 0usize;
    while let Some(k) = src[i..].find("type ") {
        let start = i + k + "type ".len();
        let name: String = src[start..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        // `start` is always past `i`, so the scan advances even when the
        // candidate below turns out not to be an alias.
        i = start;
        if name.is_empty() {
            continue;
        }
        let rest = &src[start + name.len()..];
        let Some(eq) = rest.find('=') else {
            continue;
        };
        let after = rest[eq + 1..].trim_start();
        if let Some(num) = after.strip_prefix("[u8;") {
            // `[u8; 32]` is written with a space by convention here, and
            // skipping that would make the helper silently find nothing while
            // looking for exactly the thing it was written to find.
            let num = num.trim_start();
            let len: String = num.chars().take_while(char::is_ascii_digit).collect();
            if !len.is_empty() {
                out.insert(name, len.parse().unwrap_or(0));
            }
        }
    }
    out
}

/// Fixed-size array type aliases declared anywhere under `crates/*/src`.
///
/// Exists so a *file-local* alias table is not mistaken for a complete one. An
/// alias declared in another crate and pulled in with `use` pins a key just as
/// hard, and a per-file table resolves it to nothing.
///
/// A name declared at two different widths is dropped rather than resolved to
/// one of them: guessing which `Key32` a `use` statement reached is exactly the
/// kind of unfounded claim this guard exists to avoid, and the cost of dropping
/// it is one uncaught site rather than one false accusation of a security pin.
/// The union is recorded so the test can assert the drop was deliberate rather
/// than accidental.
fn workspace_fixed_size_aliases(
    root: &std::path::Path,
) -> (
    std::collections::HashMap<String, usize>,
    std::collections::HashSet<String>,
) {
    let mut out: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut ambiguous: std::collections::HashSet<String> = std::collections::HashSet::new();
    let Ok(crates) = std::fs::read_dir(root.join("crates")) else {
        return (out, ambiguous);
    };
    let mut stack: Vec<std::path::PathBuf> =
        crates.flatten().map(|e| e.path().join("src")).collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (name, width) in fixed_size_aliases(&strip_comments(&src)) {
                match out.get(&name) {
                    Some(existing) if *existing != width => {
                        out.remove(&name);
                        ambiguous.insert(name);
                    }
                    Some(_) => {}
                    None => {
                        if !ambiguous.contains(&name) {
                            out.insert(name, width);
                        }
                    }
                }
            }
        }
    }
    (out, ambiguous)
}

/// The declared type of `name` in a function signature, with any reference,
/// `mut`, and lifetime prefix stripped.
///
/// Returns the type as written rather than a bare identifier, because the two
/// shapes the guard must tell apart are both legal and neither is a name:
/// `key: &Ed25519PublicKey` -> `Ed25519PublicKey` (resolvable against the
/// alias table) and `key: &[u8]` -> `[u8]` (not). Returning `None` for the
/// second would conflate "declared as a slice" with "not a parameter here",
/// and a helper that cannot tell those apart is not much of a helper.
fn declared_type_of<'a>(sig: &'a str, name: &str) -> Option<&'a str> {
    let re = format!("{name}:");
    let at = sig.find(&re)?;
    let mut rest = sig[at + re.len()..].trim_start();
    if rest.starts_with('\'') {
        // `key: &'a Foo` — drop the lifetime binder, not the `&` after it.
        rest = rest[1..].split_once('\'')?.1.trim_start();
    }
    let rest = rest.strip_prefix("&").unwrap_or(rest);
    let rest = rest.strip_prefix("mut ").unwrap_or(rest);
    let ty = rest
        .split([',', ')', '{', '\n'])
        .next()
        .unwrap_or("")
        .trim();
    if ty.is_empty() {
        None
    } else {
        Some(ty)
    }
}

#[test]
fn the_alias_helpers_find_the_indirection_they_exist_for() {
    // The whole point of the alias check is one case. Pin it against the real
    // file, because a helper that finds nothing looks exactly like a helper
    // that found nothing because there was nothing to find.
    let src =
        std::fs::read_to_string(workspace_root().join("crates/quota-router-storage/src/ask.rs"))
            .expect("read ask.rs");
    let stripped = strip_comments(&src);
    let aliases = fixed_size_aliases(&stripped);
    assert_eq!(
        aliases.get("Ed25519PublicKey").copied(),
        Some(32),
        "the alias helper must resolve Ed25519PublicKey to its width, got {aliases:?}"
    );

    // And the alias is still load-bearing elsewhere in the file, so it is a
    // live example rather than a declaration left over from the fix.
    assert!(
        stripped.contains("Ed25519PublicKey"),
        "the alias should still be declared; if it was removed the alias check \
         has no live subject in this crate and needs a new one"
    );

    // The guard's whole decision is one map lookup: parameter type -> width.
    // Pin both halves of that lookup on synthetic signatures, so this test
    // fails if either half silently stops working, independently of whatever
    // the live file happens to look like today.
    let pinned = "fn f(key: &Ed25519PublicKey) { }";
    assert_eq!(declared_type_of(pinned, "key"), Some("Ed25519PublicKey"));
    assert_eq!(
        aliases
            .get(declared_type_of(pinned, "key").unwrap_or_default())
            .copied(),
        Some(32),
        "the alias spelling of a pinned key must resolve to a width"
    );
    assert_eq!(
        declared_type_of("fn f(key: &[u8]) { }", "key"),
        Some("[u8]")
    );
    assert_eq!(
        aliases
            .get(declared_type_of("fn f(key: &[u8]) { }", "key").unwrap_or_default())
            .copied(),
        None,
        "a slice must not resolve to a width, or every slice param would be \
         reported as a fixed-size key"
    );
    // A by-value array is also a pin, spelled with no `&` at all.
    assert_eq!(
        declared_type_of("fn f(key: [u8; 32]) { }", "key"),
        Some("[u8; 32]")
    );

    // `AskSigned::verify` and `verify_settlement_receipt` were the two sites
    // the alias check found. Both now take a slice, so the check has nothing
    // left to report here. Pinning the widened shape is what stops a later
    // change from quietly reintroducing the pin under the same alias.
    let (sig, _body) = fn_blocks(&stripped)
        .into_iter()
        .find(|(s, _)| s.contains("fn verify("))
        .expect("AskSigned::verify");
    assert_eq!(
        declared_type_of(&sig, "asker_public_key"),
        Some("[u8]"),
        "the key must be a slice, not the fixed-width alias, got {sig}"
    );
    assert_eq!(
        aliases
            .get(declared_type_of(&sig, "asker_public_key").unwrap_or_default())
            .copied(),
        None,
        "the widened parameter must not resolve to a fixed width, got {sig}"
    );

    // The workspace table exists for the case a file-local table cannot see:
    // an alias declared in one crate and pulled in with `use` by another.
    // `Ed25519PublicKey` is declared in `quota-router-storage`; find where it
    // is actually declared rather than assuming, then confirm the workspace
    // scan finds it from a file that does not declare it.
    let (ws, ambiguous) = workspace_fixed_size_aliases(&workspace_root());
    assert_eq!(
        ws.get("Ed25519PublicKey").copied(),
        Some(32),
        "the workspace scan must find an alias declared outside the scan target"
    );
    assert!(
        !ambiguous.contains("Ed25519PublicKey"),
        "Ed25519PublicKey must not be ambiguous workspace-wide"
    );

    // And the drop-on-conflict rule is real, not decorative: a synthetic name
    // at two widths resolves to nothing rather than to whichever was seen
    // last, so the guard cannot accuse a site on a coin flip.
    let one_wide = "pub type __GuardProbeWide = [u8; 32];";
    let other_wide = "pub type __GuardProbeWide = [u8; 64];";
    assert_eq!(
        fixed_size_aliases(one_wide)
            .get("__GuardProbeWide")
            .copied(),
        Some(32)
    );
    assert_eq!(
        fixed_size_aliases(other_wide)
            .get("__GuardProbeWide")
            .copied(),
        Some(64)
    );
    assert!(
        !ws.contains_key("__GuardProbeWide"),
        "a name declared at one width must not be resolved by accident"
    );
}

#[test]
fn no_verification_site_pins_its_key_to_a_fixed_size_array() {
    let root = workspace_root();
    let mut offenders: Vec<String> = Vec::new();
    let mut files_read = 0usize;
    let mut sites_seen = 0usize;

    let mut stack: Vec<std::path::PathBuf> = AC11_DISPATCHED.iter().map(|d| root.join(d)).collect();
    let (workspace_aliases, _ambiguous) = workspace_fixed_size_aliases(&root);
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            files_read += 1;
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).unwrap();
            let stripped = strip_comments(&src);
            // Fixed-size array aliases: this file's own, plus every alias
            // declared anywhere in the workspace, because a `use` brings one in
            // from another crate and the pin is identical. This file's own
            // declaration wins on a name clash — it is the one actually in
            // scope here.
            //
            // **This indirection hid two real sites.** `AskSigned::verify` and
            // `verify_settlement_receipt` both dispatch on `algorithm_id` —
            // so they satisfied the AC-11 dispatch vector — while taking
            // `key: &Ed25519PublicKey`, and `Ed25519PublicKey` is
            // `pub type ... = [u8; 32]`. Neither could carry a 33-byte key,
            // and neither was reported, because the pin was spelled in the
            // alias rather than at the parameter.
            let mut aliases = fixed_size_aliases(&stripped);
            for (name, width) in &workspace_aliases {
                aliases.entry(name.clone()).or_insert(*width);
            }

            for (sig, body) in fn_blocks(&stripped) {
                if !body.contains("verify_dispatch(") {
                    continue;
                }
                let mut from = 0usize;
                while let Some(k) = body[from..].find("verify_dispatch(") {
                    let start = from + k + "verify_dispatch(".len();
                    from = start;
                    let bytes = body.as_bytes();
                    let mut depth = 1i32;
                    let mut end = start;
                    while end < bytes.len() && depth > 0 {
                        match bytes[end] {
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            _ => {}
                        }
                        end += 1;
                    }
                    let args = top_level_args(&body[start..end.saturating_sub(1)]);
                    if args.len() < 2 {
                        continue;
                    }
                    sites_seen += 1;
                    let binding = args[1]
                        .trim()
                        .trim_start_matches('&')
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_start_matches("mut ")
                        .to_string();
                    if binding.is_empty()
                        || !binding.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    let by_ref = format!("{binding}: &[u8;");
                    let by_val = format!("{binding}: [u8;");
                    let as_let = format!("let {binding}: [u8;");
                    if sig.contains(&by_ref)
                        || sig.contains(&by_val)
                        || body.contains(&as_let)
                        || body.contains(&format!("let mut {binding}: [u8;"))
                    {
                        offenders.push(format!("{rel} (key `{binding}` is a fixed-size array)"));
                        continue;
                    }
                    // The same pin, spelled as an alias. Resolved from the
                    // signature rather than the call, because `verify_dispatch`
                    // passes a bare identifier and the declared type lives in
                    // the parameter list.
                    if let Some(len) = declared_type_of(&sig, binding.as_str())
                        .and_then(|ty| aliases.get(ty).copied())
                    {
                        offenders.push(format!(
                            "{rel} (key `{binding}` is an alias for [u8; {len}])"
                        ));
                    }
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "AC-11: these verification sites pin their key to a fixed-size array, so \
         the algorithm parameter cannot reach any other length: {offenders:?}. \
         Take the key as a slice and let the registered algorithm report \
         BadKeyLength, which is also the only form that names the algorithm \
         and its expected size."
    );
    // Measured, not guessed. The first version of this asserted 60 and went
    // red on arrival against an actual 53 — a floor invented from a hunch is
    // the same error as counting constructors instead of sites, in the other
    // direction: it demands coverage that does not exist and gets muted.
    //
    // 45 sits below the measured 53 with room for churn, and far above what
    // any plausible regression leaves behind. Dropping the recursive walk, or
    // losing one large crate to a renamed path, lands well under it.
    assert!(
        sites_seen >= 45,
        "the key-shape scan saw only {sites_seen} dispatch sites across {files_read} \
         files, against a measured 53. A regression that stops walking, or that \
         loses a crate to a rename, is what this is here to catch."
    );
}

/// A verification site must reach the algorithm registry the same way every
/// other site does.
///
/// Both algorithm crates expose exactly one verification entry point,
/// `verify_dispatch`, and that is what the key-shape scan above looks for. The
/// problem is what `verify_dispatch` is not: both crates also publish
/// `shared_registry`, and a site that calls it can do
///
/// ```text
/// registry.get(id)?.verify(key, msg, sig)
/// ```
///
/// which verifies correctly, dispatches correctly, and is invisible to a scan
/// keyed on the function name — the exact shape of evasion this arc has hit
/// three times now. A name-based measure cannot see it, so the class is closed
/// structurally instead: the registry handle and a verification call must not
/// appear in the same function body.
///
/// Narrow on purpose. Reading the registry to *ask* what a deployment supports
/// is legitimate and must not be reported, so this fires only when the same
/// function also verifies. Verified by planting that pairing, which this vector
/// caught; the planted site was otherwise invisible to `verify_dispatch`.
///
/// What this does NOT catch: a handle taken in one function and passed to
/// another. That is the forwarder gap the key-shape scan already documents,
/// and it is not closed here.
#[test]
fn no_verification_site_reaches_the_algorithm_registry_by_hand() {
    let root = workspace_root();
    let mut offenders: Vec<String> = Vec::new();

    let mut stack: Vec<std::path::PathBuf> = AC11_DISPATCHED.iter().map(|d| root.join(d)).collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let stripped = strip_comments(&src);
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            for (sig, body) in fn_blocks(&stripped) {
                let takes_registry = body.contains("shared_registry()")
                    || body.contains("AlgorithmRegistry")
                    || body.contains("global_registry(");
                if !takes_registry {
                    continue;
                }
                // A verification call that is not the sanctioned entry point.
                let hand_rolled = body.contains(".verify(") && !body.contains("verify_dispatch(");
                if hand_rolled {
                    let name = sig
                        .split("fn ")
                        .nth(1)
                        .and_then(|s| s.split('(').next())
                        .unwrap_or("<unknown>")
                        .trim();
                    offenders.push(format!("{rel} (fn `{name}` reaches the registry by hand)"));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "AC-11: these sites reach the algorithm registry directly instead of through \
         verify_dispatch, so no scan keyed on the dispatch entry point can see them: \
         {offenders:?}. Call verify_dispatch, or take the algorithm through it, so there \
         is one way in."
    );
}

/// Verifier traits that have **no production implementation** (mission `0969-c`).
///
/// Each entry pins the *complete* set of `impl <Trait> for <Type>` type names
/// found anywhere in the workspace's `src/` trees. The gate turns red when a
/// name appears that is not listed — which is the moment somebody adds a real
/// verifier — and when a listed name disappears.
///
/// Every recorded type is a stub living in a `#[cfg(test)]` module.
///
/// **`CapabilityVerifier` was retired from this list.** It shipped a real
/// implementation, `quota_router_core::ingress::capability_verifier::
/// ProductionCapabilityVerifier`, which composes `verify_with_resolve` and
/// `CapabilityToken::ask_binding`. The gate went red on it by itself and named
/// the action, which is what the list is for. `BearerVerifier` remains: its
/// substrate is `BearerCapsule`, which is still a structural stub with no
/// cryptography, so the RFC-0903 path genuinely has no production verifier yet.
///
/// Note that the walk covers `src/` only, so `ProductionCapabilityVerifier` in
/// `octo-wallet/tests/dispatch_tv.rs` — a different, unrelated type of the
/// same name that decodes a token with `unverified_decode_capability` and does
/// no verification — is invisible here. Two types now share that name, which is
/// why the retirement is recorded in words rather than inferred from a count.
///
/// **Why an exact set rather than a count.** The honest obstacle is that a text
/// scan cannot reliably tell a production `impl` from one inside a
/// `#[cfg(test)] mod` in a `src/` file — both look identical on the page, which
/// is the same lesson as the two `chain_depth` concepts in Cairo that a grep
/// could not separate. So the guard does not classify. It pins the whole set,
/// and a human updates it. That turns "I believe these are all stubs" into a
/// claim that fails the moment it stops being true.
///
/// When a real implementation lands, delete the entry rather than adding the
/// name to it. The point of the entry is that the trait has no production impl.
const VERIFIERS_WITHOUT_PRODUCTION_IMPL: &[(&str, &[&str])] = &[(
    "BearerVerifier",
    &[
        "StubBearerVerifier",
        "RejectBearerVerifier",
        "CountingBearer",
        // Test-only, inside a `#[cfg(test)] mod`. Present because the gateway
        // reachability tests need *some* bearer impl to satisfy
        // `GatewayAuthenticator::new`, and a capability-only request must never
        // reach it — so this one panics if called. That is the point: it is a
        // tripwire, not a stand-in.
        //
        // Recorded here rather than named to dodge the guard. A name that trips
        // the guard is the guard working; renaming to evade it would be the
        // opposite of the intent. This is also the first entry added *to* a
        // recorded set rather than one removed from it, and it arrived exactly
        // the way the doc above predicts: the walk reads `src/`, cannot see the
        // `#[cfg(test)]` boundary, and cannot tell this from a production impl.
        "UnusedBearerVerifier",
    ],
)];

#[test]
fn every_verifier_trait_has_a_production_implementation_or_is_accounted_for() {
    let root = workspace_root();
    let Ok(crates) = std::fs::read_dir(root.join("crates")) else {
        panic!("crates/ not readable from {}", root.display());
    };

    // Measured: every `impl <Trait> for <Type>` in the workspace, keyed on the
    // trait name. Collect-all rather than fail-fast so one run reports every
    // trait that drifted.
    let mut measured: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();

    let mut stack: Vec<std::path::PathBuf> =
        crates.flatten().map(|e| e.path().join("src")).collect();
    let mut files_read = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            files_read += 1;
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in strip_comments(&src).lines() {
                let t = line.trim();
                if !t.starts_with("impl ") || !t.contains(" for ") {
                    continue;
                }
                let Some((head, tail)) = t.split_once(" for ") else {
                    continue;
                };
                let trait_name = head
                    .trim_start_matches("impl")
                    .trim_start_matches('<')
                    .split('<')
                    .next()
                    .unwrap_or("")
                    .rsplit("::")
                    .next()
                    .unwrap_or("")
                    .trim();
                let impl_name = tail
                    .split('{')
                    .next()
                    .unwrap_or("")
                    .split('<')
                    .next()
                    .unwrap_or("")
                    .trim();
                if trait_name.is_empty() || impl_name.is_empty() {
                    continue;
                }
                measured
                    .entry(trait_name.to_owned())
                    .or_default()
                    .insert(impl_name.to_owned());
            }
        }
    }

    assert!(
        files_read > 0,
        "the walk read no .rs files under crates/*/src"
    );

    // Only traits the guard claims to account for are checked here. A trait
    // gaining a first production impl is *not* this gate's business — that is
    // the mission landing, not a regression.
    let mut problems: Vec<String> = Vec::new();
    for (trait_name, recorded) in VERIFIERS_WITHOUT_PRODUCTION_IMPL {
        let found = measured.get(*trait_name);
        let Some(found) = found else {
            problems.push(format!(
                "{trait_name}: no `impl` found anywhere, but the guard records {} stub(s). \
                 The trait may have been renamed or removed — update the guard.",
                recorded.len()
            ));
            continue;
        };
        for name in found {
            if !recorded.contains(&name.as_str()) {
                problems.push(format!(
                    "{trait_name}: `{name}` is not in the recorded stub set {recorded:?}. If this \
                     is a real production verifier, DELETE the entry for {trait_name} and record \
                     the retirement — the entry means the trait has no production impl."
                ));
            }
        }
        for name in *recorded {
            if !found.contains(*name) {
                problems.push(format!(
                    "{trait_name}: recorded stub `{name}` no longer exists. If it was replaced by a \
                     real implementation, delete the entry; if it was merely renamed, update it."
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "0969-c: the recorded set of stub-only verifier traits has drifted — {problems:?}"
    );
}

/// Private-notes citations that must not appear in shipped documentation.
///
/// A `[[wikilink]]` resolves inside one developer's working notes and
/// nowhere else. This repository had 222 of them across tracked Rust
/// source, plus more in Markdown — every one a pointer a reader of the
/// repository could not follow. The HARD rule in the project instructions
/// (`no-memory-citations-in-official-docs`) forbids exactly this, and
/// nothing enforced it.
///
/// The guard fails on any occurrence, in any decoration: `[[name]]`,
/// `[name]`, backticked, or bare. An earlier version of this work matched
/// only the double-bracket form and reported 184 when the true count was
/// 222 — a decorated-but-unbracketed citation is still a citation.
const FORBIDDEN_NOTE_CITATIONS: [&str; 11] = [
    "cipherocto-design-principles",
    "stoolap-general-purpose-db",
    "stoolap-fork-persistence",
    "feedback-no-fabricated-commit-rule",
    "deferred-vs-unspecified",
    "feedback_initiation_user_only",
    "no-parallel-abstractions",
    "no-phantom-mission-pointers",
    "no-line-refs-anywhere",
    "grand-design-audit-refactor-2026-08-08",
    "drain-coordinator-approach-2026-08-10",
];

/// Files permitted to name the tokens, because they are the documents the
/// citations now resolve to and they quote the old form as an example of
/// what they replaced — plus this guard, which has to name what it forbids.
///
/// An allowlist, not a directory skip: a new document must be added here
/// deliberately to be exempt, which is the opposite of how a directory
/// exclusion behaves.
///
/// The guard's own exemption is not a loophole in practice. Editing the
/// list above is the only way to make this file violate, and the list is
/// short enough to read in the diff.
const CITATION_ALLOWLIST: [&str; 4] = [
    "docs/architecture/design-principles.md",
    "docs/architecture/stoolap-integration.md",
    "docs/05-process/engineering-rules.md",
    "crates/octo-cli/tests/security_regressions.rs",
];

/// Directories that never contain shipped documentation.
fn is_untracked_by_design(rel: &str) -> bool {
    const SKIP_DIRS: [&str; 5] = [".git", "target", "node_modules", ".claude", ".audit"];
    SKIP_DIRS
        .iter()
        .any(|d| rel == *d || rel.starts_with(&format!("{d}/")))
}

/// Files git tracks, filtered to source and documentation.
///
/// `git ls-files` rather than a directory walk, deliberately. An earlier
/// version of this guard walked the filesystem and reported **448** markdown
/// files against a shell measurement of **288**. The difference was untracked
/// scratch: `docs/reviews/` is gitignored and agent worktrees live under
/// `.claude/`. None of it ships. A guard that counts what will not be
/// published cannot be calibrated, because its denominator moves with whatever
/// happens to be lying around on the machine — which is exactly the mistake
/// this guard exists to prevent elsewhere.
fn tracked_docs(root: &Path) -> Vec<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "*.rs", "*.md"])
        .output()
        .expect("git ls-files must run from the test");
    assert!(
        out.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty() && !is_untracked_by_design(l))
        .map(std::borrow::ToOwned::to_owned)
        .collect()
}

/// No source or documentation file cites a private working note.
///
/// Every **tracked** `.rs` and `.md` file is checked, not just `crates/`:
/// the violations were in `.audit/`, `octo-sync/` and `determin/` as well,
/// and a guard scoped to where you happened to look is the kind that reads
/// as coverage and is not.
#[test]
fn no_source_or_doc_file_cites_a_private_working_note() {
    let root = workspace_root();
    let files = tracked_docs(&root);
    assert!(
        files.len() > 100,
        "only {} tracked source/doc files were found — the guard is not looking \
         at enough of the tree to mean anything",
        files.len()
    );

    let allow: BTreeSet<&str> = CITATION_ALLOWLIST.iter().copied().collect();
    let mut hits: Vec<String> = Vec::new();
    for rel in &files {
        if allow.contains(rel.as_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for token in FORBIDDEN_NOTE_CITATIONS {
            if text.contains(token) {
                hits.push(format!("{rel} mentions `{token}`"));
            }
        }
    }

    let rs_hits: Vec<&String> = hits.iter().filter(|h| h.contains(".rs")).collect();
    assert!(
        rs_hits.is_empty(),
        "Rust source must not cite private notes — {} file(s) do: {rs_hits:?}. \
         Cite `docs/architecture/design-principles.md`, \
         `docs/architecture/stoolap-integration.md` or \
         `docs/05-process/engineering-rules.md` by path and section instead.",
        rs_hits.len()
    );

    // Markdown is a known, measured, unfinished debt rather than a pass.
    // Counting the distinct files every run keeps the number honest and
    // stops it growing silently. This assertion fails loudly when someone
    // adds more; it is not a claim that the sweep is done.
    //
    // Distinct *files*, not (file, token) pairs — a file naming two tokens
    // is still one file to sweep, and counting pairs made the number move
    // for reasons unrelated to how much work remains.
    //
    // 288 → 54. The reduction is the citation sweep. The remaining 54 are
    // almost entirely citations inside `## Version History` and `## Closure`
    // sections, which were deliberately preserved: a dated row recording
    // what was decided then should not be rewritten to cite a document that
    // did not exist yet. They are the correct end state, not outstanding
    // work — but the baseline still has to hold them, because "correctly
    // preserved" and "swept" look identical from here and the difference is
    // the whole point.
    let md_files: BTreeSet<&str> = hits
        .iter()
        .filter_map(|h| h.split(" mentions ").next())
        .filter(|p| p.ends_with(".md"))
        .collect();
    let md_count = md_files.len();
    assert!(
        md_count <= 54,
        "markdown files citing private notes grew past the recorded baseline of 54 \
         (now {md_count}). Either sweep them or, if that is genuinely the plan, raise \
         this number deliberately — do not let it creep."
    );
}

/// Byte offset where a file's test-only region begins, if it has one.
fn test_region_start(text: &str) -> Option<usize> {
    text.find("#[cfg(test)]")
        .into_iter()
        .chain(text.find("mod tests"))
        .min()
}

/// The only files permitted to build a `HolderRecord` outside a test module.
///
/// **Two, and two is the correct number.** RFC-0871 gives the mesh two distinct
/// issuing nodes on distinct payload-kind namespaces — the wallet node serves
/// `WALLET_MINT_CAPABILITY`, the capability issuer node serves
/// `CAPABILITY_ISSUE` — and RFC-0957-A1 §Roles and Authorities gives the Token
/// Issuer, the `IdentityKey` of an issuing node, the authority _"mint + revoke +
/// register"_. Both mint, so both register; neither is a copy of the other.
///
/// Each file builds the record, writes it to its own node's registry, and puts
/// those same canonical bytes in its V2 bundle — which is also what makes the
/// two producers agree byte for byte on the wire (mission `0009-cj`).
///
/// The list is matched exactly in both directions, so deleting a registrant
/// turns this guard red as surely as adding a third one does.
const HOLDER_RECORD_PRODUCTION_FILES: &[&str] = &[
    "crates/octo-wallet-node/src/handlers/mint.rs",
    "crates/octo-capability-issuer-node/src/handlers/issue.rs",
];

/// Every file allowed to call `HolderRecord::from_capability` at all, including
/// from test modules. A new entry is a prompt to record *what registers a
/// holder* — an empty registry means every real capability is refused with
/// `CapError::HolderUnknown`.
///
/// `macaroon.rs` is a test-region entry and the guard caught it the day
/// it was written — the accessor test builds a `HolderRecord` to prove the new
/// `CapabilityCatalog::holder_registry()` reaches one. Recording it is the guard
/// doing its job rather than being worked around.
///
/// Three catches in this arc, which is the evidence that it measures something:
/// `macaroon.rs` (accessor test), then the production registry writes on both
/// issuing nodes, then `revoke.rs` (tests that assert a live record really
/// flips to revoked). Each went red on arrival rather than being discovered by
/// reading this list back.
const HOLDER_RECORD_CONSTRUCTION_FILES: &[&str] = &[
    "crates/octo-cap-macaroon/src/holder_record.rs",
    "crates/octo-cap-macaroon/src/macaroon.rs",
    "crates/octo-ident-storage/src/holder_registry.rs",
    "crates/octo-wallet/src/capability/verify.rs",
    "crates/octo-wallet-node/src/handlers/mint.rs",
    "crates/octo-capability-issuer-node/src/handlers/issue.rs",
    // Test-region only. The revoke handler's tests build a live record so they
    // can assert the registry row actually flips to revoked — a record asserted
    // against a fixture would pass whether or not the handler wrote anything.
    // This is NOT a production construction site and must not be added to
    // `HOLDER_RECORD_PRODUCTION_FILES`; layer two of the guard below is what
    // says so.
    "crates/octo-capability-issuer-node/src/handlers/revoke.rs",
    "crates/quota-router-core/src/ingress/capability_verifier.rs",
];

/// Every production site that builds a `HolderRecord` is one of the two
/// issuing nodes, and the set matches exactly.
///
/// Two layers. Layer one — a file outside
/// [`HOLDER_RECORD_CONSTRUCTION_FILES`] calls it. Layer two — the set of files
/// whose *production region* calls it does not equal
/// [`HOLDER_RECORD_PRODUCTION_FILES`]: a new production site is an unowned
/// record, and a missing one means the registry is empty again, so
/// `ProductionCapabilityVerifier` refuses every real capability with
/// `CapError::HolderUnknown` before the ask is ever read.
#[test]
fn only_the_issuing_nodes_build_a_holder_record_in_production() {
    let mut new_files: Vec<String> = Vec::new();
    let mut production_calls: Vec<String> = Vec::new();

    for path in walk_rs(&workspace_root().join("crates")) {
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let rel = path
            .strip_prefix(workspace_root())
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        // A file under a `tests/` directory is test code by location, not by
        // marker — an integration test has no `mod tests` and no
        // `#[cfg(test)]`, so the marker heuristic would read its whole body as
        // production. Path is a reliable signal precisely here.
        if rel.contains("/tests/") {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !text.contains("HolderRecord::from_capability") {
            continue;
        }

        let production_region = match test_region_start(&text) {
            Some(at) => &text[..at],
            None => &text[..],
        };
        if production_region.contains("HolderRecord::from_capability") {
            production_calls.push(rel.clone());
        }

        if !HOLDER_RECORD_CONSTRUCTION_FILES.contains(&rel.as_str()) {
            new_files.push(rel);
        }
    }

    assert!(
        new_files.is_empty(),
        "a file not recorded in HOLDER_RECORD_CONSTRUCTION_FILES calls \
         HolderRecord::from_capability: {new_files:?}. If it is a real production \
         mint path, add it to the list and record what registers a holder — an \
         empty registry means every real capability is refused with \
         CapError::HolderUnknown."
    );

    production_calls.sort();
    let mut expected: Vec<&str> = HOLDER_RECORD_PRODUCTION_FILES.to_vec();
    expected.sort_unstable();
    assert_eq!(
        production_calls, expected,
        "the set of files building a HolderRecord outside a test module must be \
         exactly HOLDER_RECORD_PRODUCTION_FILES. Growing means an unowned record \
         nobody registers; shrinking means the registry is empty again and \
         ProductionCapabilityVerifier refuses every real capability with \
         CapError::HolderUnknown before the ask is ever read."
    );
}

/// Files where **non-test** `octo-wallet` source still imports
/// `quota_router_storage`.
///
/// RFC-0871 §G9: *"Wallet depends on storage = 0 — Wallet asks nodes via
/// envelope. Node answers. No `quota-router-storage` import in `octo-wallet`."*
///
/// **The gate is currently violated**, and nothing was checking. The four
/// Three production sites, and they are not equally guilty:
///
/// - `capability/verify.rs` — the wallet's own verification substrate using
///   `Clock`, `HolderRegistry` and `RegistryError`. These are substrate types
///   that arguably belong below Layer B rather than in `quota-router-storage`.
/// - `capability/bearer_capsule_re_export.rs` — re-exports `BearerCapsule`.
/// - `bin/octo-wallet.rs` — the `ask publish/list/show/revoke` CLI subcommands.
///   A client asking a node over the envelope is exactly the shape G9 describes,
///   so this one is plausibly compliant in spirit while still being an import.
///
/// Whether they are acceptable is an RFC question. That it was *failing
/// silently* is not — nothing checked G9 at all.
///
/// Pinned rather than asserted empty, so the set can shrink and the gate still
/// says something. Deleting the last entry is the fix.
const WALLET_STORAGE_IMPORT_SITES: &[&str] = &[
    "crates/octo-wallet/src/capability/verify.rs",
    "crates/octo-wallet/src/capability/bearer_capsule_re_export.rs",
    "crates/octo-wallet/src/bin/octo-wallet.rs",
];

/// RFC-0871 §G9 — the wallet does not depend on storage.
///
/// Two layers, because G9 is stated in two places and either can be violated
/// on its own. The manifest can list the dependency while no source uses it,
/// and the source can import it while the manifest has been cleaned.
#[test]
fn the_wallet_crate_does_not_depend_on_storage() {
    // Layer one: non-test source that imports the crate.
    let mut sites: Vec<String> = Vec::new();
    for path in walk_rs(&workspace_root().join("crates/octo-wallet/src")) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let production = match test_region_start(&text) {
            Some(at) => &text[..at],
            None => &text[..],
        };
        if !production.contains("quota_router_storage") {
            continue;
        }
        let rel = path
            .strip_prefix(workspace_root())
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if !WALLET_STORAGE_IMPORT_SITES.contains(&rel.as_str()) {
            sites.push(rel);
        }
    }

    assert!(
        sites.is_empty(),
        "RFC-0871 §G9: non-test octo-wallet source imports quota_router_storage \
         here: {sites:?}. Recorded exceptions are {WALLET_STORAGE_IMPORT_SITES:?} — \
         if a site is new, the gate has been widened."
    );

    // Layer two: the manifest, conditioned on layer one.
    //
    // The recorded sites need the crate, so the dependency is present while
    // they exist. Deriving the check from the census rather than asserting a
    // flat "absent" keeps this guard green today and red when the fix lands —
    // at which point the sites go, and the manifest must follow. Asserting
    // "absent" unconditionally would just be a permanently failing test, which
    // is a note, not a gate.
    let manifest = read("crates/octo-wallet/Cargo.toml");
    let has_dep = manifest
        .split("[dependencies]")
        .nth(1)
        .and_then(|s| s.split("\n[").next())
        .unwrap_or("")
        .contains("quota-router-storage");

    assert_eq!(
        has_dep,
        !WALLET_STORAGE_IMPORT_SITES.is_empty(),
        "RFC-0871 §G9: the manifest and the recorded exception list disagree. \
         The dependency is declared: {has_dep}, while {} site(s) are recorded. \
         Delete the dependency when the last import goes, not before.",
        WALLET_STORAGE_IMPORT_SITES.len()
    );
}

/// Nodes whose `handle_envelope` dispatch arms must each be reachable from a
/// test in the same file.
const NODE_DISPATCH_FILES: &[&str] = &[
    "crates/octo-wallet-node/src/node.rs",
    "crates/octo-capability-issuer-node/src/node.rs",
    "crates/octo-identity-resolver-node/src/node.rs",
    "crates/octo-reputation-anchor-node/src/node.rs",
];

/// Every payload kind a node dispatches must be driven through an envelope in
/// that node's own tests.
///
/// A handler tested only by direct call proves the handler works. It says
/// nothing about whether the node routes to it — a wrong or missing dispatch arm
/// leaves every handler test green. That is not hypothetical: `octo-wallet-node`
/// had **zero** envelope tests for four dispatched payload kinds, so the whole
/// routing layer was untested.
///
/// The same shape let mission `0009-cj`'s defect survive: mint and verify each
/// passed their own tests while nothing spanned the join between them.
///
/// Comparing the production region against the test region catches a new
/// dispatch arm added without a test reaching it.
#[test]
fn every_dispatched_payload_kind_is_driven_through_an_envelope_in_tests() {
    let mut gaps: Vec<String> = Vec::new();

    for rel in NODE_DISPATCH_FILES {
        let path = workspace_root().join(rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            gaps.push(format!("{rel}: could not read"));
            continue;
        };

        let (production, test_region) = match text.find("#[cfg(test)]") {
            Some(at) => (&text[..at], &text[at..]),
            None => {
                gaps.push(format!("{rel}: no test module to check against"));
                continue;
            }
        };

        // Dispatch arms read `k if k == <path>::NAME`. Matching on the module
        // prefix instead would miss kinds whose constant lives elsewhere —
        // `PAID_QUERY_VERIFY` is reached through `octo_paid_query::`, not
        // `octo_protocol::payload_kind::`, and was invisible to this guard for
        // exactly that reason.
        let dispatched: Vec<&str> = production
            .match_indices("k if k == ")
            .map(|(i, _)| &production[i + "k if k == ".len()..])
            .filter_map(|rest| {
                // Bound the token first. Scanning to the end of the file for a
                // `::` segment grabs whatever appears last in the file rather
                // than the identifier under the arm, which silently reduced
                // this guard to extracting nothing at all.
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                    .unwrap_or(rest.len());
                let token = &rest[..end];
                token.rsplit("::").next().filter(|seg| !seg.is_empty())
            })
            // Constant naming, not type naming: `WALLET_SIGN_ED25519` is a
            // payload kind, `PayloadKindId` is the type those constants have.
            .filter(|name| {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            })
            .collect();

        for kind in &dispatched {
            let needle = format!("::{kind}");
            if !test_region.contains(&needle) {
                gaps.push(format!(
                    "{rel}: {needle} is dispatched but no test builds it"
                ));
            }
        }
    }

    assert!(
        gaps.is_empty(),
        "payload kinds dispatched without an envelope-level test: {gaps:?}. A handler \
         reached only by a direct call in tests proves the handler, not the routing. \
         Add a test that builds a NodeEnvelope for the kind and calls handle_envelope."
    );

    // The list of node files is hand-written, and nothing checked it against
    // the thing it exists to cover. Measured today it is complete: exactly
    // four crates in the workspace define `handle_envelope`, and all four are
    // listed. But a fifth node crate, added without touching the list, would
    // dispatch payload kinds with no envelope-level test and this guard would
    // report green — which is the exact failure the guard was written for,
    // one crate over.
    //
    // Same treatment as the reputation store list and the Chrome launcher
    // list: check the list against the rule it claims.
    let mut unscanned: Vec<String> = Vec::new();
    let crates_dir = workspace_root().join("crates");
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        panic!("crates/ must be readable");
    };
    let mut found_any = false;
    for entry in entries.flatten() {
        if !entry.path().join("Cargo.toml").is_file() {
            continue;
        }
        for src in walk_rs(&entry.path().join("src")) {
            let Ok(text) = std::fs::read_to_string(&src) else {
                continue;
            };
            if !text.contains("fn handle_envelope") {
                continue;
            }
            found_any = true;
            let Ok(rel) = src.strip_prefix(workspace_root()) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !NODE_DISPATCH_FILES.iter().any(|f| *f == rel) {
                unscanned.push(rel);
            }
        }
    }
    assert!(
        found_any,
        "no crate defines handle_envelope, so this completeness check is \
         reading nothing and reporting green"
    );
    assert!(
        unscanned.is_empty(),
        "these crates define handle_envelope and are not in NODE_DISPATCH_FILES: \
         {unscanned:?}. Their dispatch arms get no envelope-level test. Add them."
    );
}

/// A mission may assert that its named test vectors exist by exact name.
/// When it does, that assertion is checkable, so it is checked.
const EXACT_VECTOR_NAME_CLAIM: &str = "exists under its specified name";

/// When a mission claims its vectors exist under their specified names, the
/// names must resolve to functions in the tree.
///
/// Three missions in this arc carried a "the vectors exist" claim that did not
/// survive contact with the code: `0957-phase2c` (two of four never written),
/// `0009-e3` (three of five present but red), and `0009-e4` (one of four absent
/// as a function). None was caught by reading the mission.
///
/// **Scope is deliberately narrow.** This only fires on missions that make the
/// exact-name claim, and only on names in the first column of their
/// §Test vectors table. A wider scan over every backticked identifier produces
/// far more hits and they do not mean the same thing — a name that resolves to
/// nothing may have been renamed, which is a different and much weaker finding.
/// Calibration mattered: the first version of this scan reported 51 missions,
/// which was wrong in both directions until the type-name and rename confusions
/// were removed.
///
/// **Measured reach, so the list below is not mistaken for coverage.** Across
/// the mission tree, 4 of 174 `claimed` missions make the claim. `archived`
/// contributes 0 of 302, `open` 0 of 62, and `deferred` 0 of 8 — the four
/// directories that make no claim are therefore not scanned, and adding them
/// would check nothing today. This guard protects four missions. Its value is
/// that those four cannot rot silently and that the next mission making an
/// exact-name claim is checked the moment it is filed under `claimed`.
///
/// A different shape of check was measured and rejected. Finding enforcement-
/// named functions with test references but no production reference yields 43
/// hits, and most are false positives: trait dispatch, macros and re-exports
/// all read as zero textual references. Forty-three findings with a known
/// false-positive mechanism is a census, not a check.
#[test]
fn missions_claiming_named_vectors_have_those_names_in_the_tree() {
    let root = workspace_root();
    let missions_dir = root.join("missions");

    // Every `fn NAME` in the workspace.
    let mut defined: std::collections::HashSet<String> = std::collections::HashSet::new();
    for rs in walk_rs(&root.join("crates")) {
        if let Ok(text) = std::fs::read_to_string(&rs) {
            for line in text.lines() {
                let trimmed = line.trim_start();
                let rest = trimmed
                    .strip_prefix("pub ")
                    .or_else(|| trimmed.strip_prefix("pub(crate) "))
                    .unwrap_or(trimmed);
                let rest = rest.strip_prefix("async ").unwrap_or(rest);
                if let Some(after) = rest.strip_prefix("fn ") {
                    let name: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        defined.insert(name);
                    }
                }
            }
        }
    }

    let mut violations: Vec<String> = Vec::new();
    let mut checked = 0_usize;

    for dir in ["claimed", "archived"] {
        let mut files: Vec<std::path::PathBuf> = walk_rs(&missions_dir.join(dir))
            .into_iter()
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect();
        files.sort();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let lower = text.to_lowercase();
            if !lower.contains(EXACT_VECTOR_NAME_CLAIM) {
                continue;
            }
            let Some(start) = lower.find("## test vectors") else {
                continue;
            };
            let end = text[start..]
                .find("\n## ")
                .map(|i| start + i)
                .unwrap_or(text.len());
            let section = &text[start..end];

            // First cell of each table row is the vector name.
            let mut names: Vec<&str> = Vec::new();
            for row in section.lines() {
                let row = row.trim();
                if !row.starts_with('|') {
                    continue;
                }
                let first = row.trim_matches('|').split('|').next().unwrap_or("");
                for token in first.split('`') {
                    if token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && token.len() > 3
                    {
                        names.push(token);
                    }
                }
            }
            checked += 1;

            let missing: Vec<&str> = names
                .iter()
                .copied()
                .filter(|n| !defined.contains(*n))
                .collect();
            if !missing.is_empty() {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                violations.push(format!("{rel}: {missing:?}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "missions claim their vectors exist under their specified names, but these \
         names resolve to no function in the tree: {violations:?}. Either the test is \
         missing — in which case write it — or it was renamed, in which case update the \
         mission. A ticked criterion and a green vector are different claims; this \
         guard is what keeps them the same one. Scanned {checked} mission(s)."
    );
}

/// Crates that serve node payload kinds and therefore hold a `HandlerOutput`.
const HANDLER_OUTPUT_CRATES: &[&str] = &[
    "crates/octo-wallet-node",
    "crates/octo-capability-issuer-node",
    "crates/octo-identity-resolver-node",
    "crates/octo-reputation-anchor-node",
];

/// Exactly one `HandlerOutput` definition exists, and it is the real one.
///
/// The four node crates each declared their own — byte-identical, three fields,
/// same names — while `octo-protocol` held a placeholder the reference
/// dispatcher echoed into and a comment deferred to "Phase 2/3 node adoption".
/// The substrate was unfinished and every node grew around it.
///
/// §No parallel abstractions: four copies of one type can only agree by luck.
/// A field added to one is silently absent from the other three, and there is no
/// compiler error for it. The protocol type is now the real shape, the four
/// crates re-export it, and this guard fails if a fifth copy appears.
///
/// **Calibration matters here.** A broad version of this scan — every public
/// name defined in more than one crate — reported 365 hits, of which the large
/// majority are `new`, `len`, `verify` and other ordinary method names that say
/// nothing. A check that reports 365 findings and means two is worse than no
/// check, because it trains you to ignore it. This one names the exact type and
/// counts definitions of it, which is a question with a small honest answer.
#[test]
fn exactly_one_handler_output_type_is_defined() {
    let root = workspace_root();

    let mut definitions: Vec<String> = Vec::new();
    for path in walk_rs(&root.join("crates")) {
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // This file names the declaration in order to forbid it.
        if path.ends_with("security_regressions.rs") {
            continue;
        }
        if text.contains("pub struct HandlerOutput") {
            definitions.push(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    definitions.sort();

    assert_eq!(
        definitions,
        vec!["crates/octo-protocol/src/dispatch.rs".to_owned()],
        "HandlerOutput must be defined once, in octo-protocol, and re-exported \
         elsewhere. Found {definitions:?}. A second definition is a parallel \
         abstraction that can only stay in step by luck."
    );

    // And each node crate must be re-exporting, not redeclaring.
    for crate_path in HANDLER_OUTPUT_CRATES {
        let mod_rs = root.join(crate_path).join("src/handlers/mod.rs");
        let text = std::fs::read_to_string(&mod_rs).unwrap_or_default();
        assert!(
            text.contains("pub use octo_protocol::HandlerOutput"),
            "{crate_path} must re-export octo_protocol::HandlerOutput rather than \
             declare its own copy"
        );
    }
}

/// Production code paths that parse auth headers.
const AUTH_HEADER_PARSE_SITES: &[&str] = &["crates/octo-wallet/src/capability/dispatch.rs"];

/// A parsed auth-header set must carry no identity verdict.
///
/// `DispatchSet` used to have an `identity_linkage` field, computed by running
/// both tokens through `unverified_decode_bearer` and
/// `unverified_decode_capability` — decoders that derive a placeholder
/// `subject_did` and `ask_id` from the first bytes of the token string. Two
/// identical token strings produced `LinkageResult::Linked`.
///
/// No production caller read it; `authenticate()` calls `evaluate_linkage` on
/// the verified structs its injected verifiers return. So it was a landmine,
/// not a live bypass — and three tests asserted on it, which is what made it
/// look exercised. A public type carrying a field named like an authentication
/// decision, derived from unverified bytes, is the shape that turns into a real
/// bypass the moment someone reads it.
#[test]
fn a_parsed_auth_header_set_carries_no_identity_verdict() {
    let root = workspace_root();
    let mut findings: Vec<String> = Vec::new();

    for rel in AUTH_HEADER_PARSE_SITES {
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        // Only the production region. The test module legitimately constructs
        // verified structs to exercise `evaluate_linkage` directly.
        let production = text.split("#[cfg(test)]").next().unwrap_or(&text);

        // The field must not exist on the type.
        let struct_start = production.find("pub struct DispatchSet").unwrap_or(0);
        let struct_end = production[struct_start..]
            .find("\n}")
            .map(|i| struct_start + i)
            .unwrap_or(production.len());
        if production[struct_start..struct_end].contains("identity_linkage") {
            findings.push(format!(
                "{rel}: DispatchSet declares an identity_linkage field"
            ));
        }
        // And the parse function must not derive one from unverified decodes.
        if production.contains("unverified_decode_bearer(b)")
            || production.contains("unverified_decode_capability(c)")
        {
            findings.push(format!(
                "{rel}: header parsing calls an unverified decoder; an identity verdict \
                 derived from unverified bytes is not a verdict"
            ));
        }
    }

    assert!(
        findings.is_empty(),
        "a parsed auth-header set must not carry an identity verdict: {findings:?}. \
         Linkage belongs to `evaluate_linkage` over verified structs."
    );
}

/// The module that defines the placeholder decoders. It names them in its own
/// docs, so it is excluded the way a guard excludes its own source.
const UNVERIFIED_DECODER_HOME: &str = "crates/octo-wallet/src/capability/dispatch.rs";

/// No production code may call a placeholder decoder.
///
/// `unverified_decode_bearer` and `unverified_decode_capability` return a
/// `BearerVerification` / `CapabilityVerification` — the same structs a real
/// verifier returns — by deriving a DID and an ask id from the first bytes of
/// the token string. Nothing about the returned value says it was not checked.
///
/// That is exactly the shape that produced the fabricated identity linkage in
/// `DispatchSet`, and the shape survives in the decoders themselves: they are
/// `pub`, and a caller can pass the result straight to `evaluate_linkage`,
/// which cannot tell it apart from a verified one. Production `authenticate()`
/// injects real verifiers, so no live path does this today.
///
/// Test code is not a finding. Both decoders exist so tests can exercise
/// linkage evaluation without a signing key, and suppressing that would push
/// the checks somewhere less honest rather than remove them.
#[test]
fn no_production_code_calls_a_placeholder_decoder() {
    let root = workspace_root();
    let crates = root.join("crates");
    let mut findings: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    let mut stack: Vec<PathBuf> = std::fs::read_dir(&crates)
        .expect("crates/ must be readable")
        .flatten()
        .map(|e| e.path().join("src"))
        .collect();
    while let Some(dir) = stack.pop() {
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(rel) = path.strip_prefix(&root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel == UNVERIFIED_DECODER_HOME {
                continue;
            }
            scanned += 1;
            // Production region only. A `#[cfg(test)]` module legitimately
            // builds unverified structs to drive the linkage logic.
            let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
            let code = strip_comments(production);
            for line in code.lines() {
                let hits = ["unverified_decode_bearer", "unverified_decode_capability"]
                    .iter()
                    .filter(|needle| line.contains(*needle))
                    .count();
                if hits > 0 {
                    findings.push(format!(
                        "{rel}: {hits} placeholder decoder call(s) in production"
                    ));
                }
            }
        }
    }

    assert!(
        scanned > 500,
        "no production source was read, so this guard passed vacuously: {scanned} files"
    );
    assert!(
        findings.is_empty(),
        "production code must not consume a placeholder decoder: {findings:?}. A \
         BearerVerification from these functions is indistinguishable from a verified one."
    );
}
