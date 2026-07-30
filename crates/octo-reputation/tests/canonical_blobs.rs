//! Canonical anchor blob test vectors (RFC-0968-A1 §28 line 3814 +
//! RFC-0955-R1 §"ReputationAnchorBatch").
//!
//! These pinned vectors anchor the deterministic 32-byte BLAKE3
//! commitments for the canonical ReputationAnchorBatch envelope. Two
//! independent implementations (octo-reputation's anchor.rs + an
//! external verifier) MUST produce byte-identical output for every
//! vector in this file. Any drift breaks federation interoperability.
//!
//! ## Vector construction
//!
//! `CANONICAL_ANCHOR_BLOB[i] = BLAKE3(BLAKE3_REPUTATION_ANCHOR_DOMAIN
//! || controller_id_be || window_index_be || leaf_count_be ||
//! leaves[i].canonical_bytes())`
//!
//! Domain separator `BLAKE3_REPUTATION_ANCHOR_DOMAIN =
//! b"cipherocto/reputation/anchor/v1"` is declared at
//! `crates/octo-reputation/src/constants.rs:181`.
//!
//! ## Why hand-pinned + not generated
//!
//! Pinned vectors guard against accidental drift in the canonical
//! serialisation (domain string, field order, byte width, big-endian
//! integer encoding). Regenerating on every test run would defeat
//! the purpose — the vectors themselves become the cross-replica
//! contract.

use octo_reputation::anchor::{AnchorLeaf, AnchorWindow, ReputationAnchorBatch};
use octo_reputation::auth::{AnchorGovernanceProof, AnchorGovernanceSnapshot};
use octo_reputation::constants::BLAKE3_REPUTATION_ANCHOR_DOMAIN;
use octo_reputation::types::{RecorderDid, ReputationLayer, SignalKind};

/// Pinned canonical blob for vector #1: empty leaves, controller
/// `[0u8; 32]`, window_index = 0 (now_unix = 0).
///
/// RE-PINNED 2026-07-30 (mission 0968a2 AC #17): the previous bytes
/// reflected the old `AnchorLeaf::digest` field order
/// (`score_ewma_raw` last) which broke cross-implementation
/// interoperability per RFC-0955-R1 lines 420-422. The new bytes
/// reflect the canonical order `(did, signal_kind, layer,
/// last_event_id, score_ewma_raw, last_event_unix, samples,
/// severity_total)` PLUS the new `ReputationAnchorBatch` envelope
/// fields (`governance_snapshot`, `governance_proof`,
/// `governance_set_hash`, `batch_size`) added per RFC-0955-R1 lines
/// 177-200. An independent Python reimplementation using
/// `hashlib.blake3` MUST reproduce these bytes byte-identically.
pub const CANONICAL_ANCHOR_BLOB_0_LEAVES: [u8; 32] = [
    0x49, 0xb6, 0x4e, 0xc0, 0x26, 0xf9, 0xef, 0x4a, 0xc5, 0x14, 0x10, 0x70, 0x71, 0xdd, 0x9b, 0x86,
    0x22, 0x70, 0x92, 0x19, 0xb9, 0xc0, 0x65, 0x3e, 0x1b, 0x92, 0x47, 0xa3, 0x2e, 0x11, 0x14, 0xae,
];

/// Pinned canonical blob for vector #2: single leaf, controller
/// `[1u8; 32]`, window_index = 1_000_000 / 300 = 3333.
/// RE-PINNED 2026-07-30 — see note on `CANONICAL_ANCHOR_BLOB_0_LEAVES`.
pub const CANONICAL_ANCHOR_BLOB_1_LEAF: [u8; 32] = [
    0x27, 0xe0, 0xb6, 0x56, 0x11, 0x4e, 0x5c, 0xaf, 0x20, 0xa6, 0x33, 0x0b, 0x63, 0x43, 0x07, 0x9d,
    0x51, 0xc6, 0x59, 0xa8, 0x09, 0x3f, 0xf5, 0x39, 0x5e, 0x79, 0x7b, 0x65, 0x77, 0xfc, 0x2c, 0x90,
];

/// Pinned canonical blob for vector #3: 100 leaves (per-root cap),
/// controller `[0xAB; 32]`, window_index = 1_700_000_000 / 300 = 5_666_666.
/// RE-PINNED 2026-07-30 — see note on `CANONICAL_ANCHOR_BLOB_0_LEAVES`.
pub const CANONICAL_ANCHOR_BLOB_100_LEAVES: [u8; 32] = [
    0x55, 0x99, 0x1f, 0x70, 0x3b, 0x3d, 0x3a, 0x9d, 0xcf, 0xa9, 0xb7, 0x9b, 0x46, 0x8b, 0xc9, 0x9a,
    0xf0, 0xa5, 0x1c, 0xf8, 0xe2, 0x68, 0x33, 0xe0, 0x83, 0x3e, 0x0a, 0xa4, 0xf8, 0x09, 0x09, 0x20,
];

/// Convenience alias for the AC-specified constant name.
pub const CANONICAL_ANCHOR_BLOB: [[u8; 32]; 3] = [
    CANONICAL_ANCHOR_BLOB_0_LEAVES,
    CANONICAL_ANCHOR_BLOB_1_LEAF,
    CANONICAL_ANCHOR_BLOB_100_LEAVES,
];

fn build_test_leaf(seed: u8) -> AnchorLeaf {
    AnchorLeaf::from_aggregate(&octo_reputation::types::ReputationAggregate {
        recorder_did: RecorderDid::from_array([seed; 52]),
        signal_kind: SignalKind::Outcome,
        layer: ReputationLayer::Market,
        score_ewma: octo_determin::Dfp::from_f64(0.5 + (seed as f64) * 0.001),
        samples: 100 + seed as u64,
        severity_total: 0,
        last_event_id: octo_reputation::types::EventId::from_u64(seed as u64),
        last_event_unix: 1_700_000_000 + seed as u64,
        updated_at_unix: 1_700_000_000 + seed as u64,
    })
}

/// Default empty governance snapshot — block 0, epoch 0, ts 0.
fn empty_snapshot() -> AnchorGovernanceSnapshot {
    AnchorGovernanceSnapshot {
        block_height: 0,
        epoch: 0,
        finalized_at_unix: 0,
    }
}

/// Default empty governance proof — zero signers (test-only; in
/// production `meets_quorum()` requires `GOVERNANCE_QUORUM = 3`).
fn empty_proof() -> AnchorGovernanceProof {
    AnchorGovernanceProof { signers: vec![] }
}

fn assert_batch_digest_matches(batch: &ReputationAnchorBatch, expected: [u8; 32]) {
    let digest = batch.digest();
    let actual: [u8; 32] = digest.0;
    assert_eq!(
        actual, expected,
        "canonical anchor blob diverged from pinned vector (controller_id = {:?}, window_index = {})",
        batch.controller_id, batch.window.window_index
    );
}

#[test]
fn canonical_blob_zero_leaves_is_pinned() {
    let batch = ReputationAnchorBatch {
        controller_id: [0u8; 32],
        window: AnchorWindow::at(0),
        chain_block_height: Some(0),
        rotation_receipt_id: None,
        governance_snapshot: empty_snapshot(),
        governance_proof: empty_proof(),
        governance_set_hash: [0u8; 32],
        batch_size: 0,
        leaves: vec![],
    };
    assert_batch_digest_matches(&batch, CANONICAL_ANCHOR_BLOB_0_LEAVES);
}

#[test]
fn canonical_blob_single_leaf_is_pinned() {
    let batch = ReputationAnchorBatch {
        controller_id: [1u8; 32],
        window: AnchorWindow::at(1_000_000),
        chain_block_height: Some(12),
        rotation_receipt_id: None,
        governance_snapshot: empty_snapshot(),
        governance_proof: empty_proof(),
        governance_set_hash: [0u8; 32],
        batch_size: 1,
        leaves: vec![build_test_leaf(1)],
    };
    assert_batch_digest_matches(&batch, CANONICAL_ANCHOR_BLOB_1_LEAF);
}

#[test]
fn canonical_blob_hundred_leaves_is_pinned() {
    let leaves: Vec<_> = (0..100u8).map(build_test_leaf).collect();
    let batch = ReputationAnchorBatch {
        controller_id: [0xAB; 32],
        window: AnchorWindow::at(1_700_000_000),
        chain_block_height: Some(100),
        rotation_receipt_id: None,
        governance_snapshot: empty_snapshot(),
        governance_proof: empty_proof(),
        governance_set_hash: [0u8; 32],
        batch_size: leaves.len() as u32,
        leaves,
    };
    assert_batch_digest_matches(&batch, CANONICAL_ANCHOR_BLOB_100_LEAVES);
}

#[test]
fn canonical_blob_digest_domain_separator_is_stable() {
    // The domain separator `b"cipherocto/reputation/anchor/v1"` is
    // part of the cross-replica contract. Pin the bytes to detect
    // accidental edits.
    assert_eq!(
        BLAKE3_REPUTATION_ANCHOR_DOMAIN,
        b"cipherocto/reputation/anchor/v1"
    );
}

#[test]
fn canonical_blob_two_independent_computations_are_byte_identical() {
    // Property: BLAKE3 is deterministic — two calls with the same
    // input MUST produce identical output. This is the
    // cross-implementation contract per RFC-0955-R1.
    let batch = ReputationAnchorBatch {
        controller_id: [0xCD; 32],
        window: AnchorWindow::at(2_000_000),
        chain_block_height: Some(24),
        rotation_receipt_id: Some([0xEE; 32]),
        governance_snapshot: empty_snapshot(),
        governance_proof: empty_proof(),
        governance_set_hash: [0u8; 32],
        batch_size: 2,
        leaves: vec![build_test_leaf(7), build_test_leaf(13)],
    };
    let d1 = batch.digest();
    let d2 = batch.digest();
    assert_eq!(d1.0, d2.0, "BLAKE3 digest must be deterministic");
}
