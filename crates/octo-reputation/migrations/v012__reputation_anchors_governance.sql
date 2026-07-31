-- v012__reputation_anchors_governance — mission 0968a2 AC #6.
--
-- Extends the `reputation_anchors` ledger (created in v010) with the
-- 3 governance fields mandated by RFC-0955-R1 §"ReputationAnchorBatch"
-- lines 177-200 + §"Governance Snapshot Binding" lines 250-266.
--
-- Schema (per-controller Merkle-root anchor model, post-amendment-48):
--   governance_snapshot BLOB — postcard-encoded AnchorGovernanceSnapshot
--     (block_height: u64 || epoch: u64 || finalized_at_unix: u64 =
--     24 bytes canonical). NULL allowed pre-binding; the runtime
--     populates it before submitting the anchor.
--   governance_proof BLOB — postcard-encoded AnchorGovernanceProof
--     (signers: Vec<AnchorGovernanceSigner> where each signer is
--     pubkey: [u8;32] || signature: [u8;64]). Variable length;
--     canonical length = GOVERNANCE_QUORUM * 96 = 288 bytes for a
--     well-formed 3-of-3 proof.
--   governance_set_hash BLOB — 32-byte BLAKE3 hash of the governance
--     set under the BLAKE3_GOVERNANCE_SET_DOMAIN. RFC-0955-R1 lines
--     177-200 + RFC-0968 §10.
--
-- Idempotency: the SQL itself is NOT idempotent — `ALTER TABLE ADD
-- COLUMN` will fail with "duplicate column name" on re-execution.
-- The migration runner at `crates/octo-reputation/src/migrations.rs`
-- enforces idempotency at the application boundary: it queries
-- `schema_migrations` for the version name before invoking this
-- file's body (see `apply()` lines 100-115 in migrations.rs), so
-- production open() paths never re-run v012. The runner-level guard
-- is the contract; do NOT bypass the runner (operational SQL replays
-- outside the runner path will fail loudly with "duplicate column
-- name" — that failure is the desired alarm signal).
--
-- Constraints:
--   All 3 columns are nullable — pre-submission rows may have NULL
--   governance fields. The runtime populates them before the anchor
--   reaches `MIN_FINALITY_BLOCKS` depth; the verification gate
--   (`governance_proof.meets_quorum()`) rejects rows with missing or
--   non-3-of-3 proofs.

ALTER TABLE reputation_anchors ADD COLUMN governance_snapshot BLOB;
ALTER TABLE reputation_anchors ADD COLUMN governance_proof BLOB;
ALTER TABLE reputation_anchors ADD COLUMN governance_set_hash BLOB;

-- Query support: lookup anchors by governance set hash for the
-- cross-replica consistency check. Plain index (no WHERE clause) —
-- stoolap-fork does not support partial indexes.
CREATE INDEX IF NOT EXISTS idx_reputation_anchors_governance_set_hash
    ON reputation_anchors(governance_set_hash);