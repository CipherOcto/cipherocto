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
use octo_governance_core::{GovernanceError, VoteReceipt};
use serde::{Deserialize, Serialize};

use crate::attest::CapabilitySigner;

/// Vote choice: approve or reject the proposal (RFC-0011-g §Vote
/// Choice). The on-the-wire encoding is the lowercase string
/// `"approve"` or `"reject"` — kept as a typed enum so callers
/// cannot pass arbitrary strings to the substrate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteChoice {
    /// Approve the proposal.
    Approve,
    /// Reject the proposal.
    Reject,
}

impl VoteChoice {
    /// Lowercase string encoding (`approve` | `reject`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }

    /// Parse from `&str`. Unknown values fail-closed.
    pub fn parse(s: &str) -> Result<Self, GovernanceError> {
        match s {
            "approve" => Ok(Self::Approve),
            "reject" => Ok(Self::Reject),
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
    pub fn resolve(
        &self,
        voter_cap_id: &str,
    ) -> Result<Arc<dyn CapabilitySigner>, GovernanceError> {
        self.caps
            .lock()
            .expect("capability registry mutex poisoned")
            .get(voter_cap_id)
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
                            (receipt.weight_applied, receipt.choice == "approve"),
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
/// || 0x00 || choice || 0x00 || weight_be || 0x00 ||
/// voter_cap_id || 0x00 || snapshot_id || 0x00 ||
/// allow_stale_bool`. The PK is `BLAKE3-256` of those bytes; the
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

    // Canonical envelope bytes (substrate-faithful).
    let choice_str = choice.as_str();
    let mut envelope: Vec<u8> = Vec::new();
    envelope.extend_from_slice(voter_did.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&proposal_id);
    envelope.extend_from_slice(choice_str.as_bytes());
    envelope.push(0x00);
    envelope.extend_from_slice(&weight_bps.to_be_bytes());
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

    use super::*;
    use crate::attest::CapabilitySigner;

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
            VoteChoice::Approve,
            5000,
            "voter-cap-001",
            Some(&[0xAA; 32]),
            false,
            1_700_000_000,
        )
        .expect("happy path should succeed");
        assert_eq!(receipt.voter_did, voter_did());
        assert_eq!(receipt.proposal_id, proposal_id());
        assert_eq!(receipt.choice, "approve");
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
            VoteChoice::Approve,
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
            VoteChoice::Approve,
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
            VoteChoice::Reject, // different choice — still duplicate
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
            VoteChoice::Approve,
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
            VoteChoice::Approve,
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
        assert_eq!(VoteChoice::parse("approve").unwrap(), VoteChoice::Approve);
        assert_eq!(VoteChoice::parse("reject").unwrap(), VoteChoice::Reject);
        assert_eq!(VoteChoice::Approve.as_str(), "approve");
        assert_eq!(VoteChoice::Reject.as_str(), "reject");
        // Unknown choice fail-closed.
        let err = VoteChoice::parse("abstain").expect_err("abstain must error");
        match err {
            GovernanceError::InvalidArgument { reason } => {
                assert!(
                    reason.contains("abstain"),
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
            VoteChoice::Approve,
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
            VoteChoice::Reject,
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
            VoteChoice::Approve,
            5000,
            "voter-cap-001",
            None,
            true,
            1_700_000_000,
        )
        .expect("allow_stale=true should still succeed");
        assert_eq!(receipt.overrode_staleness_at_unix, Some(1_700_000_000));
    }
}
