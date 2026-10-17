//! Layer A frozen governance substrate (RFC-0013 §Specification).
//!
//! This crate owns the canonical governance primitives:
//!
//! - [`GovernancePolicy`] — canonical policy struct (issuer + model +
//!   emergency authority).
//! - [`GovernanceModel`] — 5-variant enum (Centralized, Dao, Federated,
//!   AiAssisted, Autonomous).
//! - [`EmergencyAuthority`] — 3-variant enum (None, GovernanceCouncil,
//!   DesignatedRecovery).
//! - [`GovernanceProposal`] + [`ProposalState`] — proposal struct + 6-variant
//!   state machine (Created → Voting → Approved/Rejected → Executed/Expired).
//! - [`DecisionType`] — 7-variant decision tag (Admission,
//!   RoleAssignment, TopologyChange, MissionTermination,
//!   PolicyModification, EmergencyRekey, ParticipantExpulsion).
//! - [`voting_weight`] + [`tally_quorum`] — pure tally helpers
//!   (deterministic across replicas per RFC-0013 §Cross-Replica Tally
//!   Equivalence).
//! - [`CapabilityToken`] — substrate-faithful capability handle
//!   (RFC-0011-g §7.4 vote substrate signature `voter_cap`).
//! - [`GovernanceError`] — InvalidTransition / QuorumNotReached /
//!   InvalidWeight cross-trait envelope.
//!
//! ## Layer discipline (per CLAUDE.md §Architectural Principles)
//!
//! Per RFC-0013 §Security Considerations, this crate is **RFC-frozen +
//! semver-major only**. Deps are restricted to Layer A primitives
//! (`serde` + `thiserror`); no IO, no storage, no clock, no randomness.
//! IO functions (`snapshot`, `attest`, `vote`) live in DOMAIN crates
//! (e.g. `octo-network::mon::governance`).
//!
//! ## Module layout (RFC-0013 §Module Layout)
//!
//! - [`policy`] — `GovernancePolicy` + `GovernanceModel` + `EmergencyAuthority`
//! - [`proposal`] — `GovernanceProposal` + `ProposalState` + `DecisionType`
//! - [`tally`] — `voting_weight` + `tally_quorum` pure helpers
//! - [`error`] — `GovernanceError` cross-trait envelope
//! - [`capability`] — `CapabilityToken` substrate-faithful newtype
//!   (RFC-0011-g §7.4 vote signature parameter)

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod capability;
pub mod error;
pub mod policy;
pub mod proposal;
pub mod receipt;
pub mod tally;

pub use capability::CapabilityToken;
pub use error::GovernanceError;
pub use policy::{EmergencyAuthority, GovernanceModel, GovernancePolicy};
pub use proposal::{DecisionType, GovernanceProposal, ProposalState};
pub use receipt::{AttestationReceipt, VoteReceipt};
pub use tally::{tally_quorum, voting_weight};
