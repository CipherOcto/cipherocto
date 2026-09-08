//! `octo mesh peer <list|add|remove>` — RFC-0011-f §Subcommand Taxonomy.
//!
//! Layer C wrapper over the `octo_mesh` substrate crate. Operator
//! invocation → clap parse → canonical DID validation (RFC-0010) →
//! substrate call → JSON envelope render.
//!
//! ## Mode gating (RFC-0011-f §Roles and Authorities)
//!
//! | Subcommand | Mode   | Required flags                           |
//! |------------|--------|------------------------------------------|
//! | `list`     | All    | (none — read-only)                       |
//! | `add`      | Human  | `--confirm` + `--confirm-acknowledge`    |
//! | `add`      | Ci/Dev | `--allow-write`                          |
//! | `add`      | Audit  | (denied)                                 |
//! | `remove`   | Human  | `--confirm` + `--confirm-acknowledge`    |
//! | `remove`   | Ci/Dev | `--allow-write`                          |
//! | `remove`   | Audit  | (denied)                                 |
//!
//! All mutating subcommands participate in the canonical
//! `require_confirm` gate shared with `role select` and `identity
//! rotate/revoke`. `peer list` is read-only and bypasses the gate.

use std::time::{SystemTime, UNIX_EPOCH};

use clap::Subcommand;
use octo_mesh::{
    add_peer as substrate_add_peer, list_peers as substrate_list_peers,
    remove_peer as substrate_remove_peer, trust_level_uuids, EndpointUri, MeshError, PeerFilter,
    PeerSummary, TrustLevel,
};

use crate::commands::identity::require_confirm;
use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::home;
use crate::output::OutputEnvelope;
use crate::Octo;

/// CLI-facing peer subcommand enum (Layer C; delegates to
/// `octo_mesh` substrate for decisions). `#[non_exhaustive]` per F-14.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PeerAction {
    /// List registered peers.
    ///
    /// `--filter-trust` is repeatable (inclusive-set semantics across
    /// values; empty filter = all peers). Per RFC-0011-f §Subcommand
    /// Taxonomy "Flags: --filter-trust <LEVEL> (repeatable; canonical
    /// UUID substrings)".
    List {
        /// Filter by trust level UUID substring (repeatable).
        /// Accepts `trusted`, `verified`, `untrusted` shorthand or
        /// the full canonical UUID.
        #[arg(long = "filter-trust", value_name = "LEVEL")]
        filter_trust: Vec<String>,
    },
    /// Add a peer to the local peer table.
    Add {
        /// Peer DID (RFC-0010 canonical `did:octo:z<base58btc>` wire
        /// form; legacy `did:octo:b<base32>` rejected via the
        /// `allow_legacy_bare=false` codec gate at dispatch).
        peer_did: String,
        /// Endpoint URI (allowlisted schemes: `tcp://`, `quic://`,
        /// `bluetooth://`).
        #[arg(long, value_name = "URI")]
        endpoint: String,
    },
    /// Remove a peer from the local peer table (idempotent).
    Remove {
        /// Peer DID (RFC-0010 canonical wire form).
        peer_did: String,
    },
}

// ---------------------------------------------------------------------------
// CLI-side output types — RFC-0011-f §Output Envelope
// ---------------------------------------------------------------------------
//
// Schema version is carried by `OutputEnvelope::SCHEMA_VERSION` (the
// RFC pins mesh envelope payload types under schema_version 5 per
// RFC-0011-c §9.4.1 slot table; the envelope wrapper carries the
// single global schema_version constant and the data: T parameter
// gains these three new payload types — RFC-0011-f §Compatibility).

/// `octo mesh peer list` payload.
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct PeerListOutput {
    /// The peers that matched the filter (sorted by `(trust_level
    /// ASC, peer_did LEX)` per RFC-0011-f §Test Vectors TV-1).
    pub peers: Vec<PeerSummary>,
    /// Total peer count BEFORE the filter was applied (so the
    /// operator can see the size of the underlying table even when
    /// the filter is restrictive).
    pub total_count: usize,
    /// Number of peers returned AFTER the filter (matches `peers.len()`).
    pub filtered_count: usize,
}

/// `octo mesh peer add` payload.
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct PeerAddOutput {
    /// Peer DID (canonical wire form).
    pub peer_did: String,
    /// Endpoint URI that was persisted.
    pub endpoint: EndpointUri,
    /// Initial trust level (always `Untrusted` on `add` — promotion
    /// to Verified / Trusted happens via substrate signals per
    /// RFC-0011-f §Rationale).
    pub trust_level: TrustLevel,
    /// Unix seconds of the `add_peer` call (used as the initial
    /// `last_seen_unix` value — 0 means "never seen" so the
    /// operator can distinguish freshly-added from actively-seen).
    pub added_at_unix: i64,
}

/// `octo mesh peer remove` payload.
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct PeerRemoveOutput {
    /// Peer DID (canonical wire form).
    pub peer_did: String,
    /// `true` when the peer was present and removed; `false` when
    /// the peer was absent (idempotent — no error).
    pub removed: bool,
    /// Unix seconds of the `remove_peer` call.
    pub removed_at_unix: i64,
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch a parsed `octo mesh peer ...` invocation to its handler.
pub fn dispatch(action: &PeerAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        PeerAction::List { filter_trust } => list_peers_cmd(filter_trust.clone(), cli),
        PeerAction::Add { peer_did, endpoint } => {
            require_confirm(cli, "mesh peer add")?;
            add_peer_cmd(peer_did.clone(), endpoint.clone(), cli)
        }
        PeerAction::Remove { peer_did } => {
            require_confirm(cli, "mesh peer remove")?;
            remove_peer_cmd(peer_did.clone(), cli)
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Current unix seconds. Wall clock; substrate records are timestamped
/// at the dispatch boundary so operator-visible `added_at_unix` /
/// `removed_at_unix` matches the substrate record.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `octo mesh peer list [--filter-trust <LEVEL>]...`.
///
/// Read-only — no confirmation gate. `--filter-trust` is repeatable;
/// values are matched against the canonical trust-level UUIDs via
/// shorthand expansion (`trusted` / `verified` / `untrusted`) or
/// verbatim UUID.
fn list_peers_cmd(filter_trust: Vec<String>, cli: &Octo) -> Result<(), OctoCliError> {
    // Translate operator shorthand into canonical TrustLevel UUIDs.
    let trust_levels: Vec<TrustLevel> = filter_trust
        .iter()
        .map(|s| parse_trust_level_filter(s))
        .collect::<Result<Vec<_>, _>>()?;

    let filter = PeerFilter { trust_levels };
    let octo_home = home::resolve()?;
    let summaries = substrate_list_peers(&filter, &octo_home).map_err(map_mesh_error)?;

    // `total_count` is the size of the underlying table (filter NOT
    // applied). Load again with empty filter to compute.
    let total_filter = PeerFilter::default();
    let total = substrate_list_peers(&total_filter, &octo_home)
        .map_err(map_mesh_error)?
        .len();

    let output = PeerListOutput {
        peers: summaries.clone(),
        total_count: total,
        filtered_count: summaries.len(),
    };
    render_envelope("octo.mesh.peer.list.v1", output, cli)
}

/// `octo mesh peer add <PEER_DID> --endpoint <URI>`.
///
/// Validates DID canonical wire form (rejects legacy) + endpoint URI
/// allowlist BEFORE any filesystem write per RFC-0011-f §Error
/// Handling + §Security 2. Both gates run via `octo_mesh::add_peer`.
fn add_peer_cmd(peer_did: String, endpoint: String, cli: &Octo) -> Result<(), OctoCliError> {
    let uri = EndpointUri::parse(&endpoint).map_err(map_mesh_error)?;
    let octo_home = home::resolve()?;
    let now = now_unix();
    substrate_add_peer(&peer_did, &uri, &octo_home, now).map_err(map_mesh_error)?;

    let output = PeerAddOutput {
        peer_did,
        endpoint: uri,
        trust_level: TrustLevel::untrusted(),
        added_at_unix: now,
    };
    render_envelope("octo.mesh.peer.add.v1", output, cli)
}

/// `octo mesh peer remove <PEER_DID>`.
///
/// Idempotent — returns `removed: false` when peer was absent. No
/// confirmation required for absent-peer case (substrate gate fires
/// only on write).
fn remove_peer_cmd(peer_did: String, cli: &Octo) -> Result<(), OctoCliError> {
    let octo_home = home::resolve()?;
    let now = now_unix();
    // Capture "was-present" by listing before the remove call.
    let filter = PeerFilter::default();
    let was_present = substrate_list_peers(&filter, &octo_home)
        .map_err(map_mesh_error)?
        .iter()
        .any(|p| p.peer_did == peer_did);

    substrate_remove_peer(&peer_did, &octo_home, now).map_err(map_mesh_error)?;

    let output = PeerRemoveOutput {
        peer_did,
        removed: was_present,
        removed_at_unix: now,
    };
    render_envelope("octo.mesh.peer.remove.v1", output, cli)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Translate operator `--filter-trust` shorthand into the canonical
/// `TrustLevel` UUID. Accepts:
///
/// - Canonical UUID verbatim.
/// - Short-hands: `trusted`, `verified`, `untrusted` (case-insensitive).
///
/// Unknown values fail closed with `OctoCliError::InvalidFilter` (exit
/// 16, RFC-0011-f §Subcommand Taxonomy).
fn parse_trust_level_filter(s: &str) -> Result<TrustLevel, OctoCliError> {
    let normalised = s.trim().to_lowercase();
    // Short-hand fast-path.
    let candidate: &str = match normalised.as_str() {
        "trusted" => trust_level_uuids::TRUSTED,
        "verified" => trust_level_uuids::VERIFIED,
        "untrusted" => trust_level_uuids::UNTRUSTED,
        _ => s,
    };

    // Validate candidate is one of the 3 canonical UUIDs (substring
    // match is unsafe — multiple UUIDs could share substrings).
    for known in [
        trust_level_uuids::TRUSTED,
        trust_level_uuids::VERIFIED,
        trust_level_uuids::UNTRUSTED,
    ] {
        if candidate == known {
            return Ok(TrustLevel::new(known));
        }
    }
    Err(OctoCliError::InvalidFilter(sanitize_substrate_error(
        &format!("--filter-trust `{s}` does not match any canonical trust-level UUID (allowed: trusted, verified, untrusted)"),
    )))
}

/// Map substrate [`MeshError`] to operator-facing [`OctoCliError`].
///
/// Exit-code contract:
/// - `InvalidDidShape` → `IdentityNotFound` (exit 4) per RFC-0011-f
///   §Error Handling + mission YAML §Acceptance Criteria.
/// - `InvalidEndpointScheme` → `InvalidEndpointScheme` (exit 28)
///   per RFC-0011-f §Exit Codes (shared slot with `InvalidTtlHops`).
/// - `UnknownMethod` / `RpcTimeout` → `Internal` (exit 64). The
///   peer-table path cannot surface these (it never invokes the
///   RPC substrate); they are mapped for completeness so a future
///   code path that reuses this mapper doesn't introduce a
///   non-exhaustive match warning. The `mesh rpc` dispatch uses
///   [`super::mesh::map_rpc_substrate_error`] for the operator-
///   facing mapping (exit 19 / 20).
/// - `NoOctoHome` → `NoOctoHome` (exit 27). Wave 5.5 F1: the peer
///   table is created lazily under `$OCTO_HOME/mesh/peers.toml` so
///   the substrate's env-var resolution can surface `NoOctoHome`
///   when the operator's environment is bare.
/// - `Io` / `TomlParse` / `TomlSerialise` → `Internal` (exit 64).
///
/// Wildcard arm: `MeshError` is `#[non_exhaustive]` (Wave 5.5 F1);
/// future substrate variants fail closed to `Internal`.
fn map_mesh_error(e: MeshError) -> OctoCliError {
    match e {
        MeshError::InvalidDidShape(did) => OctoCliError::IdentityNotFound(did),
        MeshError::InvalidEndpointScheme { scheme } => {
            OctoCliError::InvalidEndpointScheme { scheme }
        }
        MeshError::UnknownMethod { method } => OctoCliError::Internal(sanitize_substrate_error(
            &format!("mesh peer path encountered unexpected UnknownMethod `{method}`; peer table never invokes the RPC substrate — report this as a bug"),
        )),
        MeshError::RpcTimeout {
            peer,
            method,
            timeout_ms,
        } => OctoCliError::Internal(sanitize_substrate_error(&format!(
            "mesh peer path encountered unexpected RpcTimeout peer={peer} method={method} timeout_ms={timeout_ms}; peer table never invokes the RPC substrate — report this as a bug"
        ))),
        MeshError::NoOctoHome => OctoCliError::NoOctoHome,
        MeshError::Io(msg) | MeshError::TomlParse(msg) | MeshError::TomlSerialise(msg) => {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "mesh peer table I/O: {msg}"
            )))
        }
        other => OctoCliError::Internal(sanitize_substrate_error(&format!(
            "mesh peer table substrate (unmapped variant): {other:?}"
        ))),
    }
}

/// Render an output envelope for the given payload (serializable).
///
/// Mirrors the `role::render_envelope` / `reputation::render_envelope`
/// helper — schema string is the `OutputEnvelope::command` field
/// per RFC-0011-c §9.4.
fn render_envelope<T: serde::Serialize>(
    schema: &'static str,
    data: T,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let env = OutputEnvelope::new(schema, data);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trust_level_filter_accepts_canonical() {
        assert_eq!(
            parse_trust_level_filter(trust_level_uuids::TRUSTED).unwrap(),
            TrustLevel::new(trust_level_uuids::TRUSTED)
        );
        assert_eq!(
            parse_trust_level_filter(trust_level_uuids::VERIFIED).unwrap(),
            TrustLevel::new(trust_level_uuids::VERIFIED)
        );
        assert_eq!(
            parse_trust_level_filter(trust_level_uuids::UNTRUSTED).unwrap(),
            TrustLevel::new(trust_level_uuids::UNTRUSTED)
        );
    }

    #[test]
    fn parse_trust_level_filter_accepts_shorthand() {
        assert_eq!(
            parse_trust_level_filter("trusted").unwrap(),
            TrustLevel::new(trust_level_uuids::TRUSTED)
        );
        assert_eq!(
            parse_trust_level_filter("VERIFIED").unwrap(),
            TrustLevel::new(trust_level_uuids::VERIFIED)
        );
        assert_eq!(
            parse_trust_level_filter("UnTrusted").unwrap(),
            TrustLevel::new(trust_level_uuids::UNTRUSTED)
        );
    }

    #[test]
    fn parse_trust_level_filter_rejects_unknown() {
        let err = parse_trust_level_filter("bogus").unwrap_err();
        assert!(matches!(err, OctoCliError::InvalidFilter(_)));
    }

    #[test]
    fn map_mesh_error_invalid_did_shape_becomes_identity_not_found() {
        let e = MeshError::InvalidDidShape("did:octo:b<legacy>".to_string());
        assert!(matches!(
            map_mesh_error(e),
            OctoCliError::IdentityNotFound(_)
        ));
    }

    #[test]
    fn map_mesh_error_invalid_endpoint_scheme_becomes_invalid_endpoint_scheme() {
        let e = MeshError::InvalidEndpointScheme {
            scheme: "file".to_string(),
        };
        assert!(matches!(
            map_mesh_error(e),
            OctoCliError::InvalidEndpointScheme { .. }
        ));
    }

    #[test]
    fn map_mesh_error_io_becomes_internal() {
        let e = MeshError::Io("disk full".to_string());
        assert!(matches!(map_mesh_error(e), OctoCliError::Internal(_)));
    }
}
