//! Peer-table substrate shapes: `PeerRecord` (on-disk), `PeerSummary`
//! (operator-facing), and `PeerFilter` (query).
//!
//! Split from `peer.rs` per Wave 1.5 hygiene callout — the
//! 545-line `peer.rs` module crossed the per-module size threshold
//! (the substrate-truth principle is "split multi-concern modules
//! early" per `[[cipherocto-design-principles]]` §No god-objects).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::endpoint::EndpointUri;
use crate::trust_level::TrustLevel;

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
