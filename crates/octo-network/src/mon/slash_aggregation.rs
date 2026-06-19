//! Cross-platform witness aggregation for slash votes
//! (mission 0850p-c-cross-platform-witness).
//!
//! Slash votes are cast by witnesses on each platform and
//! aggregated cross-platform to form a 2/3 majority of TOTAL
//! witnesses. This module provides the aggregation logic,
//! SlashVote envelope type, and gossip topic derivation.
//!
//! ## Slash finalization
//!
//! - N platforms, each with 1+ witness(es)
//! - 2/3 majority of TOTAL witnesses is required
//! - Votes are collected over a 60s window
//! - After 60s, finalize if 2/3 of N votes are received;
//!   otherwise reject
//!
//! ## Tie-break (N=2 case)
//!
//! - N=2, both yes → slash finalizes (2 ≥ 4/3 = 1.33)
//! - N=2, one yes, one no → not finalized (1 < 1.33)
//! - N=2, both no → not finalized
//!
//! ## Gossip topic
//!
//! `/dot/slash/{domain_id}/{slash_id}` per the mission spec.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The default vote collection window (60s per mission spec).
pub const SLASH_VOTE_WINDOW_SECS: u64 = 60;

/// Slash vote cast by a witness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashVote {
    /// The domain being slashed.
    pub domain_id: String,
    /// The slash instance (unique per slash event).
    pub slash_id: String,
    /// The platform the witness is on (e.g., "whatsapp", "matrix").
    pub platform: String,
    /// The witness's identifier (e.g., phone number, user ID).
    pub witness: String,
    /// The vote: Yes (slashed) or No (not slashed).
    pub vote: Vote,
    /// Unix epoch seconds when the vote was cast.
    pub cast_at: u64,
    /// The witness's signature over (domain_id, slash_id, platform,
    /// witness, vote, cast_at).
    pub signature: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vote {
    Yes,
    No,
}

/// Result of aggregating a slash vote collection window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationResult {
    /// The slash is finalized (yes votes ≥ 2/3 of N).
    FinalizedYes { yes: usize, no: usize, total: usize },
    /// The slash is rejected (yes votes < 2/3 of N).
    Rejected {
        yes: usize,
        no: usize,
        total: usize,
        reason: RejectionReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReason {
    /// Fewer than 2/3 of N votes were Yes.
    InsufficientYesVotes,
    /// No votes were received within the 60s window.
    NoVotes,
}

/// Slash vote aggregator. Tracks votes received and determines
/// when the slash is finalized or rejected.
#[derive(Clone, Debug)]
pub struct SlashAggregator {
    pub domain_id: String,
    pub slash_id: String,
    /// Total number of expected witnesses (N).
    pub total_witnesses: usize,
    pub votes: Vec<SlashVote>,
    pub window_started: u64,
    pub window_deadline: u64,
}

impl SlashAggregator {
    /// Create a new aggregator with the current epoch as the
    /// window start.
    pub fn new(
        domain_id: impl Into<String>,
        slash_id: impl Into<String>,
        total_witnesses: usize,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            domain_id: domain_id.into(),
            slash_id: slash_id.into(),
            total_witnesses,
            votes: Vec::new(),
            window_started: now,
            window_deadline: now + SLASH_VOTE_WINDOW_SECS,
        }
    }

    /// Add a vote to the aggregator. Returns true if the
    /// aggregator now has a final result (finalized or rejected).
    ///
    /// The vote is rejected if:
    /// - the `domain_id` or `slash_id` doesn't match
    /// - the `witness` is empty (anonymous votes are not allowed)
    pub fn add_vote(&mut self, vote: SlashVote) -> bool {
        if vote.domain_id != self.domain_id || vote.slash_id != self.slash_id {
            return false;
        }
        if vote.witness.is_empty() {
            return false;
        }
        // Replace any previous vote from the same witness.
        self.votes.retain(|v| v.witness != vote.witness);
        self.votes.push(vote);
        self.is_finalized()
    }

    /// Returns true if the slash is finalized (yes votes ≥ 2/3 of N)
    /// or rejected (no votes from any witness).
    pub fn is_finalized(&self) -> bool {
        let yes = self.votes.iter().filter(|v| v.vote == Vote::Yes).count();
        // 2/3 majority: yes * 3 >= total * 2.
        if self.total_witnesses == 0 {
            return false;
        }
        yes * 3 >= self.total_witnesses * 2
    }

    /// Aggregate the votes and return the result. Should be called
    /// after the 60s window has elapsed.
    pub fn aggregate(&self) -> AggregationResult {
        let yes = self.votes.iter().filter(|v| v.vote == Vote::Yes).count();
        let no = self.votes.iter().filter(|v| v.vote == Vote::No).count();
        // Guard: 0 total witnesses means the aggregator was
        // constructed with no participants. Refuse to finalize.
        if self.total_witnesses == 0 {
            return AggregationResult::Rejected {
                yes,
                no,
                total: 0,
                reason: RejectionReason::NoVotes,
            };
        }
        if self.votes.is_empty() {
            return AggregationResult::Rejected {
                yes,
                no,
                total: self.total_witnesses,
                reason: RejectionReason::NoVotes,
            };
        }
        // 2/3 majority: yes * 3 >= total * 2.
        if yes * 3 >= self.total_witnesses * 2 {
            AggregationResult::FinalizedYes {
                yes,
                no,
                total: self.total_witnesses,
            }
        } else {
            AggregationResult::Rejected {
                yes,
                no,
                total: self.total_witnesses,
                reason: RejectionReason::InsufficientYesVotes,
            }
        }
    }

    /// Build the libp2p gossip topic for this slash instance.
    pub fn gossip_topic(&self) -> String {
        format!("/dot/slash/{}/{}", self.domain_id, self.slash_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vote(witness: &str, v: Vote) -> SlashVote {
        SlashVote {
            domain_id: "d1".into(),
            slash_id: "s1".into(),
            platform: "whatsapp".into(),
            witness: witness.into(),
            vote: v,
            cast_at: 1700000000,
            signature: vec![],
        }
    }

    #[test]
    fn n1_single_yes_finalizes() {
        let mut agg = SlashAggregator::new("d1", "s1", 1);
        agg.add_vote(vote("w1", Vote::Yes));
        assert!(agg.is_finalized());
        match agg.aggregate() {
            AggregationResult::FinalizedYes { yes, no, total } => {
                assert_eq!(yes, 1);
                assert_eq!(no, 0);
                assert_eq!(total, 1);
            }
            other => panic!("expected FinalizedYes, got {other:?}"),
        }
    }

    #[test]
    fn n1_single_no_rejects() {
        let agg = SlashAggregator::new("d1", "s1", 1);
        let mut agg2 = agg.clone();
        agg2.add_vote(vote("w1", Vote::No));
        match agg2.aggregate() {
            AggregationResult::Rejected { reason, .. } => {
                assert_eq!(reason, RejectionReason::InsufficientYesVotes);
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn n2_both_yes_finalizes() {
        // 2 yes / 2 total → 2*3 = 6 >= 2*2 = 4 → finalized
        let mut agg = SlashAggregator::new("d1", "s1", 2);
        agg.add_vote(vote("w1", Vote::Yes));
        agg.add_vote(vote("w2", Vote::Yes));
        assert!(agg.is_finalized());
    }

    #[test]
    fn n2_yes_no_rejects() {
        // 1 yes / 2 total → 1*3 = 3 < 2*2 = 4 → rejected
        let mut agg = SlashAggregator::new("d1", "s1", 2);
        agg.add_vote(vote("w1", Vote::Yes));
        agg.add_vote(vote("w2", Vote::No));
        assert!(!agg.is_finalized());
        match agg.aggregate() {
            AggregationResult::Rejected { reason, .. } => {
                assert_eq!(reason, RejectionReason::InsufficientYesVotes);
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn n3_two_yes_finalizes() {
        // 2 yes / 3 total → 2*3 = 6 >= 3*2 = 6 → finalized (≥)
        let mut agg = SlashAggregator::new("d1", "s1", 3);
        agg.add_vote(vote("w1", Vote::Yes));
        agg.add_vote(vote("w2", Vote::Yes));
        assert!(agg.is_finalized());
    }

    #[test]
    fn n3_one_yes_rejects() {
        // 1 yes / 3 total → 1*3 = 3 < 3*2 = 6 → rejected
        let mut agg = SlashAggregator::new("d1", "s1", 3);
        agg.add_vote(vote("w1", Vote::Yes));
        assert!(!agg.is_finalized());
    }

    #[test]
    fn empty_window_rejects() {
        let agg = SlashAggregator::new("d1", "s1", 3);
        match agg.aggregate() {
            AggregationResult::Rejected { reason, .. } => {
                assert_eq!(reason, RejectionReason::NoVotes);
            }
            other => panic!("expected Rejected(NoVotes), got {other:?}"),
        }
    }

    #[test]
    fn duplicate_witness_vote_replaces() {
        let mut agg = SlashAggregator::new("d1", "s1", 1);
        agg.add_vote(vote("w1", Vote::No));
        agg.add_vote(vote("w1", Vote::Yes));
        assert_eq!(agg.votes.len(), 1);
        assert!(agg.is_finalized());
    }

    #[test]
    fn vote_for_wrong_slash_ignored() {
        let mut agg = SlashAggregator::new("d1", "s1", 1);
        let mut bad = vote("w1", Vote::Yes);
        bad.slash_id = "s2".into();
        let result = agg.add_vote(bad);
        assert!(!result);
        assert_eq!(agg.votes.len(), 0);
    }

    #[test]
    fn gossip_topic_format() {
        let agg = SlashAggregator::new("d1", "s1", 1);
        assert_eq!(agg.gossip_topic(), "/dot/slash/d1/s1");
    }

    #[test]
    fn empty_witness_vote_rejected() {
        // An anonymous (empty-witness) vote must not be
        // accepted: this prevents an attacker from inflating
        // the yes count with a single signature-less vote.
        let mut agg = SlashAggregator::new("d1", "s1", 3);
        let mut v = vote("w1", Vote::Yes);
        v.witness = String::new();
        let result = agg.add_vote(v);
        assert!(!result);
        assert_eq!(agg.votes.len(), 0);
    }

    #[test]
    fn n0_aggregator_rejects() {
        // 0 total witnesses must not finalize; the condition
        // yes*3 >= total*2 would be 0 >= 0 = true otherwise.
        let agg = SlashAggregator::new("d1", "s1", 0);
        assert!(!agg.is_finalized());
        match agg.aggregate() {
            AggregationResult::Rejected { reason, total, .. } => {
                assert_eq!(reason, RejectionReason::NoVotes);
                assert_eq!(total, 0);
            }
            other => panic!("expected Rejected(NoVotes), got {other:?}"),
        }
    }
}
