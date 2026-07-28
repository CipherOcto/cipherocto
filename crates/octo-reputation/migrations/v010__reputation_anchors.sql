-- v010__reputation_anchors — RFC-0968-A1 §28 catalog line 3814 + RFC-0955-R1 binding contract.
--
-- Migration slot v010 is allocated in RFC-0968 §28 catalog for the
-- per-controller Merkle-root anchor store (mission 0968a). The preceding
-- slots v006/v007/v008/v009 are reserved for recorder_registration and
-- kind_weights (per the same catalog) but not yet implemented; the slot
-- numbering here matches the canonical RFC catalog, not the implementation
-- order. This means the migration loader applies v010 after v005 with no
-- intervening versions; the gap is intentional and documented in the
-- migration runner.
--
-- Schema (post-amendment-48 per-controller model):
--   event_id            BLOB PRIMARY KEY — the canonical typed EventId
--                       (32-byte BLAKE3 anchor over the canonical event bytes).
--   anchor_tx_hash      BLOB NOT NULL    — the on-chain anchor transaction hash.
--   anchored_at_unix    INTEGER NOT NULL — wall-clock unix seconds when the
--                       anchor was observed.
--   controller_id       BLOB NOT NULL    — attested controller_id (default
--                       blake3(governance_pubkey)); one Merkle root per
--                       controller per ANCHOR_INTERVAL window.
--   anchor_root         BLOB NOT NULL    — the 32-byte Merkle root committed
--                       on-chain. Distinct from anchor_tx_hash.
--   leaf_count          INTEGER NOT NULL — number of (did, kind, layer)
--                       tuples aggregated under this root; bounded by
--                       MAX_TUPLES_PER_ROOT = 100 (RFC-0955-R1 §Constants).
--   rotation_receipt_id BLOB             — NULL for the standard
--                       pre-rotation anchor path; populated with the
--                       32-byte `consume_rotation_receipt` id when this
--                       anchor is a post-rotation resubmission bound to
--                       a specific finalized rotation (RFC-0955-R1
--                       §"ReputationAnchorBatch" post-Round-7 amendment
--                       51; persistence-10 AC).
--
-- Constraints:
--   PK on event_id makes the insert idempotent on a re-submission
--   (RFC-0955-R1 §"Chain-Level Idempotency").
--   The (controller_id, anchor_root) pair is the chain-side uniqueness key
--   for the Merkle root commitment; the rows in this table are the local
--   mirror used by the read side.
CREATE TABLE IF NOT EXISTS reputation_anchors (
    event_id BLOB PRIMARY KEY,
    anchor_tx_hash BLOB NOT NULL,
    anchored_at_unix INTEGER NOT NULL,
    controller_id BLOB NOT NULL,
    anchor_root BLOB NOT NULL,
    leaf_count INTEGER NOT NULL,
    rotation_receipt_id BLOB
);

-- Query support: lookup by controller_id (read-side join from
-- reputation_events when the anchor provenance is needed).
CREATE INDEX IF NOT EXISTS idx_reputation_anchors_controller
    ON reputation_anchors(controller_id);

-- Daily fanout check: count anchored leaves per controller per 24h window.
CREATE INDEX IF NOT EXISTS idx_reputation_anchors_controller_time
    ON reputation_anchors(controller_id, anchored_at_unix);

-- Chain-side uniqueness key: the (controller_id, anchor_root) pair is
-- the Merkle root commitment per RFC-0955-R1 §"Chain-Level Idempotency".
-- A UNIQUE index on this pair guarantees the rust-side idempotency
-- insert (controller_id + root) cannot double-row on a re-submission.
-- Without this, a network retry that re-runs `plan_batches` for the
-- same window could insert two rows with the same `(controller_id,
-- anchor_root)` tuple and break idempotency tracking.
CREATE UNIQUE INDEX IF NOT EXISTS idx_reputation_anchors_controller_root
    ON reputation_anchors(controller_id, anchor_root);

-- Rotation-receipt binding index (RFC-0955-R1 §"ReputationAnchorBatch"
-- post-Round-7 amendment 51). Per-snapshot lookups by rotation
-- receipt id surface post-rotation resubmissions for the DID-rotation
-- finality interaction.
CREATE INDEX IF NOT EXISTS idx_reputation_anchors_rotation_receipt
    ON reputation_anchors(rotation_receipt_id)
    WHERE rotation_receipt_id IS NOT NULL;
