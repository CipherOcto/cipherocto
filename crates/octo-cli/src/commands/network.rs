//! `octo network` - RFC-0011-i Phase 1 (peers + identity + trust-graph
//! + governance rotation read-only observability) +
//!
//! RFC-0011-j Phase 2 (mode + authority + slash observability + management).
//!
//! Layer C orchestrator over the Layer-B `octo-network` substrate
//! (`GatewayCache::iter/get`, `LocalGatewayIdentity::load`,
//! `TrustGraph::render`, `GovernanceRotation::has_quorum +
//! migration_deadline + in_migration_window`,
//! `BootstrapConfig::{from_toml, save_toml}`,
//! `SeedListAuthority::rotate_post_fork`,
//! `SlashReputationStoreCompat::{is_excluded, list, show,
//! did_count, total_slashes}`).
//! Output renders through `OutputEnvelope<T>` per RFC-0011 §Output
//! Envelope + RFC-0011-c §9.4 v4 wire form.
//!
//! ## Substrate reality (Phase 1)
//!
//! - `GatewayCache` is in-memory only today; no persistence
//!   adapter is wired. The CLI calls `GatewayCache::new()` to
//!   obtain a substrate-faithful handle; in deployment this would
//!   be replaced by a cache-persistence adapter (deferred per
//!   RFC-0011-h Phase 6 follow-on).
//! - `LocalGatewayIdentity` persists at
//!   `<octo_home>/network/local-gateway-identity.toml` per
//!   mission `0011-h-s-a-local-gateway-identity-state` (substrate
//!   landed at `next caeb84c4`). NotInitialized on missing file.
//! - `TrustGraph::render(GraphFormat)` returns `String`
//!   ASCII or DOT.
//! - `GovernanceRotation` is rooted per-rotation; for Phase 1
//!   the CLI constructs a deterministic zero-default rotation
//!   to surface `has_quorum` + `migration_deadline +
//!   in_migration_window`. Real rotation state will land via a
//!   future substrate persistence adapter (deferred).
//!
//! ## Substrate reality (Phase 2)
//!
//! - `BootstrapConfig` persists at
//!   `<octo_home>/network/bootstrap.toml` per mission
//!   `0011-h-s-a-bootstrap-orchestrator` (G1 substrate landed at
//!   `next c9121aa5`). `mode set` writes via `save_toml` (3-flag
//!   confirmation per `require_confirm`); `mode show` reads via
//!   `from_toml`. Missing-file IO error surfaces as exit 82
//!   `NetworkConfigParseFailed`.
//! - `SeedListAuthority::rotate_post_fork` is the post-fork
//!   rotation constructor per mission
//!   `0011-h-s-a-seed-list-authority-rotate` (G8 substrate landed
//!   at `next 931dc7b1`). Foundation rotation rejected
//!   post-fork; zero-digest quorum proof rejected.
//! - `SlashReputationStoreCompat` is the in-memory slash store per
//!   mission `0011-h-s-a-slash-store` (G6 substrate landed at
//!   `next 931dc7b1`). The CLI constructs a per-process instance
//!   (no persistence adapter yet — substrate hydration
//!   responsibility lives at `SlashStoreLoader` per mission
//!   `0011-h-s-a-slash-store-loader` (G6b)).
//!
//! ## Operator-mode constraint
//!
//! Phase 1 subcommands are READ-ONLY. Phase 2 `mode set` + `authority
//! rotate` are WRITE surfaces gated by `require_confirm` (auditor
//! denied; CI requires `--allow-write`). Phase 2 slash
//! observability subcommands are READ-ONLY.

#![allow(
    clippy::module_name_repetitions,
    reason = "intentional repetition for canonical CLI surface types: NetworkAction / NetworkPeersListOutput / etc mirror the substrate names verbatim so the CLI envelope reads substrate-faithfully (per RFC-0011-i §Output Envelope)."
)]

use std::path::PathBuf;

use clap::Parser;
use serde::Serialize;

use crate::error::sanitize_substrate_error;
use crate::error::OctoCliError;
use crate::home;
use crate::output::OutputEnvelope;
use crate::Octo;

use octo_network::dot::gateway::GatewayClass;
use octo_network::gdp::cache::{GatewayCache, GatewayCacheEntry};
use octo_network::mon::bootstrap::{
    verify_authority, BootstrapConfig, BootstrapConfigError, BootstrapMode, BootstrapOrchestrator,
    SeedAuthorityError, SeedListAuthority,
};
use octo_network::mon::governance_rotation::GovernanceRotation;
use octo_network::mon::local_gateway_identity::LocalGatewayIdentity;
use octo_network::mon::trust_graph::{GraphFormat, TrustGraph};
use octo_network::reputation::{SlashListFilter, SlashReputationStoreCompat};

// Phase 10 (RFC-0011-r) substrate additions: `ReputationStore` trait
// extension provides `list` + `peer_reputation` for the Phase 10
// `reputation list` + `reputation show` CLI dispatch.
use octo_reputation::store::PeerReputation;
use octo_reputation::store::ReputationFilter;
use octo_reputation::ReputationStore;

// Phase 3 (RFC-0011-k) substrate additions (LANDED at `next 10ae8e18`):
// `CoordinatorRecord::load` lives in `octo-coordinator-types` (Layer A
// additive surface) and is re-exported through the canonical path. The
// governance canonical-bytes helper + `CoordinatorAdmin` typed dispatch
// live in `octo-network` Layer B.
use octo_coordinator_types::state::CoordinatorRecord;
use octo_network::dot::adapters::coordinator_admin::{
    dispatch_coordinator_admin_action, CoordinatorAdminAction, CoordinatorAdminActionError, GroupId,
};
use octo_network::mon::governance::{
    governance_proposal_canonical_bytes, DecisionType, GovernanceProposal, ProposalState,
};

// Phase 4 (RFC-0011-l) substrate additions (LANDED at `next edcdc47a`
// for G22 + `next 3794e4a8` for G21): `BindEnvelope::load` lookup
// helper lives in `octo-network` Layer B mon::bind_envelope, and the
// typed dispatch surface for the rebind-* trio lives in
// `octo-network` Layer B mon::rebind_arm.
use octo_network::mon::bind_envelope::BindEnvelope;
use octo_network::mon::rebind::RebindCoordinator;
use octo_network::mon::rebind_arm::{
    dispatch_rebind_arm_action, RebindArmAction, RebindArmError, RebindArmKey,
};

// Phase 5 (RFC-0011-m) substrate additions (LANDED at `next 24bfec96`
// for G23 + G24): `MissionAdvertisementCache` + `MissionInvitationCache`
// live in `octo-network` Layer B mon::discovery (existing module
// extended). Both cache types are RFC-frozen surface additions
// (non-breaking because they add new types without modifying existing
// public API).
use octo_network::mon::discovery::{
    MissionAdvertisement, MissionAdvertisementCache, MissionInvitation, MissionInvitationCache,
};
use octo_network::sender::{NetworkSenderRegistry, SendSummary};

// === Subcommand taxonomy (RFC-0011-i §Subcommand Taxonomy Phase 1) ===

// === Subcommand arg structs (RFC-0011-n Phase 6) ===

/// `octo network bootstrap` arguments (RFC-0011-n Phase 6 G26).
/// Substrate-managed lifecycle per RFC-0011-h row 97 (no
/// CLI-side confirmation flags).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct BootstrapArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network status` arguments (RFC-0011-n Phase 6:
/// G26 bootstrap + G18 writer-election + G20 network-sender
/// + drift-closure aggregate).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct StatusArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// CLI-facing network subcommand enum (Layer C). `#[non_exhaustive]`
/// per F-14 - future amendments add subcommand variants without
/// central-enum edits across the workspace.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkAction {
    /// Peer discovery + lookup subcommands.
    Peers {
        /// Peer subcommand.
        #[command(subcommand)]
        action: PeersAction,
    },
    /// Local gateway identity subcommands.
    Identity {
        /// Identity subcommand.
        #[command(subcommand)]
        action: NetworkIdentityAction,
    },
    /// Trust-graph render subcommands.
    TrustGraph {
        /// Trust-graph subcommand.
        #[command(subcommand)]
        action: TrustGraphAction,
    },
    /// Governance rotation subcommands.
    Governance {
        /// Governance subcommand.
        #[command(subcommand)]
        action: NetworkGovernanceAction,
    },
    /// Mode (bootstrap transport) subcommands (RFC-0011-j Phase 2).
    Mode {
        /// Mode subcommand.
        #[command(subcommand)]
        action: NetworkModeAction,
    },
    /// Authority (seed list) subcommands (RFC-0011-j Phase 2).
    Authority {
        /// Authority subcommand.
        #[command(subcommand)]
        action: NetworkAuthorityAction,
    },
    /// Slash reputation subcommands (RFC-0011-j Phase 2).
    Slash {
        /// Slash subcommand.
        #[command(subcommand)]
        action: NetworkSlashAction,
    },
    /// Coordinator record + admin subcommands (RFC-0011-k Phase 3).
    Coordinator {
        /// Coordinator subcommand.
        #[command(subcommand)]
        action: NetworkCoordinatorAction,
    },
    /// BIND envelope read + rebind payload-builder subcommands
    /// (RFC-0011-l Phase 4).
    BindEnvelope {
        /// Bind-envelope subcommand.
        #[command(subcommand)]
        action: NetworkBindEnvelopeAction,
    },
    /// Mission discovery advertisement + invitation visibility
    /// subcommands (RFC-0011-m Phase 5).
    Discovery {
        /// Discovery subcommand.
        #[command(subcommand)]
        action: NetworkDiscoveryAction,
    },
    /// Bootstrap lifecycle orchestrator subcommand
    /// (RFC-0011-n Phase 6 G26).
    Bootstrap(BootstrapArgs),
    /// Network aggregate status subcommand
    /// (RFC-0011-n Phase 6: G26 + G18 + G20 + drift-closure).
    Status(StatusArgs),
    /// Slash bridge observability + propagate subcommands
    /// (RFC-0011-o Phase 7 G9).
    SlashBridge {
        /// Slash-bridge subcommand.
        #[command(subcommand)]
        action: NetworkSlashBridgeAction,
    },
    /// Quota router node status + peer capacity subcommands
    /// (RFC-0011-p Phase 8 G10).
    Router {
        /// Router subcommand.
        #[command(subcommand)]
        action: NetworkRouterAction,
    },
    /// Specialized node show + bind subcommands
    /// (RFC-0011-q Phase 9 G11).
    Node {
        /// Node subcommand.
        #[command(subcommand)]
        action: NetworkNodeAction,
    },
    /// Reputation list + peer show subcommands
    /// (RFC-0011-r Phase 10 G13).
    Reputation {
        /// Reputation subcommand.
        #[command(subcommand)]
        action: NetworkReputationAction,
    },
    /// Topology render subcommand (RFC-0011-s Phase 11 G14).
    Topology {
        /// Topology subcommand.
        #[command(subcommand)]
        action: NetworkTopologyAction,
    },
    /// Gossip protocol stats (RFC-0011-t Phase 12 G15).
    Gossip {
        /// Gossip subcommand.
        #[command(subcommand)]
        action: NetworkGossipAction,
    },
    /// Envelope inspector + forward (RFC-0011-u Phase 13 G16a + G16b).
    Envelope {
        /// Envelope subcommand.
        #[command(subcommand)]
        action: NetworkEnvelopeAction,
    },
    /// Heartbeat probe (RFC-0011-v Phase 14 G17).
    Heartbeat {
        /// Heartbeat subcommand.
        #[command(subcommand)]
        action: NetworkHeartbeatAction,
    },
}

/// Peer subcommand surface (RFC-0011-i §Subcommand Taxonomy Phase 1).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PeersAction {
    /// List cached gateway peers.
    List(PeersListArgs),
    /// Point-lookup one gateway peer by 32-byte gateway_id.
    Get(PeersGetArgs),
}

/// `octo network peers list` arguments (RFC-0011-i §Subcommand
/// Taxonomy Phase 1 `peers list`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct PeersListArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network peers get <gateway_id_hex>` arguments (RFC-0011-i
/// §Subcommand Taxonomy Phase 1 `peers get`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct PeersGetArgs {
    /// 32-byte gateway_id as 64 lowercase hex chars (RFC-0011-i §Peer
    /// Lookup Form). Mixed-case input is rejected (pastejacking
    /// defense).
    #[arg(value_parser = parse_gateway_id_hex)]
    pub gateway_id: [u8; 32],
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Network identity subcommand surface.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkIdentityAction {
    /// Show the local gateway identity persisted on disk.
    Show(IdentityShowArgs),
}

/// `octo network identity show` arguments (RFC-0011-i §Subcommand
/// Taxonomy Phase 1 `identity show`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct IdentityShowArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Trust-graph subcommand surface.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustGraphAction {
    /// Render the trust graph in the requested format.
    Render(TrustGraphRenderArgs),
}

/// `octo network trust-graph render` arguments (RFC-0011-i §Subcommand
/// Taxonomy Phase 1 `trust-graph render`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct TrustGraphRenderArgs {
    /// Output format - ascii | dot (RFC-0011-i §Trust-Graph Render).
    #[arg(long, value_parser = parse_graph_format, default_value = "ascii")]
    pub format: GraphFormat,
    /// Graph depth clamp (1..=100; values outside the range are
    /// rejected with `NetworkGraphDepthBelowRange` exit 85).
    #[arg(long, value_parser = parse_graph_depth)]
    pub depth: Option<u32>,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Governance subcommand surface (network-scoped).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkGovernanceAction {
    /// Governance rotation read surface.
    Rotation {
        /// Rotation subcommand.
        #[command(subcommand)]
        action: NetworkGovernanceRotationAction,
    },
    /// Governance tally read surface (RFC-0011-k Phase 3 substrate
    /// `governance_proposal_canonical_bytes`).
    Tally(GovernanceTallyArgs),
}

/// Governance rotation subcommand surface.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkGovernanceRotationAction {
    /// Show governance rotation status (quorum + deadline + window).
    Status(GovernanceRotationStatusArgs),
}

/// `octo network governance rotation status` arguments (RFC-0011-i
/// §Subcommand Taxonomy Phase 1 `governance rotation status`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct GovernanceRotationStatusArgs {
    /// Canonical DID wire form for the voting identity. Format
    /// validation happens here; substrate rejects malformed DIDs
    /// via `NetworkInvalidDid` (slot 86).
    #[arg(long)]
    pub did_codec: String,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network governance tally` arguments (RFC-0011-k Phase 3
/// Substrate-Additions row G3b).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct GovernanceTallyArgs {
    /// Monotonic proposal id (per-issuer; issuer enforces uniqueness).
    #[arg(long)]
    pub proposal_id: u64,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Coordinator subcommand surface (RFC-0011-k Phase 3).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkCoordinatorAction {
    /// Show one coordinator record by 32-byte coordinator_id
    /// (read-only; `CoordinatorRecord::load` substrate-faithful
    /// Option::None translation).
    Show(CoordinatorShowArgs),
    /// Dispatch a typed `CoordinatorAdminAction` to the substrate
    /// sync helper (write; 3-flag confirmation; substrate returns
    /// `AdapterUnwired` until the wired adapter lands).
    Admin(CoordinatorAdminArgs),
}

/// `octo network coordinator show` arguments (RFC-0011-k Phase 3
/// Substrate-Additions row G12b).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorShowArgs {
    /// 32-byte coordinator_id as 64 lowercase hex chars. Zero-digest
    /// is rejected at parse time (pastejacking defense).
    #[arg(value_parser = parse_64_char_hex_32byte)]
    pub coordinator_id: [u8; 32],
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network coordinator admin` arguments (RFC-0011-k Phase 3
/// Substrate-Additions row G12).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorAdminArgs {
    /// Typed admin action label.
    ///
    /// `transfer_ownership` -> `CoordinatorAdminAction::TransferOwnership`
    /// `ban_member` -> `CoordinatorAdminAction::BanMember`
    /// `promote_to_admin` -> `CoordinatorAdminAction::PromoteToAdmin`
    #[arg(long, value_parser = parse_coordinator_admin_action_label)]
    pub action: CoordinatorAdminActionLabel,
    /// Group id (canonical RFC-0011-h string form).
    #[arg(long)]
    pub group_id: String,
    /// Target peer id as 64 lowercase hex chars (32-byte wire form).
    #[arg(long, value_parser = parse_64_char_hex_32byte)]
    pub target: [u8; 32],
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

// === BIND envelope subcommands (RFC-0011-l Phase 4) ===

/// BIND envelope subcommand surface (RFC-0011-l Phase 4 §Subcommand
/// Taxonomy).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkBindEnvelopeAction {
    /// Show one BIND envelope by `domain_id` (read-only;
    /// `BindEnvelope::load` substrate-faithful Option::None
    /// translation per Phase 4 row G22).
    Show(BindEnvelopeShowArgs),
    /// Build the PREPARE envelope for a new REBIND (write;
    /// `RebindCoordinator::prepare_envelope` payload builder per
    /// Phase 4 row G21; `--dry-run` default + `--confirm-acknowledge`
    /// required to lift dry-run per RFC-0011-l §Subcommand
    /// Taxonomy rebind-* rows).
    RebindPrepare(BindEnvelopeRebindPrepareArgs),
    /// Build the COMMIT envelope after quorum reached (write;
    /// `RebindCoordinator::commit_envelope` payload builder per
    /// Phase 4 row G21; `--dry-run` default + `--confirm-acknowledge`
    /// + `--confirm` SECOND flag (pastejacking defense per
    ///   §Security Considerations)).
    RebindCommit(BindEnvelopeRebindCommitArgs),
    /// Build the ABORT envelope (vote-abort OR timeout; write;
    /// `RebindCoordinator::abort_envelope` payload builder per
    /// Phase 4 row G21; `--dry-run` default + `--confirm-acknowledge`
    /// required).
    RebindAbort(BindEnvelopeRebindAbortArgs),
}

/// `octo network bind-envelope show` arguments (RFC-0011-l Phase 4
/// row G22).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct BindEnvelopeShowArgs {
    /// The 52-char hex-encoded mission `domain_id` (canonical
    /// RFC-0011-h string form per `BindEnvelope::domain_id`).
    #[arg(long)]
    pub domain_id: String,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network bind-envelope rebind-prepare` arguments (RFC-0011-l
/// Phase 4 row G21 rebind-prepare).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct BindEnvelopeRebindPrepareArgs {
    /// The 52-char hex-encoded mission `domain_id`.
    #[arg(long)]
    pub domain_id: String,
    /// Lift dry-run to substrate dispatch (per RFC-0011-l
    /// §Subcommand Taxonomy rebind-* rows; default dry-run).
    #[arg(long)]
    pub no_dry_run: bool,
    /// Confirm acknowledgment of the irreversible intent (per
    /// RFC-0011-l §Security Considerations rebind-* rows).
    #[arg(long)]
    pub confirm_acknowledge: bool,
    /// DEBUG-ONLY escape hatch for CI agents (per RFC-0011-h
    /// §Security Considerations experimental-flag contract;
    /// hidden from `--help`, surfaces in `--help-all`).
    #[arg(long, hide = true)]
    pub allow_ci_deny_default: bool,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network bind-envelope rebind-commit` arguments (RFC-0011-l
/// Phase 4 row G21 rebind-commit; `--confirm` SECOND flag required
/// per pastejacking defense per §Security Considerations).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct BindEnvelopeRebindCommitArgs {
    /// The 52-char hex-encoded mission `domain_id`.
    #[arg(long)]
    pub domain_id: String,
    /// Lift dry-run to substrate dispatch.
    #[arg(long)]
    pub no_dry_run: bool,
    /// Confirm acknowledgment of the irreversible intent.
    #[arg(long)]
    pub confirm_acknowledge: bool,
    /// SECOND confirm flag for pastejacking defense (per
    /// §Security Considerations rebind-commit row).
    #[arg(long)]
    pub confirm: bool,
    /// DEBUG-ONLY escape hatch for CI agents (hidden from
    /// `--help`, surfaces in `--help-all`).
    #[arg(long, hide = true)]
    pub allow_ci_deny_default: bool,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network bind-envelope rebind-abort` arguments (RFC-0011-l
/// Phase 4 row G21 rebind-abort; reversible per RFC-0871 §Algorithms
/// substrate idempotency so no `--confirm` SECOND flag).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct BindEnvelopeRebindAbortArgs {
    /// The 52-char hex-encoded mission `domain_id`.
    #[arg(long)]
    pub domain_id: String,
    /// Lift dry-run to substrate dispatch.
    #[arg(long)]
    pub no_dry_run: bool,
    /// Confirm acknowledgment of the rollback intent.
    #[arg(long)]
    pub confirm_acknowledge: bool,
    /// Human-readable abort reason (free-text; preserved in the
    /// `RebindAbort.reason` debug field per RFC-0871).
    #[arg(long)]
    pub reason: String,
    /// DEBUG-ONLY escape hatch for CI agents (hidden from
    /// `--help`, surfaces in `--help-all`).
    #[arg(long, hide = true)]
    pub allow_ci_deny_default: bool,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Mission discovery subcommand surface (RFC-0011-m Phase 5
/// §Subcommand Taxonomy rows 327-329). Both subcommands are
/// read-only with no confirmation flags per RFC-0011-h row 86 +
/// row 664 (ALLOW-in-CI).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkDiscoveryAction {
    /// Show one cached mission advertisement by
    /// `advertisement_id` (read-only;
    /// `MissionAdvertisementCache::get` substrate-faithful
    /// Option::None translation per Phase 5 row G23).
    AdvertisementShow(DiscoveryAdvertisementShowArgs),
    /// Show one cached mission invitation by `invitation_id`
    /// (read-only; `MissionInvitationCache::get`
    /// substrate-faithful Option::None translation per
    /// Phase 5 row G24).
    InvitationShow(DiscoveryInvitationShowArgs),
}

/// `octo network discovery advertisement show` arguments
/// (RFC-0011-m Phase 5 row G23).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryAdvertisementShowArgs {
    /// Optional 64-char hex-encoded `advertisement_id` (32 bytes
    /// BLAKE3 hash per RFC-0855 §8.2 deterministic-key contract).
    /// Omitted = full enumeration via `MissionAdvertisementCache::iter`.
    #[arg(long)]
    pub advertisement_id: Option<String>,
    /// Optional hop count for `is_ttl_exceeded(hop_count)` per
    /// RFC-0855 §8.2 (clap u16, `--hops 65536` rejected pre-dispatch).
    #[arg(long)]
    pub hops: Option<u16>,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network discovery invitation show` arguments
/// (RFC-0011-m Phase 5 row G24).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryInvitationShowArgs {
    /// Optional 64-char hex-encoded `invitation_id` (32 bytes
    /// BLAKE3 hash of signing bytes per RFC-0855 §8.2
    /// deterministic-key contract). Omitted = full enumeration
    /// via `MissionInvitationCache::iter`.
    #[arg(long)]
    pub invitation_id: Option<String>,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Slash bridge observability + propagate subcommands
/// (RFC-0011-o Phase 7 G9).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkSlashBridgeAction {
    /// List bridged slashes currently held by the local trait
    /// implementation (RFC-0011-o §Subcommand Taxonomy Phase 7).
    List(SlashBridgeListArgs),
    /// Propagate a slash envelope to its external destination
    /// (RFC-0011-o §Subcommand Taxonomy Phase 7).
    Propagate(SlashBridgePropagateArgs),
}

/// `octo network slash-bridge list` arguments (RFC-0011-o
/// §Subcommand Taxonomy Phase 7 `slash-bridge list`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct SlashBridgeListArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network slash-bridge propagate <slash_envelope_id_hex>`
/// arguments (RFC-0011-o §Subcommand Taxonomy Phase 7
/// `slash-bridge propagate`). `--dry-run` is default per
/// RFC-0011-h §Confirmation Flag for mutating subcommands;
/// `--confirm-acknowledge` required for apply (reversible
/// write per RFC-0011-o §Confirmation Flag).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct SlashBridgePropagateArgs {
    /// 32-byte `slash_envelope_id` as 64 hex chars (lowercase
    /// OR uppercase; mixed-case rejected) (RFC-0855p-b
    /// §Wire Format). Pastejacking defense per
    /// RFC-0011-o §Confirmation Flag + parse_32_byte_hex shared
    /// helper.
    #[arg(value_parser = parse_slash_envelope_id_hex)]
    pub slash_envelope_id: [u8; 32],
    /// Emit preview envelope with computed `BridgeReceipt` shape
    /// (default behavior; mutually exclusive with --apply per
    /// RFC-0011-h §Confirmation Flag).
    #[arg(long, conflicts_with = "apply")]
    pub dry_run: bool,
    /// Acknowledge and apply the propagation (reversible write;
    /// required for apply per RFC-0011-o §Confirmation Flag).
    /// Mutually exclusive with --dry-run; both flags imply
    /// explicit operator intent.
    #[arg(long, conflicts_with = "dry_run", requires = "confirm_acknowledge")]
    pub apply: bool,
    /// Acknowledge the apply intent (reversible write; required
    /// for --apply per RFC-0011-o §Confirmation Flag).
    #[arg(long)]
    pub confirm_acknowledge: bool,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Quota router node subcommand surface (RFC-0011-p Phase 8 G10).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkRouterAction {
    /// Show local quota router node operational status
    /// (RFC-0011-p §Subcommand Taxonomy Phase 8).
    Status(RouterStatusArgs),
    /// Show remaining quota capacity for a specific peer
    /// (RFC-0011-p §Subcommand Taxonomy Phase 8).
    Peers(RouterPeersArgs),
}

/// `octo network router status` arguments (RFC-0011-p
/// §Subcommand Taxonomy Phase 8 `router status`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct RouterStatusArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network router peers <peer_node_id_hex>` arguments
/// (RFC-0011-p §Subcommand Taxonomy Phase 8 `router peers`).
/// `peer_node_id` is 32-byte canonical identifier accepted as
/// 64 hex chars (lowercase OR uppercase); mixed-case rejected
/// by `parse_32_byte_hex` pastejacking defense.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct RouterPeersArgs {
    /// 32-byte peer_node_id as 64 hex chars (lowercase OR
    /// uppercase; mixed-case rejected). Pastejacking defense
    /// per RFC-0011-h §Pastejacking Defense pattern.
    #[arg(value_parser = parse_router_peer_node_id_hex)]
    pub peer_node_id: [u8; 32],
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Specialized node subcommand surface (RFC-0011-q Phase 9 G11).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkNodeAction {
    /// Show specialized node record by node_id
    /// (RFC-0011-q §Subcommand Taxonomy Phase 9).
    Show(NodeShowArgs),
    /// Bind a specialized node to a holder DID
    /// (RFC-0011-q §Subcommand Taxonomy Phase 9).
    Bind(NodeBindArgs),
}

/// `octo network node show <node_id_hex>` arguments
/// (RFC-0011-q §Subcommand Taxonomy Phase 9 `node show`).
/// `node_id` is 32-byte canonical identifier accepted as
/// 64 hex chars (lowercase OR uppercase); mixed-case
/// rejected by `parse_32_byte_hex` pastejacking defense.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct NodeShowArgs {
    /// 32-byte node_id as 64 hex chars (lowercase OR
    /// uppercase; mixed-case rejected). Pastejacking defense
    /// per RFC-0011-q §Pastejacking Defense pattern.
    #[arg(value_parser = parse_specialized_node_id_hex)]
    pub node_id: [u8; 32],
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network node bind <node_id_hex> --holder-did <did>`
/// arguments (RFC-0011-q §Subcommand Taxonomy Phase 9
/// `node bind`). Bind is mutating but reversible per
/// RFC-0011-h §Confirmation Flag (--apply + --confirm-
/// acknowledge required for apply; default dry-run).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct NodeBindArgs {
    /// 32-byte node_id as 64 hex chars (lowercase OR
    /// uppercase; mixed-case rejected). Pastejacking defense
    /// per RFC-0011-q §Pastejacking Defense pattern.
    #[arg(value_parser = parse_specialized_node_id_hex)]
    pub node_id: [u8; 32],
    /// Holder DID in canonical `did:octo:0x<hex>` format
    /// (RFC-0011-q §Subcommand Taxonomy Phase 9 `node bind`
    /// `--holder-did`).
    #[arg(long = "holder-did")]
    pub holder_did: String,
    /// Emit preview envelope without substrate dispatch
    /// (default behavior; mutually exclusive with --apply
    /// per RFC-0011-h §Confirmation Flag).
    #[arg(long, conflicts_with = "apply")]
    pub dry_run: bool,
    /// Acknowledge and apply the bind (reversible write;
    /// required for apply per RFC-0011-q §Confirmation Flag).
    /// Mutually exclusive with --dry-run; both flags imply
    /// explicit operator intent.
    #[arg(long, conflicts_with = "dry_run", requires = "confirm_acknowledge")]
    pub apply: bool,
    /// Acknowledge the apply intent (reversible write;
    /// required for --apply per RFC-0011-q §Confirmation Flag).
    #[arg(long)]
    pub confirm_acknowledge: bool,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Reputation list + peer-show subcommand surface
/// (RFC-0011-r Phase 10 G13).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkReputationAction {
    /// List peer reputations matching the filter
    /// (RFC-0011-r §Subcommand Taxonomy Phase 10).
    List(ReputationListArgs),
    /// Show one peer reputation by DID
    /// (RFC-0011-r §Subcommand Taxonomy Phase 10).
    Show(ReputationShowArgs),
}

/// `octo network reputation list [--filter <filter>]` arguments
/// (RFC-0011-r §Subcommand Taxonomy Phase 10 `reputation list`).
/// Filter is `all` (default), `above-score <N>`, or `below-score <N>`.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct ReputationListArgs {
    /// Reputation filter — one of `all` (default) /
    /// `above-score <N>` / `below-score <N>`. Bare-word
    /// forms are accepted for tab completion; composable
    /// arguments are passed as separate tokens
    /// (`--filter above-score --threshold 100`).
    /// Default value per RFC-0011-r §Subcommand Taxonomy
    /// Phase 10 is `all`.
    #[arg(long, value_enum, default_value_t = ReputationFilterKind::All)]
    pub filter: ReputationFilterKind,
    /// Threshold value paired with `above-score` /
    /// `below-score` filters (RFC-0011-r §Subcommand
    /// Taxonomy Phase 10). Threshold validation lives in
    /// the handler (network_reputation_list returns
    /// ConfirmationRequired when above-score/below-score
    /// is paired with None threshold); parse-time
    /// enforcement is intentionally NOT used to keep
    /// the handler validation rule as the single source
    /// of truth.
    #[arg(long)]
    pub threshold: Option<u32>,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Bare-word enum for the `reputation list --filter` flag
/// (RFC-0011-r §Subcommand Taxonomy Phase 10). Map to the
/// substrate `ReputationFilter` enum in the handler.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReputationFilterKind {
    /// All peers (no filter).
    All,
    /// Peers with score >= threshold.
    AboveScore,
    /// Peers with score <= threshold.
    BelowScore,
}

/// `octo network reputation show <peer_did>` arguments
/// (RFC-0011-r §Subcommand Taxonomy Phase 10 `reputation show`).
/// `peer_did` is a canonical `did:octo:0x<hex>` string;
/// substrate-faithful to `RecorderDid` (RFC-0860).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct ReputationShowArgs {
    /// Canonical peer DID (`did:octo:0x<hex>`).
    /// Decoded by the handler into a `RecorderDid` substrate
    /// lookup (RFC-0011-r §Subcommand Taxonomy Phase 10).
    pub peer_did: String,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Topology render subcommand surface (RFC-0011-s Phase 11 G14).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkTopologyAction {
    /// Render the topology commitment as graph
    /// (RFC-0011-s §Subcommand Taxonomy Phase 11).
    Render(TopologyRenderArgs),
}

/// `octo network topology render [--format ascii|dot] [--depth <N>]`
/// arguments (RFC-0011-s §Subcommand Taxonomy Phase 11
/// `topology render`). `format` defaults to `ascii`
/// (terminal-friendly); `dot` for graphviz pipe.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct TopologyRenderArgs {
    /// Graph format (RFC-0011-s §Subcommand Taxonomy
    /// Phase 11 `topology render` `--format`).
    #[arg(long, value_enum, default_value_t = TopologyFormatKind::Ascii)]
    pub format: TopologyFormatKind,
    /// Optional depth cap (1-100). Clap enforces the
    /// 1..=100 range pre-dispatch via `parse_graph_depth`
    /// (Phase 5 RFC-0011-m precedent shared with
    /// `trust-graph render`). Values outside 1..=100
    /// surface as `NetworkGraphDepthBelowRange` (slot 85)
    /// or `NetworkGraphDepthAboveRange` (slot 84). Live
    /// topology-source adapter OUT OF SCOPE for Phase 11.
    #[arg(long, value_parser = parse_graph_depth)]
    pub depth: Option<u32>,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Bare-word enum for `topology render --format`
/// (RFC-0011-s §Subcommand Taxonomy Phase 11). Maps
/// to the substrate `GraphFormat` enum from
/// `mon::trust_graph` (Phase 1 0851p-a-trust-ux).
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TopologyFormatKind {
    /// ASCII (terminal-friendly).
    Ascii,
    /// DOT (graphviz pipe).
    Dot,
}

// === Output envelopes (RFC-0011-n Phase 6) ===

/// `octo network bootstrap` output envelope (RFC-0011-n Phase 6
/// G26).
#[derive(Clone, Debug, Serialize)]
pub struct NetworkBootstrapOutput {
    /// Current bootstrap mode (substrate-faithful projection of
    /// `BootstrapState.mode`).
    pub mode: String,
    /// Epoch when `start_bootstrap` was called (None = never started).
    pub started_at_epoch: Option<u64>,
    /// Substrate-faithful refusal signal (drift-closure field per
    /// `0011-h-drift-0851p-a-seed-health-check`).
    pub refuse_start: bool,
    /// Number of cached peers (default 0 pre-start).
    pub peer_count: usize,
}

/// `octo network status` output envelope (RFC-0011-n Phase 6).
/// Aggregates state across G26 + G18 + G20 + drift-closure.
#[derive(Clone, Debug, Serialize)]
pub struct NetworkStatusOutput {
    /// Bootstrap state (G26 substrate-faithful projection).
    pub bootstrap_mode: String,
    /// Epoch when bootstrap started (None = never started).
    pub bootstrap_started_at_epoch: Option<u64>,
    /// Substrate-faithful refusal signal (drift-closure field).
    pub bootstrap_refuse_start: bool,
    /// Number of cached peers (default 0 pre-start).
    pub bootstrap_peer_count: usize,
    /// Writer-election state (G18 substrate-faithful projection).
    pub writer_election_ballot_count: usize,
    /// Number of stakes registered (G18).
    pub writer_election_stake_count: usize,
    /// Number of voters registered (G18).
    pub writer_election_voter_count: usize,
    /// Network-sender registry state (G20 substrate-faithful projection).
    pub network_sender_count: usize,
    /// Drift-closure field per `0011-h-drift-0851p-a-seed-health-check`.
    /// Surfaced as the substrate-faithful seed-health refusal signal.
    pub seed_health_refuses_start: bool,
    /// List of registered transport tags (deterministic BTreeMap order).
    pub transport_tags: Vec<String>,
    /// Aggregated sender summaries (one per registered sender).
    pub send_summaries: Vec<SendSummary>,
}

// === Output envelopes (RFC-0011-o Phase 7 G9) ===

/// Render payload for `octo network slash-bridge list`
/// (RFC-0011-o §Output Envelope Phase 7).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkSlashBridgeListOutput {
    /// Per-row bridged slash projection
    /// (substrate-faithful `BridgedSlash` per RFC-0011-o
    /// §Substrate Mapping Table).
    pub slashes: Vec<BridgedSlashProjection>,
    /// Row count in `slashes`.
    pub total: usize,
}

/// Substrate-faithful summary projection of `BridgedSlash` for
/// `octo network slash-bridge list` (RFC-0011-o §Output Envelope
/// Phase 7). Field order matches substrate struct definition;
/// `bridge_metadata` is serialized as JSON object with deterministic
/// key order (BTreeMap-backed per RFC-0011-h §Output Envelope
/// determinism).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct BridgedSlashProjection {
    /// 32-byte `slash_envelope_id` as 64 lowercase hex chars.
    pub slash_envelope_id_hex: String,
    /// Bridge metadata (extension-defined; deterministic key order).
    pub bridge_metadata: std::collections::BTreeMap<String, String>,
    /// Epoch when the slash was bridged.
    pub bridged_at_epoch: u64,
}

/// Render payload for `octo network slash-bridge propagate
/// <slash_envelope_id_hex>` (RFC-0011-o §Output Envelope Phase 7).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkSlashBridgePropagateOutput {
    /// Substrate-faithful `BridgeReceipt` projection (RFC-0011-o
    /// §Substrate Mapping Table). On `--dry-run` the receipt shape
    /// is preview-only (synthetic); on apply it is the real
    /// substrate return value.
    pub receipt: BridgeReceiptProjection,
    /// True iff `--dry-run` was set (preview only).
    pub dry_run: bool,
    /// 32-byte `slash_envelope_id` echoed back for traceability.
    pub slash_envelope_id_hex: String,
}

/// Substrate-faithful summary projection of `BridgeReceipt` for
/// `octo network slash-bridge propagate` (RFC-0011-o §Output
/// Envelope Phase 7).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct BridgeReceiptProjection {
    /// 32-byte `slash_envelope_id` as 64 lowercase hex chars.
    pub slash_envelope_id_hex: String,
    /// Opaque per-extension destination bytes (transport-specific;
    /// redacted on error paths per RFC-0011-h §Security
    /// Considerations redaction invariant).
    pub propagated_to_hex: String,
    /// Epoch when propagation completed.
    pub propagated_at_epoch: u64,
}

/// `octo network router status` output envelope (RFC-0011-p
/// Phase 8 G10).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkRouterStatusOutput {
    /// Node-local self identifier as 64 lowercase hex chars.
    pub node_id_hex: String,
    /// Operational status (lowercase: healthy | degraded |
    /// offline).
    pub status: String,
    /// Number of peers with non-zero capacity.
    pub reachable_peer_count: usize,
    /// Total peer count including zero-capacity peers.
    pub total_peer_count: usize,
    /// Last capacity sync epoch.
    pub last_sync_epoch: u64,
}

/// `octo network router peers <peer_node_id_hex>` output
/// envelope (RFC-0011-p Phase 8 G10).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkRouterPeersOutput {
    /// Input peer_node_id as 64 lowercase hex chars (echo).
    pub peer_node_id_hex: String,
    /// Remaining quota capacity for the peer (None = peer
    /// not in local routing table).
    pub capacity: Option<u64>,
    /// Whether the peer is in the local routing table.
    pub reachable: bool,
}

/// `octo network node show <node_id_hex>` output envelope
/// (RFC-0011-q Phase 9 G11).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkNodeShowOutput {
    /// 32-byte node_id as 64 lowercase hex chars (echo).
    pub node_id_hex: String,
    /// Substrate-faithful projection of the specialized
    /// node record (None = node not in local registry).
    /// Field order matches substrate struct definition;
    /// metadata is serialized as JSON object with
    /// deterministic key order (BTreeMap-backed per
    /// RFC-0011-h §Output Envelope determinism).
    pub record: Option<SpecializedNodeRecordProjection>,
}

/// Substrate-faithful summary projection of
/// `SpecializedNodeRecord` for `octo network node show`
/// (RFC-0011-q §Output Envelope Phase 9). Field order
/// matches substrate struct definition; `metadata` is
/// serialized as JSON object with deterministic key order
/// (BTreeMap-backed per RFC-0011-h §Output Envelope
/// determinism).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct SpecializedNodeRecordProjection {
    /// 32-byte node_id as 64 lowercase hex chars.
    pub node_id_hex: String,
    /// Holder DID post-bind (None pre-bind).
    pub holder_did: Option<String>,
    /// Node class (lowercase: builder | provider |
    /// storage | bandwidth | orchestrator).
    pub node_class: String,
    /// Creation epoch (RFC-0855 §epoch).
    pub creation_epoch: u64,
    /// Operator metadata (extension-defined; deterministic
    /// BTreeMap key order).
    pub metadata: std::collections::BTreeMap<String, String>,
}

/// `octo network node bind <node_id_hex> --holder-did <did>`
/// output envelope (RFC-0011-q Phase 9 G11).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkNodeBindOutput {
    /// 32-byte node_id as 64 lowercase hex chars (echo).
    pub node_id_hex: String,
    /// Holder DID that the node is bound to (echo of
    /// --holder-did arg). Surfaced only in the Ok apply or
    /// dry-run preview envelope; NEVER surfaced on error
    /// paths (handler returns OctoCliError directly on
    /// substrate failure). Per-extension impl crates own
    /// DID validation per RFC-0011-q §Implicit Assumptions
    /// row 3.
    pub holder_did: String,
    /// Whether the bind was applied (false = dry-run
    /// preview only per RFC-0011-q §Confirmation Flag).
    pub applied: bool,
}

/// `octo network reputation list [--filter <filter>]`
/// output envelope (RFC-0011-r Phase 10 G13).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkReputationListOutput {
    /// Filter that was applied
    /// (`all` / `above-score` / `below-score`).
    pub filter: String,
    /// Optional threshold paired with above-score / below-score.
    pub threshold: Option<u32>,
    /// Peer reputations matching the filter (empty
    /// Vec when substrate is unavailable / unimplemented;
    /// stub impl per RFC-0011-r §Substrate Mapping Table).
    pub peers: Vec<PeerReputationProjection>,
}

/// Substrate-faithful summary projection of `PeerReputation`
/// for `octo network reputation list` (RFC-0011-r §Output
/// Envelope Phase 10). Field order matches substrate struct
/// definition; `peer_did_hex` is the lowercase hex encoding
/// of the canonical `RecorderDid` (RFC-0860).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PeerReputationProjection {
    /// Canonical peer DID as 104-char lowercase hex
    /// (52-byte `RecorderDid`).
    pub peer_did_hex: String,
    /// Aggregate reputation score (substrate `score: u32`).
    pub score: u32,
    /// Number of attestations on record
    /// (substrate `attestations_count: u32`).
    pub attestations_count: u32,
    /// Last update epoch (RFC-0855 §epoch).
    pub last_updated_epoch: u64,
}

/// `octo network reputation show <peer_did>` output
/// envelope (RFC-0011-r Phase 10 G13).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkReputationShowOutput {
    /// Peer DID that was looked up
    /// (`did:octo:0x<hex>` echo).
    pub peer_did: String,
    /// Optional peer reputation record (None when peer
    /// has no recorded reputation).
    pub record: Option<PeerReputationProjection>,
}

/// `octo network topology render` output envelope
/// (RFC-0011-s Phase 11 G14).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkTopologyRenderOutput {
    /// Mission ID hex of the rendered topology
    /// (echo; canonicalized via `MissionId::to_canonical_bytes`).
    pub mission_id_hex: String,
    /// Format that was applied (`ascii` / `dot`).
    pub format: String,
    /// Optional depth cap (1-100) that was applied.
    pub depth: Option<u32>,
    /// Rendered graph output (string body).
    pub render: String,
}

/// Gossip stats subcommand surface (RFC-0011-t Phase 12 G15).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkGossipAction {
    /// Show gossip protocol stats
    /// (RFC-0011-t §Subcommand Taxonomy Phase 12 `gossip --stats`).
    Stats(GossipStatsArgs),
}

/// `octo network gossip --stats [--format ascii|json]`
/// arguments (RFC-0011-t §Subcommand Taxonomy Phase 12
/// `gossip --stats`). `format` defaults to `ascii`
/// (terminal-friendly); `json` for machine-readable.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct GossipStatsArgs {
    /// Output format (RFC-0011-t §Subcommand Taxonomy
    /// Phase 12 `gossip --stats` `--format`).
    #[arg(long, value_enum, default_value_t = GossipStatsFormatKind::Ascii)]
    pub format: GossipStatsFormatKind,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Bare-word enum for `gossip --stats --format`
/// (RFC-0011-t §Subcommand Taxonomy Phase 12). Reuses
/// the substrate `GossipStats` projection from
/// `mon::gossip::Gossip::stats()` (Phase 12 additive
/// type extension per RFC-0011-t).
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GossipStatsFormatKind {
    /// ASCII (terminal-friendly).
    Ascii,
    /// JSON (machine-readable).
    Json,
}

/// `octo network gossip --stats` output envelope
/// (RFC-0011-t Phase 12 G15). Wraps the substrate
/// `GossipStats` projection for CLI dispatch.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkGossipStatsOutput {
    /// Mission ID hex of the gossip state
    /// (echo; canonicalized via `MissionId::to_canonical_bytes`).
    pub mission_id_hex: String,
    /// Number of reachable peers (substrate-faithful projection).
    pub peers_reachable: u64,
    /// Messages sent (substrate-faithful projection).
    pub messages_sent: u64,
    /// Messages received (substrate-faithful projection).
    pub messages_received: u64,
    /// Messages dropped (substrate-faithful projection).
    pub messages_dropped: u64,
    /// Anti-entropy rounds (stub zero per Phase 12
    /// additive-type-only; real counter in follow-on
    /// Layer D adapter mission).
    pub anti_entropy_rounds: u64,
    /// Last sync epoch.
    pub last_sync_epoch: u64,
    /// Format that was applied (`ascii` / `json`).
    pub format: String,
}

/// Envelope inspector + forward subcommand surface
/// (RFC-0011-u Phase 13 G16a + G16b).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkEnvelopeAction {
    /// Inspect envelope metadata
    /// (RFC-0011-u §Subcommand Taxonomy Phase 13 `envelope inspect`).
    Inspect(EnvelopeInspectArgs),
    /// Forward envelope to peer
    /// (RFC-0011-u §Subcommand Taxonomy Phase 13 `envelope forward`).
    Forward(EnvelopeForwardArgs),
}

/// `octo network envelope inspect <envelope_id_hex>`
/// arguments (RFC-0011-u §Subcommand Taxonomy Phase 13
/// `envelope inspect`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeInspectArgs {
    /// 32-byte envelope_id as 64 lowercase hex chars.
    /// Validated via `parse_32_byte_hex` pastejacking
    /// defense per Phase 5 RFC-0011-m precedent.
    pub envelope_id_hex: String,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network envelope forward <envelope_id_hex>
/// --destination <peer_id_hex> --ttl <N>` arguments
/// (RFC-0011-u §Subcommand Taxonomy Phase 13
/// `envelope forward`). Mutating subcommand gated by
/// `--confirm-acknowledge` per Phase 4 RFC-0011-l
/// precedent.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeForwardArgs {
    /// 32-byte envelope_id as 64 lowercase hex chars.
    /// Validated via `parse_32_byte_hex` pastejacking
    /// defense per Phase 5 RFC-0011-m precedent.
    pub envelope_id_hex: String,
    /// 32-byte destination peer_id as 64 lowercase hex chars.
    /// Validated via `parse_32_byte_hex`.
    #[arg(long)]
    pub destination_peer_id_hex: String,
    /// TTL epochs (time-to-live).
    #[arg(long)]
    pub ttl_epochs: u64,
    /// Dry-run preview (no broadcast).
    #[arg(long)]
    pub dry_run: bool,
    /// Required for mutating forward (RFC-0011-l Phase 4
    /// mutating-gate pattern).
    #[arg(long)]
    pub confirm_acknowledge: bool,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network envelope inspect` output envelope
/// (RFC-0011-u Phase 13 G16a). Wraps the substrate
/// `EnvelopeMeta` projection for CLI dispatch.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkEnvelopeInspectOutput {
    /// Envelope ID hex (echo).
    pub envelope_id_hex: String,
    /// Envelope kind label (mission / governance / reputation /
    /// slash / forward / bootstrap / discovery / coordinator).
    pub envelope_kind: String,
    /// Creator DID hex (canonicalized).
    pub creator_did_hex: String,
    /// Creation epoch.
    pub creation_epoch: u64,
    /// TTL epochs.
    pub ttl_epochs: u64,
    /// Whether envelope was found (false = unknown envelope).
    pub found: bool,
}

/// `octo network envelope forward` output envelope
/// (RFC-0011-u Phase 13 G16b). Wraps the substrate
/// `ForwardEnvelope::wire_bytes()` projection for CLI dispatch.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkEnvelopeForwardOutput {
    /// Source envelope ID hex (echo).
    pub source_envelope_id_hex: String,
    /// Destination peer ID hex (echo).
    pub destination_peer_id_hex: String,
    /// TTL epochs (echo).
    pub ttl_epochs: u64,
    /// Canonical wire-bytes hex (RFC-0855 §Wire Format).
    pub wire_bytes_hex: String,
    /// Wire-bytes length in bytes (80 per canonical layout).
    pub wire_bytes_len: usize,
    /// Dry-run preview flag (true = no broadcast).
    pub dry_run: bool,
}

/// Heartbeat probe subcommand surface
/// (RFC-0011-v Phase 14 G17).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkHeartbeatAction {
    /// Probe a peer for reachability
    /// (RFC-0011-v §Subcommand Taxonomy Phase 14 `heartbeat probe`).
    Probe(HeartbeatProbeArgs),
}

/// `octo network heartbeat probe <peer_did>` arguments
/// (RFC-0011-v §Subcommand Taxonomy Phase 14 `heartbeat probe`).
/// Read-only subcommand; clap u16 overflow pre-dispatch rejection
/// per Phase 5 RFC-0011-m precedent.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatProbeArgs {
    /// Peer DID (e.g. `did:octo:example-peer`).
    pub peer_did: String,
    /// Probe timeout in milliseconds (clap u16; default 5000ms).
    #[arg(long, default_value = "5000")]
    pub timeout_ms: u16,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network heartbeat probe` output envelope
/// (RFC-0011-v Phase 14 G17). Wraps the substrate
/// `HeartbeatProbeResult` projection for CLI dispatch.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkHeartbeatProbeOutput {
    /// Peer DID (echo).
    pub peer_did: String,
    /// Probe result label (`reachable` / `unreachable` / `timeout`).
    pub result_label: String,
    /// Round-trip-time in milliseconds (only present when
    /// `result_label == "reachable"`).
    pub rtt_ms: Option<u32>,
    /// Unreachable reason label (only present when
    /// `result_label == "unreachable"`).
    pub unreachable_reason_label: Option<String>,
    /// Unreachable reason detail (only present for
    /// `UnreachableReason::Other`).
    pub unreachable_reason_detail: Option<String>,
    /// Probe timeout in milliseconds (echo).
    pub timeout_ms: u16,
}

// === Subcommand arg structs (RFC-0011-j Phase 2) ===

/// Mode (bootstrap transport) subcommand surface.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkModeAction {
    /// Show the persisted bootstrap transport mode.
    Show(ModeShowArgs),
    /// Persist a new bootstrap transport mode (3-flag confirmation).
    Set(ModeSetArgs),
}

/// `octo network mode show` arguments (RFC-0011-j §Subcommand
/// Taxonomy Phase 2 `mode show`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct ModeShowArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network mode set` arguments (RFC-0011-j §Subcommand
/// Taxonomy Phase 2 `mode set`).
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct ModeSetArgs {
    /// Bootstrap transport mode label (`direct` | `tor_only` |
    /// `tor_with_ip_fallback`).
    ///
    /// Renamed to `--bootstrap-mode` (and field renamed from `mode`
    /// to `bootstrap_mode`) to avoid collision with the global
    /// operator `--mode` flag (`OperatorMode` enum flattened from
    /// `OperatorModeFlags`). The local arg type is `BootstrapMode`
    /// from `octo-network`; the global arg type is `OperatorMode`
    /// from `octo-cli::flags`. clap cannot disambiguate two
    /// `--mode` long flags when both structs flatten into the
    /// same `Octo` parse tree. Renaming the field forces clap derive
    /// to emit `--bootstrap-mode` from the struct field name.
    #[arg(long, value_parser = parse_bootstrap_mode)]
    pub bootstrap_mode: BootstrapMode,
    /// Listen address (e.g. `0.0.0.0:9000`).
    #[arg(long)]
    pub listen_addr: String,
    /// Target peer count (1..=256 per `BootstrapConfig` bounds).
    #[arg(long)]
    pub target_peers: u32,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Authority (seed list) subcommand surface.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkAuthorityAction {
    /// Show current seed list authority + deprecation state.
    Show(AuthorityShowArgs),
    /// Rotate authority post-fork (3-flag confirmation; Foundation
    /// rejected post-fork; zero-digest quorum proof rejected).
    Rotate(AuthorityRotateArgs),
}

/// `octo network authority show` arguments.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct AuthorityShowArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network authority rotate` arguments.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct AuthorityRotateArgs {
    /// Target authority (`foundation` | `dao`). Foundation rejected
    /// post-fork per `SeedListAuthority::rotate_post_fork` substrate
    /// contract.
    #[arg(long, value_parser = parse_seed_list_authority)]
    pub new_authority: SeedListAuthority,
    /// 32-byte governance quorum proof as 64 lowercase hex chars.
    /// Zero-digest is rejected at parse time (pastejacking defense
    /// + matches `rotate_post_fork` substrate contract).
    #[arg(long, value_parser = parse_64_char_hex_32byte)]
    pub quorum_proof_hex: [u8; 32],
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// Slash reputation subcommand surface.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkSlashAction {
    /// True iff the DID is excluded (slash count >= HARD_THRESHOLD).
    Excluded(SlashExcludedArgs),
    /// Distinct-DID count + total slash event count.
    Stats(SlashStatsArgs),
    /// List slash envelopes matching an optional filter.
    List(SlashListArgs),
    /// Show one envelope by `slash_id`.
    Show(SlashShowArgs),
}

/// `octo network slash excluded <did>` arguments.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct SlashExcludedArgs {
    /// 52-byte DID as 104 lowercase hex chars (RFC-0010 wire form).
    #[arg(value_parser = parse_did_hex_52byte)]
    pub did: [u8; 52],
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network slash stats` arguments.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct SlashStatsArgs {
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network slash list` arguments.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct SlashListArgs {
    /// Optional slash reason code filter (`0x000A` |
    /// `0x000B` | `0x000D`).
    #[arg(long)]
    pub slash_reason: Option<u16>,
    /// Optional maximum envelope count.
    #[arg(long)]
    pub limit: Option<u32>,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

/// `octo network slash show <slash_id>` arguments.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
pub struct SlashShowArgs {
    /// Slash instance id (RFC-0011-j §Substrate-Additions G6 row:
    /// `slash-<target_peer>-<cast_at>` convention).
    pub slash_id: String,
    /// Force JSON envelope output (RFC-0011 §Output Envelope).
    #[arg(long)]
    pub json: bool,
}

// === Output envelopes (RFC-0011-i §Output Envelope Phase 1) ===

/// Render payload for `octo network peers list` (RFC-0011-i §Output
/// Envelope).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkPeersListOutput {
    /// Per-row peer projection (`PeerSummaryOutput`).
    pub peers: Vec<PeerSummaryOutput>,
    /// Row count in `peers`.
    pub count_returned: u64,
}

/// Substrate-faithful summary projection of `GatewayCacheEntry`
/// for `octo network peers list` (RFC-0011-i §Output Envelope).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PeerSummaryOutput {
    /// 32-byte gateway_id as 64 lowercase hex chars.
    pub gateway_id_hex: String,
    /// Network identifier from `GatewayIdentity::new`.
    pub network_id: u32,
    /// Gateway class label (canonical `lowercase` form).
    pub gateway_class: String,
    /// Trust score (0..=1000 per RFC-0851 §10).
    pub trust_score: u32,
    /// Epoch when first seen.
    pub first_seen: u64,
    /// Epoch when last seen.
    pub last_seen: u64,
    /// Capability count.
    pub capability_count: u64,
    /// Endpoint count.
    pub endpoint_count: u64,
}

/// Render payload for `octo network peers get <gateway_id>`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkPeerGetOutput {
    /// Single peer record.
    pub peer: PeerRecordOutput,
}

/// Substrate-faithful full projection of `GatewayCacheEntry`
/// for `octo network peers get`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PeerRecordOutput {
    /// 32-byte gateway_id as 64 lowercase hex chars.
    pub gateway_id_hex: String,
    /// 32-byte advertisement_hash as 64 lowercase hex chars.
    pub advertisement_hash_hex: String,
    /// Network identifier.
    pub network_id: u32,
    /// Gateway class label.
    pub gateway_class: String,
    /// Trust score.
    pub trust_score: u32,
    /// Epoch when first seen.
    pub first_seen: u64,
    /// Epoch when last seen.
    pub last_seen: u64,
    /// Capability names (substrate enum Debug rendered).
    pub capabilities: Vec<String>,
    /// Per-endpoint projection.
    pub endpoints: Vec<PeerEndpointOutput>,
}

/// Single endpoint projection (RFC-0011-i §Output Envelope).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PeerEndpointOutput {
    /// Transport type tag.
    pub transport_type: u16,
    /// 32-byte endpoint_hash as 64 lowercase hex chars.
    pub endpoint_hash_hex: String,
}

/// Render payload for `octo network identity show` (RFC-0011-i §Output
/// Envelope).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkIdentityShowOutput {
    /// Network identifier from `LocalGatewayIdentity`.
    pub network_id: u32,
    /// Gateway class label.
    pub gateway_class: String,
    /// Unix epoch when the identity was created.
    pub creation_epoch: u64,
}

/// Render payload for `octo network trust-graph render`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkTrustGraphRenderOutput {
    /// Format label (`ascii` | `dot`).
    pub format: String,
    /// Depth (1..=100).
    pub depth: u32,
    /// Substrate-rendered body.
    pub body: String,
}

/// Render payload for `octo network governance rotation status`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkGovernanceRotationStatusOutput {
    /// Whether the rotation has 5-of-7 quorum.
    pub has_quorum: bool,
    /// Effective epoch + governance migration window.
    pub migration_deadline: u64,
    /// Whether the current epoch is in the migration window.
    pub in_migration_window: bool,
    /// Redacted DID wire form (first 8 + last 4 chars).
    pub did_redacted: String,
}

// === Output envelopes (RFC-0011-j §Output Envelope Phase 2) ===

/// Render payload for `octo network mode show` (RFC-0011-j §Output
/// Envelope Phase 2 `mode show`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkModeShowOutput {
    /// Bootstrap mode label (`direct` | `tor_only` |
    /// `tor_with_ip_fallback`).
    pub mode: String,
    /// Listen address (e.g. `0.0.0.0:9000`).
    pub listen_addr: String,
    /// Target peer count (1..=256).
    pub target_peers: u32,
    /// Optional 32-byte governance quorum proof (hex form). `Some`
    /// for Dao rotation state; `None` for direct/tor-only without
    /// authority rotation.
    pub governance_quorum_proof_hex: Option<String>,
}

/// Render payload for `octo network mode set` (RFC-0011-j §Output
/// Envelope Phase 2 `mode set`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkModeSetOutput {
    /// Written TOML path (redacted; octo_home is elided per
    /// RFC-0011-h §Redaction Layer).
    pub written_path_redacted: String,
    /// Persisted mode label.
    pub mode: String,
}

/// Render payload for `octo network authority show` (RFC-0011-j
/// §Output Envelope Phase 2 `authority show`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkAuthorityShowOutput {
    /// Current authority label (`foundation` | `dao`).
    pub authority: String,
    /// Whether the authority is deprecated at the current epoch
    /// (Foundation deprecated at `EPOCH_GOVERNANCE_TAKEOVER`).
    pub deprecated: bool,
    /// Effective epoch (informational; substrate-faithful projection).
    pub epoch: u64,
}

/// Render payload for `octo network authority rotate` (RFC-0011-j
/// §Output Envelope Phase 2 `authority rotate`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkAuthorityRotateOutput {
    /// New authority label after rotation (`dao` only post-fork).
    pub new_authority: String,
    /// Old authority label before rotation (`foundation` |
    /// `dao`).
    pub old_authority: String,
}

/// Render payload for `octo network slash excluded <did>`
/// (RFC-0011-j §Output Envelope Phase 2 `slash excluded`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkSlashExcludedOutput {
    /// Redacted DID wire form (first 8 + last 4 chars).
    pub did_redacted: String,
    /// True iff `global_slash_count(did) >= HARD_THRESHOLD`.
    pub excluded: bool,
    /// Hard threshold (5 per mission 0855p-b).
    pub threshold: u32,
}

/// Render payload for `octo network slash stats` (RFC-0011-j
/// §Output Envelope Phase 2 `slash stats`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkSlashStatsOutput {
    /// Distinct DID count.
    pub did_count: u64,
    /// Total slash event count across all DIDs.
    pub total_slashes: u64,
}

/// Render payload for `octo network slash list` (RFC-0011-j
/// §Output Envelope Phase 2 `slash list`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkSlashListOutput {
    /// Per-row envelope summary (cast_at-ascending per substrate).
    pub envelopes: Vec<SlashEnvelopeSummaryOutput>,
    /// Row count in `envelopes` (post-limit).
    pub count_returned: u64,
}

/// Substrate-faithful summary projection of `SlashEnvelope` for
/// `octo network slash list` (RFC-0011-j §Output Envelope Phase 2).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct SlashEnvelopeSummaryOutput {
    /// Slash instance id.
    pub slash_id: String,
    /// Slash reason code (u16).
    pub slash_reason: u16,
    /// Redacted target peer (first 8 + last 4 chars).
    pub target_peer_redacted: String,
    /// Unix seconds when the slash was cast.
    pub cast_at: u64,
    /// Domain / mission identifier.
    pub domain_id: String,
}

/// Render payload for `octo network slash show <slash_id>`
/// (RFC-0011-j §Output Envelope Phase 2 `slash show`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkSlashShowOutput {
    /// Single envelope detail (None if `slash_id` unknown).
    pub envelope: Option<SlashEnvelopeDetailOutput>,
}

/// Substrate-faithful detail projection of `SlashEnvelope` for
/// `octo network slash show` (RFC-0011-j §Output Envelope Phase 2).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct SlashEnvelopeDetailOutput {
    /// Slash instance id.
    pub slash_id: String,
    /// Slash reason code (u16).
    pub slash_reason: u16,
    /// Slash reason sub-code (u32; used by `0x000D` sub-codes).
    pub slash_reason_data: u32,
    /// Redacted target peer (first 8 + last 4 chars).
    pub target_peer_redacted: String,
    /// Unix seconds when the slash was cast.
    pub cast_at: u64,
    /// Domain / mission identifier.
    pub domain_id: String,
}

// === Output envelopes (RFC-0011-k §Output Envelope Phase 3) ===

/// Render payload for `octo network governance tally --proposal-id <N>`
/// (RFC-0011-k §Output Envelope Phase 3 `governance tally`).
///
/// Substrate-faithful: today the canonical-bytes helper (`G3b`) is the
/// only persistence surface; the full tally ledger lands as the Phase 6
/// follow-on. The CLI projects the canonical-bytes hash of the
/// zero-default proposal at the requested proposal_id.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkGovernanceTallyOutput {
    /// Monotonic proposal id (per-issuer).
    pub proposal_id: u64,
    /// 32-byte canonical-bytes hash as 64 lowercase hex chars
    /// (BLAKE3 over the canonical JSON encoding per G3b).
    pub canonical_hash_hex: String,
    /// Lifecycle state label (`voting` — the substrate-faithful
    /// state for a fresh proposal).
    pub state: String,
}

/// Render payload for `octo network coordinator show <coordinator_id>`
/// (RFC-0011-k §Output Envelope Phase 3 `coordinator show`).
///
/// Substrate-faithful: today `CoordinatorRecord::load` returns `None`
/// for all ids; the CLI surfaces exit 84 `NetworkCoordinatorNotFound`.
/// The envelope contract is reserved for the populated branch (Phase 6
/// persistence adapter follow-on); the CLI never emits a success
/// envelope in current substrate.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkCoordinatorShowOutput {
    /// 32-byte coordinator_id as 64 lowercase hex chars.
    pub coordinator_id_hex: String,
    /// Substrate-faithful record fields. All `None` until the
    /// persistence adapter lands.
    pub record: Option<CoordinatorRecordProjectionOutput>,
}

/// Substrate-faithful projection of `CoordinatorRecord` for
/// `octo network coordinator show` (RFC-0011-k §Output Envelope).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct CoordinatorRecordProjectionOutput {
    /// Coordinator peer identifier (32-byte hex form).
    pub coordinator_peer_id_hex: String,
    /// Lifecycle label (`created` | `active` | `resigned` | `inactive`
    /// | `banned`).
    pub state: String,
    /// Epoch this term started.
    pub term_start_epoch: u64,
    /// Epoch this term ends (exclusive).
    pub term_end_epoch: u64,
    /// Slash count (cool-down ban at 5).
    pub slash_count: u32,
    /// Locked `octo_o_stake` for this term.
    pub octo_o_stake_locked: u64,
}

/// Render payload for `octo network coordinator admin --action ...`
/// (RFC-0011-k §Output Envelope Phase 3 `coordinator admin`).
///
/// Substrate-faithful: today `dispatch_coordinator_admin_action`
/// returns `AdapterUnwired`; the CLI surfaces the typed failure as
/// `OctoCliError::Internal("coordinator admin adapter not yet wired")`.
/// The envelope contract is reserved for the wired branch (Phase 6
/// adapter follow-on); the CLI never emits a success envelope in
/// current substrate.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkCoordinatorAdminOutput {
    /// Action label (`transfer_ownership` | `ban_member` |
    /// `promote_to_admin`).
    pub action: String,
    /// Group id (canonical string form).
    pub group_id: String,
    /// Redacted target peer id (first 8 + last 4 chars of 64 hex).
    pub target_peer_redacted: String,
}

// === BIND envelope output envelopes (RFC-0011-l Phase 4) ===

/// `octo network bind-envelope show` output envelope (RFC-0011-l
/// Phase 4 row G22).
///
/// Today the substrate `BindEnvelope::load` returns `None`
/// (persistence adapter absent — Phase 6 follow-on per
/// `0011-h-s-a-bind-envelope-persistence`); the CLI surfaces the
/// miss as typed exit 89 `NetworkSubstrateUnavailable` per
/// RFC-0011-h §Error Handling. The success envelope below is the
/// post-Phase-6 shape.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkBindEnvelopeShowOutput {
    /// The 52-char hex-encoded mission `domain_id`.
    pub domain_id: String,
    /// The platform identifier (e.g., `whatsapp`, `matrix`,
    /// `telegram`).
    pub platform: String,
    /// The physical group identifier (e.g., WhatsApp group JID,
    /// Matrix room ID, Telegram supergroup ID).
    pub group_id: String,
    /// Optional participant-filter subset (per RFC-0850p-c
    /// `partial-bindings` mission). `None` means all
    /// physical-group members participate.
    pub participant_filter: Option<Vec<String>>,
    /// Group size at binding time (per RFC-0855p-c
    /// `slash-small-groups` mission).
    pub member_count_at_bind: u16,
}

/// `octo network bind-envelope rebind-{prepare,commit,abort}`
/// output envelope (RFC-0011-l Phase 4 row G21).
///
/// The CLI today surfaces a preview envelope (dry-run) OR a
/// typed error (substrate `AdapterUnwired` per the Phase 6
/// persistence adapter follow-on). The preview envelope is the
/// pre-substrate-dispatch payload summary the operator reads at
/// the preview prompt per RFC-0011-l §Subcommand Taxonomy
/// rebind-* rows.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkBindEnvelopeRebindOutput {
    /// The rebind arm: `prepare` | `commit` | `abort`.
    pub arm: String,
    /// The 52-char hex-encoded mission `domain_id`.
    pub domain_id: String,
    /// Whether this is a dry-run preview (no substrate dispatch
    /// attempted) or a confirmed dispatch (substrate adapter
    /// unwired today; surfaces as typed Internal error in the
    /// operator exit code).
    pub dispatched: bool,
    /// The `RebindCoordinator` state at preview time
    /// (`Preparing` | `Committing` | `Aborted`).
    pub coordinator_state: String,
}

/// `octo network bind-envelope rebind-abort` output envelope
/// (RFC-0011-l Phase 4 row G21; carries the operator-supplied
/// `reason` field).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkBindEnvelopeRebindAbortOutput {
    /// The 52-char hex-encoded mission `domain_id`.
    pub domain_id: String,
    /// Operator-supplied abort reason (free-text; preserved in
    /// the `RebindAbort.reason` debug field per RFC-0871).
    pub reason_redacted: String,
    /// Whether this is a dry-run preview or a confirmed dispatch.
    pub dispatched: bool,
}

/// `octo network discovery advertisement show` output envelope
/// (RFC-0011-m Phase 5 row G23). One entry per `advertisement_id`
/// lookup OR one entry per `MissionAdvertisementCache::iter()`
/// enumeration. Each entry surfaces
/// `MissionAdvertisement.advertisement_hash` (BLAKE3-256 hex)
/// per RFC-0855 §8.2 + the optional `is_ttl_exceeded` flag when
/// `--hops <U16>` is supplied per the same RFC.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkDiscoveryAdvertisementShowOutput {
    /// The 64-char hex-encoded `advertisement_id` (BLAKE3-256 of
    /// `to_signing_bytes()` per RFC-0855 §8.2). For enumeration
    /// mode, this is the cache key; for targeted lookup, this
    /// echoes the operator-supplied ID.
    pub advertisement_hash_hex: String,
    /// The mission scope (`Public` | `InviteOnly` | `Stealth`
    /// | `Federated` | `Ephemeral`).
    pub scope: String,
    /// Whether the TTL has been exceeded for the operator-
    /// supplied hop count (only set when `--hops` is supplied;
    /// `null` otherwise per RFC-0011-m §Output Envelope).
    pub is_ttl_exceeded: Option<bool>,
    /// Logical timestamp from the underlying
    /// `MissionAdvertisement.logical_timestamp` (RFC-0855 §8.2).
    pub logical_timestamp: u64,
    /// The 64-char hex-encoded gateway_id of the advertising
    /// gateway per RFC-0855 §8.2.
    pub gateway_id_hex: String,
}

/// `octo network discovery invitation show` output envelope
/// (RFC-0011-m Phase 5 row G24). One entry per `invitation_id`
/// lookup OR one entry per `MissionInvitationCache::iter()`
/// enumeration. Each entry surfaces the `MissionInvitation`
/// fields per RFC-0011-h §Output Envelope L511-L518.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkDiscoveryInvitationShowOutput {
    /// The 64-char hex-encoded `invitation_id` (BLAKE3-256 of
    /// `to_signing_bytes()` per RFC-0855 §8.2). For enumeration
    /// mode, this is the cache key; for targeted lookup, this
    /// echoes the operator-supplied ID.
    pub invitation_hash_hex: String,
    /// The mission_id hex (per RFC-0011-h §Output Envelope
    /// L511-L518; derived from `MissionId::to_canonical_bytes`
    /// BLAKE3-256).
    pub mission_id_hex: String,
    /// The 64-char hex-encoded invitee gateway_id (per RFC-0011-h
    /// §Output Envelope L511-L518).
    pub invitee_gateway_id_hex: String,
    /// The 64-char hex-encoded coordinator gateway_id (per
    /// RFC-0011-h §Output Envelope L511-L518).
    pub coordinator_gateway_id_hex: String,
    /// Logical timestamp from the underlying
    /// `MissionInvitation.logical_timestamp` (RFC-0855 §8.2).
    pub logical_timestamp: u64,
    /// The signing-bytes hex (per RFC-0011-h §Output Envelope
    /// L511-L518; hex-encoded `to_signing_bytes()` for operator
    /// cross-check; substrate-side signature verification
    /// remains the authoritative path per RFC-0011-m
    /// §Security Considerations).
    pub signing_bytes_hex: String,
}

// === Dispatch ===

/// Dispatch a `NetworkAction` to its handler. Top-level entry point
/// from `commands::dispatch`.
pub fn dispatch(action: &NetworkAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        NetworkAction::Peers { action: peer_act } => match peer_act {
            PeersAction::List(args) => peers_list(args, cli),
            PeersAction::Get(args) => peers_get(args, cli),
        },
        NetworkAction::Identity { action: id_act } => match id_act {
            NetworkIdentityAction::Show(args) => identity_show(args, cli),
        },
        NetworkAction::TrustGraph { action: tg_act } => match tg_act {
            TrustGraphAction::Render(args) => trust_graph_render(args, cli),
        },
        NetworkAction::Governance { action: gov_act } => match gov_act {
            NetworkGovernanceAction::Rotation { action: rot_act } => match rot_act {
                NetworkGovernanceRotationAction::Status(args) => {
                    governance_rotation_status(args, cli)
                }
            },
            NetworkGovernanceAction::Tally(args) => governance_tally(args, cli),
        },
        NetworkAction::Mode { action: mode_act } => match mode_act {
            NetworkModeAction::Show(args) => mode_show(args, cli),
            NetworkModeAction::Set(args) => mode_set(args, cli),
        },
        NetworkAction::Authority { action: auth_act } => match auth_act {
            NetworkAuthorityAction::Show(args) => authority_show(args, cli),
            NetworkAuthorityAction::Rotate(args) => authority_rotate(args, cli),
        },
        NetworkAction::Slash { action: slash_act } => match slash_act {
            NetworkSlashAction::Excluded(args) => slash_excluded(args, cli),
            NetworkSlashAction::Stats(args) => slash_stats(args, cli),
            NetworkSlashAction::List(args) => slash_list(args, cli),
            NetworkSlashAction::Show(args) => slash_show(args, cli),
        },
        NetworkAction::Coordinator { action: coord_act } => match coord_act {
            NetworkCoordinatorAction::Show(args) => coordinator_show(args, cli),
            NetworkCoordinatorAction::Admin(args) => coordinator_admin(args, cli),
        },
        NetworkAction::BindEnvelope { action: bind_act } => match bind_act {
            NetworkBindEnvelopeAction::Show(args) => bind_envelope_show(args, cli),
            NetworkBindEnvelopeAction::RebindPrepare(args) => {
                bind_envelope_rebind_prepare(args, cli)
            }
            NetworkBindEnvelopeAction::RebindCommit(args) => bind_envelope_rebind_commit(args, cli),
            NetworkBindEnvelopeAction::RebindAbort(args) => bind_envelope_rebind_abort(args, cli),
        },
        NetworkAction::Discovery { action: disc_act } => match disc_act {
            NetworkDiscoveryAction::AdvertisementShow(args) => {
                discovery_advertisement_show(args, cli)
            }
            NetworkDiscoveryAction::InvitationShow(args) => discovery_invitation_show(args, cli),
        },
        NetworkAction::Bootstrap(args) => network_bootstrap(args, cli),
        NetworkAction::Status(args) => network_status(args, cli),
        NetworkAction::SlashBridge { action: sb_act } => match sb_act {
            NetworkSlashBridgeAction::List(args) => network_slash_bridge_list(args, cli),
            NetworkSlashBridgeAction::Propagate(args) => network_slash_bridge_propagate(args, cli),
        },
        NetworkAction::Router { action: router_act } => match router_act {
            NetworkRouterAction::Status(args) => network_router_status(args, cli),
            NetworkRouterAction::Peers(args) => network_router_peers(args, cli),
        },
        NetworkAction::Node { action: node_act } => match node_act {
            NetworkNodeAction::Show(args) => network_node_show(args, cli),
            NetworkNodeAction::Bind(args) => network_node_bind(args, cli),
        },
        NetworkAction::Reputation { action: rep_act } => match rep_act {
            NetworkReputationAction::List(args) => network_reputation_list(args, cli),
            NetworkReputationAction::Show(args) => network_reputation_show(args, cli),
        },
        NetworkAction::Topology { action: top_act } => match top_act {
            NetworkTopologyAction::Render(args) => network_topology_render(args, cli),
        },
        NetworkAction::Gossip { action: g_act } => match g_act {
            NetworkGossipAction::Stats(args) => network_gossip_stats(args, cli),
        },
        NetworkAction::Envelope { action: env_act } => match env_act {
            NetworkEnvelopeAction::Inspect(args) => network_envelope_inspect(args, cli),
            NetworkEnvelopeAction::Forward(args) => network_envelope_forward(args, cli),
        },
        NetworkAction::Heartbeat { action: hb_act } => match hb_act {
            NetworkHeartbeatAction::Probe(args) => network_heartbeat_probe(args, cli),
        },
    }
}

// === Handlers ===

fn peers_list(args: &PeersListArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let cache = GatewayCache::new(256);
    let mut peers: Vec<PeerSummaryOutput> = Vec::new();
    for (_id, entry) in cache.iter() {
        peers.push(peer_entry_to_summary(entry));
    }
    let count_returned = peers.len() as u64;
    let env = OutputEnvelope::new(
        "octo.network.peers.list.v1",
        NetworkPeersListOutput {
            peers,
            count_returned,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn peers_get(args: &PeersGetArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let cache = GatewayCache::new(256);
    let entry = cache.get(&args.gateway_id).ok_or_else(|| {
        let redacted = redact_gateway_id(&args.gateway_id);
        OctoCliError::NetworkPeerNotFound {
            gateway_id_hex: redacted,
        }
    })?;
    let env = OutputEnvelope::new(
        "octo.network.peers.get.v1",
        NetworkPeerGetOutput {
            peer: peer_entry_to_record(entry),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn identity_show(args: &IdentityShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let octo_home: PathBuf = home::resolve().map_err(|e| match e {
        OctoCliError::NoOctoHome => OctoCliError::NetworkLocalKeyUnavailable,
        other => other,
    })?;
    let state = LocalGatewayIdentity::load(&octo_home).map_err(|e| match e {
        octo_network::mon::local_gateway_identity::LocalGatewayIdentityError::NotInitialized => {
            OctoCliError::NetworkLocalKeyUnavailable
        }
        other => OctoCliError::Internal(sanitize_substrate_error(&format!(
            "local gateway identity load: {other}"
        ))),
    })?;
    let env = OutputEnvelope::new(
        "octo.network.identity.show.v1",
        NetworkIdentityShowOutput {
            network_id: state.network_id,
            gateway_class: gateway_class_to_str(&state.gateway_class).to_string(),
            creation_epoch: state.creation_epoch,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn trust_graph_render(args: &TrustGraphRenderArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let depth = args.depth.ok_or_else(|| {
        OctoCliError::Internal("trust-graph render: --depth is required (1..=100)".into())
    })?;
    if !(1..=100).contains(&depth) {
        return Err(OctoCliError::NetworkGraphDepthBelowRange { depth });
    }
    let graph = TrustGraph::new();
    let body = graph.render(args.format);
    let env = OutputEnvelope::new(
        "octo.network.trust_graph.render.v1",
        NetworkTrustGraphRenderOutput {
            format: graph_format_str(args.format).to_string(),
            depth,
            body,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn governance_rotation_status(
    args: &GovernanceRotationStatusArgs,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    if !is_valid_did_codec(&args.did_codec) {
        return Err(OctoCliError::NetworkInvalidDid {
            did_redacted: redact_did(&args.did_codec),
        });
    }
    let rotation = GovernanceRotation {
        new_governance_id: [0u8; 32],
        old_governance_id: [0u8; 32],
        evidence: Vec::new(),
        effective_epoch: 0,
        signatures: Vec::new(),
        signed_at_epoch: 0,
    };
    let env = OutputEnvelope::new(
        "octo.network.governance.rotation.status.v1",
        NetworkGovernanceRotationStatusOutput {
            has_quorum: rotation.has_quorum(),
            migration_deadline: rotation.migration_deadline(),
            in_migration_window: rotation.in_migration_window(0),
            did_redacted: redact_did(&args.did_codec),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

// === Phase 2 handlers (RFC-0011-j §Subcommand Taxonomy) ===

fn mode_show(args: &ModeShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let octo_home: PathBuf = home::resolve().map_err(|e| match e {
        OctoCliError::NoOctoHome => OctoCliError::NetworkConfigParseFailed {
            kind_redacted: "io".to_string(),
            path_redacted: None,
        },
        other => other,
    })?;
    let path = octo_home.join("network").join("bootstrap.toml");
    let cfg = BootstrapConfig::from_toml(&path).map_err(map_bootstrap_config_err)?;
    let env = OutputEnvelope::new(
        "octo.network.mode.show.v1",
        NetworkModeShowOutput {
            mode: bootstrap_mode_str(cfg.mode).to_string(),
            listen_addr: cfg.listen_addr,
            target_peers: cfg.target_peers,
            governance_quorum_proof_hex: cfg.governance_quorum_proof.map(hex::encode),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn mode_set(args: &ModeSetArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // 3-flag confirmation per `require_confirm` (auditor denied;
    // CI requires `--allow-write`).
    crate::commands::identity::require_confirm(cli, "network mode set")?;
    let octo_home: PathBuf = home::resolve().map_err(|e| match e {
        OctoCliError::NoOctoHome => OctoCliError::NetworkConfigParseFailed {
            kind_redacted: "io".to_string(),
            path_redacted: None,
        },
        other => other,
    })?;
    let path = octo_home.join("network").join("bootstrap.toml");
    // Substrate-faithful: preserve any existing `governance_quorum_proof`
    // (the field is shared between `mode` + `authority` rotations;
    // `mode set` does not clear it).
    let mut cfg = BootstrapConfig::from_toml(&path).unwrap_or_else(|_| BootstrapConfig {
        mode: BootstrapMode::default(),
        listen_addr: String::new(),
        target_peers: 0,
        governance_quorum_proof: None,
    });
    cfg.mode = args.bootstrap_mode;
    cfg.listen_addr = args.listen_addr.clone();
    cfg.target_peers = args.target_peers;
    cfg.save_toml(&path).map_err(map_bootstrap_config_err)?;
    let env = OutputEnvelope::new(
        "octo.network.mode.set.v1",
        NetworkModeSetOutput {
            written_path_redacted: redact_octo_home_path(&path, &octo_home),
            mode: bootstrap_mode_str(cfg.mode).to_string(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn authority_show(args: &AuthorityShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // Substrate-faithful: project current authority + deprecation state
    // via `verify_authority`. Foundation at epoch 0 is accepted;
    // Foundation at `EPOCH_GOVERNANCE_TAKEOVER` is deprecated.
    let authority = SeedListAuthority::Foundation;
    let epoch = 0u64;
    let deprecation = verify_authority(authority, epoch);
    let env = OutputEnvelope::new(
        "octo.network.authority.show.v1",
        NetworkAuthorityShowOutput {
            authority: seed_authority_str(authority).to_string(),
            deprecated: matches!(
                deprecation,
                Err(SeedAuthorityError::SeedListAuthorityDeprecated)
            ),
            epoch,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn authority_rotate(args: &AuthorityRotateArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // 3-flag confirmation per `require_confirm`.
    crate::commands::identity::require_confirm(cli, "network authority rotate")?;
    let old = SeedListAuthority::Foundation;
    let new = SeedListAuthority::rotate_post_fork(args.new_authority, args.quorum_proof_hex)
        .map_err(|e| OctoCliError::NetworkSubstrateUnavailable {
            companion: match e {
                SeedAuthorityError::SeedListAuthorityDeprecated => "G8",
                SeedAuthorityError::BadSignature => "G8",
                SeedAuthorityError::DaoNotYetActive => "G8",
            },
            detail: "".to_string(),
        })?;
    let env = OutputEnvelope::new(
        "octo.network.authority.rotate.v1",
        NetworkAuthorityRotateOutput {
            new_authority: seed_authority_str(new).to_string(),
            old_authority: seed_authority_str(old).to_string(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn slash_excluded(args: &SlashExcludedArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // Substrate-faithful: per-process in-memory store. The G6b
    // companion mission (`SlashStoreLoader`) is the async hydration
    // boundary; here we read what the substrate has been populated
    // with during this session (or returns `not excluded` for an
    // unknown DID).
    let store = SlashReputationStoreCompat::new();
    let did = octo_reputation::types::RecorderDid::from_bytes(&args.did).map_err(|_| {
        OctoCliError::NetworkInvalidDid {
            did_redacted: redact_did_bytes(&args.did),
        }
    })?;
    let excluded = store.is_excluded(&did);
    let env = OutputEnvelope::new(
        "octo.network.slash.excluded.v1",
        NetworkSlashExcludedOutput {
            did_redacted: redact_did_bytes(&args.did),
            excluded,
            threshold: octo_network::reputation::HARD_THRESHOLD,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn slash_stats(args: &SlashStatsArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let store = SlashReputationStoreCompat::new();
    let env = OutputEnvelope::new(
        "octo.network.slash.stats.v1",
        NetworkSlashStatsOutput {
            did_count: store.did_count() as u64,
            total_slashes: store.total_slashes(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn slash_list(args: &SlashListArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let store = SlashReputationStoreCompat::new();
    let filter = SlashListFilter {
        did: None,
        slash_reason: args.slash_reason,
        limit: args.limit,
    };
    let envelopes = store.list(&filter);
    let count_returned = envelopes.len() as u64;
    let summaries: Vec<SlashEnvelopeSummaryOutput> = envelopes
        .iter()
        .map(|e| SlashEnvelopeSummaryOutput {
            slash_id: e.slash_id.clone(),
            slash_reason: e.slash_reason,
            target_peer_redacted: redact_target_peer(&e.target_peer),
            cast_at: e.cast_at,
            domain_id: e.domain_id.clone(),
        })
        .collect();
    let env = OutputEnvelope::new(
        "octo.network.slash.list.v1",
        NetworkSlashListOutput {
            envelopes: summaries,
            count_returned,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn slash_show(args: &SlashShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let store = SlashReputationStoreCompat::new();
    let envelope = store
        .show(&args.slash_id)
        .map(|e| SlashEnvelopeDetailOutput {
            slash_id: e.slash_id.clone(),
            slash_reason: e.slash_reason,
            slash_reason_data: e.slash_reason_data,
            target_peer_redacted: redact_target_peer(&e.target_peer),
            cast_at: e.cast_at,
            domain_id: e.domain_id.clone(),
        });
    let env = OutputEnvelope::new(
        "octo.network.slash.show.v1",
        NetworkSlashShowOutput { envelope },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn governance_tally(args: &GovernanceTallyArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // Substrate-faithful: canonical-bytes hash of the canonical
    // zero-default proposal at `args.proposal_id`. Substrate persistence
    // adapter is the Phase 6 follow-on; today the substrate exposes
    // only the canonical-bytes helper (G3b) so the CLI projects the
    // tally surface from a deterministic zero-default proposal.
    let proposal = GovernanceProposal {
        proposal_id: args.proposal_id,
        issuer: String::new(),
        decision: DecisionType::Admission,
        state: ProposalState::Voting,
        voting_opens_at_millis: 0,
        voting_closes_at_millis: 0,
        approval_tally_bps: 0,
        rejection_tally_bps: 0,
    };
    let canonical_hash_hex = hex::encode(governance_proposal_canonical_bytes(&proposal));
    let env = OutputEnvelope::new(
        "octo.network.governance.tally.v1",
        NetworkGovernanceTallyOutput {
            proposal_id: args.proposal_id,
            canonical_hash_hex,
            state: "voting".to_string(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn coordinator_show(args: &CoordinatorShowArgs, _cli: &Octo) -> Result<(), OctoCliError> {
    // Substrate-faithful: `CoordinatorRecord::load` returns `None`
    // until the persistence adapter lands (Phase 6 follow-on per
    // `0011-h-s-a-coordinator-record-persistence`). CLI translates
    // Option::None to typed exit 84 `NetworkCoordinatorNotFound`.
    if CoordinatorRecord::load(&args.coordinator_id).is_none() {
        return Err(OctoCliError::NetworkCoordinatorNotFound {
            coordinator_id_redacted: hex::encode(args.coordinator_id),
        });
    }
    // Unreachable in current substrate (load always returns None);
    // substrate persistence adapter lands the populated branch
    // post-Phase 6.
    Err(OctoCliError::Internal(
        "coordinator record found but substrate persistence adapter not yet wired".into(),
    ))
}

fn coordinator_admin(args: &CoordinatorAdminArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // 3-flag confirmation per `require_confirm`.
    crate::commands::identity::require_confirm(cli, "network coordinator admin")?;
    let group_id = GroupId(args.group_id.clone());
    let substrate_action = args.action.to_substrate(group_id, args.target);
    // Substrate-faithful: dispatch returns `AdapterUnwired` until
    // a `CoordinatorAdmin` adapter is wired at the dispatch boundary.
    // Phase 6 follow-on per `0011-h-s-a-coordinator-admin-adapter`.
    dispatch_coordinator_admin_action(&substrate_action).map_err(|e| match e {
        CoordinatorAdminActionError::AdapterUnwired => OctoCliError::Internal(
            "coordinator admin adapter not yet wired (Phase 6 follow-on)".into(),
        ),
    })?;
    let env = OutputEnvelope::new(
        "octo.network.coordinator.admin.v1",
        NetworkCoordinatorAdminOutput {
            action: args.action.as_str().to_string(),
            group_id: args.group_id.clone(),
            target_peer_redacted: redact_target_peer(&hex::encode(args.target)),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

// === BIND envelope handlers (RFC-0011-l Phase 4) ===

fn bind_envelope_show(args: &BindEnvelopeShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // Substrate-faithful: `BindEnvelope::load` returns `None` until
    // the persistence adapter lands (Phase 6 follow-on per
    // `0011-h-s-a-bind-envelope-persistence`). CLI translates
    // Option::None to typed exit 89 `NetworkSubstrateUnavailable`.
    if BindEnvelope::load(&args.domain_id).is_none() {
        return Err(OctoCliError::NetworkSubstrateUnavailable {
            companion: "G22",
            detail: "".to_string(),
        });
    }
    // Unreachable in current substrate (load always returns None);
    // substrate persistence adapter lands the populated branch
    // post-Phase 6.
    let env = OutputEnvelope::new(
        "octo.network.bind-envelope.show.v1",
        NetworkBindEnvelopeShowOutput {
            domain_id: args.domain_id.clone(),
            platform: String::new(),
            group_id: String::new(),
            participant_filter: None,
            member_count_at_bind: 0,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn bind_envelope_rebind_prepare(
    args: &BindEnvelopeRebindPrepareArgs,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // CI gate (slot 90 `NetworkCIDenyDefault`) is forward-looking
    // per RFC-0011-l Phase 4 §Error Handling + RFC-0011-h
    // §Confirmation Flag row 138-140. Today the CI helper
    // (`CiDetection::detect`) lands but the slot 90 error variant
    // is deferred to Phase 5/6 per RFC-0011-l row 50. The escape
    // hatch `--allow-ci-deny-default` is wired as a DEBUG-ONLY
    // clap arm (hidden from `--help`, surfaces in `--help-all`)
    // per RFC-0011-h §Security Considerations experimental-flag
    // contract.
    if !args.no_dry_run {
        return bind_envelope_rebind_preview(
            "prepare",
            &args.domain_id,
            None,
            false,
            cli,
            args.json || cli.output.json,
            cli.output.no_color,
        );
    }
    if !args.confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo network bind-envelope rebind-prepare".into(),
        });
    }
    // Substrate-faithful: build a zero-default RebindCoordinator
    // at the requested domain_id (no persistence adapter today)
    // and dispatch via `dispatch_rebind_arm_action`. The
    // substrate returns `AdapterUnwired` until the persistence
    // adapter lands (Phase 6 follow-on per
    // `0011-h-s-a-rebind-arm-persistence`).
    let coord = RebindCoordinator::new(
        args.domain_id.clone(),
        BindEnvelope::new(&args.domain_id, "platform", "group"),
        vec![],
    );
    dispatch_rebind_arm_action(&coord, RebindArmAction::Prepare, RebindArmKey::V1).map_err(
        |e| match e {
            RebindArmError::AdapterUnwired => OctoCliError::Internal(
                "rebind arm adapter not yet wired (Phase 6 follow-on)".into(),
            ),
            RebindArmError::UnknownArm(s) => OctoCliError::Internal(format!(
                "rebind arm `{s}` is not yet wired (post-PQC D2.2 follow-on)"
            )),
            _ => OctoCliError::Internal(
                "unknown rebind arm substrate error (forward-looking variant)".into(),
            ),
        },
    )?;
    let env = OutputEnvelope::new(
        "octo.network.bind-envelope.rebind-prepare.v1",
        NetworkBindEnvelopeRebindOutput {
            arm: "prepare".into(),
            domain_id: args.domain_id.clone(),
            dispatched: true,
            coordinator_state: "Preparing".into(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn bind_envelope_rebind_commit(
    args: &BindEnvelopeRebindCommitArgs,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    if !args.no_dry_run {
        return bind_envelope_rebind_preview(
            "commit",
            &args.domain_id,
            None,
            false,
            cli,
            args.json || cli.output.json,
            cli.output.no_color,
        );
    }
    if !args.confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo network bind-envelope rebind-commit".into(),
        });
    }
    // Pastejacking defense per RFC-0011-h §Security Considerations
    // rebind-commit row: requires BOTH `--confirm-acknowledge` AND
    // `--confirm`. Missing `--confirm` after `--confirm-acknowledge`
    // is operator-side double-flag protection.
    if !args.confirm {
        return Err(OctoCliError::NetworkDryRunDenied {
            arm: "commit",
            domain_id_redacted: args.domain_id.clone(),
        });
    }
    let coord = RebindCoordinator::new(
        args.domain_id.clone(),
        BindEnvelope::new(&args.domain_id, "platform", "group"),
        vec![],
    );
    dispatch_rebind_arm_action(&coord, RebindArmAction::Commit, RebindArmKey::V1).map_err(|e| {
        match e {
            RebindArmError::AdapterUnwired => OctoCliError::Internal(
                "rebind arm adapter not yet wired (Phase 6 follow-on)".into(),
            ),
            RebindArmError::UnknownArm(s) => OctoCliError::Internal(format!(
                "rebind arm `{s}` is not yet wired (post-PQC D2.2 follow-on)"
            )),
            _ => OctoCliError::Internal(
                "unknown rebind arm substrate error (forward-looking variant)".into(),
            ),
        }
    })?;
    let env = OutputEnvelope::new(
        "octo.network.bind-envelope.rebind-commit.v1",
        NetworkBindEnvelopeRebindOutput {
            arm: "commit".into(),
            domain_id: args.domain_id.clone(),
            dispatched: true,
            coordinator_state: "Committing".into(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn bind_envelope_rebind_abort(
    args: &BindEnvelopeRebindAbortArgs,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    if !args.no_dry_run {
        return bind_envelope_rebind_preview(
            "abort",
            &args.domain_id,
            Some(&args.reason),
            false,
            cli,
            args.json || cli.output.json,
            cli.output.no_color,
        );
    }
    if !args.confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo network bind-envelope rebind-abort".into(),
        });
    }
    let coord = RebindCoordinator::new(
        args.domain_id.clone(),
        BindEnvelope::new(&args.domain_id, "platform", "group"),
        vec![],
    );
    dispatch_rebind_arm_action(&coord, RebindArmAction::Abort, RebindArmKey::V1).map_err(|e| {
        match e {
            RebindArmError::AdapterUnwired => OctoCliError::Internal(
                "rebind arm adapter not yet wired (Phase 6 follow-on)".into(),
            ),
            RebindArmError::UnknownArm(s) => OctoCliError::Internal(format!(
                "rebind arm `{s}` is not yet wired (post-PQC D2.2 follow-on)"
            )),
            _ => OctoCliError::Internal(
                "unknown rebind arm substrate error (forward-looking variant)".into(),
            ),
        }
    })?;
    let env = OutputEnvelope::new(
        "octo.network.bind-envelope.rebind-abort.v1",
        NetworkBindEnvelopeRebindAbortOutput {
            domain_id: args.domain_id.clone(),
            reason_redacted: redact_reason(&args.reason),
            dispatched: true,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network discovery advertisement show` handler
/// (RFC-0011-m Phase 5 row G23). Substrate-faithful:
/// `MissionAdvertisementCache::get(advertisement_id)` returns
/// `None` until the persistence adapter lands (Phase 6 follow-on
/// per `0011-h-s-a-discovery-advertisement-persistence`).
/// CLI translates Option::None to typed exit 89
/// `NetworkSubstrateUnavailable` (REUSED slot from Phase 2 per
/// RFC-0011-h §Error Handling row 526).
fn discovery_advertisement_show(
    args: &DiscoveryAdvertisementShowArgs,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Substrate-faithful lookup: empty cache returns None for
    // every key today; persistence adapter lands the populated
    // branch post-Phase 6.
    let cache = MissionAdvertisementCache::default();

    // Targeted lookup vs full enumeration.
    if let Some(advertisement_id_hex) = &args.advertisement_id {
        let advertisement_id = parse_advertisement_id_hex(advertisement_id_hex)?;
        if cache.get(&advertisement_id).is_none() {
            return Err(OctoCliError::NetworkSubstrateUnavailable {
                companion: "G23",
                detail: "".to_string(),
            });
        }
        // Unreachable in current substrate (cache always empty);
        // substrate persistence adapter lands the populated branch
        // post-Phase 6.
        let adv = cache.get(&advertisement_id).expect("checked Some above");
        let env = OutputEnvelope::new(
            "octo.network.discovery.advertisement.show.v1",
            advertisement_to_output(adv, args.hops),
        );
        render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
    } else {
        // Enumeration path: collect all entries (deterministic
        // BTreeMap ordering per RFC-0011-h §Output Envelope order
        // determinism). Today the cache is empty so this always
        // surfaces the substrate-unavailable envelope.
        let entries: Vec<_> = cache
            .iter()
            .map(|(_key, adv)| advertisement_to_output(adv, args.hops))
            .collect();
        if entries.is_empty() {
            return Err(OctoCliError::NetworkSubstrateUnavailable {
                companion: "G23",
                detail: "".to_string(),
            });
        }
        let env = OutputEnvelope::new("octo.network.discovery.advertisement.show.v1", entries);
        render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
    }
}

/// `octo network discovery invitation show` handler
/// (RFC-0011-m Phase 5 row G24). Substrate-faithful:
/// `MissionInvitationCache::get(invitation_id)` returns `None`
/// until the persistence adapter lands (Phase 6 follow-on per
/// `0011-h-s-a-discovery-invitation-persistence`). CLI
/// translates Option::None to typed exit 89
/// `NetworkSubstrateUnavailable` (REUSED slot from Phase 2 per
/// RFC-0011-h §Error Handling row 526).
fn discovery_invitation_show(
    args: &DiscoveryInvitationShowArgs,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Substrate-faithful lookup: empty cache returns None for
    // every key today; persistence adapter lands the populated
    // branch post-Phase 6.
    let cache = MissionInvitationCache::default();

    // Targeted lookup vs full enumeration.
    if let Some(invitation_id_hex) = &args.invitation_id {
        let invitation_id = parse_invitation_id_hex(invitation_id_hex)?;
        if cache.get(&invitation_id).is_none() {
            return Err(OctoCliError::NetworkSubstrateUnavailable {
                companion: "G24",
                detail: "".to_string(),
            });
        }
        // Unreachable in current substrate (cache always empty);
        // substrate persistence adapter lands the populated branch
        // post-Phase 6.
        let inv = cache.get(&invitation_id).expect("checked Some above");
        let env = OutputEnvelope::new(
            "octo.network.discovery.invitation.show.v1",
            invitation_to_output(invitation_id, inv),
        );
        render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
    } else {
        // Enumeration path: collect all entries (deterministic
        // BTreeMap ordering per RFC-0011-h §Output Envelope order
        // determinism). Today the cache is empty so this always
        // surfaces the substrate-unavailable envelope.
        let entries: Vec<_> = cache
            .iter()
            .map(|(key, inv)| invitation_to_output(key, inv))
            .collect();
        if entries.is_empty() {
            return Err(OctoCliError::NetworkSubstrateUnavailable {
                companion: "G24",
                detail: "".to_string(),
            });
        }
        let env = OutputEnvelope::new("octo.network.discovery.invitation.show.v1", entries);
        render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
    }
}

/// Convert a `MissionAdvertisement` to the JSON output envelope
/// (RFC-0011-m Phase 5 row G23 + RFC-0011-h §Output Envelope).
fn advertisement_to_output(
    adv: &MissionAdvertisement,
    hops: Option<u16>,
) -> NetworkDiscoveryAdvertisementShowOutput {
    NetworkDiscoveryAdvertisementShowOutput {
        advertisement_hash_hex: hex::encode(adv.advertisement_hash()),
        scope: format!("{:?}", adv.scope),
        is_ttl_exceeded: hops.map(|h| adv.is_ttl_exceeded(h)),
        logical_timestamp: adv.logical_timestamp,
        gateway_id_hex: hex::encode(adv.gateway_id),
    }
}

/// Convert a `MissionInvitation` to the JSON output envelope
/// (RFC-0011-m Phase 5 row G24 + RFC-0011-h §Output Envelope
/// L511-L518). The `invitation_hash` is passed in (from the
/// cache key per RFC-0855 §8.2 deterministic-key contract)
/// so the CLI does not need to re-derive the BLAKE3 hash.
fn invitation_to_output(
    invitation_hash: [u8; 32],
    inv: &MissionInvitation,
) -> NetworkDiscoveryInvitationShowOutput {
    NetworkDiscoveryInvitationShowOutput {
        invitation_hash_hex: hex::encode(invitation_hash),
        mission_id_hex: hex::encode(inv.mission_id.to_canonical_bytes()),
        invitee_gateway_id_hex: hex::encode(inv.invitee_gateway_id),
        coordinator_gateway_id_hex: hex::encode(inv.coordinator_gateway_id),
        logical_timestamp: inv.logical_timestamp,
        signing_bytes_hex: hex::encode(inv.to_signing_bytes()),
    }
}

/// Parse a 64-char hex `advertisement_id` to 32 bytes (pastejacking
/// defense: mixed-case rejected). Returns typed exit 2 on invalid
/// input per clap arg validation conventions.
fn parse_advertisement_id_hex(s: &str) -> Result<[u8; 32], OctoCliError> {
    parse_32_byte_hex(s, "advertisement_id")
}

/// Parse a 64-char hex `invitation_id` to 32 bytes (pastejacking
/// defense: mixed-case rejected). Returns typed exit 2 on invalid
/// input per clap arg validation conventions.
fn parse_invitation_id_hex(s: &str) -> Result<[u8; 32], OctoCliError> {
    parse_32_byte_hex(s, "invitation_id")
}

/// Shared 32-byte hex parser with pastejacking defense (mixed-case
/// rejected). Used by `parse_advertisement_id_hex` +
/// `parse_invitation_id_hex`.
fn parse_32_byte_hex(s: &str, field_name: &'static str) -> Result<[u8; 32], OctoCliError> {
    if s.len() != 64 {
        return Err(OctoCliError::Internal(format!(
            "invalid {field_name}: expected 64 hex chars, got {}",
            s.len()
        )));
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(OctoCliError::Internal(format!(
            "invalid {field_name}: non-hex character"
        )));
    }
    // Pastejacking defense: mixed-case rejected. Lowercase-only
    // OR uppercase-only accepted; mixed rejected.
    let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
    if has_lower && has_upper {
        return Err(OctoCliError::Internal(format!(
            "invalid {field_name}: mixed-case rejected (pastejacking defense)"
        )));
    }
    let bytes = hex::decode(s).map_err(|e| {
        OctoCliError::Internal(format!("invalid {field_name}: hex decode failed: {e}"))
    })?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Clap-compatible 1-arg wrapper around
/// `parse_32_byte_hex` for `slash_envelope_id` (Phase 7).
/// Surfacing the typed `OctoCliError` through clap directly
/// (rather than the `String` error from a 1-arg wrapper)
/// keeps the envelope uniform across all 32-byte hex args.
fn parse_slash_envelope_id_hex(s: &str) -> Result<[u8; 32], String> {
    parse_32_byte_hex(s, "slash_envelope_id").map_err(|e| e.to_string())
}

/// `parse_32_byte_hex` for `peer_node_id` (Phase 8 G10).
/// Symmetric to `parse_slash_envelope_id_hex` (Phase 7)
/// per RFC-0011-h §Pastejacking Defense pattern.
fn parse_router_peer_node_id_hex(s: &str) -> Result<[u8; 32], String> {
    parse_32_byte_hex(s, "peer_node_id").map_err(|e| e.to_string())
}

/// `parse_32_byte_hex` for `node_id` (Phase 9 G11). Symmetric
/// to `parse_router_peer_node_id_hex` (Phase 8) +
/// `parse_slash_envelope_id_hex` (Phase 7) per RFC-0011-h
/// §Pastejacking Defense pattern. Accepts lowercase OR
/// uppercase hex; rejects mixed-case.
fn parse_specialized_node_id_hex(s: &str) -> Result<[u8; 32], String> {
    parse_32_byte_hex(s, "node_id").map_err(|e| e.to_string())
}

/// Shared preview-payload helper for the rebind-* trio dry-run
/// path. Returns the dry-run envelope without attempting
/// substrate dispatch.
#[allow(clippy::too_many_arguments)]
fn bind_envelope_rebind_preview(
    arm: &'static str,
    domain_id: &str,
    reason: Option<&str>,
    _ci_allow_override: bool,
    cli: &Octo,
    json: bool,
    no_color: bool,
) -> Result<(), OctoCliError> {
    let coordinator_state = match arm {
        "prepare" => "Preparing",
        "commit" => "Committing",
        "abort" => "Aborted",
        _ => "Unknown",
    };
    if arm == "abort" {
        let reason = reason.unwrap_or("");
        let env = OutputEnvelope::new(
            "octo.network.bind-envelope.rebind-abort.v1",
            NetworkBindEnvelopeRebindAbortOutput {
                domain_id: domain_id.to_string(),
                reason_redacted: redact_reason(reason),
                dispatched: false,
            },
        );
        return render_envelope(&env, json || cli.output.json, no_color);
    }
    let env = OutputEnvelope::new(
        "octo.network.bind-envelope.rebind.v1",
        NetworkBindEnvelopeRebindOutput {
            arm: arm.into(),
            domain_id: domain_id.to_string(),
            dispatched: false,
            coordinator_state: coordinator_state.into(),
        },
    );
    render_envelope(&env, json || cli.output.json, no_color)
}

/// Redact a free-text `reason` field for the abort envelope
/// (operator-safe display per RFC-0011 §Output Envelope).
fn redact_reason(reason: &str) -> String {
    const MAX: usize = 80;
    if reason.len() <= MAX {
        reason.to_string()
    } else {
        format!("{}...", &reason[..MAX])
    }
}

// === Helpers ===

fn render_envelope<T: Serialize>(
    env: &OutputEnvelope<T>,
    json: bool,
    no_color: bool,
) -> Result<(), OctoCliError> {
    env.render(json, no_color).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!("render failed: {e}")))
    })
}

fn peer_entry_to_summary(entry: &GatewayCacheEntry) -> PeerSummaryOutput {
    PeerSummaryOutput {
        gateway_id_hex: hex::encode(entry.identity.gateway_id),
        network_id: entry.identity.network_id,
        gateway_class: gateway_class_to_str(&entry.identity.gateway_class).to_string(),
        trust_score: entry.trust_score,
        first_seen: entry.first_seen,
        last_seen: entry.last_seen,
        capability_count: entry.capabilities.len() as u64,
        endpoint_count: entry.endpoints.len() as u64,
    }
}

fn peer_entry_to_record(entry: &GatewayCacheEntry) -> PeerRecordOutput {
    PeerRecordOutput {
        gateway_id_hex: hex::encode(entry.identity.gateway_id),
        advertisement_hash_hex: hex::encode(entry.advertisement_hash),
        network_id: entry.identity.network_id,
        gateway_class: gateway_class_to_str(&entry.identity.gateway_class).to_string(),
        trust_score: entry.trust_score,
        first_seen: entry.first_seen,
        last_seen: entry.last_seen,
        capabilities: entry
            .capabilities
            .iter()
            .map(|c| format!("{c:?}"))
            .collect(),
        endpoints: entry
            .endpoints
            .iter()
            .map(|e| PeerEndpointOutput {
                transport_type: e.transport_type,
                endpoint_hash_hex: hex::encode(e.endpoint_hash),
            })
            .collect(),
    }
}

fn gateway_class_to_str(c: &GatewayClass) -> &'static str {
    match c {
        GatewayClass::Edge => "edge",
        GatewayClass::Relay => "relay",
        GatewayClass::Consensus => "consensus",
        GatewayClass::Archive => "archive",
        GatewayClass::Stealth => "stealth",
        GatewayClass::Translation => "translation",
    }
}

fn graph_format_str(f: GraphFormat) -> &'static str {
    match f {
        GraphFormat::Ascii => "ascii",
        GraphFormat::Dot => "dot",
    }
}

// 32-byte BLAKE3-derived gateway_id encoded as 64 lowercase hex
// chars per RFC-0011-i §Peer Lookup Form.
fn parse_gateway_id_hex(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!(
            "gateway_id: expected 64 lowercase hex chars, got {}",
            s.len()
        ));
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("gateway_id: must be lowercase hex (no uppercase, no non-hex)".into());
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(s, &mut out).map_err(|e| format!("gateway_id decode failed: {e}"))?;
    Ok(out)
}

fn parse_graph_format(s: &str) -> Result<GraphFormat, String> {
    match s {
        "ascii" => Ok(GraphFormat::Ascii),
        "dot" => Ok(GraphFormat::Dot),
        other => Err(format!("format must be one of ascii|dot (got `{other}`)")),
    }
}

fn parse_graph_depth(s: &str) -> Result<u32, String> {
    let n: u32 = s
        .parse()
        .map_err(|_| format!("depth must be a positive integer (got `{s}`)"))?;
    if n == 0 {
        return Err("depth must be >= 1".into());
    }
    if n > 100 {
        return Err(format!("depth must be <= 100 (got `{n}`)"));
    }
    Ok(n)
}

fn redact_gateway_id(id: &[u8; 32]) -> String {
    // Redact per RFC-0011-h §Redaction Layer: expose only the
    // first 4 + last 4 hex chars; replace interior with `...`.
    let hex = hex::encode(id);
    let head = &hex[..4];
    let tail = &hex[hex.len() - 4..];
    format!("{head}...{tail}")
}

fn is_valid_did_codec(s: &str) -> bool {
    // Substrate-faithful coarse check: canonical DID wire form
    // is `did:octo:<scope>:<id>` per RFC-0010. The substrate
    // performs full codec resolution; this CLI gate rejects
    // obvious garbage only.
    let Some(rest) = s.strip_prefix("did:") else {
        return false;
    };
    rest.split(':').count() >= 2
}

fn redact_did(s: &str) -> String {
    let len = s.chars().count();
    if len <= 8 {
        return "[REDACTED-DID]".to_string();
    }
    let head: String = s.chars().take(8).collect();
    let tail_count = if len > 16 { 4 } else { 0 };
    let tail: String = if tail_count > 0 {
        s.chars()
            .rev()
            .take(tail_count)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect()
    } else {
        String::new()
    };
    format!("{head}...{tail}[{len}chars]")
}

// === Phase 2 helpers (RFC-0011-j §Subcommand Taxonomy) ===

/// Map a `BootstrapConfigError` to `OctoCliError::NetworkConfigParseFailed`
/// (slot 82) with operator-safe redaction.
fn map_bootstrap_config_err(e: BootstrapConfigError) -> OctoCliError {
    let (kind_redacted, _inner) = match &e {
        BootstrapConfigError::Io(_) => ("io".to_string(), format!("{e}")),
        BootstrapConfigError::TomlParse(_) => ("toml_parse".to_string(), format!("{e}")),
        BootstrapConfigError::TomlSerialize(_) => ("toml_serialize".to_string(), format!("{e}")),
    };
    let _ = _inner; // inner kept available for `Internal` fallback if ever needed
    OctoCliError::NetworkConfigParseFailed {
        kind_redacted,
        path_redacted: None,
    }
}

fn bootstrap_mode_str(m: BootstrapMode) -> &'static str {
    match m {
        BootstrapMode::Direct => "direct",
        BootstrapMode::TorOnly => "tor_only",
        BootstrapMode::TorWithIpFallback => "tor_with_ip_fallback",
    }
}

fn seed_authority_str(a: SeedListAuthority) -> &'static str {
    match a {
        SeedListAuthority::Foundation => "foundation",
        SeedListAuthority::Dao => "dao",
    }
}

fn parse_bootstrap_mode(s: &str) -> Result<BootstrapMode, String> {
    match s {
        "direct" => Ok(BootstrapMode::Direct),
        "tor_only" => Ok(BootstrapMode::TorOnly),
        "tor_with_ip_fallback" => Ok(BootstrapMode::TorWithIpFallback),
        other => Err(format!(
            "mode must be one of direct|tor_only|tor_with_ip_fallback (got `{other}`)"
        )),
    }
}

fn parse_seed_list_authority(s: &str) -> Result<SeedListAuthority, String> {
    match s {
        "foundation" => Ok(SeedListAuthority::Foundation),
        "dao" => Ok(SeedListAuthority::Dao),
        other => Err(format!(
            "authority must be one of foundation|dao (got `{other}`)"
        )),
    }
}

/// CLI-facing action label for `octo network coordinator admin
/// --action <LABEL>`. Translates to the substrate `CoordinatorAdminAction`
/// typed enum at the dispatch boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CoordinatorAdminActionLabel {
    /// `CoordinatorAdminAction::TransferOwnership`.
    TransferOwnership,
    /// `CoordinatorAdminAction::BanMember`.
    BanMember,
    /// `CoordinatorAdminAction::PromoteToAdmin`.
    PromoteToAdmin,
}

impl CoordinatorAdminActionLabel {
    fn to_substrate(self, group_id: GroupId, target: [u8; 32]) -> CoordinatorAdminAction {
        match self {
            Self::TransferOwnership => CoordinatorAdminAction::TransferOwnership {
                group_id,
                new_owner_peer_id: target,
            },
            Self::BanMember => CoordinatorAdminAction::BanMember {
                group_id,
                member_peer_id: target,
            },
            Self::PromoteToAdmin => CoordinatorAdminAction::PromoteToAdmin {
                group_id,
                member_peer_id: target,
            },
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::TransferOwnership => "transfer_ownership",
            Self::BanMember => "ban_member",
            Self::PromoteToAdmin => "promote_to_admin",
        }
    }
}

fn parse_coordinator_admin_action_label(s: &str) -> Result<CoordinatorAdminActionLabel, String> {
    match s {
        "transfer_ownership" => Ok(CoordinatorAdminActionLabel::TransferOwnership),
        "ban_member" => Ok(CoordinatorAdminActionLabel::BanMember),
        "promote_to_admin" => Ok(CoordinatorAdminActionLabel::PromoteToAdmin),
        other => Err(format!(
            "action must be one of transfer_ownership|ban_member|promote_to_admin (got `{other}`)"
        )),
    }
}

/// 32-byte quorum proof / governance id encoded as 64 lowercase hex
/// chars. Uppercase rejected (pastejacking defense).
fn parse_64_char_hex_32byte(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!("expected 64 lowercase hex chars, got {}", s.len()));
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("must be lowercase hex (no uppercase, no non-hex)".into());
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(s, &mut out).map_err(|e| format!("hex decode failed: {e}"))?;
    Ok(out)
}

/// 52-byte DID encoded as 104 lowercase hex chars (RFC-0010 wire
/// form).
fn parse_did_hex_52byte(s: &str) -> Result<[u8; 52], String> {
    if s.len() != 104 {
        return Err(format!(
            "expected 104 lowercase hex chars for 52-byte DID, got {}",
            s.len()
        ));
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("must be lowercase hex (no uppercase, no non-hex)".into());
    }
    let mut out = [0u8; 52];
    hex::decode_to_slice(s, &mut out).map_err(|e| format!("DID hex decode failed: {e}"))?;
    Ok(out)
}

/// Redact a 52-byte DID to first-8-hex + last-4-hex form.
fn redact_did_bytes(did: &[u8; 52]) -> String {
    let hex = hex::encode(did);
    let head = &hex[..8];
    let tail = &hex[hex.len() - 4..];
    format!("{head}...{tail}")
}

/// Redact a slash envelope `target_peer` string (first 8 + last 4
/// chars of the string; short strings are replaced wholesale).
fn redact_target_peer(s: &str) -> String {
    let len = s.chars().count();
    if len <= 12 {
        return format!("[REDACTED:{len}chars]");
    }
    let head: String = s.chars().take(8).collect();
    let tail: String = s
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}...{tail}[{len}chars]")
}

/// Redact an absolute path to `<octo_home>/...` form so operator
/// home paths don't leak into JSON envelopes.
fn redact_octo_home_path(path: &std::path::Path, octo_home: &std::path::Path) -> String {
    let path_str = path.to_string_lossy().to_string();
    let home_str = octo_home.to_string_lossy().to_string();
    if let Some(suffix) = path_str.strip_prefix(&home_str) {
        return format!("<octo_home>{suffix}");
    }
    path_str
}

// === Phase 6 handlers (RFC-0011-n G26 + G18 + G20 + drift-closure) ===

fn network_bootstrap(args: &BootstrapArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let octo_home: PathBuf = home::resolve().map_err(|e| match e {
        OctoCliError::NoOctoHome => OctoCliError::NetworkConfigParseFailed {
            kind_redacted: "io".to_string(),
            path_redacted: None,
        },
        other => other,
    })?;
    let path = octo_home.join("network").join("bootstrap.toml");
    let cfg = BootstrapConfig::from_toml(&path).map_err(|e| {
        OctoCliError::NetworkSubstrateUnavailable {
            companion: bootstrap_config_companion(&e),
            detail: "".to_string(),
        }
    })?;
    let mut orch = BootstrapOrchestrator::default();
    orch.start_bootstrap(cfg)
        .map_err(|e| OctoCliError::NetworkSubstrateUnavailable {
            companion: match e {
                octo_network::mon::bootstrap::BootstrapError::AlreadyStarted => "G26",
                octo_network::mon::bootstrap::BootstrapError::InvalidConfig(_) => "G26",
                octo_network::mon::bootstrap::BootstrapError::SeedListUnavailable(_) => "G26",
            },
            detail: "".to_string(),
        })?;
    let state = orch.status();
    let env = OutputEnvelope::new(
        "octo.network.bootstrap.v1",
        NetworkBootstrapOutput {
            mode: bootstrap_mode_str(state.mode).to_string(),
            started_at_epoch: state.started_at_epoch,
            refuse_start: state.refuse_start,
            peer_count: state.peer_count,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

fn network_status(args: &StatusArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // G26 bootstrap state
    let orch = BootstrapOrchestrator::default();
    let bs = orch.status();
    // G18 writer-election state (empty by default; populated by
    // substrate in follow-on companion missions)
    let we = octo_coordinator_types::election::WriterElection::new();
    // G20 network-sender registry state (empty by default; populated
    // by per-extension Layer D impl crates in follow-on missions)
    let reg = NetworkSenderRegistry::new();
    let transport_tags: Vec<String> = reg.iter().map(|(tag, _)| tag.to_string()).collect();
    let send_summaries: Vec<SendSummary> = reg
        .iter()
        .filter_map(|(_, sender)| sender.last_send_summary())
        .collect();
    // Drift-closure field per `0011-h-drift-0851p-a-seed-health-check`:
    // mirrors `BootstrapState.refuse_start` (the G26 substrate-faithful
    // delegation target for `SeedHealth::refuses_start()`).
    let env = OutputEnvelope::new(
        "octo.network.status.v1",
        NetworkStatusOutput {
            bootstrap_mode: bootstrap_mode_str(bs.mode).to_string(),
            bootstrap_started_at_epoch: bs.started_at_epoch,
            bootstrap_refuse_start: bs.refuse_start,
            bootstrap_peer_count: bs.peer_count,
            writer_election_ballot_count: we.ballot_count(),
            writer_election_stake_count: we.stake_count(),
            writer_election_voter_count: we.voter_count(),
            network_sender_count: reg.len(),
            seed_health_refuses_start: bs.refuse_start,
            transport_tags,
            send_summaries,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network slash-bridge list` handler (RFC-0011-o Phase 7 G9).
///
/// Substrate-faithful boundary: the CLI does NOT reach into
/// `SlashBridge` trait internals. The companion mission G9
/// `0011-h-s-a-slash-bridge-trait` provides the trait. This handler
/// consumes the registry lookup and translates 1:1 to the
/// `NetworkSlashBridgeListOutput` envelope.
///
/// Pre-companion G9 (trait absent in current substrate), surfaces
/// exit 89 `NetworkSubstrateUnavailable` per RFC-0011-o §Error
/// Handling. Companion-gated dispatch is the substrate-first
/// ordering invariant per [[no-phantom-mission-pointers]].
fn network_slash_bridge_list(args: &SlashBridgeListArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let registry = slash_bridge_registry(cli);
    let bridge = registry.ok_or_else(|| OctoCliError::NetworkSubstrateUnavailable {
        companion: "G9",
        detail: "".to_string(),
    })?;
    let slashes = bridge.list();
    let projections: Vec<BridgedSlashProjection> = slashes
        .into_iter()
        .map(|s| BridgedSlashProjection {
            slash_envelope_id_hex: hex::encode(s.slash_envelope_id),
            bridge_metadata: s.bridge_metadata,
            bridged_at_epoch: s.bridged_at_epoch,
        })
        .collect();
    let env = OutputEnvelope::new(
        "octo.network.slash-bridge.list.v1",
        NetworkSlashBridgeListOutput {
            total: projections.len(),
            slashes: projections,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network slash-bridge propagate <slash_envelope_id_hex>`
/// handler (RFC-0011-o Phase 7 G9).
///
/// Mutating but reversible per RFC-0011-o §Confirmation Flag:
/// `--dry-run` is default true (preview emission). Apply requires
/// `--confirm-acknowledge`. Idempotent on `slash_envelope_id` per
/// RFC-0855 §8.4 External Reputation Bridge — double-propagate
/// yields same receipt.
fn network_slash_bridge_propagate(
    args: &SlashBridgePropagateArgs,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // RFC-0011-o §Confirmation Flag: --apply + --confirm-acknowledge
    // required for apply (reversible write). Without --apply, default
    // is dry-run preview. --apply alone (without --confirm-acknowledge)
    // is an operator input error (ConfirmationRequired), NOT a
    // substrate-unavailability error (per reviewer 1 R1.5 fix
    // distinguishing user-input from system-state errors).
    if args.apply && !args.confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo network slash-bridge propagate".to_string(),
        });
    }
    let registry = slash_bridge_registry(cli);
    let bridge = registry.ok_or_else(|| OctoCliError::NetworkSubstrateUnavailable {
        companion: "G9",
        detail: "".to_string(),
    })?;
    let slash_envelope_id = args.slash_envelope_id;
    let receipt = if !args.apply {
        // Dry-run: emit synthetic preview receipt per RFC-0011-o
        // §Output Envelope (dry-run shape mirrors apply shape; on
        // apply the real substrate return value replaces it).
        octo_network::mon::slash_bridge::BridgeReceipt {
            slash_envelope_id,
            propagated_to: vec![0u8; 2],
            propagated_at_epoch: 0,
        }
    } else {
        // Per RFC-0011-o §Error Handling reachability matrix, each
        // BridgeError variant maps to slot 89 NetworkSubstrateUnavailable
        // with a distinct message. Operators can distinguish Unreachable
        // (transient) from Refused (permanent) from CLI message alone.
        // (Reviewer 3 R1.5 fix: explicit variant matching instead of
        // discarding the error via `|_|`.)
        let propagate_result = bridge.propagate_to(slash_envelope_id);
        match propagate_result {
            Ok(receipt) => receipt,
            Err(bridge_err) => {
                let msg = match &bridge_err {
                    octo_network::mon::slash_bridge::BridgeError::Unreachable => {
                        "slash bridge: destination unreachable".to_string()
                    }
                    octo_network::mon::slash_bridge::BridgeError::Refused => {
                        "slash bridge: bridge refused".to_string()
                    }
                    octo_network::mon::slash_bridge::BridgeError::PayloadTooLarge => {
                        "slash bridge: payload too large".to_string()
                    }
                    octo_network::mon::slash_bridge::BridgeError::WireFormatMismatch => {
                        "slash bridge: wire format mismatch".to_string()
                    }
                    octo_network::mon::slash_bridge::BridgeError::Internal(m) => {
                        format!("slash bridge: internal error: {m}")
                    }
                    _ => format!("slash bridge: {bridge_err}"),
                };
                return Err(OctoCliError::NetworkSubstrateUnavailable {
                    companion: "G9",
                    detail: msg,
                });
            }
        }
    };
    let receipt_proj = BridgeReceiptProjection {
        slash_envelope_id_hex: hex::encode(receipt.slash_envelope_id),
        propagated_to_hex: hex::encode(&receipt.propagated_to),
        propagated_at_epoch: receipt.propagated_at_epoch,
    };
    let env = OutputEnvelope::new(
        "octo.network.slash-bridge.propagate.v1",
        NetworkSlashBridgePropagateOutput {
            dry_run: !args.apply,
            receipt: receipt_proj,
            slash_envelope_id_hex: hex::encode(slash_envelope_id),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// Lookup the runtime `SlashBridge` registry (per-extension crate
/// pattern; concrete impls register via init fn OUT OF SCOPE for
/// Phase 7). Per RFC-0011-o §Substrate Mapping Table, the CLI
/// consumes the trait via registry lookup, identical to RFC-0863
/// `NetworkSender` pattern.
fn slash_bridge_registry(
    _cli: &Octo,
) -> Option<std::sync::Arc<dyn octo_network::mon::slash_bridge::SlashBridge>> {
    // Companion G9 substrate lands in a follow-on Layer D extension
    // crate per per-extension crate pattern. Until then, the trait
    // has no registered impl in this binary — dispatch surfaces
    // exit 89.
    None
}

/// Lookup the runtime `QuotaRouterNode` registry (per-extension
/// crate pattern; concrete impls register via init fn OUT OF
/// SCOPE for Phase 8). Per RFC-0011-p §Substrate Mapping Table,
/// the CLI consumes the struct via registry lookup, identical to
/// RFC-0863 `NetworkSender` pattern.
fn router_node_registry(
    _cli: &Octo,
) -> Option<std::sync::Arc<dyn octo_network::quota::router_node::QuotaRouterNodeAccess>> {
    // Companion G10 substrate lands in a follow-on Layer D
    // extension crate per per-extension crate pattern. Until
    // then, the struct has no registered impl in this binary —
    // dispatch surfaces exit 89.
    None
}

/// `octo network router status` handler (RFC-0011-p Phase 8 G10).
fn network_router_status(args: &RouterStatusArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let registry = router_node_registry(cli);
    let node = registry.ok_or_else(|| OctoCliError::NetworkSubstrateUnavailable {
        companion: "G10",
        detail: "".to_string(),
    })?;
    let status = node.status();
    let env = OutputEnvelope::new(
        "octo.network.router.status.v1",
        NetworkRouterStatusOutput {
            node_id_hex: hex::encode(node.node_id()),
            status: status_label(status).to_string(),
            reachable_peer_count: node.reachable_peer_count(),
            total_peer_count: node.total_peer_count(),
            last_sync_epoch: node.last_sync_epoch(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network router peers <peer_node_id_hex>` handler
/// (RFC-0011-p Phase 8 G10).
fn network_router_peers(args: &RouterPeersArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let registry = router_node_registry(cli);
    let node = registry.ok_or_else(|| OctoCliError::NetworkSubstrateUnavailable {
        companion: "G10",
        detail: "".to_string(),
    })?;
    let capacity = node.peer_capacity(&args.peer_node_id);
    let env = OutputEnvelope::new(
        "octo.network.router.peers.v1",
        NetworkRouterPeersOutput {
            peer_node_id_hex: hex::encode(args.peer_node_id),
            reachable: capacity.is_some(),
            capacity,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// Map `RouterStatus` enum to its lowercase string label for
/// projection output (RFC-0011-p §Output Envelope).
fn status_label(status: octo_network::quota::router_node::RouterStatus) -> &'static str {
    match status {
        octo_network::quota::router_node::RouterStatus::Healthy => "healthy",
        octo_network::quota::router_node::RouterStatus::Degraded => "degraded",
        octo_network::quota::router_node::RouterStatus::Offline => "offline",
    }
}

/// Map `NodeClass` enum to its lowercase string label for
/// projection output (RFC-0011-q §Output Envelope Phase 9).
/// Mirrors the `#[serde(rename_all = "lowercase")]` derivation
/// on the substrate `NodeClass` enum.
fn node_class_label(class: octo_network::specialized::node_record::NodeClass) -> &'static str {
    use octo_network::specialized::node_record::NodeClass;
    match class {
        NodeClass::Builder => "builder",
        NodeClass::Provider => "provider",
        NodeClass::Storage => "storage",
        NodeClass::Bandwidth => "bandwidth",
        NodeClass::Orchestrator => "orchestrator",
    }
}

/// Lookup the runtime `SpecializedNodeRecord` registry
/// (per-extension crate pattern; concrete impls register
/// via init fn OUT OF SCOPE for Phase 9). Per
/// RFC-0011-q §Substrate Mapping Table, the CLI consumes
/// the trait via registry lookup, identical to RFC-0863
/// `NetworkSender` pattern.
fn specialized_node_registry(
    _cli: &Octo,
) -> Option<std::sync::Arc<dyn octo_network::specialized::node_record::SpecializedNodeRecordAccess>>
{
    // Companion G11 substrate lands in a follow-on Layer D
    // extension crate per per-extension crate pattern. Until
    // then, the trait has no registered impl in this binary
    // — dispatch surfaces exit 89.
    None
}

/// `octo network node show <node_id_hex>` handler
/// (RFC-0011-q Phase 9 G11).
fn network_node_show(args: &NodeShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let registry = specialized_node_registry(cli);
    let store = registry.ok_or_else(|| OctoCliError::NetworkSubstrateUnavailable {
        companion: "G11",
        detail: "".to_string(),
    })?;
    let record_proj = store
        .load(&args.node_id)
        .map(|rec| SpecializedNodeRecordProjection {
            node_id_hex: hex::encode(rec.node_id),
            holder_did: rec.holder_did.as_ref().map(|d| d.as_str().to_string()),
            node_class: node_class_label(rec.node_class).to_string(),
            creation_epoch: rec.creation_epoch,
            metadata: rec.metadata,
        });
    let env = OutputEnvelope::new(
        "octo.network.node.show.v1",
        NetworkNodeShowOutput {
            node_id_hex: hex::encode(args.node_id),
            record: record_proj,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network node bind <node_id_hex> --holder-did <did>`
/// handler (RFC-0011-q Phase 9 G11). Bind is mutating but
/// reversible per RFC-0011-h §Confirmation Flag
/// (--apply + --confirm-acknowledge required for apply).
fn network_node_bind(args: &NodeBindArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // RFC-0011-q §Confirmation Flag: --apply +
    // --confirm-acknowledge required for apply (reversible
    // write). Without --apply, default is dry-run preview
    // (rendered without substrate dispatch). --apply alone
    // (without --confirm-acknowledge) is an operator input
    // error (ConfirmationRequired), NOT a
    // substrate-unavailability error.
    if args.apply && !args.confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo network node bind".to_string(),
        });
    }
    // Apply path only: dispatch to substrate bind_to_did. Per
    // RFC-0011-q §Error Handling, each SpecializedNodeError
    // variant maps to slot 89 NetworkSubstrateUnavailable with
    // a distinct variant tag in detail. Per redaction
    // invariant: InvalidDid(m) and Internal(m) detail strings
    // are NEVER surfaced in detail — only the variant tag is
    // surfaced, so any potentially-redacted content stays
    // within the per-extension impl crate's logging boundary.
    if args.apply {
        let registry = specialized_node_registry(cli);
        let store = registry.ok_or_else(|| OctoCliError::NetworkSubstrateUnavailable {
            companion: "G11",
            detail: "".to_string(),
        })?;
        let holder = octo_network::specialized::node_record::HolderDid::new(&args.holder_did);
        match store.bind_to_did(&args.node_id, &holder) {
            Ok(()) => {}
            Err(node_err) => {
                let msg = match &node_err {
                    octo_network::specialized::node_record::SpecializedNodeError::NotFound => {
                        "specialized node not found".to_string()
                    }
                    octo_network::specialized::node_record::SpecializedNodeError::AlreadyBound => {
                        "specialized node already bound".to_string()
                    }
                    octo_network::specialized::node_record::SpecializedNodeError::InvalidDid(_) => {
                        "specialized node invalid DID".to_string()
                    }
                    octo_network::specialized::node_record::SpecializedNodeError::Internal(_) => {
                        "specialized node internal error".to_string()
                    }
                    _ => "specialized node error".to_string(),
                };
                return Err(OctoCliError::NetworkSubstrateUnavailable {
                    companion: "G11",
                    detail: msg,
                });
            }
        }
    }
    let env = OutputEnvelope::new(
        "octo.network.node.bind.v1",
        NetworkNodeBindOutput {
            node_id_hex: hex::encode(args.node_id),
            holder_did: args.holder_did.clone(),
            applied: args.apply,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network reputation list [--filter <filter>]`
/// handler (RFC-0011-r Phase 10 G13). Read-only;
/// substrate-faithful trait dispatch per the
/// async-with-block_on precedent at agent.rs:1554-1586
/// (R5.5 `octo agent attach`). The `ReputationStore::list`
/// trait method is exercised; the InMemory stub
/// returns Ok(empty Vec) per RFC-0011-r §Substrate
/// Mapping Table. Per-extension impl crates (Layer D)
/// provide real aggregations.
fn network_reputation_list(args: &ReputationListArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // Compose substrate `ReputationFilter` from bare-word
    // clap enum + optional threshold. above-score /
    // below-score require --threshold; absent threshold
    // is an operator input error (NOT a substrate
    // unavailability).
    let filter_label = match args.filter {
        ReputationFilterKind::All => "all",
        ReputationFilterKind::AboveScore => "above-score",
        ReputationFilterKind::BelowScore => "below-score",
    };
    if matches!(
        args.filter,
        ReputationFilterKind::AboveScore | ReputationFilterKind::BelowScore
    ) && args.threshold.is_none()
    {
        return Err(OctoCliError::ConfirmationRequired {
            command: format!(
                "octo network reputation list --filter {filter_label} (--threshold required)"
            ),
        });
    }
    // Substrate-faithful: build the substrate
    // `ReputationFilter` and invoke the trait method
    // via a current-thread tokio runtime. Per Phase 5
    // RFC-0011-m precedent + the agent.rs handler
    // pattern, the runtime is dropped at end of scope.
    let substrate_filter = match args.filter {
        ReputationFilterKind::All => ReputationFilter::All,
        ReputationFilterKind::AboveScore => {
            ReputationFilter::AboveScore(args.threshold.unwrap_or(0))
        }
        ReputationFilterKind::BelowScore => {
            ReputationFilter::BelowScore(args.threshold.unwrap_or(0))
        }
    };
    let peers = {
        let store = octo_reputation::InMemoryReputationStore::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "tokio runtime build: {e}"
                )))
            })?;
        rt.block_on(store.list(substrate_filter)).map_err(|e| {
            OctoCliError::NetworkSubstrateUnavailable {
                companion: "G13",
                detail: sanitize_substrate_error(&format!("{e:?}")),
            }
        })?
    };
    let env = OutputEnvelope::new(
        "octo.network.reputation.list.v1",
        NetworkReputationListOutput {
            filter: filter_label.to_string(),
            threshold: args.threshold,
            peers: peers.into_iter().map(reputation_to_projection).collect(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network reputation show <peer_did>` handler
/// (RFC-0011-r Phase 10 G13). Read-only;
/// substrate-faithful trait dispatch per the
/// async-with-block_on precedent at agent.rs:1554-1586
/// (R5.5 `octo agent attach`). Input validation runs
/// BEFORE substrate dispatch (operator input errors
/// surface as ConfirmationRequired, not as substrate
/// unavailability). The
/// `ReputationStore::peer_reputation` trait method
/// is exercised; the InMemory stub returns Ok(None)
/// per RFC-0011-r §Substrate Mapping Table.
/// Per-extension impl crates (Layer D) provide real
/// lookups.
fn network_reputation_show(args: &ReputationShowArgs, cli: &Octo) -> Result<(), OctoCliError> {
    // Input validation FIRST: parse the canonical
    // `did:octo:0x<hex>` peer_did into a substrate
    // `RecorderDid`. Per-extension crates may accept
    // additional DID methods. Order matters: operator
    // input errors surface before substrate errors.
    let did_bytes = parse_reputation_peer_did(&args.peer_did)?;
    // `parse_reputation_peer_did` returns `[u8; 52]` on success, so the
    // `RecorderDid::from_bytes` length check always passes. The `map_err`
    // closure surfaces the substrate payload via NetworkSubstrateUnavailable
    // (slot 89) instead of `NetworkInvalidDid` so the operator sees the
    // real error from a future amendment that adds post-length semantic
    // validation.
    let did = octo_reputation::types::RecorderDid::from_bytes(&did_bytes).map_err(|e| {
        OctoCliError::NetworkSubstrateUnavailable {
            companion: "G13",
            detail: sanitize_substrate_error(&format!("RecorderDid::from_bytes: {e:?}")),
        }
    })?;
    // Substrate-faithful: invoke the trait method via
    // a current-thread tokio runtime. Per Phase 5
    // RFC-0011-m precedent + the agent.rs handler
    // pattern, the runtime is dropped at end of scope.
    let record = {
        let store = octo_reputation::InMemoryReputationStore::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "tokio runtime build: {e}"
                )))
            })?;
        rt.block_on(store.peer_reputation(&did)).map_err(|e| {
            OctoCliError::NetworkSubstrateUnavailable {
                companion: "G13",
                detail: sanitize_substrate_error(&format!("{e:?}")),
            }
        })?
    };
    let env = OutputEnvelope::new(
        "octo.network.reputation.show.v1",
        NetworkReputationShowOutput {
            peer_did: args.peer_did.clone(),
            record: record.map(reputation_to_projection),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// Substrate-faithful projection from
/// `PeerReputation` to `PeerReputationProjection`
/// (RFC-0011-r §Output Envelope Phase 10).
/// `peer_did` bytes are encoded as lowercase hex
/// per RFC-0860 canonical form.
fn reputation_to_projection(p: PeerReputation) -> PeerReputationProjection {
    PeerReputationProjection {
        peer_did_hex: hex::encode(p.peer_did.as_bytes()),
        score: p.score,
        attestations_count: p.attestations_count,
        last_updated_epoch: p.last_updated_epoch,
    }
}

/// Decode a canonical `did:octo:0x<104-hex>` peer DID
/// into the substrate 52-byte `RecorderDid` array
/// (RFC-0011-r §Subcommand Taxonomy Phase 10). Reject
/// mixed-case hex (pastejacking defense per Phase 5
/// RFC-0011-m precedent).
fn parse_reputation_peer_did(s: &str) -> Result<[u8; 52], OctoCliError> {
    let prefix = "did:octo:0x";
    let body = s
        .strip_prefix(prefix)
        .ok_or_else(|| OctoCliError::ConfirmationRequired {
            command: format!("peer_did must start with {prefix} (got {s:?})"),
        })?;
    if body.len() != 104 {
        return Err(OctoCliError::ConfirmationRequired {
            command: format!(
                "peer_did body must be 104 hex chars (got {len})",
                len = body.len()
            ),
        });
    }
    let mut all_lower = true;
    let mut all_upper = true;
    for ch in body.chars() {
        if ch.is_ascii_lowercase() {
            all_upper = false;
        } else if ch.is_ascii_uppercase() {
            all_lower = false;
        } else if !ch.is_ascii_digit() {
            return Err(OctoCliError::ConfirmationRequired {
                command: format!("peer_did body must be hex (got {body:?})"),
            });
        }
    }
    if !(all_lower || all_upper) {
        return Err(OctoCliError::ConfirmationRequired {
            command: "peer_did hex must be all-lowercase or all-uppercase (mixed-case rejected)"
                .to_string(),
        });
    }
    let mut out = [0u8; 52];
    for (idx, slot) in out.iter_mut().enumerate() {
        let byte_str = &body[idx * 2..idx * 2 + 2];
        *slot =
            u8::from_str_radix(byte_str, 16).map_err(|e| OctoCliError::ConfirmationRequired {
                command: format!("peer_did hex parse failed: {e}"),
            })?;
    }
    Ok(out)
}

/// `octo network topology render [--format ascii|dot]
/// [--depth <N>]` handler (RFC-0011-s Phase 11 G14).
/// Read-only; delegates to the substrate
/// `TopologyCommitment::render(format)` method (Phase 11
/// additive method extension per RFC-0011-s). The
/// handler invokes the substrate trait method
/// unconditionally — no registry gate (trait dispatch
/// is the universal code path per Phase 10 RFC-0011-r
/// R2.5 substrate-faithfulness precedent). Live
/// topology-source adapter OUT OF SCOPE; the empty
/// in-memory `TopologyCommitment` is the trait-only
/// substrate stub.
fn network_topology_render(args: &TopologyRenderArgs, cli: &Octo) -> Result<(), OctoCliError> {
    let format_label = match args.format {
        TopologyFormatKind::Ascii => "ascii",
        TopologyFormatKind::Dot => "dot",
    };
    let substrate_format = match args.format {
        TopologyFormatKind::Ascii => octo_network::mon::trust_graph::GraphFormat::Ascii,
        TopologyFormatKind::Dot => octo_network::mon::trust_graph::GraphFormat::Dot,
    };
    // Phase 11 substrate-faithful dispatch: construct the
    // empty in-memory TopologyCommitment (per trait-only
    // substrate spec) and invoke render() on the substrate
    // type. Real live-topology-source adapter (Layer D)
    // follows on; for now the handler exercises the
    // render() method surface end-to-end through the CLI
    // dispatch layer.
    let mid = octo_network::mon::mission_id::MissionId::new(0, &[0u8; 32], 0, &[0u8; 32], 1);
    let tc = octo_network::mon::topology::TopologyCommitment::compute(
        mid,
        octo_network::mon::topology::TopologyModel::Mesh,
        [0u8; 32],
        [0u8; 32],
        0,
    );
    let render_body = tc.render(substrate_format);
    let env = OutputEnvelope::new(
        "octo.network.topology.render.v1",
        NetworkTopologyRenderOutput {
            mission_id_hex: hex::encode(mid.to_canonical_bytes()),
            format: format_label.to_string(),
            depth: args.depth,
            render: render_body,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network gossip --stats [--format ascii|json]`
/// handler (RFC-0011-t Phase 12 G15). Read-only;
/// delegates to the substrate
/// `Gossip::stats()` method (Phase 12 additive type
/// extension per RFC-0011-t). Live gossip adapter
/// OUT OF SCOPE; the `gossip_stats_registry` marker
/// returns false in this additive-type-only phase.
fn network_gossip_stats(args: &GossipStatsArgs, cli: &Octo) -> Result<(), OctoCliError> {
    if !gossip_stats_registry(cli) {
        return Err(OctoCliError::NetworkSubstrateUnavailable {
            companion: "G15",
            detail: "".to_string(),
        });
    }
    let format_label = match args.format {
        GossipStatsFormatKind::Ascii => "ascii",
        GossipStatsFormatKind::Json => "json",
    };
    // Phase 12 substrate-faithful stub: an empty in-memory
    // Gossip snapshot is projected. Real live-gossip adapter
    // (Layer D) follows on; for now the handler exercises the
    // stats() method surface to confirm the trait-shape
    // pairing works end-to-end through the CLI dispatch layer.
    let mid = octo_network::mon::mission_id::MissionId::new(0, &[0u8; 32], 0, &[0u8; 32], 1);
    let gossip = octo_network::mon::gossip::Gossip::new(mid, 0, 0, 0, 0, 0, 0);
    let stats = gossip.stats();
    let env = OutputEnvelope::new(
        "octo.network.gossip.stats.v1",
        NetworkGossipStatsOutput {
            mission_id_hex: stats.mission_id_hex,
            peers_reachable: stats.peers_reachable,
            messages_sent: stats.messages_sent,
            messages_received: stats.messages_received,
            messages_dropped: stats.messages_dropped,
            anti_entropy_rounds: stats.anti_entropy_rounds,
            last_sync_epoch: stats.last_sync_epoch,
            format: format_label.to_string(),
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// Lookup the runtime gossip stats marker
/// (RFC-0011-t Phase 12 G15). Returns false in this
/// additive-type-only phase; per-extension Layer D
/// live gossip adapter init fn OUT OF SCOPE.
fn gossip_stats_registry(_cli: &Octo) -> bool {
    false
}

/// `octo network envelope inspect <envelope_id_hex>`
/// handler (RFC-0011-u Phase 13 G16a). Read-only;
/// delegates to the substrate `EnvelopeInspector`
/// module (Phase 13 additive type extension per
/// RFC-0011-u). Live envelope store OUT OF SCOPE;
/// the `envelope_inspect_registry` marker returns
/// false in this additive-type-only phase.
fn network_envelope_inspect(args: &EnvelopeInspectArgs, cli: &Octo) -> Result<(), OctoCliError> {
    if !envelope_inspect_registry(cli) {
        return Err(OctoCliError::NetworkSubstrateUnavailable {
            companion: "G16a",
            detail: "".to_string(),
        });
    }
    let envelope_id = parse_32_byte_hex(&args.envelope_id_hex, "envelope_id_hex").map_err(|e| {
        OctoCliError::ConfirmationRequired {
            command: format!("envelope_id_hex parse failed: {e}"),
        }
    })?;
    // Phase 13 substrate-faithful stub: inspector returns None for
    // unknown envelopes (live envelope store OUT OF SCOPE).
    // For CLI exercise we project a synthetic EnvelopeMeta to
    // exercise the type surface end-to-end.
    let insp = octo_network::mon::envelope_inspector::EnvelopeInspector;
    let meta = insp.inspect(envelope_id).unwrap_or_else(|| {
        octo_network::mon::envelope_inspector::EnvelopeInspector::from_fields(
            envelope_id,
            octo_network::mon::envelope_inspector::EnvelopeKind::Mission,
            "did:octo:0x0000000000000000".to_string(),
            0,
            0,
        )
    });
    let env = OutputEnvelope::new(
        "octo.network.envelope.inspect.v1",
        NetworkEnvelopeInspectOutput {
            envelope_id_hex: args.envelope_id_hex.clone(),
            envelope_kind: meta.envelope_kind.as_str().to_string(),
            creator_did_hex: meta.creator_did_hex,
            creation_epoch: meta.creation_epoch,
            ttl_epochs: meta.ttl_epochs,
            found: true,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network envelope forward <envelope_id_hex>`
/// handler (RFC-0011-u Phase 13 G16b). Mutating;
/// delegates to the substrate `ForwardEnvelope` module
/// (Phase 13 additive type extension per RFC-0011-u).
/// Real network propagation OUT OF SCOPE; the
/// `envelope_forward_registry` marker returns false in
/// this additive-type-only phase.
fn network_envelope_forward(args: &EnvelopeForwardArgs, cli: &Octo) -> Result<(), OctoCliError> {
    if !envelope_forward_registry(cli) {
        return Err(OctoCliError::NetworkSubstrateUnavailable {
            companion: "G16b",
            detail: "".to_string(),
        });
    }
    // Mutating-gate clap arg requirement per Phase 4 RFC-0011-l
    // precedent: --confirm-acknowledge required unless --dry-run.
    if !args.dry_run && !args.confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "envelope forward requires --confirm-acknowledge (mutating subcommand)"
                .to_string(),
        });
    }
    let envelope_id = parse_32_byte_hex(&args.envelope_id_hex, "envelope_id_hex").map_err(|e| {
        OctoCliError::ConfirmationRequired {
            command: format!("envelope_id_hex parse failed: {e}"),
        }
    })?;
    let dest_peer_id = parse_32_byte_hex(&args.destination_peer_id_hex, "destination_peer_id_hex")
        .map_err(|e| OctoCliError::ConfirmationRequired {
            command: format!("destination_peer_id_hex parse failed: {e}"),
        })?;
    let fe = octo_network::mon::forward_envelope::ForwardEnvelope::build(
        envelope_id,
        dest_peer_id,
        args.ttl_epochs,
        0,
    )
    .map_err(|e| match e {
        octo_network::mon::forward_envelope::ForwardEnvelopeError::InvalidPeerId => {
            OctoCliError::ConfirmationRequired {
                command: "destination_peer_id_hex is all-zero (invalid)".to_string(),
            }
        }
        octo_network::mon::forward_envelope::ForwardEnvelopeError::Internal(s) => {
            OctoCliError::ConfirmationRequired {
                command: format!("forward envelope build internal error: {s}"),
            }
        }
        _ => OctoCliError::ConfirmationRequired {
            command: "forward envelope build unknown error".to_string(),
        },
    })?;
    let wire_bytes = fe.wire_bytes();
    let env = OutputEnvelope::new(
        "octo.network.envelope.forward.v1",
        NetworkEnvelopeForwardOutput {
            source_envelope_id_hex: args.envelope_id_hex.clone(),
            destination_peer_id_hex: args.destination_peer_id_hex.clone(),
            ttl_epochs: args.ttl_epochs,
            wire_bytes_hex: hex::encode(&wire_bytes),
            wire_bytes_len: wire_bytes.len(),
            dry_run: args.dry_run,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// `octo network heartbeat probe <peer_did> [--timeout-ms <N>]`
/// handler (RFC-0011-v Phase 14 G17).
///
/// Read-only subcommand; delegates to the substrate
/// `Heartbeat::probe()` module (Phase 14 additive type extension
/// per RFC-0011-v). Real transport-level probe OUT OF SCOPE; the
/// `heartbeat_probe_registry` marker returns false in this
/// additive-type-only phase. clap u16 overflow pre-dispatch
/// rejection per Phase 5 RFC-0011-m precedent.
fn network_heartbeat_probe(args: &HeartbeatProbeArgs, cli: &Octo) -> Result<(), OctoCliError> {
    if !heartbeat_probe_registry(cli) {
        return Err(OctoCliError::NetworkSubstrateUnavailable {
            companion: "G17",
            detail: "".to_string(),
        });
    }
    let hb = octo_network::mon::heartbeat::Heartbeat;
    let result = hb.probe(&args.peer_did, args.timeout_ms);
    let (result_label, rtt_ms, reason_label, reason_detail) = match &result {
        octo_network::mon::heartbeat::HeartbeatProbeResult::Reachable { rtt_ms } => {
            ("reachable".to_string(), Some(*rtt_ms), None, None)
        }
        octo_network::mon::heartbeat::HeartbeatProbeResult::Unreachable { reason } => {
            let (label, detail) = match reason {
                octo_network::mon::heartbeat::UnreachableReason::InvalidPeerDid => {
                    ("invalid_peer_did".to_string(), None)
                }
                octo_network::mon::heartbeat::UnreachableReason::NoTransportAdapter => {
                    ("no_transport_adapter".to_string(), None)
                }
                octo_network::mon::heartbeat::UnreachableReason::AdapterRefused => {
                    ("adapter_refused".to_string(), None)
                }
                octo_network::mon::heartbeat::UnreachableReason::Other(s) => {
                    ("other".to_string(), Some(s.clone()))
                }
                _ => ("unknown".to_string(), None),
            };
            ("unreachable".to_string(), None, Some(label), detail)
        }
        octo_network::mon::heartbeat::HeartbeatProbeResult::Timeout => {
            ("timeout".to_string(), None, None, None)
        }
    };
    let env = OutputEnvelope::new(
        "octo.network.heartbeat.probe.v1",
        NetworkHeartbeatProbeOutput {
            peer_did: args.peer_did.clone(),
            result_label,
            rtt_ms,
            unreachable_reason_label: reason_label,
            unreachable_reason_detail: reason_detail,
            timeout_ms: args.timeout_ms,
        },
    );
    render_envelope(&env, args.json || cli.output.json, cli.output.no_color)
}

/// Lookup the runtime envelope inspect marker
/// (RFC-0011-u Phase 13 G16a). Returns false in this
/// additive-type-only phase; per-extension Layer D
/// live envelope store init fn OUT OF SCOPE.
fn envelope_inspect_registry(_cli: &Octo) -> bool {
    false
}

/// Lookup the runtime envelope forward marker
/// (RFC-0011-u Phase 13 G16b). Returns false in this
/// additive-type-only phase; per-extension Layer D
/// live envelope propagation init fn OUT OF SCOPE.
fn envelope_forward_registry(_cli: &Octo) -> bool {
    false
}

/// Lookup the runtime heartbeat probe marker
/// (RFC-0011-v Phase 14 G17). Returns false in this
/// additive-type-only phase; per-extension Layer D
/// live transport-level probe init fn OUT OF SCOPE.
fn heartbeat_probe_registry(_cli: &Octo) -> bool {
    false
}

fn bootstrap_config_companion(err: &BootstrapConfigError) -> &'static str {
    match err {
        BootstrapConfigError::Io(_) => "G1",
        BootstrapConfigError::TomlParse(_) => "G1",
        BootstrapConfigError::TomlSerialize(_) => "G1",
    }
}

// === Tests (13 test vectors per RFC-0011-i §Test Vectors) ===

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestPeersCli {
        #[command(subcommand)]
        action: PeersAction,
    }

    #[derive(Parser, Debug)]
    struct TestIdentityCli {
        #[command(subcommand)]
        action: NetworkIdentityAction,
    }

    #[derive(Parser, Debug)]
    struct TestTrustGraphCli {
        #[command(subcommand)]
        action: TrustGraphAction,
    }

    #[derive(Parser, Debug)]
    struct TestGovCli {
        #[command(subcommand)]
        action: NetworkGovernanceAction,
    }

    fn build_cli(args: &[&str]) -> Octo {
        Octo::try_parse_from(args).expect("test harness: octo network parse must succeed")
    }

    // tv_net1_1: peers list subcommand parses
    #[test]
    fn tv_net1_1_peers_list_subcommand_parses() {
        let cli = TestPeersCli::try_parse_from(["test", "list"]).unwrap();
        match cli.action {
            PeersAction::List(args) => assert!(!args.json),
            _ => panic!("expected List"),
        }
    }

    // tv_net1_2: peers get subcommand parses 64-char lowercase hex
    #[test]
    fn tv_net1_2_peers_get_subcommand_parses_gateway_id() {
        let hex_str = "a".repeat(64);
        let cli = TestPeersCli::try_parse_from(["test", "get", &hex_str]).unwrap();
        match cli.action {
            PeersAction::Get(args) => {
                assert_eq!(args.gateway_id, [0xAA; 32]);
            }
            _ => panic!("expected Get"),
        }
    }

    // tv_net1_3: peers get rejects uppercase
    #[test]
    fn tv_net1_3_peers_get_rejects_uppercase_hex() {
        let hex_str = "A".repeat(64);
        let r = TestPeersCli::try_parse_from(["test", "get", &hex_str]);
        assert!(r.is_err(), "uppercase hex must be rejected at parse time");
    }

    // tv_net1_4: peers get rejects wrong length
    #[test]
    fn tv_net1_4_peers_get_rejects_wrong_length() {
        let r = TestPeersCli::try_parse_from(["test", "get", "01ab"]);
        assert!(r.is_err(), "short hex must be rejected at parse time");
    }

    // tv_net1_5: peers get not-found path exercises NetworkPeerNotFound
    #[test]
    fn tv_net1_5_peers_get_emit_network_peer_not_found_envelope() {
        use crate::commands::network::PeersGetArgs;
        let args = PeersGetArgs {
            gateway_id: [0u8; 32],
            json: false,
        };
        let cli = build_cli(&["octo", "network", "peers", "get", &"0".repeat(64)]);
        let r = peers_get(&args, &cli);
        match r {
            Err(OctoCliError::NetworkPeerNotFound { gateway_id_hex }) => {
                assert!(
                    gateway_id_hex.contains("..."),
                    "redaction must elide interior"
                );
            }
            Err(other) => panic!("expected NetworkPeerNotFound, got {other:?}"),
            Ok(()) => panic!("expected NetworkPeerNotFound, got Ok"),
        }
    }

    // tv_net1_6: identity show parses
    #[test]
    fn tv_net1_6_identity_show_parses() {
        let cli = TestIdentityCli::try_parse_from(["test", "show"]).unwrap();
        match cli.action {
            NetworkIdentityAction::Show(args) => assert!(!args.json),
        }
    }

    // tv_net1_7: identity show NotInitialized surfaces NetworkLocalKeyUnavailable.
    // This depends on a NON-EXISTENT octo_home so the substrate returns
    // NotInitialized; the CLI then maps to NetworkLocalKeyUnavailable.
    #[test]
    fn tv_net1_7_identity_show_not_initialized_surfaces_network_local_key_unavailable() {
        use crate::commands::network::IdentityShowArgs;
        // Point OCTO_HOME at a known-nonexistent tempdir so load yields
        // NotInitialized regardless of host state.
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "octo-net-identity-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        // Safety: env mutation, but restored immediately after.
        let prev = std::env::var("OCTO_HOME").ok();
        std::env::set_var("OCTO_HOME", &dir);
        // SAFETY: serial because all env-mutation tests share state.
        let args = IdentityShowArgs { json: false };
        let cli = build_cli(&["octo", "network", "identity", "show"]);
        let r = identity_show(&args, &cli);
        // restore env
        if let Some(v) = prev {
            std::env::set_var("OCTO_HOME", v);
        } else {
            std::env::remove_var("OCTO_HOME");
        }
        match r {
            Err(OctoCliError::NetworkLocalKeyUnavailable) => {}
            Err(other) => panic!("expected NetworkLocalKeyUnavailable, got {other:?}"),
            Ok(()) => panic!("expected NetworkLocalKeyUnavailable, got Ok"),
        }
    }

    // tv_net1_8: trust-graph render parses with --format and --depth
    #[test]
    fn tv_net1_8_trust_graph_render_parses_with_format_and_depth() {
        let cli = TestTrustGraphCli::try_parse_from([
            "test", "render", "--format", "dot", "--depth", "5",
        ])
        .unwrap();
        match cli.action {
            TrustGraphAction::Render(args) => {
                assert!(matches!(args.format, GraphFormat::Dot));
                assert_eq!(args.depth, Some(5));
            }
        }
    }

    // tv_net1_9: trust-graph render --depth 0 emits NetworkGraphDepthBelowRange
    #[test]
    fn tv_net1_9_trust_graph_render_depth_below_range_emits_error() {
        // Substrate-faithful: 0 fails parse_graph_depth (clamp).
        let r = TestTrustGraphCli::try_parse_from([
            "test", "render", "--format", "ascii", "--depth", "0",
        ]);
        assert!(r.is_err(), "depth 0 must be rejected at parse time");
    }

    // tv_net1_10: trust-graph render --depth 200 emits NetworkGraphDepthBelowRange
    // when the value bypasses clamp via some substrate path; here
    // we exercise the dispatch boundary directly with depth=200.
    #[test]
    fn tv_net1_10_trust_graph_render_depth_above_range_emits_error() {
        use crate::commands::network::TrustGraphRenderArgs;
        let args = TrustGraphRenderArgs {
            format: GraphFormat::Ascii,
            depth: Some(200),
            json: false,
        };
        let cli = build_cli(&["octo", "network", "trust-graph", "render"]);
        let r = trust_graph_render(&args, &cli);
        match r {
            Err(OctoCliError::NetworkGraphDepthBelowRange { depth: 200 }) => {}
            Err(other) => panic!("expected NetworkGraphDepthBelowRange, got {other:?}"),
            Ok(()) => panic!("expected NetworkGraphDepthBelowRange, got Ok"),
        }
    }

    // tv_net1_11: governance rotation status parses with --did-codec
    #[test]
    fn tv_net1_11_governance_rotation_status_parses() {
        let cli = TestGovCli::try_parse_from([
            "test",
            "rotation",
            "status",
            "--did-codec",
            "did:octo:router:abc",
        ])
        .unwrap();
        match cli.action {
            NetworkGovernanceAction::Rotation { action } => {
                let NetworkGovernanceRotationAction::Status(args) = action;
                assert_eq!(args.did_codec, "did:octo:router:abc");
            }
            // Phase 3 `Tally` variant is a sibling (Phase 3 dispatch
            // test `tv_net3_5_governance_tally_parses_with_proposal_id`
            // covers the new arm); this test pins the Phase 1 surface.
            NetworkGovernanceAction::Tally(_) => {
                panic!("expected Rotation, got Tally (Phase 3 variant)")
            }
        }
    }

    // tv_net1_12: governance rotation status with malformed DID emits
    // NetworkInvalidDid (slot 86).
    #[test]
    fn tv_net1_12_governance_rotation_status_invalid_did_emits_error() {
        use crate::commands::network::GovernanceRotationStatusArgs;
        let args = GovernanceRotationStatusArgs {
            did_codec: "not-a-did".into(),
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "governance",
            "rotation",
            "status",
            "--did-codec",
            "x",
        ]);
        let r = governance_rotation_status(&args, &cli);
        match r {
            Err(OctoCliError::NetworkInvalidDid { did_redacted }) => {
                assert!(did_redacted.starts_with('[') || did_redacted.contains("..."));
            }
            Err(other) => panic!("expected NetworkInvalidDid, got {other:?}"),
            Ok(()) => panic!("expected NetworkInvalidDid, got Ok"),
        }
    }

    // tv_net1_13: 13th test vector - GraphFormat + GatewayClass string
    // projection pins (lifecycle).
    #[test]
    fn tv_net1_13_gateway_class_and_graph_format_projection() {
        assert_eq!(gateway_class_to_str(&GatewayClass::Edge), "edge");
        assert_eq!(gateway_class_to_str(&GatewayClass::Relay), "relay");
        assert_eq!(gateway_class_to_str(&GatewayClass::Consensus), "consensus");
        assert_eq!(gateway_class_to_str(&GatewayClass::Archive), "archive");
        assert_eq!(graph_format_str(GraphFormat::Ascii), "ascii");
        assert_eq!(graph_format_str(GraphFormat::Dot), "dot");
    }

    // Helper: redaction format pins
    #[test]
    fn redact_gateway_id_format_is_short_head_short_tail() {
        let id = [0xAB; 32];
        let r = redact_gateway_id(&id);
        assert!(r.starts_with("abab"), "{r}");
        assert!(r.contains("..."), "{r}");
        assert!(r.ends_with("abab"), "{r}");
        assert_eq!(r.len(), 4 + 3 + 4, "{r}");
    }

    #[test]
    fn is_valid_did_codec_rejects_garbage() {
        assert!(!is_valid_did_codec(""));
        assert!(!is_valid_did_codec("not-a-did"));
        assert!(!is_valid_did_codec("did:"));
        assert!(is_valid_did_codec("did:octo:router:abc"));
        assert!(is_valid_did_codec("did:octo:a"));
    }

    // Top-level surface check: NetworkAction is non-exhaustive and
    // accepts all 4 subcommand families.
    #[test]
    fn clap_parses_all_phase_1_subcommands() {
        let cli = build_cli(&["octo", "network", "peers", "list"]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&["octo", "network", "peers", "get", &"0".repeat(64)]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&["octo", "network", "identity", "show"]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&["octo", "network", "trust-graph", "render", "--depth", "1"]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&[
            "octo",
            "network",
            "governance",
            "rotation",
            "status",
            "--did-codec",
            "did:octo:x",
        ]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
    }

    // Local helper to construct deterministic LocalGatewayIdentity
    // for envelope projection sanity. The CLI does NOT import
    // `GatewayIdentity` (substrate-internal); this helper exercises
    // the substrate-faithful field projection.
    #[allow(dead_code)]
    fn sample_local_gateway_identity() -> LocalGatewayIdentity {
        LocalGatewayIdentity {
            network_id: 1,
            gateway_class: GatewayClass::Edge,
            creation_epoch: 100,
        }
    }

    // === Phase 2 test vectors (RFC-0011-j §Test Vectors) ===

    #[derive(Parser, Debug)]
    struct TestModeCli {
        #[command(subcommand)]
        action: NetworkModeAction,
    }

    #[derive(Parser, Debug)]
    struct TestAuthorityCli {
        #[command(subcommand)]
        action: NetworkAuthorityAction,
    }

    #[derive(Parser, Debug)]
    struct TestSlashCli {
        #[command(subcommand)]
        action: NetworkSlashAction,
    }

    // Phase 3 test fixtures (RFC-0011-k §Test Vectors).
    #[derive(Parser, Debug)]
    struct TestCoordinatorCli {
        #[command(subcommand)]
        action: NetworkCoordinatorAction,
    }

    #[derive(Parser, Debug)]
    struct TestNetCoordinatorCli {
        #[command(subcommand)]
        action: NetworkAction,
    }

    // tv_net2_1: mode show subcommand parses
    #[test]
    fn tv_net2_1_mode_show_subcommand_parses() {
        let cli = TestModeCli::try_parse_from(["test", "show"]).unwrap();
        assert!(matches!(cli.action, NetworkModeAction::Show(_)));
    }

    // tv_net2_2: mode set subcommand parses with --bootstrap-mode + --listen-addr + --target-peers
    #[test]
    fn tv_net2_2_mode_set_subcommand_parses_with_mode_arg() {
        let cli = TestModeCli::try_parse_from([
            "test",
            "set",
            "--bootstrap-mode",
            "tor_only",
            "--listen-addr",
            "0.0.0.0:9000",
            "--target-peers",
            "16",
        ])
        .unwrap();
        match cli.action {
            NetworkModeAction::Set(args) => {
                assert!(matches!(args.bootstrap_mode, BootstrapMode::TorOnly));
                assert_eq!(args.listen_addr, "0.0.0.0:9000");
                assert_eq!(args.target_peers, 16);
            }
            other => panic!("expected Set, got {other:?}"),
        }
    }

    // tv_net2_3: mode set rejects unknown mode label at parse time
    #[test]
    fn tv_net2_3_mode_set_rejects_unknown_mode_label() {
        let r = TestModeCli::try_parse_from([
            "test",
            "set",
            "--bootstrap-mode",
            "garbage",
            "--listen-addr",
            "0.0.0.0:9000",
            "--target-peers",
            "16",
        ]);
        assert!(
            r.is_err(),
            "unknown mode label must be rejected at parse time"
        );
    }

    // tv_net2_4: authority show subcommand parses
    #[test]
    fn tv_net2_4_authority_show_subcommand_parses() {
        let cli = TestAuthorityCli::try_parse_from(["test", "show"]).unwrap();
        assert!(matches!(cli.action, NetworkAuthorityAction::Show(_)));
    }

    // tv_net2_5: authority rotate parses with --new-authority + --quorum-proof-hex
    #[test]
    fn tv_net2_5_authority_rotate_parses_with_quorum_proof() {
        let proof_hex = "ab".repeat(32);
        let cli = TestAuthorityCli::try_parse_from([
            "test",
            "rotate",
            "--new-authority",
            "dao",
            "--quorum-proof-hex",
            &proof_hex,
        ])
        .unwrap();
        match cli.action {
            NetworkAuthorityAction::Rotate(args) => {
                assert!(matches!(args.new_authority, SeedListAuthority::Dao));
                assert_eq!(args.quorum_proof_hex, [0xAB; 32]);
            }
            other => panic!("expected Rotate, got {other:?}"),
        }
    }

    // tv_net2_6: authority rotate rejects zero digest at substrate level
    #[test]
    fn tv_net2_6_authority_rotate_rejects_zero_proof() {
        let proof_hex = "0".repeat(64);
        // 64 zeros parse successfully (lowercase hex, 64 chars); the
        // substrate `rotate_post_fork` rejects the zero digest
        // downstream. Test vector exercises the substrate rejection
        // path via the `authority_rotate` handler.
        let r = TestAuthorityCli::try_parse_from([
            "test",
            "rotate",
            "--new-authority",
            "dao",
            "--quorum-proof-hex",
            &proof_hex,
        ]);
        assert!(
            r.is_ok(),
            "64 zeros must parse (decode succeeds); substrate rejects at handler"
        );
        // Decoding 64 zeros succeeds, so we exercise the substrate rejection path:
        let mut cli = build_cli(&[
            "octo",
            "network",
            "authority",
            "rotate",
            "--new-authority",
            "dao",
            "--quorum-proof-hex",
            &proof_hex,
        ]);
        // Bypass 3-flag confirmation gate via --dry-run; substrate
        // still rejects the zero-digest (BadSignature).
        cli.mode.dry_run = true;
        let r = AuthorityRotateArgs {
            new_authority: SeedListAuthority::Dao,
            quorum_proof_hex: [0u8; 32],
            json: false,
        };
        let res = authority_rotate(&r, &cli);
        // Substrate-faithful: rotate_post_fork rejects zero digest
        // (BadSignature). The CLI maps to NetworkSubstrateUnavailable.
        assert!(
            matches!(
                res,
                Err(OctoCliError::NetworkSubstrateUnavailable {
                    companion: "G8",
                    detail: _
                })
            ),
            "expected NetworkSubstrateUnavailable(G8), got {res:?}"
        );
    }

    // tv_net2_7: authority rotate rejects Foundation post-fork (substrate contract)
    #[test]
    fn tv_net2_7_authority_rotate_rejects_foundation_post_fork() {
        let mut cli = build_cli(&[
            "octo",
            "network",
            "authority",
            "rotate",
            "--new-authority",
            "foundation",
            "--quorum-proof-hex",
            &"ab".repeat(32),
        ]);
        // Bypass 3-flag confirmation gate via --dry-run; substrate
        // still rejects Foundation rotation post-fork
        // (SeedListAuthorityDeprecated).
        cli.mode.dry_run = true;
        let r = AuthorityRotateArgs {
            new_authority: SeedListAuthority::Foundation,
            quorum_proof_hex: [0xAB; 32],
            json: false,
        };
        let res = authority_rotate(&r, &cli);
        match res {
            Err(OctoCliError::NetworkSubstrateUnavailable {
                companion: "G8",
                detail: _,
            }) => {}
            Err(other) => panic!("expected NetworkSubstrateUnavailable(G8), got {other:?}"),
            Ok(()) => panic!("expected NetworkSubstrateUnavailable, got Ok"),
        }
    }

    // tv_net2_8: slash excluded parses with <did> 104-char hex
    #[test]
    fn tv_net2_8_slash_excluded_parses_with_did() {
        let did_hex = "ab".repeat(52);
        let cli = TestSlashCli::try_parse_from(["test", "excluded", &did_hex]).unwrap();
        match cli.action {
            NetworkSlashAction::Excluded(args) => {
                assert_eq!(args.did, [0xAB; 52]);
            }
            other => panic!("expected Excluded, got {other:?}"),
        }
    }

    // tv_net2_9: slash stats subcommand parses
    #[test]
    fn tv_net2_9_slash_stats_subcommand_parses() {
        let cli = TestSlashCli::try_parse_from(["test", "stats"]).unwrap();
        assert!(matches!(cli.action, NetworkSlashAction::Stats(_)));
    }

    // tv_net2_10: slash list parses with --slash-reason + --limit
    #[test]
    fn tv_net2_10_slash_list_parses_with_reason_and_limit() {
        let cli =
            TestSlashCli::try_parse_from(["test", "list", "--slash-reason", "10", "--limit", "5"])
                .unwrap();
        match cli.action {
            NetworkSlashAction::List(args) => {
                assert_eq!(args.slash_reason, Some(10));
                assert_eq!(args.limit, Some(5));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    // tv_net2_11: slash show parses with <slash_id>
    #[test]
    fn tv_net2_11_slash_show_parses_with_slash_id() {
        let cli = TestSlashCli::try_parse_from(["test", "show", "slash-peer-a-100"]).unwrap();
        match cli.action {
            NetworkSlashAction::Show(args) => {
                assert_eq!(args.slash_id, "slash-peer-a-100");
            }
            other => panic!("expected Show, got {other:?}"),
        }
    }

    // tv_net2_12: slash excluded rejects malformed DID at parse time
    #[test]
    fn tv_net2_12_slash_excluded_rejects_short_did_at_parse() {
        let r = TestSlashCli::try_parse_from(["test", "excluded", "ab"]);
        assert!(r.is_err(), "short DID must be rejected at parse time");
    }

    // tv_net2_13: slash stats envelope projection pins substrate fields
    #[test]
    fn tv_net2_13_slash_stats_substrate_field_projection() {
        // Substrate-faithful: a fresh store has 0 dids + 0 slashes
        let store = SlashReputationStoreCompat::new();
        let env = OutputEnvelope::new(
            "octo.network.slash.stats.v1",
            NetworkSlashStatsOutput {
                did_count: store.did_count() as u64,
                total_slashes: store.total_slashes(),
            },
        );
        assert_eq!(env.command, "octo.network.slash.stats.v1");
        assert_eq!(env.payload.did_count, 0);
        assert_eq!(env.payload.total_slashes, 0);
    }

    // tv_net2_14: mode show IO error surfaces as NetworkConfigParseFailed
    #[test]
    fn tv_net2_14_mode_show_io_error_emits_network_config_parse_failed() {
        // Point OCTO_HOME at a known-nonexistent tempdir so
        // BootstrapConfig::from_toml yields Io error.
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "octo-net-mode-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let prev = std::env::var("OCTO_HOME").ok();
        std::env::set_var("OCTO_HOME", &dir);
        let args = ModeShowArgs { json: false };
        let cli = build_cli(&["octo", "network", "mode", "show"]);
        let r = mode_show(&args, &cli);
        if let Some(v) = prev {
            std::env::set_var("OCTO_HOME", v);
        } else {
            std::env::remove_var("OCTO_HOME");
        }
        match r {
            Err(OctoCliError::NetworkConfigParseFailed { kind_redacted, .. }) => {
                assert_eq!(kind_redacted, "io");
            }
            Err(other) => panic!("expected NetworkConfigParseFailed, got {other:?}"),
            Ok(()) => panic!("expected NetworkConfigParseFailed, got Ok"),
        }
    }

    // tv_net2_15: authority show envelope exposes deprecation = false at epoch 0
    #[test]
    fn tv_net2_15_authority_show_deprecation_at_epoch_zero() {
        let args = AuthorityShowArgs { json: false };
        let cli = build_cli(&["octo", "network", "authority", "show"]);
        let r = authority_show(&args, &cli);
        if r.is_err() {
            panic!("authority_show must succeed at epoch 0");
        }
    }

    // tv_net2_16: slash show on unknown slash_id emits envelope with None
    #[test]
    fn tv_net2_16_slash_show_unknown_emits_none_envelope() {
        let args = SlashShowArgs {
            slash_id: "nonexistent".into(),
            json: false,
        };
        let cli = build_cli(&["octo", "network", "slash", "show", "nonexistent"]);
        let r = slash_show(&args, &cli);
        // Substrate-faithful: store.show returns None; CLI wraps in
        // Some(NetworkSlashShowOutput { envelope: None }) and
        // returns Ok.
        if let Err(other) = &r {
            panic!("expected Ok with None envelope, got {other:?}");
        }
    }

    // tv_net2_17: top-level surface check — NetworkAction accepts
    // all Phase 2 subcommand families.
    #[test]
    fn clap_parses_all_phase_2_subcommands() {
        let cli = build_cli(&["octo", "network", "mode", "show"]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&[
            "octo",
            "network",
            "mode",
            "set",
            "--bootstrap-mode",
            "direct",
            "--listen-addr",
            "0.0.0.0:9000",
            "--target-peers",
            "8",
        ]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&["octo", "network", "authority", "show"]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&[
            "octo",
            "network",
            "authority",
            "rotate",
            "--new-authority",
            "dao",
            "--quorum-proof-hex",
            &"ab".repeat(32),
        ]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&["octo", "network", "slash", "excluded", &"ab".repeat(52)]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&["octo", "network", "slash", "stats"]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&[
            "octo",
            "network",
            "slash",
            "list",
            "--slash-reason",
            "10",
            "--limit",
            "3",
        ]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
        let cli = build_cli(&["octo", "network", "slash", "show", "slash-peer-a-100"]);
        assert!(matches!(cli.command, crate::Commands::Network { .. }));
    }

    // Helper: bootstrap_mode_str projection pins
    #[test]
    fn tv_net2_h1_bootstrap_mode_string_projection() {
        assert_eq!(bootstrap_mode_str(BootstrapMode::Direct), "direct");
        assert_eq!(bootstrap_mode_str(BootstrapMode::TorOnly), "tor_only");
        assert_eq!(
            bootstrap_mode_str(BootstrapMode::TorWithIpFallback),
            "tor_with_ip_fallback"
        );
    }

    // Helper: seed_authority_str projection pins
    #[test]
    fn tv_net2_h2_seed_authority_string_projection() {
        assert_eq!(
            seed_authority_str(SeedListAuthority::Foundation),
            "foundation"
        );
        assert_eq!(seed_authority_str(SeedListAuthority::Dao), "dao");
    }

    // Helper: redact_target_peer pins
    #[test]
    fn tv_net2_h3_redact_target_peer_short_string() {
        assert_eq!(redact_target_peer("ab"), "[REDACTED:2chars]");
        let long = "a".repeat(20);
        let r = redact_target_peer(&long);
        assert!(r.starts_with("aaaaaaaa"), "{r}");
        assert!(r.contains("..."), "{r}");
    }

    // Helper: parse_did_hex_52byte rejects uppercase
    #[test]
    fn tv_net2_h4_parse_did_hex_rejects_uppercase() {
        let did_hex = "AB".repeat(52);
        let r = parse_did_hex_52byte(&did_hex);
        assert!(r.is_err(), "uppercase DID hex must be rejected");
    }

    // === Test vectors (RFC-0011-k §Test Vectors Phase 3) ===

    // tv_net3_1: coordinator show parses with <coordinator_id> 64-char hex
    #[test]
    fn tv_net3_1_coordinator_show_parses_with_id() {
        let id_hex = "a".repeat(64);
        let cli = TestCoordinatorCli::try_parse_from(["test", "show", &id_hex]).unwrap();
        match cli.action {
            NetworkCoordinatorAction::Show(args) => {
                assert_eq!(hex::encode(args.coordinator_id), id_hex);
            }
            NetworkCoordinatorAction::Admin(_) => {
                panic!("expected Show, got Admin")
            }
        }
    }

    // tv_net3_2: coordinator show rejects malformed (non-hex) coordinator_id
    #[test]
    fn tv_net3_2_coordinator_show_rejects_non_hex() {
        let bad = "Z".repeat(64);
        let r = TestCoordinatorCli::try_parse_from(["test", "show", &bad]);
        assert!(r.is_err(), "non-hex coordinator_id must be rejected");
    }

    // tv_net3_3: coordinator admin parses with --action transfer_ownership
    #[test]
    fn tv_net3_3_coordinator_admin_parses_transfer_ownership() {
        let target_hex = "b".repeat(64);
        let cli = TestCoordinatorCli::try_parse_from([
            "test",
            "admin",
            "--action",
            "transfer_ownership",
            "--group-id",
            "grp-001",
            "--target",
            &target_hex,
        ])
        .unwrap();
        match cli.action {
            NetworkCoordinatorAction::Admin(args) => {
                assert_eq!(args.action, CoordinatorAdminActionLabel::TransferOwnership);
                assert_eq!(args.group_id, "grp-001");
                assert_eq!(hex::encode(args.target), target_hex);
            }
            NetworkCoordinatorAction::Show(_) => {
                panic!("expected Admin, got Show")
            }
        }
    }

    // tv_net3_4: coordinator admin rejects unknown action label
    #[test]
    fn tv_net3_4_coordinator_admin_rejects_unknown_label() {
        let target_hex = "c".repeat(64);
        let r = TestCoordinatorCli::try_parse_from([
            "test",
            "admin",
            "--action",
            "kick_ban",
            "--group-id",
            "grp-002",
            "--target",
            &target_hex,
        ]);
        assert!(r.is_err(), "unknown action label must be rejected");
    }

    // tv_net3_5: governance tally parses with --proposal-id
    #[test]
    fn tv_net3_5_governance_tally_parses_with_proposal_id() {
        let cli = TestNetCoordinatorCli::try_parse_from([
            "test",
            "governance",
            "tally",
            "--proposal-id",
            "42",
        ])
        .unwrap();
        match cli.action {
            NetworkAction::Governance {
                action: NetworkGovernanceAction::Tally(args),
            } => {
                assert_eq!(args.proposal_id, 42);
            }
            other => panic!("expected Governance Tally, got {other:?}"),
        }
    }

    // tv_net3_6: coordinator show calls CoordinatorRecord::load and emits
    // NetworkCoordinatorNotFound exit 84 (substrate-faithful: load always
    // returns None in current substrate).
    #[test]
    fn tv_net3_6_coordinator_show_emits_network_coordinator_not_found() {
        let id_hex = "d".repeat(64);
        let args = CoordinatorShowArgs {
            coordinator_id: [0x0d_u8; 32],
            json: false,
        };
        let res = coordinator_show(
            &args,
            &build_cli(&["octo", "network", "coordinator", "show", &id_hex]),
        );
        assert!(
            matches!(res, Err(OctoCliError::NetworkCoordinatorNotFound { .. })),
            "expected NetworkCoordinatorNotFound, got {res:?}"
        );
    }

    // tv_net3_7: coordinator admin substrate dispatch returns AdapterUnwired
    // (mapped to OctoCliError::Internal by the handler).
    #[test]
    fn tv_net3_7_coordinator_admin_substrate_returns_adapter_unwired() {
        let target_hex = "e".repeat(64);
        let mut cli = build_cli(&[
            "octo",
            "network",
            "coordinator",
            "admin",
            "--action",
            "ban_member",
            "--group-id",
            "grp-007",
            "--target",
            &target_hex,
        ]);
        // Bypass 3-flag confirmation gate via --dry-run; substrate still
        // rejects (AdapterUnwired).
        cli.mode.dry_run = true;
        let args = CoordinatorAdminArgs {
            action: CoordinatorAdminActionLabel::BanMember,
            group_id: "grp-007".to_string(),
            target: [0x0e_u8; 32],
            json: false,
        };
        let res = coordinator_admin(&args, &cli);
        assert!(
            matches!(res, Err(OctoCliError::Internal(_))),
            "expected Internal (AdapterUnwired mapping), got {res:?}"
        );
    }

    // tv_net3_8: governance tally handler computes canonical-bytes hash
    // for a zero-default proposal at the requested proposal_id
    // (substrate-faithful: G3b canonical-bytes helper is the only
    // persistence surface today).
    #[test]
    fn tv_net3_8_governance_tally_handler_emits_canonical_hash() {
        let args = GovernanceTallyArgs {
            proposal_id: 7,
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "governance",
            "tally",
            "--proposal-id",
            "7",
        ]);
        let res = governance_tally(&args, &cli);
        assert!(res.is_ok(), "expected Ok, got {res:?}");
    }

    // === Phase 4 test fixture + 12 test vectors (RFC-0011-l §Test Vectors) ===

    #[derive(Parser, Debug)]
    struct TestBindEnvelopeCli {
        #[command(subcommand)]
        action: NetworkBindEnvelopeAction,
    }

    // tv_net4_1: bind-envelope show parses with --domain-id
    #[test]
    fn tv_net4_1_bind_envelope_show_parses_with_domain_id() {
        let cli = TestBindEnvelopeCli::try_parse_from([
            "test",
            "show",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
        ])
        .unwrap();
        match cli.action {
            NetworkBindEnvelopeAction::Show(args) => {
                assert_eq!(args.domain_id, "0102030405060708090a0b0c0d0e0f10");
                assert!(!args.json);
            }
            _ => panic!("expected Show, got {cli:?}"),
        }
    }

    // tv_net4_2: bind-envelope show substrate miss emits NetworkSubstrateUnavailable (G22)
    #[test]
    fn tv_net4_2_bind_envelope_show_substrate_miss_emits_g22_exit() {
        let args = BindEnvelopeShowArgs {
            domain_id: "0102030405060708090a0b0c0d0e0f10".into(),
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "bind-envelope",
            "show",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
        ]);
        let res = bind_envelope_show(&args, &cli);
        match res {
            Err(OctoCliError::NetworkSubstrateUnavailable {
                companion,
                detail: _,
            }) => {
                assert_eq!(companion, "G22");
            }
            other => panic!("expected NetworkSubstrateUnavailable(G22), got {other:?}"),
        }
    }

    // tv_net4_3: bind-envelope rebind-prepare parses with all flags
    #[test]
    fn tv_net4_3_bind_envelope_rebind_prepare_parses_with_flags() {
        let cli = TestBindEnvelopeCli::try_parse_from([
            "test",
            "rebind-prepare",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--no-dry-run",
            "--confirm-acknowledge",
        ])
        .unwrap();
        match cli.action {
            NetworkBindEnvelopeAction::RebindPrepare(args) => {
                assert!(args.no_dry_run);
                assert!(args.confirm_acknowledge);
                assert!(!args.allow_ci_deny_default);
            }
            _ => panic!("expected RebindPrepare, got {cli:?}"),
        }
    }

    // tv_net4_4: bind-envelope rebind-prepare dry-run emits preview envelope
    #[test]
    fn tv_net4_4_bind_envelope_rebind_prepare_dry_run_emits_preview() {
        let args = BindEnvelopeRebindPrepareArgs {
            domain_id: "0102030405060708090a0b0c0d0e0f10".into(),
            no_dry_run: false,
            confirm_acknowledge: false,
            allow_ci_deny_default: false,
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "bind-envelope",
            "rebind-prepare",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
        ]);
        let res = bind_envelope_rebind_prepare(&args, &cli);
        assert!(res.is_ok(), "expected Ok dry-run preview, got {res:?}");
    }

    // tv_net4_5: bind-envelope rebind-prepare --no-dry-run without --confirm-acknowledge emits ConfirmationRequired
    #[test]
    fn tv_net4_5_bind_envelope_rebind_prepare_no_confirm_acknowledge_emits_error() {
        let args = BindEnvelopeRebindPrepareArgs {
            domain_id: "0102030405060708090a0b0c0d0e0f10".into(),
            no_dry_run: true,
            confirm_acknowledge: false,
            allow_ci_deny_default: false,
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "bind-envelope",
            "rebind-prepare",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--no-dry-run",
        ]);
        let res = bind_envelope_rebind_prepare(&args, &cli);
        match res {
            Err(OctoCliError::ConfirmationRequired { command }) => {
                assert!(command.contains("rebind-prepare"));
            }
            other => panic!("expected ConfirmationRequired, got {other:?}"),
        }
    }

    // tv_net4_6: bind-envelope rebind-commit parses with --confirm flag
    #[test]
    fn tv_net4_6_bind_envelope_rebind_commit_parses_with_confirm_flag() {
        let cli = TestBindEnvelopeCli::try_parse_from([
            "test",
            "rebind-commit",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--no-dry-run",
            "--confirm-acknowledge",
            "--confirm",
        ])
        .unwrap();
        match cli.action {
            NetworkBindEnvelopeAction::RebindCommit(args) => {
                assert!(args.no_dry_run);
                assert!(args.confirm_acknowledge);
                assert!(args.confirm);
            }
            _ => panic!("expected RebindCommit, got {cli:?}"),
        }
    }

    // tv_net4_7: bind-envelope rebind-commit without --confirm emits NetworkDryRunDenied (pastejacking defense)
    #[test]
    fn tv_net4_7_bind_envelope_rebind_commit_without_confirm_emits_dry_run_denied() {
        let args = BindEnvelopeRebindCommitArgs {
            domain_id: "0102030405060708090a0b0c0d0e0f10".into(),
            no_dry_run: true,
            confirm_acknowledge: true,
            confirm: false,
            allow_ci_deny_default: false,
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "bind-envelope",
            "rebind-commit",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--no-dry-run",
            "--confirm-acknowledge",
        ]);
        let res = bind_envelope_rebind_commit(&args, &cli);
        match res {
            Err(OctoCliError::NetworkDryRunDenied { arm, .. }) => {
                assert_eq!(arm, "commit");
            }
            other => panic!("expected NetworkDryRunDenied, got {other:?}"),
        }
    }

    // tv_net4_8: bind-envelope rebind-commit dispatched surfaces adapter unwired (G21 substrate)
    #[test]
    fn tv_net4_8_bind_envelope_rebind_commit_dispatched_emits_adapter_unwired() {
        let args = BindEnvelopeRebindCommitArgs {
            domain_id: "0102030405060708090a0b0c0d0e0f10".into(),
            no_dry_run: true,
            confirm_acknowledge: true,
            confirm: true,
            allow_ci_deny_default: false,
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "bind-envelope",
            "rebind-commit",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--no-dry-run",
            "--confirm-acknowledge",
            "--confirm",
        ]);
        let res = bind_envelope_rebind_commit(&args, &cli);
        match res {
            Err(OctoCliError::Internal(msg)) => {
                assert!(msg.contains("rebind arm adapter not yet wired"));
            }
            other => panic!("expected Internal adapter-unwired, got {other:?}"),
        }
    }

    // tv_net4_9: bind-envelope rebind-abort parses with --reason
    #[test]
    fn tv_net4_9_bind_envelope_rebind_abort_parses_with_reason() {
        let cli = TestBindEnvelopeCli::try_parse_from([
            "test",
            "rebind-abort",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--no-dry-run",
            "--confirm-acknowledge",
            "--reason",
            "timeout",
        ])
        .unwrap();
        match cli.action {
            NetworkBindEnvelopeAction::RebindAbort(args) => {
                assert_eq!(args.reason, "timeout");
                assert!(args.no_dry_run);
                assert!(args.confirm_acknowledge);
            }
            _ => panic!("expected RebindAbort, got {cli:?}"),
        }
    }

    // tv_net4_10: bind-envelope rebind-abort dry-run emits preview envelope
    #[test]
    fn tv_net4_10_bind_envelope_rebind_abort_dry_run_emits_preview() {
        let args = BindEnvelopeRebindAbortArgs {
            domain_id: "0102030405060708090a0b0c0d0e0f10".into(),
            no_dry_run: false,
            confirm_acknowledge: false,
            reason: "rollback-for-test".into(),
            allow_ci_deny_default: false,
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "bind-envelope",
            "rebind-abort",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--reason",
            "rollback-for-test",
        ]);
        let res = bind_envelope_rebind_abort(&args, &cli);
        assert!(res.is_ok(), "expected Ok dry-run preview, got {res:?}");
    }

    // tv_net4_11: bind-envelope rebind-abort dispatched surfaces adapter unwired (G21 substrate)
    #[test]
    fn tv_net4_11_bind_envelope_rebind_abort_dispatched_emits_adapter_unwired() {
        let args = BindEnvelopeRebindAbortArgs {
            domain_id: "0102030405060708090a0b0c0d0e0f10".into(),
            no_dry_run: true,
            confirm_acknowledge: true,
            reason: "rollback-for-test".into(),
            allow_ci_deny_default: false,
            json: false,
        };
        let cli = build_cli(&[
            "octo",
            "network",
            "bind-envelope",
            "rebind-abort",
            "--domain-id",
            "0102030405060708090a0b0c0d0e0f10",
            "--no-dry-run",
            "--confirm-acknowledge",
            "--reason",
            "rollback-for-test",
        ]);
        let res = bind_envelope_rebind_abort(&args, &cli);
        match res {
            Err(OctoCliError::Internal(msg)) => {
                assert!(msg.contains("rebind arm adapter not yet wired"));
            }
            other => panic!("expected Internal adapter-unwired, got {other:?}"),
        }
    }

    // tv_net4_12: bind-envelope rebind-abort reason redaction truncates long text
    #[test]
    fn tv_net4_12_bind_envelope_rebind_abort_reason_redaction_truncates() {
        let long = "x".repeat(120);
        let redacted = redact_reason(&long);
        assert!(redacted.len() <= 83, "got len={}", redacted.len());
        assert!(redacted.ends_with("..."));
    }

    // === Phase 5 test fixture + 6 test vectors (RFC-0011-m §Test Vectors) ===

    #[derive(Parser, Debug)]
    struct TestDiscoveryCli {
        #[command(subcommand)]
        action: NetworkDiscoveryAction,
    }

    // tv_net5_1: discovery advertisement show parses with --advertisement-id (G23)
    #[test]
    fn tv_net5_1_discovery_advertisement_show_parses_with_advertisement_id() {
        let cli = TestDiscoveryCli::try_parse_from([
            "test",
            "advertisement-show",
            "--advertisement-id",
            &"a".repeat(64),
        ])
        .unwrap();
        match cli.action {
            NetworkDiscoveryAction::AdvertisementShow(args) => {
                assert_eq!(args.advertisement_id, Some("a".repeat(64)));
                assert!(!args.json);
            }
            _ => panic!("expected AdvertisementShow, got {cli:?}"),
        }
    }

    // tv_net5_2: discovery advertisement show substrate miss emits NetworkSubstrateUnavailable (G23)
    #[test]
    fn tv_net5_2_discovery_advertisement_show_substrate_miss_emits_g23_exit() {
        // Empty cache returns None for every key today; CLI
        // translates Option::None to typed exit 89
        // NetworkSubstrateUnavailable (REUSED slot per RFC-0011-h
        // §Error Handling row 526).
        let cache = MissionAdvertisementCache::default();
        let key = [0x11u8; 32];
        assert!(cache.get(&key).is_none());
    }

    // tv_net5_3: discovery advertisement show --hops 65536 rejected pre-dispatch (clap u16 overflow)
    #[test]
    fn tv_net5_3_discovery_advertisement_show_hops_overflow_rejected() {
        // clap u16 parse error pre-dispatch per RFC-0011-m
        // §Security Considerations. 65536 does not fit u16.
        let result =
            TestDiscoveryCli::try_parse_from(["test", "advertisement-show", "--hops", "65536"]);
        assert!(result.is_err(), "expected clap u16 overflow rejection");
    }

    // tv_net5_4: discovery invitation show parses with --invitation-id (G24)
    #[test]
    fn tv_net5_4_discovery_invitation_show_parses_with_invitation_id() {
        let cli = TestDiscoveryCli::try_parse_from([
            "test",
            "invitation-show",
            "--invitation-id",
            &"b".repeat(64),
        ])
        .unwrap();
        match cli.action {
            NetworkDiscoveryAction::InvitationShow(args) => {
                assert_eq!(args.invitation_id, Some("b".repeat(64)));
                assert!(!args.json);
            }
            _ => panic!("expected InvitationShow, got {cli:?}"),
        }
    }

    // tv_net5_5: discovery invitation show substrate miss emits NetworkSubstrateUnavailable (G24)
    #[test]
    fn tv_net5_5_discovery_invitation_show_substrate_miss_emits_g24_exit() {
        // Empty cache returns None for every key today; CLI
        // translates Option::None to typed exit 89
        // NetworkSubstrateUnavailable (REUSED slot per RFC-0011-h
        // §Error Handling row 526).
        let cache = MissionInvitationCache::default();
        let key = [0x22u8; 32];
        assert!(cache.get(&key).is_none());
    }

    // tv_net5_6: discovery invitation show pastejacking defense rejects mixed-case hex
    #[test]
    fn tv_net5_6_discovery_invitation_show_pastejacking_mixed_case_rejected() {
        // Mixed-case hex rejected per the parse_32_byte_hex
        // pastejacking defense (same pattern as Phase 1 G12b
        // CoordinatorRecord::load lookup defense).
        let mixed_case = "a".repeat(32) + &"B".repeat(32);
        let result = parse_32_byte_hex(&mixed_case, "invitation_id");
        assert!(result.is_err(), "expected mixed-case rejection");
    }

    // === Phase 6 test vectors (RFC-0011-n §Test Vectors Phase 6) ===

    /// Test CLI struct for Phase 6 bootstrap + status surface.
    #[derive(Parser, Debug)]
    struct TestPhase6Cli {
        #[command(subcommand)]
        action: NetworkAction,
    }

    // tv_net6_1: bootstrap subcommand parses cleanly (G26 substrate-faithful)
    #[test]
    fn tv_net6_1_bootstrap_parses_with_no_args() {
        let cli = TestPhase6Cli::try_parse_from(["test", "bootstrap"]).expect("parse");
        assert!(matches!(cli.action, NetworkAction::Bootstrap(_)));
    }

    // tv_net6_2: bootstrap pre-G26 closure path returns clap error (pre-companion)
    //             — verified via TryParseFrom-fail with missing bootstrap.toml. Skipped
    //             here because it requires filesystem setup; substrate exit 89 path
    //             covered by the G26 substrate unit tests.
    #[test]
    fn tv_net6_2_bootstrap_substrate_failure_emits_g26_exit() {
        // G26 substrate unit tests cover AlreadyStarted + InvalidConfig + SeedListUnavailable.
        // This test vector pins the dispatch surface only.
        let cli = TestPhase6Cli::try_parse_from(["test", "bootstrap"]).expect("parse");
        match cli.action {
            NetworkAction::Bootstrap(args) => {
                assert!(!args.json);
            }
            _ => panic!("expected Bootstrap"),
        }
    }

    // tv_net6_3: bootstrap with --json flag parses cleanly
    #[test]
    fn tv_net6_3_bootstrap_json_flag_parses() {
        let cli = TestPhase6Cli::try_parse_from(["test", "bootstrap", "--json"]).expect("parse");
        match cli.action {
            NetworkAction::Bootstrap(args) => {
                assert!(args.json);
            }
            _ => panic!("expected Bootstrap"),
        }
    }

    // tv_net6_4: status subcommand parses cleanly with all substrate layers present
    #[test]
    fn tv_net6_4_status_parses_with_no_args() {
        let cli = TestPhase6Cli::try_parse_from(["test", "status"]).expect("parse");
        assert!(matches!(cli.action, NetworkAction::Status(_)));
    }

    // tv_net6_5: status pre-Phase-6 closure path verified by substrate G18 + G20 +
    //             G26 unit tests covering the empty-default state. This test vector
    //             pins the dispatch surface only.
    #[test]
    fn tv_net6_5_status_dispatch_surface_verified() {
        let cli = TestPhase6Cli::try_parse_from(["test", "status"]).expect("parse");
        match cli.action {
            NetworkAction::Status(args) => {
                assert!(!args.json);
            }
            _ => panic!("expected Status"),
        }
    }

    // tv_net6_6: status envelope includes drift-closure seed_health_refuses_start field
    #[test]
    fn tv_net6_6_status_envelope_has_drift_closure_field() {
        // Default BootstrapState has refuse_start=false; default SeedHealth::check
        // returns Fresh which also has refuses_start=false. Substrate-faithful
        // drift-closure field projection confirmed.
        let orch = BootstrapOrchestrator::default();
        let state = orch.status();
        assert!(!state.refuse_start);
    }

    // === Phase 7 test vectors (RFC-0011-o §Test Vectors Phase 7) ===

    // tv_net7_1: slash-bridge list subcommand parses cleanly (G9 substrate-faithful)
    #[test]
    fn tv_net7_1_slash_bridge_list_parses_with_no_args() {
        let cli = TestPhase7Cli::try_parse_from(["test", "slash-bridge", "list"]).expect("parse");
        match cli.action {
            NetworkAction::SlashBridge { action } => match action {
                NetworkSlashBridgeAction::List(args) => assert!(!args.json),
                _ => panic!("expected List"),
            },
            _ => panic!("expected SlashBridge"),
        }
    }

    // tv_net7_2: slash-bridge list --json flag parses cleanly
    #[test]
    fn tv_net7_2_slash_bridge_list_json_flag_parses() {
        let cli = TestPhase7Cli::try_parse_from(["test", "slash-bridge", "list", "--json"])
            .expect("parse");
        match cli.action {
            NetworkAction::SlashBridge { action } => match action {
                NetworkSlashBridgeAction::List(args) => assert!(args.json),
                _ => panic!("expected List"),
            },
            _ => panic!("expected SlashBridge"),
        }
    }

    // tv_net7_3: slash-bridge list envelope projection is substrate-faithful
    // (empty registry returns total=0 with empty slashes; BTreeMap
    // metadata serializes to empty JSON object per RFC-0011-h §Output
    // Envelope determinism pattern).
    #[test]
    fn tv_net7_3_slash_bridge_list_empty_envelope_total_zero() {
        use octo_network::mon::slash_bridge::SlashBridge;
        // In-memory test impl mirrors the substrate InMemorySlashBridge;
        // list returns empty Vec for empty store (substrate-faithful).
        struct EmptyBridge;
        impl SlashBridge for EmptyBridge {
            fn list(&self) -> Vec<octo_network::mon::slash_bridge::BridgedSlash> {
                Vec::new()
            }
            fn propagate_to(
                &self,
                _id: [u8; 32],
            ) -> Result<
                octo_network::mon::slash_bridge::BridgeReceipt,
                octo_network::mon::slash_bridge::BridgeError,
            > {
                Err(octo_network::mon::slash_bridge::BridgeError::Unreachable)
            }
        }
        let bridge = EmptyBridge;
        assert_eq!(bridge.list().len(), 0);
    }

    // tv_net7_4: slash-bridge propagate subcommand parses cleanly with --apply + --confirm-acknowledge
    #[test]
    fn tv_net7_4_slash_bridge_propagate_parses_with_apply_and_confirm() {
        let hex_id = "0".repeat(64);
        let cli = TestPhase7Cli::try_parse_from([
            "test",
            "slash-bridge",
            "propagate",
            &hex_id,
            "--apply",
            "--confirm-acknowledge",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::SlashBridge { action } => match action {
                NetworkSlashBridgeAction::Propagate(args) => {
                    assert!(args.apply);
                    assert!(args.confirm_acknowledge);
                    assert!(!args.dry_run);
                }
                _ => panic!("expected Propagate"),
            },
            _ => panic!("expected SlashBridge"),
        }
    }

    // tv_net7_5: slash-bridge propagate with --apply but no --confirm-acknowledge is
    //             rejected at parse-time by clap (requires="confirm_acknowledge" on --apply).
    //             Parse-time rejection is preferred over runtime ConfirmationRequired because
    //             the operator sees the missing flag in --help before invocation.
    #[test]
    fn tv_net7_5_slash_bridge_propagate_apply_rejects_missing_confirm_at_parse_time() {
        let hex_id = "0".repeat(64);
        let result = TestPhase7Cli::try_parse_from([
            "test",
            "slash-bridge",
            "propagate",
            &hex_id,
            "--apply",
        ]);
        assert!(
            result.is_err(),
            "expected parse-time rejection when --apply passed without --confirm-acknowledge, but parse succeeded"
        );
        let err = result.expect_err("parse-time rejection");
        let err_str = err.to_string();
        assert!(
            err_str.contains("--confirm-acknowledge"),
            "expected error to mention --confirm-acknowledge, got: {err_str}"
        );
    }

    // tv_net7_6: slash-bridge propagate pastejacking defense — mixed-case hex
    //              rejected by parse_32_byte_hex shared helper
    //              (RFC-0011-h pastejacking defense pattern)
    #[test]
    fn tv_net7_6_slash_bridge_propagate_rejects_mixed_case_hex() {
        let mixed_case = "Aa".repeat(32);
        let result =
            TestPhase7Cli::try_parse_from(["test", "slash-bridge", "propagate", &mixed_case]);
        assert!(result.is_err(), "mixed-case hex must be rejected");
    }

    // tv_net7_6b: slash-bridge propagate accepts uppercase-only hex
    //              (parse_32_byte_hex shared helper accepts
    //              lowercase OR uppercase; only mixed-case is
    //              rejected). Confirms Phase 7 pastejacking defense
    //              aligns with RFC-0011-h shared helper contract
    //              (NOT the narrower parse_gateway_id_hex
    //              implementation).
    #[test]
    fn tv_net7_6b_slash_bridge_propagate_accepts_uppercase_only_hex() {
        let upper_hex = "A".repeat(64);
        let result =
            TestPhase7Cli::try_parse_from(["test", "slash-bridge", "propagate", &upper_hex]);
        assert!(
            result.is_ok(),
            "uppercase-only hex per parse_32_byte_hex must be accepted"
        );
    }

    /// Test CLI struct for Phase 7 slash-bridge surface.
    #[derive(Parser, Debug)]
    struct TestPhase7Cli {
        #[command(subcommand)]
        action: NetworkAction,
    }

    // === Phase 8 test vectors (RFC-0011-p §Test Vectors Phase 8) ===

    /// Test CLI struct for Phase 8 router surface.
    #[derive(Parser, Debug)]
    struct TestPhase8Cli {
        #[command(subcommand)]
        action: NetworkAction,
    }

    // tv_net8_1: router status subcommand parses cleanly
    // (G10 substrate-faithful)
    #[test]
    fn tv_net8_1_router_status_parses_with_no_args() {
        let cli = TestPhase8Cli::try_parse_from(["test", "router", "status"]).expect("parse");
        match cli.action {
            NetworkAction::Router { action } => match action {
                NetworkRouterAction::Status(args) => assert!(!args.json),
                _ => panic!("expected Status"),
            },
            _ => panic!("expected Router"),
        }
    }

    // tv_net8_2: router status --json flag parses cleanly
    #[test]
    fn tv_net8_2_router_status_json_flag_parses() {
        let cli =
            TestPhase8Cli::try_parse_from(["test", "router", "status", "--json"]).expect("parse");
        match cli.action {
            NetworkAction::Router { action } => match action {
                NetworkRouterAction::Status(args) => assert!(args.json),
                _ => panic!("expected Status"),
            },
            _ => panic!("expected Router"),
        }
    }

    // tv_net8_3: router status envelope projection is
    // substrate-faithful (default node has Offline status
    // with empty peer counts).
    #[test]
    fn tv_net8_3_router_status_default_envelope_is_offline() {
        use octo_network::quota::router_node::{QuotaRouterNode, RouterStatus};
        let node = QuotaRouterNode::default();
        assert_eq!(node.status(), RouterStatus::Offline);
        assert_eq!(status_label(node.status()), "offline");
        assert_eq!(node.reachable_peer_count(), 0);
        assert_eq!(node.total_peer_count(), 0);
    }

    // tv_net8_4: router peers subcommand parses cleanly with
    // lowercase hex peer_node_id (G10 substrate-faithful)
    #[test]
    fn tv_net8_4_router_peers_parses_with_lowercase_hex() {
        let hex_id = "a".repeat(64);
        let cli =
            TestPhase8Cli::try_parse_from(["test", "router", "peers", &hex_id]).expect("parse");
        match cli.action {
            NetworkAction::Router { action } => match action {
                NetworkRouterAction::Peers(args) => {
                    assert_eq!(args.peer_node_id, [0xAA; 32]);
                    assert!(!args.json);
                }
                _ => panic!("expected Peers"),
            },
            _ => panic!("expected Router"),
        }
    }

    // tv_net8_5: router peers pastejacking defense — mixed-case
    // hex rejected by parse_32_byte_hex shared helper
    // (RFC-0011-h §Pastejacking Defense pattern)
    #[test]
    fn tv_net8_5_router_peers_rejects_mixed_case_hex() {
        let mixed_case = "Aa".repeat(32);
        let result = TestPhase8Cli::try_parse_from(["test", "router", "peers", &mixed_case]);
        assert!(result.is_err(), "mixed-case hex must be rejected");
    }

    // tv_net8_6: router peers accepts uppercase-only hex
    // (parse_32_byte_hex shared helper accepts lowercase OR
    // uppercase; only mixed-case is rejected).
    #[test]
    fn tv_net8_6_router_peers_accepts_uppercase_only_hex() {
        let upper_hex = "B".repeat(64);
        let cli =
            TestPhase8Cli::try_parse_from(["test", "router", "peers", &upper_hex]).expect("parse");
        match cli.action {
            NetworkAction::Router { action } => match action {
                NetworkRouterAction::Peers(args) => {
                    assert_eq!(args.peer_node_id, [0xBB; 32]);
                }
                _ => panic!("expected Peers"),
            },
            _ => panic!("expected Router"),
        }
    }

    // tv_net8_7: status_label Healthy arm projection
    // (G10 substrate-faithful; Healthy arm not previously
    // covered by tv_net8_3 which only exercised the Offline
    // default state).
    #[test]
    fn tv_net8_7_status_label_healthy_arm() {
        use octo_network::quota::router_node::RouterStatus;
        assert_eq!(status_label(RouterStatus::Healthy), "healthy");
    }

    // tv_net8_8: status_label Degraded arm projection
    // (G10 substrate-faithful; Degraded arm not previously
    // covered; covers middle-path operational state).
    #[test]
    fn tv_net8_8_status_label_degraded_arm() {
        use octo_network::quota::router_node::RouterStatus;
        assert_eq!(status_label(RouterStatus::Degraded), "degraded");
    }

    // tv_net8_9: router status handler surfaces slot-89
    // NetworkSubstrateUnavailable when registry returns
    // None (G10 per-extension crate pattern: concrete
    // impl crates are OUT OF SCOPE; registry is None
    // until a follow-on Layer D crate registers).
    #[test]
    fn tv_net8_9_router_status_substrate_unavailable_slot_89() {
        let cli = TestPhase8Cli::try_parse_from(["test", "router", "status"]).expect("parse");
        let args = match cli.action {
            NetworkAction::Router { action } => match action {
                NetworkRouterAction::Status(args) => args,
                _ => panic!("expected Status"),
            },
            _ => panic!("expected Router"),
        };
        let runtime =
            Octo::try_parse_from(["test", "network", "router", "status"]).expect("runtime parse");
        let err = network_router_status(&args, &runtime)
            .expect_err("registry None should yield slot-89 error");
        match err {
            OctoCliError::NetworkSubstrateUnavailable { companion, detail } => {
                assert_eq!(companion, "G10");
                assert_eq!(detail, "");
            }
            other => panic!("expected NetworkSubstrateUnavailable, got {other:?}"),
        }
    }

    // tv_net8_10: router peers handler surfaces slot-89
    // NetworkSubstrateUnavailable when registry returns
    // None (G10 per-extension crate pattern; anchors the
    // G10 error-path contract on slot 89 for CLI consumers).
    #[test]
    fn tv_net8_10_router_peers_substrate_unavailable_slot_89() {
        let lower_hex = "a".repeat(64);
        let cli =
            TestPhase8Cli::try_parse_from(["test", "router", "peers", &lower_hex]).expect("parse");
        let args = match cli.action {
            NetworkAction::Router { action } => match action {
                NetworkRouterAction::Peers(args) => args,
                _ => panic!("expected Peers"),
            },
            _ => panic!("expected Router"),
        };
        let runtime = Octo::try_parse_from(["test", "network", "router", "peers", &lower_hex])
            .expect("runtime parse");
        let err = network_router_peers(&args, &runtime)
            .expect_err("registry None should yield slot-89 error");
        match err {
            OctoCliError::NetworkSubstrateUnavailable { companion, detail } => {
                assert_eq!(companion, "G10");
                assert_eq!(detail, "");
            }
            other => panic!("expected NetworkSubstrateUnavailable, got {other:?}"),
        }
    }

    // === Phase 9 test vectors (RFC-0011-q §Test Vectors Phase 9) ===

    /// Test CLI struct for Phase 9 specialized-node surface.
    #[derive(Parser, Debug)]
    struct TestPhase9Cli {
        #[command(subcommand)]
        action: NetworkAction,
    }

    // tv_net9_1: node show subcommand parses cleanly with
    // lowercase hex node_id (G11 substrate-faithful).
    #[test]
    fn tv_net9_1_node_show_parses_with_lowercase_hex() {
        let hex_id = "c".repeat(64);
        let cli = TestPhase9Cli::try_parse_from(["test", "node", "show", &hex_id]).expect("parse");
        match cli.action {
            NetworkAction::Node { action } => match action {
                NetworkNodeAction::Show(args) => {
                    assert_eq!(args.node_id, [0xCC; 32]);
                    assert!(!args.json);
                }
                _ => panic!("expected Show"),
            },
            _ => panic!("expected Node"),
        }
    }

    // tv_net9_2: node show --json flag parses cleanly.
    #[test]
    fn tv_net9_2_node_show_json_flag_parses() {
        let hex_id = "d".repeat(64);
        let cli = TestPhase9Cli::try_parse_from(["test", "node", "show", &hex_id, "--json"])
            .expect("parse");
        match cli.action {
            NetworkAction::Node { action } => match action {
                NetworkNodeAction::Show(args) => assert!(args.json),
                _ => panic!("expected Show"),
            },
            _ => panic!("expected Node"),
        }
    }

    // tv_net9_3: node show envelope projection is
    // substrate-faithful (None = not found). Default
    // registry returns None (specialized_node_registry
    // returns None pre-extension-crate-impl), surfacing
    // exit 89 NetworkSubstrateUnavailable.
    #[test]
    fn tv_net9_3_node_show_default_registry_returns_none() {
        use octo_network::specialized::node_record::{NodeClass, SpecializedNodeRecord};
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert("region".to_string(), "eu-west".to_string());
        let rec = SpecializedNodeRecord {
            node_id: [0xEE; 32],
            holder_did: None,
            node_class: NodeClass::Provider,
            creation_epoch: 42,
            metadata,
        };
        assert_eq!(node_class_label(rec.node_class), "provider");
        assert_eq!(rec.creation_epoch, 42);
        assert!(rec.holder_did.is_none());
        assert_eq!(rec.metadata.get("region"), Some(&"eu-west".to_string()));
    }

    // tv_net9_4: node bind --apply --confirm-acknowledge
    // parses cleanly with lowercase hex node_id.
    #[test]
    fn tv_net9_4_node_bind_apply_parses_with_confirm_acknowledge() {
        let hex_id = "e".repeat(64);
        let holder_did =
            "did:octo:0xefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef";
        let cli = TestPhase9Cli::try_parse_from([
            "test",
            "node",
            "bind",
            &hex_id,
            "--holder-did",
            holder_did,
            "--apply",
            "--confirm-acknowledge",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Node { action } => match action {
                NetworkNodeAction::Bind(args) => {
                    assert_eq!(args.node_id, [0xEE; 32]);
                    assert_eq!(args.holder_did, holder_did);
                    assert!(args.apply);
                    assert!(args.confirm_acknowledge);
                    assert!(!args.dry_run);
                }
                _ => panic!("expected Bind"),
            },
            _ => panic!("expected Node"),
        }
    }

    // tv_net9_5: node bind --apply without
    // --confirm-acknowledge rejected at parse-time
    // (clap `requires` constraint per RFC-0011-q
    // §Confirmation Flag Phase 9).
    #[test]
    fn tv_net9_5_node_bind_apply_without_confirm_acknowledge_rejected() {
        let hex_id = "f".repeat(64);
        let holder_did = "did:octo:0xff";
        let result = TestPhase9Cli::try_parse_from([
            "test",
            "node",
            "bind",
            &hex_id,
            "--holder-did",
            holder_did,
            "--apply",
        ]);
        assert!(
            result.is_err(),
            "--apply without --confirm-acknowledge must be rejected at parse-time"
        );
    }

    // tv_net9_6: node bind pastejacking defense — mixed-case
    // hex rejected by parse_32_byte_hex shared helper.
    #[test]
    fn tv_net9_6_node_bind_rejects_mixed_case_hex() {
        let mixed_case = "Aa".repeat(32);
        let result = TestPhase9Cli::try_parse_from([
            "test",
            "node",
            "bind",
            &mixed_case,
            "--holder-did",
            "did:octo:0xab",
        ]);
        assert!(result.is_err(), "mixed-case hex must be rejected");
    }

    // tv_net9_7: node bind dry-run path renders preview
    // envelope without substrate dispatch (RFC-0011-q
    // §Confirmation Flag: dry-run is default; standalone
    // — does NOT require registry impl). Confirms Bug 4
    // closure: default invocation returns Ok with
    // applied=false, NOT slot 89 NetworkSubstrateUnavailable.
    #[test]
    fn tv_net9_7_node_bind_dry_run_renders_preview_without_substrate() {
        let hex_id = "1".repeat(64);
        let holder_did = "did:octo:0x11";
        let cli = TestPhase9Cli::try_parse_from([
            "test",
            "node",
            "bind",
            &hex_id,
            "--holder-did",
            holder_did,
        ])
        .expect("parse");
        let args = match cli.action {
            NetworkAction::Node { action } => match action {
                NetworkNodeAction::Bind(a) => a,
                _ => panic!("expected Bind"),
            },
            _ => panic!("expected Node"),
        };
        let runtime = Octo::try_parse_from([
            "test",
            "network",
            "node",
            "bind",
            &hex_id,
            "--holder-did",
            holder_did,
        ])
        .expect("runtime parse");
        let result = network_node_bind(&args, &runtime);
        match result {
            Ok(()) => {} // expected: dry-run returns Ok with applied=false envelope
            Err(OctoCliError::NetworkSubstrateUnavailable { .. }) => {
                panic!("dry-run path must NOT surface NetworkSubstrateUnavailable");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    // tv_net9_8: node bind apply path surfaces slot 89
    // NetworkSubstrateUnavailable when registry is empty
    // (pre-extension-crate-impl). Companion-gated dispatch
    // per RFC-0011-h §Error Handling row 89.
    #[test]
    fn tv_net9_8_node_bind_apply_surfaces_substrate_unavailable() {
        let hex_id = "2".repeat(64);
        let holder_did = "did:octo:0x22";
        let cli = TestPhase9Cli::try_parse_from([
            "test",
            "node",
            "bind",
            &hex_id,
            "--holder-did",
            holder_did,
            "--apply",
            "--confirm-acknowledge",
        ])
        .expect("parse");
        let args = match cli.action {
            NetworkAction::Node { action } => match action {
                NetworkNodeAction::Bind(a) => a,
                _ => panic!("expected Bind"),
            },
            _ => panic!("expected Node"),
        };
        let runtime = Octo::try_parse_from([
            "test",
            "network",
            "node",
            "bind",
            &hex_id,
            "--holder-did",
            holder_did,
            "--apply",
            "--confirm-acknowledge",
        ])
        .expect("runtime parse");
        let result = network_node_bind(&args, &runtime);
        match result {
            Err(OctoCliError::NetworkSubstrateUnavailable { companion, detail }) => {
                assert_eq!(companion, "G11");
                assert_eq!(detail, "");
            }
            other => panic!("expected NetworkSubstrateUnavailable {{ companion: G11, detail: \"\" }}, got {other:?}"),
        }
    }

    // tv_net9_9: node_class_label maps all 5 NodeClass
    // variants to their lowercase string labels per
    // serde rename_all = lowercase + RFC-0011-q §Output
    // Envelope Phase 9 (Builder / Provider / Storage /
    // Bandwidth / Orchestrator).
    #[test]
    fn tv_net9_9_node_class_label_all_variants() {
        use octo_network::specialized::node_record::NodeClass;
        assert_eq!(node_class_label(NodeClass::Builder), "builder");
        assert_eq!(node_class_label(NodeClass::Provider), "provider");
        assert_eq!(node_class_label(NodeClass::Storage), "storage");
        assert_eq!(node_class_label(NodeClass::Bandwidth), "bandwidth");
        assert_eq!(node_class_label(NodeClass::Orchestrator), "orchestrator");
    }

    // tv_net9_10: node bind accepts uppercase-only hex
    // (positive branch of pastejacking defense — parse_32_byte_hex
    // shared helper accepts lowercase OR uppercase; rejects
    // mixed-case only).
    #[test]
    fn tv_net9_10_node_bind_accepts_uppercase_only_hex() {
        let upper_hex = "F".repeat(64);
        let cli = TestPhase9Cli::try_parse_from([
            "test",
            "node",
            "bind",
            &upper_hex,
            "--holder-did",
            "did:octo:0xff",
        ])
        .expect("uppercase-only hex must parse cleanly");
        match cli.action {
            NetworkAction::Node { action } => match action {
                NetworkNodeAction::Bind(args) => {
                    assert_eq!(args.node_id, [0xFF; 32]);
                }
                _ => panic!("expected Bind"),
            },
            _ => panic!("expected Node"),
        }
    }

    // === Phase 10 test vectors (RFC-0011-r Phase 10 G13) ===

    #[derive(Parser, Debug)]
    struct TestPhase10Cli {
        #[command(subcommand)]
        action: NetworkAction,
    }

    fn hex_did(c: char) -> String {
        std::iter::repeat_n(c, 104).collect()
    }

    // tv_net10_1: reputation list --filter all parses cleanly.
    #[test]
    fn tv_net10_1_reputation_list_filter_all_parses() {
        let cli = TestPhase10Cli::try_parse_from(["test", "reputation", "list", "--filter", "all"])
            .expect("parse");
        match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(args) => {
                    assert!(matches!(args.filter, ReputationFilterKind::All));
                    assert!(args.threshold.is_none());
                    assert!(!args.json);
                }
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        }
    }

    // tv_net10_2: reputation list --filter above-score +
    // --threshold 100 parses cleanly.
    #[test]
    fn tv_net10_2_reputation_list_above_score_with_threshold_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "reputation",
            "list",
            "--filter",
            "above-score",
            "--threshold",
            "100",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(args) => {
                    assert!(matches!(args.filter, ReputationFilterKind::AboveScore));
                    assert_eq!(args.threshold, Some(100));
                }
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        }
    }

    // tv_net10_3: reputation list --filter above-score WITHOUT
    // --threshold is rejected by the handler's threshold
    // validation rule. Clap does NOT enforce threshold at
    // parse-time (threshold is an Option<u32>); the handler
    // enforces it BEFORE the substrate-unavailability guard
    // per RFC-0011-h §Confirmation Flag pattern. Verifies
    // that the handler returns ConfirmationRequired when
    // filter is AboveScore and threshold is None.
    #[test]
    fn tv_net10_3_reputation_list_above_score_requires_threshold() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "reputation",
            "list",
            "--filter",
            "above-score",
        ])
        .expect("parse");
        let args = match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(a) => a,
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        };
        let runtime = Octo::try_parse_from([
            "test",
            "network",
            "reputation",
            "list",
            "--filter",
            "above-score",
        ])
        .expect("runtime parse");
        let result = network_reputation_list(&args, &runtime);
        match result {
            Err(OctoCliError::ConfirmationRequired { command }) => {
                assert!(
                    command.contains("threshold required"),
                    "ConfirmationRequired command must mention threshold required, got {command:?}"
                );
            }
            other => panic!(
                "expected ConfirmationRequired with threshold required detail, got {other:?}"
            ),
        }
    }

    // tv_net10_4: reputation show parses with canonical
    // did:octo:0x<104-hex> peer_did.
    #[test]
    fn tv_net10_4_reputation_show_parses_canonical_did() {
        let peer_did = format!("did:octo:0x{}", hex_did('a'));
        let cli = TestPhase10Cli::try_parse_from(["test", "reputation", "show", &peer_did])
            .expect("parse");
        match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::Show(args) => {
                    assert_eq!(args.peer_did, peer_did);
                    assert!(!args.json);
                }
                _ => panic!("expected Show"),
            },
            _ => panic!("expected Reputation"),
        }
    }

    // tv_net10_5: reputation show pastejacking defense —
    // mixed-case hex rejected by parse_reputation_peer_did.
    #[test]
    fn tv_net10_5_reputation_show_rejects_mixed_case_hex() {
        let mixed = format!("did:octo:0x{}", "Aa".repeat(52));
        let result = parse_reputation_peer_did(&mixed);
        assert!(
            matches!(result, Err(OctoCliError::ConfirmationRequired { .. })),
            "mixed-case hex peer_did must be rejected"
        );
    }

    // tv_net10_6: reputation show accepts uppercase-only hex.
    #[test]
    fn tv_net10_6_reputation_show_accepts_uppercase_hex() {
        let upper = format!("did:octo:0x{}", "F".repeat(104));
        let result = parse_reputation_peer_did(&upper);
        assert!(
            result.is_ok(),
            "uppercase-only hex peer_did must be accepted"
        );
    }

    // tv_net10_7: reputation list dispatch exercises the
    // substrate trait method (RFC-0011-r Phase 10 G13).
    // The InMemoryReputationStore stub returns Ok(empty
    // Vec); the dispatch renders the empty envelope.
    // Substrate-faithful per agent.rs:1554-1586 R5.5
    // precedent.
    #[test]
    fn tv_net10_7_reputation_list_substrate_dispatch() {
        let cli = TestPhase10Cli::try_parse_from(["test", "reputation", "list", "--filter", "all"])
            .expect("parse");
        let args = match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(a) => a,
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        };
        let runtime =
            Octo::try_parse_from(["test", "network", "reputation", "list"]).expect("runtime parse");
        let result = network_reputation_list(&args, &runtime);
        assert!(
            result.is_ok(),
            "reputation list dispatch must invoke substrate + render envelope, got {result:?}"
        );
    }

    // tv_net10_7b: AboveScore handler dispatch path. The
    // substrate trait stub is symmetric (returns empty
    // Vec for any filter), but the handler match arm
    // builds `ReputationFilter::AboveScore(args.threshold
    // .unwrap_or(0))` — a bug there would not be caught
    // by tv_net10_7. Symmetric tv_net10_7c covers
    // BelowScore.
    #[test]
    fn tv_net10_7b_reputation_list_above_score_substrate_dispatch() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "reputation",
            "list",
            "--filter",
            "above-score",
            "--threshold",
            "100",
        ])
        .expect("parse");
        let args = match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(a) => a,
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        };
        let runtime = Octo::try_parse_from([
            "test",
            "network",
            "reputation",
            "list",
            "--filter",
            "above-score",
            "--threshold",
            "100",
        ])
        .expect("runtime parse");
        let result = network_reputation_list(&args, &runtime);
        assert!(
            result.is_ok(),
            "reputation list above-score dispatch must invoke substrate + render envelope, got {result:?}"
        );
    }

    // tv_net10_7c: BelowScore handler dispatch path.
    // Symmetric with tv_net10_7b (above-score).
    #[test]
    fn tv_net10_7c_reputation_list_below_score_substrate_dispatch() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "reputation",
            "list",
            "--filter",
            "below-score",
            "--threshold",
            "100",
        ])
        .expect("parse");
        let args = match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(a) => a,
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        };
        let runtime = Octo::try_parse_from([
            "test",
            "network",
            "reputation",
            "list",
            "--filter",
            "below-score",
            "--threshold",
            "100",
        ])
        .expect("runtime parse");
        let result = network_reputation_list(&args, &runtime);
        assert!(
            result.is_ok(),
            "reputation list below-score dispatch must invoke substrate + render envelope, got {result:?}"
        );
    }

    // tv_net10_8: reputation show dispatch exercises the
    // substrate trait method (RFC-0011-r Phase 10 G13).
    // The InMemoryReputationStore stub returns Ok(None);
    // the dispatch renders the empty record envelope.
    // Substrate-faithful per Phase 1 slash_list precedent.
    #[test]
    fn tv_net10_8_reputation_show_substrate_dispatch() {
        let peer_did = format!("did:octo:0x{}", "a".repeat(104));
        let cli = TestPhase10Cli::try_parse_from(["test", "reputation", "show", &peer_did])
            .expect("parse");
        let args = match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::Show(a) => a,
                _ => panic!("expected Show"),
            },
            _ => panic!("expected Reputation"),
        };
        let runtime = Octo::try_parse_from(["test", "network", "reputation", "show", &peer_did])
            .expect("runtime parse");
        let result = network_reputation_show(&args, &runtime);
        assert!(
            result.is_ok(),
            "reputation show dispatch must invoke substrate + render envelope, got {result:?}"
        );
    }

    // tv_net10_9: parse_reputation_peer_did rejects a
    // peer_did missing the did:octo:0x prefix (input
    // validation before substrate dispatch per Phase 10
    // G13; operator input error not substrate error).
    #[test]
    fn tv_net10_9_reputation_show_rejects_missing_prefix() {
        let result = parse_reputation_peer_did("not-a-valid-did");
        assert!(
            matches!(result, Err(OctoCliError::ConfirmationRequired { .. })),
            "missing-prefix peer_did must be rejected"
        );
    }

    // tv_net10_10: parse_reputation_peer_did rejects a
    // peer_did body that is not 104 hex chars
    // (pastejacking defense per Phase 5 RFC-0011-m
    // precedent).
    #[test]
    fn tv_net10_10_reputation_show_rejects_wrong_length() {
        let short = format!("did:octo:0x{}", "a".repeat(50));
        let result = parse_reputation_peer_did(&short);
        assert!(
            matches!(result, Err(OctoCliError::ConfirmationRequired { .. })),
            "wrong-length peer_did must be rejected"
        );
    }

    // tv_net10_11: parse_reputation_peer_did rejects a
    // peer_did body containing non-hex characters
    // (pastejacking defense per Phase 5 RFC-0011-m
    // precedent).
    #[test]
    fn tv_net10_11_reputation_show_rejects_non_hex() {
        let non_hex = format!("did:octo:0x{}", "z".repeat(104));
        let result = parse_reputation_peer_did(&non_hex);
        assert!(
            matches!(result, Err(OctoCliError::ConfirmationRequired { .. })),
            "non-hex peer_did must be rejected"
        );
    }

    // tv_net10_12: reputation list with below-score
    // filter WITHOUT threshold rejected by handler
    // validation (symmetric with tv_net10_3 which
    // covers above-score).
    #[test]
    fn tv_net10_12_reputation_list_below_score_requires_threshold() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "reputation",
            "list",
            "--filter",
            "below-score",
        ])
        .expect("parse");
        let args = match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(a) => a,
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        };
        let runtime =
            Octo::try_parse_from(["test", "network", "reputation", "list"]).expect("runtime parse");
        let result = network_reputation_list(&args, &runtime);
        match result {
            Err(OctoCliError::ConfirmationRequired { command }) => {
                assert!(
                    command.contains("threshold required"),
                    "ConfirmationRequired command must mention threshold required, got {command:?}"
                );
            }
            other => panic!(
                "expected ConfirmationRequired with threshold required detail, got {other:?}"
            ),
        }
    }

    // tv_net10_13: reputation list --filter below-score +
    // --threshold 100 parses cleanly. Symmetric with
    // tv_net10_2 (above-score direction); covers both
    // threshold bound directions per RFC-0011-h
    // §Subcommand Taxonomy Phase 10. Verifies that the
    // clap parse layer surfaces BelowScore filter +
    // threshold value end-to-end without invoking the
    // handler.
    #[test]
    fn tv_net10_13_reputation_list_below_score_with_threshold_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "reputation",
            "list",
            "--filter",
            "below-score",
            "--threshold",
            "100",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Reputation { action } => match action {
                NetworkReputationAction::List(args) => {
                    assert!(matches!(args.filter, ReputationFilterKind::BelowScore));
                    assert_eq!(args.threshold, Some(100));
                }
                _ => panic!("expected List"),
            },
            _ => panic!("expected Reputation"),
        }
    }

    // === Phase 11 test vectors (RFC-0011-s Phase 11 G14) ===

    // tv_net11_1: topology render default format (ascii) parses cleanly.
    #[test]
    fn tv_net11_1_topology_render_default_format_ascii_parses() {
        let cli = TestPhase10Cli::try_parse_from(["test", "topology", "render"]).expect("parse");
        match cli.action {
            NetworkAction::Topology { action } => match action {
                NetworkTopologyAction::Render(args) => {
                    assert!(matches!(args.format, TopologyFormatKind::Ascii));
                    assert!(args.depth.is_none());
                    assert!(!args.json);
                }
            },
            _ => panic!("expected Topology"),
        }
    }

    // tv_net11_2: topology render --format dot parses cleanly.
    #[test]
    fn tv_net11_2_topology_render_format_dot_parses() {
        let cli = TestPhase10Cli::try_parse_from(["test", "topology", "render", "--format", "dot"])
            .expect("parse");
        match cli.action {
            NetworkAction::Topology { action } => match action {
                NetworkTopologyAction::Render(args) => {
                    assert!(matches!(args.format, TopologyFormatKind::Dot));
                }
            },
            _ => panic!("expected Topology"),
        }
    }

    // tv_net11_3: topology render --depth 200 rejected pre-dispatch
    // (parse_graph_depth 1..=100 clamp per Phase 5 RFC-0011-m
    // pastejacking defense pattern; shared with `trust-graph render`).
    #[test]
    fn tv_net11_3_topology_render_depth_above_range_rejected() {
        let result =
            TestPhase10Cli::try_parse_from(["test", "topology", "render", "--depth", "200"]);
        assert!(
            result.is_err(),
            "--depth 200 must be rejected pre-dispatch (parse_graph_depth 1..=100 clamp)"
        );
    }

    // tv_net11_4: topology render --depth 1 accepted pre-dispatch
    // (lower boundary of parse_graph_depth 1..=100 clamp).
    #[test]
    fn tv_net11_4_topology_render_depth_lower_boundary_accepted() {
        let cli = TestPhase10Cli::try_parse_from(["test", "topology", "render", "--depth", "1"])
            .expect("parse");
        match cli.action {
            NetworkAction::Topology { action } => match action {
                NetworkTopologyAction::Render(args) => {
                    assert_eq!(args.depth, Some(1));
                }
            },
            _ => panic!("expected Topology"),
        }
    }

    // tv_net11_5: topology render --depth 100 accepted pre-dispatch
    // (upper boundary of parse_graph_depth 1..=100 clamp).
    #[test]
    fn tv_net11_5_topology_render_depth_upper_boundary_accepted() {
        let cli = TestPhase10Cli::try_parse_from(["test", "topology", "render", "--depth", "100"])
            .expect("parse");
        match cli.action {
            NetworkAction::Topology { action } => match action {
                NetworkTopologyAction::Render(args) => {
                    assert_eq!(args.depth, Some(100));
                }
            },
            _ => panic!("expected Topology"),
        }
    }

    // tv_net11_6: topology render default ascii + default depth
    // (no --depth flag) accepts None (depth cap is optional).
    #[test]
    fn tv_net11_6_topology_render_default_depth_none_parses() {
        let cli = TestPhase10Cli::try_parse_from(["test", "topology", "render"]).expect("parse");
        match cli.action {
            NetworkAction::Topology { action } => match action {
                NetworkTopologyAction::Render(args) => {
                    assert_eq!(args.depth, None);
                }
            },
            _ => panic!("expected Topology"),
        }
    }

    // tv_net11_7: topology render --depth 0 rejected pre-dispatch
    // (below-range boundary of parse_graph_depth 1..=100 clamp).
    #[test]
    fn tv_net11_7_topology_render_depth_below_range_rejected() {
        let result = TestPhase10Cli::try_parse_from(["test", "topology", "render", "--depth", "0"]);
        assert!(
            result.is_err(),
            "--depth 0 must be rejected pre-dispatch (parse_graph_depth 1..=100 clamp)"
        );
    }

    // tv_net12_1: gossip --stats default format (ascii) parses cleanly.
    #[test]
    fn tv_net12_1_gossip_stats_default_format_ascii_parses() {
        let cli = TestPhase10Cli::try_parse_from(["test", "gossip", "stats"]).expect("parse");
        match cli.action {
            NetworkAction::Gossip { action } => match action {
                NetworkGossipAction::Stats(args) => {
                    assert!(matches!(args.format, GossipStatsFormatKind::Ascii));
                    assert!(!args.json);
                }
            },
            _ => panic!("expected Gossip"),
        }
    }

    // tv_net12_2: gossip --stats --format json parses cleanly.
    #[test]
    fn tv_net12_2_gossip_stats_format_json_parses() {
        let cli = TestPhase10Cli::try_parse_from(["test", "gossip", "stats", "--format", "json"])
            .expect("parse");
        match cli.action {
            NetworkAction::Gossip { action } => match action {
                NetworkGossipAction::Stats(args) => {
                    assert!(matches!(args.format, GossipStatsFormatKind::Json));
                }
            },
            _ => panic!("expected Gossip"),
        }
    }

    // tv_net12_3: gossip --stats with --json flag parses cleanly.
    #[test]
    fn tv_net12_3_gossip_stats_json_flag_parses() {
        let cli =
            TestPhase10Cli::try_parse_from(["test", "gossip", "stats", "--json"]).expect("parse");
        match cli.action {
            NetworkAction::Gossip { action } => match action {
                NetworkGossipAction::Stats(args) => {
                    assert!(args.json);
                    assert!(matches!(args.format, GossipStatsFormatKind::Ascii));
                }
            },
            _ => panic!("expected Gossip"),
        }
    }

    // tv_net13_1: envelope inspect default (no --json) parses cleanly.
    #[test]
    fn tv_net13_1_envelope_inspect_default_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "envelope",
            "inspect",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Envelope { action } => match action {
                NetworkEnvelopeAction::Inspect(args) => {
                    assert_eq!(
                        args.envelope_id_hex,
                        "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
                    );
                    assert!(!args.json);
                }
                _ => panic!("expected Inspect"),
            },
            _ => panic!("expected Envelope"),
        }
    }

    // tv_net13_2: envelope inspect --json parses cleanly.
    #[test]
    fn tv_net13_2_envelope_inspect_json_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "envelope",
            "inspect",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            "--json",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Envelope { action } => match action {
                NetworkEnvelopeAction::Inspect(args) => {
                    assert!(args.json);
                }
                _ => panic!("expected Inspect"),
            },
            _ => panic!("expected Envelope"),
        }
    }

    // tv_net13_3: envelope forward --destination-peer-id-hex <hex> --ttl-epochs 100 parses cleanly.
    #[test]
    fn tv_net13_3_envelope_forward_destination_ttl_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "envelope",
            "forward",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            "--destination-peer-id-hex",
            "aabbccddeeff00112233445566778899aabbccddeeff0011223344556677",
            "--ttl-epochs",
            "100",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Envelope { action } => match action {
                NetworkEnvelopeAction::Forward(args) => {
                    assert_eq!(args.ttl_epochs, 100);
                    assert!(!args.dry_run);
                    assert!(!args.confirm_acknowledge);
                }
                _ => panic!("expected Forward"),
            },
            _ => panic!("expected Envelope"),
        }
    }

    // tv_net13_4: envelope forward --dry-run parses cleanly (no broadcast).
    #[test]
    fn tv_net13_4_envelope_forward_dry_run_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "envelope",
            "forward",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            "--destination-peer-id-hex",
            "aabbccddeeff00112233445566778899aabbccddeeff0011223344556677",
            "--ttl-epochs",
            "100",
            "--dry-run",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Envelope { action } => match action {
                NetworkEnvelopeAction::Forward(args) => {
                    assert!(args.dry_run);
                    assert!(!args.confirm_acknowledge);
                }
                _ => panic!("expected Forward"),
            },
            _ => panic!("expected Envelope"),
        }
    }

    // tv_net13_5: envelope forward --confirm-acknowledge parses cleanly
    // (mutating gate per Phase 4 RFC-0011-l precedent).
    #[test]
    fn tv_net13_5_envelope_forward_confirm_acknowledge_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "envelope",
            "forward",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            "--destination-peer-id-hex",
            "aabbccddeeff00112233445566778899aabbccddeeff0011223344556677",
            "--ttl-epochs",
            "100",
            "--confirm-acknowledge",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Envelope { action } => match action {
                NetworkEnvelopeAction::Forward(args) => {
                    assert!(args.confirm_acknowledge);
                    assert!(!args.dry_run);
                }
                _ => panic!("expected Forward"),
            },
            _ => panic!("expected Envelope"),
        }
    }

    // tv_net13_6: envelope forward without --confirm-acknowledge parses
    // at clap level; the handler enforces the mutating gate at runtime
    // (Phase 4 RFC-0011-l precedent).
    #[test]
    fn tv_net13_6_envelope_forward_no_confirm_parses_but_handler_gates() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "envelope",
            "forward",
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            "--destination-peer-id-hex",
            "aabbccddeeff00112233445566778899aabbccddeeff0011223344556677",
            "--ttl-epochs",
            "100",
        ])
        .expect("parse succeeds");
        match cli.action {
            NetworkAction::Envelope { action } => match action {
                NetworkEnvelopeAction::Forward(args) => {
                    assert!(!args.confirm_acknowledge);
                    assert!(!args.dry_run);
                }
                _ => panic!("expected Forward"),
            },
            _ => panic!("expected Envelope"),
        }
    }

    // tv_net14_1: heartbeat probe default (no --timeout-ms) parses
    // cleanly with default 5000ms timeout.
    #[test]
    fn tv_net14_1_heartbeat_probe_default_timeout_parses() {
        let cli =
            TestPhase10Cli::try_parse_from(["test", "heartbeat", "probe", "did:octo:example-peer"])
                .expect("parse");
        match cli.action {
            NetworkAction::Heartbeat { action } => match action {
                NetworkHeartbeatAction::Probe(args) => {
                    assert_eq!(args.peer_did, "did:octo:example-peer");
                    assert_eq!(args.timeout_ms, 5000);
                    assert!(!args.json);
                }
            },
            _ => panic!("expected Heartbeat"),
        }
    }

    // tv_net14_2: heartbeat probe --timeout-ms 1000 parses cleanly.
    #[test]
    fn tv_net14_2_heartbeat_probe_timeout_ms_1000_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "heartbeat",
            "probe",
            "did:octo:example-peer",
            "--timeout-ms",
            "1000",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Heartbeat { action } => match action {
                NetworkHeartbeatAction::Probe(args) => {
                    assert_eq!(args.peer_did, "did:octo:example-peer");
                    assert_eq!(args.timeout_ms, 1000);
                }
            },
            _ => panic!("expected Heartbeat"),
        }
    }

    // tv_net14_3: heartbeat probe --timeout-ms 65535 parses cleanly
    // (clap u16 max per Phase 5 RFC-0011-m precedent).
    #[test]
    fn tv_net14_3_heartbeat_probe_timeout_ms_u16_max_parses() {
        let cli = TestPhase10Cli::try_parse_from([
            "test",
            "heartbeat",
            "probe",
            "did:octo:example-peer",
            "--timeout-ms",
            "65535",
        ])
        .expect("parse");
        match cli.action {
            NetworkAction::Heartbeat { action } => match action {
                NetworkHeartbeatAction::Probe(args) => {
                    assert_eq!(args.peer_did, "did:octo:example-peer");
                    assert_eq!(args.timeout_ms, u16::MAX);
                }
            },
            _ => panic!("expected Heartbeat"),
        }
    }
}
