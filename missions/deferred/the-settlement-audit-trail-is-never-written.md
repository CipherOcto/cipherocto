---
name: the-settlement-audit-trail-is-never-written
description: "SettlementEventRepository::insert has no production caller. The live settle-replay path writes only a replay-index row, so the settlement_events table stays empty in production and quota-router settle-list reports zero events. Measured on the built binary against a persistent DB: a settlement that verified successfully is absent from the audit trail. Four code locations also cite RFC-0959 §Event Sourcing, a section that does not exist in the RFC."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
---

# Mission `the-settlement-audit-trail-is-never-written`

## Status

Deferred — writing an audit record is a policy decision about what a
settlement is and when it is final, and the three findings it would need
to be able to catch are themselves unresolved. Four false RFC citations
are corrected here; no behaviour changed. Measurement probes removed,
tree matches its prior state.

This is the fourth finding on the live settlement-verification path, and
it is the one that removes the safety net for the other three.

## Finding

`SettlementEventRepository` has exactly one consumer in the tree, and it
is a reader:

```text
quota-router-cli/src/commands.rs   SettlementEventRepository::open(db_path)   <- settle_list
```

`SettlementEventRepository::insert` — the only write method — has **no
production caller**. `SettlementEventInsert` is constructed at exactly one
place in the whole workspace, and that place is inside `#[cfg(test)]`.

So the `settlement_events` table is never written outside tests. The
migration that creates it inserts fixture rows, but only inside
`#[test] fn v004_creates_settlement_events_table`.

The live `settle_replay` path writes exactly one row, into
`consumed_receipt_index`, and stops:

```text
verify_and_insert:
  1. recompute settlement_hash
  2. contains_nonce -> AlreadyConsumed?
  3. insert into consumed_receipt_index
```

Nothing appends a settlement event.

## Measurement

A settlement that verifies successfully, against a persistent database,
then queried on the event-sourced audit trail:

```text
$ quota-router settle-replay --from env.json --db-path audit.db
settlement_hash = 1962f5fea72778310e48e578207412d788407ac90107937179b688022edf80f4
index_len       = 1
verify: OK (hash matches + nonce inserted)
exit=0

$ quota-router settle-list --asker-did did:octo:asker1 --db-path audit.db
asker_did = did:octo:asker1  events = 0
exit=0
```

`events = 0` with **exit 0**. The query succeeds and reports emptiness, so
the output is indistinguishable from a system where no settlement has ever
occurred.

That is the whole audit surface for this table: it answers, correctly,
that nothing was ever written to it.

## Four code locations cited a section that does not exist

Before this commit:

```text
quota-router-cli/src/cli.rs                    RFC-0959 §Event Sourcing
quota-router-cli/src/commands.rs               RFC-0959 §Event Sourcing  (two sites)
quota-router-storage/src/settlement_event_repo.rs   RFC-0959 §Event Sourcing
```

`grep -rn "Event Sourcing" rfcs/accepted/economics/0959-ask-settlement-chain.md`
returns **nothing**. The string does not appear in RFC-0959 at all, and
there is no such heading. These were citations to a phantom section, and
they clustered in exactly the module whose table is never written.

All four are repointed at §Wire Format v2.0 in this commit, each with a
note recording that the original section does not exist. Repointing is
the conservative correction; whether RFC-0959 _should_ have an
event-sourcing section is the RFC owner's call.

For accuracy, what RFC-0959 actually contains:

- `settlement_events` is mentioned once, in §Wire Format v2.0, in
  passing — as the source of "legacy v1.0 rows" that predate
  `cost_vault_id`.
- §Backward says `quota-router-cli settle/settle-replay` works "on every
  persisted SettlementReceipt".
- §Implicit Assumptions records the persistence boundary as a Class B
  concern, and Future Work F5 lists `ConsumedReceiptIndex` persistence to
  stoolap as still to do — which is now stale, since
  `ConsumedReceiptRepository` exists behind `--db-path`.

There is no event-sourcing requirement in RFC-0959 for a settlement
repository to be implementing. So this DAO is not an unimplemented RFC
feature; it is an implemented feature whose RFC justification does not
exist.

## Why this is the most consequential of the four

The other three findings are all about checks that fail to catch
something. This one removes the record that would let anyone notice
afterwards:

| Step                    | Implemented?   | Enforced live?                   | Would leave a record? |
| ----------------------- | -------------- | -------------------------------- | --------------------- |
| Amount bound by hash    | No             | No, and only integrity mechanism | Yes, in the event row |
| Replay detection        | Yes            | Vacuous by default               | Yes, in the event row |
| Vault row + chain match | Yes, correctly | Never called                     | Yes, in the event row |
| Event trail written     | Yes            | Never called                     | —                     |

Note the third column. The `settlement_events` schema does have
`cap_root_hash`, `invocation_hash`, `cost_micro_octo_w` and
`router_signature` columns — the fields that would let a later auditor
re-derive and challenge any of the three. The schema is adequate. Nothing
writes to it.

An inflated amount, a replayed settlement, or a settlement against a
nonexistent vault all leave the same trace: a nonce in a replay index,
and nothing else.

## What the RFC owner needs to decide

- When a settlement becomes final, and therefore when an event is
  appended? `verify_and_insert` currently inserts on verification, which
  is a replay-index cursor rather than a settlement.
- Should `settle_replay` write an event on success? That is the smallest
  change that makes `settle-list` mean anything.
- Should the event be written only after the amount, replay and vault
  checks are actually enforced? Writing a canonical settlement record
  before its integrity checks run would make the record authoritative for
  a settlement that has not been checked.
- The four `§Event Sourcing` citations have been repointed at §Wire
  Format v2.0 with a note. The remaining question is whether the RFC
  should gain the section the code believed it implemented. That is an
  RFC decision, not an edit.
- Future Work F5 describes `ConsumedReceiptIndex` persistence as
  outstanding, but `ConsumedReceiptRepository` now provides it behind
  `--db-path`. The RFC is stale in the other direction.

## Reproduction

```bash
# the only consumer is a reader
grep -rn 'SettlementEventRepository' --include=*.rs crates/ \
  | grep -v 'settlement_event_repo.rs\|lib.rs'

# the insert type is constructed once, in tests
grep -rn 'SettlementEventInsert' --include=*.rs crates/

# that construction is inside cfg(test)
grep -n '#\[cfg(test)\]' crates/quota-router-storage/src/settlement_event_repo.rs

# the write path, which stops at the replay index
grep -n -A12 'pub fn verify_and_insert' \
  crates/quota-router-storage/src/consumed_receipt_repo.rs

# the migration fixture, also test-only
grep -n 'fn v004_creates_settlement_events_table' \
  crates/quota-router-storage/src/migrations.rs

# the phantom section
grep -rn 'Event Sourcing' --include=*.rs crates/
grep -c 'Event Sourcing' rfcs/accepted/economics/0959-ask-settlement-chain.md

# end to end on the real binary
./target/debug/quota-router settle-replay --from env.json --db-path audit.db
./target/debug/quota-router settle-list --asker-did did:octo:asker1 --db-path audit.db
```

## Cross-references

- RFC-0959 §Wire Format v2.0 — the only place the RFC mentions
  `settlement_events`, in passing
- RFC-0959 §Backward — claims settle-replay works on every persisted
  receipt
- RFC-0959 Future Work F5 — stale in the opposite direction
- Mission `settlement-hash-does-not-bind-the-cost` — one of the three
  findings this leaves unrecorded
- Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration` —
  the second
- Mission `the-cross-chain-settlement-reject-is-never-called` — the third
