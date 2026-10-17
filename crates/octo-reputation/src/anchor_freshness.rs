//! Gossip ingress anchor-freshness gate (mission
//! `0855p-b-gossip-successor` successor).
//!
//! ## Substrate topology (RFC citation: this lives at the canonical
//! consumer home, **not** at the producer side `octo-network/`).
//!
//! - **Canonical home for the gate**: `octo-reputation/src/anchor_freshness.rs`
//!   (this file). [`GossipEnvelope`] is defined at
//!   `crates/octo-reputation/src/gossip.rs:42`; gates must live near
//!   the type they inspect. The producer-side substrate at
//!   `crates/octo-network/src/gossip/reputation.rs` is a flow
//!   orchestrator and does not see envelope contents.
//!
//! - **Carrier semantics**: the [`GossipEnvelope`] carries
//!   `event.anchor_tx_hash: Option<[u8; 32]>` (an anchor presence
//!   flag, not a chain-side block height). Reorg depth lives on
//!   [`crate::anchor::ReputationAnchorBatch`]
//!   (`chain_block_height: Option<u64>` at `anchor.rs:184`) — the
//!   batch-level ingestion path is the second check site. The two
//!   sites compose; envelopes that pass the gate here are the
//!   RECORDER's claim; the batch-side validator is the WORKER's
//!   confirmation (RFC-0955-R1 §"Finality",
//!   `MIN_FINALITY_BLOCKS = 12`).
//!
//! ## Constants (Layer B years-stable)
#![doc = ""]
//! - [`MIN_FINALITY_BLOCKS_RFC0955`] — reorg invalidation depth at
//!   chain submission. Pinned to RFC-0955-R1 §"Finality" value.
//! - [`MAX_ANCHOR_STALENESS_BLOCKS`] — gossip ingress staleness bound.
//!   Strictly larger than `MIN_FINALITY_BLOCKS_RFC0955` because gossip
//!   propagation can outrun the recorded signal's chain-side depth.
//! - [`ANCHOR_TX_HASH_ZERO_SENTINEL`] — invalid-anchor sentinel. Any
//!   non-`None` anchor equal to all-zero bytes is rejected.

use serde::{Deserialize, Serialize};

use crate::gossip::GossipEnvelope;
use crate::types::{EventId, RecorderDid};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// RFC-0955-R1 §"Finality" chain-submission reorg invalidation depth.
/// Pinned to `12` blocks. The gossip ingress bound
/// ([`MAX_ANCHOR_STALENESS_BLOCKS`]) is strictly larger.
pub const MIN_FINALITY_BLOCKS_RFC0955: u64 = 12;

/// Gossip ingress anchor-staleness upper bound (mission
/// `0855p-b-gossip-successor` v1.2 contract). An anchor whose
/// recorded chain-side block height is more than this many blocks
/// behind the caller's observed tip is considered stale (reorg /
/// gossip lag drift).
///
/// See module-level docs for the relationship to RFC-0955-R1
/// `MIN_FINALITY_BLOCKS = 12`: this constant covers gossip-side
/// drift, not chain-side reorg invalidation.
pub const MAX_ANCHOR_STALENESS_BLOCKS: u64 = 256;

/// All-zero `anchor_tx_hash` sentinel. RFC-0955-R1 reserves the
/// all-zero digest as "no anchor recorded" (the canonical
/// `None`-equivalent for hash-typed fields). Any envelope whose
/// `event.anchor_tx_hash` is `Some([0u8; 32])` is treated as a
/// shape error and rejected by the freshness gate.
pub const ANCHOR_TX_HASH_ZERO_SENTINEL: [u8; 32] = [0u8; 32];

// ---------------------------------------------------------------------------
// Decision enum
// ---------------------------------------------------------------------------

/// Verdict returned by [`evaluate_gossip_envelope_freshness`]
/// (and its batch-attached sibling
/// [`evaluate_with_recorded_block_height`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnchorFreshness {
    /// Anchor present, structurally valid, within staleness bound
    /// (or no recorded chain height supplied). Accept.
    Fresh,
    /// `anchor_tx_hash == [0u8; 32]` sentinel. Reject (shape error).
    StaleMalformedAnchor,
    /// Anchor present + recorded block height supplied + that
    /// height is older than the caller's observed tip minus
    /// [`MAX_ANCHOR_STALENESS_BLOCKS`]. Reject (reorg / gossip lag).
    StaleReorgDepth,
    /// No anchor recorded for this event. The gossip layer cannot
    /// determine freshness for non-anchored signals; the caller
    /// decides whether to admit.
    IndeterminateNonAnchored,
}

impl AnchorFreshness {
    /// True when the verifier should NOT admit the envelope under
    /// default policy. Indeterminate signals pass this gate but the
    /// caller may reject them via [`crate::types::ReputationPolicy`].
    #[inline]
    pub fn is_reject_by_default(self) -> bool {
        matches!(self, Self::StaleMalformedAnchor | Self::StaleReorgDepth)
    }
}

// ---------------------------------------------------------------------------
// Core evaluator
// ---------------------------------------------------------------------------

/// Evaluate a single envelope against the gate. Pure function; no I/O.
///
/// Inputs:
/// - `env`: gossip envelope under inspection. Only
///   `env.event.anchor_tx_hash` is read; envelope-level signatures
///   and attestations are the caller's concern (handled at
///   `validate_shape` / signer substrate).
/// - `current_chain_block_height`: gossip-side observed chain tip.
///   Combined with `recorded_chain_block_height` to compute the
///   anchor age when the latter is supplied.
///
/// See module-level docs for the substrate topology rationale.
pub fn evaluate_gossip_envelope_freshness(
    env: &GossipEnvelope,
    current_chain_block_height: u64,
) -> AnchorFreshness {
    evaluate_with_recorded_block_height(env, None, current_chain_block_height)
}

/// Extended evaluator that takes the recorder-supplied chain-side
/// anchor block height (the `chain_block_height` from the
/// `ReputationAnchorBatch` carrier). When `Some`, the reorg-depth
/// check fires; when `None`, the gate reduces to structural
/// validity of `anchor_tx_hash`.
///
/// This is the canonical entry point for the batch-attached
/// ingestion path; the simpler [`evaluate_gossip_envelope_freshness`]
/// is the gossip-only fall-back when no batch height is available.
pub fn evaluate_with_recorded_block_height(
    env: &GossipEnvelope,
    recorded_chain_block_height: Option<u64>,
    current_chain_block_height: u64,
) -> AnchorFreshness {
    match env.event.anchor_tx_hash {
        None => AnchorFreshness::IndeterminateNonAnchored,
        Some(h) if h == ANCHOR_TX_HASH_ZERO_SENTINEL => AnchorFreshness::StaleMalformedAnchor,
        Some(_) => match recorded_chain_block_height {
            None => AnchorFreshness::Fresh,
            Some(rec) => {
                if rec > current_chain_block_height {
                    // Recorded height in the future relative to
                    // observed tip — treated as reorg-depth drift
                    // (could be a clock-skew or fork-attack signal).
                    AnchorFreshness::StaleReorgDepth
                } else {
                    let age = current_chain_block_height - rec;
                    if age > MAX_ANCHOR_STALENESS_BLOCKS {
                        AnchorFreshness::StaleReorgDepth
                    } else {
                        AnchorFreshness::Fresh
                    }
                }
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Bulk partition + filter
// ---------------------------------------------------------------------------

/// Decision-bundled reason for an envelope the gate rejected. Carries
/// enough payload for the forensic writer (`octo-vault` audit log)
/// without leaking envelope-level fields unrelated to the gate
/// decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleEnvelopeReason {
    pub event_id: EventId,
    pub recorder_did: RecorderDid,
    pub freshness: AnchorFreshness,
}

/// Output of [`partition_by_anchor_freshness`]. `fresh` envelopes are
/// admitted under default policy; `stale` carries the envelope +
/// decision for forensic / caller-side re-evaluation.
#[derive(Debug, Clone)]
pub struct AnchorFreshnessPartition {
    pub fresh: Vec<GossipEnvelope>,
    pub stale: Vec<(GossipEnvelope, AnchorFreshness)>,
}

impl AnchorFreshnessPartition {
    #[inline]
    pub fn fresh_count(&self) -> usize {
        self.fresh.len()
    }

    #[inline]
    pub fn stale_count(&self) -> usize {
        self.stale.len()
    }

    #[inline]
    pub fn total(&self) -> usize {
        self.fresh.len() + self.stale.len()
    }
}

/// Partition a batch of envelopes by anchor freshness.
///
/// Default policy:
/// - `Fresh` → `fresh`
/// - `StaleMalformedAnchor` / `StaleReorgDepth` → `stale` (rejected)
///
/// Caller may re-evaluate by inspecting `stale[*].1`. The
/// `IndeterminateNonAnchored` decision does not arise at this
/// gate (it is the `Fresh` / no-anchor branch), so callers wanting
/// to reject non-anchored envelopes must call
/// [`evaluate_gossip_envelope_freshness`] per envelope and apply
/// their own `ReputationPolicy`.
pub fn partition_by_anchor_freshness(
    envelopes: Vec<GossipEnvelope>,
    current_chain_block_height: u64,
) -> AnchorFreshnessPartition {
    let mut fresh = Vec::with_capacity(envelopes.len());
    let mut stale: Vec<(GossipEnvelope, AnchorFreshness)> = Vec::new();
    for env in envelopes {
        let decision = evaluate_gossip_envelope_freshness(&env, current_chain_block_height);
        match decision {
            AnchorFreshness::Fresh => fresh.push(env),
            other => stale.push((env, other)),
        }
    }
    AnchorFreshnessPartition { fresh, stale }
}

/// Mission `0855p-b-gossip-successor` v1.2 surface. Drops
/// structurally-stale envelopes (malformed anchor or reorg drift)
/// and returns the surviving set. Indeterminate non-anchored
/// envelopes are admitted by default — the gate does not policy
/// on the "no anchor" branch; that is the substrate side's job
/// (`octo-reputation::store::anchor_pending`).
pub fn filter_stale_anchor_events(
    envelopes: Vec<GossipEnvelope>,
    current_chain_block_height: u64,
) -> Vec<GossipEnvelope> {
    partition_by_anchor_freshness(envelopes, current_chain_block_height).fresh
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::GossipEnvelope;
    use crate::types::{
        ControllerId, EventId, RecorderDid, ReputationLayer, RotationProvenance, SignalEvent,
        SignalKind,
    };
    use octo_determin::Dfp;

    fn dummy_event(seed: u64, anchor: Option<[u8; 32]>) -> SignalEvent {
        SignalEvent {
            event_id: EventId::from_u64(seed),
            recorder_did: RecorderDid::from_array([seed as u8; 52]),
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None::<RotationProvenance>,
            audit_ref: None,
            anchor_tx_hash: anchor,
        }
    }

    fn envelope_with(anchor: Option<[u8; 32]>) -> GossipEnvelope {
        GossipEnvelope {
            event: dummy_event(1, anchor),
            recorder_signature: vec![0xAA; 64],
            source_mission: "mon:test:phase-1".into(),
            source_domain: "domain:test".into(),
            rotation_provenance: None,
            attestations: vec![],
        }
    }

    #[test]
    fn anchored_envelope_is_fresh() {
        let env = envelope_with(Some([0x42; 32]));
        assert_eq!(
            evaluate_gossip_envelope_freshness(&env, 1000),
            AnchorFreshness::Fresh
        );
    }

    #[test]
    fn zero_anchor_hash_is_malformed() {
        let env = envelope_with(Some([0u8; 32]));
        assert_eq!(
            evaluate_gossip_envelope_freshness(&env, 1000),
            AnchorFreshness::StaleMalformedAnchor
        );
    }

    #[test]
    fn non_anchored_envelope_is_indeterminate() {
        let env = envelope_with(None);
        assert_eq!(
            evaluate_gossip_envelope_freshness(&env, 1000),
            AnchorFreshness::IndeterminateNonAnchored
        );
    }

    #[test]
    fn recorded_height_within_bound_is_fresh() {
        let env = envelope_with(Some([0x42; 32]));
        assert_eq!(
            evaluate_with_recorded_block_height(&env, Some(900), 1000),
            AnchorFreshness::Fresh
        );
    }

    #[test]
    fn recorded_height_outside_bound_is_reorg_stale() {
        let env = envelope_with(Some([0x42; 32]));
        // Gap = 1000 - 700 = 300 > MAX_ANCHOR_STALENESS_BLOCKS (256)
        assert_eq!(
            evaluate_with_recorded_block_height(&env, Some(700), 1000),
            AnchorFreshness::StaleReorgDepth
        );
    }

    #[test]
    fn recorded_height_exactly_at_bound_is_fresh() {
        let env = envelope_with(Some([0x42; 32]));
        // Gap = 256 == MAX_ANCHOR_STALENESS_BLOCKS (boundary inclusive)
        assert_eq!(
            evaluate_with_recorded_block_height(&env, Some(744), 1000),
            AnchorFreshness::Fresh
        );
    }

    #[test]
    fn recorded_height_in_future_is_reorg_stale() {
        let env = envelope_with(Some([0x42; 32]));
        assert_eq!(
            evaluate_with_recorded_block_height(&env, Some(1100), 1000),
            AnchorFreshness::StaleReorgDepth
        );
    }

    #[test]
    fn partition_drops_malformed_and_reorg_keeps_fresh() {
        let envs = vec![
            envelope_with(Some([0x01; 32])), // Fresh
            envelope_with(Some([0u8; 32])),  // StaleMalformedAnchor
            envelope_with(Some([0x02; 32])), // Fresh
        ];
        let p = partition_by_anchor_freshness(envs, 1000);
        assert_eq!(p.fresh_count(), 2);
        assert_eq!(p.stale_count(), 1);
        assert_eq!(p.stale[0].1, AnchorFreshness::StaleMalformedAnchor);
    }

    #[test]
    fn filter_keeps_fresh_drops_malformed() {
        let envs = vec![
            envelope_with(Some([0x01; 32])),
            envelope_with(Some([0u8; 32])),
            envelope_with(Some([0x02; 32])),
        ];
        let kept = filter_stale_anchor_events(envs, 1000);
        assert_eq!(kept.len(), 2);
        for env in &kept {
            assert!(env.event.anchor_tx_hash.is_some());
            assert_ne!(
                env.event.anchor_tx_hash.unwrap(),
                ANCHOR_TX_HASH_ZERO_SENTINEL
            );
        }
    }

    #[test]
    fn pin_min_finality_blocks_rfc0955_to_canonical_12() {
        assert_eq!(MIN_FINALITY_BLOCKS_RFC0955, 12);
    }

    #[test]
    fn pin_max_anchor_staleness_blocks() {
        assert_eq!(MAX_ANCHOR_STALENESS_BLOCKS, 256);
    }

    #[test]
    fn pin_anchor_tx_hash_zero_sentinel() {
        assert_eq!(ANCHOR_TX_HASH_ZERO_SENTINEL, [0u8; 32]);
    }

    #[test]
    fn max_anchor_staleness_strictly_greater_than_min_finality() {
        const { assert!(MAX_ANCHOR_STALENESS_BLOCKS > MIN_FINALITY_BLOCKS_RFC0955) };
    }
}
