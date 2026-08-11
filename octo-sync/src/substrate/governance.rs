//! Governance attestation + signature verification (per RFC-0862 v1.3
//! §Specification §WriterElection Protocol §Governance).
//!
//! Includes:
//! - `GovernanceAttestation` struct (binds shard_key + chain_id + term + signatures + nonce)
//! - `governance_signature_message` (BLAKE3-256 domain-separated binding)
//! - `verify_governance_attestation` (M-of-N threshold + ed25519 verifications + nonce consume)
//! - `ed25519_verify` (single-signature verify helper)
//!
//! `MAX_GOVERNANCE_SIGNATURES = 32` per RFC-0862 v1.3 R12 M23 (DoS bound).

use std::collections::HashSet;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use octo_ident::ChainId;

use super::ids::OperatorId;
use super::ids::OperatorSet;
use super::ids::ShardKey;
use super::records::NonceRecord;
use super::records::WriterElectionError;

/// Maximum signatures per `GovernanceAttestation` (per RFC-0862 v1.3
/// R12 M23). Caps the ed25519-verify cost against a malicious deployment.
pub const MAX_GOVERNANCE_SIGNATURES: usize = 32;

/// Governance attestation for a `force_relinquish_writer` call (per
/// RFC-0862 v1.3 §Specification §Governance).
///
/// `chain_id` binds the attestation to a specific deployment (per
/// R12 M23: prevents replay across deployments sharing an operator
/// set). `nonce` is consumed via `NonceTracker::consume` to bound
/// replay risk within a deployment.
#[derive(Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct GovernanceAttestation {
    /// Shard key the attestation applies to.
    pub shard_key: ShardKey,
    /// Deployment chain id (per R12 M23: replay-binding).
    pub chain_id: ChainId,
    /// Election term the attestation applies to.
    pub term: u64,
    /// Advisory list of operators (per R11 M5: signatures carry
    /// `operator_id`; field retained for forward-compat with future
    /// operator-set updates).
    pub operators: Vec<OperatorId>,
    /// Co-signatures collected for the attestation.
    pub signatures: Vec<OperatorSignature>,
    /// Threshold of valid signatures required.
    pub threshold: usize,
    /// Single-use nonce bound to `(shard_key, term)`.
    pub nonce: [u8; 32],
}

/// Single operator signature over `governance_signature_message`.
#[derive(Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct OperatorSignature {
    /// Operator that produced the signature.
    pub operator_id: OperatorId,
    /// 64-byte ed25519 signature.
    pub signature: [u8; 64],
}

/// Domain-separated message bound over `(shard_key, chain_id, term, nonce)`.
///
/// Per RFC-0862 v1.3 R12 M23: the `chain_id` inclusion binds the
/// attestation to a deployment so an operator-set cannot be replayed
/// across deployments. The blake3 domain prefix distinguishes
/// governance signatures from other protocol signatures.
pub fn governance_signature_message(
    shard_key: &ShardKey,
    chain_id: &ChainId,
    term: u64,
    nonce: &[u8; 32],
) -> [u8; 32] {
    let mut input = Vec::with_capacity(93 + 32 + 8);
    input.extend_from_slice(b"cipherocto/governance/v1");
    input.extend_from_slice(&shard_key.0);
    input.extend_from_slice(chain_id.as_str().as_bytes());
    input.extend_from_slice(&term.to_be_bytes());
    input.extend_from_slice(nonce);
    *blake3::hash(&input).as_bytes()
}

/// ed25519 signature verification (per RFC-0862 v1.3 §Supporting types).
///
/// Returns `true` if the signature is valid for the supplied message
/// and public key. Used by `verify_governance_attestation` and by
/// follow-on substrate verifiers.
pub fn ed25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let Ok(verifying_key) = VerifyingKey::from_bytes(pk) else {
        return false;
    };
    let signature = Signature::from_bytes(sig);
    verifying_key.verify(msg, &signature).is_ok()
}

/// Verify a `GovernanceAttestation` against the configured
/// operator set + nonce tracker.
///
/// Per RFC-0862 v1.3 §Specification §Governance:
/// 1. `shard_key` + `chain_id` + `threshold` must match configured values
/// 2. Signature count must be `≤ MAX_GOVERNANCE_SIGNATURES`
/// 3. Each signature is verified against `governance_signature_message`
/// 4. Each signer must be in the configured operator set
/// 5. Each signer must be unique (no duplicate signatures)
/// 6. Valid signature count must meet/exceed threshold
/// 7. Nonce must be consumed via `NonceTracker::consume` (replay-resistance)
pub fn verify_governance_attestation(
    shard_key: &ShardKey,
    chain_id: &ChainId,
    attestation: &GovernanceAttestation,
    configured_operator_set: &OperatorSet,
    nonce_tracker: &NonceTracker,
) -> Result<(), WriterElectionError> {
    if attestation.shard_key != *shard_key {
        return Err(WriterElectionError::ShardKeyMismatch);
    }
    if attestation.chain_id != *chain_id {
        return Err(WriterElectionError::ChainIdMismatch);
    }
    if attestation.threshold != configured_operator_set.threshold {
        return Err(WriterElectionError::ThresholdMismatch);
    }
    if attestation.signatures.len() > MAX_GOVERNANCE_SIGNATURES {
        return Err(WriterElectionError::TooManySignatures {
            count: attestation.signatures.len(),
            max: MAX_GOVERNANCE_SIGNATURES,
        });
    }
    let message = governance_signature_message(
        &attestation.shard_key,
        chain_id,
        attestation.term,
        &attestation.nonce,
    );
    let configured_set: HashSet<&OperatorId> = configured_operator_set.operators.iter().collect();
    let mut unique_signers = HashSet::new();
    let mut valid_count = 0;
    for sig in &attestation.signatures {
        if !unique_signers.insert(sig.operator_id) {
            return Err(WriterElectionError::DuplicateSigner);
        }
        if !configured_set.contains(&sig.operator_id) {
            return Err(WriterElectionError::UnauthorizedSigner);
        }
        if !ed25519_verify(&sig.operator_id.pubkey(), &message, &sig.signature) {
            return Err(WriterElectionError::InvalidSignature);
        }
        valid_count += 1;
    }
    if valid_count < attestation.threshold {
        return Err(WriterElectionError::InsufficientSignatures);
    }
    nonce_tracker.consume(&attestation.shard_key, attestation.term, &attestation.nonce)?;
    Ok(())
}

/// Per-shard replay-resistant nonce tracker (per RFC-0862 v1.3 R11 H1 + R13 M3).
///
/// Stores `(term, nonce)` tuples per `ShardKey` so `gc_expired_nonces`
/// can prune by term boundary. `consume` writes the nonce to the WAL
/// (durable record) so process restart can rebuild the in-memory map
/// via `replay_from_wal`.
pub struct NonceTracker {
    used_nonces: dashmap::DashMap<ShardKey, std::collections::HashSet<(u64, [u8; 32])>>,
    wal: std::sync::Arc<dyn WalAppender>,
}

impl NonceTracker {
    /// Build a `NonceTracker` and replay existing nonce records from
    /// the WAL.
    pub fn new(wal: std::sync::Arc<dyn WalAppender>) -> Self {
        let used_nonces = Self::replay_from_wal(&*wal);
        Self { used_nonces, wal }
    }

    /// Replay all `ENTRY_TYPE_NONCE_RECORD` entries from the WAL into
    /// the in-memory map. Called once at `new`.
    fn replay_from_wal(
        wal: &dyn WalAppender,
    ) -> dashmap::DashMap<ShardKey, std::collections::HashSet<(u64, [u8; 32])>> {
        let map: dashmap::DashMap<ShardKey, std::collections::HashSet<(u64, [u8; 32])>> =
            dashmap::DashMap::new();
        for nonce_record in wal.scan_nonce_records() {
            map.entry(nonce_record.shard_key)
                .or_default()
                .insert((nonce_record.term, nonce_record.nonce));
        }
        map
    }

    /// Consume a nonce; fail if already used.
    ///
    /// Per RFC-0862 v1.3 R12 H15: check-then-append (NOT append-then-check)
    /// so replayed nonces do NOT grow the WAL unboundedly. Per R13 M4:
    /// on WAL append failure, ROLL BACK the in-memory insert.
    pub fn consume(
        &self,
        shard_key: &ShardKey,
        term: u64,
        nonce: &[u8; 32],
    ) -> Result<(), WriterElectionError> {
        let key = (term, *nonce);
        let mut set = self.used_nonces.entry(shard_key.clone()).or_default();
        if !set.insert(key) {
            return Err(WriterElectionError::NonceReplayed);
        }
        let record = NonceRecord {
            shard_key: shard_key.clone(),
            term,
            nonce: *nonce,
        };
        if let Err(e) = self.wal.append_nonce_record(&record) {
            set.remove(&key);
            return Err(e);
        }
        Ok(())
    }

    /// Prune nonces older than `current_term - MAX_NONCE_RETENTION_TERMS`.
    ///
    /// Per RFC-0862 v1.3 R12 H15 + R13 M3: term-scoped GC. Runs on each
    /// new term boundary.
    pub fn gc_expired_nonces(&self, current_term: u64) {
        const MAX_NONCE_RETENTION_TERMS: u64 = 1_000;
        for mut entry in self.used_nonces.iter_mut() {
            entry
                .value_mut()
                .retain(|(term, _)| *term + MAX_NONCE_RETENTION_TERMS >= current_term);
        }
    }

    /// Test-only: peek the in-memory map (no WAL replay).
    #[cfg(test)]
    pub fn in_memory_for_test(&self) -> usize {
        self.used_nonces.iter().map(|e| e.value().len()).sum()
    }
}

/// Lightweight seal for `WalAppender` reuse in `NonceTracker`.
///
/// The full `WalWriter` + `WalReader` + `WalNonceScanner` traits live
/// in the `wal_traits` module. `NonceTracker` only needs
/// `append_nonce_record` + `scan_nonce_records`, so we declare a
/// minimal local trait bound here to avoid pulling the full
/// `WalAppender` supertrait into this module.
pub trait WalAppender: Send + Sync {
    /// Append a nonce record to the WAL. Returns `WriterElectionError::WalCorruption`
    /// on disk failure.
    fn append_nonce_record(&self, record: &NonceRecord) -> Result<(), WriterElectionError>;
    /// Scan all nonce records (used in `replay_from_wal`).
    fn scan_nonce_records(&self) -> Box<dyn Iterator<Item = NonceRecord> + '_>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governance_message_is_deterministic() {
        let sk = ShardKey([1u8; 32]);
        let cid = ChainId::new("cipherocto-mainnet");
        let m1 = governance_signature_message(&sk, &cid, 5, &[7u8; 32]);
        let m2 = governance_signature_message(&sk, &cid, 5, &[7u8; 32]);
        assert_eq!(m1, m2);
    }

    #[test]
    fn governance_message_binds_chain_id() {
        let sk = ShardKey([1u8; 32]);
        let cid1 = ChainId::new("chain-a");
        let cid2 = ChainId::new("chain-b");
        let m1 = governance_signature_message(&sk, &cid1, 5, &[7u8; 32]);
        let m2 = governance_signature_message(&sk, &cid2, 5, &[7u8; 32]);
        assert_ne!(m1, m2);
    }

    #[test]
    fn governance_message_binds_term() {
        let sk = ShardKey([1u8; 32]);
        let cid = ChainId::new("cipherocto-mainnet");
        let m1 = governance_signature_message(&sk, &cid, 5, &[7u8; 32]);
        let m2 = governance_signature_message(&sk, &cid, 6, &[7u8; 32]);
        assert_ne!(m1, m2);
    }

    #[test]
    fn governance_message_binds_nonce() {
        let sk = ShardKey([1u8; 32]);
        let cid = ChainId::new("cipherocto-mainnet");
        let m1 = governance_signature_message(&sk, &cid, 5, &[7u8; 32]);
        let m2 = governance_signature_message(&sk, &cid, 5, &[8u8; 32]);
        assert_ne!(m1, m2);
    }

    #[test]
    fn ed25519_verify_rejects_garbage() {
        let pk = [0u8; 32];
        let msg = b"hello";
        let sig = [0u8; 64];
        assert!(!ed25519_verify(&pk, msg, &sig));
    }
}
