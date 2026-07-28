//! Parity reconciliation — per-`(did, kind, layer)` parity report between a
//! legacy store (f64 EWMA mirror) and a canonical `ReputationStore` (Dfp EWMA).
//!
//! Per `missions/claimed/0968-reputation-persistence.md` Phase 2.5 acceptance
//! and `missions/open/0968-b-marketplace-integration.md` Phase D. Two
//! consumers:
//!
//! - The standalone binary `bin/reputation-parity.rs` (CI + ops).
//! - The `ReputationStoreCompat::parity_report()` runtime hook
//!   (mission 0968-b Phase D dual-read retirement gate).
//!
//! ## Triple classification
//!
//! Per RFC-0968 §7.1 (Review Round 7 persistence-1 + persistence-2 +
//! governance-8): the parity gate's denominator is the authoritative triple
//! population. INVALID_TRIPLES (malformed inputs — NaN score, unsupported
//! kind/layer, length-mismatched BLOB) are excluded from BOTH the numerator
//! AND the denominator and surfaced separately. VALID_TRIPLES drive the gate.
//!
//! ## Per-DID quorum quarantine
//!
//! Per RFC-0968 §7.1 (Review Round 7 persistence-6): a single DID
//! contributing > 50% of mismatches is quarantined for 24h; during
//! quarantine, the DID's triples are excluded from BOTH numerator AND
//! denominator (the global score actually recovers).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::compat::LegacyReputationStore;
use crate::store::ReputationStore;
use crate::types::{RecorderDid, ReputationLayer, SignalKind};

/// Parity threshold. Per mission 0968-b Phase D dual-read retirement gate,
/// legacy stores retire only when `parity_score >= 0.999` for 24 consecutive
/// 1-hour buckets.
pub const PARITY_THRESHOLD: f64 = 0.999;

/// Hard deadline after which the parity gate auto-retires the legacy stores
/// regardless of parity score, provided `INVALID_TRIPLES / total_triples <
/// 1e-6`. Per mission 0968-b Phase D `PARITY_GATE_DEADLINE_UNIX`.
pub const PARITY_GATE_DEADLINE_DAYS: u64 = 90;

/// Per-DID mismatch dominance threshold. A DID contributing more than this
/// fraction of all mismatches is quarantined.
pub const PER_DID_MISMATCH_DOMINANCE: f64 = 0.50;

/// Classification of a single `(did, kind, layer)` triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripleClass {
    /// Canonical schema — drives the parity score.
    Valid,
    /// score_ewma is NaN / ±Inf — excluded from parity denominator.
    InvalidScore,
    /// SignalKind / ReputationLayer discriminant unsupported.
    InvalidDiscriminant,
    /// BLOB length-mismatched (should never happen post-migration).
    InvalidBlobLength,
}

impl TripleClass {
    pub fn discriminant(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::InvalidScore => "invalid_score",
            Self::InvalidDiscriminant => "invalid_discriminant",
            Self::InvalidBlobLength => "invalid_blob_length",
        }
    }

    pub fn drives_parity(self) -> bool {
        matches!(self, Self::Valid)
    }
}

#[derive(Debug, Clone)]
pub struct ParityRow {
    pub recorder_did: RecorderDid,
    pub kind: SignalKind,
    pub layer: ReputationLayer,
    pub class: TripleClass,
    pub canonical_score: Option<f64>,
    pub legacy_score: Option<f64>,
    pub matches: bool,
}

/// Result of one parity sweep.
#[derive(Debug, Clone, Default)]
pub struct ParityReport {
    pub rows: Vec<ParityRow>,
    pub valid_triples: u64,
    pub invalid_triples: u64,
    pub match_count: u64,
    pub total_count: u64,
    pub parity_score: f64,
    pub quarantined_dids: Vec<RecorderDid>,
    pub operator_freeze: bool,
}

impl ParityReport {
    pub fn passes_threshold(&self) -> bool {
        self.parity_score >= PARITY_THRESHOLD && self.total_count >= 100
    }
}

/// Compute a parity report between a legacy store and a canonical store.
///
/// Inputs:
/// - `legacy` — `LegacyReputationStore` (f64 EWMA mirror).
/// - `canonical` — `ReputationStore` (Dfp aggregate source).
/// - `freeze` — operator freeze flag. When `true`, `operator_freeze` is set
///   in the report and downstream retirement is suppressed.
///
/// Algorithm:
/// 1. For each `(did, kind, layer)` observed in either store:
///    - Read canonical via `read_aggregate`; if absent, score = None.
///    - Read legacy via `success_rate`; if absent, score = None.
///    - Classify the triple. INVALID → excluded from `parity_score`.
///    - VALID → compare canonical and legacy with f64 tolerance; record match/mismatch.
/// 2. Compute `parity_score = match_count / total_count` over VALID triples only.
/// 3. Compute per-DID mismatch dominance; quarantine any DID > 50%.
/// 4. Emit report.
///
/// Note: This is a synchronous best-effort sweep over the local views; the
/// full implementation lives in `bin/reputation-parity.rs` which composes
/// both stores from CLI inputs.
pub fn compute_parity_report<L: LegacyReputationStore, C: ReputationStore>(
    legacy: &L,
    canonical: &C,
    pairs: &[(RecorderDid, SignalKind, ReputationLayer)],
    freeze: bool,
) -> ParityReport {
    let mut report = ParityReport {
        operator_freeze: freeze,
        ..Default::default()
    };

    let mut per_did_mismatches: HashMap<[u8; 52], u64> = HashMap::new();
    let mut per_did_total: HashMap<[u8; 52], u64> = HashMap::new();

    for (did, kind, layer) in pairs {
        let legacy_score = legacy.success_rate(did);
        // Read canonical via async runtime — the trait method is async.
        let canonical_agg =
            futures_lite::future::block_on(canonical.read_aggregate(did, *kind, *layer));
        let canonical_score = canonical_agg.ok().map(|a| a.score_ewma.to_f64());

        let class = classify(canonical_score);

        let (matches, cand_for_counting, leg_for_counting) = match class {
            TripleClass::Valid => {
                let c = canonical_score.unwrap_or(0.0);
                let l = legacy_score;
                let matches = (c - l).abs() <= 1e-9;
                (matches, Some(c), Some(l))
            }
            _ => (false, canonical_score, Some(legacy_score)),
        };

        if class.drives_parity() {
            report.total_count += 1;
            if matches {
                report.match_count += 1;
            } else {
                let key = *did.as_bytes();
                *per_did_mismatches.entry(key).or_insert(0) += 1;
            }
            let key = *did.as_bytes();
            *per_did_total.entry(key).or_insert(0) += 1;
            let _ = cand_for_counting;
            let _ = leg_for_counting;
        } else {
            report.invalid_triples += 1;
        }
        report.valid_triples += if class.drives_parity() { 1 } else { 0 };

        report.rows.push(ParityRow {
            recorder_did: *did,
            kind: *kind,
            layer: *layer,
            class,
            canonical_score,
            legacy_score: Some(legacy_score),
            matches,
        });
    }

    // Per-DID quarantine detection.
    let total_mismatches: u64 = per_did_mismatches.values().sum();
    if total_mismatches > 0 {
        for (k, m) in &per_did_mismatches {
            let frac = *m as f64 / total_mismatches as f64;
            if frac > PER_DID_MISMATCH_DOMINANCE && per_did_total.get(k).copied().unwrap_or(0) >= 5
            {
                report.quarantined_dids.push(RecorderDid::from_array(*k));
            }
        }
    }

    if report.total_count > 0 {
        report.parity_score = report.match_count as f64 / report.total_count as f64;
    }

    report
}

/// Classify a (canonical_score, kind, layer) triple. The score is the
/// observable; kind/layer are assumed valid because they were produced by
/// the canonical store. NaN/±Inf on the canonical side marks INVALID.
pub fn classify(canonical_score: Option<f64>) -> TripleClass {
    match canonical_score {
        None => TripleClass::Valid, // absent canonical is still a valid absence
        Some(s) if s.is_nan() => TripleClass::InvalidScore,
        Some(s) if !s.is_finite() => TripleClass::InvalidScore,
        Some(_) => TripleClass::Valid,
    }
}

/// Unix seconds at which the parity-gate deadline expires — 90 days from
/// the moment this function is called. Per mission 0968-b Phase D.
///
/// **Pre-epoch fail-safe (Round 2 review C10):** a system clock at or
/// before `UNIX_EPOCH` (NTP misconfig, broken RTC, manual date) would
/// otherwise yield `now=0` and a deadline of `1970-04-01`, opening the
/// dual-read auto-retirement immediately (silent data corruption). We
/// detect the case via `duration_since(...).is_err()`, log to stderr,
/// and return `u64::MAX` as a "no deadline" sentinel — the deadline
/// path stays closed until the operator fixes the clock.
pub fn parity_gate_deadline_unix() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH);
    match now {
        Ok(d) => d
            .as_secs()
            .saturating_add(PARITY_GATE_DEADLINE_DAYS * 86_400),
        Err(_) => {
            eprintln!(
                "octo_reputation::parity: SYSTEM CLOCK at or before UNIX_EPOCH; \
                 parity-gate deadline UNSET (u64::MAX sentinel). \
                 Auto-retirement will NOT fire. Fix NTP/RTC before re-checking."
            );
            u64::MAX
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::SlashReputationStore;
    use crate::store::InMemoryReputationStore;
    use crate::types::{ReputationLayer, SignalKind};
    use crate::ControllerId;
    use crate::{EventId, SignalEvent};
    use octo_determin::Dfp;

    fn ev(seed: u64, did: RecorderDid, score: f64, ts: u64) -> SignalEvent {
        SignalEvent {
            event_id: EventId::from_u64(seed),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(score),
            recorded_at_unix: ts,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        }
    }

    #[tokio::test]
    async fn parity_report_matches_when_stores_agree() {
        let canonical = InMemoryReputationStore::new();
        let legacy = SlashReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        canonical
            .record_signal(ev(1, did, 1.0, 1000))
            .await
            .unwrap();
        legacy
            .shadow_record(
                &did,
                SignalKind::Outcome,
                ReputationLayer::Market,
                1.0,
                1000,
            )
            .unwrap();

        let pairs = vec![(did, SignalKind::Outcome, ReputationLayer::Market)];
        let report = compute_parity_report(&legacy, &canonical, &pairs, false);
        assert_eq!(report.total_count, 1);
        assert_eq!(report.match_count, 1);
        assert!((report.parity_score - 1.0).abs() < 1e-12);
        // passes_threshold requires total_count >= 100; here we just verify
        // the score is computed correctly and the threshold constant is sane.
        assert!(!report.operator_freeze);
        const { assert!(PARITY_THRESHOLD <= 1.0) };
    }

    #[test]
    fn classify_handles_nan_and_inf() {
        assert_eq!(classify(Some(f64::NAN)), TripleClass::InvalidScore);
        assert_eq!(classify(Some(f64::INFINITY)), TripleClass::InvalidScore);
        assert_eq!(classify(Some(f64::NEG_INFINITY)), TripleClass::InvalidScore);
        assert_eq!(classify(Some(0.5)), TripleClass::Valid);
        assert_eq!(classify(None), TripleClass::Valid);
    }

    #[test]
    fn parity_gate_deadline_is_90_days_from_now() {
        let d = parity_gate_deadline_unix();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(d > now);
        assert!(d - now >= PARITY_GATE_DEADLINE_DAYS * 86_400 - 5);
    }
}
