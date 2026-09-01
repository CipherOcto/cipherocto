//! Mesh peer-table substrate — RFC-0011-f.
//!
//! This module owns:
//! - The `TrustLevel` typed-discriminator newtype (canonical UUID table).
//! - The `EndpointUri` allowlist validator (tcp://, quic://, bluetooth://).
//! - The `PeerSummary` / `PeerRecord` / `PeerFilter` substrate shapes.
//! - The atomic TOML persistence (`$OCTO_HOME/mesh/peers.toml`, 0700 perms,
//!   write-to-tmp + fsync + rename — mirrors `octo-wallet::WalletStore`
//!   discipline per RFC-0011-f §Implicit Assumptions Audit row 7).
//!
//! ## TrustLevel — typed-discriminator, not central enum
//!
//! Per [[cipherocto-design-principles]] "Extension over enumeration (no
//! central enums)", `TrustLevel` is a `String` newtype wrapping a
//! canonical UUID discriminator (RFC-0871 typed-discriminator pattern).
//! New trust signals extend the canonical UUID table without substrate
//! or CLI enum edits; old code fails-closed on unknown discriminators
//! per [[cipherocto-design-principles]] "Open/Closed".

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::MeshError;
// Bring `DidCodec::parse` into scope — the trait is implemented on
// `CanonicalCodec` but the method is a trait method, not an inherent
// associated function.
use octo_ident::DidCodec;

// =============================================================================
// TrustLevel — typed-discriminator UUID newtype
// =============================================================================

/// Canonical UUIDs for the 3 trust levels defined by RFC-0011-f
/// §Trust Level Canonical UUIDs.
///
/// New trust signals extend the canonical UUID table WITHOUT
/// substrate or CLI enum edits (RFC-0011-f §Rationale "Why TrustLevel
/// is a typed-discriminator newtype (not a central enum)").
pub mod trust_level_uuids {
    /// Peer is trusted (RFC-0855p-c `DomainCoordinatorRecord` present +
    /// RFC-0871 envelope handshake history successful).
    pub const TRUSTED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000001";
    /// Peer is verified (RFC-0871 envelope handshake history successful
    /// without RFC-0855p-c DomainCoordinatorRecord).
    pub const VERIFIED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000002";
    /// Peer is untrusted (initial state on add — promotion to
    /// `VERIFIED` / `TRUSTED` happens via subsequent substrate signals,
    /// NOT via the CLI per RFC-0011-f §Rationale).
    pub const UNTRUSTED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000003";
}

/// Typed-discriminator wrapper around the canonical TrustLevel UUID
/// string.
///
/// Per RFC-0011-f §Rationale "Why TrustLevel enum is in the CLI (not
/// substrate)", the substrate carries the underlying signals (RFC-0855p-c
/// `DomainCoordinatorRecord` + RFC-0871 envelope handshake history)
/// separately and the CLI is the canonical place to aggregate them
/// into an operator-friendly discriminator string. The substrate
/// exports this newtype so the CLI can pass UUIDs through without
/// inventing parallel enums.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TrustLevel(pub String);

impl TrustLevel {
    /// Wrap a canonical UUID string.
    #[must_use]
    pub fn new(uuid: &'static str) -> Self {
        Self(uuid.to_string())
    }

    /// `Untrusted` — initial state on `peer add`.
    #[must_use]
    pub fn untrusted() -> Self {
        Self(trust_level_uuids::UNTRUSTED.to_string())
    }

    /// Return the canonical UUID string.
    #[must_use]
    pub fn as_uuid_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// =============================================================================
// EndpointUri — allowlist-validated URI wrapper
// =============================================================================

/// Allowlisted endpoint URI schemes (RFC-0011-f §Peer Summary Shape).
pub const ALLOWED_ENDPOINT_SCHEMES: &[&str] = &["tcp://", "quic://", "bluetooth://"];

/// Wrapper around an endpoint URI string.
///
/// Validated against the [`ALLOWED_ENDPOINT_SCHEMES`] allowlist at
/// construction time. Unknown schemes return `MeshError::InvalidEndpointScheme`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EndpointUri(pub String);

impl EndpointUri {
    /// Parse + validate an endpoint URI string against the allowlist.
    ///
    /// # Errors
    ///
    /// Returns [`MeshError::InvalidEndpointScheme`] when the scheme is
    /// not in [`ALLOWED_ENDPOINT_SCHEMES`] or when the URI payload is
    /// empty after the scheme prefix.
    pub fn parse(s: &str) -> Result<Self, MeshError> {
        for prefix in ALLOWED_ENDPOINT_SCHEMES {
            if let Some(rest) = s.strip_prefix(prefix) {
                if rest.is_empty() {
                    return Err(MeshError::InvalidEndpointScheme {
                        scheme: prefix.trim_end_matches("://").to_string(),
                    });
                }
                return Ok(Self(s.to_string()));
            }
        }
        // Extract the scheme up to `://` for the diagnostic.
        let scheme = s
            .split_once("://")
            .map_or_else(|| s.to_string(), |(scheme, _)| scheme.to_string());
        Err(MeshError::InvalidEndpointScheme { scheme })
    }

    /// Return the inner URI string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EndpointUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// =============================================================================
// PeerRecord / PeerSummary / PeerFilter — substrate shapes
// =============================================================================

/// On-disk substrate record.
///
/// `TrustLevel` is always `Untrusted` on initial `add_peer` (RFC-0011-f
/// §Subcommand Taxonomy `peer add`); promotion to `Verified` / `Trusted`
/// happens via subsequent substrate signals (RFC-0855p-c coordinator
/// discovery + RFC-0871 envelope handshake history) and is NOT
/// triggered by the CLI per RFC-0011-f §Rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PeerRecord {
    /// Canonical RFC-0010 wire form: `did:octo:z<base58btc>`.
    pub peer_did: String,
    /// Allowlisted endpoint URI (tcp://, quic://, bluetooth://).
    pub endpoint: EndpointUri,
    /// Typed-discriminator trust level UUID (initial: `Untrusted`).
    pub trust_level: TrustLevel,
    /// Unix seconds of last successful envelope handshake (0 = never).
    #[serde(default)]
    pub last_seen_unix: i64,
    /// Display-only capability references (RFC-0011-f §Peer Summary
    /// Shape). Verification uses the full `root_id`; this field is the
    /// first 16 hex chars + ellipsis for compact operator rendering.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Operator-facing summary projection (RFC-0011-f §Peer Summary Shape).
///
/// Currently a 1:1 mirror of `PeerRecord`; the type is split so the
/// substrate can grow internal substrate-only fields (e.g., envelope
/// handshake history digests) without breaking the operator surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PeerSummary {
    /// Canonical RFC-0010 wire form: `did:octo:z<base58btc>`.
    pub peer_did: String,
    /// Allowlisted endpoint URI.
    pub endpoint: EndpointUri,
    /// Typed-discriminator trust level UUID.
    pub trust_level: TrustLevel,
    /// Unix seconds of last successful envelope handshake (0 = never).
    pub last_seen_unix: i64,
    /// Display-only capability references.
    pub capabilities: Vec<String>,
}

impl From<PeerRecord> for PeerSummary {
    fn from(r: PeerRecord) -> Self {
        Self {
            peer_did: r.peer_did,
            endpoint: r.endpoint,
            trust_level: r.trust_level,
            last_seen_unix: r.last_seen_unix,
            capabilities: r.capabilities,
        }
    }
}

/// Filter applied at `list_peers` time.
///
/// AND semantics across `trust_levels` (empty filter = all peers).
/// `trust_levels` is the only filter axis in v1.0; future
/// amendments add filters (e.g., capability substring, endpoint
/// scheme) without breaking this struct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerFilter {
    /// Trust levels to include (AND across levels — e.g., `[Trusted,
    /// Verified]` returns peers matching Trusted ∪ Verified; the
    /// intersection interpretation is impossible since `TrustLevel`
    /// values are mutually exclusive — RFC-0011-f §Test Vectors TV-10
    /// demonstrates the union / "include if in this set" semantics).
    pub trust_levels: Vec<TrustLevel>,
}

impl PeerFilter {
    /// Test whether a peer passes this filter.
    #[must_use]
    pub fn matches(&self, peer: &PeerSummary) -> bool {
        if self.trust_levels.is_empty() {
            return true;
        }
        self.trust_levels.iter().any(|t| t == &peer.trust_level)
    }
}

// =============================================================================
// Peer-table persistence — atomic TOML, 0700 perms
// =============================================================================

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

// =============================================================================
// Substrate functions
// =============================================================================

/// Validate a peer DID against the canonical RFC-0010 wire form via
/// `octo_ident::CanonicalCodec::parse(s, allow_legacy_bare=false)`.
///
/// Returns `MeshError::InvalidDidShape` on any failure — the CLI maps
/// this to `OctoCliError::IdentityNotFound` (exit 4) per RFC-0011-f
/// §Error Handling.
fn validate_peer_did(peer_did: &str) -> Result<(), MeshError> {
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

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_uri_accepts_allowlisted_schemes() {
        for s in [
            "tcp://1.2.3.4:9000",
            "quic://host.example:4433",
            "bluetooth://AA:BB:CC:DD:EE:FF",
        ] {
            let uri = EndpointUri::parse(s).expect("allowlisted scheme must parse");
            assert_eq!(uri.as_str(), s);
        }
    }

    #[test]
    fn endpoint_uri_rejects_disallowed_schemes() {
        for s in ["file:///etc/passwd", "http://x", "ws://x", "ftp://x"] {
            let err = EndpointUri::parse(s).expect_err("disallowed scheme must fail");
            assert!(
                matches!(err, MeshError::InvalidEndpointScheme { .. }),
                "{err:?}"
            );
        }
    }

    #[test]
    fn endpoint_uri_rejects_empty_payload() {
        let err = EndpointUri::parse("tcp://").expect_err("empty payload must fail");
        assert!(matches!(err, MeshError::InvalidEndpointScheme { .. }));
    }

    #[test]
    fn peer_filter_empty_includes_all() {
        let f = PeerFilter::default();
        let p = PeerSummary {
            peer_did: "did:octo:zabc".to_string(),
            endpoint: EndpointUri("tcp://1.2.3.4:1".to_string()),
            trust_level: TrustLevel::untrusted(),
            last_seen_unix: 0,
            capabilities: vec![],
        };
        assert!(f.matches(&p));
    }

    #[test]
    fn peer_filter_trust_levels_is_inclusive_set() {
        let f = PeerFilter {
            trust_levels: vec![
                TrustLevel::new(trust_level_uuids::TRUSTED),
                TrustLevel::new(trust_level_uuids::VERIFIED),
            ],
        };
        let mut p = PeerSummary {
            peer_did: "did:octo:zabc".to_string(),
            endpoint: EndpointUri("tcp://1.2.3.4:1".to_string()),
            trust_level: TrustLevel::new(trust_level_uuids::TRUSTED),
            last_seen_unix: 0,
            capabilities: vec![],
        };
        assert!(f.matches(&p));
        p.trust_level = TrustLevel::new(trust_level_uuids::VERIFIED);
        assert!(f.matches(&p));
        p.trust_level = TrustLevel::new(trust_level_uuids::UNTRUSTED);
        assert!(!f.matches(&p));
    }

    #[test]
    fn validate_peer_did_accepts_canonical_form() {
        // RFC-0010 canonical form is `did:octo:z<base58btc of 32 bytes>`.
        // Use the substrate `CanonicalCodec::mint` + `raw_to_wire` to
        // produce a guaranteed-valid canonical wire form (avoid hand-
        // crafting base58btc — the decoder is strict on the alphabet).
        let raw = octo_ident::CanonicalCodec::mint(&[1u8; 32]);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        validate_peer_did(wire.as_str()).expect("canonical z-form must parse");
    }

    #[test]
    fn validate_peer_did_rejects_legacy_form() {
        // Legacy `did:octo:b<base32 of 52 bytes>` (62 chars after prefix).
        let payload = "b".to_string() + &"a".repeat(62);
        let did = format!("did:octo:{payload}");
        let err = validate_peer_did(&did).expect_err("legacy form must fail");
        assert!(matches!(err, MeshError::InvalidDidShape(_)));
    }
}
