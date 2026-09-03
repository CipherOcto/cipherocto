//! `octo-mesh` — CipherOcto mesh peer-table substrate (RFC-0011-f).
//!
//! Layer C substrate per [[cipherocto-design-principles]]. Owns the local
//! peer table (`$OCTO_HOME/mesh/peers.toml`, 0700 perms, atomic write +
//! fsync + rename) and the typed-discriminator `TrustLevel` / `EndpointUri`
//! types the CLI projects onto RFC-0871 envelope history + RFC-0855p-c
//! DomainCoordinator signals (`GroupBinding::state = Bound` +
//! `CoordinatorRecord.state = Active` — no `DomainCoordinatorRecord`
//! wrapper exists in substrate per RFC-0855p-c §2 "DomainCoordinator
//! State and Binding (Layered Split)").
//!
//! `TrustLevel` is a typed-discriminator `String` newtype per
//! [[cipherocto-design-principles]] "Extension over enumeration (no
//! central enums)"; the substrate carries the canonical UUID table and
//! the CLI is the canonical place to aggregate underlying signals into
//! an operator-friendly discriminator (RFC-0011-f §Rationale "Why
//! TrustLevel enum is in the CLI (not substrate)").

#![deny(missing_docs)]

pub mod endpoint;
pub mod error;
pub mod peer;
pub mod peer_record;
pub mod peer_table;
pub mod rpc;
pub mod trust_level;

pub use error::MeshError;
pub use peer::{
    peer_table_path, trust_level_uuids, EndpointUri, PeerFilter, PeerRecord, PeerSummary,
    TrustLevel, ALLOWED_ENDPOINT_SCHEMES,
};
pub use rpc::{rpc_invoke, RpcCorrelation, RpcRequest};

// RFC section names + project identifiers appear verbatim in doc
// comments (e.g., `RFC-0011-f §Output Envelope`, `octo-wallet`).
// `clippy::doc_markdown` (a pedantic lint) would force backticks
// around every such mention — purely noise at this layer.
// `clippy::missing_errors_doc` is satisfied via `# Errors` sections
// on every `Result`-returning function.

use std::path::{Path, PathBuf};

/// Default `$OCTO_HOME` resolution when the env var is unset.
///
/// Resolution order:
/// 1. `$OCTO_HOME` env var (canonical — wallet substrate + CLI both honour this).
///    An empty value fails closed (Wave 5.5 F1: empty string is treated
///    the same as unset — never produces `Some(PathBuf::from(""))`).
/// 2. `$HOME/.octo` (POSIX convention via `dirs::home_dir()`).
///
/// Fails closed with [`MeshError::NoOctoHome`] when neither env var is
/// set OR `OCTO_HOME` is empty. Per Wave 5.5 F1, the substrate no
/// longer falls back to `/tmp/.octo` — a world-readable/writable
/// directory on shared hosts where any local user could race the
/// operator's writes or inject peer-table entries. The CLI maps this
/// variant to `OctoCliError::NoOctoHome` (exit 27) per RFC-0011 §Error
/// Handling slot allocation.
///
/// `# Errors`
///
/// Returns [`MeshError::NoOctoHome`] when both `OCTO_HOME` and `HOME`
/// are unset/empty.
fn resolve_octo_home() -> Result<PathBuf, MeshError> {
    match std::env::var("OCTO_HOME") {
        Ok(p) if !p.is_empty() => return Ok(PathBuf::from(p)),
        // Empty value falls through to HOME — same as unset per Wave 5.5 F1.
        _ => {}
    }
    match dirs::home_dir() {
        Some(h) => Ok(h.join(".octo")),
        None => Err(MeshError::NoOctoHome),
    }
}

/// Canonical mesh peer-table location: `$OCTO_HOME/mesh/peers.toml`.
///
/// The `mesh/` directory is created lazily by [`peer::add_peer`] /
/// [`peer::list_peers`] at first access; the path resolver itself does
/// not touch the filesystem.
///
/// `# Errors`
///
/// Propagates [`MeshError::NoOctoHome`] when neither `OCTO_HOME` nor
/// `HOME` is set (or `OCTO_HOME` is the empty string) — Wave 5.5 F1
/// fail-closed contract.
pub fn peer_table_path_default() -> Result<PathBuf, MeshError> {
    let home = resolve_octo_home()?;
    Ok(peer_table_path(&home))
}

/// Canonical mesh peer-table location for an explicit `$OCTO_HOME`
/// override (used by tests + the `--octo-home` flag when added).
#[must_use]
pub fn peer_table_path_with_home(octo_home: &Path) -> PathBuf {
    peer_table_path(octo_home)
}

/// Add a peer to the operator's local peer table.
///
/// Atomic write to `$OCTO_HOME/mesh/peers.toml` (0700 perms). Returns
/// `MeshError::InvalidEndpointScheme` if the URI scheme is not in the
/// allowlist (`tcp://`, `quic://`, `bluetooth://`); returns
/// `MeshError::InvalidDidShape` if `peer_did` is not a canonical
/// `did:octo:z<base58btc>` wire form. Both gates run BEFORE any
/// filesystem write so a rejected call never mutates the table.
///
/// # Errors
///
/// Propagates [`MeshError`] from the substrate peer module
/// (invalid DID shape, disallowed endpoint scheme, or filesystem I/O).
pub fn add_peer(
    peer_did: &str,
    endpoint: &EndpointUri,
    octo_home: &Path,
    now_unix: i64,
) -> Result<(), MeshError> {
    peer::add_peer(peer_did, endpoint, octo_home, now_unix)
}

/// Remove a peer from the operator's local peer table.
///
/// Idempotent: returns `Ok(())` whether or not the peer was present
/// (RFC-0011-f §Subcommand Taxonomy `peer remove` — "idempotent;
/// returns Ok(()) if peer not present; does NOT contact the peer").
///
/// # Errors
///
/// Propagates [`MeshError`] from the substrate peer module
/// (invalid DID shape or filesystem I/O).
pub fn remove_peer(peer_did: &str, octo_home: &Path, now_unix: i64) -> Result<(), MeshError> {
    peer::remove_peer(peer_did, octo_home, now_unix)
}

/// List peers matching `filter`.
///
/// AND semantics across `filter.trust_levels` (empty filter = all
/// peers). Output sorted by `(trust_level ASC, peer_did LEX)` per
/// RFC-0011-f §Test Vectors TV-1.
///
/// # Errors
///
/// Propagates [`MeshError`] from the substrate peer module
/// (filesystem I/O when reading the peer table).
pub fn list_peers(filter: &PeerFilter, octo_home: &Path) -> Result<Vec<PeerSummary>, MeshError> {
    peer::list_peers(filter, octo_home)
}
