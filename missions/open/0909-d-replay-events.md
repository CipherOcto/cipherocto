# Mission: RFC-0909 replay_events — Deterministic Spend Aggregation

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `replay_events()` — reconstructs per-key spend aggregates from an ordered slice of SpendEvents. Uses BTreeMap for deterministic key ordering and `event_id`-only sort (SpendEvent has no `created_at` field). NOT for Merkle proof generation (see build_merkle_tree).

## Acceptance Criteria

- [ ] `replay_events(events: &[SpendEvent]) -> BTreeMap<String, u64>` — returns key_id.to_string() → total spend
- [ ] Sorts events by event_id (hex string, ascending) for deterministic ordering
- [ ] Uses `BTreeMap<String, u64>` for deterministic iteration order
- [ ] Uses `saturating_add` for accumulation (overflow requires >1.8×10¹⁹ micro-units — effectively impossible)
- [ ] Returns per-key aggregate spend suitable for quota enforcement and budget checks
- [ ] Does NOT generate Merkle proofs (see Mission 0909-e build_merkle_tree)

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `replay.rs` module
- `SpendEvent` struct has fields: `event_id: String`, `key_id: uuid::Uuid`, `cost_amount: u64`
- Sort: `sorted_events.sort_by(|a, b| a.event_id.cmp(&b.event_id))`
- Aggregation: `entry.saturating_add(event.cost_amount)`
- `key_id.to_string()` creates String from Uuid (allocates — unavoidable given hyphenated UUID format)
- Note: In-memory replay uses event_id-only ordering. DB-level replay uses `ORDER BY created_at ASC, event_id ASC` (created_at is schema-only, not in struct)

## Reference

- RFC-0909 §replay_events
- RFC-0909 §Budget Computation Procedure
- RFC-0909 §Event Ordering (canonical path: event_id ASC)

## Complexity

Low — BTreeMap aggregation with deterministic sort

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0909 Phase 1 Core
