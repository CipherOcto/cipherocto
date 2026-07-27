//! octo-reputation — CipherOcto Reputation Registry per RFC-0968.
//!
//! Phase 1 (this session): types + error + constants + digest foundations.
//! Phase 2-3 land in subsequent sessions per
//! `docs/plans/2026-07-27-mission-0968-implementation.md`.
//!
//! ## Module map
//!
//! - [`constants`] — canonical stake / quorum / TTL / domain-separator constants.
//! - [`digest`] — `ReputationDigest` 32-byte envelope digest over domain-separated BLAKE3.
//! - [`error`] — `ReputationError` enum, `#[repr(u8)]` 0x01..=0x32 per RFC-0968 §13.
//! - [`types`] — `SignalEvent`, `SignalKind`, `ReputationLayer`, `ReputationAggregate`,
//!   `RecorderId`, `RecorderDid`, `ControllerId`, `EventId`, `RotationProvenance`,
//!   `ParityEvidence`, `RetirementEligibility`.
//!
//! ## Determinism contract (RFC-0104)
//!
//! All score / delta fields are `octo_determin::Dfp`, never raw `f64`.
//! Canonical wire form: `DfpEncoding::from_dfp(&d).to_bytes() -> [u8; 24]`.

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod auth;
pub mod compat;
pub mod constants;
pub mod digest;
pub mod error;
pub mod recorder;
pub mod store;
pub mod types;

pub use auth::{
    governance_set_hash, AssetTag, ChainRef, GovernanceProof, GovernanceSnapshot, SlashDestination,
    SuspensionAuth,
};
pub use compat::{
    deterministic_f64_mirror, CompatKeymap, CompatMapping, DcRootedSlashReputationStore,
    F64MirrorPolicy, LegacyReputationStore, LegacyShadowError, ReputationStoreCompat,
    SlashReputationStore,
};
pub use digest::ReputationDigest;
pub use error::{ReputationError, StakeComponent};
pub use recorder::{check_stake, verify_registration};
pub use store::{InMemoryReputationStore, ReputationStore, StoreResult};
pub use types::{
    ControllerId, EventId, ParityEvidence, RecorderDid, RecorderId, ReputationAggregate,
    ReputationLayer, RetirementEligibility, RotationProvenance, SignalEvent, SignalKind,
};
