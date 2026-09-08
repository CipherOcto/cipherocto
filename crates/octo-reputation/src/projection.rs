//! Read-only projection surface for `octo reputation show` — RFC-0011-b §7.4.
//!
//! This module is the `[ADD]` Layer-B substrate entry point that the
//! CLI's `reputation show` subcommand consumes. It defines:
//!
//! - `Role` — typed newtype wrapper around the role slug (RFC-0011-b §7.4).
//! - `ReputationRecord` — projection-shaped struct (RFC-0968 §10 +
//!   RFC-0011-b §7.4) returned by `project()`.
//! - `ReputationComponents` — 4-Dfp breakdown (identity / stake /
//!   performance / social) per RFC-0011-b §7.5.
//! - `AnchorRef` — canonical anchor reference (RFC-0955-r1 §Wire Contract).
//! - `AttestationSummary` — attestation window row (RFC-0968 §11).
//!
//! Per RFC-0011-b §7.4 this module is **pure Layer B** — no CLI / no
//! operator-mode awareness. The CLI consumes the re-exports through
//! `pub use octo_reputation::{...}` (no parallel CLI-side type per
//! the §Rationale reuse rule).
//!
//! ## Determinism contract (RFC-0104)
//!
//! All score / delta fields are `octo_determin::Dfp`, never raw `f64`.
//! Canonical wire form: 24-byte `DfpEncoding` per RFC-0104 §3.
//!
//! ## Phase status
//!
//! Phase 1 (this module): types + projection entry points that return
//! deterministic placeholder values for v1.0. The substrate aggregate
//! table (RFC-0968 §10) and attestation window (RFC-0968 §11) are
//! filled by the storage layer; `project()` and `attestations()` will
//! route through `ReputationStore::load_aggregate` /
//! `ReputationStore::window_attestations` once those land per the
//! storage split mission. Until then, the entry points return the
//! canonical zero-record + empty-window pair so the CLI surface can be
//! verified end-to-end against the contract (TV-REP-1..4).

#![allow(clippy::result_large_err)]

use serde::{Deserialize, Serialize};

use octo_determin::Dfp;

use crate::digest::ReputationDigest;
use crate::types::{EventId, RecorderDid, ReputationLayer, SignalKind};

/// Hard cap on `--limit` enforced by the CLI per RFC-0011-b §7.6.
///
/// The CLI rejects `--limit > 1000` with `exit 2` (clap
/// `value_parser` range). The substrate accepts up to this value in
/// `attestations()` without further validation — callers that want a
/// tighter cap should apply it themselves.
pub const ATTESTATION_LIMIT_CAP: usize = 1000;

/// Typed newtype for the role slug — RFC-0011-b §7.4.
///
/// `Role::parse(s)` rejects empty / whitespace-only input. The catalog
/// enumeration (which role slugs are valid) lands via RFC-0011-d role
/// provisioning; v1.0 accepts any non-empty ASCII string without
/// whitespace so the projection entry point is nameable before the
/// catalog ships. Case is preserved verbatim (no lowercase folding).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Role(String);

impl Role {
    /// Parse a role slug. Returns `Err` when the input is empty or
    /// contains ASCII whitespace.
    pub fn parse(s: &str) -> Result<Self, RoleParseError> {
        if s.is_empty() {
            return Err(RoleParseError::Empty);
        }
        if s.chars().any(char::is_whitespace) {
            return Err(RoleParseError::Whitespace);
        }
        Ok(Self(s.to_string()))
    }

    /// Borrow the role slug as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Role {
    type Err = RoleParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<Role> for String {
    fn from(r: Role) -> Self {
        r.0
    }
}

impl TryFrom<String> for Role {
    type Error = RoleParseError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

/// Failure modes for `Role::parse`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoleParseError {
    /// Input was empty.
    #[error("role slug is empty")]
    Empty,
    /// Input contained ASCII whitespace.
    #[error("role slug contains whitespace")]
    Whitespace,
}

/// Four-Dfp breakdown per RFC-0011-b §7.5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReputationComponents {
    /// Identity component: cryptographic identity anchoring + lifecycle
    /// state weight (RFC-0955-r1).
    pub identity: Dfp,
    /// Stake component: dual-stake weight (OCTO + role-token per
    /// RFC-0968 §4 dual-stake model).
    pub stake: Dfp,
    /// Performance component: outcome signal EWMA + latency penalty
    /// (RFC-0968 §6 EWMA Algorithm).
    pub performance: Dfp,
    /// Social component: attestation count + attestor trust weight
    /// (RFC-0955-r1 §Cross-Layer Aggregation).
    pub social: Dfp,
}

impl Default for ReputationComponents {
    fn default() -> Self {
        Self {
            identity: Dfp::from_f64(0.0),
            stake: Dfp::from_f64(0.0),
            performance: Dfp::from_f64(0.0),
            social: Dfp::from_f64(0.0),
        }
    }
}

/// Reference to the last anchor on chain — RFC-0955-r1 §Wire Contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorRef {
    /// BLAKE3-256 digest of the anchor envelope (RFC-0955-r1 §Wire
    /// Contract — 32-byte canonical anchor commitment).
    pub anchor_digest: ReputationDigest,
    /// Block height on the anchor chain at which the envelope was
    /// submitted (RFC-0955-r1 §Chain Anchoring).
    pub chain_block_height: u64,
    /// Unix seconds at which the anchor was submitted.
    pub submitted_at_unix: i64,
}

/// One row of the attestation window — RFC-0968 §11 audit trail
/// summary surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttestationSummary {
    /// Event ID (BLAKE3-256 truncated to 8 bytes per RFC-0968 §11;
    /// the `event_id` tiebreaker in sort order per RFC-0011-b §7.6).
    pub event_id: EventId,
    /// Subject DID (canonical `did:octo:` per RFC-0968 §2).
    pub subject_did: RecorderDid,
    /// Attestor DID (canonical `did:octo:` per RFC-0968 §3 Recorder
    /// Authorization; public on the wire).
    pub attestor_did: RecorderDid,
    /// Recorder DID (the recorder that witnessed the event;
    /// distinct from the attestor per RFC-0968 §3).
    pub recorder_did: RecorderDid,
    /// Signal kind discriminant (RFC-0968 §5).
    pub signal_kind: SignalKind,
    /// Layer discriminant (RFC-0968 §5).
    pub layer: ReputationLayer,
    /// Normalized score delta (Dfp; RFC-0104 wire form).
    pub score_delta: Dfp,
    /// Unix seconds at which the attestation was recorded.
    pub recorded_at_unix: i64,
}

/// Per-(did, role) projection-shaped aggregate — RFC-0011-b §7.4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReputationRecord {
    /// Subject DID (canonical `did:octo:b<52>` per RFC-0968 §2).
    pub did: RecorderDid,
    /// Role for which this aggregate is computed.
    pub role: Role,
    /// Composite score (substrate-owned per RFC-0968 §9; surfaced
    /// unchanged via the canonical RFC-0104 Dfp wire form).
    pub score: Dfp,
    /// 4-Dfp breakdown of the composite score (RFC-0011-b §7.5).
    pub components: ReputationComponents,
    /// Reference to the last anchor on chain. `None` if the subject
    /// has never been anchored (initial state per RFC-0968 §10).
    pub anchor_ref: Option<AnchorRef>,
    /// Unix seconds of the last aggregate update.
    pub last_updated_unix: i64,
}

impl ReputationRecord {
    /// Construct the canonical zero-record for a (did, role) tuple.
    ///
    /// Used by `project()` in Phase 1 (v1.0) where the substrate
    /// aggregate table is not yet wired to a backing store. The
    /// zero-record is the deterministic baseline every replica must
    /// produce before any attestations land; see RFC-0968 §10.
    pub fn zero_record(did: RecorderDid, role: Role, last_updated_unix: i64) -> Self {
        Self {
            did,
            role,
            score: Dfp::from_f64(0.0),
            components: ReputationComponents::default(),
            anchor_ref: None,
            last_updated_unix,
        }
    }
}

/// Project the per-(did, role) reputation aggregate.
///
/// Phase 1 (this module): returns the canonical zero-record for the
/// given `did` + `role` tuple. Once the substrate storage layer
/// (RFC-0968 §10) is wired, this entry point routes through
/// `ReputationStore::load_aggregate(did, role)` and returns the
/// persisted aggregate. The CLI contract is stable across both
/// implementations — the zero-record is the deterministic baseline.
///
/// ## Determinism contract
///
/// Pure function of the input `(did, role)` plus the substrate clock
/// (`last_updated_unix`). No I/O. No environment reads. Replicas that
/// hold the same aggregate state produce identical `ReputationRecord`
/// bytes (RFC-0008 Class B).
pub fn project(did: &RecorderDid, role: &Role, last_updated_unix: i64) -> ReputationRecord {
    ReputationRecord::zero_record(*did, role.clone(), last_updated_unix)
}

/// Window SELECT over the attestation log — RFC-0011-b §7.6.
///
/// Returns the most-recent-first window of attestation summaries for
/// the given `(did, role)` tuple, filtered to events with
/// `recorded_at_unix >= since_unix` and capped at `limit` rows.
///
/// Phase 1 (this module): returns the empty window. Once the substrate
/// attestation log (RFC-0968 §11) is wired, this entry point routes
/// through `ReputationStore::window_attestations(did, role, since,
/// limit)` and returns the persisted rows.
///
/// ## Determinism contract
///
/// Sort order is `ORDER BY recorded_at_unix DESC, event_id ASC`. The
/// `event_id` tiebreaker is a BLAKE3-256 truncated to 8 bytes per
/// RFC-0968 §11; deterministic ordering across replicas requires it
/// (RFC-0008 Class B).
///
/// ## Limit cap
///
/// `limit` is clamped to `ATTESTATION_LIMIT_CAP` (= 1000) to match
/// the CLI's `--limit` hard cap per RFC-0011-b §7.6. Callers that
/// pass `0` receive the empty window (a zero-limit is intentionally
/// distinct from "no limit" — the caller asked for nothing).
pub fn attestations(
    _did: &RecorderDid,
    _role: &Role,
    since_unix: i64,
    limit: usize,
) -> Vec<AttestationSummary> {
    // Phase 1: empty window. The store wiring lands with the
    // storage split mission; until then the attestation log is
    // not addressable from this entry point.
    let _ = since_unix;
    let _ = limit.clamp(0, ATTESTATION_LIMIT_CAP);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did_bytes(seed: u8) -> [u8; 52] {
        let mut a = [0u8; 52];
        a[0] = seed;
        a
    }

    #[test]
    fn role_parse_rejects_empty() {
        assert_eq!(Role::parse(""), Err(RoleParseError::Empty));
    }

    #[test]
    fn role_parse_rejects_whitespace() {
        assert_eq!(Role::parse("octo a"), Err(RoleParseError::Whitespace));
        assert_eq!(Role::parse("\t"), Err(RoleParseError::Whitespace));
        assert_eq!(Role::parse(" "), Err(RoleParseError::Whitespace));
    }

    #[test]
    fn role_parse_accepts_canonical_slugs() {
        for slug in [
            "builder", "provider", "recorder", "auditor", "octo-a", "OCTO-W",
        ] {
            let r = Role::parse(slug).expect("non-empty + no whitespace");
            assert_eq!(r.as_str(), slug);
        }
    }

    #[test]
    fn role_serde_roundtrip() {
        let r = Role::parse("builder").unwrap();
        let s: String = r.clone().into();
        assert_eq!(s, "builder");
        let back: Role = s.try_into().unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn zero_record_is_deterministic_for_same_inputs() {
        let did = RecorderDid::from_array(did_bytes(1));
        let role = Role::parse("builder").unwrap();
        let a = project(&did, &role, 1_000);
        let b = project(&did, &role, 1_000);
        assert_eq!(a, b);
    }

    #[test]
    fn zero_record_components_are_zero_dfp() {
        let did = RecorderDid::from_array(did_bytes(2));
        let role = Role::parse("recorder").unwrap();
        let r = project(&did, &role, 0);
        assert_eq!(r.components.identity, Dfp::from_f64(0.0));
        assert_eq!(r.components.stake, Dfp::from_f64(0.0));
        assert_eq!(r.components.performance, Dfp::from_f64(0.0));
        assert_eq!(r.components.social, Dfp::from_f64(0.0));
        assert_eq!(r.score, Dfp::from_f64(0.0));
        assert!(r.anchor_ref.is_none());
    }

    #[test]
    fn attestations_returns_empty_window_in_phase_1() {
        let did = RecorderDid::from_array(did_bytes(3));
        let role = Role::parse("builder").unwrap();
        let w = attestations(&did, &role, 0, 10);
        assert!(w.is_empty());
    }

    #[test]
    fn attestation_limit_cap_is_1000() {
        assert_eq!(ATTESTATION_LIMIT_CAP, 1000);
    }
}
