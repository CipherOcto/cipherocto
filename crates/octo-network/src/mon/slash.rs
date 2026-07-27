//! Slash reason codes and SlashEnvelope (RFC-0855p-b §B + mission 0851p-a-bootstrap-slashing).
//!
//! Slash reason codes are 16-bit identifiers allocated by the
//! mission overlay network governance:
//!
//! - `0x0001` = `founder-squat` (mission 0855p-b)
//! - `0x0002` = `evidence-tampering` (mission 0855p-b)
//! - `0x0003` = `transport-lying` (mission 0855p-b)
//! - `0x000A` = `transport-binding-lie` (mission 0850p-c §6)
//! - `0x000B` = `transport-route-misroute` (mission 0850p-c §6)
//! - `0x000C, 0x000E-0xFFFF` = reserved (0x000D, 0x000F in use)
//! - `0x000D` = `bootstrap_node_misbehavior` (mission 0851p-a)
//!
//! ## Mission 0851p-a-bootstrap-slashing
//!
//! The new `0x000D` code covers bootstrap node misbehavior with
//! sub-codes stored in `slash_reason_data`:
//! - `0x000D.01` = `withholds_peers`
//! - `0x000D.02` = `stale_data`
//! - `0x000D.03` = `censors_legit_peer`
//! - `0x000D.04` = `false_reachability_claim`
//!
//! The sub-code is encoded as `(0x000D << 16) | sub_code` in the
//! 32-bit `slash_reason_data` field.

use serde::{Deserialize, Serialize};

/// Bootstrap node misbehavior sub-codes (mission 0851p-a).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum BootstrapMisbehavior {
    /// `0x000D.01` — claims 0 reachable peers when it has > 0.
    WithholdsPeers = 0x0001,
    /// `0x000D.02` — serves seed list older than MAX_SEED_AGE_EPOCHS.
    StaleData = 0x0002,
    /// `0x000D.03` — refuses to include a specific peer that other seeds have.
    CensorsLegitPeer = 0x0003,
    /// `0x000D.04` — claims a peer is reachable when it is not.
    FalseReachabilityClaim = 0x0004,
}

/// Slash reason code constants.
pub mod slash_code {
    /// Bootstrap node misbehavior (mission 0851p-a).
    pub const BOOTSTRAP_NODE_MISBEHAVIOR: u16 = 0x000D;
    /// Transport binding lie (mission 0850p-c §6).
    pub const TRANSPORT_BINDING_LIE: u16 = 0x000A;
    /// Transport route misroute (mission 0850p-c §6).
    pub const TRANSPORT_ROUTE_MISROUTE: u16 = 0x000B;
}

/// A slash envelope cast by a witness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashEnvelope {
    /// The domain being slashed.
    pub domain_id: String,
    /// The slash instance id (unique per slash event).
    pub slash_id: String,
    /// The slash reason code (see `slash_code` constants).
    pub slash_reason: u16,
    /// Optional sub-code (used by `0x000D` for sub-codes like
    /// `WithholdsPeers`).
    #[serde(default)]
    pub slash_reason_data: u32,
    /// The slashed peer_id.
    pub target_peer: String,
    /// The witness's signature.
    pub signature: Vec<u8>,
    /// Unix epoch seconds.
    pub cast_at: u64,
}

impl SlashEnvelope {
    /// Create a bootstrap-misbehavior slash envelope with the
    /// given sub-code.
    pub fn bootstrap_misbehavior(
        domain_id: impl Into<String>,
        slash_id: impl Into<String>,
        target_peer: impl Into<String>,
        sub_code: BootstrapMisbehavior,
        signature: Vec<u8>,
        cast_at: u64,
    ) -> Self {
        let slash_reason_data =
            ((slash_code::BOOTSTRAP_NODE_MISBEHAVIOR as u32) << 16) | (sub_code as u32);
        Self {
            domain_id: domain_id.into(),
            slash_id: slash_id.into(),
            slash_reason: slash_code::BOOTSTRAP_NODE_MISBEHAVIOR,
            slash_reason_data,
            target_peer: target_peer.into(),
            signature,
            cast_at,
        }
    }

    /// Returns the bootstrap sub-code if this envelope uses the
    /// `0x000D` code, else None.
    pub fn bootstrap_sub_code(&self) -> Option<BootstrapMisbehavior> {
        if self.slash_reason != slash_code::BOOTSTRAP_NODE_MISBEHAVIOR {
            return None;
        }
        let sub = (self.slash_reason_data & 0xFFFF) as u16;
        match sub {
            0x0001 => Some(BootstrapMisbehavior::WithholdsPeers),
            0x0002 => Some(BootstrapMisbehavior::StaleData),
            0x0003 => Some(BootstrapMisbehavior::CensorsLegitPeer),
            0x0004 => Some(BootstrapMisbehavior::FalseReachabilityClaim),
            _ => None,
        }
    }

    /// Convert this envelope into a `SlashVote` for the canonical
    /// cross-platform aggregator (`mon::slash_aggregation`). The
    /// `platform` and `witness` are caller-supplied because the
    /// envelope only carries the witness's signature; `signature`
    /// is the witness's signature over the envelope's slash fields
    /// (already on `self.signature`).
    ///
    /// Per RFC-0968 §21 + §23 (Review-Round-7) the 2/3 witness
    /// majority here is the **EVIDENCE** threshold; the
    /// authoritative slash is the governance-issued event that
    /// `issue_governance_slash` produces once the evidence
    /// finalises. The vote is `Yes` always (envelope presence ==
    /// witness observation); the aggregator counts distinct
    /// witnesses.
    pub fn to_slash_vote(
        &self,
        platform: impl Into<String>,
        witness: impl Into<String>,
    ) -> crate::mon::slash_aggregation::SlashVote {
        crate::mon::slash_aggregation::SlashVote {
            domain_id: self.domain_id.clone(),
            slash_id: self.slash_id.clone(),
            platform: platform.into(),
            witness: witness.into(),
            vote: crate::mon::slash_aggregation::Vote::Yes,
            cast_at: self.cast_at,
            signature: self.signature.clone(),
        }
    }
}

/// Per-recorder bootstrap evidence flow (mission 0851p-a AC).
///
/// Witness envelopes with the same `(domain_id, slash_id)` AND the
/// same sub-code are aggregated via the canonical
/// `SlashAggregator`. When 2/3 of the bootstrap-domain witnesses
/// have submitted an envelope for the same recorder+sub-code, the
/// evidence is finalised; the caller then invokes
/// `octo_reputation::slash_api::issue_governance_slash` to issue
/// the authoritative governance-issued slash event that persists
/// to `ReputationStore`.
///
/// Per-recorder means: the `slash_id` is unique per target peer
/// (the bootstrap node being slashed). Different slashed peers
/// never share a `slash_id`, so two simultaneous slash flows
/// cannot accidentally aggregate each other's evidence.
pub struct BootstrapEvidence<'a> {
    /// The expected witness count for the bootstrap domain
    /// (`N` in the 2/3 majority formula).
    pub total_witnesses: usize,
    /// The sub-code being aggregated. Only envelopes matching this
    /// sub-code count toward the evidence threshold.
    pub sub_code: BootstrapMisbehavior,
    /// Envelopes submitted for the same `(domain_id, slash_id)`
    /// across the bootstrap domain.
    pub envelopes: &'a [SlashEnvelope],
    /// The platform name to stamp on each derived `SlashVote`
    /// (e.g. `"whatsapp"`, `"matrix"`).
    pub platform: &'a str,
}

impl<'a> BootstrapEvidence<'a> {
    /// Run the per-recorder evidence aggregation. Returns
    /// `AggregationResult::FinalizedYes` once the 2/3 witness
    /// threshold is met for the configured sub-code. Envelopes
    /// that don't match the sub-code are silently dropped (they
    /// flow to a sibling aggregation keyed by their sub-code).
    ///
    /// `witness_fn(envelope) -> Option<String>` resolves a
    /// witness identifier from each envelope. Witnesses that
    /// resolve to `None` or an empty string are rejected by the
    /// underlying `SlashAggregator` (no anonymous votes).
    pub fn finalize<F>(&self, mut witness_fn: F) -> crate::mon::slash_aggregation::AggregationResult
    where
        F: FnMut(&SlashEnvelope) -> Option<String>,
    {
        let domain_id = self
            .envelopes
            .first()
            .map(|e| e.domain_id.clone())
            .unwrap_or_default();
        let slash_id = self
            .envelopes
            .first()
            .map(|e| e.slash_id.clone())
            .unwrap_or_default();
        let mut agg = crate::mon::slash_aggregation::SlashAggregator::new(
            domain_id,
            slash_id,
            self.total_witnesses,
        );
        for env in self.envelopes {
            if env.bootstrap_sub_code() != Some(self.sub_code) {
                continue;
            }
            let Some(witness) = witness_fn(env) else {
                continue;
            };
            agg.add_vote(env.to_slash_vote(self.platform, witness));
        }
        agg.aggregate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mon::slash_aggregation::{AggregationResult, RejectionReason};

    #[test]
    fn bootstrap_misbehavior_sub_code_roundtrip() {
        let env = SlashEnvelope::bootstrap_misbehavior(
            "d1",
            "s1",
            "peer-abc",
            BootstrapMisbehavior::StaleData,
            vec![],
            1700000000,
        );
        assert_eq!(env.slash_reason, 0x000D);
        assert_eq!(
            env.bootstrap_sub_code(),
            Some(BootstrapMisbehavior::StaleData)
        );
    }

    #[test]
    fn non_bootstrap_slash_has_no_sub_code() {
        let env = SlashEnvelope {
            domain_id: "d1".into(),
            slash_id: "s1".into(),
            slash_reason: slash_code::TRANSPORT_BINDING_LIE,
            slash_reason_data: 0,
            target_peer: "p".into(),
            signature: vec![],
            cast_at: 0,
        };
        assert!(env.bootstrap_sub_code().is_none());
    }

    #[test]
    fn unknown_bootstrap_sub_code_returns_none() {
        let env = SlashEnvelope {
            domain_id: "d1".into(),
            slash_id: "s1".into(),
            slash_reason: 0x000D,
            slash_reason_data: 0x0099, // unknown
            target_peer: "p".into(),
            signature: vec![],
            cast_at: 0,
        };
        assert!(env.bootstrap_sub_code().is_none());
    }

    #[test]
    fn slash_envelope_serde_roundtrip() {
        let env = SlashEnvelope::bootstrap_misbehavior(
            "d1",
            "s1",
            "peer",
            BootstrapMisbehavior::WithholdsPeers,
            vec![1, 2, 3],
            1700000000,
        );
        let json = serde_json::to_string(&env).unwrap();
        let back: SlashEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
    }

    // -- Per-recorder evidence flow (mission 0851p-a AC) --

    /// Each of the 4 sub-codes (.01-.04) must flow through the
    /// aggregator and reach `FinalizedYes` once 2/3 of N=3
    /// witnesses submit an envelope.
    #[test]
    fn sub_code_01_withholds_peers_finalizes() {
        let envs = [
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer",
                BootstrapMisbehavior::WithholdsPeers,
                vec![1],
                1700000000,
            ),
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer",
                BootstrapMisbehavior::WithholdsPeers,
                vec![2],
                1700000001,
            ),
        ];
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::WithholdsPeers,
            envelopes: &envs,
            platform: "whatsapp",
        };
        let witnesses = ["w1", "w2"];
        let mut i = 0;
        let result = flow.finalize(|_| {
            let w = witnesses[i].to_string();
            i += 1;
            Some(w)
        });
        match result {
            AggregationResult::FinalizedYes { yes, .. } => assert_eq!(yes, 2),
            other => panic!("expected FinalizedYes, got {other:?}"),
        }
    }

    #[test]
    fn sub_code_02_stale_data_finalizes() {
        let envs = [
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s2",
                "peer",
                BootstrapMisbehavior::StaleData,
                vec![1],
                1700000000,
            ),
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s2",
                "peer",
                BootstrapMisbehavior::StaleData,
                vec![2],
                1700000001,
            ),
        ];
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::StaleData,
            envelopes: &envs,
            platform: "matrix",
        };
        let witnesses = ["w1", "w2"];
        let mut i = 0;
        let result = flow.finalize(|_| {
            let w = witnesses[i].to_string();
            i += 1;
            Some(w)
        });
        assert!(matches!(result, AggregationResult::FinalizedYes { .. }));
    }

    #[test]
    fn sub_code_03_censors_legit_peer_finalizes() {
        let envs = [
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s3",
                "peer",
                BootstrapMisbehavior::CensorsLegitPeer,
                vec![1],
                1700000000,
            ),
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s3",
                "peer",
                BootstrapMisbehavior::CensorsLegitPeer,
                vec![2],
                1700000001,
            ),
        ];
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::CensorsLegitPeer,
            envelopes: &envs,
            platform: "nostr",
        };
        let witnesses = ["w1", "w2"];
        let mut i = 0;
        let result = flow.finalize(|_| {
            let w = witnesses[i].to_string();
            i += 1;
            Some(w)
        });
        assert!(matches!(result, AggregationResult::FinalizedYes { .. }));
    }

    #[test]
    fn sub_code_04_false_reachability_claim_finalizes() {
        let envs = [
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s4",
                "peer",
                BootstrapMisbehavior::FalseReachabilityClaim,
                vec![1],
                1700000000,
            ),
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s4",
                "peer",
                BootstrapMisbehavior::FalseReachabilityClaim,
                vec![2],
                1700000001,
            ),
        ];
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::FalseReachabilityClaim,
            envelopes: &envs,
            platform: "telegram",
        };
        let witnesses = ["w1", "w2"];
        let mut i = 0;
        let result = flow.finalize(|_| {
            let w = witnesses[i].to_string();
            i += 1;
            Some(w)
        });
        assert!(matches!(result, AggregationResult::FinalizedYes { .. }));
    }

    /// 1 of 3 witnesses does NOT reach the 2/3 threshold; the
    /// aggregator rejects with `InsufficientYesVotes`. This is
    /// the EVIDENCE-path guard: a single witness can never
    /// unilaterally slash a bootstrap node.
    #[test]
    fn single_witness_rejects_with_insufficient_yes() {
        let envs = [SlashEnvelope::bootstrap_misbehavior(
            "d1",
            "s5",
            "peer",
            BootstrapMisbehavior::StaleData,
            vec![1],
            1700000000,
        )];
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::StaleData,
            envelopes: &envs,
            platform: "whatsapp",
        };
        let result = flow.finalize(|_| Some("w1".to_string()));
        match result {
            AggregationResult::Rejected { reason, yes, .. } => {
                assert_eq!(reason, RejectionReason::InsufficientYesVotes);
                assert_eq!(yes, 1);
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    /// Per-recorder isolation: an envelope for a different
    /// recorder (different `slash_id`) MUST NOT count toward the
    /// current evidence aggregation. Two parallel slash flows
    /// cannot accidentally cross-aggregate.
    #[test]
    fn mismatched_slash_id_does_not_pollute_aggregation() {
        let envs = [
            // s1 = peer A
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer-A",
                BootstrapMisbehavior::StaleData,
                vec![1],
                1700000000,
            ),
            // s2 = peer B — same sub-code, different recorder
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s2",
                "peer-B",
                BootstrapMisbehavior::StaleData,
                vec![2],
                1700000001,
            ),
        ];
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::StaleData,
            envelopes: &envs,
            platform: "whatsapp",
        };
        // Aggregator keys on the FIRST envelope's slash_id
        // (`s1`). Only envelope matching s1 counts. With 1 of 3
        // yes votes, evidence is rejected.
        let result = flow.finalize(|_| Some("w1".to_string()));
        match result {
            AggregationResult::Rejected { yes, .. } => {
                assert_eq!(yes, 1, "envelope for s2 must NOT count toward s1");
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    /// Sub-code filtering: envelopes with a DIFFERENT sub-code
    /// must not contaminate the current sub-code's aggregation.
    /// This is the per-evidence-type isolation property — a
    /// `WithholdsPeers` envelope must not flow into a `StaleData`
    /// aggregation.
    #[test]
    fn mismatched_sub_code_does_not_contaminate() {
        let envs = [
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer",
                BootstrapMisbehavior::WithholdsPeers,
                vec![1],
                1700000000,
            ),
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer",
                BootstrapMisbehavior::StaleData,
                vec![2],
                1700000001,
            ),
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer",
                BootstrapMisbehavior::StaleData,
                vec![3],
                1700000002,
            ),
        ];
        // Aggregating for StaleData: WithholdsPeers envelope dropped.
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::StaleData,
            envelopes: &envs,
            platform: "whatsapp",
        };
        let witnesses = ["w1", "w2"];
        let mut i = 0;
        let result = flow.finalize(|_| {
            let w = witnesses[i].to_string();
            i += 1;
            Some(w)
        });
        // 2 of 3 yes votes → finalized (2*3=6 >= 3*2=6).
        match result {
            AggregationResult::FinalizedYes { yes, .. } => {
                assert_eq!(yes, 2, "WithholdsPeers envelope must be filtered out");
            }
            other => panic!("expected FinalizedYes with 2 votes, got {other:?}"),
        }
    }

    /// Anonymous (no-witness) envelopes are dropped by the
    /// aggregator: `witness_fn` returns `None` for them. This
    /// prevents a single signature-less envelope from inflating
    /// the yes count.
    #[test]
    fn anonymous_envelope_dropped_by_witness_fn() {
        let envs = [
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer",
                BootstrapMisbehavior::StaleData,
                vec![1],
                1700000000,
            ),
            SlashEnvelope::bootstrap_misbehavior(
                "d1",
                "s1",
                "peer",
                BootstrapMisbehavior::StaleData,
                vec![2],
                1700000001,
            ),
        ];
        let flow = BootstrapEvidence {
            total_witnesses: 3,
            sub_code: BootstrapMisbehavior::StaleData,
            envelopes: &envs,
            platform: "whatsapp",
        };
        // Resolve only the first envelope; second is anonymous.
        let mut called = 0;
        let result = flow.finalize(|_| {
            called += 1;
            if called == 1 {
                Some("w1".to_string())
            } else {
                None
            }
        });
        match result {
            AggregationResult::Rejected { yes, .. } => {
                assert_eq!(yes, 1, "only w1 envelope counts; second is anonymous");
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    /// `to_slash_vote` propagates the envelope's slash_id +
    /// domain_id + cast_at + signature; the platform and witness
    /// are caller-supplied. The vote is always `Yes` because
    /// envelope presence == witness observation.
    #[test]
    fn envelope_to_slash_vote_round_trip() {
        let env = SlashEnvelope::bootstrap_misbehavior(
            "d1",
            "s1",
            "peer",
            BootstrapMisbehavior::CensorsLegitPeer,
            vec![0xAB, 0xCD],
            1_700_000_000,
        );
        let vote = env.to_slash_vote("whatsapp", "+15555550100");
        assert_eq!(vote.domain_id, "d1");
        assert_eq!(vote.slash_id, "s1");
        assert_eq!(vote.platform, "whatsapp");
        assert_eq!(vote.witness, "+15555550100");
        assert_eq!(vote.cast_at, 1_700_000_000);
        assert_eq!(vote.signature, vec![0xAB, 0xCD]);
        assert_eq!(vote.vote, crate::mon::slash_aggregation::Vote::Yes);
    }
}
