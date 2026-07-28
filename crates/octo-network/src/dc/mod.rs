//! DomainCoordinator (DC) role (RFC-0855p-c + missions 0855p-c-*).
//!
//! ## Missions in this module
//!
//! - **0855p-c-admin-attestation**: `PlatformAdminAttest` envelope
//!   with freshness check + `ATTEST_CHALLENGE` flow.
//! - **0855p-c-auto-rejoin**: `REJOIN_REQUEST` + `RejoinTicket`
//!   with rate limiting.
//! - **0855p-c-cross-domain-slash**: 0x000F slash reason code
//!   for DC misbehavior with sub-codes + cross-domain reputation
//!   update.
//! - **0855p-c-cross-platform-consensus**: 2PC for
//!   REBIND/UNBIND across N platforms (N=1 unilateral, N=2
//!   unanimous, N≥3 2/3).
//! - **0855p-c-reputation**: `DcRootedSlashReputationStoreCompat`
//!   (DID-keyed, RFC-0968-A1 amendment 29) for cross-domain
//!   reputation. Lives at
//!   `octo_network::reputation::DcRootedSlashReputationStoreCompat`.
//!   The legacy pubkey-keyed `DcRootedSlashReputationStore`
//!   (`octo_network::dc::reputation`) was deleted 2026-07-27
//!   per RFC-0968-A1 amendment 29.
//! - **0855p-c-slash-small-groups**: Slash vs UNBIND for
//!   < 4-member groups with re-strike escalation.
//! - **0855p-c-sub-admins**: Sub-admin designation, authority
//!   policy, activation delay.

pub mod admin_attest;
pub mod consensus;
pub mod discipline;
pub mod rejoin;
pub mod slash;
pub mod sub_admin;

pub use admin_attest::{
    attest_topic, AttestChallenge, PlatformAdminAttest, PlatformAdminAttestError,
    ATTEST_PERIOD_EPOCHS, CHALLENGE_RESPONSE_EPOCHS, MAX_ATTEST_AGE_EPOCHS,
};
pub use consensus::{
    consensus_topic, ConsensusEnvelope, ConsensusOutcome, ConsensusState, ConsensusVote,
    DcConsensusCoordinator, DC_CONSENSUS_TIMEOUT_EPOCHS,
};
pub use discipline::{
    discipline_for, DisciplineAction, DisciplineContext, SuspectState, MIN_GROUP_SIZE_FOR_UNBIND,
};
pub use rejoin::{
    RejoinCooldown, RejoinError, RejoinRequest, RejoinTicket, REJOIN_COOLDOWN_EPOCHS,
    REJOIN_TICKET_VALID_EPOCHS,
};
pub use slash::{
    dc_slash_topic, DcFinalState, DcMisbehavior, DcSlashEnvelope, DcSlashError, DcSlashOutcome,
    DcSlashSubCode, DC_SLASH_REASON_DOMAIN_COORDINATOR_MISBEHAVIOR,
};
pub use sub_admin::{
    elect_active_sub_admin, should_activate_sub_admin, SubAdminAuthority, SubAdminDesignation,
    SubAdminState, SUB_ADMIN_ACTIVATION_EPOCHS,
};
