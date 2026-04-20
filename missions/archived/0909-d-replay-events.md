# Mission: RFC-0909 replay_events — Deterministic Spend Aggregation

## Status

Completed (v4)

## Claimant

@mmacedoeu

## Pull Request

#

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `replay_events()` — reconstructs per-key spend aggregates from an ordered slice of SpendEvents. Uses BTreeMap for deterministic key ordering and `event_id`-only sort (SpendEvent has no `created_at` field). NOT for Merkle proof generation (see build_merkle_tree).

## Acceptance Criteria

- [ ] `replay_events(events: &[SpendEvent]) -> BTreeMap<String, u64>` — returns key_id.to_string() → total spend
- [ ] Sorts events by event_id (hex string, ascending) for deterministic ordering
- [ ] Uses `BTreeMap<String, u64>` for deterministic iteration order
- [ ] Uses `saturating_add` for accumulation (overflow requires >1.8×10¹⁹ micro-units — effectively impossible)
- [ ] Returns per-key aggregate spend suitable for audit, historical reconciliation, and budget state verification. NOT for live quota enforcement (use `record_spend` for that) (H1)
- [ ] Does NOT generate Merkle proofs (see Mission 0909-e build_merkle_tree)
- [ ] `SpendEvent` struct fields: `event_id: String`, `request_id: String`, `key_id: uuid::Uuid`, `team_id: Option<uuid::Uuid>`, `provider: String`, `model: String`, `input_tokens: u32`, `output_tokens: u32`, `cost_amount: u64`, `pricing_hash: [u8; 32]`, `token_source: TokenSource`, `tokenizer_id: Option<[u8; 16]>` (per RFC-0903-B1 §SpendEvent)

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `replay.rs` module
- Sort: `sorted_events.sort_by(|a, b| a.event_id.cmp(&b.event_id))`
- Aggregation: `entry.saturating_add(event.cost_amount)`
- `key_id.to_string()` creates String from Uuid (allocates — unavoidable given hyphenated UUID format)
- Note: In-memory replay uses event_id-only ordering. DB-level replay uses `ORDER BY created_at ASC, event_id ASC` (created_at is schema-only, not in struct)
- **Sort vs SUM distinction (H1):** The sort is required for deterministic replay/audit ordering, NOT because the math requires it. Per RFC-0909 §Budget Computation Procedure: "No ORDER BY is needed for SUM — aggregation is order-independent." Two consumers exist: (1) aggregate budget computation (order-independent), (2) deterministic replay/audit (requires event_id ordering). This function serves both by always sorting.
- **saturating_add vs checked_add distinction (H2):** `saturating_add` is used here for in-memory replay/audit — overflow saturates (best-effort audit). Live quota enforcement via `record_spend` uses `checked_add` which returns `Err` on overflow. These are intentionally different behaviors: overflow in live enforcement means budget exceeded (hard error), while overflow in replay means data corruption or under attack (saturates silently). See RFC-0909 §Overflow Safety. `record_spend` (for live quota enforcement) is defined in RFC-0903 Final §record_spend — this mission does not implement it (H1).
- **Edge cases (M2):** Empty events slice returns empty BTreeMap. Duplicate key_id entries across events are summed via `saturating_add`.

## Reference

- RFC-0909 §replay_events
- RFC-0909 §Budget Computation Procedure
- RFC-0909 §Event Ordering (canonical path: event_id ASC)
- RFC-0903-B1 §SpendEvent (struct definition with all fields)
- RFC-0909 §Overflow Safety (checked_add vs saturating_add distinction)

## Dependencies

- `uuid = "1.x"` for uuid::Uuid

## Complexity

Low — BTreeMap aggregation with deterministic sort

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v4 | 2026-04-20 | Implemented: replay_events function + 6 unit tests, all 7 AC items complete |
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix H1 (add record_spend cross-reference to RFC-0903 Final §record_spend); fix M2 (move Dependencies to own section after Reference for consistency) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (add full SpendEvent struct fields to AC); fix H1 (clarify sort is for audit/replay not math; add Budget Computation Procedure distinction); fix H2 (document saturating_add vs checked_add live/audit distinction); fix M1 (return type description clarifies NOT for live quota enforcement); fix M2 (add empty events edge case note); fix L1 (add uuid crate dependency); fix L2 (add RFC-0903-B1 §SpendEvent to references); fix L3 (Priority High → Critical) |
