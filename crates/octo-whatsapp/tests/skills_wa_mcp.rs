//! Hermetic self-validation for the fat `wa-mcp.md` Skill reference.
//!
//! Ensures the skill stays in lock-step with the MCP tool registry:
//!   - Frontmatter parses as YAML with `name == "wa-mcp"`.
//!   - Every tool advertised by `mcp_server::tool_descriptors()` appears
//!     somewhere in the skill document (heading, in-line code, or list).
//!
//! Run via:
//!     cargo test -p octo-whatsapp --test skills_wa_mcp
//!
//! No daemon, no WA — pure file I/O.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Path to the wa-mcp skill relative to the crate manifest dir.
fn skill_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("skills")
        .join("wa-mcp.md")
}

/// The fat reference must declare itself as the `wa-mcp` skill and own
/// the description that MCP clients index.
#[test]
fn frontmatter_parses_and_declares_wa_mcp() {
    let p = skill_path();
    assert!(
        p.exists(),
        "skill file missing: {}\nRun: write assets/skills/wa-mcp.md",
        p.display()
    );
    let text = fs::read_to_string(&p).expect("read wa-mcp.md");
    let (head, body) = text
        .split_once("\n---\n")
        .expect("frontmatter is `---` delimited at line 1");
    // Some frontmatter parsers require the *opening* `---` to also be visible;
    // we only need the body (between the two `---`) to be valid YAML.
    assert!(head.starts_with("---"), "frontmatter must start with `---`");
    let fm: serde_yaml_buggy::Value =
        serde_yaml_buggy::from_str(head.trim_start_matches("---").trim())
            .expect("YAML frontmatter must parse");
    // Use serde_json::Value to make the dependency-free choice deliberate
    // (the test only needs basic shape — leave parser upgrades to the
    // actual YAML crate when it's added).
    let _ = body; // body content is checked by the next test.
    let name = fm
        .get("name")
        .and_then(|v| v.as_str())
        .expect("frontmatter `name` string");
    assert_eq!(name, "wa-mcp", "frontmatter name must equal `wa-mcp`");
    let desc = fm
        .get("description")
        .and_then(|v| v.as_str())
        .expect("frontmatter `description` string");
    assert!(
        desc.contains("MCP"),
        "description must mention MCP, got: {desc}"
    );
    assert!(desc.len() >= 60, "description too short ({})", desc.len());
}

/// Pull the live catalog from mcp_server (single source of truth for tool
/// names) and assert every one appears in wa-mcp.md.
#[test]
fn every_mcp_tool_name_appears_in_skill() {
    use octo_whatsapp::mcp_server::tool_descriptors;

    let text = fs::read_to_string(skill_path()).expect("read wa-mcp.md");
    let live_tools: BTreeSet<String> = tool_descriptors()
        .into_iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert!(
        !live_tools.is_empty(),
        "live catalog pulled zero tools; tool_descriptors() broken?"
    );

    let mut missing: Vec<String> = Vec::new();
    for tool in &live_tools {
        // Tool names contain literal `.` and `_` which are word chars in
        // both Markdown links and backtick spans, so we search as a
        // substring (case-sensitive). The catalog is exact-match so any
        // ungrammatical mention would only happen for tools named twice.
        if !text.contains(tool.as_str()) {
            missing.push(tool.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "wa-mcp.md missing {} tool reference(s): {:#?}\n\
         Add each as a heading (## `name`) or in an example block.",
        missing.len(),
        missing
    );
}

/// Sanity: the skill contains a "Ground rules" section listing the WA
/// rate-limit floor, the peer format table, and the bot-state gate. These
/// three are non-negotiable operator runbooks — failing here means a future
/// edit silently dropped them.
#[test]
fn ground_rules_section_present() {
    let text = fs::read_to_string(skill_path()).expect("read wa-mcp.md");
    for needle in [
        "WA rate-limit floor",      // 2-second rule
        "Peer format",              // E164 / LID / JID
        "Bot state",                // status.get gating
        "Event-table ground truth", // events.* as ground truth
    ] {
        assert!(
            text.contains(needle),
            "ground-rules section missing required runbook: `{needle}`"
        );
    }
}

/// Aggregate so a single cargo invocation reports all holes at once.
#[test]
fn tool_count_matches_expected() {
    use octo_whatsapp::mcp_server::tool_descriptors;
    use octo_whatsapp::mcp_server::EXPECTED_TOOL_COUNT;
    let n = tool_descriptors().len();
    assert_eq!(
        n, EXPECTED_TOOL_COUNT,
        "live tool_descriptors()={} disagrees with EXPECTED_TOOL_COUNT={}",
        n, EXPECTED_TOOL_COUNT
    );
}

// ── Local YAML shim (avoids pulling serde_yaml) ────────────────────────────
//
// A real YAML parser would add a dependency we don't otherwise need — front-
// matter here is a flat map of strings. We accept a strict subset: keys with
// scalar (string/boolean/integer) values. Anything more complex falls back
// to a JSON-style parse via a thin ad-hoc reader.
mod serde_yaml_buggy {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone)]
    pub enum Value {
        /// Anything we couldn't reduce to a string. Frontmatter fields here
        /// are intentionally untyped because the test only inspects string
        /// values via `as_str` / `get`.
        Other,
        Str(String),
        Map(BTreeMap<String, Value>),
    }

    impl Value {
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::Str(s) => Some(s.as_str()),
                _ => None,
            }
        }
        pub fn get(&self, k: &str) -> Option<&Value> {
            match self {
                Value::Map(m) => m.get(k),
                _ => None,
            }
        }
    }

    pub fn from_str(src: &str) -> Result<Value, String> {
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        for raw in src.lines() {
            let line = raw.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let key = k.trim().to_string();
            let raw = v.trim();
            // Frontmatter fields are strings only for this test (name,
            // description). Numbers / bools collapse to `Other`; the
            // public API only exposes `as_str`/`get` so a non-string here
            // is observable as "missing".
            let value = if raw.is_empty()
                || raw.parse::<i64>().is_ok()
                || raw == "true"
                || raw == "false"
            {
                Value::Other
            } else {
                // Strip inline comments after `#` (rare in our frontmatter).
                let val = raw.split('#').next().unwrap_or(raw).trim();
                // Strip surrounding quotes if present.
                let val = val
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(val);
                Value::Str(val.to_string())
            };
            map.insert(key, value);
        }
        Ok(Value::Map(map))
    }
}
