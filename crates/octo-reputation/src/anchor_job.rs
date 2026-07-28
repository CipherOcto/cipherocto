//! Background anchor submission job — mission 0968a core scheduling.
//!
//! Per RFC-0955-R1 §"Chain-Level Idempotency" + §"Cost Model", and
//! RFC-0968 §28 amendment 48 per-controller Merkle-root batching. The
//! `AnchorJob` plans and submits batches for one attested
//! `controller_id` per `AnchorWindow`. It is pure scheduling — actual
//! on-chain settlement is delegated to a `ChainAnchorSubmitter`
//! trait implementation that the runtime provides (no wallet/CipherOcto
//! chain wiring inside this crate).
//!
//! ## Workflow
//!
//! 1. Caller scans its `ReputationStore` for unanchored aggregates (the
//!    persistence layer keys unanchored events via `anchor_tx_hash IS NULL`
//!    in `reputation_anchors`).
//! 2. Caller groups them by attested `controller_id`.
//! 3. For each controller, caller invokes
//!    `AnchorJob::plan_batches(aggregates, controller_id, now_unix)`.
//! 4. Caller submits each batch via
//!    `ChainAnchorSubmitter::submit(batch, fee)` and records the
//!    returned `anchor_tx_hash` in `reputation_anchors`.
//!
//! ## Constraints enforced
//!
//! - `MAX_TUPLES_PER_ROOT = 100` per batch
//! - `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL = 1` (one batch per
//!   controller per `AnchorWindow::at(now_unix)`)
//! - `MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY = 100` aggregated over
//!   the rolling 24-hour window
//! - `ANCHOR_FEE_PER_ROOT = 5_000` + `MIN_FEE_PER_LEAF * leaves.len()`
//! - `MIN_FINALITY_BLOCKS = 12` after which an anchor is considered
//!   finalised (the chain side uses `is_finality_reached`)
//!
//! The chain submission is delegated; this crate contains no wallet /
//! node / chain adapter code.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::anchor::{
    exceeds_daily_fanout, window_collision, AnchorLeaf, AnchorWindow, ReputationAnchorBatch,
};
use crate::constants::{
    ANCHOR_FEE_PER_ROOT, MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL, MAX_TUPLES_PER_ROOT,
    MIN_FEE_PER_LEAF,
};
use crate::types::ReputationAggregate;

/// Errors the job can surface. Chain-side errors are opaque (the
/// submitter owns the wire format); the job itself only branches on
/// `QuotaExceeded` (cap hit) and `AlreadyAnchored` (idempotency).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorJobError {
    /// A cap was hit. Per-controller / per-window / per-day — all mapped
    /// to the same variant; the cap field identifies which.
    CapExceeded {
        cap: &'static str,
        controller_id: [u8; 32],
    },
    /// Idempotency: a window already had an anchor for this controller.
    AlreadyAnchored {
        controller_id: [u8; 32],
        window: AnchorWindow,
    },
    /// Underlying chain submitter rejected the batch.
    SubmitterRejected(String),
}

/// Outcome of a single `plan_batches` call. Carries both the planned
/// batches AND the spillover count so callers cannot silently lose
/// aggregates past the per-window one-root cap.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanBatchesOutcome {
    /// The planned batches (at most
    /// `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL` entries).
    pub batches: Vec<ReputationAnchorBatch>,
    /// Aggregates the function did NOT plan into a batch because the
    /// per-window one-root cap rejects a second batch in the same
    /// window. Caller MUST re-invoke `plan_batches` for a later
    /// `now_unix` to consume them.
    pub spilled_aggregates: usize,
}

impl PlanBatchesOutcome {
    /// True iff no aggregates were spilled (i.e., everything fit in
    /// the returned batches).
    #[must_use]
    pub fn no_spill(&self) -> bool {
        self.spilled_aggregates == 0
    }
}

/// Outcome of a single `AnchorJob::run_once` execution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnchorJobOutcome {
    /// Number of batches successfully submitted.
    pub submitted: usize,
    /// Per-batch anchor transaction hashes (in submit order).
    pub anchor_tx_hashes: Vec<[u8; 32]>,
    /// Aggregated fee ACTUALLY paid across the batches whose submit
    /// returned `Ok`. Zero when `submitted == 0`. NOT the quoted fee.
    pub fee_paid: u128,
}

/// Top-level outcome of `run_once` — batches submitted when the
/// per-controller / per-day fanout cap (`MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY
/// = 100`) would be exceeded. Mirrors
/// `ReputationError::AnchorTupleFanoutExceeded(u64) = 0x2A` so the
/// canonical error variant carries the count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorTupleFanout {
    /// `existing_anchored_today + proposed_count`.
    pub count: u64,
    /// `MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY`.
    pub max: u64,
}

impl AnchorTupleFanout {
    /// Convert to the canonical `ReputationError` variant.
    pub fn to_reputation_error(&self) -> crate::error::ReputationError {
        crate::error::ReputationError::AnchorTupleFanoutExceeded(self.count)
    }
}

/// Trait the runtime implements to bridge to the on-chain settlement
/// layer. The `submit` call returns the on-chain anchor tx hash.
pub trait ChainAnchorSubmitter: Send + Sync {
    /// Submit one batch on chain. The runtime must have already
    /// verified the per-window / per-leaf / per-fee caps (the job
    /// guarantees this).
    fn submit(&self, batch: &ReputationAnchorBatch, fee: u128) -> Result<[u8; 32], AnchorJobError>;
}

/// Default chain-submission contract: a stub implementation that
/// returns a deterministic placeholder. Useful for tests + first-boot
/// before the chain adapter is wired.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubChainAnchorSubmitter;

impl ChainAnchorSubmitter for StubChainAnchorSubmitter {
    fn submit(
        &self,
        batch: &ReputationAnchorBatch,
        _fee: u128,
    ) -> Result<[u8; 32], AnchorJobError> {
        // Deterministic test placeholder: hash the batch digest so two
        // distinct batches yield distinct "tx hashes" without claiming
        // a real on-chain identity.
        let mut out = [0u8; 32];
        out.copy_from_slice(batch.digest().as_bytes());
        Ok(out)
    }
}

/// Per-execution window state. The job opens one window per call and
/// runs at most one batch per controller through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorJobConfig {
    pub now_unix: u64,
}

/// Plan one or more batches for the input aggregates. Caller is
/// expected to have already partitioned by `controller_id`. This
/// function enforces the per-batch leaf cap and the daily fanout
/// cap. Returns the planned batches AND a `spilled_aggregates`
/// count so the caller can re-invoke with a later `now_unix`
/// instead of silently losing data.
///
/// Returns `Err` only on fatal config errors (per-day cap which
/// surfaces as a hard rejection, OR a window collision).
pub fn plan_batches(
    aggregates: &[ReputationAggregate],
    controller_id: [u8; 32],
    config: AnchorJobConfig,
    existing_anchored_today: u64,
    existing_window_anchor: Option<AnchorWindow>,
) -> Result<PlanBatchesOutcome, AnchorJobError> {
    // Cap 1: per-day fanout across the aggregated window.
    if exceeds_daily_fanout(existing_anchored_today, aggregates.len()) {
        return Err(AnchorJobError::CapExceeded {
            cap: "daily_fanout",
            controller_id,
        });
    }

    // Cap 2: per-window one-root constraint. If a root was already
    // submitted in this window by this controller, no further root
    // can land in the same window.
    let proposed_window = AnchorWindow::at(config.now_unix);
    if let Some(existing) = existing_window_anchor {
        if window_collision(existing, proposed_window) {
            return Err(AnchorJobError::AlreadyAnchored {
                controller_id,
                window: proposed_window,
            });
        }
    }

    if aggregates.is_empty() {
        return Ok(PlanBatchesOutcome::default());
    }

    // Chunks of at most MAX_TUPLES_PER_ROOT leaves per batch.
    let chunks: Vec<Vec<AnchorLeaf>> = aggregates
        .chunks(MAX_TUPLES_PER_ROOT as usize)
        .map(|c| c.iter().map(AnchorLeaf::from_aggregate).collect())
        .collect();

    // Cap 3: at most one batch per controller per anchor window. Per
    // MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL=1 we restrict to
    // exactly one root per window. Any extra chunks become
    // `spilled_aggregates` so the caller re-invokes with a later
    // `now_unix`.
    let max_batches = MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL as usize;
    let (kept_chunks, spilled_chunks) = chunks.split_at(chunks.len().min(max_batches));
    let batches: Vec<ReputationAnchorBatch> = kept_chunks
        .iter()
        .map(|leaves| {
            let batch = ReputationAnchorBatch {
                controller_id,
                window: proposed_window,
                chain_block_height: 0,
                rotation_receipt_id: None,
                leaves: leaves.clone(),
            };
            assert!(
                batch.within_leaf_cap(),
                "internal: batch exceeds MAX_TUPLES_PER_ROOT"
            );
            batch
        })
        .collect();
    let spilled_aggregates: usize = spilled_chunks.iter().map(|c| c.len()).sum();

    // Batches.len() may be 0 here when chunks.len() == 0 (already
    // returned the default above), but defensively clamp:
    // `kept_chunks` is bounded by `max_batches` so `batches.len() <= max_batches`
    // is an invariant, not a fallback. No truncate required.

    Ok(PlanBatchesOutcome {
        batches,
        spilled_aggregates,
    })
}

/// Compute the OCTO fee for a planned batch list.
pub fn total_fee(batches: &[ReputationAnchorBatch]) -> u128 {
    batches
        .iter()
        .map(|b| {
            (ANCHOR_FEE_PER_ROOT as u128) + (MIN_FEE_PER_LEAF as u128) * (b.leaves.len() as u128)
        })
        .sum()
}

/// Run the job: plan + submit each batch through `submitter`. Returns
/// the per-batch outcomes; submits are sequential (idempotency is
/// preserved by the per-window cap check).
///
/// `fee_paid` in the outcome is incremented ONLY for batches whose
/// `submit` returned `Ok`, so a mid-loop failure leaves the field
/// accurately reflecting actually-paid fees (not the quoted fee).
pub async fn run_once<S: ChainAnchorSubmitter>(
    submitter: Arc<S>,
    aggregates: &[ReputationAggregate],
    controller_id: [u8; 32],
    config: AnchorJobConfig,
    existing_anchored_today: u64,
    existing_window_anchor: Option<AnchorWindow>,
) -> Result<AnchorJobOutcome, AnchorJobError> {
    let plan = plan_batches(
        aggregates,
        controller_id,
        config,
        existing_anchored_today,
        existing_window_anchor,
    )?;
    if plan.batches.is_empty() {
        return Ok(AnchorJobOutcome::default());
    }
    let mut outcome = AnchorJobOutcome::default();
    for batch in &plan.batches {
        // Per-batch fee: ANCHOR_FEE_PER_ROOT + MIN_FEE_PER_LEAF * leaves.len().
        // We charge the caller per-batch (matches chain settlement's
        // per-tx fee model); summing into `fee_paid` only on success
        // guarantees the post-condition `fee_paid == sum(per-batch on Ok)`.
        let per_batch_fee = batch.fee();
        let tx_hash = submitter.submit(batch, per_batch_fee)?;
        outcome.fee_paid = outcome
            .fee_paid
            .checked_add(per_batch_fee)
            .expect("anchor fee overflow: per-controller daily fee must fit in u128");
        outcome.submitted += 1;
        outcome.anchor_tx_hashes.push(tx_hash);
    }
    Ok(outcome)
}

/// Pre-flight check for the per-controller daily fanout cap.
///
/// Returns `Some(AnchorTupleFanout)` when
/// `existing_anchored_today + aggregates.len() >
/// MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY`. Callers convert to
/// `ReputationError::AnchorTupleFanoutExceeded(u64)` via
/// `to_reputation_error()` for the canonical error variant.
pub fn check_daily_fanout(
    existing_anchored_today: u64,
    proposed_count: usize,
) -> Option<AnchorTupleFanout> {
    use crate::constants::MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY;
    let total = existing_anchored_today.saturating_add(proposed_count as u64);
    if total > MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY {
        Some(AnchorTupleFanout {
            count: total,
            max: MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY,
        })
    } else {
        None
    }
}

/// Run-once with the canonical `AnchorTupleFanoutExceeded` error
/// variant. Wraps `run_once` with a pre-flight fanout check that
/// surfaces `ReputationError::AnchorTupleFanoutExceeded(u64) = 0x2A`
/// instead of `AnchorJobError::CapExceeded`.
pub async fn run_once_strict<S: ChainAnchorSubmitter>(
    submitter: Arc<S>,
    aggregates: &[ReputationAggregate],
    controller_id: [u8; 32],
    config: AnchorJobConfig,
    existing_anchored_today: u64,
    existing_window_anchor: Option<AnchorWindow>,
) -> Result<AnchorJobOutcome, crate::error::ReputationError> {
    if let Some(fanout) = check_daily_fanout(existing_anchored_today, aggregates.len()) {
        return Err(fanout.to_reputation_error());
    }
    run_once(
        submitter,
        aggregates,
        controller_id,
        config,
        existing_anchored_today,
        existing_window_anchor,
    )
    .await
    .map_err(|_| crate::error::ReputationError::AnchorTupleFanoutExceeded(existing_anchored_today))
}

/// Convenience: compute the per-batch fee for a single batch.
pub fn fee_for_batch(batch: &ReputationAnchorBatch) -> u128 {
    batch.fee()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RecorderDid, ReputationLayer, SignalKind};

    fn agg(score: f64, samples: u64, did_byte: u8) -> ReputationAggregate {
        use octo_determin::Dfp;
        ReputationAggregate {
            recorder_did: RecorderDid::from_array([did_byte; 52]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_ewma: Dfp::from_f64(score),
            samples,
            severity_total: 0,
            last_event_id: crate::types::EventId::from_u64(1),
            last_event_unix: 1_000,
            updated_at_unix: 1_000,
        }
    }

    #[test]
    fn empty_aggregates_produces_no_batches() {
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let plan = plan_batches(&[], [1u8; 32], cfg, 0, None).unwrap();
        assert!(plan.batches.is_empty());
        assert_eq!(plan.spilled_aggregates, 0);
        assert!(plan.no_spill());
    }

    #[test]
    fn daily_cap_exceeded_rejects_all() {
        let aggs = vec![agg(0.5, 100, 1), agg(0.5, 100, 2)];
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let err = plan_batches(&aggs, [1u8; 32], cfg, 99, None).unwrap_err();
        // 99 existing + 2 proposed = 101 > 100 → reject
        assert_eq!(
            err,
            AnchorJobError::CapExceeded {
                cap: "daily_fanout",
                controller_id: [1u8; 32],
            }
        );
    }

    #[test]
    fn window_collision_rejects_reanchor() {
        let aggs = vec![agg(0.5, 100, 1)];
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let window = AnchorWindow::at(cfg.now_unix);
        let err = plan_batches(&aggs, [1u8; 32], cfg, 0, Some(window)).unwrap_err();
        assert_eq!(
            err,
            AnchorJobError::AlreadyAnchored {
                controller_id: [1u8; 32],
                window,
            }
        );
    }

    #[test]
    fn chunks_at_max_leaf_cap() {
        // 80 aggregates — within the daily cap (80 ≤ 100), but large
        // enough to verify the chunking path takes effect if a future
        // capacity bump raises MAX_TUPLES_PER_ROOT. The per-window
        // cap trims to 1 batch; the rest spill to the next window
        // (caller responsibility).
        let aggs: Vec<_> = (0..80u8).map(|i| agg(0.5, 100, i)).collect();
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let plan = plan_batches(&aggs, [1u8; 32], cfg, 0, None).unwrap();
        // per-window cap trims to 1 root
        assert_eq!(plan.batches.len(), 1);
        // The trimmed batch must respect the leaf cap.
        assert!(plan.batches[0].within_leaf_cap());
        assert!(plan.batches[0].leaves.len() as u64 <= MAX_TUPLES_PER_ROOT);
        assert_eq!(plan.batches[0].leaves.len(), 80);
    }

    #[test]
    fn at_cap_boundary_no_spillover() {
        // 100 aggregates — at the daily cap (100 ≤ 100), exactly fills
        // one batch. The per-window one-root cap holds it; no spillover.
        // Spillover can only trigger if MAX_TUPLES_PER_ROOT were
        // raised above 100 without a corresponding daily-cap bump.
        let aggs: Vec<_> = (0..100u8).map(|i| agg(0.5, 100, i)).collect();
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let plan = plan_batches(&aggs, [1u8; 32], cfg, 0, None).unwrap();
        assert_eq!(plan.batches.len(), 1);
        assert_eq!(plan.spilled_aggregates, 0);
        assert!(plan.no_spill());
    }

    #[test]
    fn saturating_existing_count_rejected_by_daily_cap() {
        // existing = u64::MAX still trips the daily cap (saturating_add
        // path goes 0 + proposed).
        let aggs = vec![agg(0.5, 100, 1)];
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let err = plan_batches(&aggs, [1u8; 32], cfg, u64::MAX, None).unwrap_err();
        assert_eq!(
            err,
            AnchorJobError::CapExceeded {
                cap: "daily_fanout",
                controller_id: [1u8; 32],
            }
        );
    }

    #[test]
    fn total_fee_is_root_plus_per_leaf() {
        let aggs: Vec<_> = (0..10u8).map(|i| agg(0.5, 100, i)).collect();
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let plan = plan_batches(&aggs, [1u8; 32], cfg, 0, None).unwrap();
        let fee = total_fee(&plan.batches);
        // 1 batch, 10 leaves: 5_000 + 10 * 50 = 5_500
        assert_eq!(
            fee,
            (ANCHOR_FEE_PER_ROOT as u128) + (MIN_FEE_PER_LEAF as u128) * 10
        );
    }

    #[tokio::test]
    async fn run_once_submits_via_stub_with_expected_fee() {
        let aggs: Vec<_> = (0..3u8).map(|i| agg(0.5, 100, i)).collect();
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let outcome = run_once(
            Arc::new(StubChainAnchorSubmitter),
            &aggs,
            [1u8; 32],
            cfg,
            0,
            None,
        )
        .await
        .unwrap();
        assert_eq!(outcome.submitted, 1);
        assert_eq!(outcome.anchor_tx_hashes.len(), 1);
        // 5_000 + 3 * 50 = 5_150
        assert_eq!(
            outcome.fee_paid,
            (ANCHOR_FEE_PER_ROOT as u128) + (MIN_FEE_PER_LEAF as u128) * 3
        );
    }

    #[tokio::test]
    async fn empty_aggregates_short_circuits_to_default_outcome() {
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        let outcome = run_once(
            Arc::new(StubChainAnchorSubmitter),
            &[],
            [1u8; 32],
            cfg,
            0,
            None,
        )
        .await
        .unwrap();
        assert_eq!(outcome, AnchorJobOutcome::default());
    }

    #[test]
    fn check_daily_fanout_returns_some_at_cap() {
        let f = check_daily_fanout(50, 51).unwrap();
        assert_eq!(f.count, 101);
        assert_eq!(
            f.max,
            crate::constants::MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY
        );
        // 0x2A = 42 discriminant
        let err = f.to_reputation_error();
        assert_eq!(err.discriminant(), 0x2A);
        assert_eq!(
            err,
            crate::error::ReputationError::AnchorTupleFanoutExceeded(101)
        );
    }

    #[test]
    fn check_daily_fanout_returns_none_below_cap() {
        assert!(check_daily_fanout(50, 50).is_none());
        assert!(check_daily_fanout(0, 100).is_none());
        assert!(check_daily_fanout(0, 0).is_none());
    }

    #[tokio::test]
    async fn run_once_strict_emits_anchor_tuple_fanout_exceeded() {
        // Pre-flight emits ReputationError::AnchorTupleFanoutExceeded
        // (0x2A) when existing + proposed > MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY.
        let aggs: Vec<_> = (0..5u8).map(|i| agg(0.5, 100, i)).collect();
        let cfg = AnchorJobConfig { now_unix: 1_000 };
        // 96 existing + 5 proposed = 101 > 100.
        let err = run_once_strict(
            Arc::new(StubChainAnchorSubmitter),
            &aggs,
            [1u8; 32],
            cfg,
            96,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            err,
            crate::error::ReputationError::AnchorTupleFanoutExceeded(101)
        );
        assert_eq!(err.discriminant(), 0x2A);
    }
}
