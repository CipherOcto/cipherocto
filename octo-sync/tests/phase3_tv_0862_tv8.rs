//! Mission `0862-c13` — Phase 3 Performance Test Vector
//! Fixture gate for RFC-0862 v1.3.0 TV-8.
//!
//! 1 perf-budget test vector gating Phase 3 acceptance:
//!
//! - **TV-8** — `phase3_tv_0862_tv8_wal_fanout_lag_under_100ms`:
//!   100 sequential `Cluster::append_wal_entry` + `Cluster::read_wal_range`
//!   round-trips (writer + reader share the same `Arc<Cluster>` state —
//!   fan-out IS shared-state propagation in the test harness, no separate
//!   replication layer) MUST complete with per-iter round-trip wall-clock
//!   ≤ 100 ms (p99 budget per RFC-0862 §Performance Targets). CI slack
//!   factor of 10× (assertion threshold = 1000 ms p99) absorbs CI
//!   jitter without false-failing the gate.
//!
//! ## Why TV-8 substrate is sync (simplest perf-budget harness)
//!
//! `Cluster::append_wal_entry` is sync — no async runtime needed. Sibling
//! to `phase3_tv_0862.rs` (TV-5, LANDED 2026-08-19) which uses the same
//! pattern with `Cluster::try_acquire_leader`. Per R17 M3 scope discipline,
//! each TV owns its own fixture + gate-test file (separate from TV-5).
//!
//! ## Substrate fan-out note
//!
//! No dedicated `WalFanOut` type exists in the substrate — fan-out IS
//! the shared `Arc<Cluster>` state propagation. Reader via
//! `read_wal_range(from_lsn, from_lsn+1)` sees appended entry
//! immediately (deterministic — no async replication layer in test
//! harness). Measures append→read round-trip wall-clock.
//!
//! ## Verification pattern
//!
//! Run the gate tests (loads fixture, asserts perf budget):
//!
//! ```bash
//! cargo test -p octo-sync --test phase3_tv_0862_tv8
//! ```
//!
//! Bootstrap the fixture (deterministic; overwrites the JSON when
//! run with `UPDATE_PHASE3_TV=1`):
//!
//! ```bash
//! UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862_tv8 phase3_tv_0862_tv8_dump -- --nocapture
//! ```
//!
//! Per [[feedback-no-fabricated-commit-rule]] + RFC-0008 Class A
//! determinism: the fixture budget is canonical; the substrate must
//! stay under it. Any drift = a substrate performance regression.
//!
//! ## Out of scope (NOT this fixture)
//!
//! - TV-6 (`drain_throughput_1k_per_sec`) — follow-on mission per
//!   R17 M3. Requires async `RaftLikeDrainCoordinator::submit_drain`
//!   harness.
//! - TV-7 (`failover_pause_under_3s`) — follow-on mission.
//!   Requires `set_lease_duration_ms(0)` + `kill` + cross-node
//!   re-acquire pattern.

#![allow(clippy::vec_init_then_push)]

use std::fmt::Write;
use std::path::PathBuf;
use std::time::Instant;

use octo_sync::substrate::{Cluster, ShardKey, WalEntry, ENTRY_TYPE_DRAIN};

/// Path to the JSON fixture. Use `CARGO_MANIFEST_DIR` for
/// absolute determinism (cargo sets this env var to the
/// package root at compile time) — relative paths via
/// `current_dir()` are flaky because cargo test CWD varies
/// per-test-binary vs combined `--tests` runs. The package
/// root is `octo-sync/`, so `../tests/fixtures/...` is one
/// level up = the repo root.
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures");
const FIXTURE_NAME: &str = "phase3_tv_0862_tv8.json";

/// TV-8: WAL fan-out lag under 100 ms p99.
///
/// 100 sequential `Cluster::append_wal_entry` + `Cluster::read_wal_range`
/// round-trips on a fresh `Cluster`, each with a unique `ShardKey` and
/// payload. Measure per-iter wall-clock (Instant::now() before append,
/// after read confirms LSN delivered). Compute p99 across iterations;
/// assert under CI slack.
///
/// Inputs (declared in fixture):
/// - `budget_ms = 100` (RFC-0862 §Performance Targets)
/// - `ci_slack_factor = 10` (CI jitter absorption)
/// - `iterations = 100`
/// - `entry_type = ENTRY_TYPE_DRAIN` (drain-record entry)
/// - `shard_key = [i;32]` (per-iter unique)
/// - `payload = vec![i as u8; 64]` (64-byte payload)
///
/// Output: `Vec<u8>` containing total_ms (u64 LE) followed by
/// per-iter elapsed-us values (u64 LE each). NOT byte-exact
/// reproducible across runs (perf noise) — fixture stores BUDGET,
/// not observed value.
fn tv8_wal_fanout_lag_under_100ms(iterations: u32) -> Vec<u8> {
    let cluster = Cluster::new();
    let mut per_iter_us: Vec<u64> = Vec::with_capacity(iterations as usize);
    let start = Instant::now();

    for i in 0..iterations {
        let shard_key = ShardKey({
            let mut bytes = [0u8; 32];
            bytes[0] = i as u8;
            bytes
        });
        let payload: Vec<u8> = vec![i as u8; 64];
        let entry = WalEntry::build_v13(ENTRY_TYPE_DRAIN, shard_key, payload);

        let t0 = Instant::now();
        let lsn = cluster
            .append_wal_entry(entry)
            .unwrap_or_else(|e| panic!("append iter {i} failed: {e:?}"));
        // Round-trip read confirms entry delivered to fan-out.
        let read_back = cluster.read_wal_range(lsn, Some(lsn + 1));
        assert_eq!(
            read_back.len(),
            1,
            "iter {i}: read_wal_range must return exactly the appended entry"
        );
        assert_eq!(
            read_back[0].lsn, lsn,
            "iter {i}: read-back LSN must match appended LSN (fan-out lag invariant)"
        );
        per_iter_us.push(t0.elapsed().as_micros() as u64);
    }
    let total_ms = start.elapsed().as_millis() as u64;

    // p99 = per_iter_us[iterations - 1] after sort. Cheap p99: sort a copy.
    let mut sorted_us = per_iter_us.clone();
    sorted_us.sort_unstable();
    let p99_idx = ((sorted_us.len() as f64) * 0.99).ceil() as usize - 1;
    let p99_us = sorted_us[p99_idx.min(sorted_us.len() - 1)];
    eprintln!(
        "TV-8: {iterations} append+read round-trips in {total_ms}ms total, \
         p99 = {p99_us}us ({:.3}ms), max = {}us",
        p99_us as f64 / 1000.0,
        sorted_us.last().copied().unwrap_or(0)
    );

    let mut out = Vec::with_capacity(8 + 8 * iterations as usize);
    out.extend_from_slice(&total_ms.to_le_bytes());
    for us in per_iter_us {
        out.extend_from_slice(&us.to_le_bytes());
    }
    out
}

/// Canonical 1-TV fixture struct. JSON serialization is hand-rolled
/// (no `serde_json` dep in this test module's emit path) to keep the
/// fixture diff-friendly.
struct Phase3Tv8Fixture {
    entries: Vec<Phase3Tv8Entry>,
}

struct Phase3Tv8Entry {
    name: String,
    description: String,
    test_function: String,
    budget_ms: u64,
    ci_slack_factor: u64,
    iterations: u32,
    verification_command: String,
}

impl Phase3Tv8Fixture {
    fn compute() -> Self {
        let mut entries = Vec::new();
        entries.push(Phase3Tv8Entry {
            name: "TV-8".to_string(),
            description: "WAL fan-out lag under 100 ms p99: 100 sequential \
                `Cluster::append_wal_entry` + `Cluster::read_wal_range` \
                round-trips on a single Cluster (writer + reader share \
                `Arc<Cluster>` state — no separate replication layer in \
                test harness) MUST complete with p99 round-trip ≤ 100 ms \
                wall-clock. Per RFC-0862 v1.3.0 §Performance Targets. CI \
                slack factor 10x = 1000 ms assertion threshold. Substrate \
                fan-out = mutex acquire + HashMap insert; observed well \
                under 1 ms in current substrate."
                .to_string(),
            test_function: "wal_fanout_lag_under_100ms".to_string(),
            budget_ms: 100,
            ci_slack_factor: 10,
            iterations: 100,
            verification_command: "cargo test -p octo-sync --test phase3_tv_0862_tv8 \
                phase3_tv_0862_tv8_wal_fanout_lag_under_100ms -- --nocapture"
                .to_string(),
        });
        Self { entries }
    }

    fn to_json(&self) -> String {
        let mut s = String::new();
        writeln!(s).unwrap();
        s.push_str(
            "  \"_comment\": \"RFC-0862 v1.3.0 Phase 3 performance test vector TV-8 ONLY \
            (wal_fanout_lag_under_100ms). Phase 3 TVs are PERFORMANCE BUDGETS — not byte-exact. \
            The fixture stores the budget (`budget_ms`, `ci_slack_factor`, `iterations`); the gate \
            test measures fresh and asserts under CI slack. Re-bootstrap via `UPDATE_PHASE3_TV=1 \
            cargo test -p octo-sync --test phase3_tv_0862_tv8 phase3_tv_0862_tv8_dump -- \
            --nocapture`. Per RFC-0008 Class A determinism: the budget is canonical; the \
            substrate must stay under it.\",\n",
        );
        s.push_str("  \"_rfc\": \"RFC-0862 v1.3.0\",\n");
        s.push_str("  \"_phase\": \"Phase 3\",\n");
        s.push_str(
            "  \"_scope\": \"TV-8 ONLY. Sibling to phase3_tv_0862.json (TV-5, LANDED 2026-08-19). \
            Separate file per R17 M3 scope discipline (each TV owns its own fixture file). \
            TV-6/7 land in separate follow-on missions.\",\n",
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
    fn from_json(json: &str) -> Result<Vec<Phase3Tv8Entry>, String> {
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

fn parse_tv_entry(obj: &str) -> Result<Phase3Tv8Entry, String> {
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
    Ok(Phase3Tv8Entry {
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
fn phase3_tv_0862_tv8_dump() {
    if std::env::var("UPDATE_PHASE3_TV").ok().as_deref() != Some("1") {
        eprintln!(
            "phase3_tv_0862_tv8_dump is a no-op unless UPDATE_PHASE3_TV=1. \
             To bootstrap the fixture, run: \
             UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862_tv8 phase3_tv_0862_tv8_dump -- --nocapture"
        );
        return;
    }
    let fixture = Phase3Tv8Fixture::compute();
    let path = fixture_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture dir");
    }
    std::fs::write(&path, fixture.to_json()).expect("write fixture");
    eprintln!("phase3_tv_0862_tv8 fixture written: {}", path.display());
    for e in &fixture.entries {
        eprintln!(
            "  {} (test_function={}, budget_ms={}, ci_slack_factor={}, iterations={})",
            e.name, e.test_function, e.budget_ms, e.ci_slack_factor, e.iterations
        );
    }
}

/// TV-8 gate: WAL fan-out lag under 100 ms p99.
#[test]
fn phase3_tv_0862_tv8_wal_fanout_lag_under_100ms() {
    let path = fixture_path();
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let entries = Phase3Tv8Fixture::from_json(&json)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
    let entry = entries
        .iter()
        .find(|e| e.name == "TV-8")
        .expect("TV-8 entry must exist");

    assert_eq!(
        entry.test_function, "wal_fanout_lag_under_100ms",
        "TV-8: test_function drift (fixture says {}, expected wal_fanout_lag_under_100ms)",
        entry.test_function
    );
    assert_eq!(
        entry.budget_ms, 100,
        "TV-8: budget_ms drift (fixture says {}, expected 100 per RFC-0862 §Performance Targets)",
        entry.budget_ms
    );

    // Re-measure under the declared budget + CI slack factor.
    let total_bytes = tv8_wal_fanout_lag_under_100ms(entry.iterations);
    let total_ms = u64::from_le_bytes(total_bytes[..8].try_into().unwrap());

    // Compute p99 from per-iter samples.
    let per_iter_us: Vec<u64> = total_bytes[8..]
        .chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect();
    let mut sorted = per_iter_us.clone();
    sorted.sort_unstable();
    let p99_idx = ((sorted.len() as f64) * 0.99).ceil() as usize - 1;
    let p99_us = sorted[p99_idx.min(sorted.len() - 1)];
    let p99_ms = ((p99_us as f64) / 1000.0).ceil() as u64;

    // Assert under threshold (budget × ci_slack_factor).
    let threshold_ms = entry.budget_ms * entry.ci_slack_factor;
    assert!(
        p99_ms <= threshold_ms,
        "TV-8 PERF BUDGET REGRESSION\n  budget: {} ms p99 × ci_slack_factor {} = {} ms threshold\n  \
         observed: p99 = {p99_us}us ({p99_ms}ms), total = {total_ms}ms across {} iterations\n  \
         Re-bootstrap via:\n  \
         UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862_tv8 phase3_tv_0862_tv8_dump -- --nocapture\n\
         \n\
         (If p99 is much higher than budget under normal load, this is a \
         substrate performance regression — investigate Cluster::append_wal_entry + \
         Cluster::read_wal_range hot path: parking_lot::Mutex acquire + Vec push/pop.)",
        entry.budget_ms,
        entry.ci_slack_factor,
        threshold_ms,
        entry.iterations
    );
}
