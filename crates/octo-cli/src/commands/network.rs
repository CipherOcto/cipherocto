//! `octo network` - RFC-0011-i Phase 1 (peers + identity + trust-graph
//! + governance rotation read-only observability).
//!
//! Layer C orchestrator over the Layer-B `octo-network` substrate
//! (`GatewayCache::iter/get`, `LocalGatewayIdentity::load`,
//! `TrustGraph::render`, `GovernanceRotation::has_quorum +
//! migration_deadline + in_migration_window`). Output renders
//! through `OutputEnvelope<T>` per RFC-0011 §Output Envelope +
//! RFC-0011-c §9.4 v4 wire form.
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
//! ## Operator-mode constraint
//!
//! All five Phase 1 subcommands are READ-ONLY. No operator-mode
//! gating is required at the dispatch boundary. Auditor mode is
//! honoured transparently (same wire form, no `--status`-style
//! filtering applies).

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
use octo_network::mon::governance_rotation::GovernanceRotation;
use octo_network::mon::local_gateway_identity::LocalGatewayIdentity;
use octo_network::mon::trust_graph::{GraphFormat, TrustGraph};

// === Subcommand taxonomy (RFC-0011-i §Subcommand Taxonomy Phase 1) ===

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
        let NetworkGovernanceAction::Rotation { action } = cli.action;
        let NetworkGovernanceRotationAction::Status(args) = action;
        assert_eq!(args.did_codec, "did:octo:router:abc");
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
}
