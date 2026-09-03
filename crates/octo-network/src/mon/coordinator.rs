//! Layer C re-export shim for `octo_coordinator_types::state`.
//!
//! Per RFC-0855p-b §Key Files L847, the `mon/coordinator.rs` path is the
//! canonical Layer-C consumer entry-point for coordinator state-machine
//! substrate. We re-export rather than duplicate types per
//! [[cipherocto-design-principles]] §Stable Abstractions: state machine
//! survives 10-year migrations; specialization (DomainCoordinator per
//! RFC-0855p-c, mission-bound overrides per future RFC amendments) layers
//! on top via composition, not duplication.
//!
//! See `octo-coordinator-types::state` for the canonical Layer-B
//! definitions + transition validity + canonical-bytes derivation.

pub use octo_coordinator_types::state::{
    genesis_transition_valid, transition_valid, validate_transition, CoordinatorError,
    CoordinatorId, CoordinatorLifecycle, CoordinatorRecord, CoordinatorSource, ElectionBallot,
    ElectionTally, GenesisState, SlashProof, BLAKE3_REPUTATION_COORDINATOR_DOMAIN,
};
