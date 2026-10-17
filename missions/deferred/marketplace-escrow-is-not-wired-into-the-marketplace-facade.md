---
name: marketplace-escrow-is-not-wired-into-the-marketplace-facade
description: "The escrow and slashing components ship inside crates/quota-router-core::marketplace but the Marketplace facade holds neither — its struct has five fields and none of them is an Escrow or SlashingLedger, so RFC-0900 Gap 5.2 and Gap 5.3 have no implementation behind the facade. 23 of 24 tests in marketplace_e2e.rs drive Escrow and OrderBook by hand while the header presents them as marketplace end-to-end coverage."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.1"
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

`marketplace::escrow::Escrow` and `marketplace::slashing::SlashingLedger`
are declared and documented as parts of the marketplace, but nothing in the
marketplace uses them.

The decisive evidence is the struct itself. `Marketplace` holds exactly five
fields:

| Field               | Component                             | Wired         |
| ------------------- | ------------------------------------- | ------------- |
| `repo`              | `Arc<DynAskRepository>`               | yes           |
| `axes`              | `Vec<PricingAxis>`                    | yes           |
| `book`              | `Mutex<OrderBook<AskSpec>>`           | yes (Gap 5.1) |
| `reputation`        | `ProviderReputationRegistry`          | yes (Gap 7)   |
| `reputation_compat` | `ProviderReputationRegistryCompat<S>` | yes (Gap 7)   |

There is no escrow field, no slashing field, no constructor call, no
`lock`/`settle`/`dispute`/`resolve_*` call site, and no `slash` call site
reachable from the facade. `orderbook.rs`, `scoring.rs` and `slashing.rs`
contain zero `Escrow` references.

So of the three components RFC-0900 v1.1 records as landed under Gap 5 —
order book (5.1), escrow (5.2), slashing (5.3) — **only the order book is
actually wired.** The version-history row lists all three without
distinguishing module-landed from facade-integrated.

The only consumers of `Escrow` anywhere in the tree:

- inline unit tests in `escrow.rs`
- `marketplace_e2e.rs`, which constructs it directly in test-local structs
- `task_market/escrow.rs`, which wraps it as `TaskEscrow`

`marketplace/mod.rs` does not reference `task_market`, so that wiring does not
reach the marketplace either.

### Explicitly NOT a defect: `task_market`

`task_market` (RFC-0901) has only two references outside its own directory —
the `pub mod task_market;` line in `lib.rs` and `tests/task_market.rs`. No
production code in any workspace crate consumes it.

That is **expected**: RFC-0901 is **Draft**, not Accepted. A module landing
ahead of its integration is normal for a draft RFC. It is recorded here only
so the absence is not later mistaken for an oversight, and it is explicitly
out of scope for this mission.

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

## Three more gaps against RFC-0900 (added at v1.1)

Mapping the Rust enum onto RFC-0900 §Purchase Lifecycle and §Dispute
Lifecycle shows three further absences. None is a bug in what the code
does — each is state or data the RFC specifies and the type has no room
for.

### 1. There is no investigation state

§Dispute Lifecycle is:

```
FILED → INVESTIGATING: Evidence collected
INVESTIGATING → {VALID, INVALID, PARTIAL}
```

`EscrowState` has no `Investigating`. The Rust states are `Pending`,
`Locked`, `Settled`, `Disputed`, `Slashed`, and `dispute()` moves straight
to `Disputed` — which therefore conflates **FILED** with **INVESTIGATING**.

The consequence is that the arbitrator may resolve **immediately** after
the buyer raises the dispute: `resolve_valid` and `resolve_invalid` both
require `Disputed`, and nothing gates them behind an evidence-collection
window. The RFC's mandatory investigation stage does not exist, so the
"evidence" half of the dispute lifecycle is unmodelled rather than merely
unimplemented.

### 2. No timestamps anywhere on the escrow

§Observability requires a `timestamp: number` on every `purchase`,
`listing`, `swap` and `dispute` telemetry event.

`Escrow` carries six fields — `id`, `buyer`, `seller`, `arbitrator`,
`amount_micro_octo_w`, `state` — and **no time field at all**. There is no
`locked_at`, no `disputed_at`, no `resolved_at`. A `dispute` event
therefore cannot be timestamped from the state machine, and the gap
between filing and resolution is unrecoverable after the fact, which is
exactly the interval §Dispute Evidence Challenge depends on.

### 3. Disputes are indistinguishable

§Dispute Evidence Challenge classifies disputes by cause and
verifiability — Timeout, Provider error, Latency high (all **Automatic**)
versus Garbage response (**Manual**, requires human review).

The escrow has no dispute-reason field and no evidence field. A timeout
and a garbage-response complaint are the same state transition
(`Locked → Disputed`) and produce the same record, so the
automatic-versus-manual split the RFC calls "for MVE" cannot be
distinguished, and nothing records what the arbitrator actually examined
before calling `resolve_valid`.

### None of these are pinned by a test

Deliberately. Each is a missing capability, so a test would have to
assert the absence — enshrining the gap. They are recorded here so the
wiring work inherits a complete list rather than rediscovering them at
Phase 2.

### 4. Terminal states conflate distinct RFC outcomes

Both terminal states are reachable by two materially different paths, and
nothing records which happened:

| Rust state | Path A                           | Path B                                           | RFC terminal states                      |
| ---------- | -------------------------------- | ------------------------------------------------ | ---------------------------------------- |
| `Settled`  | `settle` — buyer paid, work done | `resolve_invalid` — dispute rejected, buyer pays | `COMPLETED` **and** `CONFIRMED` / `KEEP` |
| `Slashed`  | —                                | `resolve_valid` — dispute upheld                 | `REFUNDED` **and** `SLASHED`             |

`EscrowSnapshot` adds no discriminator either — it carries the same six
fields, so a snapshot taken after resolution is indistinguishable from one
taken after a clean success. There is no flag, no history, no timestamp.

Two consequences worth stating plainly:

- The doc comment on `EscrowState::Settled` reads "Funds released to seller
  (success path)". That describes only Path A. A rejected dispute lands in
  the same variant, and the two are indistinguishable afterwards — so an
  operator reading the enum cannot tell a clean completion from a dispute
  the buyer lost.
- `Slashed` names only the slash. On a valid dispute §Escrow Flow step 4
  says "**Refund to buyer**, slash seller stake" — two effects. The refund
  half has no representation at all, consistent with the module moving no
  funds.

The suite already demonstrates Path B: `dispute_invalid_confirms_payment`
ends in `Settled`, exactly as `happy_path_bid_matches_ask_escrow_settles`
does. The two tests differ in history, not in the resulting state.

## Actioned in this mission (documentation only)

`marketplace_e2e.rs` headers and comments now state what the file covers and
what it does not, instead of implying facade-level integration. The
close/reopen test's comments previously said "Process A" / "Process B" while
running in a single process; they now say "Scope", consistent with the
deferral already recorded by mission `marketplace-subprocess-restart-recovery`.

`marketplace/mod.rs` module inventory now marks each submodule **Wired** or
**Not wired** and states the five fields the facade actually holds, so the
module list cannot be read as a claim that all three Gap 5 components are
integrated.

No production logic, no RFC, and no archived record was modified.

## A fail-closed property worth knowing

An `Escrow` built with `Escrow::new` (or `TaskEscrow::new`, which is the
default constructor) has an **empty arbitrator**. `check_caller` rejects any
resolve when the stored identity is empty, so both `resolve_valid` and
`resolve_invalid` return `UnauthorizedCaller` forever.

`Disputed` is not terminal (`is_terminal()` is `Settled | Slashed` only) and
`resolve_valid` / `resolve_invalid` are the only transitions out of it. There
is no timeout, expiry or admin recovery.

So an escrow created without an arbitrator, once disputed, is **permanently
stuck**. This is intentional and documented ("Always fail-closed: unknown /
unset arbitrators reject all `resolve_*` calls"), and it is the correct
default — but it means `TaskEscrow::new` callers inherit an unresolvable
dispute path unless they opt into `with_arbitrator`.

No test pins this. Worth a guard test either way. _(Since filed:
`escrow_without_arbitrator_dispute_is_permanently_stuck` in
`marketplace_e2e.rs` pins it, with a positive control.)_

## The locked amount is unvalidated — deliberately not fixed here

`Escrow::new` and `Escrow::with_arbitrator` both accept
`amount_micro_octo_w: Dqa` with **no validation at all** — no scale check, no
sign check. A survey of the Dqa-accepting surface found this is one of nine
unguarded public entry points in the marketplace + storage boundary:

| API                                                               | Guard                                              |
| ----------------------------------------------------------------- | -------------------------------------------------- |
| `SlashingLedger::register`                                        | non-negative                                       |
| `SlashingLedger::withdraw_stake` / `can_withdraw`                 | positive                                           |
| `stoolap_spend_ledger::seed` / `try_deduct`                       | inline sign check                                  |
| `Marketplace::put`                                                | representability (`InvalidSettlementCost`)         |
| **`Escrow::new` / `with_arbitrator`**                             | **none**                                           |
| `select_ask` (`budget_ceiling`)                                   | none (benign — a negative ceiling selects nothing) |
| `dqa_add` / `dqa_mul` / `dqa_cmp` / `dqa_to_bytes` / `from_micro` | none (correctly sign-agnostic)                     |

A negative or fractional-scale locked amount is accepted.

**Not fixed here, on purpose.** Three reasons:

1. The field is currently **inert**. `Escrow` performs no arithmetic on
   `amount_micro_octo_w` — every reference is storage, clone, or snapshot.
   There is no present harm.
2. The escrow is unwired, so the marketplace cannot even reach this
   constructor today (see the main finding above).
3. A correct fix means changing `Escrow::new` / `with_arbitrator` to return
   `Result`, which is a **breaking public API change** across 17 call sites —
   real churn for an inert field in an unreachable module.

The validation belongs with the wiring work. Once escrow is connected and the
amount starts doing arithmetic (refunds, the `Partial` dispute outcome), it
must be `scale=0` and non-negative, matching what `Marketplace::put` and
`SlashingLedger::register` already enforce.

## What the RFC owner needs to decide

- Should the quota marketplace hold funds at all before Phase 2, or is
  discovery-only (`put` → `cheapest`) the intended end state for RFC-0900?
- If funds are in scope, where does the escrow get constructed — on match, or
  on buyer-initiated purchase per §Escrow Flow step 1?
- Is `Partial` required before the dispute path can be considered complete,
  or is Valid/Invalid sufficient for the first release?
- Which layer owns settlement: the Rust `Escrow`, an on-chain contract, or
  both?
- Is the §Dispute Lifecycle investigation stage in scope for the first
  release, or should the escrow collapse FILED and INVESTIGATING
  deliberately — in which case the RFC should say so, because today the
  collapse reads as an omission rather than a decision?
- Should the escrow carry timestamps (`locked_at`, `disputed_at`,
  `resolved_at`) so §Observability's required `timestamp` can be emitted?
- Should a dispute record its cause and evidence class, so the
  automatic-versus-manual split in §Dispute Evidence Challenge is
  expressible?

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
