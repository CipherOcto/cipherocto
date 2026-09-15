//! Layer B governance facade for the cipherocto workspace (RFC-0013
//! §Module Layout).
//!
//! This facade re-exports the substrate surface (`octo-governance-core`)
//! verbatim. IO functions (`snapshot`, `attest`, `vote`) live at the
//! DOMAIN layer in `octo-network::mon::governance` per RFC-0013 §Scope.
//!
//! ## Snapshot projection (RFC-0011-g Phase 1)
//!
//! Per RFC-0011-g §Substrate `[ADD]`, the snapshot projection
//! surface (`snapshot()`, `SnapshotRef`, `ProposalSummary`,
//! `ProposalFilter`, `OctoGovernanceSnapshotCache`,
//! `GovernanceSnapshotError`) lands in the Layer B façade rather
//! than the Layer A frozen core, preserving the core contract
//! against additive changes.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles` Layer B, this crate is
//! **RFC-driven, additive only**. The re-export surface mirrors the
//! substrate's public API byte-for-byte.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cache;
pub mod error;
pub mod snapshot;

pub use cache::{OctoGovernanceSnapshotCache, SnapshotCacheKey, SNAPSHOT_CACHE_CAPACITY};
pub use error::GovernanceSnapshotError;
// Explicit curated re-export (RFC-0013 §Module Layout).
pub use octo_governance_core::tally_quorum;
pub use octo_governance_core::voting_weight;
pub use octo_governance_core::DecisionType;
pub use octo_governance_core::EmergencyAuthority;
pub use octo_governance_core::GovernanceError;
pub use octo_governance_core::GovernanceModel;
pub use octo_governance_core::GovernancePolicy;
pub use octo_governance_core::GovernanceProposal;
pub use octo_governance_core::ProposalState;
pub use snapshot::{
    snapshot, ProposalFilter, ProposalSummary, SnapshotRef, SnapshotView, TTL_SNAPSHOT_SECONDS,
};
