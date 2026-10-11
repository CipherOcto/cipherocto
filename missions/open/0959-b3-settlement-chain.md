---
name: 0959-b3-settlement-chain
description: Owns append_deal_settled, the SettlementChainEvent::DealSettled variant, the chain-hash continuity rule, and RFC-0959-A1 TV8/TV11. Deferred to by mission 0959-b in 2026-08-06 and never opened, while 0959-a1 records those vectors as GREEN.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959-A1
    - 0959-a1-market-delivery
    - 0959-b2-deliver-algorithm
---

# Mission `0959-b3-settlement-chain`

## Status

Open. Created 2026-10-12 to give this work an owner; it was deferred to by
`0959-b` on 2026-08-06 and the deferral target was never opened.

## Why this mission exists

`0959-b` closed Band A on 2026-08-06 and deferred its settlement-chain phase to
this mission. It was never created, while the parent roll-up in `0959-a1`
records the vectors it owns as GREEN.

`0959-b` also recorded a correction to its own task text that is worth carrying
forward rather than re-discovering: the chain work belongs in
`crates/quota-router-storage/src/ask.rs`, **not**
`crates/quota-router-core/src/settlement/chain.rs` as the original mission text
claimed — that path does not exist on disk. The settlement chain lives in
`ask.rs`, the consolidated store, after `0959-a` landed.

## Vectors owned here

- **TV8 — Chain Hash Continuity.** `chain_tip_hash` after 100 sequential
  `append_deal_settled` calls matches the expected hash.
- **TV11 — Idempotency via UNIQUE.** A duplicate `DealSettled.ask_id` insert
  returns an idempotency error, backed by a UNIQUE constraint on `ask_id`.

## Substrate this extends

- `SettlementChainEvent::DealSettled` — the fourth chained artifact joining
  `Ask`, `SettlementEvent` and `SettlementReceipt`
- an `append_settlement_event` overload
- the chain-hash continuity rule that TV8 measures

## Acceptance Criteria

- [ ] `SettlementChainEvent::DealSettled` exists and participates in the chain.
- [ ] `append_deal_settled` updates the chain hash in `crates/quota-router-storage/src/ask.rs`, not the non-existent path the original text named.
- [ ] `append_settlement_event` has the overload the new variant needs.
- [ ] TV8 resolves to a test that performs 100 sequential appends and asserts the tip hash.
- [ ] TV11 resolves to a test backed by an actual UNIQUE constraint on `ask_id`, not by a read-then-write check that races.
- [ ] `0959-a1`'s roll-up is amended to stop claiming TV8 and TV11 as GREEN before they are.

## What must not happen

Do not satisfy TV11 with an application-level existence check. The vector names
**UNIQUE** because the guarantee has to survive concurrency: two callers checking
"does this ask_id exist?" and then inserting will both pass. The constraint is the
property.

Do not implement the chain extension in a new module and leave `ask.rs` as a
shim. `0959-b` already established that the chain lives there; a second home for
it is the parallel-abstraction failure the design principles prohibit.

Do not defer this a second time without opening the mission. That is precisely
what happened between 2026-08-06 and now, and it is why a roll-up one level up
can claim two vectors that no one has built.

## Dependencies

- Mission `0959-b-market-delivery-impl` — deferred this work explicitly
- Mission `0959-b2-deliver-algorithm` — produces the `DealSettled` this appends; TV1 and TV10 need both
- Mission `0959-a1` — holds the contested roll-up
- RFC-0959-A1 §Algorithms, §Test Vectors
