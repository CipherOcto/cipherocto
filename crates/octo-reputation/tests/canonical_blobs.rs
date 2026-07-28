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
use octo_reputation::constants::BLAKE3_REPUTATION_ANCHOR_DOMAIN;
use octo_reputation::types::{RecorderDid, ReputationLayer, SignalKind};

/// Pinned canonical blob for vector #1: empty leaves, controller
/// `[0u8; 32]`, window_index = 0 (now_unix = 0).
pub const CANONICAL_ANCHOR_BLOB_0_LEAVES: [u8; 32] = [
    0x37, 0x96, 0xb8, 0x3f, 0x5b, 0xe1, 0x5c, 0xfb, 0x36, 0xad, 0xec, 0x1a, 0x91, 0x98, 0x02, 0x26,
    0x8b, 0xf4, 0x5a, 0x7b, 0xdc, 0xb6, 0x95, 0x7a, 0xf2, 0x03, 0x0d, 0xe7, 0x72, 0x26, 0xf7, 0xad,
];

/// Pinned canonical blob for vector #2: single leaf, controller
/// `[1u8; 32]`, window_index = 1_000_000 / 300 = 3333.
pub const CANONICAL_ANCHOR_BLOB_1_LEAF: [u8; 32] = [
    0x19, 0xba, 0x10, 0xd5, 0x6c, 0x14, 0x39, 0x78, 0xb1, 0x85, 0x46, 0x0a, 0x80, 0x22, 0xc4, 0x4f,
    0x33, 0x3d, 0x8d, 0x14, 0x6e, 0xab, 0xc5, 0x96, 0xf6, 0x4b, 0xa4, 0x04, 0x38, 0x11, 0x0c, 0x51,
];

/// Pinned canonical blob for vector #3: 100 leaves (per-root cap),
/// controller `[0xAB; 32]`, window_index = 1_700_000_000 / 300 = 5_666_666.
pub const CANONICAL_ANCHOR_BLOB_100_LEAVES: [u8; 32] = [
    0xd1, 0x05, 0xcb, 0xdb, 0x71, 0xce, 0xe2, 0xe7, 0xba, 0xfd, 0x7a, 0x4d, 0xf4, 0x14, 0x23, 0xb3,
    0xcf, 0x63, 0xc7, 0xef, 0x6c, 0x81, 0xae, 0xe1, 0x26, 0x2e, 0xf6, 0x4f, 0x3f, 0x94, 0x5c, 0x40,
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
        chain_block_height: 0,
        rotation_receipt_id: None,
        leaves: vec![],
    };
    assert_batch_digest_matches(&batch, CANONICAL_ANCHOR_BLOB_0_LEAVES);
}

#[test]
fn canonical_blob_single_leaf_is_pinned() {
    let batch = ReputationAnchorBatch {
        controller_id: [1u8; 32],
        window: AnchorWindow::at(1_000_000),
        chain_block_height: 12,
        rotation_receipt_id: None,
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
        chain_block_height: 100,
        rotation_receipt_id: None,
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
        chain_block_height: 24,
        rotation_receipt_id: Some([0xEE; 32]),
        leaves: vec![build_test_leaf(7), build_test_leaf(13)],
    };
    let d1 = batch.digest();
    let d2 = batch.digest();
    assert_eq!(d1.0, d2.0, "BLAKE3 digest must be deterministic");
}
