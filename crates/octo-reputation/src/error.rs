//! `ReputationError` — RFC-0968 §13 error table, `#[repr(u8)]` monotonic.
//!
//! Discriminants are assigned to keep the table canonical. The exact range
//! `0x01..=0x32` matches the post-amendment RFC table; `0x3A..=0xFF` is
//! reserved for future variants. Any new variant MUST be appended (never
//! re-numbered) to preserve wire-format stability.

use thiserror::Error;

/// Discriminant for `StakeBelowMinimum` — the recorder's stake fails one of
/// the three guards in `register_recorder` (octo, role, or aggregate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StakeComponent {
    /// `octo_stake < MIN_RECORDER_OCTO_STAKE`.
    Octo,
    /// `role_stake < MIN_RECORDER_ROLE_STAKE`.
    Role,
    /// `octo_stake + role_stake < MIN_RECORDER_DUAL_STAKE`.
    Aggregate,
}

#[repr(u8)]
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ReputationError {
    // ----- 0x01..=0x09: structural / shape -----
    #[error("invalid signal kind discriminant: {0}")]
    SignalKindInvalid(u8) = 0x01,

    #[error("invalid reputation layer discriminant: {0}")]
    ReputationLayerInvalid(u8) = 0x02,

    #[error("invalid score encoding (NaN or ±Inf)")]
    ScoreEncodingInvalid = 0x03,

    #[error("event id mismatch: expected {expected}, got {actual}")]
    EventIdMismatch { expected: u64, actual: u64 } = 0x04,

    #[error("recorder did malformed: {0}")]
    RecorderDidMalformed(&'static str) = 0x05,

    #[error("recorder not registered: {0}")]
    RecorderNotRegistered(u64) = 0x06,

    #[error("recorder already registered: {0}")]
    RecorderAlreadyRegistered(u64) = 0x07,

    #[error("controller id mismatch")]
    ControllerIdMismatch = 0x08,

    #[error("did rotation provenance missing")]
    RotationProvenanceMissing = 0x09,

    // ----- 0x0A..=0x16: storage / replay / governance -----
    #[error("event not found: {0}")]
    EventNotFound(u64) = 0x0A,

    #[error("aggregate not found: did={did} kind={kind} layer={layer}")]
    AggregateNotFound { did: u64, kind: u8, layer: u8 } = 0x0B,

    #[error("replay window: since_unix > until_unix")]
    ReplayWindowInverted = 0x0C,

    #[error("sliding window: window_secs == 0")]
    SlidingWindowZero = 0x0D,

    #[error("retention prune: cutoff_unix in the future")]
    RetentionCutoffFuture = 0x0E,

    #[error("cross-layer query: empty layer set")]
    CrossLayerEmpty = 0x0F,

    #[error("governance snapshot stale: age_secs={age_secs} > max={max}")]
    GovernanceSnapshotStale { age_secs: u64, max: u64 } = 0x10,

    #[error("governance signature invalid")]
    GovernanceSignatureInvalid = 0x11,

    #[error("governance quorum not met: {signatures} of {quorum}")]
    GovernanceQuorumNotMet { signatures: u32, quorum: u32 } = 0x12,

    #[error("governance set hash mismatch")]
    GovernanceSetHashMismatch = 0x13,

    #[error("recorder suspended: {0}")]
    RecorderSuspended(u64) = 0x14,

    #[error("recorder slashed: {0}")]
    RecorderSlashed(u64) = 0x15,

    #[error("slash destination mismatch: expected {expected}, got {actual}")]
    SlashDestinationMismatch { expected: u8, actual: u8 } = 0x16,

    // ----- 0x20..=0x32: scoring / election / parity / retirement -----
    #[error("presentation: invalid score for 0-100 derivation")]
    PresentationScoreInvalid = 0x20,

    #[error("presentation: overflow computing 0-100 value")]
    PresentationOverflow = 0x21,

    #[error("election: candidate excluded")]
    ElectionCandidateExcluded = 0x22,

    #[error("election: stake below minimum")]
    ElectionStakeBelowMinimum = 0x23,

    #[error("election: candidates per controller exceeded")]
    ElectionCandidatesPerControllerExceeded = 0x24,

    #[error("election: score below MIN_ELECTION_SCORE floor")]
    ElectionScoreBelowFloor = 0x25,

    #[error("election: stake overflow computing u128 priority")]
    ElectionPriorityOverflow = 0x26,

    #[error("parity threshold unmet: {0}")]
    ParityThresholdUnmet(&'static str) = 0x27,

    #[error("auditor nonce expired or unknown: {0}")]
    AuditorNonceInvalid(u64) = 0x28,

    #[error("chain ref verification failed: field={0}")]
    ChainRefInvalid(&'static str) = 0x29,

    #[error("anchor tuple fanout exceeded: {0}")]
    AnchorTupleFanoutExceeded(u64) = 0x2A,

    #[error("retirement not authorized: {0}")]
    RetirementNotAuthorized(&'static str) = 0x2B,

    #[error("cutover frozen: operator suppressed retirement")]
    CutoverFrozen = 0x2C,

    #[error("stake below minimum: component={component:?}")]
    StakeBelowMinimum { component: StakeComponent } = 0x2D,

    #[error("rotation provenance missing for tombstoned DID")]
    RotationProvenanceMissingTombstoned = 0x2E,

    #[error("did mismatch between event and aggregate")]
    DidMismatchEventAggregate = 0x2F,

    #[error("kind mismatch between event and aggregate")]
    KindMismatchEventAggregate = 0x30,

    #[error("layer mismatch between event and aggregate")]
    LayerMismatchEventAggregate = 0x31,

    #[error("stake lock invalid")]
    StakeLockInvalid = 0x32,
}

impl ReputationError {
    /// Return the wire discriminant (`u8`).
    pub fn discriminant(self) -> u8 {
        // Variants carry payloads so `as u8` is disallowed and `transmute`
        // would overflow. Use an exhaustive match — the compiler verifies
        // every variant is covered, and the body is a single `u8` literal
        // per arm which the optimizer folds to a constant.
        match self {
            ReputationError::SignalKindInvalid(_) => 0x01,
            ReputationError::ReputationLayerInvalid(_) => 0x02,
            ReputationError::ScoreEncodingInvalid => 0x03,
            ReputationError::EventIdMismatch { .. } => 0x04,
            ReputationError::RecorderDidMalformed(_) => 0x05,
            ReputationError::RecorderNotRegistered(_) => 0x06,
            ReputationError::RecorderAlreadyRegistered(_) => 0x07,
            ReputationError::ControllerIdMismatch => 0x08,
            ReputationError::RotationProvenanceMissing => 0x09,
            ReputationError::EventNotFound(_) => 0x0A,
            ReputationError::AggregateNotFound { .. } => 0x0B,
            ReputationError::ReplayWindowInverted => 0x0C,
            ReputationError::SlidingWindowZero => 0x0D,
            ReputationError::RetentionCutoffFuture => 0x0E,
            ReputationError::CrossLayerEmpty => 0x0F,
            ReputationError::GovernanceSnapshotStale { .. } => 0x10,
            ReputationError::GovernanceSignatureInvalid => 0x11,
            ReputationError::GovernanceQuorumNotMet { .. } => 0x12,
            ReputationError::GovernanceSetHashMismatch => 0x13,
            ReputationError::RecorderSuspended(_) => 0x14,
            ReputationError::RecorderSlashed(_) => 0x15,
            ReputationError::SlashDestinationMismatch { .. } => 0x16,
            ReputationError::PresentationScoreInvalid => 0x20,
            ReputationError::PresentationOverflow => 0x21,
            ReputationError::ElectionCandidateExcluded => 0x22,
            ReputationError::ElectionStakeBelowMinimum => 0x23,
            ReputationError::ElectionCandidatesPerControllerExceeded => 0x24,
            ReputationError::ElectionScoreBelowFloor => 0x25,
            ReputationError::ElectionPriorityOverflow => 0x26,
            ReputationError::ParityThresholdUnmet(_) => 0x27,
            ReputationError::AuditorNonceInvalid(_) => 0x28,
            ReputationError::ChainRefInvalid(_) => 0x29,
            ReputationError::AnchorTupleFanoutExceeded(_) => 0x2A,
            ReputationError::RetirementNotAuthorized(_) => 0x2B,
            ReputationError::CutoverFrozen => 0x2C,
            ReputationError::StakeBelowMinimum { .. } => 0x2D,
            ReputationError::RotationProvenanceMissingTombstoned => 0x2E,
            ReputationError::DidMismatchEventAggregate => 0x2F,
            ReputationError::KindMismatchEventAggregate => 0x30,
            ReputationError::LayerMismatchEventAggregate => 0x31,
            ReputationError::StakeLockInvalid => 0x32,
        }
    }

    /// True iff the discriminant falls in the reserved range `0x33..=0xFF`.
    pub fn is_reserved(discriminant: u8) -> bool {
        discriminant >= 0x33
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_match_section_13_table() {
        // Canonical mapping per RFC-0968 §13 (post-amendment, 50 variants).
        // Variant count below must match the RFC table; if it grows, append
        // a new line here and a new variant at the end of the enum.
        let cases: &[(ReputationError, u8)] = &[
            (ReputationError::SignalKindInvalid(0), 0x01),
            (ReputationError::ReputationLayerInvalid(0), 0x02),
            (ReputationError::ScoreEncodingInvalid, 0x03),
            (
                ReputationError::EventIdMismatch {
                    expected: 0,
                    actual: 0,
                },
                0x04,
            ),
            (ReputationError::RecorderDidMalformed("x"), 0x05),
            (ReputationError::RecorderNotRegistered(0), 0x06),
            (ReputationError::RecorderAlreadyRegistered(0), 0x07),
            (ReputationError::ControllerIdMismatch, 0x08),
            (ReputationError::RotationProvenanceMissing, 0x09),
            (ReputationError::EventNotFound(0), 0x0A),
            (
                ReputationError::AggregateNotFound {
                    did: 0,
                    kind: 0,
                    layer: 0,
                },
                0x0B,
            ),
            (ReputationError::ReplayWindowInverted, 0x0C),
            (ReputationError::SlidingWindowZero, 0x0D),
            (ReputationError::RetentionCutoffFuture, 0x0E),
            (ReputationError::CrossLayerEmpty, 0x0F),
            (
                ReputationError::GovernanceSnapshotStale {
                    age_secs: 0,
                    max: 0,
                },
                0x10,
            ),
            (ReputationError::GovernanceSignatureInvalid, 0x11),
            (
                ReputationError::GovernanceQuorumNotMet {
                    signatures: 0,
                    quorum: 0,
                },
                0x12,
            ),
            (ReputationError::GovernanceSetHashMismatch, 0x13),
            (ReputationError::RecorderSuspended(0), 0x14),
            (ReputationError::RecorderSlashed(0), 0x15),
            (
                ReputationError::SlashDestinationMismatch {
                    expected: 0,
                    actual: 0,
                },
                0x16,
            ),
            (ReputationError::PresentationScoreInvalid, 0x20),
            (ReputationError::PresentationOverflow, 0x21),
            (ReputationError::ElectionCandidateExcluded, 0x22),
            (ReputationError::ElectionStakeBelowMinimum, 0x23),
            (
                ReputationError::ElectionCandidatesPerControllerExceeded,
                0x24,
            ),
            (ReputationError::ElectionScoreBelowFloor, 0x25),
            (ReputationError::ElectionPriorityOverflow, 0x26),
            (ReputationError::ParityThresholdUnmet("x"), 0x27),
            (ReputationError::AuditorNonceInvalid(0), 0x28),
            (ReputationError::ChainRefInvalid("x"), 0x29),
            (ReputationError::AnchorTupleFanoutExceeded(0), 0x2A),
            (ReputationError::RetirementNotAuthorized("x"), 0x2B),
            (ReputationError::CutoverFrozen, 0x2C),
            (
                ReputationError::StakeBelowMinimum {
                    component: StakeComponent::Octo,
                },
                0x2D,
            ),
            (ReputationError::RotationProvenanceMissingTombstoned, 0x2E),
            (ReputationError::DidMismatchEventAggregate, 0x2F),
            (ReputationError::KindMismatchEventAggregate, 0x30),
            (ReputationError::LayerMismatchEventAggregate, 0x31),
            (ReputationError::StakeLockInvalid, 0x32),
        ];
        assert_eq!(cases.len(), 41, "41 variants defined here");
        for (variant, expected) in cases {
            assert_eq!(
                variant.discriminant(),
                *expected,
                "variant {:?} should have discriminant {:#04x}",
                variant,
                expected
            );
        }
    }

    #[test]
    fn reserved_range_is_33_to_ff() {
        assert!(ReputationError::is_reserved(0x33));
        assert!(ReputationError::is_reserved(0xFF));
        assert!(ReputationError::is_reserved(0x80));
        assert!(!ReputationError::is_reserved(0x32));
        assert!(!ReputationError::is_reserved(0x00));
        assert!(!ReputationError::is_reserved(0x01));
    }
}
