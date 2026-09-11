//! Layer B governance facade for the cipherocto workspace (RFC-0013
//! §Module Layout).
//!
//! This facade re-exports the substrate surface (`octo-governance-core`)
//! verbatim. IO functions (`snapshot`, `attest`, `vote`) live at the
//! DOMAIN layer in `octo-network::mon::governance` per RFC-0013 §Scope.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles` Layer B, this crate is
//! **RFC-driven, additive only**. The re-export surface mirrors the
//! substrate's public API byte-for-byte.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

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
