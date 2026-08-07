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
//! Canonical wire form: `DfpEncoding::from_dfp(&&d).to_bytes() -> [u8; 24]`.

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod anchor;
pub mod anchor_job;
pub mod audit;
pub mod auth;
pub mod compat;
pub mod constants;
pub mod cross_layer;
pub mod digest;
pub mod election;
pub mod error;
pub mod gossip;
pub mod migrations;
pub mod parity;
pub mod parity_daemon;
pub mod presentation;
pub mod prometheus;
pub mod reconciler;
pub mod recorder;
pub mod retention;
pub mod retirement;
pub mod slash_api;
pub mod sliding;
pub mod store;
pub mod types;

pub use anchor::{
    exceeds_daily_fanout, is_finality_reached, window_collision, AnchorLeaf, AnchorWindow,
    ReputationAnchorBatch,
};
pub use anchor_job::{
    plan_batches, run_once, run_once_strict, total_fee, AnchorJobConfig, AnchorJobError,
    AnchorJobOutcome, AnchorTupleFanout, ChainAnchorSubmitter, StubChainAnchorSubmitter,
};
pub use audit::{
    audit_commitment, drop_pre_rotation_events, max_rotation_id, replay as audit_replay,
    AuditReplay,
};
pub use auth::{
    governance_set_hash, AnchorGovernanceProof, AnchorGovernanceSigner, AnchorGovernanceSnapshot,
    AnchorSignature, AssetTag, Attestation, AttestorAuth, AttestorId, AttestorRegistration,
    ChainRef, GovernanceProof, GovernanceSnapshot, ReaderAuth, ResumeProof, RetentionAuth,
    SlashDestination, SuspensionAuth,
};
pub use compat::{
    deterministic_f64_mirror, CompatKeymap, CompatMapping, DcRootedSlashReputationStore,
    F64MirrorPolicy, LegacyReputationStore, LegacyShadowError, ReputationStoreCompat,
    SlashReputationStore,
};
pub use cross_layer::{cross_layer_query, dedup_layers, CrossLayerResult, MAX_CROSS_LAYER_FANOUT};
pub use digest::ReputationDigest;
pub use election::{
    apply_per_controller_cap, election_priority_v2, ElectionCandidate, ElectionPriority,
    PerControllerCounts,
};
pub use error::{ReputationError, StakeComponent};
pub use gossip::{
    message_id_for_envelope, topic_for_dc_recorder, topic_for_recorder, GossipCatchUp,
    GossipEnvelope, RateLimitDecision, RateLimitedAttestor, ATTESTOR_RATE_WINDOW_SECS,
    DEFAULT_ATTESTOR_RATE_LIMIT,
};
pub use migrations::{MigrationVersion, BUILTIN_MIGRATIONS};
pub use parity::{
    compute_parity_report, parity_gate_deadline_unix, ParityReport, ParityRow, TripleClass,
    PARITY_GATE_DEADLINE_DAYS, PARITY_THRESHOLD, PER_DID_MISMATCH_DOMINANCE,
};
pub use parity_daemon::{
    parity_deadline_unix_from_epoch, ParityBucket, ParityDaemonState, MIN_BUCKET_TOTAL,
    REQUIRED_CONSECUTIVE_BUCKETS,
};
pub use presentation::reputation_score_0_100;
pub use prometheus::{render_prometheus, write_prometheus_file, MetricsSnapshot};
pub use reconciler::{
    build_replay_event, dfp_from_legacy_f64, dfp_to_canonical_blob, event_id_from_envelope,
    legacy_has_event_source, legacy_lacks_event_source, reconcile_once, ReconcileOutcome,
    ReconcilerConfig,
};
pub use recorder::{check_stake, verify_registration};
pub use retention::{
    effective_cutoff, is_within_audit_window, retention_prune_with_floor, RetentionReport,
    MIN_EVENTS_RETAINED, MIN_RETENTION_WINDOW_SECS,
};
pub use retirement::{
    declare_on, retirement_envelope_hash, stub_verify_proof_shape, validate_evidence,
    ADAPTER_DC_SLASH, ADAPTER_MARKETPLACE, ADAPTER_SLASH, KNOWN_ADAPTERS,
    MIN_RETIREMENT_BUCKET_COUNT, MIN_RETIREMENT_PARITY_SCORE_BP, MIN_RETIREMENT_SIGNATURE_BYTES,
};
pub use slash_api::{issue_governance_slash, MismatchField};
pub use sliding::{effective_window, sliding_window, MAX_SLIDING_WINDOW_SECS};
pub use store::{
    AnchorRecord, InMemoryReputationStore, ReputationStore, StoolapReputationStore, StoreResult,
};
pub use types::{
    ControllerId, EventId, ParityEvidence, RecorderDid, RecorderId, ReputationAggregate,
    ReputationLayer, ReputationPolicy, RetirementEligibility, RotationProvenance, SignalEvent,
    SignalKind,
};
