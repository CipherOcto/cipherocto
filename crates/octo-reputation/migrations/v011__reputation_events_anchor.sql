-- v011__reputation_events_anchor — adds `anchor_tx_hash` column to
-- `reputation_events` (mission 0968a AC).
--
-- Pre-v011 schema did not track per-event anchor provenance. The
-- anchor job (anchor_job.rs:run_once) needs to:
--   1. Find events with `anchor_tx_hash IS NULL` (anchor_pending sweep)
--   2. UPDATE the row with the on-chain anchor tx hash post-submit
--      (set_event_anchor_tx_hash)
--   3. JOIN on `controller_id + anchor_tx_hash IS NOT NULL` for the
--      read-side provenance query (query_anchors_by_controller_id)
--
-- The v010 migration added the `reputation_anchors` table for the
-- cross-controller anchor ledger; v011 extends `reputation_events`
-- itself with the per-event nullable BLOB column that the anchor
-- job operates on.
--
-- Schema:
--   anchor_tx_hash BLOB NULL — populated by `set_event_anchor_tx_hash`
--     after the anchor job submits the on-chain batch containing the
--     event. `None` (NULL) means the event has not been anchored yet.
--     Length: 32 bytes when populated (BLAKE3 anchor tx hash digest).
ALTER TABLE reputation_events ADD COLUMN anchor_tx_hash BLOB NULL;

-- Index on (controller_id, anchor_tx_hash) for the read-side JOIN
-- (`query_anchors_by_controller_id` filters by controller_id and
-- `anchor_tx_hash IS NOT NULL`).
CREATE INDEX IF NOT EXISTS idx_reputation_events_controller_anchor
    ON reputation_events(controller_id, anchor_tx_hash);
