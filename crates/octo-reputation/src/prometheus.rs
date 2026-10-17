//! Prometheus textfile exporter for the reputation registry (mission 0968-b
//! Phase D observability acceptance criteria).
//!
//! Emits the canonical 5 gauges consumed by the cutover Grafana board:
//!
//! - `reputation_parity_match_count` — number of valid (did, kind, layer)
//!   triples that match between legacy and canonical.
//! - `reputation_parity_total_count` — total valid triples observed.
//! - `reputation_invalid_triple_count` — INVALID_TRIPLES (NaN score, etc.)
//!   excluded from the parity denominator.
//! - `reputation_parity_quarantined_did_count` — DIDs quarantined for
//!   > 50 % mismatch dominance in the sweep.
//! - `reputation_cutover_frozen` — 1 if an operator has frozen the cutover
//!   (env `QUOTA_ROUTER_REPUTATION_FREEZE_CUTOVER=1` or `--freeze-cutover`).
//!
//! All metrics are gauges (snapshot state). The textfile format follows the
//! node_exporter `--collector.textfile.directory` convention: one file per
//! push, atomic rename by the upstream collector.
//!
//! ## Wire format example
//!
//! ```text
//! # HELP reputation_parity_match_count ...
//! # TYPE reputation_parity_match_count gauge
//! reputation_parity_match_count 42
//! ...
//! ```

use std::fmt::Write;
use std::path::Path;

use crate::parity::ParityReport;

/// Snapshot of every metric the exporter emits. Pass this to
/// [`render_prometheus`] or [`write_prometheus_file`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricsSnapshot {
    pub match_count: u64,
    pub total_count: u64,
    pub invalid_triple_count: u64,
    pub quarantined_did_count: u64,
    pub operator_freeze: bool,
}

impl MetricsSnapshot {
    /// Build from a `ParityReport` + the live operator-freeze flag.
    pub fn from_report(report: &ParityReport) -> Self {
        Self {
            match_count: report.match_count,
            total_count: report.total_count,
            invalid_triple_count: report.invalid_triples,
            quarantined_did_count: report.quarantined_dids.len() as u64,
            operator_freeze: report.operator_freeze,
        }
    }

    /// Build from a passed/failed threshold check.
    pub fn empty(freeze: bool) -> Self {
        Self {
            match_count: 0,
            total_count: 0,
            invalid_triple_count: 0,
            quarantined_did_count: 0,
            operator_freeze: freeze,
        }
    }
}

/// Render the textfile-format output. The output ends with a trailing
/// newline per Prometheus conventions.
pub fn render_prometheus(m: &MetricsSnapshot) -> String {
    let mut out = String::with_capacity(1024);
    writeln!(
        out,
        "# HELP reputation_parity_match_count Number of (did, kind, layer) triples where legacy and canonical agree.\n\
         # TYPE reputation_parity_match_count gauge\n\
         reputation_parity_match_count {}\n\
         # HELP reputation_parity_total_count Number of valid (did, kind, layer) triples observed.\n\
         # TYPE reputation_parity_total_count gauge\n\
         reputation_parity_total_count {}\n\
         # HELP reputation_invalid_triple_count Number of triples excluded from the parity denominator (NaN score, malformed, etc.).\n\
         # TYPE reputation_invalid_triple_count gauge\n\
         reputation_invalid_triple_count {}\n\
         # HELP reputation_parity_quarantined_did_count Number of DIDs quarantined (>50%% mismatch dominance).\n\
         # TYPE reputation_parity_quarantined_did_count gauge\n\
         reputation_parity_quarantined_did_count {}\n\
         # HELP reputation_cutover_frozen 1 if operator has frozen the cutover (suppresses auto-retirement).\n\
         # TYPE reputation_cutover_frozen gauge\n\
         reputation_cutover_frozen {}\n",
        m.match_count,
        m.total_count,
        m.invalid_triple_count,
        m.quarantined_did_count,
        if m.operator_freeze { 1 } else { 0 },
    )
    .expect("writing to String never fails");
    out
}

/// Atomic write — render then write.
pub fn write_prometheus_file(path: &Path, m: &MetricsSnapshot) -> std::io::Result<()> {
    std::fs::write(path, render_prometheus(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecorderDid;

    #[test]
    fn render_contains_all_five_metrics() {
        let m = MetricsSnapshot {
            match_count: 10,
            total_count: 11,
            invalid_triple_count: 2,
            quarantined_did_count: 1,
            operator_freeze: true,
        };
        let s = render_prometheus(&m);
        for needle in [
            "reputation_parity_match_count 10",
            "reputation_parity_total_count 11",
            "reputation_invalid_triple_count 2",
            "reputation_parity_quarantined_did_count 1",
            "reputation_cutover_frozen 1",
        ] {
            assert!(s.contains(needle), "missing `{needle}` in:\n{s}");
        }
    }

    #[test]
    fn render_freeze_off_emits_zero() {
        let m = MetricsSnapshot::empty(false);
        let s = render_prometheus(&m);
        assert!(s.contains("reputation_cutover_frozen 0"));
    }

    #[test]
    fn from_report_extracts_fields() {
        let report = ParityReport {
            rows: vec![],
            valid_triples: 100,
            invalid_triples: 5,
            match_count: 95,
            total_count: 100,
            parity_score: 0.95,
            quarantined_dids: vec![RecorderDid::from_array([0u8; 52])],
            operator_freeze: false,
        };
        let m = MetricsSnapshot::from_report(&report);
        assert_eq!(m.match_count, 95);
        assert_eq!(m.total_count, 100);
        assert_eq!(m.invalid_triple_count, 5);
        assert_eq!(m.quarantined_did_count, 1);
        assert!(!m.operator_freeze);
    }

    #[test]
    fn write_prometheus_file_roundtrips() {
        let m = MetricsSnapshot::empty(false);
        let dir = std::env::temp_dir();
        let p = dir.join("reputation_metrics_test.prom");
        write_prometheus_file(&p, &m).unwrap();
        let body = std::fs::read_to_string(&p).unwrap();
        assert!(body.contains("reputation_cutover_frozen 0"));
        let _ = std::fs::remove_file(&p);
    }
}
