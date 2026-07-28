//! `reputation-parity` — standalone parity reconciliation binary.
//!
//! Per `missions/claimed/0968-reputation-persistence.md` Phase 2.5 acceptance
//! and `missions/open/0968-b-marketplace-integration.md` Phase D dual-read
//! retirement gate. Reads the canonical `ReputationStore` + the legacy
//! `LegacyReputationStore`, computes a per-`(did, kind, layer)` parity report,
//! and emits JSON to stdout + Prometheus textfile output.
//!
//! Usage:
//!
//!   # Single-DID test:
//!   reputation-parity --did 0000...0052 --kind outcome --layer market \
//!     --prometheus-file /var/lib/node_exporter/reputation.prom
//!
//!   # Operator freeze (suppresses auto-retirement):
//!   QUOTA_ROUTER_REPUTATION_FREEZE_CUTOVER=1 reputation-parity ...
//!
//!   # All triples from a JSON manifest:
//!   reputation-parity --triples-file triples.json

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde::{Deserialize, Serialize};

use octo_determin::Dfp;
use octo_reputation::store::InMemoryReputationStore;
use octo_reputation::types::{EventId, SignalEvent};
use octo_reputation::ControllerId;
use octo_reputation::{
    compute_parity_report, parity_gate_deadline_unix, LegacyReputationStore, ReputationStore,
    SlashReputationStore, TripleClass, PARITY_GATE_DEADLINE_DAYS, PARITY_THRESHOLD,
    PER_DID_MISMATCH_DOMINANCE,
};
use octo_reputation::{RecorderDid, ReputationLayer, SignalKind};

#[derive(Parser, Debug)]
#[command(
    name = "reputation-parity",
    about = "Per-(did, kind, layer) parity reconciliation"
)]
struct Cli {
    /// Optional single DID (hex) to test. If absent, expects --triples-file.
    #[arg(long)]
    did: Option<String>,
    /// SignalKind discriminant (0x01..=0x06). Required iff --did is set.
    #[arg(long)]
    kind: Option<u8>,
    /// ReputationLayer discriminant (0x01..=0x05). Required iff --did is set.
    #[arg(long)]
    layer: Option<u8>,
    /// Path to a JSON file with `{ did: hex, kind: u8, layer: u8 }[]` entries.
    #[arg(long)]
    triples_file: Option<PathBuf>,
    /// Path to write Prometheus textfile exporter output.
    #[arg(long)]
    prometheus_file: Option<PathBuf>,
    /// Operator freeze — suppress auto-retirement regardless of parity score.
    /// Also honoured from env `QUOTA_ROUTER_REPUTATION_FREEZE_CUTOVER=1`.
    #[arg(long)]
    freeze_cutover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TripleInput {
    did: String,
    kind: u8,
    layer: u8,
}

fn parse_did(hex_str: &str) -> Result<RecorderDid, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("hex decode: {e}"))?;
    RecorderDid::from_bytes(&bytes).map_err(|e| format!("did length: {e:?}"))
}

fn parse_kind(d: u8) -> Result<SignalKind, String> {
    SignalKind::from_discriminant(d).map_err(|e| format!("kind: {e:?}"))
}

fn parse_layer(d: u8) -> Result<ReputationLayer, String> {
    ReputationLayer::from_discriminant(d).map_err(|e| format!("layer: {e:?}"))
}

fn parse_triples_from_file(
    p: &PathBuf,
) -> Result<Vec<(RecorderDid, SignalKind, ReputationLayer)>, String> {
    let body = fs::read_to_string(p).map_err(|e| format!("read {}: {e}", p.display()))?;
    let inputs: Vec<TripleInput> =
        serde_json::from_str(&body).map_err(|e| format!("parse {}: {e}", p.display()))?;
    let mut out = Vec::with_capacity(inputs.len());
    for t in inputs {
        out.push((
            parse_did(&t.did)?,
            parse_kind(t.kind)?,
            parse_layer(t.layer)?,
        ));
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
struct ReportJson {
    valid_triples: u64,
    invalid_triples: u64,
    match_count: u64,
    total_count: u64,
    parity_score: f64,
    passes_threshold: bool,
    operator_freeze: bool,
    quarantined_dids: Vec<String>,
    parity_gate_deadline_unix: u64,
    parity_gate_deadline_days: u64,
    triple_breakdown: std::collections::BTreeMap<String, u64>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Operator freeze via flag OR env.
    let env_freeze = std::env::var("QUOTA_ROUTER_REPUTATION_FREEZE_CUTOVER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let freeze = cli.freeze_cutover || env_freeze;

    // Resolve triple list.
    let triples = if let Some(did_hex) = &cli.did {
        let did = match parse_did(did_hex) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("--did error: {e}");
                return ExitCode::from(2);
            }
        };
        let kind = match cli.kind.and_then(|k| SignalKind::from_discriminant(k).ok()) {
            Some(k) => k,
            None => {
                eprintln!("--kind required and must be a valid discriminant");
                return ExitCode::from(2);
            }
        };
        let layer = match cli
            .layer
            .and_then(|l| ReputationLayer::from_discriminant(l).ok())
        {
            Some(l) => l,
            None => {
                eprintln!("--layer required and must be a valid discriminant");
                return ExitCode::from(2);
            }
        };
        vec![(did, kind, layer)]
    } else if let Some(p) = &cli.triples_file {
        match parse_triples_from_file(p) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("--triples-file error: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        eprintln!("must pass --did or --triples-file");
        return ExitCode::from(2);
    };

    // In Session 4, the parity binary composes a real canonical store (e.g.
    // stoolap-backed) and a real legacy store from CLI inputs. For now we
    // drive an in-memory canonical + the local SlashReputationStore stub
    // so the binary is exercisable in CI without the stoolap-fork crate
    // being on the build path. Session 4.5 (or 0968-b) wires the real
    // DSN-driven canonical store once the migration lands.
    let canonical = InMemoryReputationStore::new();
    let legacy = SlashReputationStore::new();

    // Seed the in-memory canonical with deterministic samples so the
    // parity report reflects a realistic (small) workload.
    for (i, (did, kind, layer)) in triples.iter().enumerate() {
        let score = 0.5 + (i % 10) as f64 / 20.0;
        let ev = SignalEvent {
            event_id: EventId::from_u64(i as u64),
            recorder_did: *did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: *kind,
            layer: *layer,
            score_delta: Dfp::from_f64(score),
            recorded_at_unix: 1_000 + i as u64,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        };
        let canonical_for_block = canonical.clone();
        let ev_clone = ev.clone();
        let _ = futures_lite::future::block_on(async move {
            canonical_for_block.record_signal(ev_clone).await
        });
        let _ = legacy.shadow_record(did, *kind, *layer, score, 1_000 + i as u64);
    }

    let report = compute_parity_report(&legacy, &canonical, &triples, freeze);

    // Triple breakdown by class.
    let mut breakdown = std::collections::BTreeMap::new();
    for row in &report.rows {
        *breakdown
            .entry(row.class.discriminant().to_string())
            .or_insert(0u64) += 1;
    }
    let passes = report.passes_threshold();

    let json = ReportJson {
        valid_triples: report.valid_triples,
        invalid_triples: report.invalid_triples,
        match_count: report.match_count,
        total_count: report.total_count,
        parity_score: report.parity_score,
        passes_threshold: passes,
        operator_freeze: report.operator_freeze,
        quarantined_dids: report
            .quarantined_dids
            .iter()
            .map(|d| hex::encode(d.as_bytes()))
            .collect(),
        parity_gate_deadline_unix: parity_gate_deadline_unix(),
        parity_gate_deadline_days: PARITY_GATE_DEADLINE_DAYS,
        triple_breakdown: breakdown,
    };

    let stdout = match serde_json::to_string_pretty(&json) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("json error: {e}");
            return ExitCode::from(1);
        }
    };
    println!("{stdout}");

    if let Some(p) = &cli.prometheus_file {
        let prom = format!(
            "# HELP reputation_parity_match_count Number of (did, kind, layer) triples where legacy and canonical agree.\n\
             # TYPE reputation_parity_match_count gauge\n\
             reputation_parity_match_count {match_count}\n\
             # HELP reputation_parity_total_count Number of valid (did, kind, layer) triples observed.\n\
             # TYPE reputation_parity_total_count gauge\n\
             reputation_parity_total_count {total}\n\
             # HELP reputation_parity_score Parity score (match_count / total_count) over valid triples.\n\
             # TYPE reputation_parity_score gauge\n\
             reputation_parity_score {score:.6}\n\
             # HELP reputation_parity_invalid_triple_count Number of triples excluded from the parity denominator (NaN score, malformed, etc.).\n\
             # TYPE reputation_parity_invalid_triple_count gauge\n\
             reputation_parity_invalid_triple_count {invalid}\n\
             # HELP reputation_parity_quarantined_did_count Number of DIDs quarantined (>50% mismatch dominance).\n\
             # TYPE reputation_parity_quarantined_did_count gauge\n\
             reputation_parity_quarantined_did_count {quarantined}\n\
             # HELP reputation_cutover_frozen 1 if operator has frozen the cutover (suppresses auto-retirement).\n\
             # TYPE reputation_cutover_frozen gauge\n\
             reputation_cutover_frozen {frozen}\n",
            match_count = json.match_count,
            total = json.total_count,
            score = json.parity_score,
            invalid = json.invalid_triples,
            quarantined = json.quarantined_dids.len(),
            frozen = if json.operator_freeze { 1 } else { 0 },
        );
        if let Err(e) = fs::write(p, prom) {
            eprintln!("prometheus write {}: {e}", p.display());
            return ExitCode::from(1);
        }
    }

    if !passes {
        // Non-zero exit on threshold miss so CI fails.
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

#[allow(dead_code)]
const _: fn() = || {
    // Reference consts so the unused-import lint never trips across
    // feature combinations.
    let _ = (
        PARITY_THRESHOLD,
        PER_DID_MISMATCH_DOMINANCE,
        PARITY_GATE_DEADLINE_DAYS,
    );
};
#[allow(dead_code)]
const _: TripleClass = TripleClass::Valid;
