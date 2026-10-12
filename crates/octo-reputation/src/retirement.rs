//! Retirement eligibility gate (RFC-0968 §3 amendment + mission 0968 Phase 3).
//!
//! `declare_retirement_eligible` is the only entry point through which a
//! legacy compatibility adapter (Slash, DC-Rooted, Marketplace read-side) can
//! be retired. The gate composes three checks:
//!
//! 1. **Parity score** — the per-adapter evidence bundle claims a rolling
//!    parity score ≥ 0.999 (basis points × 10_000 encoded as `u32`).
//! 2. **Bucket coverage** — the evidence bundle covers at least
//!    `MIN_RETIREMENT_BUCKET_COUNT` distinct 1h buckets.
//! 3. **Governance authorisation** — a verified signature, plus a fresh
//!    snapshot whose `members` list reaches `GOVERNANCE_QUORUM` entries.
//!
//! The signature is now verified. `declare_retirement_eligible` calls
//! `GovernanceProof::verify_signature` before reporting eligibility, so this
//! module's shape check is no longer the whole of it. What is still shape
//! only:
//!
//! - Asserts the snapshot is fresh (`MAX_GOVERNANCE_SNAPSHOT_AGE_SECS`).
//! - Asserts the snapshot's `governance_set_hash` matches the proof claim.
//!   **This is self-consistency, not authorisation.** Both sides are fields
//!   the caller supplies, so agreement between them proves only that the
//!   caller filled in both. RFC-0968 requires this hash to be checked
//!   against `GovernanceRegistry::lookup_at_snapshot`'s active-set digest,
//!   which is a lookup against state this function never consults.
//! - Asserts `snapshot.members.len() >= GOVERNANCE_QUORUM`. It counts
//!   entries. It does not check the members are distinct, and it does not
//!   verify anything about any of them, so repeating one 32-byte value
//!   three times satisfies it.
//! A caller who reaches this function alone can still satisfy every check
//! here: a fresh `finalized_at_unix`, a set hash matching itself, and three
//! members. That is why the store no longer treats this as sufficient — it
//! verifies the signature too. The quorum and set-hash halves remain open,
//! tracked in `missions/deferred/0968-c-governance-proof-verification.md`.
//!
//! The `signature.len() >= MIN_RETIREMENT_SIGNATURE_BYTES` check that used
//! to sit here is **removed**. The constant documented itself as "3 × 32 =
//! 96, the compact ed25519-packing for a 3-sig governance proof", but
//! `GovernanceProof` carries a single signature, so no genuine signature
//! from the registered algorithm could have reached 96 bytes. It rejected
//! every real proof while accepting any long enough run of zeros. Signature
//! validity is the verifier's business now.
//!
//! Returns `RetirementEligibility { eligible, since_unix, evidence_hash, adapter }`
//! on success. The `evidence_hash` is the canonical envelope:
//!
//! ```text
//! BLAKE3(BLAKE3_REPUTATION_RETIREMENT_DOMAIN
//!       || evidence.evidence_hash
//!       || evidence.last_bucket_unix (BE u64)
//!       || adapter (1 byte))
//! ```
//!
//! ## Per-adapter independence
//!
//! Each adapter retires independently. A successful declaration against
//! adapter `0x03` (Marketplace) does not retire adapters `0x04` (Slash) or
//! `0x05` (DcSlash). Operators must run a separate declaration per adapter.

use crate::auth::GovernanceProof;
use crate::constants::{BLAKE3_REPUTATION_RETIREMENT_DOMAIN, GOVERNANCE_QUORUM};
use crate::error::ReputationError;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{ParityEvidence, RetirementEligibility};

/// Minimum parity score (basis points × 10_000) for retirement eligibility.
/// `9999` represents `0.9999` — above the `PARITY_THRESHOLD = 0.999` set
/// in `parity.rs`.
pub const MIN_RETIREMENT_PARITY_SCORE_BP: u32 = 9_999;

/// Minimum number of distinct 1h buckets the parity evidence must cover.
pub const MIN_RETIREMENT_BUCKET_COUNT: u64 = 24;

/// Adapter identifier for the Marketplace read-side. Per mission 0968-b.
pub const ADAPTER_MARKETPLACE: u8 = 0x03;

/// Adapter identifier for the Slash store.
pub const ADAPTER_SLASH: u8 = 0x04;

/// Adapter identifier for the DC-rooted Slash store.
pub const ADAPTER_DC_SLASH: u8 = 0x05;

/// All known adapter identifiers. Used by `known_adapter` for shape
/// validation. Add new adapters here as missions adopt them.
pub const KNOWN_ADAPTERS: &[u8] = &[ADAPTER_MARKETPLACE, ADAPTER_SLASH, ADAPTER_DC_SLASH];

fn known_adapter(a: u8) -> bool {
    KNOWN_ADAPTERS.contains(&a)
}

/// Shape-check a `GovernanceProof`. Returns `Ok(())` iff:
///
/// - snapshot is fresh against `now_unix`
/// - snapshot's `governance_set_hash` equals the proof's claim
/// - `snapshot.members` holds at least `GOVERNANCE_QUORUM` entries
///
/// **None of these is an authorisation check.** The set-hash comparison is
/// self-consistency between two caller-supplied fields, the member count is
/// a length with no distinctness and nothing verified about the members, and
/// the signature check is a length on bytes that are never parsed. RFC-0968
/// requires an Ed25519 signature checked against `governance_pubkey` and a
/// set hash checked against the governance registry's active-set digest.
///
/// Real signature verification is deferred. See the module docs and
/// `missions/deferred/0968-c-governance-proof-verification.md`.
pub fn stub_verify_proof_shape(
    proof: &GovernanceProof,
    now_unix: u64,
) -> Result<(), ReputationError> {
    if !proof.snapshot.is_fresh(now_unix) {
        return Err(ReputationError::GovernanceSnapshotStale {
            age_secs: proof.snapshot.age_secs(now_unix),
            max: crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
        });
    }
    if proof.snapshot.governance_set_hash != proof.governance_set_hash {
        return Err(ReputationError::GovernanceSetHashMismatch);
    }
    if proof.snapshot.quorum_count() < GOVERNANCE_QUORUM {
        return Err(ReputationError::GovernanceQuorumNotMet {
            signatures: proof.snapshot.quorum_count(),
            quorum: GOVERNANCE_QUORUM,
        });
    }
    Ok(())
}

/// Validate a `ParityEvidence` bundle for retirement-gate consumption.
pub fn validate_evidence(ev: &ParityEvidence) -> Result<(), ReputationError> {
    if !known_adapter(ev.adapter) {
        return Err(ReputationError::RetirementNotAuthorized("adapter"));
    }
    if ev.parity_score < MIN_RETIREMENT_PARITY_SCORE_BP {
        return Err(ReputationError::RetirementNotAuthorized("parity_score"));
    }
    if ev.bucket_count < MIN_RETIREMENT_BUCKET_COUNT {
        return Err(ReputationError::RetirementNotAuthorized("bucket_count"));
    }
    if ev.first_bucket_unix > ev.last_bucket_unix {
        return Err(ReputationError::RetirementNotAuthorized(
            "bucket_window_inverted",
        ));
    }
    if ev.evidence_hash == [0u8; 32] {
        return Err(ReputationError::RetirementNotAuthorized(
            "evidence_hash_zero",
        ));
    }
    Ok(())
}

/// Compute the canonical retirement envelope hash.
///
/// ```text
/// BLAKE3(BLAKE3_REPUTATION_RETIREMENT_DOMAIN
///       || evidence.evidence_hash
///       || evidence.last_bucket_unix (BE u64)
///       || adapter (1 byte))
/// ```
pub fn retirement_envelope_hash(
    evidence_hash: &[u8; 32],
    last_bucket_unix: u64,
    adapter: u8,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(BLAKE3_REPUTATION_RETIREMENT_DOMAIN);
    hasher.update(evidence_hash);
    hasher.update(&last_bucket_unix.to_be_bytes());
    hasher.update(&[adapter]);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    arr
}

/// Reference default — `declare_retirement_eligible` on any `ReputationStore`
/// that wants the canonical envelope hash for a given evidence bundle.
///
/// Session 5 re-exports this so binary `reputation-declare-retirement`
/// (future) and tests can compute the same hash.
pub async fn declare_on<S: ReputationStore + ?Sized>(
    store: &S,
    adapter: u8,
    evidence: ParityEvidence,
    proof: GovernanceProof,
    now_unix: u64,
) -> StoreResult<RetirementEligibility> {
    store
        .declare_retirement_eligible(adapter, evidence, proof, now_unix)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AssetTag, GovernanceSnapshot};
    use crate::store::InMemoryReputationStore;
    use crate::types::RecorderId;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn good_snapshot(now: u64) -> GovernanceSnapshot {
        GovernanceSnapshot {
            finalized_at_unix: now,
            governance_set_hash: [1u8; 32],
            members: vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        }
    }

    fn good_proof(now: u64) -> GovernanceProof {
        // Signed over the proof's own suspension preimage.
        //
        // This used to emit a 32-byte all-ones pubkey and a signature of
        // `MIN_RETIREMENT_SIGNATURE_BYTES` zeros — a byte-length stub. It
        // passed because nothing on this path parsed the signature, which is
        // precisely why the suite could not tell retirement authorisation
        // from a shape check.
        let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let mut proof = GovernanceProof {
            governance_algorithm_id: crate::auth::default_governance_algorithm_id(),
            governance_pubkey: sk.verifying_key().to_bytes().to_vec(),
            recorder_id: RecorderId::from_u64(0),
            reason_hash: [0u8; 32],
            signature: Vec::new(),
            snapshot: good_snapshot(now),
            governance_set_hash: [1u8; 32],
            slash_destination: None,
            slash_amount: 0,
            slash_asset: AssetTag::None,
        };
        let digest = proof.suspension_signature_digest(now);
        proof.signature = ed25519_dalek::Signer::sign(&sk, &digest)
            .to_bytes()
            .to_vec();
        proof
    }

    fn good_evidence(adapter: u8) -> ParityEvidence {
        ParityEvidence {
            adapter,
            parity_score: MIN_RETIREMENT_PARITY_SCORE_BP,
            bucket_count: MIN_RETIREMENT_BUCKET_COUNT,
            first_bucket_unix: 0,
            last_bucket_unix: 86_400,
            evidence_hash: [7u8; 32],
        }
    }

    fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[tokio::test]
    async fn declare_retirement_happy_path_marketplace() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        let r = store
            .declare_retirement_eligible(
                ADAPTER_MARKETPLACE,
                good_evidence(ADAPTER_MARKETPLACE),
                good_proof(now),
                now,
            )
            .await
            .unwrap();
        assert!(r.eligible);
        assert_eq!(r.adapter, ADAPTER_MARKETPLACE);
        assert_ne!(r.evidence_hash, [0u8; 32]);
        assert_eq!(r.since_unix, now);
    }

    /// Retirement eligibility must require a signature, not a byte count.
    ///
    /// Every check `stub_verify_proof_shape` makes describes caller-supplied data:
    /// a freshness timestamp the caller sets, a set hash compared against the
    /// proof's own claim, and a member count. Before the verifier was wired in,
    /// `eligible: true` was reachable holding no key at all.
    #[tokio::test]
    async fn declare_retirement_refuses_an_unsigned_proof() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        let mut proof = good_proof(now);
        proof.signature = vec![0u8; 64];
        let err = store
            .declare_retirement_eligible(
                ADAPTER_MARKETPLACE,
                good_evidence(ADAPTER_MARKETPLACE),
                proof,
                now,
            )
            .await
            .unwrap_err();
        assert_eq!(
            err.discriminant(),
            0x11,
            "an unsigned retirement proof must be refused as an invalid signature"
        );
    }

    /// A proof signed by a key other than the one it names.
    #[tokio::test]
    async fn declare_retirement_refuses_a_proof_signed_by_another_key() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        let mut proof = good_proof(now);
        let other = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        let digest = proof.suspension_signature_digest(now);
        proof.signature = ed25519_dalek::Signer::sign(&other, &digest)
            .to_bytes()
            .to_vec();
        assert!(
            store
                .declare_retirement_eligible(
                    ADAPTER_MARKETPLACE,
                    good_evidence(ADAPTER_MARKETPLACE),
                    proof,
                    now,
                )
                .await
                .is_err(),
            "a signature from a key other than the named one must not confer \
         retirement eligibility"
        );
    }

    /// A proof whose signature is truncated must be refused.
    ///
    /// This used to be a length check against a 96-byte threshold, and the
    /// constant behind it documented itself as "3 × 32 = 96, the compact
    /// ed25519-packing for a 3-sig governance proof". `GovernanceProof`
    /// carries a **single** `signature`, so no genuine signature from the
    /// registered algorithm could ever have reached 96 bytes — the check
    /// rejected every real proof and accepted any long enough run of zeros.
    ///
    /// It is replaced rather than reworded: the signature is now verified,
    /// so a truncated one fails as a signature failure and not as a length.
    #[tokio::test]
    async fn declare_retirement_truncated_signature_rejected() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        let mut proof = good_proof(now);
        proof.signature.truncate(32);
        let err = store
            .declare_retirement_eligible(
                ADAPTER_MARKETPLACE,
                good_evidence(ADAPTER_MARKETPLACE),
                proof,
                now,
            )
            .await
            .unwrap_err();
        assert_eq!(
            err.discriminant(),
            0x11,
            "expected an invalid-signature refusal"
        );
    }

    #[tokio::test]
    async fn declare_retirement_stale_snapshot_rejected() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        let mut proof = good_proof(now);
        // Snapshot is far in the past — past the staleness threshold.
        proof.snapshot = GovernanceSnapshot {
            finalized_at_unix: 0,
            governance_set_hash: [1u8; 32],
            members: vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        };
        let err = store
            .declare_retirement_eligible(
                ADAPTER_MARKETPLACE,
                good_evidence(ADAPTER_MARKETPLACE),
                proof,
                now,
            )
            .await
            .unwrap_err();
        assert_eq!(err.discriminant(), 0x10);
    }

    #[tokio::test]
    async fn declare_retirement_per_adapter_independence() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        // Marketplace declaration.
        let r_market = store
            .declare_retirement_eligible(
                ADAPTER_MARKETPLACE,
                good_evidence(ADAPTER_MARKETPLACE),
                good_proof(now),
                now,
            )
            .await
            .unwrap();
        assert!(r_market.eligible);
        assert_eq!(r_market.adapter, ADAPTER_MARKETPLACE);

        // Unknown adapter (0xFF) → rejected.
        let unknown_evidence = good_evidence(0xFF);
        let err = store
            .declare_retirement_eligible(0xFF, unknown_evidence, good_proof(now), now)
            .await
            .unwrap_err();
        assert_eq!(err, ReputationError::RetirementNotAuthorized("adapter"));

        // Slash (0x04) is a known adapter; accepted but does not retire Marketplace.
        let slash_evidence = good_evidence(ADAPTER_SLASH);
        let r_slash = store
            .declare_retirement_eligible(ADAPTER_SLASH, slash_evidence, good_proof(now), now)
            .await
            .unwrap();
        assert_eq!(r_slash.adapter, ADAPTER_SLASH);
        assert_ne!(r_market.evidence_hash, r_slash.evidence_hash);
    }

    #[tokio::test]
    async fn declare_retirement_low_parity_score_rejected() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        let mut ev = good_evidence(ADAPTER_MARKETPLACE);
        ev.parity_score = MIN_RETIREMENT_PARITY_SCORE_BP - 1;
        let err = store
            .declare_retirement_eligible(ADAPTER_MARKETPLACE, ev, good_proof(now), now)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            ReputationError::RetirementNotAuthorized("parity_score")
        );
    }

    #[tokio::test]
    async fn declare_retirement_low_bucket_count_rejected() {
        let store = InMemoryReputationStore::new();
        let now = now_unix();
        let mut ev = good_evidence(ADAPTER_MARKETPLACE);
        ev.bucket_count = MIN_RETIREMENT_BUCKET_COUNT - 1;
        let err = store
            .declare_retirement_eligible(ADAPTER_MARKETPLACE, ev, good_proof(now), now)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            ReputationError::RetirementNotAuthorized("bucket_count")
        );
    }

    #[tokio::test]
    async fn envelope_hash_is_deterministic() {
        let h1 = retirement_envelope_hash(&[1u8; 32], 1_000_000, ADAPTER_MARKETPLACE);
        let h2 = retirement_envelope_hash(&[1u8; 32], 1_000_000, ADAPTER_MARKETPLACE);
        assert_eq!(h1, h2);

        // Different adapter → different hash.
        let h3 = retirement_envelope_hash(&[1u8; 32], 1_000_000, ADAPTER_SLASH);
        assert_ne!(h1, h3);

        // Different bucket → different hash.
        let h4 = retirement_envelope_hash(&[1u8; 32], 1_000_001, ADAPTER_MARKETPLACE);
        assert_ne!(h1, h4);
    }

    #[test]
    fn envelope_hash_uses_retirement_domain() {
        // Sanity: hash should differ from a plain BLAKE3 of the same bytes.
        let mut hasher = blake3::Hasher::new();
        hasher.update(BLAKE3_REPUTATION_RETIREMENT_DOMAIN);
        hasher.update(&[1u8; 32]);
        hasher.update(&1_000_000_u64.to_be_bytes());
        hasher.update(&[ADAPTER_MARKETPLACE]);
        let out = hasher.finalize();
        let mut expected = [0u8; 32];
        expected.copy_from_slice(out.as_bytes());
        assert_eq!(
            retirement_envelope_hash(&[1u8; 32], 1_000_000, ADAPTER_MARKETPLACE),
            expected
        );
    }

    #[test]
    fn known_adapters_contain_marketplace_slash_dcslash() {
        for a in [ADAPTER_MARKETPLACE, ADAPTER_SLASH, ADAPTER_DC_SLASH] {
            assert!(known_adapter(a));
        }
        assert!(!known_adapter(0xFF));
        assert!(!known_adapter(0x00));
    }
}
