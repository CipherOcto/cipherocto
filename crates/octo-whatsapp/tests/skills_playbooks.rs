//! Hermetic self-validation for the four thin playbooks that complement
//! `wa-mcp.md`.
//!
//! Each playbook is a workflow-oriented entry point for a category of MCP
//! tool usage:
//!   - `wa-send.md`    — outbound messages
//!   - `wa-monitor.md` — inbound observation + queries
//!   - `wa-recover.md` — connection + pairing + lifecycle
//!   - `wa-config.md`  — rules, triggers, audit, accounts, actions
//!
//! This test asserts (per playbook):
//!   1. File exists.
//!   2. Frontmatter parses as a flat map with `name == <expected>`.
//!   3. Required "Ground rules" section is present.
//!   4. Required "When to use this playbook" section is present.
//!   5. Required "Common failure modes" section is present.
//!   6. Body cross-references at least one MCP tool name from the live catalog.
//!
//! All tests are pure file I/O — no daemon, no WA, no network.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const SKILL_DIR_FROM_MANIFEST: &str = "assets/skills";

/// Manifest of expected playbooks + their canonical name + required needles.
const PLAYBOOKS: &[(&str, &str, &[&str])] = &[
    (
        "wa-send.md",
        "wa-send",
        &[
            "Ground rules",
            "When to use this playbook",
            "Common failure modes",
        ],
    ),
    (
        "wa-monitor.md",
        "wa-monitor",
        &[
            "Ground rules",
            "When to use this playbook",
            "Common failure modes",
        ],
    ),
    (
        "wa-recover.md",
        "wa-recover",
        &[
            "Ground rules",
            "When to use this playbook",
            "Common failure modes",
        ],
    ),
    (
        "wa-config.md",
        "wa-config",
        &[
            "Ground rules",
            "When to use this playbook",
            "Common failure modes",
        ],
    ),
];

fn skill_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SKILL_DIR_FROM_MANIFEST)
}

fn read_split(path: &PathBuf) -> (String, String) {
    let text = fs::read_to_string(path).expect("playbook file readable");
    let (head, body) = text
        .split_once("\n---\n")
        .expect("playbook has `---`-delimited frontmatter");
    assert!(head.starts_with("---"), "frontmatter must start with `---`");
    (
        head.trim_start_matches("---").trim().to_string(),
        body.to_string(),
    )
}

/// Returns the expected playbook name list as a flat BTreeSet for cross-tests.
fn expected_names() -> std::collections::BTreeSet<&'static str> {
    PLAYBOOKS.iter().map(|(_, name, _)| *name).collect()
}

// ─── Per-file structural checks ───────────────────────────────────────────

#[test]
fn all_four_playbooks_exist() {
    let dir = skill_dir();
    for (fname, _, _) in PLAYBOOKS {
        let p = dir.join(fname);
        assert!(
            p.exists(),
            "playbook missing: {}\nExpected at: {}",
            fname,
            p.display()
        );
    }
}

#[test]
fn each_playbook_frontmatter_declares_correct_name() {
    for (fname, expected_name, _) in PLAYBOOKS {
        let p = skill_dir().join(fname);
        let (head, _body) = read_split(&p);
        let fm: BTreeMap<String, String> =
            serde_yaml_min::parse_flat(&head).expect("frontmatter parses");
        let got = fm
            .get("name")
            .unwrap_or_else(|| panic!("{fname} frontmatter missing `name`"));
        assert_eq!(got, expected_name, "{fname} frontmatter name mismatch");
    }
}

#[test]
fn each_playbook_has_a_description() {
    for (fname, _, _) in PLAYBOOKS {
        let p = skill_dir().join(fname);
        let (head, _body) = read_split(&p);
        let fm: BTreeMap<String, String> =
            serde_yaml_min::parse_flat(&head).expect("frontmatter parses");
        let desc = fm
            .get("description")
            .unwrap_or_else(|| panic!("{fname} frontmatter missing `description`"));
        assert!(
            desc.len() >= 60,
            "{fname} description too short ({} chars)",
            desc.len()
        );
    }
}

#[test]
fn each_playbook_contains_required_sections() {
    for (fname, _, needles) in PLAYBOOKS {
        let p = skill_dir().join(fname);
        let (_head, body) = read_split(&p);
        for needle in *needles {
            assert!(
                body.contains(needle),
                "{fname} missing required section: `{needle}`"
            );
        }
    }
}

// ─── Cross-playbook / cross-catalog checks ────────────────────────────────

/// Every playbook must cross-reference at least one MCP tool name from the
/// live catalog. Catches the case where a playbook goes stale because the
/// tool registry evolved.
#[test]
fn every_playbook_references_at_least_one_live_tool() {
    use octo_whatsapp::mcp_server::tool_descriptors;

    let live_tools: std::collections::BTreeSet<String> = tool_descriptors()
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

    for (fname, expected_name, _) in PLAYBOOKS {
        let p = skill_dir().join(fname);
        let text = fs::read_to_string(p).expect("playbook readable");
        let hits: Vec<&String> = live_tools
            .iter()
            .filter(|t| text.contains(t.as_str()))
            .collect();
        assert!(
            !hits.is_empty(),
            "{fname} (name={expected_name}) does not reference any MCP tool.\n\
             It must cross-reference at least one of the 100 live tools."
        );
    }
}

/// Names match the four pinned playbook slugs.
#[test]
fn playbook_names_match_expected_set() {
    let names = expected_names();
    assert_eq!(names.len(), 4, "expected 4 distinct playbooks");
    for n in ["wa-send", "wa-monitor", "wa-recover", "wa-config"] {
        assert!(names.contains(n), "missing playbook name: {n}");
    }
}

// ─── Local YAML shim ──────────────────────────────────────────────────────
//
// Same philosophy as `skills_wa_mcp.rs`: avoid pulling serde_yaml by writing a
// minimal flat-map parser sufficient for frontmatter here. Values are always
// strings (the only field we inspect for assertions).
mod serde_yaml_min {
    use std::collections::BTreeMap;

    pub fn parse_flat(src: &str) -> Result<BTreeMap<String, String>, String> {
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        for raw in src.lines() {
            let line = raw.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let key = k.trim().to_string();
            let raw_val = v.trim();
            // Strip inline comments after `#`.
            let raw_val = raw_val.split('#').next().unwrap_or(raw_val).trim();
            // Strip surrounding quotes if present.
            let val = raw_val
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(raw_val);
            out.insert(key, val.to_string());
        }
        Ok(out)
    }
}
