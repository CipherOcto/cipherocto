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
        let body = fn_body(&src, "async fn verify_governance_suspension");
        assert!(
            body.contains("verify_governance_signature("),
            "C-4: {store} does not verify the governance signature. A governance \
             proof would suspend or slash an arbitrary recorder with no key \
             material."
        );
    }
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
    let calls = auth.matches(".verify(&message, &signature)").count();
    assert_eq!(
        calls, 1,
        "C-4: expected exactly one ed25519 verify site in auth.rs, found {calls}"
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
        // Matched on whitespace-collapsed text because rustfmt wraps the call,
        // and against either binding name because the adapters were fixed by
        // two different code paths.
        let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains(".verify(&envelope.source_peer)")
                || flat.contains(".verify(&env.source_peer)"),
            "M-15: {} must verify against the envelope's own source_peer. A \
             hardcoded key would make the check theatre.",
            file.display()
        );
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
