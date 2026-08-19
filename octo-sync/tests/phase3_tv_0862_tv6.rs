//! Mission `0862-c15` — Phase 3 Performance Test Vector
//! Fixture gate for RFC-0862 v1.3.0 TV-6.
//!
//! 1 perf-budget test vector gating Phase 3 acceptance:
//!
//! - **TV-6** — `phase3_tv_0862_tv6_drain_throughput_1k_per_sec`:
//!   100 sequential `RaftLikeDrainCoordinator::submit_drain` calls on a
//!   single shard (lease acquired once upfront via
//!   `RaftLikeWriterElection::acquire_writer`; all 100 drains commit via
//!   the leader-election path) MUST complete with per-iter wall-clock
//!   ≤ 1 ms p99 (1000 txn/s per shard budget = 1 ms/op per RFC-0862
//!   §Performance Targets). CI slack factor of 10× (assertion threshold
//!   = 10 ms p99) absorbs CI jitter without false-failing the gate.
//!
//! ## Why async harness (unlike TV-5/7/8 sync substrate)
//!
//! `RaftLikeDrainCoordinator::submit_drain` is `async fn` (per
//! RFC-0862 v1.3 §Concrete Impl + v1.4 §Concrete Impl Extension).
//! Sibling TV-5/7/8 use sync `Cluster::*` paths. Requires
//! `#[tokio::test]` + `RaftLikeWriterElection` + `RaftLikeDrainCoordinator`
//! construction following the existing pattern in
//! `cross_instance_drain_tv.rs` (single-shard variant).
//!
//! ## Per-iter latency interpretation (uniform with sibling TVs)
//!
//! Budget = 1 ms per-op (1000 txn/s = 1 ms/op); per-iter p99 ceiling.
//! Same `ci_slack_factor` × `budget_ms` formula as TV-5/7/8. Raw
//! throughput floor would require a different fixture schema.
//!
//! ## Verification pattern
//!
//! Run the gate tests (loads fixture, asserts perf budget):
//!
//! ```bash
//! cargo test -p octo-sync --test phase3_tv_0862_tv6
//! ```
//!
//! Bootstrap the fixture (deterministic; overwrites the JSON when
//! run with `UPDATE_PHASE3_TV=1`):
//!
//! ```bash
//! UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862_tv6 phase3_tv_0862_tv6_dump -- --nocapture
//! ```
//!
//! Per [[feedback-no-fabricated-commit-rule]] + RFC-0008 Class A
//! determinism: the fixture budget is canonical; the substrate must
//! stay under it. Any drift = a substrate performance regression.

#![allow(clippy::vec_init_then_push)]

use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use octo_ident::ChainId;
use octo_sync::substrate::{
    Cluster, DrainCoordinator, RaftLikeDrainCoordinator, RaftLikeWriterElection, ShardKey,
    WriterElection, WriterNodeId,
};

/// Path to the JSON fixture. Use `CARGO_MANIFEST_DIR` for
/// absolute determinism (cargo sets this env var to the
/// package root at compile time).
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures");
const FIXTURE_NAME: &str = "phase3_tv_0862_tv6.json";

/// TV-6: drain throughput ≥ 1000 txn/s per shard.
///
/// 100 sequential `RaftLikeDrainCoordinator::submit_drain` calls on a
/// single shard. Lease acquired once upfront via
/// `RaftLikeWriterElection::acquire_writer` (60 s lease — long enough
/// to cover all 100 drains). Each iter submits a drain with a unique
/// `requested_cost` (avoids any idempotency check); per-iter wall-clock
/// measured via `Instant::now()`.
///
/// Inputs (declared in fixture):
/// - `budget_ms = 1` (RFC-0862 §Performance Targets; 1000 txn/s per shard)
/// - `ci_slack_factor = 10` (CI jitter absorption)
/// - `iterations = 100`
/// - holder = `"did:octo:zHolder"`
/// - macaroon_id = `[0xA6; 16]`
/// - requested_cost = `100 + i` (per-iter unique to bypass idempotency)
///
/// Output: `Vec<u8>` containing total_ms (u64 LE) followed by
/// per-iter elapsed-us values (u64 LE each). NOT byte-exact
/// reproducible across runs (perf noise) — fixture stores BUDGET,
/// not observed value.
async fn tv6_drain_throughput_1k_per_sec(iterations: u32) -> Vec<u8> {
    let cluster = Cluster::new();
    let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
    let node_id = WriterNodeId([1u8; 32]);
    let election = Arc::new(RaftLikeWriterElection::new(
        node_id,
        cluster.clone(),
        chain_id.clone(),
    ));
    let coord = Arc::new(RaftLikeDrainCoordinator::new(
        cluster.clone(),
        chain_id.clone(),
        node_id,
        election.clone() as Arc<dyn WriterElection>,
    ));

    let holder = "did:octo:zHolder";
    let macaroon_id: [u8; 16] = [0xA6; 16];
    let shard_key = ShardKey::derive_canonical(holder.as_bytes());

    // Acquire leader lease once upfront (60 s lease covers all 100 iters).
    election
        .acquire_writer(&shard_key, 60_000)
        .await
        .expect("acquire_writer must succeed for leader");

    let start = Instant::now();
    let mut per_iter_us: Vec<u64> = Vec::with_capacity(iterations as usize);

    for i in 0..iterations {
        let t0 = Instant::now();
        let _r = coord
            .submit_drain(holder, &macaroon_id, 100 + u128::from(i))
            .await
            .unwrap_or_else(|e| panic!("submit_drain iter {i} failed: {e:?}"));
        per_iter_us.push(t0.elapsed().as_micros() as u64);
    }
    let total_ms = start.elapsed().as_millis() as u64;

    // p99 = per_iter_us[iterations - 1] after sort. Cheap p99: sort a copy.
    let mut sorted_us = per_iter_us.clone();
    sorted_us.sort_unstable();
    let p99_idx = ((sorted_us.len() as f64) * 0.99).ceil() as usize - 1;
    let p99_us = sorted_us[p99_idx.min(sorted_us.len() - 1)];
    let throughput_tps = (iterations as f64) / (total_ms as f64 / 1000.0);
    eprintln!(
        "TV-6: {iterations} submit_drain calls in {total_ms}ms total, \
         throughput = {throughput_tps:.1} txn/s, \
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
struct Phase3Tv6Fixture {
    entries: Vec<Phase3Tv6Entry>,
}

struct Phase3Tv6Entry {
    name: String,
    description: String,
    test_function: String,
    budget_ms: u64,
    ci_slack_factor: u64,
    iterations: u32,
    verification_command: String,
}

impl Phase3Tv6Fixture {
    fn compute() -> Self {
        let mut entries = Vec::new();
        entries.push(Phase3Tv6Entry {
            name: "TV-6".to_string(),
            description: "Drain throughput ≥ 1000 txn/s per shard: 100 sequential \
                `RaftLikeDrainCoordinator::submit_drain` calls on a single \
                shard (lease acquired once upfront, all 100 drains commit \
                via the leader-election path) MUST complete with per-iter \
                wall-clock ≤ 1 ms p99 (1000 txn/s = 1 ms/op budget). Per \
                RFC-0862 v1.3.0 §Performance Targets. CI slack factor 10x \
                = 10 ms assertion threshold. Substrate submit_drain = \
                leader check + WAL append (sync via cluster mutex); \
                observed well under 1 ms in current substrate."
                .to_string(),
            test_function: "drain_throughput_1k_per_sec".to_string(),
            budget_ms: 1,
            ci_slack_factor: 10,
            iterations: 100,
            verification_command: "cargo test -p octo-sync --test phase3_tv_0862_tv6 \
                phase3_tv_0862_tv6_drain_throughput_1k_per_sec -- --nocapture"
                .to_string(),
        });
        Self { entries }
    }

    fn to_json(&self) -> String {
        let mut s = String::new();
        writeln!(s).unwrap();
        s.push_str(
            "  \"_comment\": \"RFC-0862 v1.3.0 Phase 3 performance test vector TV-6 ONLY \
            (drain_throughput_1k_per_sec). Phase 3 TVs are PERFORMANCE BUDGETS — not byte-exact. \
            The fixture stores the budget (`budget_ms`, `ci_slack_factor`, `iterations`); the gate \
            test measures fresh and asserts under CI slack. Re-bootstrap via `UPDATE_PHASE3_TV=1 \
            cargo test -p octo-sync --test phase3_tv_0862_tv6 phase3_tv_0862_tv6_dump -- \
            --nocapture`. Per RFC-0008 Class A determinism: the budget is canonical; the \
            substrate must stay under it.\",\n",
        );
        s.push_str("  \"_rfc\": \"RFC-0862 v1.3.0\",\n");
        s.push_str("  \"_phase\": \"Phase 3\",\n");
        s.push_str(
            "  \"_scope\": \"TV-6 ONLY. Sibling to phase3_tv_0862.json (TV-5) + \
            phase3_tv_0862_tv7.json (TV-7) + phase3_tv_0862_tv8.json (TV-8). \
            Separate file per R17 M3 scope discipline (each TV owns its own \
            fixture file). Last of RFC-0862 v1.3.0 Phase 3 perf-budget TVs.\",\n",
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
    fn from_json(json: &str) -> Result<Vec<Phase3Tv6Entry>, String> {
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

fn parse_tv_entry(obj: &str) -> Result<Phase3Tv6Entry, String> {
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
    Ok(Phase3Tv6Entry {
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
fn phase3_tv_0862_tv6_dump() {
    if std::env::var("UPDATE_PHASE3_TV").ok().as_deref() != Some("1") {
        eprintln!(
            "phase3_tv_0862_tv6_dump is a no-op unless UPDATE_PHASE3_TV=1. \
             To bootstrap the fixture, run: \
             UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862_tv6 phase3_tv_0862_tv6_dump -- --nocapture"
        );
        return;
    }
    let fixture = Phase3Tv6Fixture::compute();
    let path = fixture_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture dir");
    }
    std::fs::write(&path, fixture.to_json()).expect("write fixture");
    eprintln!("phase3_tv_0862_tv6 fixture written: {}", path.display());
    for e in &fixture.entries {
        eprintln!(
            "  {} (test_function={}, budget_ms={}, ci_slack_factor={}, iterations={})",
            e.name, e.test_function, e.budget_ms, e.ci_slack_factor, e.iterations
        );
    }
}

/// TV-6 gate: drain throughput ≥ 1000 txn/s per shard (per-iter p99 ≤ 1 ms).
#[tokio::test]
async fn phase3_tv_0862_tv6_drain_throughput_1k_per_sec() {
    let path = fixture_path();
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let entries = Phase3Tv6Fixture::from_json(&json)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
    let entry = entries
        .iter()
        .find(|e| e.name == "TV-6")
        .expect("TV-6 entry must exist");

    assert_eq!(
        entry.test_function, "drain_throughput_1k_per_sec",
        "TV-6: test_function drift (fixture says {}, expected drain_throughput_1k_per_sec)",
        entry.test_function
    );
    assert_eq!(
        entry.budget_ms, 1,
        "TV-6: budget_ms drift (fixture says {}, expected 1 per RFC-0862 §Performance Targets)",
        entry.budget_ms
    );

    // Re-measure under the declared budget + CI slack factor.
    let total_bytes = tv6_drain_throughput_1k_per_sec(entry.iterations).await;
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
        "TV-6 PERF BUDGET REGRESSION\n  budget: {} ms p99 × ci_slack_factor {} = {} ms threshold\n  \
         observed: p99 = {p99_us}us ({p99_ms}ms), total = {total_ms}ms across {} iterations\n  \
         Re-bootstrap via:\n  \
         UPDATE_PHASE3_TV=1 cargo test -p octo-sync --test phase3_tv_0862_tv6 phase3_tv_0862_tv6_dump -- --nocapture\n\
         \n\
         (If p99 is much higher than budget under normal load, this is a \
         substrate performance regression — investigate RaftLikeDrainCoordinator::submit_drain \
         hot path: WriterElection::current_writer check + WalWriter::append_entry \
         (mutex + Vec push).)",
        entry.budget_ms,
        entry.ci_slack_factor,
        threshold_ms,
        entry.iterations
    );
}
