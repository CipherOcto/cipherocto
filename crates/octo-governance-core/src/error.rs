//! Cross-trait `GovernanceError` envelope.

use thiserror::Error;

/// Cross-trait governance error envelope.
///
/// Variants introduced by RFC-0011-g (governance attestation +
/// vote IO surfaces):
/// - [`GovernanceError::UnknownAttestationKind`] — unknown
///   TypedDiscriminator (RFC-0011-g §Attestation Kind Resolution).
/// - [`GovernanceError::PrereqNotAccepted`] — substrate
///   prereq gate (RFC-0011-g §Compatibility Mixed-Version
///   Compatibility; multi-prereq conjunction).
/// - [`GovernanceError::DuplicateAttestation`] — append-only
///   invariant on `attestation_log` (RFC-0011-g §7.4
///   `attestation_id = BLAKE3-256(canonical_ser(envelope))`
///   PK uniqueness).
/// - [`GovernanceError::InvalidArgument`] — substrate IO
///   argument validation (RFC-0011-g §7.4 input invariants).
/// - [`GovernanceError::Internal`] — wraps substrate IO /
///   signing failures (fail-closed envelope).
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

    /// A voter weight exceeds 10_000 bps (100%). Cold-path error:
    /// `voter` is cloned because this error is expected to be returned
    /// at most once per malformed tally and is logged / displayed
    /// rather than chained through a hot loop.
    #[error("invalid weight {weight} bps for voter {voter}")]
    InvalidWeight {
        /// Voter DID.
        voter: String,
        /// Reported weight in basis points.
        weight: u32,
    },

    /// Unknown attestation `kind_ref` (TypedDiscriminator not
    /// registered in the substrate registry). RFC-0011-g
    /// §Attestation Kind Resolution + cipherocto-design-principles
    /// §Extension over enumeration (fail-closed on unknown
    /// discriminators).
    #[error("unknown attestation kind: {kind_ref}")]
    UnknownAttestationKind {
        /// Novel `kind_ref` string that did not resolve.
        kind_ref: String,
    },

    /// Substrate prereq gate (RFC-0011-g §Compatibility
    /// Mixed-Version Compatibility). The operator invoked a
    /// surface whose prerequisite RFC has not yet reached
    /// Accepted. The substrate fails-closed with the failing
    /// `rfc_ref` so the CLI surfaces `OctoCliError::PrereqNotAccepted`
    /// (exit 38) during the Draft → Accepted window.
    #[error("prerequisite RFC not accepted: {rfc_ref}")]
    PrereqNotAccepted {
        /// Failing prerequisite RFC id (e.g. `RFC-0855p-d`).
        rfc_ref: String,
    },

    /// Duplicate `attestation_id` PK detected in
    /// `attestation_log`. The append-only invariant guarantees
    /// uniqueness: a duplicate PK means the canonical
    /// `BLAKE3-256(canonical_ser(envelope))` collisioned (i.e.
    /// two distinct envelopes that share canonical-bytes).
    /// Cold-path error; the ledger state is unchanged on
    /// error.
    #[error("duplicate attestation_id: {attestation_id:?}")]
    DuplicateAttestation {
        /// Colliding attestation PK.
        attestation_id: [u8; 32],
    },

    /// Substrate IO argument validation failure (RFC-0011-g
    /// §7.4 input invariants). E.g. both `evidence` and
    /// `evidence_hash` supplied, neither supplied, etc.
    #[error("invalid argument: {reason}")]
    InvalidArgument {
        /// Sanitized validation reason.
        reason: String,
    },

    /// Internal substrate failure (HSM unavailable,
    /// persistence IO, etc.). Fail-closed envelope per
    /// RFC-0011-g §Error Handling "substrate fails closed".
    #[error("internal substrate error: {reason}")]
    Internal {
        /// Sanitized internal failure reason.
        reason: String,
    },
}
