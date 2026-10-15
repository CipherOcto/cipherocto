---
name: marketplace-escrow-is-not-wired-into-the-marketplace-facade
description: "The escrow state machine ships inside crates/quota-router-core::marketplace but the Marketplace facade never constructs, locks, settles or disputes one, so the RFC-0900 escrow flow has no implementation behind it. 23 of 24 tests in marketplace_e2e.rs drive Escrow and OrderBook by hand while the header presents them as marketplace end-to-end coverage."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0901
---

# Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade`

## Status

Deferred — upstream governance decision. Not actioned here.

The evidence below is established and reproducible. The _decision_ of whether
and when the marketplace should hold funds belongs to the RFC owner, so this
mission is filed under `missions/deferred/` per BLUEPRINT §Mission Lifecycle
("Upstream blocker — governance decision"), not claimed.

## Finding

`marketplace::escrow::Escrow` is declared and documented as part of the
marketplace, but nothing in the marketplace uses it.

`crates/quota-router-core/src/marketplace/mod.rs` references `Escrow` exactly
twice:

| Line | Reference                                               |
| ---- | ------------------------------------------------------- |
| 9    | doc comment — `escrow` — escrow state machine (Gap 5.2) |
| 15   | `pub mod escrow;`                                       |

There is no escrow field, no constructor call, no `lock`/`settle`/`dispute` /
`resolve_*` call site, and no transition out of the facade. `orderbook.rs`,
`scoring.rs` and `slashing.rs` contain zero `Escrow` references.

The only consumers of the type anywhere in the tree:

- inline unit tests in `escrow.rs`
- `marketplace_e2e.rs`, which constructs it directly in test-local structs
- `task_market/escrow.rs`, which wraps it as `TaskEscrow` for RFC-0901

`marketplace/mod.rs` does not reference `task_market`, so that wiring does not
reach the marketplace either.

## Consequence for the test suite

`marketplace_e2e.rs` opens with "End-to-end integration tests for the
marketplace" and lists five scenarios including "escrow locks, settlement
releases funds". No `Marketplace` method participates in any of them. The only
facade calls in the whole file are `open_path`, `put` and `cheapest`, all
inside `stale_view_after_restart_sees_writes`. The other 23 tests build
`OrderBook` / `Escrow` / `SlashingLedger` by hand and assert on their state.

A census over test names or the file header therefore reports escrow
settlement coverage that does not exist.

## Two gaps against RFC-0900

1. **Escrow performs no fund movement.** RFC-0900 §Escrow Flow step 2 is
   "OCTO-W held in escrow (protocol contract)". The Rust type is a pure state
   machine — `Pending`, `Locked`, `Settled`, `Disputed`, `Slashed` — with an
   `amount_micro_octo_w` field that no transition reads to move anything.
   There is no ledger entry and no transfer on any path. Note RFC-0900's own
   off-chain/on-chain table places the escrow contract in **Phase 2**.

2. **`DisputeOutcome::Partial` is unimplemented.** §Dispute Resolution
   specifies three outcomes — Valid, Invalid, **Partial** ("partial refund").
   The escrow exposes only `resolve_valid` (→ `Slashed`) and
   `resolve_invalid` (→ `Settled`). A partial refund has no representation
   anywhere in `marketplace/` or `task_market/`.

## Actioned in this mission (documentation only)

`marketplace_e2e.rs` headers and comments now state what the file covers and
what it does not, instead of implying facade-level integration. The
close/reopen test's comments previously said "Process A" / "Process B" while
running in a single process; they now say "Scope", consistent with the
deferral already recorded by mission `marketplace-subprocess-restart-recovery`.

No production code, no RFC, and no archived record was modified.

## What the RFC owner needs to decide

- Should the quota marketplace hold funds at all before Phase 2, or is
  discovery-only (`put` → `cheapest`) the intended end state for RFC-0900?
- If funds are in scope, where does the escrow get constructed — on match, or
  on buyer-initiated purchase per §Escrow Flow step 1?
- Is `Partial` required before the dispute path can be considered complete,
  or is Valid/Invalid sufficient for the first release?
- Which layer owns settlement: the Rust `Escrow`, an on-chain contract, or
  both?

## Reproduction

```bash
# facade references to Escrow: doc comment + module decl only
grep -nE 'escrow|Escrow' crates/quota-router-core/src/marketplace/mod.rs

# escrow has zero presence in the sibling components
grep -c 'Escrow' crates/quota-router-core/src/marketplace/{orderbook,scoring,slashing}.rs

# facade calls in the e2e suite: all inside one test
grep -n 'Marketplace::\|\.cheapest(' crates/quota-router-core/tests/marketplace_e2e.rs
```

## Cross-references

- RFC-0900 §Escrow Flow, §Dispute Resolution, v1.1 version-history row
- RFC-0901 (Task Market) — the surface that _does_ wire the escrow
- Mission `marketplace-e2e-strong-scenarios` (owner of `marketplace_e2e.rs`)
- Mission `marketplace-subprocess-restart-recovery` (deferred cross-process
  restart coverage)
- Mission `marketplace-escrow-caller-authorization` (transition authz)
