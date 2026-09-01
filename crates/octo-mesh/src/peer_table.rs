//! Peer-table persistence + substrate functions (RFC-0011-f).
//!
//! Owns:
//! - Atomic TOML read/write of `$OCTO_HOME/mesh/peers.toml`
//!   (0700 perms, write-to-tmp + fsync + rename).
//! - Substrate functions: `add_peer`, `remove_peer`, `list_peers`.
//!
//! Split from `peer.rs` per Wave 1.5 hygiene callout. The
//! persistence path lives in this module so the substrate types
//! (`TrustLevel`, `EndpointUri`, `PeerRecord`/`PeerSummary`/
//! `PeerFilter`) stay free of filesystem concerns.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::endpoint::EndpointUri;
use crate::error::MeshError;
use crate::peer_record::{PeerFilter, PeerRecord, PeerSummary};
use crate::trust_level::TrustLevel;
// Bring `DidCodec::parse` into scope — the trait is implemented on
// `CanonicalCodec` but the method is a trait method, not an inherent
// associated function.
use octo_ident::DidCodec;

/// TOML on-disk schema wrapper. The inner `Vec<PeerRecord>` is the
/// substrate truth; the wrapper carries the schema version for
/// future migrations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PeerTableFile {
    /// Schema version (RFC-0011-f §Compatibility "schema_version
    /// discipline"). v1 = initial landing.
    schema_version: u32,
    /// Peer records (substrate truth).
    peers: Vec<PeerRecord>,
}

const PEER_TABLE_SCHEMA_VERSION: u32 = 1;

/// Resolve the canonical peer-table path for an `$OCTO_HOME`.
#[must_use]
pub fn peer_table_path(octo_home: &Path) -> PathBuf {
    octo_home.join("mesh").join("peers.toml")
}

/// Ensure `$OCTO_HOME/mesh/` exists with 0700 permissions. Idempotent.
fn ensure_mesh_dir(octo_home: &Path) -> Result<PathBuf, MeshError> {
    let dir = octo_home.join("mesh");
    fs::create_dir_all(&dir)
        .map_err(|e| MeshError::Io(format!("create mesh dir {}: {e}", dir.display())))?;
    let perms = std::fs::Permissions::from_mode(0o700);
    fs::set_permissions(&dir, perms)
        .map_err(|e| MeshError::Io(format!("set mesh dir perms: {e}")))?;
    Ok(dir)
}

/// Load the peer table from disk. Returns an empty table if the file
/// does not yet exist (v1.0 initial state — first `add_peer` creates
/// the file).
fn load_table(path: &Path) -> Result<PeerTableFile, MeshError> {
    if !path.exists() {
        return Ok(PeerTableFile {
            schema_version: PEER_TABLE_SCHEMA_VERSION,
            peers: Vec::new(),
        });
    }
    let raw = fs::read_to_string(path)
        .map_err(|e| MeshError::Io(format!("read {}: {e}", path.display())))?;
    let parsed: PeerTableFile = toml::from_str(&raw)
        .map_err(|e| MeshError::TomlParse(format!("{}: {e}", path.display())))?;
    Ok(parsed)
}

/// Write the peer table atomically: write to `<path>.tmp`, fsync,
/// rename to final path, set 0700 perms.
fn write_table_atomic(path: &Path, table: &PeerTableFile) -> Result<(), MeshError> {
    let dir = path.parent().ok_or_else(|| {
        MeshError::Io(format!("peer table has no parent dir: {}", path.display()))
    })?;
    ensure_mesh_dir(dir)?;

    let tmp = path.with_extension("toml.tmp");
    let serialised =
        toml::to_string_pretty(table).map_err(|e| MeshError::TomlSerialise(e.to_string()))?;

    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| MeshError::Io(format!("create {}: {e}", tmp.display())))?;
        f.write_all(serialised.as_bytes())
            .map_err(|e| MeshError::Io(format!("write {}: {e}", tmp.display())))?;
        f.sync_all()
            .map_err(|e| MeshError::Io(format!("fsync {}: {e}", tmp.display())))?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        MeshError::Io(format!(
            "rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        ))
    })?;
    let perms = std::fs::Permissions::from_mode(0o700);
    fs::set_permissions(path, perms)
        .map_err(|e| MeshError::Io(format!("set perms on {}: {e}", path.display())))?;
    Ok(())
}

/// Validate a peer DID against the canonical RFC-0010 wire form via
/// `octo_ident::CanonicalCodec::parse(s, allow_legacy_bare=false)`.
///
/// Returns `MeshError::InvalidDidShape` on any failure — the CLI maps
/// this to `OctoCliError::IdentityNotFound` (exit 4) per RFC-0011-f
/// §Error Handling.
pub(crate) fn validate_peer_did(peer_did: &str) -> Result<(), MeshError> {
    octo_ident::CanonicalCodec::parse(peer_did, false)
        .map(|_| ())
        .map_err(|e| MeshError::InvalidDidShape(format!("{peer_did}: {e}")))
}

/// Add a peer to the operator's local peer table (atomic TOML write).
///
/// Validates the DID wire form + endpoint URI allowlist BEFORE any
/// filesystem write so a rejected call never mutates the table.
/// `peer_did` collision is a silent overwrite (v1.0 — last-writer-wins;
/// per RFC-0011-f §Compatibility the table is locally-scoped).
///
/// # Errors
///
/// Returns [`MeshError::InvalidDidShape`] when `peer_did` is not the
/// canonical RFC-0010 wire form; returns [`MeshError::Io`] /
/// [`MeshError::TomlSerialise`] on filesystem failures.
pub fn add_peer(
    peer_did: &str,
    endpoint: &EndpointUri,
    octo_home: &Path,
    now_unix: i64,
) -> Result<(), MeshError> {
    validate_peer_did(peer_did)?;

    let path = peer_table_path(octo_home);
    let mut table = load_table(&path)?;

    // Upsert: replace existing record by `peer_did` (last-writer-wins
    // for v1.0 — RFC-0011-f §Compatibility "schema_version discipline").
    let new_record = PeerRecord {
        peer_did: peer_did.to_string(),
        endpoint: endpoint.clone(),
        trust_level: TrustLevel::untrusted(),
        last_seen_unix: now_unix,
        capabilities: Vec::new(),
    };
    let mut replaced = false;
    for existing in &mut table.peers {
        if existing.peer_did == peer_did {
            *existing = new_record.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        table.peers.push(new_record);
    }

    write_table_atomic(&path, &table)?;
    // `now_unix` reserved for future "last_modified_unix" audit row
    // (RFC-0011-f §Implicit Assumptions Audit). Currently unused after
    // the record is built; suppress the lint explicitly.
    let _ = now_unix;
    Ok(())
}

/// Remove a peer from the operator's local peer table (idempotent).
///
/// Returns `Ok(())` whether or not the peer was present. Does NOT
/// contact the peer — local table mutation only (RFC-0011-f §Subcommand
/// Taxonomy `peer remove`).
///
/// # Errors
///
/// Returns [`MeshError::InvalidDidShape`] when `peer_did` is not the
/// canonical RFC-0010 wire form; returns [`MeshError::Io`] on
/// filesystem failures.
pub fn remove_peer(peer_did: &str, octo_home: &Path, now_unix: i64) -> Result<(), MeshError> {
    validate_peer_did(peer_did)?;

    let path = peer_table_path(octo_home);
    let mut table = load_table(&path)?;

    let before = table.peers.len();
    table.peers.retain(|p| p.peer_did != peer_did);
    if table.peers.len() != before {
        write_table_atomic(&path, &table)?;
    }
    // `now_unix` reserved for future audit row (RFC-0011-f §Implicit
    // Assumptions Audit). The idempotent no-op path intentionally
    // doesn't write the file when nothing changed.
    let _ = now_unix;
    Ok(())
}

/// List peers matching `filter`.
///
/// Output sorted by `(trust_level ASC, peer_did LEX)` per RFC-0011-f
/// §Test Vectors TV-1. Sorting is stable: equal keys preserve
/// insertion order.
///
/// # Errors
///
/// Returns [`MeshError::Io`] on filesystem read failures.
pub fn list_peers(filter: &PeerFilter, octo_home: &Path) -> Result<Vec<PeerSummary>, MeshError> {
    let path = peer_table_path(octo_home);
    let table = load_table(&path)?;

    let mut summaries: Vec<PeerSummary> = table
        .peers
        .into_iter()
        .map(PeerSummary::from)
        .filter(|p| filter.matches(p))
        .collect();

    summaries.sort_by(|a, b| {
        // TrustLevel ASC: lower UUID byte-lex wins; the canonical UUIDs
        // are monotonic in the lower byte so a lexicographic compare
        // yields Trusted < Verified < Untrusted per RFC-0011-f §Test
        // Vectors TV-1.
        a.trust_level
            .0
            .cmp(&b.trust_level.0)
            .then_with(|| a.peer_did.cmp(&b.peer_did))
    });

    Ok(summaries)
}
