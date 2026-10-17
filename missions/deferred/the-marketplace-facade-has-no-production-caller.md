---
name: the-marketplace-facade-has-no-production-caller
description: "RFC-0900 is Accepted and its v1.1 history records Gaps 5 and 7 as implemented in crates/quota-router-core::marketplace. Nothing in the workspace calls the Marketplace facade outside tests. The live request path selects providers through scorer.rs and ProviderCapacity, never through the marketplace, so the module is fully tested and entirely unreachable. v1.1: RFC-0918 adds a second one. task_market is fully built per its own Phase 1 checklist, all five items verified, and also has zero production callers, so RFC-0918 stacks an unreachable market on top of the already-unreachable one."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0900
---

# Mission `the-marketplace-facade-has-no-production-caller`

## v1.1 — a second unreachable market module, stacked on the first

RFC-0918 §Version History v1.1 records Phase 1 as implemented:

> Phase 1 (Core Market) implemented as `crates/quota-router-core::task_market`
> (Gap 6). ... Order book reuses `marketplace::orderbook::OrderBook<TaskSpec>`
> (Gap 5.1). Escrow layer uses `TaskEscrow` wrapping `marketplace::escrow::Escrow`
> (Gap 5.2). Slashing reuses `marketplace::slashing::SlashingLedger` (Gap 5.3).

I verified all five Phase 1 items independently. Every one is real:
`task_market::orders::TaskMarket`, `task_market::orders::TaskSpec`,
`task_market::escrow::TaskEscrow`, `task_market::slashing::TaskMarketSlashing`,
and `match_top`. The module has its own escrow, slashing and dispute files and a
substantive test suite.

**And it has zero production callers.** Outside its own directory, the only
references anywhere in `crates/` are `pub mod task_market;` in `lib.rs` and
`crates/quota-router-core/tests/task_market.rs`. Nothing in `octo-cli`, nothing
in any other crate. The control search for `marketplace::` does return real
production uses, so the absence is real and not a broken search.

This makes the shape worse rather than different. **RFC-0918's task market wraps
the already-unreachable marketplace in a second unreachable module.** Reaching
either one is the same amount of work, and both are fully tested. Neither
compounds into anything a user can exercise.

RFC-0918's §Implementation Phases is also the best-documented phase table in the
family: five `[x]` items in Phase 1, three unchecked phases, and the checkboxes
agree exactly with the v1.1 history row. **The documentation here is exemplary** —
which is what makes the reachability worth stating plainly rather than as a
defect in the document.

## Status

Deferred — architecture decision, not an implementation defect.

Nothing here is broken code. The marketplace is well-built and heavily
tested. What follows is a statement about **reachability**, recorded so the
distinction between "implemented and tested" and "in the request path" is not
lost.

**Re-verified.** Several later missions in this audit cite this one as the
standing caveat that makes their findings latent, so the claim was re-checked
independently rather than carried forward. Two sweeps, both excluding test
directories and the module's own tree:

```text
Marketplace::      -> 3 hits, all doc comments in quota-router-storage
Marketplace (type) -> 3 hits, all comments in octo-reputation
```

Zero calls. The claim stands.

## Finding

`Marketplace` and its methods have **no production caller** anywhere in the
workspace. Every reference is a test.

The one non-test file that imports from `quota_router_core::marketplace` is
`crates/quota-router-cli/src/commands.rs`, and it imports exactly one symbol:

```rust
use quota_router_core::marketplace::reputation_compat::parse_canonical_did;
```

A DID parser. The facade is never constructed there.

## The live request path does not use it

The router selects providers through a separate mechanism:

- `crates/quota-router-core/src/node/scorer.rs` — `score_provider`,
  `filter_capacity`, `filter_provider_preference`, over `ProviderCapacity`
- `crates/quota-router-core/src/node/provider.rs` — `ProviderCapacity`
  (health, remaining capacity, latency)

Both `node/` and `proxy.rs` contain **zero** references to `marketplace` or
`Marketplace`. Pricing, order books, reputation scoring, escrow and slashing
play no part in how a request is actually routed.

## Reachability of the defect fixed in `marketplace-put-panics-or-wraps`

The `put()` panic and `u128` wrap are real and were fixed, but the fuller
context is worth stating plainly: `Marketplace::put` also had no production
caller at the time. The fix matters because `put` is public API that any
consumer — internal, CLI, or external crate — can call, and because it was
wrong. It was not, and is not, being exploited in a shipped path.

That is also why the negative-rate hazard below is currently theoretical.

## Carried-forward hazard: negative rates

`ModelRateTable` and `cost_for` perform no validation. A negative
`rate_per_1k` is accepted and flows to every consumer:

| Consumer                                                | Effect of a negative rate                                         |
| ------------------------------------------------------- | ----------------------------------------------------------------- |
| `Marketplace::put`                                      | now rejected (`InvalidSettlementCost`)                            |
| `quota-router-storage/src/marketplace.rs` budget filter | `dqa_cmp(cost, budget_ceiling) > 0` is false, so it always passes |
| `StoolapAskRepository::cheapest`                        | negative cost always wins                                         |

Once settlement exists, a provider publishing a negative rate would be
preferred and would credit rather than charge. Today the money path is not
wired (see `marketplace-escrow-is-not-wired-into-the-marketplace-facade`), so
there is nothing to exploit — this is a trap to close before the settlement
work lands, not an active exploit.

## What the RFC owner needs to decide

- Is the marketplace intended to be the router's provider-selection path? If
  so, `scorer.rs` / `ProviderCapacity` and `Marketplace::cheapest` are two
  competing selection mechanisms and one should be retired.
- If not, what is the integration milestone, and should RFC-0900's version
  history say "implemented" rather than "in the request path" so a reader is
  not misled?

## Reproduction

```bash
# the only non-test import of the marketplace module
grep -rn 'quota_router_core::marketplace' --include=*.rs crates/ | grep -v '/tests/'

# the live routing path has no marketplace reference at all
grep -rn 'marketplace\|Marketplace' --include=*.rs \
  crates/quota-router-core/src/node/ crates/quota-router-core/src/proxy.rs

# what selection actually runs on
grep -rn 'fn score_provider\|fn filter_capacity' --include=*.rs \
  crates/quota-router-core/src/node/scorer.rs

# --- v1.1: the second unreachable market ---

# RFC-0918 v1.1 claims Phase 1 landed. All five items verify:
ls crates/quota-router-core/src/task_market/
grep -rn 'pub struct TaskMarket\b\|pub struct TaskSpec\b' \
  crates/quota-router-core/src/task_market/orders.rs
grep -rn 'pub struct TaskEscrow\b' crates/quota-router-core/src/task_market/escrow.rs
grep -rn 'TaskMarketSlashing' crates/quota-router-core/src/task_market/slashing.rs | head -1
grep -rn 'fn match_top' crates/quota-router-core/src/task_market/orders.rs

# ...and it has no caller. Only the mod declaration and the test file.
grep -rn --include=*.rs 'task_market' crates/ \
  | grep -v 'crates/quota-router-core/src/task_market/'
grep -rn -E 'task_market|TaskMarket' crates/octo-cli/src/
# empty

# CONTROL: the same search finds real production uses of `marketplace`,
# so the zero above is real rather than a broken pattern
grep -rn --include=*.rs 'marketplace::' crates/ \
  | grep -v 'crates/quota-router-core/src/marketplace/' | head -2

# and the phase table is honest about what did not ship
grep -nE '^- \[ \]' rfcs/accepted/economics/0918-inference-task-market.md | wc -l
# 9 unchecked, matching the "Phases 2-4 remain future work" history row
```

## Cross-references

- RFC-0900 v1.1 version-history row ("Gaps 5 + 7 implemented")
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade`
- Mission `marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost`
- Mission `marketplace-cheapest-with-ranking-async`, `marketplace-e2e-strong-scenarios`
