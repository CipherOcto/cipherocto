---
name: marketplace-index-is-a-parallel-unwired-abstraction
description: "quota_router_storage::marketplace::MarketplaceIndex is a second, complete marketplace index that nothing consumes: its only references outside its own file are the pub use in lib.rs and its own inline tests. It duplicates the facade's OrderBook<AskSpec> with divergent semantics — it has its own list_by_asker, select_ask, budget_ceiling, jurisdiction filter, expiry handling and ACTIVE_ASK_CAP eviction, and it retains expired asks until an overflow prune. lib.rs advertises it as the storage surface for ordered lookups."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0959
---

# Mission `marketplace-index-is-a-parallel-unwired-abstraction`

## Status

Deferred — removing a public re-export or wiring a second index is an
RFC-owner architecture decision, not a marketplace-local fix. One property
defect inside the type was **actioned**; see §Actioned.

## Finding

`quota_router_storage` ships **two** complete marketplace indexes over the
same `Ask` type.

|           | `quota_router_core::marketplace::OrderBook<AskSpec>`                               | `quota_router_storage::marketplace::MarketplaceIndex`     |
| --------- | ---------------------------------------------------------------------------------- | --------------------------------------------------------- |
| Home      | the facade's `book` field                                                          | standalone, re-exported from `lib.rs`                     |
| Lookup    | `asks_matching` + in-crate ranking                                                 | `select_ask` (own ranking, own budget/jurisdiction logic) |
| Expiry    | filtered on **read**                                                               | filtered on **select** only; retained otherwise           |
| Capacity  | none                                                                               | `ACTIVE_ASK_CAP` with overflow eviction                   |
| Consumers | `Marketplace::{cheapest, cheapest_with_ranking, cheapest_with_ranking_async, put}` | **none**                                                  |

Every reference to `MarketplaceIndex` outside its own file is the re-export:

```text
crates/quota-router-storage/src/lib.rs  (module doc)    //! - [`marketplace`] — in-memory `MarketplaceIndex` ...
crates/quota-router-storage/src/lib.rs  (re-export)     pub use marketplace::{MarketplaceIndex, ACTIVE_ASK_CAP};
```

Every other reference is inside `marketplace.rs`, and every one of those
after the type's own `impl` block is an inline `#[cfg(test)]` test. No
facade method, no other crate, and no integration test constructs one.

Yet `lib.rs` presents it as a live capability: "`marketplace` — in-memory
`MarketplaceIndex` for ordered lookups (RFC-0959 §Roles)". A reader
scanning the crate's public surface would reasonably conclude that
routing goes through it. It does not — the facade's order book does, and
the facade reaches storage only through `DynAskRepository`.

This is the duplication pattern that has produced most of the defects
found in this area: two implementations of the same idea, each patched at
whichever site the auditor happened to read.

## Divergent semantics, not just duplication

The two indexes do not agree on behaviour:

- **Expiry.** `OrderBook` filters expired asks on read. `MarketplaceIndex`
  retains them indefinitely — `insert` only prunes when the count exceeds
  `ACTIVE_ASK_CAP`. Verified: an index holding one live and two expired
  asks reported `len() == 3`, and `select_ask` returns `None` for them
  only because it evicts first.
- **Ranking.** Each has its own cost computation, tie-break, and budget
  ceiling. `Marketplace::put` writes to `OrderBook` and to the repo; the
  index is never updated, so even if it were wired it would be stale by
  construction.

## Actioned in this mission

`MarketplaceIndex::list_by_asker` was documented as "List all **active**
asks for a given asker DID" but took no time argument and applied no
expiry filter, so it returned expired asks. This contradicted both its own
doc and the storage-level `AskRead::list_by_asker`, which has always
taken `(asker_did, now_unix)` and filtered on `expires_at_unix > now_unix`
in SQL.

Fixed to match the sibling contract: `list_by_asker(&self, asker_did,
now_unix)` now filters on `expires_at_unix > now_unix`. One call site
existed, in the type's own tests.

The pre-existing test `list_by_asker_filters` used only live asks, so it
passed without ever exercising the cutoff — a check over a population it
did not read. Verified by mutation: removing the new filter fails
`list_by_asker_excludes_expired_asks` (`left: 3, right: 1`) while
`list_by_asker_filters` keeps passing, which is what demonstrates the old
test was vacuous rather than merely redundant.

`list_by_asker_excludes_expired_asks` carries two controls so the filter
cannot pass by accident: a cutoff of `0` returns all three retained asks,
and a cutoff past every expiry returns none. The retained count is
asserted separately (`idx.len() == 3`) to pin that expiry is a read-time
filter, not an insert-time one.

The wider duplication is **not** actioned.

## Why this was not simply removed

Deleting a public, re-exported type is a breaking API change, and this is
not a marketplace-local call:

1. `MarketplaceIndex` may be intended as the RFC-0959 substrate that the
   settlement engine builds on, independent of the RFC-0900 facade. The
   RFC, not the code, decides that.
2. `ACTIVE_ASK_CAP` overflow eviction has no counterpart in `OrderBook`.
   If the cap is a real requirement, it belongs in the facade's book, and
   that is a design change.
3. `select_ask`'s jurisdiction filter and budget ceiling are implemented
   here and **not** in the facade's ranking. If any of that logic is
   wanted, it needs porting rather than deleting — which is work, and
   should be done against a decision rather than in anticipation of one.

## What the RFC owner needs to decide

- Is `MarketplaceIndex` live surface, or residue from RFC-0959? If
  residue, should the re-export be removed, or kept with an explicit
  "not wired into the RFC-0900 marketplace" note?
- If it is live, how does it get updated on `Marketplace::put`? Today it
  is never written to outside its own tests, so any consumer would read a
  permanently stale index.
- Should `ACTIVE_ASK_CAP` and its overflow eviction exist for the
  marketplace at all? `OrderBook` has no capacity bound.
- Which of the two implements jurisdiction filtering and `budget_ceiling`
  — the index has both, the facade's ranking has neither?
- Should the facade's `OrderBook` adopt the index's read-time expiry
  discipline, or the index adopt the book? (The book already filters on
  read, per an earlier fix; the index only filters on select.)

## Reproduction

```bash
# the only references outside the type's own file
grep -rn "MarketplaceIndex" --include=*.rs crates/ | grep -v "src/marketplace.rs"

# what lib.rs advertises
grep -n "MarketplaceIndex" crates/quota-router-storage/src/lib.rs

# the divergence in expiry discipline
grep -n -A8 "pub fn select_ask" crates/quota-router-storage/src/marketplace.rs
grep -n -B2 -A8 "fn list_by_asker" crates/quota-router-storage/src/marketplace.rs

# the actioned fix
cargo test -p quota-router-storage --lib \
  marketplace::tests::list_by_asker_excludes_expired_asks -- --nocapture
```

## Cross-references

- RFC-0900 §Listing Lifecycle — the facade path the book actually serves
- RFC-0959 §Roles — cited by `lib.rs` as the rationale for this index; that
  citation is the strongest evidence the type was intended for something
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade` —
  the same shape of finding, for escrow and slashing
- Mission `the-marketplace-facade-has-no-production-caller` — the facade
  itself is also unwired, so neither index is on a live path
- Mission `raising-an-asks-rate-silently-leaves-the-old-price-in-the-book` —
  supersession semantics, unresolved, and load-bearing for any decision to
  wire a second index
