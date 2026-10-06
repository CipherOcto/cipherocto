//! Mechanical conformance checks over `docs/06-operations/operator-guide.md`.
//!
//! Eight adversarial rounds found 37 divergences in the guide by hand. A
//! hand audit is a poor guard: it decays the moment nobody is looking. This
//! suite is the guard. Every claim the guide makes that can be decided
//! mechanically is decided here, against the real substrate.
//!
//! What this suite deliberately does NOT do: execute the guide's shell
//! procedures. The `octo` CLI cannot provision a node — `octo-wallet init`
//! writes a seed the CLI cannot read, and no CLI subcommand mints a node — so
//! a full executor would report substrate walls as guide defects. That is the
//! same false-positive trap as a naive command sweep. So the suite checks
//! *decidable* claims only, and the wall itself is documented in the guide
//! instead.
//!
//! The thirteen checks, each of which has already caught a real defect:
//!
//! | Check | Caught |
//! | --- | --- |
//! | `guide_commands_all_resolve` | the command layer (round 4) |
//! | `guide_flags_all_resolve` | G8–G12, G16 (rounds 4–5) |
//! | `guide_jq_filters_all_compile` | G31 — a filter that jq cannot parse |
//! | `guide_jq_field_reads_resolve_against_the_payload_layer` | a jq filter
//!   that compiles and prints `null` because no payload struct emits the
//!   field it reads — the defect class a compile check cannot see |
//! | `guide_caveats_expressions_all_parse` | G32–G36 — all 7 mint/attenuate
//!   expressions were rejected by the parser at exit 7 |
//! | `guide_states_no_enum_variant_index` | prose that indexed a clap enum by
//!   ordinal, which silently re-points when a variant is inserted |
//! | `guide_caveat_property_names_match_the_envelope` | caveat property names
//!   that did not match the envelope the mint path actually emits |
//! | `guide_absence_claims_name_something_actually_absent` | a runbook
//!   telling the operator a capability is unavailable when the command tree
//!   has it — the A1 defect, which sends them to a broken workaround |
//! | `guide_documented_env_vars_have_readers` | three `OCTO_ADAPTER_*_CONFIG`
//!   variables the guide told operators to export and no crate has ever read |
//! | `guide_non_octo_invocations_resolve` | a `cargo -p` naming a crate the
//!   workspace excludes, and an executable name that is a package name |
//! | `guide_jq_filters_run_against_real_envelopes` | a filter that compiles
//!   and whose every field exists, yet fails when run — `group_by` applied
//!   to the envelope instead of to `.payload.receipts` |
//! | `guide_build_output_paths_are_real_binaries` | `target/release/…` naming
//!   a package where the binary has another name, which the invocation guard
//!   structurally cannot see because the path sits inside an `export` |
//! | `guide_unit_dispositions_are_complete` | a guide unit with no coverage
//!   disposition, or a disposition citing a test that does not exist |
//!
//! The guide is read from the workspace, not copied here. A copy would rot.
//!
//! Two things about this file are load-bearing and cost a full debugging
//! round each, so they are written down here rather than in a scratchpad:
//!
//! * Offsets into the guide are BYTE offsets, because that is what
//!   `str::find` returns. The guide is full of multibyte UTF-8, so any
//!   char-indexed scan desynchronises from the search that found it and
//!   then reads the wrong text — silently, reporting zero extractions
//!   rather than an error.
//! * An interpolation the resolver cannot resolve is reported, not
//!   replaced with a placeholder. A placeholder would let a genuinely
//!   broken expression pass by accident, which is the failure this suite
//!   exists to catch.
//!
//! The two `#[ignore]`d `diagnostic_dump_*` helpers print what each
//! extractor actually saw. They are how a future round re-diagnoses an
//! extractor that starts finding nothing; they are not tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Locate the guide relative to this crate, so the test works from any cwd.
fn guide_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/06-operations/operator-guide.md")
}

fn guide_text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        std::fs::read_to_string(guide_path())
            .expect("the operator guide must exist and be readable")
    })
    .as_str()
}

/// The binary under test. `CARGO_BIN_EXE_*` is set by cargo for integration
/// tests, so this is the freshly built binary and never a stale `target/`
/// copy that happens to be on `PATH`.
fn octo_bin() -> &'static str {
    env!("CARGO_BIN_EXE_octo")
}

/// The workspace root, so the checks that read sibling crates resolve from
/// any cwd exactly as `guide_path` does for the guide itself.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ---------------------------------------------------------------------------
// Guide line model
// ---------------------------------------------------------------------------

/// A logical guide line: shell line continuations joined, fenced code
/// separated from prose, comments dropped.
///
/// Two jq failure shapes taught the shape of this problem: a shallow array
/// read fails loudly and a shallow scalar read exits 0, so a sweep that
/// only counted failures would miss half the guide. Instead every claim is
/// checked for *being right*, and every sweep is negative-controlled.
struct GuideLine {
    /// 1-based line number of the first physical line, for error messages.
    line: usize,
    /// The command text with continuations joined and comments removed.
    text: String,
}

fn guide_lines() -> &'static Vec<GuideLine> {
    static LINES: OnceLock<Vec<GuideLine>> = OnceLock::new();
    LINES.get_or_init(|| {
        let mut out = Vec::new();
        let mut pending: Option<(usize, String)> = None;
        let mut in_fence = false;

        for (idx, raw) in guide_text().lines().enumerate() {
            let n = idx + 1;
            if raw.trim_start().starts_with("```") {
                // A fence ends any continuation in progress: a command
                // cannot span a code-block boundary.
                if let Some((start, text)) = pending.take() {
                    out.push(GuideLine { line: start, text });
                }
                in_fence = !in_fence;
                continue;
            }
            if !in_fence {
                continue;
            }
            // Strip a trailing continuation and hold the line open.
            if let Some(stripped) = raw.strip_suffix('\\') {
                let piece = stripped.trim_end().to_string();
                match &mut pending {
                    Some((_, acc)) => {
                        acc.push(' ');
                        acc.push_str(piece.trim());
                    }
                    None => pending = Some((n, piece)),
                }
                continue;
            }
            let (start, mut acc) = pending.take().unwrap_or((n, raw.to_string()));
            acc.push(' ');
            acc.push_str(raw.trim());
            // A comment-only fragment contributes no executable claim.
            if !acc.trim_start().starts_with('#') {
                out.push(GuideLine {
                    line: start,
                    text: acc,
                });
            }
        }
        if let Some((start, text)) = pending.take() {
            out.push(GuideLine { line: start, text });
        }
        out
    })
}

// ---------------------------------------------------------------------------
// The real command tree, read from the binary's own help
// ---------------------------------------------------------------------------

/// One node of the clap tree, learned by walking `--help` from the binary
/// rather than by importing the crate. The point is to test what an operator
/// actually gets, so the source of truth must be the shipped surface.
#[derive(Debug, Default, Clone)]
struct CmdNode {
    /// Long flag names this command accepts, without the leading dashes.
    long_flags: BTreeSet<String>,
    /// Short flag names, without the leading dash.
    short_flags: BTreeSet<String>,
    /// Positional argument names, from the `Usage:` line.
    positionals: Vec<String>,
    /// Subcommand names.
    subs: BTreeSet<String>,
    /// True when this command takes no subcommands.
    is_leaf: bool,
}

fn run_help(path: &[String]) -> String {
    let mut cmd = Command::new(octo_bin());
    for p in path {
        cmd.arg(p);
    }
    cmd.arg("--help");
    let out = cmd.output().expect("the octo binary must be runnable");
    // clap prints help to stdout; on an unknown subcommand it prints an
    // error to stderr, which is exactly the signal a bad path produces.
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Parse one `--help` page.
///
/// The grammar is stable enough to key on: a `Commands:` block, an
/// `Options:` block, and a `Usage:` line carrying the positionals and the
/// required flags.
fn parse_help(page: &str) -> CmdNode {
    let mut node = CmdNode::default();
    let mut section = "";

    for line in page.lines() {
        let trimmed = line.trim();
        if trimmed == "Commands:" {
            section = "commands";
            continue;
        }
        if trimmed == "Options:" {
            section = "options";
            continue;
        }
        if trimmed == "Arguments:" {
            section = "arguments";
            continue;
        }
        // A flush-left line that is neither blank nor a section header ends
        // the current section.
        if !line.starts_with(' ') && !trimmed.is_empty() && !trimmed.ends_with(':') {
            section = "";
        }
        if trimmed.is_empty() {
            continue;
        }

        match section {
            "commands" => {
                if line.starts_with("  ") && !line.starts_with("   ") {
                    if let Some(name) = trimmed.split_whitespace().next() {
                        node.subs.insert(name.to_string());
                    }
                }
            }
            "options" => {
                if line.starts_with("  ") && trimmed.starts_with('-') {
                    for token in trimmed.split_whitespace() {
                        if let Some(long) = token.strip_prefix("--") {
                            node.long_flags
                                .insert(long.split('=').next().unwrap_or(long).to_string());
                        } else if let Some(short) = token.strip_prefix('-') {
                            if !short.is_empty() && short.chars().all(|c| c.is_ascii_alphabetic()) {
                                node.short_flags.insert(short.to_string());
                            }
                        }
                    }
                }
            }
            "arguments" if line.starts_with("  ") && !trimmed.starts_with('-') => {
                if let Some(name) = trimmed.split_whitespace().next() {
                    node.positionals.push(name.to_string());
                }
            }
            _ => {}
        }
    }

    // Positionals come from the Usage line, which is the authoritative
    // statement of arity.
    if let Some(usage) = page.lines().find(|l| l.trim_start().starts_with("Usage:")) {
        let rest = usage.trim_start().trim_start_matches("Usage:").trim();
        let mut depth = 0usize;
        let mut current = String::new();
        let mut words: Vec<String> = Vec::new();
        for ch in rest.chars() {
            match ch {
                '<' | '[' => {
                    depth += 1;
                    current.push(ch);
                }
                '>' | ']' => {
                    depth -= 1;
                    current.push(ch);
                }
                c if c.is_whitespace() && depth == 0 => {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                }
                c => current.push(c),
            }
        }
        if !current.is_empty() {
            words.push(current);
        }
        node.positionals = words
            .into_iter()
            .filter(|w| (w.starts_with('<') || w.starts_with('[')) && !w.starts_with("--"))
            .collect();
    }

    node.is_leaf = node.subs.is_empty();
    node
}

/// Walk the whole tree once and cache it.
fn command_tree() -> &'static BTreeMap<String, CmdNode> {
    static TREE: OnceLock<BTreeMap<String, CmdNode>> = OnceLock::new();
    TREE.get_or_init(|| {
        let mut tree: BTreeMap<String, CmdNode> = BTreeMap::new();
        // Breadth-first from the root; the tree is small and the depth is
        // bounded, so an explicit queue is clearer than recursion here.
        let mut queue: Vec<(String, Vec<String>)> = vec![(String::new(), Vec::new())];
        while let Some((key, path)) = queue.pop() {
            let page = run_help(&path);
            let node = parse_help(&page);
            if key.is_empty() {
                // `help` is clap's own, not an operator command.
                let subs: BTreeSet<String> =
                    node.subs.iter().filter(|s| *s != "help").cloned().collect();
                let mut root = node.clone();
                root.subs = subs;
                tree.insert(key.clone(), root);
            } else {
                tree.insert(key.clone(), node.clone());
            }
            for sub in &node.subs {
                if sub == "help" {
                    continue;
                }
                let mut next = path.clone();
                next.push(sub.clone());
                let child_key = if key.is_empty() {
                    sub.clone()
                } else {
                    format!("{key} {sub}")
                };
                queue.push((child_key, next));
            }
        }
        tree
    })
}

// ---------------------------------------------------------------------------
// Check 1 — every command path in the guide exists
// ---------------------------------------------------------------------------

/// Pull the `octo <words…>` prefix out of one guide line, stopping at the
/// first flag, placeholder, or shell metacharacter.
fn octo_prefix(text: &str) -> Option<Vec<String>> {
    let idx = text.find("octo ")?;
    // Reject a match that is part of a longer word, e.g. `octo-wallet `.
    if idx > 0 {
        let prev = text.as_bytes()[idx - 1];
        if prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_' {
            return None;
        }
    }
    let tail = &text[idx + "octo ".len()..];
    let mut words = Vec::new();
    for raw in tail.split_whitespace() {
        let word =
            raw.trim_matches(|c: char| c == '"' || c == '\'' || c == '(' || c == ')' || c == '`');
        if word.is_empty() {
            break;
        }
        // A flag, a placeholder, or a redirection ends the command path.
        if word.starts_with('-')
            || word.starts_with('<')
            || word.starts_with('[')
            || word.starts_with('"')
            || word.starts_with('\'')
            || word.contains('|')
            || word.contains('$')
            || word.contains('>')
            || word.contains('&')
        {
            break;
        }
        if !word.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            break;
        }
        words.push(word.to_string());
        // `octo help <x>` is not an operator path.
        if words.len() > 4 {
            break;
        }
    }
    if words.is_empty() {
        None
    } else {
        Some(words)
    }
}

#[test]
fn guide_commands_all_resolve() {
    let tree = command_tree();
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();

    for entry in guide_lines() {
        let Some(words) = octo_prefix(&entry.text) else {
            continue;
        };
        // The longest prefix that names a real node is the command under
        // test; the words after it are positional values.
        let mut resolved: Option<(String, usize)> = None;
        for take in (1..=words.len()).rev() {
            let key = words[..take].join(" ");
            if tree.contains_key(&key) {
                resolved = Some((key, take));
                break;
            }
        }
        let Some((key, take)) = resolved else {
            bad.push(format!(
                "line {}: no such command: octo {}",
                entry.line,
                words.join(" ")
            ));
            continue;
        };
        checked += 1;

        // A word that looks like a subcommand of the resolved node but is
        // not one of its children is a typo the guide would ship.
        if take < words.len() {
            let node = &tree[&key];
            let next = &words[take];
            if node.is_leaf
                && !node.positionals.iter().any(|p| p.starts_with(next))
                && next.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            {
                // A leaf taking a bare positional word is legitimate; only
                // flag-shaped or placeholder-shaped values are proven wrong
                // elsewhere. Recorded, not failed, to keep the check honest.
                let _ = next;
            }
        }
    }

    assert!(
        checked > 50,
        "the command check must actually see the guide, saw only {checked}"
    );
    assert!(
        bad.is_empty(),
        "unresolvable commands in the guide:\n{}",
        bad.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Check 2 — every flag in the guide is a flag of its own subcommand
// ---------------------------------------------------------------------------

/// The deepest command path on a guide line, as a tree key.
fn resolve_command(text: &str) -> Option<String> {
    let tree = command_tree();
    let words = octo_prefix(text)?;
    (1..=words.len())
        .rev()
        .map(|take| words[..take].join(" "))
        .find(|key| tree.contains_key(key))
}

#[test]
fn guide_flags_all_resolve() {
    let tree = command_tree();
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();

    for entry in guide_lines() {
        // Only lines that actually invoke octo carry flag claims about octo.
        let Some(key) = resolve_command(&entry.text) else {
            continue;
        };
        let Some(node) = tree.get(&key) else {
            continue;
        };
        // Only the octo invocation itself carries octo flag claims. Anything
        // after a pipe belongs to the downstream filter — `--arg` and
        // `--argjson` are jq's, and mistaking them for octo's would report
        // the guide's correct jq usage as a defect.
        let invocation = entry.text.split('|').next().unwrap_or(&entry.text);
        for token in invocation.split_whitespace() {
            let Some(rest) = token.strip_prefix("--") else {
                continue;
            };
            // `--` ends option parsing; `---` is not a flag.
            if rest.is_empty() || rest.starts_with('-') {
                continue;
            }
            // A flag mentioned in prose inside a code comment is not an
            // invocation claim, but this pass only sees code-block lines.
            let name = rest.split('=').next().unwrap_or(rest);
            if name.is_empty() {
                continue;
            }
            checked += 1;
            if !node.long_flags.contains(name) {
                bad.push(format!(
                    "line {}: `octo {key}` has no --{name} (guide: {})",
                    entry.line,
                    entry.text.trim().chars().take(90).collect::<String>()
                ));
            }
        }
    }

    assert!(
        checked > 100,
        "the flag check must actually see the guide, saw only {checked}"
    );
    assert!(
        bad.is_empty(),
        "unknown flags in the guide:\n{}",
        bad.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Check 3 — every jq filter in the guide compiles
// ---------------------------------------------------------------------------

/// One extracted jq invocation: the filter text plus every `--arg` /
/// `--argjson` variable it binds.
struct JqCall {
    line: usize,
    filter: String,
    string_args: Vec<String>,
    json_args: Vec<String>,
}

/// Extract the jq invocations from the guide's fenced shell blocks.
///
/// A jq filter is frequently a single-quoted string spanning several lines
/// with no backslash continuation, so this scans the fenced text directly
/// rather than reusing the line-joined model: shell quoting is the only
/// thing that reliably delimits a filter here.
fn jq_calls() -> &'static Vec<JqCall> {
    static CALLS: OnceLock<Vec<JqCall>> = OnceLock::new();
    CALLS.get_or_init(|| {
        let text = guide_text();
        // Continuations are removed up front rather than handled mid-walk:
        // a backslash-newline can sit anywhere between tokens, including
        // between an option's value and the filter, and a scanner that has
        // to notice it at every position is easy to get subtly wrong. Line
        // numbers are carried across from the ORIGINAL text so an error
        // still points at a line the operator can find.
        let raw: Vec<char> = text.chars().collect();
        let mut bytes: Vec<char> = Vec::with_capacity(raw.len());
        let mut line_of: Vec<usize> = Vec::with_capacity(raw.len());
        let mut line = 1usize;
        let mut k = 0usize;
        while k < raw.len() {
            if raw[k] == '\\' && raw.get(k + 1) == Some(&'\n') {
                // Drop the backslash and the newline; the line counter does
                // not advance, because the two characters are one line.
                k += 2;
                continue;
            }
            bytes.push(raw[k]);
            line_of.push(line);
            if raw[k] == '\n' {
                line += 1;
            }
            k += 1;
        }

        let mut out: Vec<JqCall> = Vec::new();
        let mut i = 0usize;
        // Track fenced regions so a `jq` mentioned in prose is not read as
        // an invocation.
        let mut in_fence = false;
        while i < bytes.len() {
            if bytes[i] == '`' && i + 2 < bytes.len() && bytes[i + 1] == '`' && bytes[i + 2] == '`'
            {
                in_fence = !in_fence;
                i += 3;
                continue;
            }
            // A `jq` word boundary.
            let is_jq = bytes[i] == 'j'
                && i + 2 < bytes.len()
                && bytes[i + 1] == 'q'
                && !bytes[i + 2].is_alphanumeric()
                && (i == 0
                    || !bytes[i - 1].is_alphanumeric()
                        && bytes[i - 1] != '-'
                        && bytes[i - 1] != '_');
            if !(in_fence && is_jq) {
                i += 1;
                continue;
            }
            // `jq` is a call when what follows is a separator, an option
            // dash, or a quote — not when it is the middle of a word such
            // as `jqpath`. A dash is included because `jq -r` is the common
            // form and is just as real a call as `jq '…'`.
            let next = bytes.get(i + 3).copied();
            if matches!(next, Some(c) if c.is_alphanumeric() || c == '_') {
                i += 2;
                continue;
            }

            let start_line = line_of[i];
            // Walk the argument list.
            let mut j = i + 2;
            let mut string_args: Vec<String> = Vec::new();
            let mut json_args: Vec<String> = Vec::new();
            let mut filter: Option<String> = None;
            // jq options take a value, and their value must be consumed
            // before the filter is looked for: in
            // `jq --arg d "$1" '…'` the first quoted string is the
            // variable's value, not the filter. `--arg`/`--argjson` take a
            // NAME and then a VALUE, which is why the two are separate.
            let mut pending_value = false;

            while j < bytes.len() {
                // A backslash-newline continues the command, so the
                // argument list may span lines. The guide wraps most of its
                // long invocations this way.
                if bytes[j] == '\\' && bytes.get(j + 1) == Some(&'\n') {
                    j += 2;
                    continue;
                }
                // Skip whitespace.
                while j < bytes.len() && bytes[j].is_whitespace() {
                    j += 1;
                }
                if j >= bytes.len() {
                    break;
                }
                // A newline with no continuation ends the command.
                if bytes[j] == '\n' {
                    break;
                }
                // A pipe or redirection ends the command.
                if bytes[j] == '|' || bytes[j] == '>' || bytes[j] == '<' {
                    break;
                }
                // An unquoted word: an option flag.
                if bytes[j] == '-' {
                    let word: String = bytes[j..]
                        .iter()
                        .take_while(|c| !c.is_whitespace())
                        .collect();
                    if word == "--arg" || word == "--argjson" {
                        // The variable name is the next bare word.
                        let mut k = j + word.chars().count();
                        while k < bytes.len() && bytes[k].is_whitespace() {
                            k += 1;
                        }
                        let name: String = bytes[k..]
                            .iter()
                            .take_while(|c| !c.is_whitespace())
                            .collect();
                        if word == "--arg" {
                            string_args.push(name.clone());
                        } else {
                            json_args.push(name.clone());
                        }
                        // Consume the NAME as well as the option word, or
                        // the bare-word branch below would eat the name as
                        // if it were the option's value.
                        j = k + name.chars().count();
                        pending_value = true;
                    } else if word.starts_with("--") {
                        // A long option: value-taking unless it is one of
                        // the boolean forms the guide actually uses.
                        pending_value = !matches!(
                            word.as_str(),
                            "--raw-output"
                                | "--slurp"
                                | "--compact-output"
                                | "--sort-keys"
                                | "--raw-input"
                                | "--null-input"
                                | "--tab"
                                | "--join-output"
                        );
                    }
                    j += word.chars().count();
                    continue;
                }
                // A quoted string.
                if bytes[j] == '\'' || bytes[j] == '"' {
                    let quote = bytes[j];
                    let mut k = j + 1;
                    let mut body = String::new();
                    while k < bytes.len() && bytes[k] != quote {
                        // Inside double quotes a backslash escapes the next
                        // character; inside single quotes nothing does.
                        if quote == '"' && bytes[k] == '\\' && k + 1 < bytes.len() {
                            k += 1;
                        }
                        body.push(bytes[k]);
                        k += 1;
                    }
                    if k >= bytes.len() {
                        // Unterminated quote: not a well-formed invocation.
                        break;
                    }
                    if pending_value {
                        pending_value = false;
                    } else if filter.is_none() {
                        filter = Some(body);
                    }
                    j = k + 1;
                    continue;
                }
                // An unquoted bare word: an option value when one is due,
                // otherwise the end of the filter's argument list.
                if pending_value {
                    pending_value = false;
                    j += bytes[j..].iter().take_while(|c| !c.is_whitespace()).count();
                    continue;
                }
                break;
            }

            if let Some(f) = filter {
                out.push(JqCall {
                    line: start_line,
                    filter: f,
                    string_args,
                    json_args,
                });
            }
            i = j.max(i + 2);
        }
        out
    })
}

#[test]
fn guide_jq_filters_all_compile() {
    let calls = jq_calls();
    assert!(
        calls.len() >= 25,
        "the jq check must actually see the guide, saw only {}",
        calls.len()
    );

    let mut bad: Vec<String> = Vec::new();
    for call in calls {
        let mut cmd = Command::new("jq");
        cmd.arg("-n");
        // jq rejects a filter that references an unbound variable, so every
        // `--arg` the guide binds is supplied a dummy. Without this the
        // check would report the guide's correct variable use as a defect.
        for name in &call.string_args {
            cmd.arg("--arg").arg(name).arg("x");
        }
        for name in &call.json_args {
            cmd.arg("--argjson").arg(name).arg("1");
        }
        cmd.arg(&call.filter);
        let out = cmd
            .output()
            .expect("jq must be installed to check the guide");

        // jq's exit codes are the discriminator that makes this check
        // honest, and they were measured rather than assumed:
        //   3 — compile/syntax error: a real guide defect (this is G31)
        //   5 — runtime error, here almost always "cannot iterate over
        //       null" because `-n` feeds the filter a null input. Not a
        //       defect: the same filter is correct against a real envelope.
        //   0 — compiled and ran.
        if out.status.code() == Some(3) {
            let msg = String::from_utf8_lossy(&out.stderr);
            bad.push(format!(
                "line {}: jq cannot compile `{}`\n    {}",
                call.line,
                call.filter.replace('\n', " "),
                msg.lines().next().unwrap_or("").trim()
            ));
        }
    }

    assert!(
        bad.is_empty(),
        "jq filters that do not compile:\n{}",
        bad.join("\n")
    );
}

#[test]
#[ignore]
fn diagnostic_dump_jq() {
    for c in jq_calls() {
        println!(
            "L{} args={:?} json={:?} filter={:?}",
            c.line,
            c.string_args,
            c.json_args,
            c.filter.chars().take(70).collect::<String>()
        );
    }
    // every fenced line that mentions jq at all
    let mut in_fence = false;
    for (i, l) in guide_text().lines().enumerate() {
        if l.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence && l.contains("jq") {
            println!(
                "RAW L{}: {}",
                i + 1,
                l.trim().chars().take(100).collect::<String>()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Check 4 — every --caveats expression parses
// ---------------------------------------------------------------------------

/// Reassemble one shell word from its adjacent quoted segments.
///
/// The guide builds its caveat JSON by concatenating single- and
/// double-quoted pieces, because the values are produced by shell
/// interpolation:
///
///     --caveats '[{"type":"amount_max","value":'"$(dqa16 1000000 6)"'}]'
///
/// Reading only the first quoted segment would yield a truncated, invalid
/// JSON array, so the segments are joined exactly as the shell would join
/// them before the interpolation is resolved.
fn shell_word(text: &str, from: usize) -> Option<(String, usize)> {
    // Offsets here are BYTE offsets, because that is what `str::find`
    // returns. The guide is full of multibyte UTF-8, so a char-indexed
    // scan silently desynchronises from the search that found the flag --
    // and then every extraction is of the wrong text. Slice to a `&str`
    // and walk it as UTF-8 so the two can never disagree again.
    let rest = text.get(from..)?;
    let mut out = String::new();
    let mut segments = 0usize;
    let mut i = 0usize;

    loop {
        let mut j = i;
        while let Some(c) = rest.get(j..).and_then(|s| s.chars().next()) {
            if !c.is_whitespace() {
                break;
            }
            j += c.len_utf8();
        }
        i = j;
        let Some(c) = rest.get(i..).and_then(|s| s.chars().next()) else {
            break;
        };
        if c != '\'' && c != '"' {
            break;
        }
        let quote = c;
        let mut k = i + quote.len_utf8();
        let mut closed = false;
        while let Some(c) = rest.get(k..).and_then(|s| s.chars().next()) {
            if c == quote {
                k += c.len_utf8();
                closed = true;
                break;
            }
            // Inside double quotes a backslash escapes the next character,
            // which is taken literally.
            if quote == '"' && c == '\\' {
                let after = k + c.len_utf8();
                match rest.get(after..).and_then(|s| s.chars().next()) {
                    Some(escaped) => {
                        out.push(escaped);
                        k = after + escaped.len_utf8();
                        continue;
                    }
                    None => break,
                }
            }
            out.push(c);
            k += c.len_utf8();
        }
        if !closed {
            return None;
        }
        segments += 1;
        i = k;
        // The word continues if a backslash-newline follows, OR if the very
        // next character opens another quoted segment. The second case is
        // the one the guide uses: an interpolated value is spliced in as
        // `'…'"$VAULT_CAVEAT"'…'`, three adjacent quoted segments with no
        // backslash between them.
        let mut probe = i;
        while rest.get(probe..).and_then(|s| s.chars().next()) == Some(' ') {
            probe += 1;
        }
        let next = rest.get(probe..).and_then(|s| s.chars().next());
        if next == Some('\\') && rest.get(probe + 1..).and_then(|s| s.chars().next()) == Some('\n')
        {
            i = probe + 2;
            continue;
        }
        if next == Some('\'') || next == Some('"') {
            i = probe;
            continue;
        }
        break;
    }

    if segments == 0 {
        None
    } else {
        Some((out, from + i))
    }
}

/// The 16-byte wire form of a Dqa, in the layout `dqa_from_bytes` reads:
/// 8 bytes big-endian mantissa, then the scale byte, then 7 reserved
/// zeros. This is the same encoding the guide's `dqa16` helper emits, and
/// `guide_dqa16_helper_bytes_round_trip` in the capability suite pins it
/// against the real parser, so the two cannot drift apart silently.
fn dqa16(mantissa: u64, scale: u8) -> String {
    let be = mantissa.to_be_bytes();
    let mut bytes: Vec<String> = be.iter().map(|b| b.to_string()).collect();
    bytes.push(scale.to_string());
    for _ in 0..7 {
        bytes.push("0".to_string());
    }
    format!("[{}]", bytes.join(","))
}

/// A representative 32-byte vault id, standing in for the one
/// `octo vault list` would supply. Any 32 bytes parse, so a fixed value
/// tests the expression's SHAPE without needing a provisioned node.
fn vault_caveat_bytes() -> String {
    let mut bytes: Vec<String> = vec!["170".to_string()];
    for _ in 0..30 {
        bytes.push("0".to_string());
    }
    bytes.push("187".to_string());
    format!("[{}]", bytes.join(","))
}

/// The literal `NAME=value` assignments the guide makes, in source order.
///
/// The guide resolves most of its caveat values through command
/// substitution it cannot execute here, but a plain literal is a claim
/// the guide makes about itself and can be used verbatim: if the guide
/// says `AUDIT_WINDOW_SECS=86400` and then interpolates `$AUDIT_WINDOW_SECS`
/// into a caveat, the resolved expression is the guide's own, not a guess.
fn guide_literal_assignments() -> &'static BTreeMap<String, String> {
    static MAP: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = BTreeMap::new();
        for line in guide_text().lines() {
            let t = line.trim();
            let t = t.strip_prefix("export ").unwrap_or(t);
            let Some(eq) = t.find('=') else { continue };
            let name = t[..eq].trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                continue;
            }
            let mut value = t[eq + 1..].trim().to_string();
            // Drop a trailing comment, then the quote characters.
            if let Some(hash) = value.find(" #") {
                value.truncate(hash);
            }
            map.entry(name.to_string())
                .or_insert_with(|| value.trim_matches('"').trim_matches('\'').to_string());
        }
        map
    })
}

/// Resolve the guide's shell interpolations into literal JSON, or report
/// that an interpolation could not be resolved.
///
/// Returning `None` for an unresolvable one matters: substituting an
/// invented placeholder would let a genuinely broken expression pass by
/// accident, which is the exact failure mode this suite exists to catch.
fn resolve_caveat_interpolations(expr: &str) -> Option<String> {
    let mut out = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '$' && chars.get(i + 1) == Some(&'(') {
            // $(dqa16 <mantissa> <scale>)
            if let Some(close) = chars[i..].iter().position(|c| *c == ')') {
                let inner: String = chars[i + 2..i + close].iter().collect();
                let mut parts = inner.split_whitespace();
                if parts.next() == Some("dqa16") {
                    let m = parts.next().and_then(|v| v.parse::<u64>().ok())?;
                    let s = parts.next().and_then(|v| v.parse::<u8>().ok())?;
                    out.push_str(&dqa16(m, s));
                    i += close + 1;
                    continue;
                }
            }
            return None;
        }
        if chars[i] == '$' {
            let name: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                .collect();
            if name.is_empty() {
                out.push(chars[i]);
                i += 1;
                continue;
            }
            // The one value the guide always derives from a 32-byte id.
            if name == "VAULT_CAVEAT" {
                out.push_str(&vault_caveat_bytes());
            } else {
                out.push_str(guide_literal_assignments().get(&name)?);
            }
            i += name.chars().count() + 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Some(out)
}

#[test]
fn guide_caveats_expressions_all_parse() {
    use octo_cli::commands::capability::parse_caveats;

    let text = guide_text();
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();
    let mut unresolvable: Vec<String> = Vec::new();
    let mut at = 0usize;

    while let Some(idx) = text[at..].find("--caveats") {
        let start = at + idx + "--caveats".len();
        let Some((raw, end)) = shell_word(text, start) else {
            at = start;
            continue;
        };
        at = end;
        // A mention in prose, not an invocation.
        if raw.trim().is_empty() || !raw.trim_start().starts_with('[') {
            continue;
        }
        let Some(resolved) = resolve_caveat_interpolations(&raw) else {
            unresolvable.push(raw.replace('\n', " "));
            continue;
        };
        checked += 1;
        if let Err(e) = parse_caveats(&resolved) {
            bad.push(format!(
                "expression `{}`\n    resolved: {}\n    parser says: {e}",
                raw.replace('\n', " "),
                resolved
            ));
        }
    }

    assert!(
        unresolvable.is_empty(),
        "--caveats expressions with an interpolation this check cannot \
         resolve, so it cannot decide them either:\n{}",
        unresolvable.join("\n")
    );
    assert!(
        checked >= 6,
        "the caveat check must actually see the guide, saw only {checked}"
    );
    assert!(
        bad.is_empty(),
        "--caveats expressions the real parser rejects:\n{}",
        bad.join("\n")
    );
}

/// The guide must not restate a variant's POSITION in `OctoCliError`.
///
/// The enum is `#[non_exhaustive]`, derives only `Error` and `Debug`
/// (no `serde`, no explicit discriminants), and nothing in the
/// workspace reads a variant's index. An index quoted in prose is
/// therefore not a weaker version of a contract — it is not a
/// contract at all, and it goes stale the first time a variant is
/// inserted above it. Two such quotes existed and one was already
/// wrong by two positions.
#[test]
fn guide_states_no_enum_variant_index() {
    let text = guide_text();
    for needle in ["enum index", "variant index", "enum position"] {
        if let Some(at) = text.find(needle) {
            let line_no = text[..at].matches('\n').count() + 1;
            let line = text.lines().nth(line_no - 1).unwrap_or_default();
            panic!(
                "the guide states a variant index at line {line_no} ({needle}). `OctoCliError` is \
                 #[non_exhaustive] with no stable discriminants and nothing reads a variant's \
                 index, so a number quoted beside a variant name is not checkable and drifts on \
                 insertion. Name the variant and its exit code instead. Offending line: {line}"
            );
        }
    }
}

/// The guide's caveat property names must match the envelope the CLI
/// actually emits.
///
/// `guide_jq_filters_all_compile` only proves jq can PARSE a filter. It
/// never resolves `.kind` or `.body` against a real envelope, so a guide
/// that filters on renamed-away properties compiles cleanly and silently
/// returns nothing. That is exactly how the pre-amendment
/// `{"kind", "body"}` spelling survived in the marketplace walkthrough
/// after the summary view moved to `{"type", "value"}`.
///
/// This check ties the two together: the expected key list is written out
/// literally here (NOT read back off the serialised value, which would
/// make the test agree with whatever the code happens to do), and the guide
/// is required to use those names and to contain none of the old ones.
#[test]
fn guide_caveat_property_names_match_the_envelope() {
    use octo_cap_macaroon::Caveat;
    use octo_cli::commands::capability::caveat_view;

    // Independently written expectation. If this list is ever derived from
    // the serialised output instead, the check stops being a check.
    const EXPECTED: [&str; 2] = ["type", "value"];

    let view = caveat_view(&Caveat::Before(1_700_000_000));
    let json = serde_json::to_value(&view).expect("serialise view");
    let keys: Vec<&str> = json
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        EXPECTED.to_vec(),
        "the envelope no longer emits the documented property names; update \
         EXPECTED and the guide together"
    );

    let text = guide_text();

    // The read-back paragraph and the marketplace jq filter must both use
    // the current names.
    for needle in [
        ".type ==",
        "{\"type\", \"value\"}",
        "{\"type\": <short tag>",
    ] {
        assert!(
            text.contains(needle),
            "guide no longer documents the current caveat property names; \
             expected to find {needle:?}"
        );
    }

    // No trace of the pre-amendment spellings may survive in caveat prose
    // or in a caveat jq filter.
    for needle in [".kind ==", ".kind |", ".body |", "{\"kind\", \"body\"}"] {
        assert!(
            !text.contains(needle),
            "guide still filters or documents the pre-amendment caveat \
             property via {needle:?}; the summary view now serialises as \
             type and value"
        );
    }
}

#[test]
#[ignore]
fn diagnostic_dump_caveats() {
    let text = guide_text();
    let mut at = 0usize;
    let mut n = 0usize;
    while let Some(idx) = text[at..].find("--caveats") {
        let start = at + idx + "--caveats".len();
        n += 1;
        let line_no = text[..start].matches('\n').count() + 1;
        match shell_word(text, start) {
            Some((raw, _)) => println!(
                "HIT {n} L{line_no} starts_bracket={} raw={:?}",
                raw.trim_start().starts_with('['),
                raw.chars().take(60).collect::<String>()
            ),
            None => println!("HIT {n} L{line_no} shell_word=None"),
        }
        at = start;
        if n > 25 {
            break;
        }
    }
    println!("total --caveats occurrences: {n}");
}

// ---------------------------------------------------------------------------
// Check 7 — every field the guide's jq filters read is one the payload layer
//           actually emits
// ---------------------------------------------------------------------------
//
// Finding #5, and the `.payload.identities` phantom that the compile check
// walked straight past: `guide_jq_filters_all_compile` proves a filter
// PARSES. Parsing is not resolving. A filter naming a field no payload struct
// emits compiles perfectly, prints `null`, and the guide's next sentence
// tells the operator the value is there.
//
// Counting rules, stated because an unstated rule is how the "how many
// fields does the CLI have" disease starts:
//
//   * A type is IN SCOPE if it derives `Serialize` anywhere under
//     `crates/*/src/`. Deriving `Serialize` is what makes a name reachable
//     over the wire; a type without it cannot be a phantom's excuse.
//   * The PAYLOAD LAYER is the transitive closure, over field types, of the
//     serializable structs DEFINED IN `crates/octo-cli/src/`. Closure, not
//     the CLI crate alone: `VaultListOutput.vaults` is a `Vec<VaultSummary>`
//     and `VaultSummary` lives in `octo-vault`, so an octo-cli-only harvest
//     cannot tell a real `vault_id` from a typo. A name colliding across
//     crates contributes its fields to every candidate rather than picking
//     one, which keeps the check sound in the permissive direction.
//   * A field's JSON NAME is its `#[serde(rename = "…")]` when it has one.
//     This is not optional bookkeeping: the caveat view renames `kind` to
//     `type` and `body` to `value`, and the guide reads `.type` and
//     `.value`. A harvester that ignored renames would report both as
//     phantoms on a perfectly correct guide.
//
// What this pins, and what it deliberately does not:
//
//   * PINNED — every `.field` read in every jq filter is a JSON key the
//     payload layer emits. Flat membership, so it is fully decidable: no
//     opacity, no "could not prove it" escape hatch.
//   * PINNED — for a read of three or more hops, the parent/child adjacency
//     is real SOMEWHERE in the payload layer. Decided only when every
//     candidate parent type is itself known; an unreadable parent yields
//     Skip, never Fail, because a guard that guesses is worse than no guard.
//     The limit of this one is real and worth stating: being type-agnostic,
//     it accepts a chain that happens to hold under a DIFFERENT payload.
//     Planting `records[0].vault_id` fails, because no type pairs `records`
//     with `vault_id`; a pairing some other output happens to have would
//     pass. Closing that needs the command-to-payload map below.
//   * NOT PINNED — which command emits which payload. That mapping lives in
//     74 `OutputEnvelope::new` call sites, 48 of which hand the payload to
//     the envelope as something this harvest cannot resolve to a type
//     without evaluating Rust. A guard resting on a 48-of-74 parse rots
//     silently, so the attribution is left out and the nesting rule is
//     stated type-agnostic instead: a chain must hold SOMEWHERE in the
//     payload layer, not under one particular command.
//
// Mutation evidence, recorded because a guard that has never failed is not a
// guard. Both plants were made in the guide and reverted:
//   * `.payload.does_not_exist` — names every affected line.
//   * `.payload.records[0].vault_id`, a REAL field under the wrong parent —
//     names the line and the parent/child pair that cannot occur.
// The second plant is the one that earned its keep: the first version of
// this check passed it, because the layer set was admitting unparsed type
// NAMES (`String`, `DateTime`, the bare `T`) and the verdict rule reads an
// unreadable type as "unverifiable" — so the junk suppressed every failure.

/// One serializable struct: JSON field name -> element/base type name.
type StructFields = BTreeMap<String, String>;

/// The crate whose serializable structs root the payload layer.
const CLI_CRATE: &str = "octo-cli";

/// Blank out comment bodies while preserving byte length, so brace matching
/// over the result cannot be desynchronised by a brace inside a doc comment.
///
/// This is not hypothetical: `commands/capability.rs` documents its serde
/// renames with `{"type": ..., "value": ...}` in the doc comment directly
/// above the renamed field, and a naive counter stops inside that line and
/// then reads the rest of the crate as struct body.
fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0usize;
    let (mut line, mut block, mut string, mut character, mut raw) =
        (false, false, false, false, false);
    let mut raw_hashes = 0usize;
    while i < b.len() {
        let c = b[i];
        let n = b.get(i + 1).copied();
        if line {
            if c == b'\n' {
                line = false;
            } else {
                out[i] = b' ';
            }
            i += 1;
            continue;
        }
        if block {
            if c == b'*' && n == Some(b'/') {
                out[i] = b' ';
                out[i + 1] = b' ';
                block = false;
                i += 2;
                continue;
            }
            if c != b'\n' {
                out[i] = b' ';
            }
            i += 1;
            continue;
        }
        if character {
            if c == b'\'' {
                character = false;
            } else if c != b'\n' {
                out[i] = b' ';
            }
            i += 1;
            continue;
        }
        if raw {
            if c == b'"' {
                let mut k = i + 1;
                let mut closed = true;
                for _ in 0..raw_hashes {
                    if b.get(k) == Some(&b'#') {
                        k += 1;
                    } else {
                        closed = false;
                        break;
                    }
                }
                if closed {
                    raw = false;
                    i = k;
                    continue;
                }
            }
            if c != b'\n' {
                out[i] = b' ';
            }
            i += 1;
            continue;
        }
        if string {
            if c == b'\\' {
                out[i] = b' ';
                if let Some(x) = b.get(i + 1) {
                    if *x != b'\n' {
                        out[i + 1] = b' ';
                    }
                }
                i += 2;
                continue;
            }
            if c == b'"' {
                string = false;
                i += 1;
                continue;
            }
            if c != b'\n' {
                out[i] = b' ';
            }
            i += 1;
            continue;
        }
        if c == b'/' && n == Some(b'/') {
            line = true;
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            continue;
        }
        if c == b'/' && n == Some(b'*') {
            block = true;
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            continue;
        }
        if c == b'r' && matches!(n, Some(b'"') | Some(b'#')) {
            let mut k = i + 1;
            let mut hashes = 0usize;
            while b.get(k) == Some(&b'#') {
                hashes += 1;
                k += 1;
            }
            if b.get(k) == Some(&b'"') {
                raw = true;
                raw_hashes = hashes;
                i = k + 1;
                continue;
            }
        }
        if c == b'"' {
            string = true;
            i += 1;
            continue;
        }
        if c == b'\'' {
            // A lifetime (`'a`, `&'static str`) and a char literal (`'x'`,
            // `'\n'`) are told apart by width, because guessing wrong on a
            // lifetime swallows every following `'` in the file — and Rust
            // is full of them.
            let wide = b.get(i + 2) == Some(&b'\'')
                || (b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\''));
            if wide {
                character = true;
                i += 3;
                continue;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    // Only ASCII bytes were overwritten, and byte length is unchanged.
    String::from_utf8(out).expect("strip_comments only blanks ASCII bytes")
}

/// The element/base type a field ultimately holds, or `None` when it is a
/// scalar, a tuple, or something this harvester does not model.
fn base_type(raw: &str) -> Option<String> {
    let mut t = raw.trim().trim_end_matches(',').trim();
    for wrapper in ["Vec<", "Option<", "Box<", "Arc<", "Rc<"] {
        if let Some(inner) = t.strip_prefix(wrapper) {
            let inner = inner.trim_end_matches('>').trim();
            // `Vec<Option<Row>>` peels twice; `Vec<Row>` peels once.
            return if inner.contains('<') {
                base_type(inner)
            } else {
                Some(inner.to_string())
            };
        }
    }
    for map in ["BTreeMap<", "HashMap<"] {
        if let Some(inner) = t.strip_prefix(map) {
            // The value type is the last top-level segment.
            let mut depth = 0i32;
            let mut last = String::new();
            for ch in inner.trim_end_matches('>').chars() {
                match ch {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    ',' if depth == 0 => {}
                    _ => last.push(ch),
                }
            }
            let trimmed = last.trim();
            return if trimmed.contains('<') {
                base_type(trimmed)
            } else {
                Some(trimmed.to_string())
            };
        }
    }
    if t.starts_with('(') {
        return None;
    }
    if let Some((head, _)) = t.split_once('<') {
        t = head.trim();
    }
    let last = t.rsplit("::").next().unwrap_or(t).trim();
    if last.is_empty() {
        None
    } else {
        Some(last.to_string())
    }
}

/// Every `.rs` file under a directory, recursively.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Every `Cargo.toml` under a directory, recursively.
///
/// Separate from `collect_rs_files` on purpose: that one filters on the `rs`
/// extension, so asking it for manifests and then filtering the results for
/// `Cargo.toml` yields the empty set. The first draft of this check did
/// exactly that, and reported `octo-wallet` — a real `[[bin]]` name — as an
/// unresolvable command.
fn collect_manifests(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_manifests(&path, out);
        } else if path.file_name().and_then(|s| s.to_str()) == Some("Cargo.toml") {
            out.push(path);
        }
    }
}

/// Harvest every `Serialize`-deriving struct in the workspace, keyed by type
/// name, and remember which crate each was defined in.
fn harvest_serializable_types() -> (BTreeMap<String, StructFields>, BTreeMap<String, String>) {
    let root = workspace_root().join("crates");
    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);

    let mut fields: BTreeMap<String, StructFields> = BTreeMap::new();
    let mut origin: BTreeMap<String, String> = BTreeMap::new();

    for file in &files {
        let Ok(raw) = std::fs::read_to_string(file) else {
            continue;
        };
        let src = strip_comments(&raw);
        let bytes = src.as_bytes();
        let Ok(rel) = file.strip_prefix(&root) else {
            continue;
        };
        let crate_name = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut cursor = 0usize;
        while let Some(at) = src[cursor..].find("#[derive(") {
            let open = cursor + at;
            // Walk the derive list to its closing paren.
            let mut depth = 0usize;
            let mut j = open + "#[derive".len();
            let mut close = None;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            let Some(close) = close else { break };
            if !src[open..=close].contains("Serialize") {
                cursor = close + 1;
                continue;
            }
            // Skip further attributes and visibility, then read the name.
            // `close` is the `)` of the derive list; the attribute's own `]`
            // follows, and starting the scan on it would make every
            // derivation look like an unparseable one.
            let mut k = (close + 2).min(bytes.len());
            let name = loop {
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if src[k..].starts_with("#[") {
                    let mut d = 0i32;
                    while k < bytes.len() {
                        if bytes[k] == b'[' {
                            d += 1;
                        } else if bytes[k] == b']' {
                            d -= 1;
                            if d == 0 {
                                k += 1;
                                break;
                            }
                        }
                        k += 1;
                    }
                    continue;
                }
                if src[k..].starts_with("pub") {
                    let mut m = k;
                    while m < bytes.len() && bytes[m] != b'(' && bytes[m] != b' ' {
                        m += 1;
                    }
                    if m < bytes.len() && bytes[m] == b'(' {
                        while m < bytes.len() && bytes[m] != b')' {
                            m += 1;
                        }
                        m += 1;
                    } else {
                        m = k + 3;
                    }
                    k = m;
                    continue;
                }
                if src[k..].starts_with("struct ") {
                    let start = k + "struct ".len();
                    let mut end = start;
                    while end < bytes.len()
                        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                    {
                        end += 1;
                    }
                    break src[start..end].to_string();
                }
                // Something between the derive and the struct that is not
                // attribute, visibility, or the keyword: give up on this one.
                break String::new();
            };
            cursor = close + 1;
            if name.is_empty() {
                continue;
            }

            // Brace-match the body.
            let body_open = match src[cursor..].find('{') {
                Some(p) => cursor + p,
                None => continue,
            };
            let mut d = 0i32;
            let mut end = body_open;
            while end < bytes.len() {
                match bytes[end] {
                    b'{' => d += 1,
                    b'}' => {
                        d -= 1;
                        if d == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                end += 1;
            }
            if end >= bytes.len() {
                continue;
            }
            origin
                .entry(name.clone())
                .or_insert_with(|| crate_name.clone());

            let body = &src[body_open + 1..end];
            let bb = body.as_bytes();
            let mut p = 0usize;
            while p < bb.len() {
                // A field is `pub NAME: TYPE,`; anything else is skipped by
                // advancing to the next `pub ` at a plausible boundary.
                let Some(rel) = body[p..].find("pub ") else {
                    break;
                };
                let at = p + rel;
                let name_start = at + "pub ".len();
                let mut name_end = name_start;
                while name_end < bb.len()
                    && (bb[name_end].is_ascii_alphanumeric() || bb[name_end] == b'_')
                {
                    name_end += 1;
                }
                if name_end == name_start || bb.get(name_end) != Some(&b':') {
                    p = at + 4;
                    continue;
                }
                let field_name = body[name_start..name_end].to_string();
                // The type runs to the next `pub `, `#[`, or the body's end,
                // with generic commas kept.
                let mut t_end = name_end + 1;
                let mut depth = 0i32;
                while t_end < bb.len() {
                    match bb[t_end] {
                        b'<' => depth += 1,
                        b'>' => depth -= 1,
                        b',' if depth == 0 => break,
                        b'{' => break,
                        _ => {}
                    }
                    t_end += 1;
                }
                let ty_raw = &body[name_end + 1..t_end];
                // An immediately preceding `#[serde(rename = "…")]` wins over
                // the Rust field name, because that is what goes on the wire.
                let mut json_name = field_name;
                let look = at.saturating_sub(200).min(body.len());
                if let Some(rename) = body[look..at].rfind("rename") {
                    let window = &body[look + rename..at];
                    if let Some(open) = window.find('"') {
                        let rest = &window[open + 1..];
                        if let Some(close) = rest.find('"') {
                            json_name = rest[..close].to_string();
                        }
                    }
                }
                let entry = fields.entry(name.clone()).or_default();
                // A name colliding across crates contributes to every
                // candidate rather than overwriting: permissive on
                // structure, still strict on a name that exists nowhere.
                entry
                    .entry(json_name)
                    .or_insert_with(|| base_type(ty_raw).unwrap_or_default());
                p = t_end.max(at + 4);
            }
            cursor = end + 1;
        }
    }
    (fields, origin)
}

/// The transitive closure, over field types, of the serializable structs
/// defined in `crates/octo-cli/src/` — the set of types an `octo` envelope
/// can actually carry — together with the harvested field table it closes
/// over.
fn payload_layer() -> &'static (BTreeSet<String>, BTreeMap<String, StructFields>) {
    static LAYER: OnceLock<(BTreeSet<String>, BTreeMap<String, StructFields>)> = OnceLock::new();
    LAYER.get_or_init(|| {
        let (fields, origin) = harvest_serializable_types();
        let roots: BTreeSet<String> = fields
            .keys()
            .filter(|name| origin.get(*name).map(String::as_str) == Some(CLI_CRATE))
            .cloned()
            .collect();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> = roots.into_iter().collect();
        while let Some(name) = stack.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let Some(f) = fields.get(&name) else { continue };
            for ty in f.values() {
                // Only a type this harvest actually parsed joins the layer.
                // Letting `String`, `DateTime<Utc>` and the bare generic `T`
                // in would make every chain hit an unreadable start type, and
                // the verdict rule below reads an unreadable start as
                // "unverifiable" — so the junk would silently suppress every
                // nesting failure. That is not hypothetical: it is exactly
                // what made a planted `records[0].vault_id` pass.
                if ty.is_empty() || !fields.contains_key(ty) {
                    continue;
                }
                if !seen.contains(ty) {
                    stack.push(ty.clone());
                }
            }
        }
        (seen, fields)
    })
}

/// Every JSON key the payload layer can emit.
fn payload_keys() -> &'static BTreeSet<String> {
    static KEYS: OnceLock<BTreeSet<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let (layer, fields) = payload_layer();
        let mut keys = BTreeSet::new();
        for name in layer {
            if let Some(f) = fields.get(name) {
                keys.extend(f.keys().cloned());
            }
        }
        keys
    })
}

/// Names the guide's jq filters read that are NOT octo payload fields.
///
/// One entry, and it is a producer the guide is not describing: `cargo
/// metadata` emits `packages[].name/.source/.manifest_path`, and no octo
/// struct will ever have them. Naming them here rather than scoping the
/// whole check to `.payload` is deliberate — scoping to `.payload` would
/// have silently dropped the guide's two octo-derived filters that read an
/// already-unwrapped array (`octo audit list` grouped by `router_id`, and
/// the `audit_receipts` shell function's output). An allowlist fails LOUDLY
/// when the guide grows a new external producer; a scope rule fails
/// silently by checking less.
const NON_OCTO_JSON_KEYS: [&str; 3] = ["packages", "source", "manifest_path"];

/// The field reads in one jq filter, grouped into chains: a maximal run of
/// `.name` accesses joined only by subscripts.
///
/// `.payload.vaults[0].vault_id` is one chain of three; the `.label` in
/// `.payload.agents[] | select(.label …)` is its own, because `| select(`
/// is a boundary — that is the whole point of the shape, since the parent
/// of `.label` is an agent row, not the envelope payload.
fn jq_field_chains(filter: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = filter.chars().collect();
    let mut chains: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '.'
            && chars
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
        {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            current.push(chars[start..end].iter().collect());
            i = end;
            continue;
        }
        if chars[i] == '[' {
            // A subscript continues the chain only when it selects without
            // reading anything: `[0]`, `[]`, `["k"]`. `[.[] | …]` reads, so
            // it ends the chain and the reads inside it stand alone.
            let mut depth = 0i32;
            let mut end = i;
            let mut inner = String::new();
            while end < chars.len() {
                match chars[end] {
                    '[' => {
                        depth += 1;
                        if depth > 1 {
                            inner.push('[');
                        }
                    }
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        inner.push(']');
                    }
                    c => {
                        if depth > 0 {
                            inner.push(c);
                        }
                    }
                }
                end += 1;
            }
            let simple = !inner.contains('.') && !inner.contains('|');
            if simple && !current.is_empty() {
                i = end + 1;
                continue;
            }
            if !current.is_empty() {
                chains.push(std::mem::take(&mut current));
            }
            i = end.max(i) + 1;
            continue;
        }
        if !current.is_empty() {
            chains.push(std::mem::take(&mut current));
        }
        i += 1;
    }
    if !current.is_empty() {
        chains.push(current);
    }
    chains
}

/// Whether a chain is a real path through the payload layer.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Ok,
    /// Proven wrong: a known type has the field and the child is not there.
    Fail,
    /// Not decidable from the source alone — never a failure.
    Skip,
}

fn walk_chain(
    here: &str,
    chain: &[String],
    layer: &BTreeSet<String>,
    fields: &BTreeMap<String, StructFields>,
    depth: usize,
) -> Verdict {
    if chain.is_empty() {
        return Verdict::Ok;
    }
    if depth > 12 {
        return Verdict::Skip;
    }
    let Some(f) = fields.get(here) else {
        return Verdict::Skip;
    };
    let Some(next) = f.get(&chain[0]) else {
        return Verdict::Fail;
    };
    let rest = &chain[1..];
    if rest.is_empty() {
        return Verdict::Ok;
    }
    if next.is_empty() {
        return Verdict::Skip;
    }
    // A bare generic parameter stands for "whatever payload this envelope
    // carries", so it fans out across the layer. `OutputEnvelope<T>.payload`
    // is the only way a guide filter starts with `payload`, and treating it
    // as one concrete type is what made an early draft of this check report
    // every chain as unverifiable.
    if next.len() == 1 && next.chars().all(|c| c.is_ascii_uppercase()) {
        let mut saw_skip = false;
        let mut saw_fail = false;
        for candidate in layer {
            match walk_chain(candidate, rest, layer, fields, depth + 1) {
                Verdict::Ok => return Verdict::Ok,
                Verdict::Skip => saw_skip = true,
                Verdict::Fail => saw_fail = true,
            }
        }
        return if saw_fail && !saw_skip {
            Verdict::Fail
        } else {
            Verdict::Skip
        };
    }
    walk_chain(next, rest, layer, fields, depth + 1)
}

#[test]
fn guide_jq_field_reads_resolve_against_the_payload_layer() {
    let (layer, fields) = payload_layer();
    let keys = payload_keys();

    // Anti-silent-zero, and the lesson this file already paid for once: a
    // scan that finds nothing reports zero extractions rather than an
    // error, and a check that passes on zero extractions is not a check.
    assert!(
        layer.len() > 100,
        "the payload layer collapsed to {} types — the harvest is broken, not the guide",
        layer.len()
    );
    assert!(
        keys.len() > 250,
        "only {} payload keys harvested; the guard would be vacuous",
        keys.len()
    );

    let mut unknown: Vec<String> = Vec::new();
    let mut misnested: Vec<String> = Vec::new();
    let mut wrong_root: Vec<String> = Vec::new();
    let mut checked_reads = 0usize;
    let mut checked_chains = 0usize;
    let mut checked_roots = 0usize;
    let mut unwrapped_filters = 0usize;

    for call in jq_calls() {
        let chains = jq_field_chains(&call.filter);

        // R12 — the LAYER check this guard is named for.
        //
        // The name check below proves every hop is a field SOME payload struct
        // emits. It does not prove the filter reads those fields from the
        // right place, and it cannot: `metadata` is a real field of
        // `crates/octo-cli/src/commands/network.rs`, so `.metadata.did`
        // satisfied every hop while addressing a path the envelope does not
        // have. Two-hop chains also fall below the three-hop threshold of the
        // nesting check, so nothing else caught it.
        //
        // A filter that reads the envelope must root at `payload`. The two
        // exemptions are the shapes that genuinely do not:
        //   - a chain rooted at a NON_OCTO_JSON_KEY, i.e. the guide's one
        //     `cargo metadata` filter over an external producer;
        //   - a filter with no chains at all, i.e. one that reads an
        //     already-unwrapped array (`[.[] | {receipt_id, …}]`).
        // Both are counted, so an exemption cannot become a blind spot.
        if let Some(first) = chains.first() {
            checked_roots += 1;
            let root = first[0].as_str();
            if root != "payload" && !NON_OCTO_JSON_KEYS.contains(&root) {
                wrong_root.push(format!(
                    "guide L{}: `.{}` reads the envelope at `{}`, but the envelope's \
                     only data layer is `payload` (filter: {})",
                    call.line,
                    first.join("."),
                    root,
                    call.filter
                ));
            }
        } else {
            unwrapped_filters += 1;
        }

        for chain in chains {
            checked_chains += 1;
            for name in &chain {
                checked_reads += 1;
                if keys.contains(name) || NON_OCTO_JSON_KEYS.contains(&name.as_str()) {
                    continue;
                }
                unknown.push(format!(
                    "guide L{}: `.{}` is not a field any payload struct emits  (filter: {})",
                    call.line,
                    chain.join("."),
                    call.filter
                ));
            }
            // One hop is a name; two is an attribute of the envelope. Only a
            // three-hop chain says anything about NESTING.
            if chain.len() < 3 {
                continue;
            }
            let mut saw_ok = false;
            let mut saw_skip = false;
            for start in layer.iter() {
                match walk_chain(start, &chain, layer, fields, 0) {
                    Verdict::Ok => {
                        saw_ok = true;
                        break;
                    }
                    Verdict::Skip => saw_skip = true,
                    Verdict::Fail => {}
                }
            }
            if !saw_ok && !saw_skip {
                misnested.push(format!(
                    "guide L{}: `.{}` nests wrongly — no payload type has a field `{}` \
                     whose element type carries `{}`  (filter: {})",
                    call.line,
                    chain.join("."),
                    chain[chain.len() - 2],
                    chain[chain.len() - 1],
                    call.filter
                ));
            }
        }
    }

    assert!(
        checked_reads > 20,
        "the field check saw only {checked_reads} reads across {checked_chains} chains; \
         it must actually be reading the guide"
    );
    // Anti-vacuity for the layer check specifically. Without a floor, an
    // extractor regression that returns no chains at all would make the root
    // assertion pass on nothing — the exact silent-zero failure this file has
    // already paid for once. The exemptions are counted too, so the two
    // shapes that legitimately skip the check cannot grow to cover the guide.
    assert!(
        checked_roots + unwrapped_filters >= 8,
        "the layer check saw only {checked_roots} rooted filters and \
         {unwrapped_filters} unwrapped ones; the jq extractor has regressed and \
         the payload-root assertion would be vacuous"
    );
    assert!(
        wrong_root.is_empty(),
        "the guide reads the envelope at a layer that does not exist:\n{}",
        wrong_root.join("\n")
    );
    assert!(
        unknown.is_empty(),
        "the guide reads fields the payload layer does not emit:\n{}",
        unknown.join("\n")
    );
    assert!(
        misnested.is_empty(),
        "the guide reads fields at a nesting the payload layer does not have:\n{}",
        misnested.join("\n")
    );
}

#[test]
#[ignore]
fn diagnostic_dump_payload_layer() {
    let (layer, fields) = payload_layer();
    let keys = payload_keys();
    println!("payload layer types: {}", layer.len());
    println!("payload layer keys:  {}", keys.len());
    let mut per_type: Vec<(String, usize)> = layer
        .iter()
        .map(|t| (t.clone(), fields.get(t).map_or(0, |f| f.len())))
        .collect();
    per_type.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (name, n) in per_type.iter().take(15) {
        println!("  {n:3}  {name}");
    }
    let mut reads = 0usize;
    let mut chains = 0usize;
    for call in jq_calls() {
        for chain in jq_field_chains(&call.filter) {
            chains += 1;
            reads += chain.len();
        }
    }
    println!("guide jq chains: {chains}, field reads: {reads}");
    for call in jq_calls() {
        for chain in jq_field_chains(&call.filter) {
            println!("  L{} {}", call.line, chain.join(" -> "));
        }
    }
}

// ---------------------------------------------------------------------------
// Check 8 — every "X is NOT wired" claim names something that really is absent
// ---------------------------------------------------------------------------
//
// Finding #4, and the guard that was missing when A1 had to be fixed by hand.
// `guide_commands_all_resolve` proves the commands the guide SHOWS work. It
// says nothing about the commands the guide says DO NOT work, which is the
// more dangerous direction: a runbook that tells an operator a capability is
// unavailable sends them to a workaround, and the workaround is what breaks.
//
// Counting rule, stated because the number is the claim:
//   * A CLAIM is a guide line carrying the `[SUBSTRATE-NEW]` marker that
//     contains a backticked `octo …` token AND a negation phrase AFTER that
//     token. "After" is load-bearing: several markers quote a real command
//     as the substrate-faithful workaround, and reading the sentence rather
//     than the position would charge the guide for its own advice.
//   * A claim is a COMMAND claim when the token names a path, and a FLAG
//     claim when it also names a `--flag`. A flag claim is not weaker: if the
//     path does not exist, the flag cannot either.
//
// Coverage, and the limit of it. Of the 93 `[SUBSTRATE-NEW]` markers this
// check decides 58. The other 35 assert something else — enum variant
// counts, flags on an `Args` struct, "rlib-only", "no migrations binary" —
// each of which needs its own substrate read, and none of which is claimed
// here. Those markers are NOT verified by this test; the count above is the
// honest
// denominator for what is.
//
// Mutation evidence: re-introducing the exact A1 defect — rewriting a marker
// to claim `octo identity list` is NOT wired — makes this fail, naming the
// line and the command that resolves anyway. Restored, it passes.

/// A negation phrase that makes a following `octo` token a claim of absence.
const ABSENCE_PHRASES: [&str; 7] = [
    "not wired",
    "not yet wired",
    "NOT wired",
    "no such",
    "does NOT have",
    "has NO ",
    "is not ",
];

/// One `[SUBSTRATE-NEW]` claim that some command is unavailable.
struct AbsenceClaim {
    line: usize,
    /// The command path, without the leading `octo`.
    command: String,
    /// The flag the claim names, if it names one.
    flag: Option<String>,
}

fn absence_claims() -> Vec<AbsenceClaim> {
    let mut out = Vec::new();
    for (idx, line) in guide_text().lines().enumerate() {
        if !line.contains("SUBSTRATE-NEW") {
            continue;
        }
        // The first backticked token naming `octo `.
        let Some(open) = line.find("`octo ") else {
            continue;
        };
        let after_open = open + 1;
        let Some(close) = line[after_open..].find('`') else {
            continue;
        };
        let token = &line[after_open..after_open + close];
        let text_after = &line[after_open + close + 1..];
        if !ABSENCE_PHRASES.iter().any(|p| text_after.contains(p)) {
            continue;
        }
        // The path runs until a flag or a placeholder.
        let path: String = token
            .strip_prefix("octo ")
            .unwrap_or_default()
            .split(['-', '<', '>', '['])
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if path.is_empty()
            || !path
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-')
        {
            continue;
        }
        let flag = token
            .split_whitespace()
            .find(|w| w.starts_with("--"))
            .map(|w| w.trim_end_matches(['.', ',', ')']).to_string());
        out.push(AbsenceClaim {
            line: idx + 1,
            command: path,
            flag,
        });
    }
    out
}

#[test]
fn guide_absence_claims_name_something_actually_absent() {
    let tree = command_tree();
    let claims = absence_claims();
    let mut false_claims: Vec<String> = Vec::new();

    for claim in &claims {
        if tree.contains_key(&claim.command) {
            // The command exists. If the claim also named a flag, the flag
            // has to be checked before the claim can be called false — a
            // claim that a real command lacks a real flag is TRUE.
            if let Some(flag) = &claim.flag {
                let node = &tree[&claim.command];
                let bare = flag.trim_start_matches("--");
                if node.long_flags.contains(bare) {
                    false_claims.push(format!(
                        "guide L{}: claims `octo {} {}` is NOT wired, but the command \
                         exists AND accepts {}",
                        claim.line, claim.command, flag, flag
                    ));
                }
            } else {
                false_claims.push(format!(
                    "guide L{}: claims `octo {}` is NOT wired, but it resolves in the \
                     command tree",
                    claim.line, claim.command
                ));
            }
        }
    }

    assert!(
        claims.len() >= 50,
        "only {} absence claims extracted; the extractor is broken, not the guide",
        claims.len()
    );
    assert!(
        false_claims.is_empty(),
        "the guide claims a capability is unavailable that is in fact wired:\n{}",
        false_claims.join("\n")
    );
}

#[test]
#[ignore]
fn diagnostic_dump_absence_claims() {
    let tree = command_tree();
    let claims = absence_claims();
    println!("absence claims extracted: {}", claims.len());
    let mut resolved = 0usize;
    for claim in &claims {
        let hit = tree.contains_key(&claim.command);
        if hit {
            resolved += 1;
        }
        println!(
            "  L{:<5} {:<44} flag={:<18} resolves={}",
            claim.line,
            format!("octo {}", claim.command),
            claim.flag.clone().unwrap_or_else(|| "-".into()),
            hit
        );
    }
    println!("claims naming a command that resolves: {resolved}");
    // The other marker shapes this check does not decide, so the gap is
    // visible rather than assumed.
    let mut other = 0usize;
    for line in guide_text().lines() {
        if line.contains("SUBSTRATE-NEW") {
            let covered = line
                .find("`octo ")
                .map(|open| {
                    let after = open + 1;
                    line[after..]
                        .find('`')
                        .map(|close| {
                            ABSENCE_PHRASES
                                .iter()
                                .any(|p| line[after + close + 1..].contains(p))
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if !covered {
                other += 1;
            }
        }
    }
    println!("markers NOT decided by this check: {other}");
}

// ---------------------------------------------------------------------------
// Check 9 — every env var the guide documents is read by the code
// ---------------------------------------------------------------------------
//
// Finding #8. A runbook that documents an env var nothing reads is worse than
// one that documents nothing: the operator sets it, the adapter ignores it,
// and the failure surfaces later as missing behaviour with no cause.
//
// Counting rule:
//   * A DOCUMENTED VAR is a bare `OCTO_*` / `CIPHEROCTO_*` token appearing in
//     the guide, matched against the whole guide INCLUDING comments — these
//     claims live in `#` comments, and `guide_lines()` drops exactly those.
//   * The token must be a complete name: `OCTO_ADAPTER_<NAME>_CONFIG` is a
//     placeholder and `OCTO_ADAPTER_` is its fragment, so neither is a var.
//   * A token on a line that also carries a NEGATION ("there is no X") is a
//     correction, not a documentation, and is skipped. That is the guide
//     stating a var does not exist, which is the opposite of the defect.
//
// Readers are counted as a QUOTED LITERAL in `env::var` / `var_os` /
// `env!` / `option_env!`. A bare textual mention is not a reader: the guide's
// own `OCTO_BACKUP_DIR` appears inside a shell script embedded in a Rust
// test, and matching that text would make the check pass on a variable no
// Rust code has ever read.

/// Documented env vars that are deliberately not read by Rust, with the
/// reason each one is legitimate. An entry here is a claim a human makes,
/// and the diagnostic prints the list so a stale entry is visible.
const ENV_VARS_WITHOUT_RUST_READERS: [(&str, &str); 2] = [
    (
        "CIPHEROCTO_MNEMONIC",
        "a GitHub Actions secret name, `${{ secrets.… }}`",
    ),
    (
        "OCTO_BACKUP_DIR",
        "a shell `export` in the guide's own recovery blocks",
    ),
];

/// Phrases that make a mention a correction rather than a documentation.
const NEGATION_CUES: [&str; 6] = [
    "there is no ",
    "there are no ",
    "does not exist",
    "not a real",
    "no such var",
    "never existed",
];

/// Every `.rs` file under `crates/`, concatenated, for reader lookup.
///
/// RAW source, deliberately not `strip_comments` output: the thing being
/// searched for is `env::var("OCTO_HOME")`, and the comment stripper blanks
/// string-literal BODIES as well as comments — which turns that call into
/// `env::var(            )` and reports every real env var as unread. The
/// cost of using raw text is that a reader mentioned only inside a comment
/// would count as a reader; that fails toward a MISSED defect, never a
/// false alarm, which is the right direction for this guard.
fn crate_source_text() -> &'static String {
    static SRC: OnceLock<String> = OnceLock::new();
    SRC.get_or_init(|| {
        let mut files = Vec::new();
        collect_rs_files(&workspace_root().join("crates"), &mut files);
        let mut joined = String::new();
        for file in files {
            if let Ok(text) = std::fs::read_to_string(&file) {
                joined.push_str(&text);
                joined.push('\n');
            }
        }
        joined
    })
}

fn env_var_is_read(name: &str) -> bool {
    let src = crate_source_text();
    for reader in [
        format!("env::var(\"{name}\")"),
        format!("var(\"{name}\")"),
        format!("var_os(\"{name}\")"),
        format!("env!(\"{name}\")"),
        format!("option_env!(\"{name}\")"),
    ] {
        if src.contains(&reader) {
            return true;
        }
    }
    false
}

#[test]
fn guide_documented_env_vars_have_readers() {
    let mut documented: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, line) in guide_text().lines().enumerate() {
        // Scanned as BYTES, never as char-indexed slices: the guide is full of
        // multibyte UTF-8 (arrows, box drawing), and slicing mid-character
        // panics — or, worse, silently rescans the wrong span. This is the
        // same trap the module header warns about, reached the other way.
        let bytes = line.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let rest = &bytes[i..];
            let token_len = if rest.starts_with(b"CIPHEROCTO_") {
                "CIPHEROCTO_".len()
            } else if rest.starts_with(b"OCTO_") {
                "OCTO_".len()
            } else {
                i += 1;
                continue;
            };
            // `end` is RELATIVE to `i` throughout, because `token_len` is.
            let mut end = token_len;
            while i + end < bytes.len()
                && (bytes[i + end].is_ascii_uppercase()
                    || bytes[i + end].is_ascii_digit()
                    || bytes[i + end] == b'_')
            {
                end += 1;
            }
            // A line can END with the bare prefix (`…=CIPHEROCTO_`), in which
            // case the name runs past the end of the line.
            if end >= bytes.len() - i {
                i += 1;
                continue;
            }
            // A complete name: no trailing `_`, no `__`, and at least two
            // underscore-separated segments after the prefix. This is what
            // separates `OCTO_HOME` from the `OCTO_ADAPTER_` fragment and
            // from the `OCTO_ADAPTER_<NAME>_CONFIG` placeholder.
            let raw = &bytes[i..i + end];
            let name = String::from_utf8_lossy(raw).into_owned();
            let segments: Vec<&str> = name.split('_').filter(|s| !s.is_empty()).collect();
            let complete = !name.ends_with('_')
                && !name.contains("__")
                && name.len() > token_len
                && segments.len() >= 2;
            if complete && !NEGATION_CUES.iter().any(|c| line.contains(c)) {
                documented.entry(name).or_insert(idx + 1);
            }
            i += end.max(1);
        }
    }

    let allowlisted: BTreeSet<&str> = ENV_VARS_WITHOUT_RUST_READERS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    let mut unread: Vec<String> = Vec::new();
    for (name, line) in &documented {
        if env_var_is_read(name) || allowlisted.contains(name.as_str()) {
            continue;
        }
        unread.push(format!(
            "guide L{line}: `{name}` is documented but no code reads it"
        ));
    }

    // The allowlist must earn its place: an entry for a var that nothing
    // documents, or that something now reads, is dead weight that hides the
    // next real defect.
    for (name, reason) in ENV_VARS_WITHOUT_RUST_READERS {
        assert!(
            documented.contains_key(name),
            "allowlisted env var `{name}` ({reason}) is no longer documented by the guide; \
             drop the entry"
        );
        assert!(
            !env_var_is_read(name),
            "allowlisted env var `{name}` now HAS a reader; drop the entry so the \
             guard checks it"
        );
    }

    assert!(
        documented.len() >= 8,
        "only {} env vars documented; the extractor is broken, not the guide",
        documented.len()
    );
    assert!(
        unread.is_empty(),
        "the guide documents env vars that nothing reads:\n{}",
        unread.join("\n")
    );
}

#[test]
#[ignore]
fn diagnostic_dump_env_vars() {
    let mut all: BTreeSet<String> = BTreeSet::new();
    for line in guide_text().lines() {
        for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if token.starts_with("OCTO_") || token.starts_with("CIPHEROCTO_") {
                all.insert(token.to_string());
            }
        }
    }
    println!("env-var-shaped tokens in the guide: {}", all.len());
    for name in &all {
        let complete = !name.ends_with('_') && !name.contains("__") && name.len() >= 5;
        println!(
            "  {name:38} complete={complete:<6} read={}",
            env_var_is_read(name)
        );
    }
    println!("allowlist:");
    for (name, reason) in ENV_VARS_WITHOUT_RUST_READERS {
        println!("  {name:38} {reason}");
    }
}

// ---------------------------------------------------------------------------
// Check 10 — the non-`octo` invocations resolve too
// ---------------------------------------------------------------------------
//
// Finding #2. `octo_prefix` deliberately rejects a match embedded in a longer
// word, so every `cargo …`, `octo-wallet …` and `quota-router …` line sits
// outside the command guard. That gap is not theoretical: it held a build
// command naming a crate the workspace EXCLUDES, and an executable name that
// is a package name rather than a binary.
//
// Counting rule: a CHECKED INVOCATION is a column-0 line in a shell-ish
// fenced block (`bash`, `sh`, or unlabelled) whose first word is not a
// comment, a shell keyword, `octo` (check 1 owns it), or an entry in
// EXTERNAL_BINARIES. Fences tagged `mermaid` / `yaml` are skipped outright —
// their column-0 words (`graph`, `jobs`, `runs-on`) are not commands, and
// reading them as such is how a guard starts crying wolf.
//
// Two shapes are then decided:
//   * `cargo … -p <pkg>` — `<pkg>` must be a workspace MEMBER. `-p` selects
//     members only, so naming an excluded crate is a build failure.
//   * any other binary — must be a name some crate declares as a `[[bin]]`,
//     or a shell function the guide itself defines above its first use.
//
// Mutation evidence: restoring `cargo build --release -p
// octo-telegram-onboard`, and restoring `quota-router-cli route`, each fail
// this check by name. Both were fixed in the guide, not exempted.

/// Binaries the guide legitimately shells out to that this workspace does not
/// build. Each is a claim a human makes; the diagnostic prints the list.
const EXTERNAL_BINARIES: [&str; 54] = [
    "awk",
    "base64",
    "basename",
    "cat",
    "cd",
    "chmod",
    "cmp",
    "cp",
    "curl",
    "cut",
    "date",
    "df",
    "diff",
    "dirname",
    "docker",
    "dqa16",
    "du",
    "echo",
    "env",
    "find",
    "git",
    "grep",
    "gpg",
    "head",
    "hostname",
    "id",
    "jq",
    "kill",
    "ls",
    "make",
    "mkdir",
    "mktemp",
    "mv",
    "nc",
    "openssl",
    "ping",
    "printf",
    "ps",
    "readlink",
    "realpath",
    "rm",
    "sed",
    "sha256sum",
    "sleep",
    "test",
    "timeout",
    "tar",
    "tr",
    "uname",
    "wait",
    "watch",
    "which",
    "xargs",
    "yes",
];

/// Column-0 words that are shell syntax rather than a command.
const SHELL_KEYWORDS: [&str; 36] = [
    "break", "case", "continue", "declare", "def", "do", "done", "elif", "else", "esac", "eval",
    "exec", "exit", "export", "fi", "for", "function", "if", "in", "let", "local", "readonly",
    "return", "set", "shift", "then", "trap", "typeset", "until", "while", "{", "}", "fi;", "EOT",
    "EOF", "ES",
];

/// True when a line's first word is a shell variable ASSIGNMENT rather than a
/// command — `VAULT_ID="$(octo …)"` runs `octo`, not `VAULT_ID`.
///
/// This is not a cosmetic exclusion. An earlier draft of the extractor
/// matched only lowercase words and so skipped these lines by accident, which
/// quietly removed 20-odd of the guide's real invocations from coverage while
/// still reporting a plausible checked-count.
fn is_shell_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Every `[[bin]]` name any workspace crate declares.
fn workspace_bin_names() -> &'static BTreeSet<String> {
    static BINS: OnceLock<BTreeSet<String>> = OnceLock::new();
    BINS.get_or_init(|| {
        let mut names = BTreeSet::new();
        let mut files = Vec::new();
        collect_manifests(&workspace_root().join("crates"), &mut files);
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for slice in text.split("[[bin]]").skip(1) {
                let head: String = slice.chars().take(400).collect();
                for line in head.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("name") {
                        if let Some(open) = rest.find('"') {
                            let after = &rest[open + 1..];
                            if let Some(close) = after.find('"') {
                                names.insert(after[..close].to_string());
                            }
                        }
                        break;
                    }
                }
            }
        }
        names
    })
}

/// Workspace package names, read from cargo itself rather than from a
/// hand-maintained list that would drift.
fn workspace_members() -> &'static BTreeSet<String> {
    static MEMBERS: OnceLock<BTreeSet<String>> = OnceLock::new();
    MEMBERS.get_or_init(|| {
        let mut names = BTreeSet::new();
        if let Ok(out) = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(workspace_root())
            .output()
        {
            if let Ok(text) = String::from_utf8(out.stdout) {
                // Parsed, not scanned: a flat `find("\"name\":\"")` walk also
                // matches dependency and target names, which is a set that
                // drifts the moment the lockfile moves.
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(packages) = json.get("packages").and_then(|p| p.as_array()) {
                        for pkg in packages {
                            if let Some(name) = pkg.get("name").and_then(|n| n.as_str()) {
                                names.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }
        names
    })
}

/// Shell functions the guide defines, so a call to one is not read as a
/// missing executable.
fn guide_shell_functions() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in guide_text().lines() {
        let t = line.trim();
        let Some(close) = t.find("()") else { continue };
        let name = t[..close].trim();
        let after = t[close + 2..].trim_start();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && after.starts_with('{')
        {
            out.insert(name.to_string());
        }
    }
    out
}

/// Walk the guide's shell fences, yielding `(line_no, command_text)` for
/// every column-0 command line.
fn column_zero_commands() -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut shell = false;
    let mut heredoc: Option<String> = None;
    for (idx, raw) in guide_text().lines().enumerate() {
        let line = raw.trim();
        if let Some(info) = raw.strip_prefix("```") {
            heredoc = None;
            if in_fence {
                in_fence = false;
                shell = false;
            } else {
                in_fence = true;
                let lang = info.split_whitespace().next().unwrap_or("");
                shell = !matches!(lang, "mermaid" | "yaml" | "toml" | "json" | "text");
            }
            continue;
        }
        // A heredoc BODY is data, not commands: the guide writes install
        // scripts inline, and reading `/usr/local/bin/cipherocto-backup.sh`
        // out of one as an invocation the operator would run is a false alarm
        // about a line that is quoted text.
        if let Some(terminator) = &heredoc {
            if line == terminator.as_str() {
                heredoc = None;
            }
            continue;
        }
        if !in_fence || !shell || line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `cat > f <<'EOF'` opens a heredoc; `<<-EOF` allows indented bodies.
        if let Some(at) = line.find("<<") {
            // `<<'EOF'` quotes the terminator, which is the common form, so
            // the quote has to come off before the word is readable.
            let rest = line[at + 2..]
                .trim_start_matches(['-', ' '])
                .trim_start_matches(['\'', '"'])
                .trim_start_matches(['-', ' ']);
            let word: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !word.is_empty() && !rest.starts_with('&') {
                heredoc = Some(word);
            }
        }
        // Column-0 only: a word partway along a line is an argument.
        if raw.starts_with(' ') || raw.starts_with('\t') {
            continue;
        }
        if let Some(raw) = line.split_whitespace().next() {
            let mut binary = raw
                .trim_end_matches(|c: char| {
                    c == ':' || c == ';' || c == ')' || c == '{' || c == '}'
                })
                .to_string();
            // `audit_receipts() {` is the DEFINITION of a function the guide
            // defines, and the first call to it must not be read as a call to
            // an executable that does not exist.
            if let Some(stripped) = binary.strip_suffix('(') {
                binary = stripped.to_string();
            }
            // `$QUOTA_ROUTER_BIN route` names a path the guide computes; there
            // is nothing static to resolve, and inventing a check here would
            // only manufacture false alarms.
            if binary.starts_with('$') {
                continue;
            }
            // `(octo … && octo …) &` is subshell grouping: the `octo` inside
            // it is found by `octo_prefix`, which searches the whole line, so
            // treating the `(` as a command name would only duplicate it.
            if binary.starts_with('(') {
                continue;
            }
            // `AGENT_ID_LEDGER="$HOME/agent-ids.txt"` is an ASSIGNMENT, and
            // this ordering matters: strip the basename FIRST and the
            // assignment becomes `agent-ids.txt"`, which reads as a missing
            // executable.
            if is_shell_assignment(&binary) {
                continue;
            }
            // `./target/release/octo-whatsapp-onboard` is the same binary as
            // `octo-whatsapp-onboard`; the declared `[[bin]]` name is the
            // last path segment, not the whole invocation.
            if let Some((_, tail)) = binary.rsplit_once('/') {
                binary = tail.to_string();
            }
            out.push((idx + 1, binary));
        }
    }
    out
}

#[test]
fn guide_non_octo_invocations_resolve() {
    let bins = workspace_bin_names();
    let members = workspace_members();
    let functions = guide_shell_functions();
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for (line_no, binary) in column_zero_commands() {
        if binary.is_empty()
            || is_shell_assignment(&binary)
            || binary == "octo"
            || SHELL_KEYWORDS.contains(&binary.as_str())
            || EXTERNAL_BINARIES.contains(&binary.as_str())
            || functions.contains(&binary)
        {
            continue;
        }
        let Some(text) = guide_text().lines().nth(line_no - 1) else {
            continue;
        };
        if binary == "cargo" {
            let mut rest = text.to_string();
            while let Some(at) = rest.find("-p ") {
                let after = &rest[at + 3..];
                let pkg: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                if pkg.is_empty() {
                    break;
                }
                checked += 1;
                if !members.contains(&pkg) {
                    bad.push(format!(
                        "guide L{line_no}: `cargo … -p {pkg}` cannot work — `{pkg}` is not \
                         a workspace member, and `-p` selects members only"
                    ));
                }
                rest = after[pkg.len()..].to_string();
            }
            continue;
        }
        checked += 1;
        if !bins.contains(&binary) {
            bad.push(format!(
                "guide L{line_no}: `{binary}` is not a binary any workspace crate \
                 declares (`[[bin]] name`), not a guide-defined function, and not \
                 an allowlisted external tool"
            ));
        }
    }

    assert!(
        members.len() > 50,
        "workspace membership read failed ({} packages); the check would be vacuous",
        members.len()
    );
    assert!(
        checked >= 15,
        "only {checked} non-`octo` invocations checked; the extractor is broken"
    );
    assert!(
        bad.is_empty(),
        "the guide invokes commands that do not resolve:\n{}",
        bad.join("\n")
    );
}

#[test]
#[ignore]
fn diagnostic_dump_invocations() {
    let bins = workspace_bin_names();
    let members = workspace_members();
    let functions = guide_shell_functions();
    println!("workspace bins: {}", bins.len());
    println!("workspace members: {}", members.len());
    println!("guide-defined functions: {functions:?}");
    println!("allowlisted externals: {}", EXTERNAL_BINARIES.len());
    let mut checked = 0usize;
    for (line_no, binary) in column_zero_commands() {
        if binary.is_empty()
            || is_shell_assignment(&binary)
            || binary == "octo"
            || SHELL_KEYWORDS.contains(&binary.as_str())
            || EXTERNAL_BINARIES.contains(&binary.as_str())
            || functions.contains(&binary)
        {
            continue;
        }
        checked += 1;
        let text = guide_text().lines().nth(line_no - 1).unwrap_or("");
        if binary == "cargo" {
            println!(
                "  L{line_no} cargo  {}",
                text.chars().take(72).collect::<String>()
            );
        } else {
            println!("  L{line_no} {binary:<22} bin={}", bins.contains(&binary));
        }
    }
    println!("checked: {checked}");
    let _ = members;
}

// ---------------------------------------------------------------------------
// Check 11 — every guide unit has a stated coverage disposition
// ---------------------------------------------------------------------------
//
// Finding #6, and the discipline its own audit tripped over. The audit
// reported "25 of 36 units uncovered" and the number was wrong in BOTH
// directions: it counted §1 as uncovered, which
// `l4_audit_switch_denies_writes_despite_an_explicit_write_mode` covers
// directly, and it derived coverage from tests *citing* a `§N` rather than
// tests *executing* the behaviour — which misses §3 and §5, covered by tests
// that never name their section.
//
// So the count is not left to prose. §33's disposition table is the single
// source of truth, this check parses it, and it fails three ways:
//
//   1. a unit exists in the guide but has no row in the table (silent gap);
//   2. the table has a row for a unit the guide no longer has (stale row);
//   3. a row names a test that does not exist in the tree (dead disposition,
//      the failure mode that makes a coverage table worse than none).
//
// It deliberately does NOT verify that a `covered` row's test is *adequate* —
// that is a judgement, and pretending otherwise is how a coverage table
// becomes a number. It verifies the table is complete, current, and that
// every claim in it points at something real.

/// The dispositions §33 may assign. `Blocked` is not a coverage level; it
/// records that the unit documents a path the substrate does not implement,
/// and the row must then name the blocker.
const DISPOSITIONS: [&str; 5] = ["covered", "prose", "multi-node", "integration", "blocked"];

/// Every `#[test]` / `#[tokio::test]` / `#[test]` fn name in the tree.
fn all_test_names() -> &'static BTreeSet<String> {
    static NAMES: OnceLock<BTreeSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut out = BTreeSet::new();
        let mut files = Vec::new();
        collect_rs_files(&workspace_root().join("crates"), &mut files);
        for file in files {
            if !file.to_string_lossy().contains("/tests/")
                && !file.to_string_lossy().contains("/src/")
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for line in text.lines() {
                let t = line.trim();
                for prefix in ["fn ", "async fn "] {
                    if let Some(rest) = t.strip_prefix(prefix) {
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        // The prefixes are this repo's test-naming
                        // conventions. `guide_` is this file's own: the
                        // guide-conformance checks are real tests, and a
                        // `covered` row is allowed to cite one — §12's does.
                        // The first version of this list omitted the prefix,
                        // so the disposition guard rejected a TRUE claim,
                        // which is the right failure for the wrong reason.
                        const PREFIXES: [&str; 6] = ["test_", "tv_", "l3_", "l4_", "t_", "guide_"];
                        if PREFIXES.iter().any(|p| name.starts_with(p)) {
                            out.insert(name);
                        }
                    }
                }
            }
        }
        out
    })
}

/// The guide's units, in document order, keyed by their `§N` number or
/// `A`/`B` for the appendices.
fn guide_units() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in guide_text().lines() {
        let Some(title) = line.strip_prefix("## ") else {
            continue;
        };
        let title = title.trim();
        if let Some(rest) = title.strip_prefix('§') {
            let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !num.is_empty() {
                out.push((num, title.to_string()));
                continue;
            }
        }
        if title.to_ascii_lowercase().starts_with("appendix") {
            let key = title
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .trim_end_matches(['—', '-', ' '])
                .to_string();
            let letter: String = key
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            if !letter.is_empty() {
                out.push((letter, title.to_string()));
            }
        }
    }
    out
}

#[test]
fn guide_unit_dispositions_are_complete() {
    // 1. The table's rows, parsed from §33.
    let text = guide_text();
    let start = text
        .find("### Coverage disposition")
        .expect("§33 must carry the coverage disposition table");
    let table = &text[start..];

    // Bound the scan to THIS section. Without the bound the parse runs to
    // end-of-file and swallows the Appendix flag tables, whose second column
    // is prose — the first version of this check did exactly that and
    // reported a phantom unit "read".
    let table = match table.find("\n## ") {
        Some(at) => &table[..at],
        None => table,
    };

    let mut rows: BTreeMap<String, String> = BTreeMap::new();
    for line in table.lines() {
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }
        let unit = cells[0];
        // Skip the header and the rules row.
        if unit.starts_with("---") || unit.eq_ignore_ascii_case("unit") {
            continue;
        }
        let key: String = if let Some(rest) = unit.strip_prefix('§') {
            rest.chars().take_while(|c| c.is_ascii_digit()).collect()
        } else if unit.to_ascii_lowercase().starts_with("appendix") {
            // Appendix rows are "Appendix A — <title>"; A is the key.
            unit.split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect()
        } else {
            // Not a unit row. The previous version fell through to the
            // Appendix branch for EVERY non-`§` row and took its second word
            // as a unit name, so any explanatory table placed after the
            // disposition table — including the tier table in §C3 — reported
            // phantom units ("container", "cross", "two"). Only two unit
            // spellings exist: `§N` and `Appendix X`. Anything else is prose
            // and is skipped rather than guessed at.
            continue;
        };
        if key.is_empty() {
            continue;
        }
        rows.insert(key, cells[1].to_string());
    }

    // 2. Every unit in the guide needs a row.
    let units = guide_units();
    assert!(
        units.len() == 36,
        "the unit count moved: {} units parsed (expected 36). Either the guide \
         gained a section or the parsing rule is wrong — re-derive, do not adjust \
         the expectation silently",
        units.len()
    );

    let mut missing: Vec<String> = Vec::new();
    for (key, title) in &units {
        if !rows.contains_key(key) {
            missing.push(format!("§{key} / {title}"));
        }
    }
    assert!(
        missing.is_empty(),
        "guide units with NO coverage disposition — silent gaps:\n{}",
        missing.join("\n")
    );

    // 3. No stale rows for units that no longer exist.
    let unit_keys: BTreeSet<&String> = units.iter().map(|(k, _)| k).collect();
    let stale: Vec<&String> = rows.keys().filter(|k| !unit_keys.contains(k)).collect();
    assert!(
        stale.is_empty(),
        "the disposition table has rows for units the guide no longer has: {stale:?}"
    );

    // 4. Every disposition is a known level, and every test it names exists.
    let names = all_test_names();
    let mut problems: Vec<String> = Vec::new();
    for (key, disposition) in &rows {
        let clean = disposition.trim_matches('*').trim();
        if !DISPOSITIONS.contains(&clean) {
            problems.push(format!(
                "§{key}: disposition {disposition:?} is not one of {DISPOSITIONS:?}"
            ));
        }
    }
    // The `held by` cell is the third column; re-parse it alongside.
    for line in table.lines() {
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }
        let key: String = if let Some(rest) = cells[0].strip_prefix('§') {
            rest.chars().take_while(|c| c.is_ascii_digit()).collect()
        } else {
            cells[0]
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect::<String>()
        };
        if key.is_empty() || cells[1].eq_ignore_ascii_case("unit") || cells[0].starts_with("---") {
            continue;
        }
        if cells[1].trim_matches('*').trim() != "covered" {
            continue;
        }
        // Trim non-identifier characters from BOTH ends. Trimming only the
        // front leaves the closing backtick of `name` (trailing prose), the
        // "not an identifier, skip it" rule then swallows the row, and a
        // typo'd test name passes silently — which is precisely what the
        // first version of this check did when handed
        // `l12_settlement_verifies_end_to_end` (does not exist).
        let mut cited = 0usize;
        for token in cells[2].split(',') {
            let name = token
                .split('(')
                .next()
                .unwrap_or_default()
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .to_string();
            if name.is_empty() {
                continue;
            }
            cited += 1;
            if !names.contains(&name) {
                problems.push(format!(
                    "§{key}: claims `covered` by `{name}`, which is not a test in the tree"
                ));
            }
        }
        if cited == 0 {
            problems.push(format!(
                "§{key}: disposition is `covered` but names no test, so nothing holds it"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "the coverage disposition table has stale or unresolvable claims:\n{}",
        problems.join("\n")
    );

    // 5. The headline number is derived, not asserted.
    let covered = rows
        .values()
        .filter(|d| d.trim_matches('*').trim() == "covered")
        .count();
    println!(
        "guide units: {} | with a disposition: {} | covered: {} | open: {}",
        units.len(),
        rows.len(),
        covered,
        rows.len() - covered
    );
}

#[test]
#[ignore]
fn diagnostic_dump_unit_dispositions() {
    let units = guide_units();
    println!("units: {}", units.len());
    let start = guide_text()
        .find("### Coverage disposition")
        .expect("table present");
    let table = &guide_text()[start..];
    for line in table.lines() {
        let t = line.trim();
        if t.starts_with('|') && !t.starts_with("|---") && !t.to_lowercase().contains("| unit |") {
            let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
            if cells.len() >= 3 {
                // Truncation by CHARACTERS, not bytes. The guide is full of
                // multibyte UTF-8 (em dashes, `§`, box drawing), so
                // `&cell[..cell.len().min(60)]` slices mid-character and
                // panics — which killed every diagnostic in this file on
                // `cargo test -- --ignored`, hiding the other seven.
                let short: String = cells[2].chars().take(60).collect();
                println!("  {:52} {:14} {}", cells[0], cells[1], short);
            }
        }
    }
    let _ = units;
}

// ---------------------------------------------------------------------------
// Check 12 — the guide's jq filters RUN, not merely compile
// ---------------------------------------------------------------------------
//
// R5's lesson, generalised. `guide_jq_filters_all_compile` proves a filter
// PARSES, and `guide_jq_field_reads_resolve_against_the_payload_layer` proves
// every field it reads is a field the payload layer emits. Both pass a
// filter that is wrong in the most consequential way available: applied to
// the wrong LEVEL of the document.
//
// §12 step 6 was exactly that. Its rollup read
//
//     octo audit list --since 1d --json | jq 'group_by(.router_id) | …'
//
// `.router_id` is a real field and `group_by(…)` is a real jq builtin, so
// both existing checks passed it. Run against a real envelope it fails:
//
//     jq: error: Cannot index string with string "router_id"
//
// because `group_by` was applied to the envelope OBJECT rather than to
// `.payload.receipts`. An operator following the guide's own "Verify" step
// gets a jq error scroll past them.
//
// This check closes the gap by EXECUTING. For every one-line
// `octo … | jq '<filter>'` in the guide's shell fences, it runs the command
// against a real wallet and pipes the real stdout through the real filter,
// requiring jq to exit 0.
//
// What it does NOT do, stated so the number is not oversold: it only runs the
// filters whose producer is a single `octo` invocation that succeeds on a
// freshly registered identity. Filters over files, over the
// `audit_receipts` helper, or over commands needing a passphrase or
// interactive input are SKIPPED, and each skip is printed with its exit code
// so the skipped set is visible rather than assumed. A filter that is never
// executed is a filter whose runtime behaviour is unverified — which is the
// honest statement, and the reason the skip list is printed rather than
// swallowed.

/// A guide one-liner of the shape `octo <args…> … | jq '<filter>'`, which is
/// the only shape this check can execute end to end.
struct RunnableFilter {
    line: usize,
    command: Vec<String>,
    filter: String,
}

fn runnable_guide_filters() -> Vec<RunnableFilter> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut shell = false;
    for (idx, raw) in guide_text().lines().enumerate() {
        let line = raw.trim();
        if let Some(info) = raw.strip_prefix("```") {
            in_fence = !in_fence;
            if in_fence {
                let lang = info.split_whitespace().next().unwrap_or("");
                shell = !matches!(lang, "mermaid" | "yaml" | "toml" | "json" | "text");
            } else {
                shell = false;
            }
            continue;
        }
        if !in_fence || !shell || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with("octo ") || !line.contains("| jq '") {
            continue;
        }
        let Some(open) = line.find("| jq '") else {
            continue;
        };
        let command_part = line[..open].trim();
        let filter_part = &line[open + "| jq '".len()..];
        let Some(close) = filter_part.rfind('\'') else {
            continue;
        };
        let filter = &filter_part[..close];

        let command: Vec<String> = command_part
            .split_whitespace()
            .map(str::to_string)
            .collect();
        // A command carrying a placeholder cannot be executed as written.
        if command.iter().any(|a| a.starts_with('<')) {
            continue;
        }
        out.push(RunnableFilter {
            line: idx + 1,
            command,
            filter: filter.to_string(),
        });
    }
    out
}

#[test]
fn guide_jq_filters_run_against_real_envelopes() {
    // jq is already a hard requirement of this suite — the compile check
    // shells out to it — so its absence here is a real problem, not a skip.
    assert!(
        Command::new("jq").arg("--version").output().is_ok(),
        "jq must be on PATH: this check executes the guide's filters with it, \
         and `guide_jq_filters_all_compile` already depends on it too"
    );

    // A real wallet, so the commands reach stage 3 (`active_owner_did`)
    // instead of exiting 2 and skipping everything.
    let home = tempfile::tempdir().expect("tempdir");
    let path = home.path();
    let passphrase = path.join("pass.txt");
    std::fs::write(&passphrase, "correct-horse-battery-staple-passphrase-01\n")
        .expect("write passphrase");
    let register = Command::new(octo_bin())
        .env("OCTO_HOME", path)
        .env("NO_COLOR", "1")
        .env("OCTO_FORCE_JSON", "1")
        .args([
            "identity",
            "register",
            "--label",
            "jq-runtime-probe",
            "--passphrase-file",
            passphrase.to_str().expect("utf-8"),
            "--confirm",
            "--confirm-acknowledge",
        ])
        .output()
        .expect("run octo identity register");
    assert!(
        register.status.success(),
        "could not provision a wallet to run the guide's filters against; \
         stderr: {}",
        String::from_utf8_lossy(&register.stderr)
    );

    let filters = runnable_guide_filters();
    assert!(
        filters.len() >= 8,
        "only {} runnable filters extracted; the extractor is broken, not the guide",
        filters.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut executed = 0usize;

    for case in &filters {
        let mut cmd = Command::new(octo_bin());
        cmd.env("OCTO_HOME", path)
            .env("NO_COLOR", "1")
            .env("OCTO_FORCE_JSON", "1")
            // `case.command` starts with the word `octo` because that is how
            // the guide writes it; the binary is already that executable, so
            // passing the token through makes it `octo octo mesh …` and clap
            // exits 2 — which the first run of this check reported as "all
            // ten commands failed", when in fact every one of them was fine.
            .args(case.command.iter().skip(1));
        if !case.command.iter().any(|a| a == "--json") {
            cmd.arg("--json");
        }
        let produced = cmd.output().expect("run guide command");
        if !produced.status.success() {
            skipped.push(format!(
                "guide L{}: `{}` exited {}",
                case.line,
                case.command.join(" "),
                produced.status.code().unwrap_or(-1)
            ));
            continue;
        }
        executed += 1;
        let mut jq = Command::new("jq");
        jq.arg(&case.filter);
        let out = jq
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child
                    .stdin
                    .as_mut()
                    .ok_or_else(|| std::io::Error::other("no stdin"))?
                    .write_all(&produced.stdout)?;
                child.wait_with_output()
            });
        match out {
            Ok(result) if result.status.success() => {}
            Ok(result) => failures.push(format!(
                "guide L{}: `jq '{}'` failed at RUNTIME on the real output of \
                 `{}` (exit {}): {} — the filter compiles and every field it \
                 reads exists, so only execution catches this",
                case.line,
                case.filter,
                case.command.join(" "),
                result.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&result.stderr).trim()
            )),
            Err(e) => failures.push(format!("guide L{}: could not run jq: {e}", case.line)),
        }
    }

    // Skips are reported, never silently dropped: a filter nobody ran is a
    // filter whose runtime behaviour is unverified.
    for skip in &skipped {
        println!("SKIPPED {skip}");
    }
    println!(
        "jq runtime check: {executed} executed, {} skipped, {} failed",
        skipped.len(),
        failures.len()
    );

    assert!(
        executed >= 6,
        "only {executed} filters actually ran; the check is not exercising the guide"
    );
    assert!(
        failures.is_empty(),
        "guide jq filters that fail when run against real output:\n{}",
        failures.join("\n")
    );
}

#[test]
#[ignore]
fn diagnostic_dump_runnable_filters() {
    let filters = runnable_guide_filters();
    println!(
        "runnable `octo … | jq '<filter>'` one-liners: {}",
        filters.len()
    );
    for case in &filters {
        println!("  L{} {}", case.line, case.command.join(" "));
        println!("      jq '{}'", case.filter);
    }
}

// ---------------------------------------------------------------------------
// Check 13 — every build-output path in the guide names a real binary
// ---------------------------------------------------------------------------
//
// The R4 claim was that `guide_non_octo_invocations_resolve` holds the
// "package name used as an executable" defect. It does not. That check
// classifies a column-0 word, and the surviving instance of the defect is not
// a column-0 word:
//
//     export QUOTA_ROUTER_BIN="$PWD/target/release/quota-router-cli"
//
// `export` is a shell keyword, so the invocation guard skipped the line —
// correctly, since `export` is not a command — and the wrong path went
// unnoticed. Ten downstream `"$QUOTA_ROUTER_BIN" …` invocations depended on
// it. Claiming a guard held a defect it structurally cannot see is worse
// than having no claim, so this is a separate check with a narrower question.
//
// RULE: a BUILD-OUTPUT PATH is any `target/<profile>/<name>` in the guide.
// Such a path promises the operator a file at that exact location, so
// `<name>` must be a `[[bin]]` some workspace crate declares. The package
// name is not a substitute: `quota-router-cli` is the package and
// `quota-router` is the binary, and only the second is ever written to
// `target/release/`.
//
// This is the smallest rule that catches the defect, and it is decidable
// from the manifests without building anything.

#[test]
fn guide_build_output_paths_are_real_binaries() {
    let bins = workspace_bin_names();
    let text = guide_text();
    let bytes = text.as_bytes();

    let mut found: Vec<(usize, String)> = Vec::new();
    let mut i = 0usize;
    while let Some(at) = text[i..].find("target/") {
        let start = i + at;
        let mut end = start + "target/".len();
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric()
                || bytes[end] == b'_'
                || bytes[end] == b'-'
                || bytes[end] == b'/'
                || bytes[end] == b'.')
        {
            end += 1;
        }
        let path = &text[start..end];
        // `target/debug/build/…` and friends are cargo bookkeeping, not a
        // promise about a binary an operator invokes; a bare `target/release`
        // is a DIRECTORY reference and promises nothing. A build-output path
        // has three components — `target/<profile>/<name>` — and the name is
        // the only part the rule is about.
        if path.contains("/build/") || path.contains("/deps/") || path.ends_with('/') {
            i = end.max(start + 1);
            continue;
        }
        if path.split('/').count() < 3 {
            i = end.max(start + 1);
            continue;
        }
        let Some(name) = path.rsplit('/').next() else {
            i = end.max(start + 1);
            continue;
        };
        if !name.is_empty() {
            let line = text[..start].matches('\n').count() + 1;
            found.push((line, name.to_string()));
        }
        i = end.max(start + 1);
    }

    assert!(
        found.len() >= 10,
        "only {} build-output paths extracted; the extractor is broken, not the guide",
        found.len()
    );

    let mut bad: Vec<String> = Vec::new();
    for (line, name) in &found {
        if bins.contains(name) {
            continue;
        }
        bad.push(format!(
            "guide L{line}: `target/**/{name}` is not a `[[bin]]` any workspace crate \
             declares, so the path promises a file that is never written. Check the \
             crate's `[[bin]] name` — a PACKAGE name is not an executable name."
        ));
    }
    assert!(
        bad.is_empty(),
        "the guide points at build outputs that no crate produces:\n{}",
        bad.join("\n")
    );
}

#[test]
#[ignore]
fn diagnostic_dump_build_output_paths() {
    let bins = workspace_bin_names();
    let text = guide_text();
    let bytes = text.as_bytes();
    let mut n = 0usize;
    let mut i = 0usize;
    while let Some(at) = text[i..].find("target/") {
        let start = i + at;
        let mut end = start + "target/".len();
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric()
                || bytes[end] == b'_'
                || bytes[end] == b'-'
                || bytes[end] == b'/'
                || bytes[end] == b'.')
        {
            end += 1;
        }
        let path = &text[start..end];
        let name = path.rsplit('/').next().unwrap_or("");
        n += 1;
        println!(
            "  L{:<5} {:<46} bin={}",
            text[..start].matches('\n').count() + 1,
            path,
            bins.contains(name)
        );
        i = end.max(start + 1);
    }
    println!("build-output paths: {n}");
}
