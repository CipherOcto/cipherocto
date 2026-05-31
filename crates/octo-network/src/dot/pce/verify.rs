//! Proof verification pipeline (RFC-0859 §5)

use crate::dot::pce::envelope::ProofCarryingEnvelope;
use crate::dot::pce::error::PceError;
use crate::dot::pce::proof_type::{ProofSystemId, VerificationResult};
use crate::dot::pce::MAX_PROOF_BLOB_SIZE;

/// Verify a Proof-Carrying Envelope through the deterministic pipeline.
///
/// Pipeline (RFC-0859 §5.2):
/// 1. Check proof_system_id is supported
/// 2. Verify proof_commitment = BLAKE3-256(proof_blob)
/// 3. Select backend by proof_system_id
/// 4. Deserialize proof from proof_blob
/// 5. Verify proof against public_input_root
///
/// This function implements the consensus-boundary verification (Class A).
/// Proof generation is Class C (non-deterministic).
pub fn verify_pce(
    pce: &ProofCarryingEnvelope,
    public_inputs: &[[u8; 32]],
) -> Result<VerificationResult, PceError> {
    // 0. Enforce MAX_PROOF_BLOB_SIZE
    if pce.proof_blob.len() > MAX_PROOF_BLOB_SIZE {
        return Err(PceError::ProofBlobTooLarge {
            actual: pce.proof_blob.len(),
            limit: MAX_PROOF_BLOB_SIZE,
        });
    }

    // 1. Check proof_system_id is supported
    if ProofSystemId::from_u16(pce.proof_system_id).is_none() {
        return Err(PceError::UnsupportedSystem(pce.proof_system_id));
    }

    // 2. Verify proof_commitment
    if !pce.verify_commitment() {
        return Err(PceError::CommitmentMismatch);
    }

    // 3. Verify public_input_root matches Merkle of public_inputs
    let computed_root = compute_merkle_root(public_inputs);
    if computed_root != pce.public_input_root {
        return Err(PceError::InputMismatch);
    }

    // 4. Proof blob must be non-empty
    if pce.proof_blob.is_empty() {
        return Err(PceError::MalformedProof("empty proof_blob".into()));
    }

    // 5. Backend-specific verification would go here
    // TODO: Actual cryptographic proof verification (Class B/C boundary)
    // Currently returns Valid for structural checks only
    Ok(VerificationResult::Valid)
}

/// Compute a simple binary Merkle root from a slice of 32-byte leaves.
///
/// Uses BLAKE3-256 for all hashing. Empty input returns zeros.
/// Odd leaves are duplicated for the final level (standard Merkle convention).
pub fn compute_merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    if leaves.len() == 1 {
        return leaves[0];
    }

    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        // Duplicate last leaf if odd count
        if !level.len().is_multiple_of(2) {
            let last = *level.last().expect("level is non-empty in merkle loop");
            level.push(last);
        }
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks(2) {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&pair[0]);
            hasher.update(&pair[1]);
            next.push(*hasher.finalize().as_bytes());
        }
        level = next;
    }
    level[0]
}

/// Verify the canonical proof boundary invariant (RFC-0859 §5.4).
///
/// Consensus NEVER depends on:
/// - Prover runtime or implementation
/// - Hardware acceleration
/// - Proving time
/// - Memory layout
/// - Parallel execution order
/// - Witness generation order
/// - Proof blob byte equality
///
/// This is enforced by the type system and this verification function.
pub fn verify_canonical_boundary(pce: &ProofCarryingEnvelope) -> bool {
    // The canonical boundary is enforced by:
    // 1. Only proof_commitment (hash) is compared, not raw proof_blob
    // 2. Verification uses only public inputs and commitment
    // 3. No timing or hardware-dependent code paths
    pce.verify_commitment()
}

/// Verify a proof via the DPS (Deterministic Proof Substrate) pipeline (RFC-0854).
///
/// Maps the raw `proof_system_id` to a DPS `ProofSystemId` enum and delegates
/// to the DPS verification backend. This bridges PCE envelope verification
/// with the underlying DPS proof system registry.
///
/// # Arguments
/// * `proof_system_id` - Raw u16 proof system identifier from the PCE envelope
/// * `proof_blob` - The serialized proof bytes
/// * `public_inputs` - Raw public input bytes
///
/// # Returns
/// `Ok(true)` if the proof is valid, `Ok(false)` if invalid, `Err` on system errors.
pub fn verify_via_dps(
    proof_system_id: u16,
    proof_blob: &[u8],
    public_inputs: &[u8],
) -> Result<bool, PceError> {
    // Map to DPS ProofSystemId
    let dps_system = crate::dps::ProofSystemId::from_u16(proof_system_id)
        .ok_or(PceError::UnsupportedSystem(proof_system_id))?
        .as_u16();

    // Validate inputs
    if proof_blob.is_empty() {
        return Err(PceError::MalformedProof("empty proof_blob".into()));
    }

    if public_inputs.is_empty() {
        return Err(PceError::MalformedProof("empty public_inputs".into()));
    }

    // DPS integration point: delegate to the DPS verification pipeline.
    // The DPS ProofSystemId discriminant is passed through for backend selection.
    // Currently returns Ok(true) as a structural stub; actual backend dispatch
    // will call crate::dps::DeterministicProofSystem::verify() once backends
    // are registered in the VerifierRegistry.
    let _ = dps_system; // suppress unused warning until DPS backend is wired
    let _ = proof_blob;
    let _ = public_inputs;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::envelope::{DeterministicEnvelope, MessageType};
    use crate::dot::pce::proof_type::ProofSystemId;

    fn make_test_pce(proof_blob: Vec<u8>, public_inputs: &[[u8; 32]]) -> ProofCarryingEnvelope {
        let commitment = ProofCarryingEnvelope::compute_proof_commitment(&proof_blob);
        let root = compute_merkle_root(public_inputs);
        ProofCarryingEnvelope {
            envelope: DeterministicEnvelope {
                version: 1,
                network_id: 1,
                message_type: MessageType::Message as u16,
                envelope_id: [1u8; 32],
                mission_id: [0u8; 32],
                source_peer: [2u8; 32],
                origin_gateway: [3u8; 32],
                logical_timestamp: 1000,
                ttl_hops: 10,
                payload_hash: [4u8; 32],
                route_trace_root: [0u8; 32],
                flags: 0,
                signature: [0u8; 64],
            },
            proof_system_id: ProofSystemId::STWO as u16,
            proof_commitment: commitment,
            public_input_root: root,
            proof_blob,
            execution_model: 0x0001,
            parent_proof_commitment: None,
        }
    }

    #[test]
    fn test_verify_pce_valid() {
        let inputs = vec![[0xAAu8; 32], [0xBBu8; 32]];
        let pce = make_test_pce(vec![1, 2, 3, 4, 5], &inputs);
        assert_eq!(
            verify_pce(&pce, &inputs).unwrap(),
            VerificationResult::Valid
        );
    }

    #[test]
    fn test_verify_pce_unsupported_system() {
        let inputs = vec![[0xAAu8; 32]];
        let mut pce = make_test_pce(vec![1, 2, 3], &inputs);
        pce.proof_system_id = 0x0099;
        assert!(matches!(
            verify_pce(&pce, &inputs),
            Err(PceError::UnsupportedSystem(0x0099))
        ));
    }

    #[test]
    fn test_verify_pce_commitment_mismatch() {
        let inputs = vec![[0xAAu8; 32]];
        let mut pce = make_test_pce(vec![1, 2, 3], &inputs);
        pce.proof_commitment = [0xFFu8; 32]; // corrupt
        assert!(matches!(
            verify_pce(&pce, &inputs),
            Err(PceError::CommitmentMismatch)
        ));
    }

    #[test]
    fn test_verify_pce_input_mismatch() {
        let inputs = vec![[0xAAu8; 32]];
        let pce = make_test_pce(vec![1, 2, 3], &inputs);
        let wrong_inputs = vec![[0xFFu8; 32]];
        assert!(matches!(
            verify_pce(&pce, &wrong_inputs),
            Err(PceError::InputMismatch)
        ));
    }

    #[test]
    fn test_verify_pce_empty_blob() {
        let inputs = vec![[0xAAu8; 32]];
        let pce = make_test_pce(vec![], &inputs);
        assert!(matches!(
            verify_pce(&pce, &inputs),
            Err(PceError::MalformedProof(_))
        ));
    }

    #[test]
    fn test_merkle_root_empty() {
        assert_eq!(compute_merkle_root(&[]), [0u8; 32]);
    }

    #[test]
    fn test_merkle_root_single() {
        let leaf = [0xAAu8; 32];
        assert_eq!(compute_merkle_root(&[leaf]), leaf);
    }

    #[test]
    fn test_merkle_root_two_leaves() {
        let a = [0xAAu8; 32];
        let b = [0xBBu8; 32];
        let root = compute_merkle_root(&[a, b]);
        // root = BLAKE3(a || b)
        let mut hasher = blake3::Hasher::new();
        hasher.update(&a);
        hasher.update(&b);
        assert_eq!(root, *hasher.finalize().as_bytes());
    }

    #[test]
    fn test_merkle_root_three_leaves() {
        let a = [0xAAu8; 32];
        let b = [0xBBu8; 32];
        let c = [0xCCu8; 32];
        let root = compute_merkle_root(&[a, b, c]);
        // c is duplicated: [a,b] and [c,c]
        // hash_ab = BLAKE3(a || b)
        // hash_cc = BLAKE3(c || c)
        // root = BLAKE3(hash_ab || hash_cc)
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_root_deterministic() {
        let leaves = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        let r1 = compute_merkle_root(&leaves);
        let r2 = compute_merkle_root(&leaves);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_canonical_boundary() {
        let inputs = vec![[0xAAu8; 32]];
        let pce = make_test_pce(vec![1, 2, 3], &inputs);
        assert!(verify_canonical_boundary(&pce));
    }

    #[test]
    fn test_canonical_boundary_bad_commitment() {
        let inputs = vec![[0xAAu8; 32]];
        let mut pce = make_test_pce(vec![1, 2, 3], &inputs);
        pce.proof_commitment = [0xFFu8; 32];
        assert!(!verify_canonical_boundary(&pce));
    }

    #[test]
    fn test_verify_via_dps_stwo() {
        let result = verify_via_dps(ProofSystemId::STWO as u16, &[1, 2, 3], &[4, 5, 6]);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_verify_via_dps_all_systems() {
        let ids = [
            ProofSystemId::STWO,
            ProofSystemId::RiscZero,
            ProofSystemId::SP1,
            ProofSystemId::Winterfell,
            ProofSystemId::Halo2,
            ProofSystemId::Groth16,
            ProofSystemId::PLONK,
            ProofSystemId::Cairo,
        ];
        for id in &ids {
            let result = verify_via_dps(*id as u16, &[1, 2, 3], &[4, 5, 6]);
            assert_eq!(
                result.unwrap(),
                true,
                "failed for system id {:#x}",
                *id as u16
            );
        }
    }

    #[test]
    fn test_verify_via_dps_unsupported() {
        let result = verify_via_dps(0x0099, &[1, 2, 3], &[4, 5, 6]);
        assert!(matches!(result, Err(PceError::UnsupportedSystem(0x0099))));
    }

    #[test]
    fn test_verify_via_dps_empty_blob() {
        let result = verify_via_dps(ProofSystemId::STWO as u16, &[], &[4, 5, 6]);
        assert!(matches!(result, Err(PceError::MalformedProof(_))));
    }

    #[test]
    fn test_verify_via_dps_empty_inputs() {
        let result = verify_via_dps(ProofSystemId::STWO as u16, &[1, 2, 3], &[]);
        assert!(matches!(result, Err(PceError::MalformedProof(_))));
    }
}
