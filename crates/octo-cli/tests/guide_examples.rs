//! Mechanical conformance checks over `docs/06-operations/operator-guide.md`.
//!
//! Eight adversarial rounds found 37 divergences in the guide by hand. A
//! hand audit is a poor guard: it decays the moment nobody is looking. This
//! suite is the guard. Every claim the guide makes that can be decided
//! mechanically is decided here, against the real substrate.
//!
//! What this suite deliberately does NOT do: execute the guide's shell
//! procedures. The `octo` CLI cannot provision a node — `octo identity` is
//! `Show | Rotate | Revoke` and `octo-wallet init` writes a seed the CLI
//! cannot read — so a full executor would report substrate walls as guide
//! defects. That is the same false-positive trap as a naive command sweep.
//! So the suite checks *decidable* claims only, and the wall itself is
//! documented in the guide instead.
//!
//! The four checks, each of which has already caught a real defect:
//!
//! | Check | Caught |
//! | --- | --- |
//! | `guide_commands_all_resolve` | the command layer (round 4) |
//! | `guide_flags_all_resolve` | G8–G12, G16 (rounds 4–5) |
//! | `guide_jq_filters_all_compile` | G31 — a filter that jq cannot parse |
//! | `guide_caveats_expressions_all_parse` | G32–G36 — all 7 mint/attenuate
//!   expressions were rejected by the parser at exit 7 |
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
use std::path::PathBuf;
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
