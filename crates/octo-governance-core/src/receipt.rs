//! Canonical governance receipt structs (RFC-0011-g §7.4 Substrate
//! `[ADD]` + RFC-0011-g v1.2 §Test Vectors TV-21 stale-override
//! parity).
//!
//! Two receipt types land here (Layer A frozen per the RFC-0013
//! §Module Layout + RFC-0011-g v1.4 layer-model amendment):
//!
//! - [`AttestationReceipt`] — substrate-owned receipt returned by
//!   `octo_governance::attest`. Fields:
//!   `attestation_id` (BLAKE3-256 envelope PK), `subject_did`
//!   (RFC-0010 canonical form), `kind_ref` (TypedDiscriminator;
//!   see RFC-0011-g §Attestation Kind Resolution +
//!   cipherocto-design-principles §Extension over enumeration),
//!   `signer_did` (active identity DID), `evidence_hash`
//!   (BLAKE3-256 over canonical evidence bytes; raw evidence
//!   bytes NEVER reach this struct per RFC-0011-g §7.8 Redaction),
//!   `expires_at_unix` (optional TTL), `appended_at_unix`
//!   (substrate append timestamp), `overrode_staleness_at_unix`
//!   (optional override timestamp recorded when `--allow-stale`
//!   was invoked per RFC-0011-g v1.2 TV-19/TV-21 parity).
//!
//! - [`VoteReceipt`] — substrate-owned receipt returned by
//!   `octo_governance::vote`. Fields: `vote_id` (BLAKE3-256
//!   per-`(proposal_id, voter_did)` PK), `proposal_id` (BLAKE3-256
//!   of canonical proposal envelope), `voter_did`, `choice`
//!   (RFC-0011-g §Subcommand Taxonomy `Yes | No | Abstain`),
//!   `weight_applied` (basis-points weight derived from
//!   RFC-0855p-c role stake at proposal snapshot time; substrate
//!   authoritative per RFC-0011-g §Adversarial Review "Vote weight
//!   derivation mismatches" row), `voter_cap_id` (the
//!   `CapabilityToken.id` used to verify per RFC-0957 §Capability
//!   Verification caveats), `recorded_at_unix` (substrate record
//!   timestamp), `overrode_staleness_at_unix` (mirrors attest
//!   parity).
//!
//! ## Layer discipline
//!
//! Per RFC-0013 §Security Considerations, this module is **Layer A
//! frozen + RFC-driven additive**. IO functions (`attest`, `vote`)
//! live in DOMAIN crates (`octo-network::mon::governance`). The
//! CLI binding (`octo-cli` commands `governance.rs`) mirrors these
//! types in `AttestOutput { receipt, attestation_id, content_hash,
//! appended_at_unix }` + `VoteOutput { receipt, vote_id,
//! weight_applied, current_quorum_weight, quorum_threshold,
//! recorded_at_unix }` per RFC-0011-g §7.3 Output Envelope.
//!
//! ## Hex-encoding boundary
//!
//! Receipt fields carry **raw `[u8; 32]` BLAKE3-256 bytes**, not
//! hex strings. The CLI envelope hex-encodes at the dispatch
//! boundary so downstream tooling sees stable wire form per
//! RFC-0011 §Output Envelope "Substrate BLAKE3-256 hex encoding
//! happens at the dispatch boundary" discipline.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Substrate-owned attestation receipt (RFC-0011-g §7.4). Returned
/// from `octo_governance::attest`; wrapped by CLI `AttestOutput`
/// for envelope emission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AttestationReceipt {
    /// BLAKE3-256 PK of the canonical envelope (raw bytes; CLI
    /// hex-encodes at envelope time).
    pub attestation_id: [u8; 32],
    /// Subject DID the attestation attests to (RFC-0010 canonical
    /// wire form).
    pub subject_did: String,
    /// TypedDiscriminator for the attestation kind (e.g.
    /// `route-quality:uptime-30d`). Reserved in the substrate
    /// registry; unknown values fail-closed with
    /// `GovernanceError::UnknownAttestationKind` per RFC-0011-g
    /// §Attestation Kind Resolution.
    pub kind_ref: String,
    /// Signer DID (active identity at attest time).
    pub signer_did: String,
    /// BLAKE3-256 over canonical evidence bytes. Raw evidence
    /// bytes NEVER reach this struct (RFC-0011-g §7.8 Redaction).
    pub evidence_hash: [u8; 32],
    /// Optional expiry timestamp (unix seconds). `None` = no
    /// expiry (substrate pins the attestation as evergreen).
    pub expires_at_unix: Option<u64>,
    /// Substrate append timestamp (unix seconds).
    pub appended_at_unix: u64,
    /// Optional override timestamp recorded when `--allow-stale`
    /// was invoked (RFC-0011-g v1.2 TV-21 stale-override parity).
    /// `None` = no override (default fresh path).
    pub overrode_staleness_at_unix: Option<u64>,
}

/// Substrate-owned vote receipt (RFC-0011-g §7.4). Returned from
/// `octo_governance::vote`; wrapped by CLI `VoteOutput` for
/// envelope emission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VoteReceipt {
    /// BLAKE3-256 PK per-`(proposal_id, voter_did)` (raw bytes;
    /// CLI hex-encodes at envelope time).
    pub vote_id: [u8; 32],
    /// BLAKE3-256 of canonical proposal envelope.
    pub proposal_id: [u8; 32],
    /// Voter DID (active identity at vote time).
    pub voter_did: String,
    /// Vote choice (RFC-0011-g §Subcommand Taxonomy
    /// `Yes | No | Abstain`).
    pub choice: String,
    /// Substrate-authoritative weight in basis points
    /// (0..=10000). CLI displays from this field; never
    /// computes weight itself per RFC-0011-g §Adversarial Review
    /// "Vote weight derivation mismatches" row.
    pub weight_applied: u32,
    /// The `CapabilityToken.id` used for RFC-0957 §Capability
    /// Verification (`Audience(proposal_id)` +
    /// `Before(proposal_open_deadline)` + `Provider(active_role)`).
    pub voter_cap_id: String,
    /// Substrate record timestamp (unix seconds).
    pub recorded_at_unix: u64,
    /// Optional override timestamp recorded when `--allow-stale`
    /// was invoked (mirrors attest parity).
    pub overrode_staleness_at_unix: Option<u64>,
}
