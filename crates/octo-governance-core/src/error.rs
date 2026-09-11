//! Cross-trait `GovernanceError` envelope.

use thiserror::Error;

/// Cross-trait governance error envelope.
#[derive(Debug, Error)]
pub enum GovernanceError {
    /// Attempted invalid state transition on a `ProposalState`.
    #[error("invalid proposal transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// Source state.
        from: super::proposal::ProposalState,
        /// Target state.
        to: super::proposal::ProposalState,
    },

    /// Quorum threshold was not reached (voters did not collectively
    /// meet the `quorum_bps` threshold).
    #[error("quorum not reached: approval {approval_bps} + rejection {rejection_bps} < required {quorum_bps}")]
    QuorumNotReached {
        /// Summed approval basis points.
        approval_bps: u32,
        /// Summed rejection basis points.
        rejection_bps: u32,
        /// Required quorum in basis points.
        quorum_bps: u32,
    },

    /// A voter weight exceeds 10_000 bps (100%).
    #[error("invalid weight {weight} bps for voter {voter}")]
    InvalidWeight {
        /// Voter DID.
        voter: String,
        /// Reported weight in basis points.
        weight: u32,
    },

    /// Caller error (e.g. invalid input shape).
    #[error("governance error: {0}")]
    Caller(String),
}
