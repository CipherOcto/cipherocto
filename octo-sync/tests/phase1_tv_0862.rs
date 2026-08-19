//! Mission `0862-phase1-tv-fixture` — Phase 1 Test Vector Fixture for
//! RFC-0862 v1.3.0.
//!
//! 4 byte-exact test vectors (TV-1..TV-4) gating Phase 1 acceptance:
//!
//! - **TV-1** — `HLC monotonicity`: 3 consecutive `HlcClock::now()`
//!   calls across an advancing wall-clock produce strictly increasing
//!   `(physical_ms, logical, writer_node_id)` lexicographic order
//!   (per RFC-0862 v1.3 §HLC Substrate; RFC-0008 Class A
//!   determinism).
//! - **TV-2** — `HLC logical increment`: 3 consecutive
//!   `HlcClock::now()` calls on a fixed wall-clock produce
//!   `physical_ms` constant + `logical` advancing `0, 1, 2` (per
//!   RFC-0862 v1.3 §HLC Substrate; intra-millisecond logical
//!   counter).
//! - **TV-3** — `WriterIdentity caching`: a fully-populated
//!   `WriterIdentity` (writer_node_id + mission_id + term +
//!   elected_at_hlc + shard_key) borsh-serializes to a canonical
//!   148-byte payload that round-trips byte-exactly. This is the
//!   "cache equivalence" invariant: any `WriterIdentity` value the
//!   writer-election substrate caches must serialize deterministically
//!   so cross-instance replay reconstructs identical bytes.
//! - **TV-4** — `bootstrap peer acquisition`: 2 `PeerIdentity`
//!   records produced by a `MockBootstrapOrchestrator` canonicalize
//!   to a 32-byte BLAKE3 fingerprint over the sorted
//!   `(node_id, overlay_id, mission_id)` triples. This is the
//!   "acquired peer list" wire form downstream consumers
//!   hash-verify against (per RFC-0862 v1.3 §BootstrapOrchestrator).
//!
//! ## Verification pattern
//!
//! Run the gate test (loads fixture, asserts byte-exact):
//!
//! ```bash
//! cargo test -p octo-sync --test phase1_tv_0862 phase1_tv_0862_match
//! ```
//!
//! Bootstrap the fixture (deterministic; overwrites the JSON
//! when run with `UPDATE_PHASE1_TV=1`):
//!
//! ```bash
//! UPDATE_PHASE1_TV=1 cargo test -p octo-sync --test phase1_tv_0862 phase1_tv_0862_dump -- --nocapture
//! ```
//!
//! Per [[feedback-no-fabricated-commit-rule]] + RFC-0008 Class A
//! determinism: the fixture is byte-exact reproducible from the
//! declared inputs; any drift is a substrate change (RFC + test
//! together).
//!
//! ## Out of scope (NOT this fixture)
//!
//! - Phase 3 TV-5..TV-8 (election latency, drain throughput,
//!   failover pause, WAL fan-out lag) — separate mission per
//!   RFC-0862 v1.3 §Test Vectors.
// Allow `vec_init_then_push` for the `Vec<Phase1TvEntry>` builders
// below — the `vec![..]` form would split the 4 TV across many lines
// and the impl remains readable in push-form.
#![allow(clippy::vec_init_then_push)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use borsh::BorshDeserialize;
use octo_sync::substrate::{
    HlcClock, HlcTimestamp, PeerIdentity, ShardKey, ShardMissionId, WriterIdentity, WriterNodeId,
};

/// Path to the JSON fixture. Use `CARGO_MANIFEST_DIR` for
/// absolute determinism (cargo sets this env var to the
/// package root at compile time) — relative paths via
/// `current_dir()` are flaky because cargo test CWD varies
/// per-test-binary vs combined `--tests` runs. The package
/// root is `octo-sync/`, so `../tests/fixtures/...` is one
/// level up = the repo root. The previous
/// `../../../tests/fixtures/...` was a pre-existing drift that
/// happened to pass under standalone test runs but broke when
/// `phase3_tv_0862` was added to the same `cargo test --tests`
/// sweep; fixed in mission `0862-phase3-tv-fixture` to use
/// `CARGO_MANIFEST_DIR` for absolute determinism.
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures");
const FIXTURE_NAME: &str = "phase1_tv_0862.json";

/// TV-1: HLC monotonicity across an advancing wall-clock.
///
/// 3 `now()` calls in a row on a single `HlcClock` with synthetic
/// clock that increments by 1ms per call. Output: concatenation of
/// the 3 `HlcTimestamp` borsh payloads (44 bytes each = 132 bytes
/// total).
///
/// Inputs:
/// - `writer_node_id = [0u8; 32]`
/// - `clock_values = [1_700_000_000_000, 1_700_000_000_001, 1_700_000_000_002]`
///
/// Invariant: `t1 < t2 < t3` lexicographically
/// (`physical_ms` advances monotonically; `logical` stays 0 because
/// physical_ms strictly increases between calls).
fn tv1_hlc_monotonicity() -> Vec<u8> {
    let counter = AtomicU64::new(1_700_000_000_000);
    let clock = HlcClock::new_with_clock(
        WriterNodeId([0u8; 32]),
        Box::new(move || counter.fetch_add(1, Ordering::SeqCst)),
    );
    let t1 = clock.now().expect("t1 ok");
    let t2 = clock.now().expect("t2 ok");
    let t3 = clock.now().expect("t3 ok");

    // Monotonicity invariant: lexicographic `(physical_ms, logical, writer_node_id)`.
    assert!(t1 < t2, "t1 {t1:?} must be < t2 {t2:?}");
    assert!(t2 < t3, "t2 {t2:?} must be < t3 {t3:?}");
    assert_eq!(t1.writer_node_id, WriterNodeId([0u8; 32]));
    assert_eq!(t2.writer_node_id, WriterNodeId([0u8; 32]));
    assert_eq!(t3.writer_node_id, WriterNodeId([0u8; 32]));

    let mut out = Vec::with_capacity(3 * 44);
    out.extend(borsh::to_vec(&t1).expect("t1 borsh"));
    out.extend(borsh::to_vec(&t2).expect("t2 borsh"));
    out.extend(borsh::to_vec(&t3).expect("t3 borsh"));
    out
}

/// TV-2: HLC logical increment on a fixed wall-clock.
///
/// 3 `now()` calls on a single `HlcClock` with a CONSTANT synthetic
/// clock. Output: concatenation of 3 `HlcTimestamp` borsh payloads
/// with identical `physical_ms` and `logical` advancing 0, 1, 2.
///
/// Inputs:
/// - `writer_node_id = [0u8; 32]`
/// - `clock_value = 1_700_000_000_000` (constant)
///
/// Invariant: `physical_ms` constant across all 3;
/// `logical = 0, 1, 2` (monotonic within a single millisecond).
fn tv2_hlc_logical_increment() -> Vec<u8> {
    let clock =
        HlcClock::new_with_clock(WriterNodeId([0u8; 32]), Box::new(|| 1_700_000_000_000u64));
    let t1 = clock.now().expect("t1 ok");
    let t2 = clock.now().expect("t2 ok");
    let t3 = clock.now().expect("t3 ok");

    // Same physical_ms (clock is constant); logical advances.
    assert_eq!(t1.physical_ms, 1_700_000_000_000);
    assert_eq!(t2.physical_ms, 1_700_000_000_000);
    assert_eq!(t3.physical_ms, 1_700_000_000_000);
    assert_eq!(t1.logical, 0);
    assert_eq!(t2.logical, 1);
    assert_eq!(t3.logical, 2);
    assert!(t1 < t2 && t2 < t3);

    let mut out = Vec::with_capacity(3 * 44);
    out.extend(borsh::to_vec(&t1).expect("t1 borsh"));
    out.extend(borsh::to_vec(&t2).expect("t2 borsh"));
    out.extend(borsh::to_vec(&t3).expect("t3 borsh"));
    out
}

/// TV-3: `WriterIdentity` caching — canonical borsh round-trip.
///
/// A fully-populated `WriterIdentity` (writer_node_id +
/// mission_id + term + elected_at_hlc + shard_key) borsh-serializes
/// to a canonical 148-byte payload. Round-trip invariant:
/// decoded bytes == original bytes (the "cache equivalence"
/// guarantee — the writer-election substrate can store a
/// `WriterIdentity` and reconstruct it byte-exactly for cross-
/// instance replay).
///
/// Inputs:
/// - `writer_node_id = [1u8; 32]`
/// - `mission_id     = [2u8; 32]`
/// - `term           = 1`
/// - `elected_at_hlc = HlcTimestamp { physical_ms: 1_700_000_000_000, logical: 7, writer_node_id: [1u8; 32] }`
/// - `shard_key      = [3u8; 32]`
///
/// Output: 148-byte borsh payload (32 + 32 + 8 + 44 + 32).
fn tv3_writer_identity_canonical() -> Vec<u8> {
    let identity = WriterIdentity {
        writer_node_id: WriterNodeId([1u8; 32]),
        mission_id: ShardMissionId([2u8; 32]),
        term: 1,
        elected_at_hlc: HlcTimestamp {
            physical_ms: 1_700_000_000_000,
            logical: 7,
            writer_node_id: WriterNodeId([1u8; 32]),
        },
        shard_key: ShardKey([3u8; 32]),
    };
    let bytes = borsh::to_vec(&identity).expect("WriterIdentity borsh");
    assert_eq!(
        bytes.len(),
        148,
        "WriterIdentity borsh must be 148 bytes (32 + 32 + 8 + 44 + 32); got {}",
        bytes.len()
    );

    // Round-trip invariant: the cache MUST return byte-exact equivalent.
    let decoded = WriterIdentity::try_from_slice(&bytes).expect("WriterIdentity round-trip");
    assert_eq!(decoded, identity, "WriterIdentity round-trip drift");

    bytes
}

/// TV-4: bootstrap peer acquisition — canonical peer-list fingerprint.
///
/// 2 `PeerIdentity` records canonicalize to a 32-byte BLAKE3
/// fingerprint over the sorted `(node_id, overlay_id, mission_id)`
/// triples. This is the "acquired peer list" wire form downstream
/// consumers hash-verify against.
///
/// Inputs:
/// - peer[0] = `PeerIdentity { node_id: [0xA0; 32], overlay_id: [0xB0; 32], mission_id: [0xC0; 32] }`
/// - peer[1] = `PeerIdentity { node_id: [0xA1; 32], overlay_id: [0xB1; 32], mission_id: [0xC1; 32] }`
///
/// Output: 32-byte BLAKE3 hash of
/// `peer[0].node_id || peer[0].overlay_id || peer[0].mission_id
///  || peer[1].node_id || peer[1].overlay_id || peer[1].mission_id`.
fn tv4_bootstrap_peer_acquisition() -> Vec<u8> {
    let peers = vec![
        PeerIdentity {
            node_id: WriterNodeId([0xA0; 32]),
            overlay_id: [0xB0; 32],
            mission_id: ShardMissionId([0xC0; 32]),
        },
        PeerIdentity {
            node_id: WriterNodeId([0xA1; 32]),
            overlay_id: [0xB1; 32],
            mission_id: ShardMissionId([0xC1; 32]),
        },
    ];
    let mut hasher = blake3::Hasher::new();
    for p in &peers {
        hasher.update(&p.node_id.0);
        hasher.update(&p.overlay_id);
        hasher.update(&p.mission_id.0);
    }
    hasher.finalize().as_bytes().to_vec()
}

/// Canonical 4-TV fixture struct. JSON serialization is hand-rolled
/// to keep the file diff-friendly (one TV per `name` key).
struct Phase1Fixture {
    entries: Vec<Phase1TvEntry>,
}

struct Phase1TvEntry {
    name: String,
    description: String,
    inputs_hex: Vec<(String, String)>,
    outputs_hex: String,
    byte_len: usize,
    verification_command: String,
}

impl Phase1Fixture {
    fn compute() -> Self {
        let mut entries = Vec::new();
        entries.push(Phase1TvEntry {
            name: "TV-1".to_string(),
            description: "HLC monotonicity: 3 now() calls across advancing wall-clock \
                (1ms per call) — physical_ms strictly increases, logical stays 0."
                .to_string(),
            inputs_hex: vec![
                (
                    "writer_node_id".to_string(),
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                ),
                (
                    "clock_values".to_string(),
                    "0068e5cf8b0100000168e5cf8b0100000268e5cf8b010000".to_string(),
                ),
            ],
            outputs_hex: hex::encode(tv1_hlc_monotonicity()),
            byte_len: 132,
            verification_command: "cargo test -p octo-sync --test phase1_tv_0862 \
                phase1_tv_0862_match -- --nocapture"
                .to_string(),
        });
        entries.push(Phase1TvEntry {
            name: "TV-2".to_string(),
            description: "HLC logical increment: 3 now() calls on a fixed wall-clock — \
                physical_ms constant, logical advances 0, 1, 2."
                .to_string(),
            inputs_hex: vec![
                (
                    "writer_node_id".to_string(),
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                ),
                ("clock_value".to_string(), "0068e5cf8b010000".to_string()),
            ],
            outputs_hex: hex::encode(tv2_hlc_logical_increment()),
            byte_len: 132,
            verification_command: "cargo test -p octo-sync --test phase1_tv_0862 \
                phase1_tv_0862_match -- --nocapture"
                .to_string(),
        });
        entries.push(Phase1TvEntry {
            name: "TV-3".to_string(),
            description: "WriterIdentity caching: 148-byte borsh payload (32 + 32 + 8 + \
                44 + 32) round-trips byte-exactly — cache equivalence invariant."
                .to_string(),
            inputs_hex: vec![
                (
                    "writer_node_id".to_string(),
                    "0101010101010101010101010101010101010101010101010101010101010101".to_string(),
                ),
                (
                    "mission_id".to_string(),
                    "0202020202020202020202020202020202020202020202020202020202020202".to_string(),
                ),
                ("term".to_string(), "0100000000000000".to_string()),
                (
                    "elected_at_hlc_physical_ms".to_string(),
                    "0068e5cf8b010000".to_string(),
                ),
                ("elected_at_hlc_logical".to_string(), "07000000".to_string()),
                (
                    "elected_at_hlc_writer_node_id".to_string(),
                    "0101010101010101010101010101010101010101010101010101010101010101".to_string(),
                ),
                (
                    "shard_key".to_string(),
                    "0303030303030303030303030303030303030303030303030303030303030303".to_string(),
                ),
            ],
            outputs_hex: hex::encode(tv3_writer_identity_canonical()),
            byte_len: 148,
            verification_command: "cargo test -p octo-sync --test phase1_tv_0862 \
                phase1_tv_0862_match -- --nocapture"
                .to_string(),
        });
        entries.push(Phase1TvEntry {
            name: "TV-4".to_string(),
            description: "Bootstrap peer acquisition: 2 PeerIdentity records canonicalize \
                to a 32-byte BLAKE3 fingerprint over \
                (node_id || overlay_id || mission_id) for each peer."
                .to_string(),
            inputs_hex: vec![
                (
                    "peer_0_node_id".to_string(),
                    "a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0".to_string(),
                ),
                (
                    "peer_0_overlay_id".to_string(),
                    "b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0".to_string(),
                ),
                (
                    "peer_0_mission_id".to_string(),
                    "c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0".to_string(),
                ),
                (
                    "peer_1_node_id".to_string(),
                    "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1".to_string(),
                ),
                (
                    "peer_1_overlay_id".to_string(),
                    "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1".to_string(),
                ),
                (
                    "peer_1_mission_id".to_string(),
                    "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1".to_string(),
                ),
            ],
            outputs_hex: hex::encode(tv4_bootstrap_peer_acquisition()),
            byte_len: 32,
            verification_command: "cargo test -p octo-sync --test phase1_tv_0862 \
                phase1_tv_0862_match -- --nocapture"
                .to_string(),
        });
        Self { entries }
    }

    fn to_json(&self) -> String {
        let mut s = String::new();
        s.push_str("{\n");
        s.push_str(
            "  \"_comment\": \"RFC-0862 v1.3.0 Phase 1 test vectors (TV-1..TV-4). \
            Regenerate via `UPDATE_PHASE1_TV=1 cargo test -p octo-sync --test \
            phase1_tv_0862 phase1_tv_0862_dump -- --nocapture`. Per RFC-0008 Class A \
            determinism: every output is byte-exact reproducible from the declared inputs.\",\n",
        );
        s.push_str("  \"_rfc\": \"RFC-0862 v1.3.0\",\n");
        s.push_str("  \"_phase\": \"Phase 1\",\n");
        s.push_str(
            "  \"_scope\": \"TV-1..TV-4 ONLY. Phase 3 TV-5..TV-8 land in a \
            separate fixture + mission per RFC-0862 v1.3.0 §Test Vectors.\",\n",
        );
        s.push_str("  \"entries\": [\n");
        for (i, e) in self.entries.iter().enumerate() {
            s.push_str("    {\n");
            s.push_str(&format!("      \"name\": \"{}\",\n", e.name));
            s.push_str(&format!("      \"description\": \"{}\",\n", e.description));
            s.push_str("      \"inputs\": {\n");
            for (j, (k, v)) in e.inputs_hex.iter().enumerate() {
                let comma = if j + 1 < e.inputs_hex.len() { "," } else { "" };
                s.push_str(&format!("        \"{k}\": \"{v}\"{comma}\n"));
            }
            s.push_str("      },\n");
            s.push_str(&format!("      \"outputs_hex\": \"{}\",\n", e.outputs_hex));
            s.push_str(&format!("      \"byte_len\": {},\n", e.byte_len));
            s.push_str(&format!(
                "      \"verification_command\": \"{}\"\n",
                e.verification_command
            ));
            let comma = if i + 1 < self.entries.len() { "," } else { "" };
            s.push_str(&format!("    }}{comma}\n"));
        }
        s.push_str("  ]\n");
        s.push_str("}\n");
        s
    }

    /// Parse the JSON fixture, returning the 4 entries. Hand-rolled
    /// minimal parser to avoid adding a `serde_json` dep to
    /// `octo-sync`'s dev-dependencies.
    fn from_json(json: &str) -> Result<Vec<Phase1TvEntry>, String> {
        // Locate the "entries": [ ... ] block.
        let entries_start = json
            .find("\"entries\": [")
            .ok_or_else(|| "missing entries key".to_string())?;
        let after_array = entries_start + "\"entries\": [".len();
        let entries_end_rel = json[after_array..]
            .find("]\n")
            .ok_or_else(|| "missing entries closing".to_string())?;
        let entries_block = &json[after_array..after_array + entries_end_rel];

        // Split on top-level "{ ... }" objects.
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

fn parse_tv_entry(obj: &str) -> Result<Phase1TvEntry, String> {
    let name = extract_string(obj, "name")?;
    let description = extract_string(obj, "description")?;
    let outputs_hex = extract_string(obj, "outputs_hex")?;
    let byte_len_str = extract_number(obj, "byte_len")?;
    let byte_len: usize = byte_len_str
        .parse()
        .map_err(|e| format!("byte_len parse: {e}"))?;
    let verification_command = extract_string(obj, "verification_command")?;

    // Parse inputs: a nested { "k": "v", ... } object.
    let inputs_start = obj
        .find("\"inputs\": {")
        .ok_or_else(|| "missing inputs key".to_string())?;
    let after_inputs = inputs_start + "\"inputs\": {".len();
    let inputs_end_rel = obj[after_inputs..]
        .find('}')
        .ok_or_else(|| "missing inputs close".to_string())?;
    let inputs_block = &obj[after_inputs..after_inputs + inputs_end_rel];
    let mut inputs_hex = Vec::new();
    for line in inputs_block.lines() {
        let line = line.trim().trim_end_matches(',');
        if line.is_empty() {
            continue;
        }
        // Format: "key": "value"
        let colon = line
            .find(": \"")
            .ok_or_else(|| "bad input line".to_string())?;
        let key = &line[1..colon];
        let value_start = colon + 3;
        let value_end = line.len() - 1; // strip closing "
        let value = &line[value_start..value_end];
        inputs_hex.push((key.to_string(), value.to_string()));
    }
    Ok(Phase1TvEntry {
        name,
        description,
        inputs_hex,
        outputs_hex,
        byte_len,
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

/// Extract an unquoted JSON number (integer) value for `key`.
/// Used for `byte_len` (the only numeric field in the fixture).
fn extract_number(obj: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\": ");
    let start = obj
        .find(&needle)
        .ok_or_else(|| format!("missing key: {key}"))?;
    let after = start + needle.len();
    let rest = &obj[after..];
    // Take chars until a delimiter (`,`, `}`, whitespace).
    let end = rest
        .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
        .ok_or_else(|| format!("unterminated number: {key}"))?;
    Ok(rest[..end].to_string())
}

/// Resolve the fixture path via the compile-time
/// `CARGO_MANIFEST_DIR` constant. Absolute + deterministic
/// (does not depend on `cargo test`'s CWD, which varies per
/// integration test binary vs combined `--tests` sweep).
fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(FIXTURE_DIR);
    p.push(FIXTURE_NAME);
    p
}

/// Dump mode: regenerate the JSON fixture from the canonical
/// reference impl. Run with `UPDATE_PHASE1_TV=1` to bootstrap (or
/// refresh after a substrate change). The dump is deterministic —
/// identical inputs produce identical outputs across runs.
#[test]
fn phase1_tv_0862_dump() {
    if std::env::var("UPDATE_PHASE1_TV").ok().as_deref() != Some("1") {
        eprintln!(
            "phase1_tv_0862_dump is a no-op unless UPDATE_PHASE1_TV=1. \
             To bootstrap the fixture, run: \
             UPDATE_PHASE1_TV=1 cargo test -p octo-sync --test phase1_tv_0862 phase1_tv_0862_dump -- --nocapture"
        );
        return;
    }
    let fixture = Phase1Fixture::compute();
    let path = fixture_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture dir");
    }
    std::fs::write(&path, fixture.to_json()).expect("write fixture");
    eprintln!("phase1_tv_0862 fixture written: {}", path.display());
    for e in &fixture.entries {
        eprintln!(
            "  {} ({} bytes): outputs_hex={}",
            e.name, e.byte_len, e.outputs_hex
        );
    }
}

/// Gate mode: load the JSON fixture, re-derive each TV from the
/// declared inputs, assert byte-exact match against the recorded
/// `outputs_hex`. This is the test that lands in CI.
#[test]
fn phase1_tv_0862_match() {
    let path = fixture_path();
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let entries = Phase1Fixture::from_json(&json)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
    assert_eq!(
        entries.len(),
        4,
        "fixture must contain exactly 4 TV entries"
    );

    // Map name -> computed output.
    let mut computed: std::collections::HashMap<&str, Vec<u8>> = std::collections::HashMap::new();
    computed.insert("TV-1", tv1_hlc_monotonicity());
    computed.insert("TV-2", tv2_hlc_logical_increment());
    computed.insert("TV-3", tv3_writer_identity_canonical());
    computed.insert("TV-4", tv4_bootstrap_peer_acquisition());

    for entry in &entries {
        let actual = computed
            .get(entry.name.as_str())
            .unwrap_or_else(|| panic!("unknown TV name: {}", entry.name));
        let expected = hex::decode(&entry.outputs_hex).expect("outputs_hex must be valid hex");
        assert_eq!(
            actual.len(),
            entry.byte_len,
            "{}: byte_len mismatch (fixture says {}, computed {})",
            entry.name,
            entry.byte_len,
            actual.len()
        );
        assert_eq!(
            actual.as_slice(),
            expected.as_slice(),
            "{}: outputs_hex drift\n  expected: {}\n  actual:   {}\n\
             Re-bootstrap via:\n  UPDATE_PHASE1_TV=1 cargo test -p octo-sync --test phase1_tv_0862 phase1_tv_0862_dump -- --nocapture",
            entry.name,
            entry.outputs_hex,
            hex::encode(actual)
        );
    }
}
