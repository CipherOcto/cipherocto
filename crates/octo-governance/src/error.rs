//! Snapshot-specific error envelope.
//!
//! Lives in the Layer B façade (`octo-governance`) per the
//! anti-Layer-A-change discipline: `octo-governance-core`
//! (Layer A frozen per RFC-0013) carries the canonical
//! `GovernanceError`; additive snapshot-specific failures
//! (`SnapshotStale`, `InvalidProposalState`, `InvalidFilter`)
//! land in this façade envelope without disturbing the frozen
//! core contract.
//!
//! `#[non_exhaustive]` per F-14 — future amendments add variants
//! (e.g., `CacheCapacityExceeded`, `FilterConflict`) without
//! central-enum edits across the workspace.

use thiserror::Error;

/// Snapshot-specific failure envelope. Distinct from
/// `octo_governance_core::GovernanceError` (which carries the
/// canonical proposal-state-machine errors) so the snapshot path
/// can evolve independently of the frozen core.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GovernanceSnapshotError {
    /// A `--proposal-state` filter label did not match any
    /// canonical `ProposalState` label. The substrate `ProposalState`
    /// `#[non_exhaustive]` enum carries the labels the CLI
    /// surface accepts; the CLI label mapping is documented at the
    /// `octo governance snapshot` dispatch boundary per
    /// RFC-0011-g §Subcommand Taxonomy.
    #[error("invalid proposal-state label `{state}`; expected one of `Open`, `Quorum-Reached`, `Closed-Accepted`, `Closed-Rejected`, `Closed-Expired` per RFC-0011-g §Subcommand Taxonomy")]
    InvalidProposalState {
        /// The unrecognized label supplied on the CLI.
        state: String,
    },

    /// A `--chain-id` filter failed RFC-0010 canonical form parsing.
    /// Carries the raw input + parse failure reason so the operator
    /// can correct the flag value.
    #[error("invalid chain-id `{input}`: {reason}")]
    InvalidChainId {
        /// Raw input supplied on the CLI.
        input: String,
        /// Parse failure reason (sanitized).
        reason: String,
    },

    /// The supplied `--snapshot-id` resolved to a snapshot whose
    /// `expires_at_unix <= now_unix` (TTL boundary inclusive on the
    /// stale side per RFC-0011-g §Performance Targets).
    #[error("snapshot stale: snapshot_id {snapshot_id_hex} expired {age_secs}s ago (TTL = 600s)")]
    SnapshotStale {
        /// Hex-encoded snapshot id (BLAKE3-256).
        snapshot_id_hex: String,
        /// Age in seconds since `expires_at_unix`.
        age_secs: u64,
    },

    /// Snapshot cache failed to record the freshly-minted
    /// `SnapshotRef` (LRU eviction race or capacity exhaustion).
    #[error("snapshot cache error: {reason}")]
    CacheError {
        /// Failure reason.
        reason: String,
    },
}
