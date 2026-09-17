//! `octo_governance::vote` — vote append path (RFC-0011-g §7.4
//! Substrate `[ADD]`).
//!
//! ## Substrate surface
//!
//! Public items re-exported by `octo-governance`:
//!
//! - [`VoteLog`] — append-only ledger keyed by
//!   `proposal_id → voter_did → VoteReceipt` per RFC-0011-g §Substrate
//!   `[ADD]`. In-memory for Phase 2. Persistence substrate lands in
//!   a follow-on mission.
//! - [`CapabilityRegistry`] — `HashMap<voter_cap_id, Arc<dyn
//!   CapabilitySigner>>` lookup. The CLI populates this at startup
//!   from the wallet's capability set. Unknown `voter_cap_id`
//!   fails-closed with `GovernanceError::UnknownCapability`.
//! - [`vote`] — `pub fn vote(...) -> Result<VoteReceipt,
//!   GovernanceError>` per RFC-0011-g §7.4 substrate signature.
//!
//! ## Layer discipline
//!
//! This module is **Layer B (RFC-driven additive only)**. It
//! depends on Layer A primitives (`octo-governance-core` for
//! canonical types; `blake3` for BLAKE3-256 PK computation;
//! `thiserror` for the substrate-specific errors). No storage
//! IO, no clock, no randomness beyond the caller's signer.
//!
//! ## Prereq gate (RFC-0011-g §Compatibility Mixed-Version)
//!
//! The `vote` function returns
//! `GovernanceError::PrereqNotAccepted { rfc_ref }` for
//! sub-group-scoped proposals until `RFC-0855p-d` reaches
//! Accepted. The CLI surfaces this as
//! `OctoCliError::PrereqNotAccepted` (exit 38) per RFC-0011-g
//! §Error Handling.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use blake3::Hasher;
use octo_governance_core::tally_quorum;
use octo_governance_core::{CapabilityToken, GovernanceError, VoteReceipt};
use serde::{Deserialize, Serialize};

use crate::attest::CapabilitySigner;
use crate::session::GovernanceSession;

/// Vote choice (RFC-0011-g §Vote Choice + §Command Taxonomy).
///
/// On-the-wire canonical encoding is the lowercase string
/// `yes` | `no` | `abstain` per RFC §Command Taxonomy L391
/// (substrate-recognized choice string, RFC-0855 amendment
/// cadence). Two back-compat aliases `approve` | `reject` are
/// accepted at `parse` for legacy callers (parse-only; the
/// canonical `as_str` form is `yes` | `no` | `abstain`).
///
/// Kept as a typed enum so callers cannot pass arbitrary
/// strings to the substrate — the substrate fails-closed on
/// unknown choices via `GovernanceError::InvalidArgument`,
/// which `map_governance_error` routes to
/// `OctoCliError::VoteRejected` (exit 36) per RFC §Error
/// Handling slot table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteChoice {
    /// Vote in favor of the proposal.
    Yes,
    /// Vote against the proposal.
    No,
    /// Abstain from the proposal (recorded but does not count
    /// toward quorum per RFC-0855 amendment cadence).
    Abstain,
}

impl VoteChoice {
    /// Canonical lowercase wire form (`yes` | `no` | `abstain`).
    /// Per RFC §Command Taxonomy L391 — substrate-recognized
    /// choice vocabulary.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Abstain => "abstain",
        }
    }

    /// Parse from `&str` (case-insensitive). Recognizes the
    /// canonical vocabulary `yes` | `no` | `abstain` plus
    /// the back-compat aliases `approve` | `reject` (which
    /// map to `Yes` | `No` respectively). Unknown values
    /// fail-closed with `GovernanceError::InvalidArgument`.
    pub fn parse(s: &str) -> Result<Self, GovernanceError> {
        // case-insensitive match via lowercase comparison
        let lower = s.trim().to_ascii_lowercase();
        match lower.as_str() {
            "yes" | "approve" => Ok(Self::Yes),
            "no" | "reject" => Ok(Self::No),
            "abstain" => Ok(Self::Abstain),
            other => Err(GovernanceError::InvalidArgument {
                reason: format!("unknown vote choice: {other}"),
            }),
        }
    }
}

/// Capability registry mapping `voter_cap_id → Arc<dyn CapabilitySigner>`.
/// Populated by the CLI at startup from the wallet's capability set.
/// Unknown `voter_cap_id` fails-closed with
/// `GovernanceError::UnknownCapability { voter_cap_id }`.
#[derive(Default)]
pub struct CapabilityRegistry {
    caps: Mutex<HashMap<String, Arc<dyn CapabilitySigner>>>,
}

impl CapabilityRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a capability under the given `voter_cap_id`. Overwrites
    /// any prior registration for the same id (last-writer-wins).
    pub fn register(&self, voter_cap_id: String, signer: Arc<dyn CapabilitySigner>) {
        self.caps
            .lock()
            .expect("capability registry mutex poisoned")
            .insert(voter_cap_id, signer);
    }

    /// Resolve a `voter_cap_id` to its `CapabilitySigner`. Returns
    /// `GovernanceError::UnknownCapability` on miss.
    ///
    /// Fail-closed on mutex poisoning: a prior panicking holder
    /// surfaces as `GovernanceError::Internal { reason }` rather
    /// than aborting the substrate ledger reader (RFC-0011-g
    /// §Error Handling fail-closed invariant — substrate must
    /// never panic on the caller).
    pub fn resolve(
        &self,
        voter_cap_id: &str,
    ) -> Result<Arc<dyn CapabilitySigner>, GovernanceError> {
        let caps = self.caps.lock().map_err(|e| GovernanceError::Internal {
            reason: format!("capability registry mutex poisoned: {e}"),
        })?;
        caps.get(voter_cap_id)
            .cloned()
            .ok_or_else(|| GovernanceError::UnknownCapability {
                voter_cap_id: voter_cap_id.to_string(),
            })
    }

    /// Number of registered capabilities (diagnostic).
    #[must_use]
    pub fn len(&self) -> usize {
        self.caps
            .lock()
            .expect("capability registry mutex poisoned")
            .len()
    }

    /// `true` if no capabilities are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Append-only vote ledger. Per-proposal sub-ledger of
/// `voter_did → VoteReceipt`. `vote()` enforces voter uniqueness
/// within a proposal: a second vote by the same `voter_did` for
/// the same `proposal_id` returns
/// `GovernanceError::DuplicateVote { proposal_id, voter_did }`.
#[derive(Debug, Default)]
pub struct VoteLog {
    /// `proposal_id → voter_did → receipt`.
    entries: Mutex<BTreeMap<[u8; 32], BTreeMap<String, VoteReceipt>>>,
}

impl VoteLog {
    /// Construct an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a vote receipt by `(proposal_id, voter_did)`.
    #[must_use]
    pub fn get(&self, proposal_id: &[u8; 32], voter_did: &str) -> Option<VoteReceipt> {
        self.entries
            .lock()
            .expect("vote log mutex poisoned")
            .get(proposal_id)
            .and_then(|by_voter| by_voter.get(voter_did))
            .cloned()
    }

    /// Number of distinct voters for a given proposal (diagnostic).
    #[must_use]
    pub fn voter_count(&self, proposal_id: &[u8; 32]) -> usize {
        self.entries
            .lock()
            .expect("vote log mutex poisoned")
            .get(proposal_id)
            .map_or(0, BTreeMap::len)
    }

    /// Number of distinct proposals in the ledger (diagnostic).
    #[must_use]
    pub fn proposal_count(&self) -> usize {
        self.entries.lock().expect("vote log mutex poisoned").len()
    }

    /// Snapshot the current tally state for a proposal as
    /// `(voter_did → (weight_bps, approve_bool))`. Empty when the
    /// proposal has no recorded votes.
    #[must_use]
    pub fn tally_snapshot(&self, proposal_id: &[u8; 32]) -> BTreeMap<String, (u32, bool)> {
        self.entries
            .lock()
            .expect("vote log mutex poisoned")
            .get(proposal_id)
            .map(|by_voter| {
                by_voter
                    .iter()
                    .map(|(voter_did, receipt)| {
                        (
                            voter_did.clone(),
                            (receipt.weight_applied, receipt.choice == "yes"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Append a vote receipt. Returns
    /// `Err(GovernanceError::DuplicateVote)` if the `(proposal_id,
    /// voter_did)` pair already has a receipt; the ledger state is
    /// unchanged on error.
    pub fn append(&self, receipt: VoteReceipt) -> Result<(), GovernanceError> {
        let proposal_id = receipt.proposal_id;
        let voter_did = receipt.voter_did.clone();
        let mut entries = self.entries.lock().expect("vote log mutex poisoned");
        let by_voter = entries.entry(proposal_id).or_default();
        if by_voter.contains_key(&voter_did) {
            return Err(GovernanceError::DuplicateVote {
                proposal_id,
                voter_did,
            });
        }
        by_voter.insert(voter_did, receipt);
        Ok(())
    }
}

/// Quorum projection consumed by `VoteOutput` at the CLI boundary:
/// `(current_approval_bps, current_rejection_bps, quorum_required_bps)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuorumProjection {
    /// Summed approval bps across all recorded votes.
    pub approval_bps: u32,
    /// Summed rejection bps across all recorded votes.
    pub rejection_bps: u32,
    /// Quorum threshold in bps (governance-policy-supplied; 10_000
    /// is the canonical default — substrate does not enforce, the
    /// CLI does per RFC-0011-g §Quorum Threshold).
    pub quorum_required_bps: u32,
}

/// Cast a vote per RFC-0011-g §7.4 substrate signature. The
/// canonical envelope bytes are `voter_did || 0x00 || proposal_id
/// || 0x00 || choice_str || 0x00 || weight_be || 0x00 ||
/// voter_cap_id || 0x00 || snapshot_id || 0x00 ||
/// allow_stale_bool`. Every field is preceded by an explicit
/// `0x00` delimiter — the parsing layer does NOT rely on
/// implicit fixed-length separators (32-byte PK, 4-byte
/// `u32_be`). The PK is `BLAKE3-256` of those bytes; the
/// `VoteReceipt.vote_id` is set to the PK for downstream tooling
/// convenience.
///
/// Returns `GovernanceError::UnknownCapability` for unregistered
/// `voter_cap_id`; `GovernanceError::PrereqNotAccepted` for
/// sub-group-scoped proposals until `RFC-0855p-d` reaches Accepted
/// (gate enforced via [`prereq_vote_subgroup_check`] before the
/// envelope is built); `GovernanceError::DuplicateVote` for repeat
/// voters; `GovernanceError::InvalidArgument` for invalid
/// `weight_bps` (> 10_000) or unrecognized `choice`.
///
/// `quorum_required_bps` is recorded on the receipt as the
/// snapshot policy at the time of voting; the substrate does not
/// enforce a threshold — the CLI does per RFC-0011-g §Quorum
/// Threshold.
///
/// ## Deprecation
///
/// This 10-parameter surface predates the RFC-0011-g §7.4
/// stateless caller-owned-session design (the substrate
/// accepts state via explicit arguments). New callers should
/// use [`vote_v2`] which accepts `&GovernanceSession` +
/// `&CapabilityToken` and reads state from the session.
#[deprecated(
    since = "0.0.0",
    note = "use vote_v2 with a GovernanceSession and CapabilityToken (RFC-0011-g §7.4 stateless substrate signature)"
)]
#[allow(clippy::too_many_arguments)]
pub fn vote(
    log: &VoteLog,
    registry: &CapabilityRegistry,
    proposal_id: [u8; 32],
    voter_did: &str,
    choice: VoteChoice,
    weight_bps: u32,
    voter_cap_id: &str,
    snapshot_id: Option<&[u8; 32]>,
    allow_stale: bool,
    recorded_at_unix: u64,
) -> Result<(VoteReceipt, QuorumProjection), GovernanceError> {
    // Sub-group prereq gate (RFC-0855p-d). Proposal IDs prefixed
    // `did:octo:subgroup:` (carried via the proposal_id bytes)
    // are gated until `RFC-0855p-d` reaches Accepted. Substrate
    // does not parse the proposal_id bytes — the CLI passes the
    // full 32-byte proposal_id derived from the proposal content.
    // The gate is keyed on `voter_did` for Phase 2 simplicity
    // (the CLI surfaces the prereq error on the voter's behalf).
    prereq_vote_subgroup_check(voter_did)?;

    // Capability verification — unknown voter_cap_id fails-closed.
    let _signer = registry.resolve(voter_cap_id)?;

    // weight_bps invariant — caller must clamp at the boundary.
    if weight_bps > 10_000 {
        return Err(GovernanceError::InvalidArgument {
            reason: format!("weight_bps {weight_bps} exceeds 10_000 (100%)"),
        });
    }

    // Canonical envelope bytes (substrate-faithful). Every field
    // is preceded by an explicit `0x00` delimiter; the parsing
    // layer does NOT rely on implicit fixed-length separators
    // (32-byte `proposal_id`, 4-byte `weight_bps`).
    let choice_str = choice.as_str();
    let mut envelope: Vec<u8> = Vec::new();
    envelope.extend_from_slice(voter_did.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&proposal_id);
    envelope.push(0x00);
    envelope.extend_from_slice(choice_str.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&weight_bps.to_be_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(voter_cap_id.as_bytes());
    envelope.push(0x00);
    if let Some(snap) = snapshot_id {
        envelope.extend_from_slice(snap);
    }
    envelope.push(0x00);
    envelope.push(u8::from(allow_stale));

    let vote_id = blake3_256(&envelope);

    // Signer signs the canonical envelope (HSM-bound). Recorded
    // for audit but not currently in the `VoteReceipt` (audit
    // lives in the substrate's governance_envelopes table per
    // RFC-0862 §Data Structures).
    let _signature = _signer
        .sign_envelope(&envelope)
        .map_err(|reason| GovernanceError::Internal { reason })?;

    let receipt = VoteReceipt {
        vote_id,
        proposal_id,
        voter_did: voter_did.to_string(),
        choice: choice_str.to_string(),
        weight_applied: weight_bps,
        voter_cap_id: voter_cap_id.to_string(),
        recorded_at_unix,
        overrode_staleness_at_unix: if allow_stale {
            Some(recorded_at_unix)
        } else {
            None
        },
    };

    // Append first; the duplicate-vote check is the substrate
    // invariant for voter uniqueness within a proposal.
    log.append(receipt.clone())?;

    // Quorum projection (post-append snapshot).
    let tally = log.tally_snapshot(&proposal_id);
    let (approval_bps, rejection_bps) = tally_quorum(&tally)?;
    let projection = QuorumProjection {
        approval_bps,
        rejection_bps,
        quorum_required_bps: 10_000,
    };

    Ok((receipt, projection))
}

/// Compute `BLAKE3-256(bytes)` returning a 32-byte array.
#[must_use]
pub fn blake3_256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut pk = [0u8; 32];
    pk.copy_from_slice(out.as_bytes());
    pk
}

/// Cast a vote per RFC-0011-g §7.4 stateless substrate signature.
///
/// ## Substrate surface (new)
///
/// - `session` — caller-owned `GovernanceSession` carrying the
///   vote ledger + capability registry + clock + active DID.
/// - `proposal_id` — 32-byte content-addressed proposal ID.
/// - `choice` — `VoteChoice` (RFC §Command Taxonomy canonical
///   vocabulary `Yes` / `No` / `Abstain`).
/// - `voter_cap` — `CapabilityToken` carrying `cap_id` +
///   `issuer_did` + `weight_bps`. The substrate derives
///   `voter_cap_id`, `voter_did`, and `weight_bps` from the
///   token; the CLI does not inject them as separate args.
/// - `rationale` — optional plaintext rationale string (CLI
///   responsibility to redact per RFC-0011-g §Redaction;
///   substrate accepts the bytes verbatim).
/// - `snapshot_id` — optional governance-snapshot pin (when
///   `Some`, the substrate records it on the receipt envelope
///   for downstream quorum projection).
/// - `allow_stale` — when `true`, records `overrode_staleness_at_unix`
///   on the receipt.
///
/// The canonical envelope bytes are
/// `voter_did || 0x00 || proposal_id || 0x00 || choice_str ||
/// 0x00 || weight_be || 0x00 || voter_cap_id || 0x00 ||
/// rationale_be || 0x00 || snapshot_id_be || 0x00 ||
/// allow_stale_bool`. Every field is preceded by an explicit
/// `0x00` delimiter; the parsing layer does NOT rely on
/// implicit fixed-length separators (32-byte PK, 4-byte
/// `u32_be`). The PK is `BLAKE3-256` of those bytes; the
/// `VoteReceipt.vote_id` is set to the PK.
///
/// Returns:
/// - `GovernanceError::UnknownCapability` for unregistered
///   `voter_cap.cap_id` (looked up in `session.capability_registry()`).
/// - `GovernanceError::PrereqNotAccepted` for sub-group-scoped
///   proposals until `RFC-0855p-d` reaches Accepted (gate
///   keyed on `voter_cap.issuer_did` prefix).
/// - `GovernanceError::DuplicateVote` for repeat voters
///   (`(proposal_id, voter_did)` already present in
///   `session.vote_log()`).
/// - `GovernanceError::InvalidArgument` for `weight_bps > 10_000`.
#[allow(clippy::too_many_arguments)]
pub fn vote_v2(
    session: &GovernanceSession,
    proposal_id: [u8; 32],
    choice: VoteChoice,
    voter_cap: &CapabilityToken,
    rationale: Option<&str>,
    snapshot_id: Option<&[u8; 32]>,
    allow_stale: bool,
) -> Result<(VoteReceipt, QuorumProjection), GovernanceError> {
    let voter_did = voter_cap.issuer_did.as_str();
    let voter_cap_id = voter_cap.cap_id.as_str();
    let weight_bps = voter_cap.weight_bps;
    let recorded_at_unix = session.now_unix();

    // Sub-group prereq gate (RFC-0855p-d).
    prereq_vote_subgroup_check(voter_did)?;

    // Capability verification — unknown voter_cap.cap_id fails-closed.
    let signer = session.capability_registry().resolve(voter_cap_id)?;

    // weight_bps invariant — caller must clamp at the boundary.
    if weight_bps > 10_000 {
        return Err(GovernanceError::InvalidArgument {
            reason: format!("weight_bps {weight_bps} exceeds 10_000 (100%)"),
        });
    }

    // Canonical envelope bytes (substrate-faithful). Every field
    // is preceded by an explicit `0x00` delimiter; the parsing
    // layer does NOT rely on implicit fixed-length separators
    // (32-byte `proposal_id`, 4-byte `weight_bps`).
    let choice_str = choice.as_str();
    let mut envelope: Vec<u8> = Vec::new();
    envelope.extend_from_slice(voter_did.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&proposal_id);
    envelope.push(0x00);
    envelope.extend_from_slice(choice_str.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&weight_bps.to_be_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(voter_cap_id.as_bytes());
    envelope.push(0x00);
    if let Some(r) = rationale {
        envelope.extend_from_slice(r.as_bytes());
    }
    envelope.push(0x00);
    if let Some(snap) = snapshot_id {
        envelope.extend_from_slice(snap);
    }
    envelope.push(0x00);
    envelope.push(u8::from(allow_stale));

    let vote_id = blake3_256(&envelope);

    // Signer signs the canonical envelope (HSM-bound). Recorded
    // for audit but not currently in the `VoteReceipt` (audit
    // lives in the substrate's governance_envelopes table per
    // RFC-0862 §Data Structures).
    let _signature = signer
        .sign_envelope(&envelope)
        .map_err(|reason| GovernanceError::Internal { reason })?;

    let receipt = VoteReceipt {
        vote_id,
        proposal_id,
        voter_did: voter_did.to_string(),
        choice: choice_str.to_string(),
        weight_applied: weight_bps,
        voter_cap_id: voter_cap_id.to_string(),
        recorded_at_unix,
        overrode_staleness_at_unix: if allow_stale {
            Some(recorded_at_unix)
        } else {
            None
        },
    };

    // Append first; the duplicate-vote check is the substrate
    // invariant for voter uniqueness within a proposal.
    session.vote_log().append(receipt.clone())?;

    // Quorum projection (post-append snapshot).
    let tally = session.vote_log().tally_snapshot(&proposal_id);
    let (approval_bps, rejection_bps) = tally_quorum(&tally)?;
    let projection = QuorumProjection {
        approval_bps,
        rejection_bps,
        quorum_required_bps: 10_000,
    };

    Ok((receipt, projection))
}

/// Sub-group vote prereq gate (RFC-0855p-d). Until `RFC-0855p-d`
/// reaches Accepted, any voter DID prefixed `did:octo:subgroup:`
/// fails-closed with `GovernanceError::PrereqNotAccepted`.
fn prereq_vote_subgroup_check(voter_did: &str) -> Result<(), GovernanceError> {
    if voter_did.starts_with("did:octo:subgroup:") {
        return Err(GovernanceError::PrereqNotAccepted {
            rfc_ref: "RFC-0855p-d".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    //! Substrate-faithful tests for the vote append path.
    //!
    //! Test vector coverage:
    //! - V1: happy-path vote records receipt + computes quorum
    //! - V2: unknown voter_cap_id rejected (UnknownCapability)
    //! - V3: duplicate (proposal_id, voter_did) rejected (DuplicateVote)
    //! - V4: sub-group voter DID gated (prereq RFC-0855p-d)
    //! - V5: weight_bps > 10_000 rejected (InvalidArgument)
    //! - V6: vote_choice parse roundtrip + invalid choice rejected
    //! - V7: ledger voter_count + proposal_count + tally_snapshot
    //! - V8: allow_stale=true records overrode_staleness_at_unix
    //! - V9: vote_v2 stateless session-based happy path
    //! - V10: vote_v2 unknown voter_cap_id rejected
    //! - V11: vote_v2 rationale included in envelope PK
    //! - V12: vote_v2 fixed-clock deterministic receipt timestamp
    //! - V13: vote_v2 quorum_not_reached when tally saturates beyond
    //!   100_000 bps (11+ voters each capped at 10_000)

    use super::*;
    use crate::attest::CapabilitySigner;
    use crate::session::FixedClock;

    /// Test `CapabilitySigner` impl that returns a fixed 64-byte
    /// signature. Distinct from the attest test signer to avoid
    /// cross-test state leakage.
    struct TestSigner {
        signature: [u8; 64],
    }

    impl TestSigner {
        fn new() -> Self {
            Self {
                signature: [0xCD; 64],
            }
        }
    }

    impl CapabilitySigner for TestSigner {
        fn sign_envelope(&self, _envelope_bytes: &[u8]) -> Result<[u8; 64], String> {
            Ok(self.signature)
        }
    }

    fn voter_did() -> &'static str {
        "did:octo:z6MkVoterXYZABCDEF1234567890abcdef1234567890ab"
    }

    fn proposal_id() -> [u8; 32] {
        [0x42; 32]
    }

    #[test]
    fn vote_v1_happy_path_records_receipt() {
        let log = VoteLog::new();
        let registry = CapabilityRegistry::new();
        registry.register("voter-cap-001".to_string(), Arc::new(TestSigner::new()));
        let (receipt, projection) = vote(
            &log,
            &registry,
            proposal_id(),
            voter_did(),
            VoteChoice::Yes,
            5000,
            "voter-cap-001",
            Some(&[0xAA; 32]),
            false,
            1_700_000_000,
        )
        .expect("happy path should succeed");
        assert_eq!(receipt.voter_did, voter_did());
        assert_eq!(receipt.proposal_id, proposal_id());
        assert_eq!(receipt.choice, "yes");
        assert_eq!(receipt.weight_applied, 5000);
        assert_eq!(receipt.voter_cap_id, "voter-cap-001");
        assert_eq!(receipt.recorded_at_unix, 1_700_000_000);
        assert_eq!(receipt.overrode_staleness_at_unix, None);
        assert_eq!(projection.approval_bps, 5000);
        assert_eq!(projection.rejection_bps, 0);
        assert_eq!(projection.quorum_required_bps, 10_000);
        // Log state.
        assert_eq!(log.voter_count(&proposal_id()), 1);
        assert_eq!(log.proposal_count(), 1);
        assert_eq!(
            log.get(&proposal_id(), voter_did()).as_ref(),
            Some(&receipt)
        );
    }

    #[test]
    fn vote_v2_unknown_capability_rejected() {
        let log = VoteLog::new();
        let registry = CapabilityRegistry::new();
        // No cap registered.
        let err = vote(
            &log,
            &registry,
            proposal_id(),
            voter_did(),
            VoteChoice::Yes,
            5000,
            "voter-cap-does-not-exist",
            None,
            false,
            1_700_000_000,
        )
        .expect_err("unknown capability must error");
        match err {
            GovernanceError::UnknownCapability { voter_cap_id } => {
                assert_eq!(voter_cap_id, "voter-cap-does-not-exist");
            }
            other => panic!("expected UnknownCapability, got {other:?}"),
        }
        assert_eq!(log.voter_count(&proposal_id()), 0);
    }

    #[test]
    fn vote_v3_duplicate_voter_rejected() {
        let log = VoteLog::new();
        let registry = CapabilityRegistry::new();
        registry.register("voter-cap-001".to_string(), Arc::new(TestSigner::new()));
        let first = vote(
            &log,
            &registry,
            proposal_id(),
            voter_did(),
            VoteChoice::Yes,
            5000,
            "voter-cap-001",
            None,
            false,
            1_700_000_000,
        )
        .expect("first vote should succeed");
        // Same (proposal_id, voter_did) → duplicate.
        let err = vote(
            &log,
            &registry,
            proposal_id(),
            voter_did(),
            VoteChoice::No, // different choice — still duplicate
            6000,
            "voter-cap-001",
            None,
            false,
            1_700_000_001,
        )
        .expect_err("duplicate voter for same proposal must error");
        match err {
            GovernanceError::DuplicateVote {
                proposal_id,
                voter_did,
            } => {
                assert_eq!(proposal_id, first.0.proposal_id);
                assert_eq!(voter_did, first.0.voter_did);
            }
            other => panic!("expected DuplicateVote, got {other:?}"),
        }
        // Ledger state unchanged (still 1 voter for this proposal).
        assert_eq!(log.voter_count(&proposal_id()), 1);
    }

    #[test]
    fn vote_v4_subgroup_voter_did_gated() {
        let log = VoteLog::new();
        let registry = CapabilityRegistry::new();
        registry.register("voter-cap-001".to_string(), Arc::new(TestSigner::new()));
        let err = vote(
            &log,
            &registry,
            proposal_id(),
            "did:octo:subgroup:abc123",
            VoteChoice::Yes,
            5000,
            "voter-cap-001",
            None,
            false,
            1_700_000_000,
        )
        .expect_err("subgroup voter DID must fail-closed until RFC-0855p-d accepted");
        match err {
            GovernanceError::PrereqNotAccepted { rfc_ref } => {
                assert_eq!(rfc_ref, "RFC-0855p-d");
            }
            other => panic!("expected PrereqNotAccepted, got {other:?}"),
        }
        assert_eq!(log.voter_count(&proposal_id()), 0);
    }

    #[test]
    fn vote_v5_weight_over_10k_rejected() {
        let log = VoteLog::new();
        let registry = CapabilityRegistry::new();
        registry.register("voter-cap-001".to_string(), Arc::new(TestSigner::new()));
        let err = vote(
            &log,
            &registry,
            proposal_id(),
            voter_did(),
            VoteChoice::Yes,
            10_001,
            "voter-cap-001",
            None,
            false,
            1_700_000_000,
        )
        .expect_err("weight_bps > 10_000 must error");
        match err {
            GovernanceError::InvalidArgument { reason } => {
                assert!(
                    reason.contains("10_000"),
                    "reason should describe weight cap: {reason}"
                );
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
        assert_eq!(log.voter_count(&proposal_id()), 0);
    }

    #[test]
    fn vote_v6_choice_parse_roundtrip() {
        // Canonical vocabulary (RFC §Command Taxonomy L391).
        assert_eq!(VoteChoice::parse("yes").unwrap(), VoteChoice::Yes);
        assert_eq!(VoteChoice::parse("no").unwrap(), VoteChoice::No);
        assert_eq!(VoteChoice::parse("abstain").unwrap(), VoteChoice::Abstain);
        // Back-compat aliases (parse-only; canonical as_str is
        // lowercase vocabulary).
        assert_eq!(VoteChoice::parse("approve").unwrap(), VoteChoice::Yes);
        assert_eq!(VoteChoice::parse("reject").unwrap(), VoteChoice::No);
        // Case-insensitive parse.
        assert_eq!(VoteChoice::parse("YES").unwrap(), VoteChoice::Yes);
        assert_eq!(VoteChoice::parse("Approve").unwrap(), VoteChoice::Yes);
        // Canonical as_str returns canonical wire form.
        assert_eq!(VoteChoice::Yes.as_str(), "yes");
        assert_eq!(VoteChoice::No.as_str(), "no");
        assert_eq!(VoteChoice::Abstain.as_str(), "abstain");
        // Unknown choice fail-closed.
        let err = VoteChoice::parse("garbage").expect_err("unknown choice must error");
        match err {
            GovernanceError::InvalidArgument { reason } => {
                assert!(
                    reason.contains("garbage"),
                    "reason should mention the unknown choice: {reason}"
                );
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    #[test]
    fn vote_v7_tally_snapshot_aggregation() {
        let log = VoteLog::new();
        let registry = CapabilityRegistry::new();
        registry.register("cap-a".to_string(), Arc::new(TestSigner::new()));
        registry.register("cap-b".to_string(), Arc::new(TestSigner::new()));
        // Voter A: approve 3000 bps
        vote(
            &log,
            &registry,
            proposal_id(),
            "did:octo:voter-a",
            VoteChoice::Yes,
            3000,
            "cap-a",
            None,
            false,
            1_700_000_000,
        )
        .expect("voter-a vote should succeed");
        // Voter B: reject 2000 bps
        vote(
            &log,
            &registry,
            proposal_id(),
            "did:octo:voter-b",
            VoteChoice::No,
            2000,
            "cap-b",
            None,
            false,
            1_700_000_001,
        )
        .expect("voter-b vote should succeed");
        // Snapshot tally.
        let tally = log.tally_snapshot(&proposal_id());
        assert_eq!(tally.len(), 2);
        assert_eq!(tally.get("did:octo:voter-a"), Some(&(3000, true)));
        assert_eq!(tally.get("did:octo:voter-b"), Some(&(2000, false)));
        assert_eq!(log.voter_count(&proposal_id()), 2);
    }

    #[test]
    fn vote_v8_allow_stale_records_overrode_staleness() {
        let log = VoteLog::new();
        let registry = CapabilityRegistry::new();
        registry.register("voter-cap-001".to_string(), Arc::new(TestSigner::new()));
        let (receipt, _) = vote(
            &log,
            &registry,
            proposal_id(),
            voter_did(),
            VoteChoice::Yes,
            5000,
            "voter-cap-001",
            None,
            true,
            1_700_000_000,
        )
        .expect("allow_stale=true should still succeed");
        assert_eq!(receipt.overrode_staleness_at_unix, Some(1_700_000_000));
    }

    // ---- vote_v2 tests (RFC §7.4 stateless caller-owned session) ----

    fn session_with_cap(did: &str, cap_id: &str, clock_unix: u64) -> GovernanceSession {
        let session = GovernanceSession::new(did, Arc::new(FixedClock::new(clock_unix)));
        session.register_capability(cap_id, Arc::new(TestSigner::new()));
        session
    }

    fn voter_token() -> CapabilityToken {
        CapabilityToken::new("voter-cap-001", voter_did(), 5_000)
    }

    #[test]
    fn vote_v9_session_happy_path_records_receipt() {
        let session = session_with_cap(voter_did(), "voter-cap-001", 1_700_000_000);
        let (receipt, projection) = vote_v2(
            &session,
            proposal_id(),
            VoteChoice::Yes,
            &voter_token(),
            None,
            Some(&[0xAA; 32]),
            false,
        )
        .expect("happy path should succeed");
        assert_eq!(receipt.voter_did, voter_did());
        assert_eq!(receipt.proposal_id, proposal_id());
        assert_eq!(receipt.choice, "yes");
        assert_eq!(receipt.weight_applied, 5_000);
        assert_eq!(receipt.voter_cap_id, "voter-cap-001");
        // Deterministic clock reads through session.
        assert_eq!(receipt.recorded_at_unix, 1_700_000_000);
        assert_eq!(receipt.overrode_staleness_at_unix, None);
        assert_eq!(projection.approval_bps, 5_000);
        assert_eq!(projection.rejection_bps, 0);
        assert_eq!(projection.quorum_required_bps, 10_000);
        // Session ledger holds the receipt.
        assert_eq!(session.vote_log().voter_count(&proposal_id()), 1);
        assert_eq!(session.vote_log().proposal_count(), 1);
        assert_eq!(
            session.vote_log().get(&proposal_id(), voter_did()).as_ref(),
            Some(&receipt)
        );
    }

    #[test]
    fn vote_v10_unknown_capability_rejected_via_session_registry() {
        let session = session_with_cap(voter_did(), "registered-cap", 1_700_000_000);
        let unknown = CapabilityToken::new("not-registered", voter_did(), 5_000);
        let err = vote_v2(
            &session,
            proposal_id(),
            VoteChoice::Yes,
            &unknown,
            None,
            None,
            false,
        )
        .expect_err("unknown voter_cap.cap_id must error");
        match err {
            GovernanceError::UnknownCapability { voter_cap_id } => {
                assert_eq!(voter_cap_id, "not-registered");
            }
            other => panic!("expected UnknownCapability, got {other:?}"),
        }
        assert_eq!(session.vote_log().voter_count(&proposal_id()), 0);
    }

    #[test]
    fn vote_v11_rationale_changes_envelope_pk() {
        // vote_v2 envelope bytes include the rationale; two votes
        // with the same args but different rationale must produce
        // distinct vote_id PKs.
        let session_a = session_with_cap(voter_did(), "voter-cap-001", 1_700_000_000);
        let token_a = voter_token();
        let (r_a, _) = vote_v2(
            &session_a,
            proposal_id(),
            VoteChoice::Yes,
            &token_a,
            Some("approve: budget aligned"),
            None,
            false,
        )
        .expect("rationale-A path should succeed");

        let session_b = session_with_cap(voter_did(), "voter-cap-001", 1_700_000_001);
        let token_b = voter_token();
        let (r_b, _) = vote_v2(
            &session_b,
            proposal_id(),
            VoteChoice::Yes,
            &token_b,
            Some("reject: budget misaligned"),
            None,
            false,
        )
        .expect("rationale-B path should succeed");

        assert_ne!(
            r_a.vote_id, r_b.vote_id,
            "different rationale must produce distinct PKs"
        );
        assert_eq!(r_a.choice, "yes");
        assert_eq!(r_b.choice, "yes");
    }

    #[test]
    fn vote_v12_fixed_clock_deterministic_receipt_timestamp() {
        // Same args + same fixed clock + same session content
        // must produce identical recorded_at_unix (proves the
        // substrate consults session.clock() rather than
        // SystemTime::now()).
        let session = session_with_cap(voter_did(), "voter-cap-001", 1_700_000_777);
        let token = voter_token();
        let (r1, _) = vote_v2(
            &session,
            proposal_id(),
            VoteChoice::Yes,
            &token,
            None,
            None,
            false,
        )
        .expect("first vote should succeed");
        // Build a second session with a different clock but same args.
        let session2 = session_with_cap(voter_did(), "voter-cap-001", 1_700_000_777);
        let token2 = voter_token();
        // Different proposal_id to avoid duplicate-voter error.
        let (r2, _) = vote_v2(
            &session2,
            [0x77; 32],
            VoteChoice::Yes,
            &token2,
            None,
            None,
            false,
        )
        .expect("second vote should succeed");
        assert_eq!(r1.recorded_at_unix, 1_700_000_777);
        assert_eq!(r2.recorded_at_unix, 1_700_000_777);
    }

    #[test]
    fn vote_v13_quorum_not_reached_via_session_when_tally_saturates() {
        // The substrate tally guards against absurd totals by
        // returning QuorumNotReached when approval+rejection
        // exceeds 100_000 bps (10 full-cap voters). Stacking 11
        // voters each at the 10_000 bps cap trips the guard at the
        // quorum-projection step, after the 10th voter is appended
        // to the ledger. The 11th vote succeeds at the append step
        // but the projection step fails with QuorumNotReached.
        let session = GovernanceSession::new(voter_did(), Arc::new(FixedClock::new(1_700_000_888)));
        for i in 0..10 {
            let cap_id = format!("voter-cap-{i:02}");
            let did = format!("did:octo:z6MkVoter{i:02}XYZABCDEF1234567890abcdef1234567890ab");
            let signer: Arc<dyn CapabilitySigner> = Arc::new(TestSigner::new());
            session.register_capability(&cap_id, signer);
            let token = CapabilityToken::new(&cap_id, &did, 10_000);
            vote_v2(
                &session,
                proposal_id(),
                VoteChoice::Yes,
                &token,
                None,
                None,
                false,
            )
            .unwrap_or_else(|e| panic!("voter {i} should succeed: {e:?}"));
        }
        // 11th voter crosses the 100_000 bps tally guard.
        let cap_id = "voter-cap-overflow".to_string();
        let did = "did:octo:z6MkVoterXXXYZABCDEF1234567890abcdef1234567890ab".to_string();
        let signer: Arc<dyn CapabilitySigner> = Arc::new(TestSigner::new());
        session.register_capability(&cap_id, signer);
        let token = CapabilityToken::new(&cap_id, &did, 10_000);
        let err = vote_v2(
            &session,
            proposal_id(),
            VoteChoice::Yes,
            &token,
            None,
            None,
            false,
        )
        .expect_err("11th voter must trip quorum guard");
        match err {
            GovernanceError::QuorumNotReached {
                approval_bps,
                rejection_bps,
                quorum_bps,
            } => {
                assert!(approval_bps + rejection_bps > 100_000);
                assert_eq!(quorum_bps, 100_000);
            }
            other => panic!("expected QuorumNotReached, got {other:?}"),
        }
        // Ledger state preserved: the 11th voter is appended before
        // the projection step runs, so 11 receipts are present even
        // though the projection failed. The substrate pattern is
        // append-then-project; the projection error does not roll
        // back the append (no saga / no rollback).
        assert_eq!(session.vote_log().voter_count(&proposal_id()), 11);
    }
}
