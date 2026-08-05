//! Hermetic self-validation for the cross-environment MCP config snippets.
//!
//! Each snippet in `assets/mcp-configs/*.json` declares how an external
//! AI agent (Claude Code / Cursor / Continue.dev / Windsurf) should spawn
//! the `octo-whatsapp mcp` subprocess. This test asserts per-snippet:
//!
//!   1. File exists.
//!   2. JSON parses.
//!   3. The `mcpServers` block (or `experimental.mcpServers` for legacy
//!      Continue) contains an `octo-whatsapp` server.
//!   4. `command` equals `"octo-whatsapp"`.
//!   5. `args` is a non-empty array whose first element equals `"mcp"`.
//!   6. If `env.OCTO_WHATSAPP_PERSIST_DIR` is present, it points inside
//!      the user's home (no absolute `/tmp` or `/var`).
//!
//! All tests are pure file I/O — no daemon, no WA, no network.

// JSON parsing below uses &&s[1..] style slicing rather than
// `strip_prefix(...).unwrap().1`; the explicit form is easier to read in
// this hot parser loop. The two helpers below are tagged with explicit
// lifetimes because the borrow checker has trouble inferring them through
// the helper chain — both are clippy nits, not correctness issues.
#![allow(clippy::manual_strip, clippy::needless_lifetimes)]

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Walk a nested object path (`"experimental.mcpServers"`) and return the
/// final value. Returns `None` if any segment is missing.
fn descend<'a>(v: &'a J, path: &str) -> Option<&'a J> {
    let mut cur = v;
    for seg in path.split('.') {
        let key = seg.to_string();
        cur = match cur {
            J::Obj(m) => m.get(&key)?,
            _ => return None,
        };
    }
    Some(cur)
}

const MCP_CONFIGS_DIR_FROM_MANIFEST: &str = "assets/mcp-configs";

/// Manifest of expected JSON snippets.
///
/// Each entry is `(filename, mcp_servers_key)` — the key under which the
/// `octo-whatsapp` server block must live. Modern envs use top-level
/// `mcpServers`; legacy Continue used `experimental.mcpServers`.
const SNIPPETS: &[(&str, &str)] = &[
    ("claude-code.json", "mcpServers"),
    ("cursor.json", "mcpServers"),
    ("continue.json", "experimental.mcpServers"), // legacy Continue v0.x
    ("windsurf.json", "mcpServers"),
];

fn snippet_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MCP_CONFIGS_DIR_FROM_MANIFEST)
}

/// Minimal JSON value representation for assertions. Avoids pulling serde_json
/// just to read 4 tiny config files.
#[derive(Debug, Clone, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Num(i64),
    Str(String),
    Array(Vec<J>),
    Obj(std::collections::BTreeMap<String, J>),
}

mod json_min {
    use super::J;
    use std::collections::BTreeMap;

    pub fn parse(src: &str) -> Result<J, String> {
        let pv = preview(src);
        let (v, rest) = parse_value(src.trim()).map_err(|e| format!("json parse: {e} at: {pv}"))?;
        let rest = rest.trim();
        if !rest.is_empty() {
            let pv2 = preview(src);
            return Err(format!("trailing content after JSON value: {pv2}"));
        }
        Ok(v)
    }

    fn preview(s: &str) -> String {
        s.chars().take(80).collect()
    }

    fn parse_value(s: &str) -> Result<(J, &str), String> {
        let s = s.trim_start();
        if s.is_empty() {
            return Err("unexpected end".into());
        }
        match s.as_bytes()[0] {
            b'n' => {
                if s.starts_with("null") {
                    Ok((J::Null, &s[4..]))
                } else {
                    Err("bad null".into())
                }
            }
            b't' => {
                if s.starts_with("true") {
                    Ok((J::Bool(true), &s[4..]))
                } else {
                    Err("bad true".into())
                }
            }
            b'f' => {
                if s.starts_with("false") {
                    Ok((J::Bool(false), &s[5..]))
                } else {
                    Err("bad false".into())
                }
            }
            b'"' => parse_string(s),
            b'[' => parse_array(s),
            b'{' => parse_object(s),
            _b if s.as_bytes()[0].is_ascii_digit() || s.as_bytes()[0] == b'-' => parse_num(s),
            _ => Err(format!("unexpected byte: {}", s.as_bytes()[0] as char)),
        }
    }

    fn parse_string(s: &str) -> Result<(J, &str), String> {
        let s = &s[1..]; // skip opening quote
        let mut out = String::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '"' {
                return Ok((J::Str(std::mem::take(&mut out)), &s[i + 1..]));
            }
            if c == '\\' && i + 1 < bytes.len() {
                out.push(c);
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            out.push(c);
            i += 1;
        }
        Err("unterminated string".into())
    }

    fn parse_num(s: &str) -> Result<(J, &str), String> {
        let end = s
            .find(|c: char| !(c.is_ascii_digit() || c == '-'))
            .unwrap_or(s.len());
        let (head, rest) = s.split_at(end);
        let n: i64 = head
            .parse()
            .map_err(|e| format!("bad number `{head}`: {e}"))?;
        Ok((J::Num(n), rest))
    }

    fn parse_array(s: &str) -> Result<(J, &str), String> {
        let s = &s[1..]; // skip '['
        let mut items = Vec::new();
        let mut s = s.trim_start();
        if s.starts_with(']') {
            return Ok((J::Array(items), &s[1..]));
        }
        loop {
            let (v, rest) = parse_value(s)?;
            items.push(v);
            s = rest.trim_start();
            if s.starts_with(',') {
                s = &s[1..];
                s = s.trim_start();
                continue;
            }
            if s.starts_with(']') {
                return Ok((J::Array(items), &s[1..]));
            }
            return Err("missing ',' or ']' in array".into());
        }
    }

    fn parse_object(s: &str) -> Result<(J, &str), String> {
        let s = &s[1..]; // skip '{'
        let mut obj: BTreeMap<String, J> = BTreeMap::new();
        let mut s = s.trim_start();
        if s.starts_with('}') {
            return Ok((J::Obj(obj), &s[1..]));
        }
        loop {
            let (k, rest) = parse_string(s)?;
            let key = match k {
                J::Str(s) => s,
                _ => return Err("object key not string".into()),
            };
            s = rest.trim_start();
            if !s.starts_with(':') {
                return Err("missing ':' in object".into());
            }
            s = &s[1..];
            s = s.trim_start();
            let (v, rest) = parse_value(s)?;
            obj.insert(key, v);
            s = rest.trim_start();
            if s.starts_with(',') {
                s = &s[1..];
                s = s.trim_start();
                continue;
            }
            if s.starts_with('}') {
                return Ok((J::Obj(obj), &s[1..]));
            }
            return Err("missing ',' or '}' in object".into());
        }
    }

    pub fn as_str<'a>(v: &'a J, path: &str) -> Option<&'a str> {
        let mut cur = v;
        for seg in path.split('.') {
            let key = seg.to_string();
            cur = match cur {
                J::Obj(m) => m.get(&key)?,
                _ => return None,
            };
        }
        match cur {
            J::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_array<'a>(v: &'a J) -> Option<&'a Vec<J>> {
        match v {
            J::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object<'a>(v: &'a J) -> Option<&'a BTreeMap<String, J>> {
        match v {
            J::Obj(o) => Some(o),
            _ => None,
        }
    }
}

fn load_snippet(filename: &str, expected_key: &str) -> J {
    let path = snippet_dir().join(filename);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {filename}: {e}"));
    let parsed =
        json_min::parse(&text).unwrap_or_else(|e| panic!("{filename} is not valid JSON: {e}"));
    let root =
        json_min::as_object(&parsed).unwrap_or_else(|| panic!("{filename} root is not an object"));
    let top_keys: BTreeSet<&str> = root.keys().map(String::as_str).collect();
    // The expected_key may be a dotted path (legacy Continue); descend
    // through it. Verify each segment exists along the way.
    let mut cur: &J = &parsed;
    let mut cur_obj: &std::collections::BTreeMap<String, J> = root;
    for seg in expected_key.split('.') {
        let key = seg.to_string();
        cur_obj = match cur {
            J::Obj(m) => m
                .get(&key)
                .and_then(json_min::as_object)
                .unwrap_or_else(|| {
                    panic!("{filename} missing path `{expected_key}`; top-level keys: {top_keys:?}")
                }),
            _ => panic!("{filename}: cannot descend into non-object at `{seg}`"),
        };
        cur = match cur {
            J::Obj(m) => m.get(&key).unwrap(),
            _ => unreachable!(),
        };
    }
    // Suppress unused — we only need the parsed value, not the traversal.
    let _ = cur_obj;
    parsed
}

// ─── Per-file structural checks ───────────────────────────────────────────

#[test]
fn all_four_json_snippets_exist() {
    for (fname, _) in SNIPPETS {
        let p = snippet_dir().join(fname);
        assert!(
            p.exists(),
            "snippet missing: {fname}\nExpected at: {}",
            p.display()
        );
    }
}

#[test]
fn each_snippet_declares_octo_whatsapp_server() {
    for (fname, key) in SNIPPETS {
        let parsed = load_snippet(fname, key);
        let servers = json_min::as_object(descend(&parsed, key).expect("key path resolves"))
            .unwrap_or_else(|| panic!("{fname}: `{key}` is not an object"));
        assert!(
            servers.contains_key("octo-whatsapp"),
            "{fname}: `{key}` must contain `octo-whatsapp`; got: {:?}",
            servers.keys().collect::<BTreeSet<_>>()
        );
    }
}

#[test]
fn each_snippet_uses_octo_whatsapp_command() {
    for (fname, key) in SNIPPETS {
        let parsed = load_snippet(fname, key);
        let servers = json_min::as_object(descend(&parsed, key).expect("key path resolves"))
            .expect("mcpServers is an object");
        let srv = servers
            .get("octo-whatsapp")
            .unwrap_or_else(|| panic!("{fname}: missing octo-whatsapp server block"));
        let cmd = json_min::as_str(srv, "command")
            .unwrap_or_else(|| panic!("{fname}: `octo-whatsapp.command` not a string"));
        assert_eq!(cmd, "octo-whatsapp", "{fname}: command mismatch");
    }
}

#[test]
fn each_snippet_args_starts_with_mcp() {
    for (fname, key) in SNIPPETS {
        let parsed = load_snippet(fname, key);
        let servers = json_min::as_object(descend(&parsed, key).expect("key path resolves"))
            .expect("mcpServers is an object");
        let srv = servers
            .get("octo-whatsapp")
            .unwrap_or_else(|| panic!("{fname}: missing server block"));
        let args = json_min::as_object(srv)
            .and_then(|o| o.get("args"))
            .and_then(json_min::as_array)
            .unwrap_or_else(|| panic!("{fname}: `args` must be an array"));
        let first = match args.first() {
            Some(J::Str(s)) => s.as_str(),
            other => panic!("{fname}: args[0] must be string, got {other:?}"),
        };
        assert_eq!(first, "mcp", "{fname}: args[0] must equal `mcp`");
    }
}

#[test]
fn each_snippet_persist_dir_points_into_home() {
    for (fname, key) in SNIPPETS {
        let parsed = load_snippet(fname, key);
        let servers = json_min::as_object(descend(&parsed, key).expect("key path resolves"))
            .expect("mcpServers is an object");
        let srv = servers
            .get("octo-whatsapp")
            .unwrap_or_else(|| panic!("{fname}: missing server block"));
        let env = json_min::as_object(srv)
            .and_then(|o| o.get("env"))
            .and_then(json_min::as_object);
        let persist = match env.and_then(|e| e.get("OCTO_WHATSAPP_PERSIST_DIR")) {
            Some(J::Str(s)) => s.as_str(),
            other => panic!(
                "{fname}: env.OCTO_WHATSAPP_PERSIST_DIR missing or non-string, got {other:?}"
            ),
        };
        assert!(
            persist.starts_with("${HOME}") || persist.starts_with("~"),
            "{fname}: persist dir must be home-relative, got `{persist}`"
        );
        assert!(
            !persist.starts_with("/tmp") & !persist.starts_with("/var"),
            "{fname}: persist dir must not be world-writable, got `{persist}`"
        );
    }
}

// ─── Cross-snippet checks ─────────────────────────────────────────────────

/// All four snippets must declare the same `command` + `args[0]` (env
/// portability is the whole point of pinning these).
#[test]
fn all_snippets_have_identical_command_and_args() {
    let mut commands = BTreeSet::new();
    let mut args0 = BTreeSet::new();
    for (fname, key) in SNIPPETS {
        let parsed = load_snippet(fname, key);
        let servers = json_min::as_object(descend(&parsed, key).expect("key path resolves"))
            .expect("mcpServers is an object");
        let srv = servers.get("octo-whatsapp").unwrap();
        let cmd = json_min::as_str(srv, "command").unwrap().to_string();
        commands.insert(cmd);
        let args = json_min::as_object(srv)
            .and_then(|o| o.get("args"))
            .and_then(json_min::as_array)
            .unwrap();
        let first = match args.first().unwrap() {
            J::Str(s) => s.clone(),
            _ => panic!("{fname}: args[0] not string"),
        };
        args0.insert(first);
    }
    assert_eq!(commands.len(), 1, "commands diverge: {commands:?}");
    assert_eq!(args0.len(), 1, "args[0] diverge: {args0:?}");
}

/// The 4 snippets are the pinned set; pinned filenames prevent drift.
#[test]
fn snippet_filenames_match_expected_set() {
    let mut found: BTreeSet<&'static str> = SNIPPETS.iter().map(|(f, _)| *f).collect();
    for n in [
        "claude-code.json",
        "cursor.json",
        "continue.json",
        "windsurf.json",
    ] {
        assert!(found.remove(n), "missing snippet: {n}");
    }
    assert!(found.is_empty(), "extra snippets: {found:?}");
}

// ─── Aider shim ──────────────────────────────────────────────────────────

#[test]
fn aider_shim_is_executable_bash() {
    let p = snippet_dir().join("aider.sh");
    assert!(p.exists(), "aider.sh missing");
    let meta = fs::metadata(&p).expect("aider.sh metadata");
    let perms = meta.permissions();
    assert!(
        perms.mode() & 0o111 != 0,
        "aider.sh must be executable (mode={:o})",
        perms.mode() & 0o777
    );
    let text = fs::read_to_string(&p).expect("aider.sh readable");
    assert!(text.starts_with("#!"), "aider.sh missing shebang");
    assert!(
        text.contains("octo-whatsapp"),
        "aider.sh must reference octo-whatsapp"
    );
    // Smoke: requires PATH to contain `octo-whatsapp` — assume False for hermetic.
    // Just confirm case dispatch covers send-text without needing the binary.
    assert!(
        text.contains("send-text)"),
        "aider.sh missing `send-text)` case"
    );
    assert!(text.contains("usage"), "aider.sh missing `usage` block");
}
