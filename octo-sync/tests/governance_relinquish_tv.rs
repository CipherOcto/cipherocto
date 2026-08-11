//! 8 end-to-end test vectors for `WriterElectionForceRelinquish`
//! (mission `0871e-force-relinquish-governance`, RFC-0862 v1.3
//! §Specification §Governance).
//!
//! Covers the full attestation-verify → nonce-consume →
//! cluster-force-relinquish pipeline against a `RaftLikeWriterElection`
//! + `Cluster` + `NonceTracker` + 3-operator-set fixture.
//!
//! ## Test vectors
//!
//! - TV-1 two_of_three_force_relinquish_clears_lease
//! - TV-2 wrong_chain_id_rejected
//! - TV-3 replayed_nonce_rejected
//! - TV-4 unauthorized_signer_rejected
//! - TV-5 below_threshold_rejected
//! - TV-6 invalid_signature_rejected
//! - TV-7 duplicate_signer_rejected
//! - TV-8 shard_key_mismatch_rejected
//!
//! ## Why a single `force_relinquish_writer` TV suite
//!
//! Prior tests cover single-signer ed25519 verify + governance
//! message binding (`governance.rs::tests`) + cluster `force_relinquish`
//! (`cluster.rs::tests`). No TV exercises the full `M-of-N operator
//! attestation → NonceTracker consume → cluster force_relinquish`
//! path end-to-end. This suite closes that gap.

#![allow(clippy::doc_lazy_continuation)]

use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey};
use octo_ident::ChainId;
use octo_sync::substrate::{
    governance_signature_message, Cluster, GovernanceAttestation, InMemoryWal, NonceTracker,
    OperatorId, OperatorSet, OperatorSignature, RaftLikeWriterElection, ShardKey, WriterElection,
    WriterElectionError, WriterElectionForceRelinquish, WriterNodeId,
};

const CIPHEROCTO_TEST_CHAIN_ID: &str = "cipherocto-test";
const ATTACKER_CHAIN_ID: &str = "partner-mainnet";

/// Deterministic ed25519 seed: SHA-512-equivalent via BLAKE3 keyed hash.
fn seed_for(byte: u8) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"cipherocto/test-seed/v1");
    h.update(&[byte]);
    let out = h.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&out.as_bytes()[..32]);
    bytes
}

/// 3-operator fixture: 3 ed25519 signing keys + 3 operator IDs +
/// 2-of-3 threshold `OperatorSet`. Operators are sorted lex by
/// `OperatorId(pubkey)` per `OperatorSet::new`.
fn operator_fixture() -> (Vec<SigningKey>, OperatorSet, ChainId) {
    let sk1 = SigningKey::from_bytes(&seed_for(0xA1));
    let sk2 = SigningKey::from_bytes(&seed_for(0xA2));
    let sk3 = SigningKey::from_bytes(&seed_for(0xA3));
    let ops = vec![
        OperatorId(sk1.verifying_key().to_bytes()),
        OperatorId(sk2.verifying_key().to_bytes()),
        OperatorId(sk3.verifying_key().to_bytes()),
    ];
    let os = OperatorSet::new(ops, 2).expect("threshold 2 ≤ 3 operators");
    let chain_id = ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal");
    (vec![sk1, sk2, sk3], os, chain_id)
}

/// Build a 2-of-3 `GovernanceAttestation` signed by operators
/// `signer_indices` over the supplied `nonce` + `term`.
fn build_attestation(
    signers: &[SigningKey],
    operator_set: &OperatorSet,
    shard_key: ShardKey,
    chain_id: &ChainId,
    term: u64,
    nonce: [u8; 32],
    signer_indices: &[usize],
) -> GovernanceAttestation {
    let message = governance_signature_message(&shard_key, chain_id, term, &nonce);
    let signatures = signer_indices
        .iter()
        .map(|&i| OperatorSignature {
            operator_id: OperatorId(signers[i].verifying_key().to_bytes()),
            signature: signers[i].sign(&message).to_bytes(),
        })
        .collect();
    GovernanceAttestation {
        shard_key,
        chain_id: chain_id.clone(),
        term,
        operators: operator_set.operators.clone(),
        signatures,
        threshold: operator_set.threshold,
        nonce,
    }
}

/// Acquire a leader on `shard_key` for the given writer.
async fn acquire_leader(cluster: Arc<Cluster>, writer_id: WriterNodeId, shard_key: ShardKey) {
    let writer = RaftLikeWriterElection::new(
        writer_id,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );
    writer.acquire_writer(&shard_key, 1_000).await.unwrap();
}

/// Wire a `NonceTracker` over an `InMemoryWal` backed by `cluster`.
fn tracker(cluster: Arc<Cluster>) -> NonceTracker {
    let wal = Arc::new(InMemoryWal::new(cluster.clone()));
    NonceTracker::new(wal)
}

/// TV-1 two_of_three_force_relinquish_clears_lease — happy path.
/// A acquires the lease; 2-of-3 operator attestation force-relinquishes;
/// `current_leader` returns None.
#[tokio::test]
async fn tv1_two_of_three_force_relinquish_clears_lease() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_a
    );

    let (signers, os, chain_id) = operator_fixture();
    let nonce = [0x42u8; 32];
    let att = build_attestation(&signers, &os, shard_key, &chain_id, 1, nonce, &[0, 1]);

    let tracker = tracker(cluster.clone());
    let election = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r = election
        .force_relinquish_writer(&shard_key, &att, &os, &tracker)
        .await;
    assert!(r.is_ok(), "happy-path 2-of-3 must succeed: {r:?}");

    // Lease is gone.
    assert!(cluster.current_leader(shard_key).is_none());
    // NonceTracker consumed exactly 1 nonce (visible via WAL nonce records).
    assert_eq!(cluster.scan_nonce_records().len(), 1);
}

/// TV-2 wrong_chain_id_rejected — attestation's chain_id != configured
/// chain_id → `ChainIdMismatch`. Lease stays.
#[tokio::test]
async fn tv2_wrong_chain_id_rejected() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;

    let (signers, os, _configured_chain_id) = operator_fixture();
    let wrong_chain_id = ChainId::new(ATTACKER_CHAIN_ID).expect("static literal");
    let nonce = [0x43u8; 32];
    let att = build_attestation(&signers, &os, shard_key, &wrong_chain_id, 1, nonce, &[0, 1]);

    let tracker = tracker(cluster.clone());
    let election = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r = election
        .force_relinquish_writer(&shard_key, &att, &os, &tracker)
        .await;
    assert!(matches!(r, Err(WriterElectionError::ChainIdMismatch)));

    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_a
    );
    assert_eq!(cluster.scan_nonce_records().len(), 0);
}

/// TV-3 replayed_nonce_rejected — first call succeeds; second call with
/// the same nonce → `NonceReplayed`.
#[tokio::test]
async fn tv3_replayed_nonce_rejected() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    let writer_b = WriterNodeId([2u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;

    let (signers, os, chain_id) = operator_fixture();
    let nonce = [0x44u8; 32];
    let att1 = build_attestation(&signers, &os, shard_key, &chain_id, 1, nonce, &[0, 1]);
    let att2 = build_attestation(&signers, &os, shard_key, &chain_id, 1, nonce, &[1, 2]);

    let tracker = tracker(cluster.clone());
    let election_a = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r1 = election_a
        .force_relinquish_writer(&shard_key, &att1, &os, &tracker)
        .await;
    assert!(r1.is_ok(), "first call must succeed: {r1:?}");
    assert!(cluster.current_leader(shard_key).is_none());

    acquire_leader(cluster.clone(), writer_b, shard_key).await;
    let election_b = RaftLikeWriterElection::new(
        writer_b,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );
    let r2 = election_b
        .force_relinquish_writer(&shard_key, &att2, &os, &tracker)
        .await;
    assert!(matches!(r2, Err(WriterElectionError::NonceReplayed)));

    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_b
    );
    assert_eq!(cluster.scan_nonce_records().len(), 1);
}

/// TV-4 unauthorized_signer_rejected — signature from an operator NOT
/// in the configured operator set → `UnauthorizedSigner`.
#[tokio::test]
async fn tv4_unauthorized_signer_rejected() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;

    let (signers, os, chain_id) = operator_fixture();
    let outsider_sk = SigningKey::from_bytes(&seed_for(0xF9));

    let nonce = [0x45u8; 32];
    let message = governance_signature_message(&shard_key, &chain_id, 1, &nonce);
    let outsider_sig = outsider_sk.sign(&message);
    let valid_sig = signers[0].sign(&message);
    let att = GovernanceAttestation {
        shard_key,
        chain_id: chain_id.clone(),
        term: 1,
        operators: os.operators.clone(),
        signatures: vec![
            OperatorSignature {
                operator_id: OperatorId(signers[0].verifying_key().to_bytes()),
                signature: valid_sig.to_bytes(),
            },
            OperatorSignature {
                operator_id: OperatorId(outsider_sk.verifying_key().to_bytes()),
                signature: outsider_sig.to_bytes(),
            },
        ],
        threshold: 2,
        nonce,
    };

    let tracker = tracker(cluster.clone());
    let election = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r = election
        .force_relinquish_writer(&shard_key, &att, &os, &tracker)
        .await;
    assert!(matches!(r, Err(WriterElectionError::UnauthorizedSigner)));

    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_a
    );
}

/// TV-5 below_threshold_rejected — only 1 valid signature (threshold=2)
/// → `InsufficientSignatures`.
#[tokio::test]
async fn tv5_below_threshold_rejected() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;

    let (signers, os, chain_id) = operator_fixture();
    let nonce = [0x46u8; 32];
    let att = build_attestation(&signers, &os, shard_key, &chain_id, 1, nonce, &[0]);

    let tracker = tracker(cluster.clone());
    let election = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r = election
        .force_relinquish_writer(&shard_key, &att, &os, &tracker)
        .await;
    assert!(matches!(
        r,
        Err(WriterElectionError::InsufficientSignatures)
    ));
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_a
    );
}

/// TV-6 invalid_signature_rejected — 1 valid + 1 tampered signature
/// (still attached to a valid operator) → `InvalidSignature`.
#[tokio::test]
async fn tv6_invalid_signature_rejected() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;

    let (signers, os, chain_id) = operator_fixture();
    let nonce = [0x47u8; 32];
    let message = governance_signature_message(&shard_key, &chain_id, 1, &nonce);
    let valid_sig = signers[0].sign(&message).to_bytes();
    let mut tampered_sig = valid_sig;
    tampered_sig[0] ^= 0xFF;
    let att = GovernanceAttestation {
        shard_key,
        chain_id: chain_id.clone(),
        term: 1,
        operators: os.operators.clone(),
        signatures: vec![
            OperatorSignature {
                operator_id: OperatorId(signers[0].verifying_key().to_bytes()),
                signature: valid_sig,
            },
            OperatorSignature {
                operator_id: OperatorId(signers[1].verifying_key().to_bytes()),
                signature: tampered_sig,
            },
        ],
        threshold: 2,
        nonce,
    };

    let tracker = tracker(cluster.clone());
    let election = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r = election
        .force_relinquish_writer(&shard_key, &att, &os, &tracker)
        .await;
    assert!(matches!(r, Err(WriterElectionError::InvalidSignature)));
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_a
    );
}

/// TV-7 duplicate_signer_rejected — same operator signs twice
/// → `DuplicateSigner`.
#[tokio::test]
async fn tv7_duplicate_signer_rejected() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;

    let (signers, os, chain_id) = operator_fixture();
    let nonce = [0x48u8; 32];
    let message = governance_signature_message(&shard_key, &chain_id, 1, &nonce);
    let sig_a = signers[0].sign(&message);
    let att = GovernanceAttestation {
        shard_key,
        chain_id: chain_id.clone(),
        term: 1,
        operators: os.operators.clone(),
        signatures: vec![
            OperatorSignature {
                operator_id: OperatorId(signers[0].verifying_key().to_bytes()),
                signature: sig_a.to_bytes(),
            },
            OperatorSignature {
                operator_id: OperatorId(signers[0].verifying_key().to_bytes()),
                signature: sig_a.to_bytes(),
            },
        ],
        threshold: 2,
        nonce,
    };

    let tracker = tracker(cluster.clone());
    let election = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r = election
        .force_relinquish_writer(&shard_key, &att, &os, &tracker)
        .await;
    assert!(matches!(r, Err(WriterElectionError::DuplicateSigner)));
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_a
    );
}

/// TV-8 shard_key_mismatch_rejected — attestation shard_key != caller's
/// shard_key → `ShardKeyMismatch`.
#[tokio::test]
async fn tv8_shard_key_mismatch_rejected() {
    let cluster = Cluster::new();
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = WriterNodeId([1u8; 32]);
    acquire_leader(cluster.clone(), writer_a, shard_key).await;

    let (signers, os, chain_id) = operator_fixture();
    let nonce = [0x49u8; 32];
    let wrong_shard = ShardKey([0xCCu8; 32]);
    let att = build_attestation(&signers, &os, wrong_shard, &chain_id, 1, nonce, &[0, 1]);

    let tracker = tracker(cluster.clone());
    let election = RaftLikeWriterElection::new(
        writer_a,
        cluster.clone(),
        ChainId::new(CIPHEROCTO_TEST_CHAIN_ID).expect("static literal"),
    );

    let r = election
        .force_relinquish_writer(&shard_key, &att, &os, &tracker)
        .await;
    assert!(matches!(r, Err(WriterElectionError::ShardKeyMismatch)));
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        writer_a
    );
}
