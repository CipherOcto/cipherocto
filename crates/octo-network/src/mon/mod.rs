//! Mission Overlay Networks (MON) — RFC-0855
//!
//! Mission-scoped overlay networks with deterministic lifecycle,
//! membership roles, topology models, key hierarchy, and governance.

pub mod bind_envelope;
pub mod bootstrap;
pub mod discovery;
pub mod economics;
pub mod error;
pub mod execution;
pub mod gossip;
pub mod governance;
pub mod keys;
pub mod lifecycle;
pub mod membership;
pub mod nostr_bootstrap;
pub mod rebind;
pub mod reconciliation;
pub mod slash;
pub mod slash_aggregation;
pub mod topology;
pub mod trust_graph;

pub mod mission_id;
pub mod routing;

// Re-exports for convenience
pub use bind_envelope::{BindEnvelope, RebindAbortReason, RebindEnvelope, RebindPrepare};
pub use bootstrap::{
    verify_authority, BootstrapMode, SeedAuthorityError, SeedEntry, SeedHealth, SeedListAuthority,
    SeedListEnvelope, SlashedSeedBlacklist, StaleSeed, EPOCH_GOVERNANCE_TAKEOVER,
    MAX_SEED_AGE_EPOCHS,
};
pub use discovery::MissionDiscoveryScope;
pub use error::MonError;
pub use gossip::{
    MissionGossipMessage, MissionGossipScope, MissionPropagationClass, SCOPE_FLAG_ENCRYPTED,
    SCOPE_FLAG_PRIORITY, SCOPE_FLAG_RELIABLE,
};
pub use governance::{GovernanceModel, GovernancePolicy, GovernanceProposal, ProposalState};
pub use keys::MissionKeyHierarchy;
pub use lifecycle::MissionState;
pub use membership::{AdmissionPolicy, MissionNode};
pub use mission_id::{MissionId, MissionType};
pub use nostr_bootstrap::{DotCapabilityClaim, Nip05Error, Nip05Identifier, NostrBootstrapAdapter};
pub use rebind::{CoordinatorState, PrepareVote, RebindCoordinator, REBIND_TIMEOUT_SECS};
pub use reconciliation::{MobilitySession, ReconciliationState, TransportCarrier};
pub use slash::{slash_code, BootstrapMisbehavior, SlashEnvelope};
pub use slash_aggregation::{AggregationResult, RejectionReason, SlashAggregator, SlashVote, Vote, SLASH_VOTE_WINDOW_SECS};
pub use topology::{MissionDescriptor, TopologyModel};
pub use trust_graph::{GraphFormat, TrustEdge, TrustGraph, TrustNode};
