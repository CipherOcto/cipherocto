//! On-chain reputation anchoring — RFC-0955-R1 binding contract + mission 0968a.
//!
//! This module owns the in-memory types and digest helpers for the
//! post-amendment-48 per-controller Merkle-root anchoring model. The
//! stoolap migration slot `v010__reputation_anchors.sql` is allocated
//! but the actual chain submission lives in a separate runtime;
//! this module is pure data + digest so it is testable without chain
//! infrastructure.
//!
//! ## Envelope wire form (RFC-0955-R1 §"ReputationAnchorBatch")
//!
//! The on-chain envelope carries the canonical 24-byte Dfp encoding for
//! `score_ewma` plus the tuple identity `(did, signal_kind, layer,
//! last_event_id, last_event_unix, samples, severity_total)`. The
//! envelope-domain-separated BLAKE3 digest (32 bytes) is the
//! `ReputationDigest` accepted at the chain boundary.
//!
//! ## Per-controller cap (RFC-0955-R1 §"Constants")
//!
//! Each attested `controller_id` is limited to one Merkle root per
//! `DEFAULT_ANCHOR_INTERVAL_SECS = 300` window
//! (`MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL = 1`), with at most
//! `MAX_TUPLES_PER_ROOT = 100` leaves. The rolled-up daily fanout cap
//! is `MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY = 100`.

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::constants::{
    ANCHOR_FEE_PER_ROOT, BLAKE3_REPUTATION_ANCHOR_DOMAIN, DEFAULT_ANCHOR_INTERVAL_SECS,
    MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY, MAX_TUPLES_PER_ROOT, MIN_FEE_PER_LEAF,
    MIN_FINALITY_BLOCKS,
};
use crate::digest::ReputationDigest;
use crate::types::{EventId, RecorderDid, ReputationAggregate, ReputationLayer, SignalKind};

/// Per-controller anchor submission window. The anchor job opens a
/// window of `DEFAULT_ANCHOR_INTERVAL_SECS` seconds; in that window
/// each attested `controller_id` may submit exactly one root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnchorWindow {
    /// `now_unix / DEFAULT_ANCHOR_INTERVAL_SECS` — integer division
    /// defines the window index.
    pub window_index: u64,
}

impl AnchorWindow {
    /// Compute the anchor window index for `now_unix`.
    #[must_use]
    pub fn at(now_unix: u64) -> Self {
        Self {
            window_index: now_unix / DEFAULT_ANCHOR_INTERVAL_SECS,
        }
    }
}

/// One leaf inside the per-controller Merkle root. Carries the canonical
/// 24-byte Dfp encoding for `score_ewma` plus the tuple identity that
/// identifies the leaf under the BLAKE3 anchor domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorLeaf {
    /// DID of the subject the leaf pertains to.
    pub did: RecorderDid,
    /// Signal kind of the source aggregate.
    pub signal_kind: SignalKind,
    /// Reputation layer of the source aggregate.
    pub layer: ReputationLayer,
    /// Last event id that produced the source aggregate.
    pub last_event_id: EventId,
    /// Last event unix timestamp.
    pub last_event_unix: u64,
    /// Number of samples feeding the EWMA.
    pub samples: u64,
    /// Total `severity_total` of the source aggregate.
    pub severity_total: u64,
    /// Canonical 24-byte Dfp wire form of the EWMA score.
    pub score_ewma_raw: [u8; 24],
}

impl AnchorLeaf {
    /// Build an anchor leaf from the persisted aggregate.
    pub fn from_aggregate(agg: &ReputationAggregate) -> Self {
        Self {
            did: agg.recorder_did,
            signal_kind: agg.signal_kind,
            layer: agg.layer,
            last_event_id: agg.last_event_id,
            last_event_unix: agg.last_event_unix,
            samples: agg.samples,
            severity_total: agg.severity_total,
            score_ewma_raw: crate::types::dfp_to_blob(&agg.score_ewma),
        }
    }

    /// Compute the leaf digest under the anchor domain. Distinct leaves
    /// with the same byte content (one event) produce the same digest;
    /// two leaves differing on any field produce different digests.
    pub fn digest(&self) -> ReputationDigest {
        let mut hasher = Hasher::new();
        hasher.update(BLAKE3_REPUTATION_ANCHOR_DOMAIN);
        // Canonical serialisation: tuple identity, then 24-byte Dfp blob,
        // then the aggregate counters.
        hasher.update(self.did.as_bytes());
        hasher.update(&[self.signal_kind.discriminant()]);
        hasher.update(&[self.layer.discriminant()]);
        hasher.update(self.last_event_id.as_bytes());
        hasher.update(&self.last_event_unix.to_be_bytes());
        hasher.update(&self.samples.to_be_bytes());
        hasher.update(&self.severity_total.to_be_bytes());
        hasher.update(&self.score_ewma_raw);
        let out = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(out.as_bytes());
        ReputationDigest(arr)
    }
}

/// A batch of anchor leaves submitted under one Merkle root by one
/// attested `controller_id` in one anchor window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReputationAnchorBatch {
    /// Attested `controller_id` (default `blake3(governance_pubkey)` per
    /// RFC-0968 §28 amendment 40 / 44).
    pub controller_id: [u8; 32],
    /// Anchor window this batch was opened in.
    pub window: AnchorWindow,
    /// Chain block height at submission time.
    pub chain_block_height: u64,
    /// Rotation-receipt binding (RFC-0955-R1 §"ReputationAnchorBatch"
    /// post-Round-7 amendment 51; persistence-10 AC).
    /// `Some(rotation_receipt_id)` when a post-rotation resubmission
    /// must bind to a specific finalized `consume_rotation_receipt`.
    /// `None` for the standard pre-rotation anchor path.
    pub rotation_receipt_id: Option<[u8; 32]>,
    /// Leaves (capped at `MAX_TUPLES_PER_ROOT = 100`).
    pub leaves: Vec<AnchorLeaf>,
}

impl ReputationAnchorBatch {
    /// Compute the batch digest (the on-chain commitment).
    pub fn digest(&self) -> ReputationDigest {
        let mut hasher = Hasher::new();
        hasher.update(BLAKE3_REPUTATION_ANCHOR_DOMAIN);
        hasher.update(&self.controller_id);
        hasher.update(&self.window.window_index.to_be_bytes());
        hasher.update(&self.chain_block_height.to_be_bytes());
        // Domain-separate the rotation-receipt binding presence, then
        // (when present) the 32-byte receipt id. Encoding the absence
        // as a single `0` byte keeps the digest non-ambiguous across
        // the presence/absence boundary.
        match self.rotation_receipt_id {
            None => {
                hasher.update(&[0u8]);
            }
            Some(id) => {
                hasher.update(&[1u8]);
                hasher.update(&id);
            }
        }
        for leaf in &self.leaves {
            hasher.update(leaf.digest().as_bytes());
        }
        let out = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(out.as_bytes());
        ReputationDigest(arr)
    }

    /// Per-batch fee: `ANCHOR_FEE_PER_ROOT + MIN_FEE_PER_LEAF * leaves.len()`
    /// when below the per-leaf floor; the chain settlement enforces the
    /// upper bound.
    pub fn fee(&self) -> u128 {
        (ANCHOR_FEE_PER_ROOT as u128) + (MIN_FEE_PER_LEAF as u128) * (self.leaves.len() as u128)
    }

    /// True iff the batch respects the per-root leaf cap.
    pub fn within_leaf_cap(&self) -> bool {
        (self.leaves.len() as u64) <= MAX_TUPLES_PER_ROOT
    }
}

/// Verifies whether `proposed` exceeds any per-controller cap given the
/// `existing_daily_count` of anchored leaves for the same
/// attested `controller_id` in the current rolling 24-hour window.
#[must_use]
pub fn exceeds_daily_fanout(existing_daily_count: u64, proposed: usize) -> bool {
    let new_count = existing_daily_count.saturating_add(proposed as u64);
    new_count > MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY
}

/// True iff `proposed_window` collides with an existing
/// `existing_window` for the same `controller_id`.
///
/// Per `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL = 1`: a controller
/// may not submit two roots in the same window. Across windows (e.g.,
/// 0 and 1) submissions are independent.
#[must_use]
pub fn window_collision(existing_window: AnchorWindow, proposed_window: AnchorWindow) -> bool {
    existing_window == proposed_window
}

/// Compute the chain-side finality gate: returns `true` iff the chain
/// block height `finalized_at_height` is at least `MIN_FINALITY_BLOCKS`
/// below the anchor's `submitted_at_height`.
#[must_use]
pub fn is_finality_reached(submitted_at_height: u64, finalized_at_height: u64) -> bool {
    submitted_at_height.saturating_sub(finalized_at_height) >= MIN_FINALITY_BLOCKS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RecorderDid;

    fn dummy_leaf(score: f64, samples: u64, did_byte: u8) -> AnchorLeaf {
        // Build a leaf without going through an aggregate. The digest
        // contract only depends on the canonical fields.
        AnchorLeaf {
            did: RecorderDid::from_array([did_byte; 52]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            last_event_id: EventId::from_u64(1),
            last_event_unix: 1_000,
            samples,
            severity_total: 0,
            score_ewma_raw: crate::types::dfp_to_blob(&Dfp::from_f64(score)),
        }
    }
    use octo_determin::Dfp;

    // ----- Constants are pinned -----

    #[test]
    fn anchor_constants_are_pinned() {
        const { assert!(MIN_FINALITY_BLOCKS == 12) };
        const { assert!(DEFAULT_ANCHOR_INTERVAL_SECS == 300) };
        const { assert!(MAX_TUPLES_PER_ROOT == 100) };
        const { assert!(MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY == 100) };
        const { assert!(ANCHOR_FEE_PER_ROOT == 5_000) };
        const { assert!(MIN_FEE_PER_LEAF == 50) };
    }

    // ----- Leaf digest -----

    #[test]
    fn leaf_digest_is_deterministic() {
        let a = dummy_leaf(0.5, 100, 1);
        let b = dummy_leaf(0.5, 100, 1);
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn leaf_digest_distinct_on_score_change() {
        let a = dummy_leaf(0.5, 100, 1);
        let b = dummy_leaf(0.6, 100, 1);
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn leaf_digest_distinct_on_did_change() {
        let a = dummy_leaf(0.5, 100, 1);
        let b = dummy_leaf(0.5, 100, 2);
        assert_ne!(a.digest(), b.digest());
    }

    // ----- Batch digest + caps -----

    #[test]
    fn batch_digest_changes_on_leaf_membership() {
        let now = 1_000u64; // inside window 3 (1000/300=3)
        let window = AnchorWindow::at(now);
        let b1 = ReputationAnchorBatch {
            controller_id: [1u8; 32],
            window,
            chain_block_height: 100,
            rotation_receipt_id: None,
            leaves: vec![dummy_leaf(0.5, 100, 1)],
        };
        let b2 = ReputationAnchorBatch {
            controller_id: [1u8; 32],
            window,
            chain_block_height: 100,
            rotation_receipt_id: None,
            leaves: vec![dummy_leaf(0.5, 100, 1), dummy_leaf(0.5, 100, 2)],
        };
        assert_ne!(b1.digest(), b2.digest());
    }

    #[test]
    fn batch_digest_changes_on_rotation_receipt_presence() {
        // Post-Round-7 amendment 51: a rotation-receipt binding MUST
        // change the on-chain commitment so the chain side can
        // distinguish pre-rotation resubmissions (None) from
        // post-rotation resubmissions (Some(id)). Without this the
        // DID-rotation finality rule cannot be enforced.
        let now = 1_000u64;
        let window = AnchorWindow::at(now);
        let without = ReputationAnchorBatch {
            controller_id: [1u8; 32],
            window,
            chain_block_height: 100,
            rotation_receipt_id: None,
            leaves: vec![dummy_leaf(0.5, 100, 1)],
        };
        let mut receipt = [0u8; 32];
        receipt[0] = 0xAB;
        receipt[31] = 0xCD;
        let with = ReputationAnchorBatch {
            controller_id: [1u8; 32],
            window,
            chain_block_height: 100,
            rotation_receipt_id: Some(receipt),
            leaves: vec![dummy_leaf(0.5, 100, 1)],
        };
        assert_ne!(without.digest(), with.digest());
    }

    #[test]
    fn within_leaf_cap_zero_leaves_passes() {
        let b = ReputationAnchorBatch {
            controller_id: [1u8; 32],
            window: AnchorWindow::at(0),
            chain_block_height: 0,
            rotation_receipt_id: None,
            leaves: vec![],
        };
        assert!(b.within_leaf_cap());
    }

    #[test]
    fn within_leaf_cap_at_limit_passes() {
        let mut b = ReputationAnchorBatch {
            controller_id: [1u8; 32],
            window: AnchorWindow::at(0),
            chain_block_height: 0,
            rotation_receipt_id: None,
            leaves: vec![],
        };
        for i in 0..MAX_TUPLES_PER_ROOT {
            b.leaves.push(dummy_leaf(0.5, 100, i as u8));
        }
        assert!(b.within_leaf_cap());
        assert_eq!(b.leaves.len() as u64, MAX_TUPLES_PER_ROOT);
    }

    #[test]
    fn exceeds_daily_fanout_at_limit_rejects() {
        // 100 already anchored + 1 proposed = 101 > 100 → reject
        assert!(exceeds_daily_fanout(100, 1));
    }

    #[test]
    fn exceeds_daily_fanout_below_limit_passes() {
        assert!(!exceeds_daily_fanout(0, 50));
        assert!(!exceeds_daily_fanout(99, 1));
    }

    // ----- Window collision -----

    #[test]
    fn window_collision_same_window_collides() {
        let a = AnchorWindow::at(1_000);
        let b = AnchorWindow::at(1_100); // also window 3
        assert!(window_collision(a, b));
    }

    #[test]
    fn window_collision_different_windows_do_not_collide() {
        let a = AnchorWindow::at(1_000); // window 3
        let b = AnchorWindow::at(1_500); // window 5
        assert!(!window_collision(a, b));
    }

    // ----- Finality gate -----

    #[test]
    fn finality_reached_at_minimum_depth() {
        // chain has progressed 12 blocks past the anchor's submission height
        assert!(is_finality_reached(100, 88));
    }

    #[test]
    fn finality_unreached_below_minimum_depth() {
        assert!(!is_finality_reached(100, 89));
    }

    #[test]
    fn finality_handles_saturating_subtraction() {
        // defensive: when finalized > submitted (reorg moved past the
        // anchor's claimed block), saturating_sub returns 0 which is
        // below MIN_FINALITY_BLOCKS, so the function returns false.
        // The assertion is that this does NOT underflow.
        assert!(!is_finality_reached(50, 100));
    }

    // ----- From aggregate -----

    #[test]
    fn leaf_from_aggregate_round_trips_score_ewma() {
        let agg = ReputationAggregate::dummy_for_test(1, 1_000, 0.5, 100);
        let leaf = AnchorLeaf::from_aggregate(&agg);
        assert_eq!(
            leaf.score_ewma_raw,
            crate::types::dfp_to_blob(&Dfp::from_f64(0.5))
        );
        assert_eq!(leaf.samples, 100);
        assert_eq!(leaf.signal_kind, agg.signal_kind);
        assert_eq!(leaf.layer, agg.layer);
    }
}
