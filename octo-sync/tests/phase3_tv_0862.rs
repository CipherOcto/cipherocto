//! Mission `0862-phase3-tv-fixture` — Phase 3 Performance Test Vector
//! Fixture gate for RFC-0862 v1.3.0.
//!
//! 1 perf-budget test vector (TV-5) gating Phase 3 acceptance:
//!
//! - **TV-5** — `phase3_tv_0862_election_acquire_within_3s`:
//!   100 sequential `Cluster::try_acquire_leader` calls (each on a
//!   unique shard_key so no lease contention) must complete within
//!   the RFC-0862 §Performance Targets budget of 3 seconds. CI slack
//!   factor of 10× (so assertion threshold = 30 s) absorbs CI jitter
//!   without false-failing the gate. Per RFC-0862 v1.3.0 §Test
//!   Vectors Phase 3 list.
//!
//! ## Why a perf-budget TV (different from Phase 1 byte-exact)
//!
//! Phase 1 TVs (TV-1..TV-4) are byte-exact fixtures — `outputs_hex`
//! is the canonical reference. Phase 3 TVs are performance budgets —
//! the fixture stores the budget (`budget_ms`, `ci_slack_factor`,
//! `iterations`) and the gate test measures fresh, asserting under
//! CI slack. Re-measurement is intentional: perf TVs gate that the
//! substrate stays under the budget, not that it produces a specific
//! byte string.
//!
//! ## Verification pattern
//!
//! Run the gate tests (loads fixture, asserts perf budget):
//!
//! ```bash
//! cargo test -p octo-sync --test phase3_tv_0862
//! ```
//!
//! Bootstrap the fixture (deterministic; overwrites the JSON when
//! run with `UPDATE_PHASE3_TV=1`):
//!
//! ```bash
//! UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862 phase3_tv_0862_dump -- --nocapture
//! ```
//!
//! Per [[feedback-no-fabricated-commit-rule]] + RFC-0008 Class A
//! determinism: the fixture budget is canonical; the substrate must
//! stay under it. Any drift = a substrate performance regression.
//!
//! ## Out of scope (NOT this fixture)
//!
//! - TV-6 (`drain_throughput_1k_per_sec`) — follow-on mission per
//!   R17 M3 (deferral of related work).
//! - TV-7 (`failover_pause_under_3s`) — follow-on mission.
//! - TV-8 (`wal_fanout_lag_under_100ms`) — follow-on mission.

#![allow(clippy::vec_init_then_push)]

use std::fmt::Write;
use std::path::PathBuf;
use std::time::Instant;

use octo_sync::substrate::{Cluster, HlcTimestamp, ShardKey, WriterNodeId};

/// Path to the JSON fixture. Use `CARGO_MANIFEST_DIR` for
/// absolute determinism (cargo sets this env var to the
/// package root at compile time) — relative paths via
/// `current_dir()` are flaky because cargo test CWD varies
/// per-test-binary vs combined `--tests` runs. The package
/// root is `octo-sync/`, so `../tests/fixtures/...` is one
/// level up = the repo root.
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures");
const FIXTURE_NAME: &str = "phase3_tv_0862.json";

/// TV-5: election acquire returns within 3 seconds.
///
/// 100 sequential `Cluster::try_acquire_leader` calls on a fresh
/// `Cluster`, each with a unique `WriterNodeId` + `ShardKey` so all
/// acquires succeed (no lease contention). Measure total elapsed
/// wall-clock time via `Instant::now()`. Assert under CI slack
/// (`budget_ms × ci_slack_factor`).
///
/// Inputs (declared in fixture):
/// - `budget_ms = 3000` (RFC-0862 §Performance Targets)
/// - `ci_slack_factor = 10` (CI jitter absorption)
/// - `iterations = 100`
/// - `writer_node_id_template = [i;32]` (per-iter unique)
/// - `shard_key_template = [i;32]` (per-iter unique)
/// - `hlc_physical_ms = 1_700_000_000_000 + i`
///
/// Output: `Vec<u8>` containing the per-iteration elapsed-ms values
/// (u32 LE) concatenated. NOT byte-exact reproducible across runs
/// (perf noise) — fixture stores BUDGET, not observed value.
fn tv5_election_acquire_within_3s(iterations: u32) -> Vec<u8> {
    let cluster = Cluster::new();
    let start = Instant::now();
    let mut per_iter_ms = Vec::with_capacity(iterations as usize);
    for i in 0..iterations {
        let t0 = Instant::now();
        let node_id = WriterNodeId({
            let mut bytes = [0u8; 32];
            bytes[0] = i as u8;
            bytes
        });
        let shard_key = ShardKey({
            let mut bytes = [0u8; 32];
            bytes[0] = i as u8;
            bytes
        });
        let hlc = HlcTimestamp {
            physical_ms: 1_700_000_000_000 + i as u64,
            logical: 0,
            writer_node_id: node_id,
        };
        cluster
            .try_acquire_leader(node_id, shard_key, hlc)
            .unwrap_or_else(|e| panic!("acquire iter {i} failed: {e:?}"));
        per_iter_ms.push(t0.elapsed().as_millis() as u32);
    }
    let total_ms = start.elapsed().as_millis();
    eprintln!(
        "TV-5: {iterations} acquires in {total_ms}ms (mean {:.2}ms/iter)",
        total_ms as f64 / iterations as f64
    );

    let mut out = Vec::with_capacity(4 * iterations as usize + 8);
    out.extend_from_slice(&(total_ms as u64).to_le_bytes());
    for ms in per_iter_ms {
        out.extend_from_slice(&ms.to_le_bytes());
    }
    out
}

/// Canonical 1-TV fixture struct. JSON serialization is hand-rolled
/// (no `serde_json` dep in this test module's emit path beyond
/// `to_string`) to keep the fixture diff-friendly.
struct Phase3Fixture {
    entries: Vec<Phase3TvEntry>,
}

struct Phase3TvEntry {
    name: String,
    description: String,
    test_function: String,
    budget_ms: u64,
    ci_slack_factor: u64,
    iterations: u32,
    verification_command: String,
}

impl Phase3Fixture {
    fn compute() -> Self {
        let mut entries = Vec::new();
        entries.push(Phase3TvEntry {
            name: "TV-5".to_string(),
            description: "Election acquire returns within 3 s: 100 sequential \
                Cluster::try_acquire_leader calls (each on a unique \
                WriterNodeId + ShardKey so no lease contention) MUST \
                complete within 3000 ms wall-clock. Per RFC-0862 v1.3.0 \
                §Performance Targets. CI slack factor 10x = 30 s \
                assertion threshold."
                .to_string(),
            test_function: "election_acquire_returns_within_3s".to_string(),
            budget_ms: 3_000,
            ci_slack_factor: 10,
            iterations: 100,
            verification_command: "cargo test -p octo-sync --test phase3_tv_0862 \
                phase3_tv_0862_election_acquire_within_3s -- --nocapture"
                .to_string(),
        });
        Self { entries }
    }

    fn to_json(&self) -> String {
        let mut s = String::new();
        writeln!(s).unwrap();
        s.push_str(
            "  \"_comment\": \"RFC-0862 v1.3.0 Phase 3 performance test vectors (TV-5 ONLY). \
            Phase 3 TVs are PERFORMANCE BUDGETS — not byte-exact. The fixture stores the budget \
            (`budget_ms`, `ci_slack_factor`, `iterations`); the gate test measures fresh and \
            asserts under CI slack. Re-bootstrap via `UPDATE_PHASE3_TV=1 cargo test -p \
            octo-sync --test phase3_tv_0862 phase3_tv_0862_dump -- --nocapture`. Per RFC-0008 \
            Class A determinism: the budget is canonical; the substrate must stay under it.\",\n",
        );
        s.push_str("  \"_rfc\": \"RFC-0862 v1.3.0\",\n");
        s.push_str("  \"_phase\": \"Phase 3\",\n");
        s.push_str(
            "  \"_scope\": \"TV-5 ONLY. TV-6/7/8 land in separate follow-on missions per RFC-0862 \
            §Test Vectors scope discipline (R17 M3).\",\n",
        );
        s.push_str("  \"entries\": [\n");
        for (i, e) in self.entries.iter().enumerate() {
            s.push_str("    {\n");
            writeln!(s, "      \"name\": \"{}\",", e.name).unwrap();
            writeln!(s, "      \"description\": \"{}\",", e.description).unwrap();
            writeln!(s, "      \"test_function\": \"{}\",", e.test_function).unwrap();
            writeln!(s, "      \"budget_ms\": {},", e.budget_ms).unwrap();
            writeln!(s, "      \"ci_slack_factor\": {},", e.ci_slack_factor).unwrap();
            writeln!(s, "      \"iterations\": {},", e.iterations).unwrap();
            writeln!(
                s,
                "      \"verification_command\": \"{}\"",
                e.verification_command
            )
            .unwrap();
            let comma = if i + 1 < self.entries.len() { "," } else { "" };
            writeln!(s, "    }}{comma}").unwrap();
        }
        s.push_str("  ]\n");
        s.push_str("}\n");
        s
    }

    /// Parse the JSON fixture, returning the 1 entry. Hand-rolled
    /// minimal parser (no `serde_json` parse dep).
    fn from_json(json: &str) -> Result<Vec<Phase3TvEntry>, String> {
        let entries_start = json
            .find("\"entries\": [")
            .ok_or_else(|| "missing entries key".to_string())?;
        let after_array = entries_start + "\"entries\": [".len();
        let entries_end_rel = json[after_array..]
            .find("]\n")
            .ok_or_else(|| "missing entries closing".to_string())?;
        let entries_block = &json[after_array..after_array + entries_end_rel];

        let mut entries = Vec::new();
        let mut depth = 0_i32;
        let mut obj_start: Option<usize> = None;
        for (i, ch) in entries_block.char_indices() {
            match ch {
                '{' => {
                    if depth == 0 {
                        obj_start = Some(i);
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some(start) = obj_start {
                            let obj = &entries_block[start..=i];
                            entries.push(parse_tv_entry(obj)?);
                            obj_start = None;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(entries)
    }
}

fn parse_tv_entry(obj: &str) -> Result<Phase3TvEntry, String> {
    let name = extract_string(obj, "name")?;
    let description = extract_string(obj, "description")?;
    let test_function = extract_string(obj, "test_function")?;
    let budget_ms: u64 = extract_number(obj, "budget_ms")?
        .parse()
        .map_err(|e| format!("budget_ms parse: {e}"))?;
    let ci_slack_factor: u64 = extract_number(obj, "ci_slack_factor")?
        .parse()
        .map_err(|e| format!("ci_slack_factor parse: {e}"))?;
    let iterations: u32 = extract_number(obj, "iterations")?
        .parse()
        .map_err(|e| format!("iterations parse: {e}"))?;
    let verification_command = extract_string(obj, "verification_command")?;
    Ok(Phase3TvEntry {
        name,
        description,
        test_function,
        budget_ms,
        ci_slack_factor,
        iterations,
        verification_command,
    })
}

fn extract_string(obj: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\": \"");
    let start = obj
        .find(&needle)
        .ok_or_else(|| format!("missing key: {key}"))?;
    let after = start + needle.len();
    let rest = &obj[after..];
    let end = rest
        .find('"')
        .ok_or_else(|| format!("unterminated key: {key}"))?;
    Ok(rest[..end].to_string())
}

fn extract_number(obj: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\": ");
    let start = obj
        .find(&needle)
        .ok_or_else(|| format!("missing key: {key}"))?;
    let after = start + needle.len();
    let rest = &obj[after..];
    let end = rest
        .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
        .ok_or_else(|| format!("unterminated number: {key}"))?;
    Ok(rest[..end].to_string())
}

/// Resolve the fixture path via the compile-time
/// `CARGO_MANIFEST_DIR` constant. Absolute + deterministic.
fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(FIXTURE_DIR);
    p.push(FIXTURE_NAME);
    p
}

/// Dump mode: regenerate the JSON fixture from the canonical
/// reference impl. Run with `UPDATE_PHASE3_TV=1` to bootstrap (or
/// refresh after a substrate change). The dump is deterministic —
/// the budget values are constants.
#[test]
fn phase3_tv_0862_dump() {
    if std::env::var("UPDATE_PHASE3_TV").ok().as_deref() != Some("1") {
        eprintln!(
            "phase3_tv_0862_dump is a no-op unless UPDATE_PHASE3_TV=1. \
             To bootstrap the fixture, run: \
             UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862 phase3_tv_0862_dump -- --nocapture"
        );
        return;
    }
    let fixture = Phase3Fixture::compute();
    let path = fixture_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture dir");
    }
    std::fs::write(&path, fixture.to_json()).expect("write fixture");
    eprintln!("phase3_tv_0862 fixture written: {}", path.display());
    for e in &fixture.entries {
        eprintln!(
            "  {} (test_function={}, budget_ms={}, ci_slack_factor={}, iterations={})",
            e.name, e.test_function, e.budget_ms, e.ci_slack_factor, e.iterations
        );
    }
}

/// TV-5 gate: election acquire returns within 3 s.
#[test]
fn phase3_tv_0862_election_acquire_within_3s() {
    let path = fixture_path();
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let entries = Phase3Fixture::from_json(&json)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
    let entry = entries
        .iter()
        .find(|e| e.name == "TV-5")
        .expect("TV-5 entry must exist");

    assert_eq!(
        entry.test_function, "election_acquire_returns_within_3s",
        "TV-5: test_function drift (fixture says {}, expected election_acquire_returns_within_3s)",
        entry.test_function
    );
    assert_eq!(
        entry.budget_ms, 3_000,
        "TV-5: budget_ms drift (fixture says {}, expected 3000 per RFC-0862 §Performance Targets)",
        entry.budget_ms
    );

    // Re-measure under the declared budget + CI slack factor.
    let total_bytes = tv5_election_acquire_within_3s(entry.iterations);
    let total_ms = u64::from_le_bytes(total_bytes[..8].try_into().unwrap());
    let threshold_ms = entry.budget_ms * entry.ci_slack_factor;
    assert!(
        total_ms <= threshold_ms,
        "TV-5 PERF BUDGET REGRESSION\n  budget: {} ms × ci_slack_factor {} = {} ms\n  \
         observed: {} ms across {} iterations\n  \
         Re-bootstrap via:\n  \
         UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862 phase3_tv_0862_dump -- --nocapture\n\
         \n\
         (If observed is much higher than budget under normal load, this is a \
         substrate performance regression — investigate Cluster::try_acquire_leader \
         hot path: parking_lot::Mutex acquisition + HashMap insert for leaders / terms \
         / last_heartbeat_ms per acquire.)",
        entry.budget_ms,
        entry.ci_slack_factor,
        threshold_ms,
        total_ms,
        entry.iterations
    );
}
