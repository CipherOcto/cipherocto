---
name: ask-repo-cheapest-silently-ignores-asks-outside-its-candidate-limit
description: "AskRead::cheapest was documented as listing all non-expired Asks sorted by computed cost ascending, but StoolapAskRepository fetches at most 1000 rows ordered by expires_at_unix DESC and sorts only those. Measured: with 1001 asks for one model where the genuinely cheapest also expires soonest, cheapest returned a rate-50_000 ask and never considered the rate-1 one, a 50_000x miss returned as Ok with no indication it was approximate."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0959
---

# Mission `ask-repo-cheapest-silently-ignores-asks-outside-its-candidate-limit`

## Status

Deferred — the cap is deliberate and documented at the constant, so
whether it stays is an owner call. The **false contract was corrected**
here; no behaviour changed.

## Finding

`StoolapAskRepository::cheapest` runs:

```sql
SELECT ... FROM asks WHERE model = $1 AND expires_at_unix > $2
ORDER BY expires_at_unix DESC LIMIT 1000
```

then computes cost in Rust for those rows and returns the cheapest of
them.

Cost cannot be ordered in SQL because it lives in the per-axis
`rates_json`, so bounding the fetch is a real constraint, not laziness.
But the ordering is the problem: the window is the **1000 most
soon-to-expire** asks, and among those it picks the cheapest. An Ask that
expires _soonest_ is therefore the **last** candidate considered, no matter
how cheap it is.

The method doc claimed completeness:

> List all non-expired Asks for `model`, sorted by computed cost ascending.

It does not do that, and nothing in the signature, the return type, or the
doc told callers otherwise.

## Measurement

1001 asks for `openai/gpt-4`: 1000 priced at 50_000 and expiring far in
the future, plus one priced at **1** and expiring soonest — so it sorts
last under `expires_at_unix DESC` and falls outside the limit.

```text
total asks for model = 1001
winner asker = far-0
winner rate  = Dqa { value: 50000, scale: 0 }
TRUE-CHEAPEST returned: false
```

A **50,000x** miss, returned as `Ok(Some(ask))`.

## The number, and a dangling citation worth correcting

The cap is **1000**.

It is tempting to read that as "the RFC expects ~1k providers, so the
bound is fine" — and the tree does contain a comment saying exactly that:

> "for the documented ≤1k-provider target (Gap 5 perf note)"

in `cheapest_with_ranking`'s cost note in `marketplace/mod.rs`.

**That citation does not check out.** RFC-0900 contains no provider-count
target, no performance section, and no "Gap 5 perf note". The claim has
no RFC behind it. It is recorded here so nobody repeats the reasoning, and
so the decision below is made on the actual number rather than on an
invented target.

The practical trigger is narrow: it needs >1000 asks **for a single
model** sharing one model key, with the cheapest among the
soonest-to-expire. A marketplace with many providers across many models
never reaches it. One with >1000 providers on one hot model does. Whether
that scale is expected is exactly what the RFC does not say.

## Reachability today: latent

`AskRead::cheapest` has **no production caller**. The only references are
`tests/fixtures_asks.rs` and `ask_repo.rs`'s own inline tests. The
marketplace facade resolves cheapest from its in-memory order book, not
from this query — so the shipped `Marketplace::cheapest` is unaffected.

That also means the two implementations can disagree without any test
noticing: the facade's book-based `cheapest` scans every candidate with no
cap at all, so at >1000 asks the two would return different providers for
the same question.

## Actioned here

Both doc comments — the trait method and the inherent — now state that the
result is **approximate**, describe the `expires_at_unix DESC` window and
the cap, quote the 50,000x measurement, and link this mission. The
previously false "all non-expired Asks ... ascending" wording is gone from
both.

No behaviour changed, and no test was altered.

## Why the cap was not simply removed

It exists for stated reasons — "Keeps memory bounded + index-friendly (no
full table scan)" — and removing it trades an unbounded fetch for exactness
on a hot path. That is a performance decision, not a correctness one, and
it belongs to the RFC owner.

Removing the `ORDER BY expires_at_unix DESC` would not help either: with
no SQL ordering the window is arbitrary, so an Ask could be excluded on
every call and inconsistently. Ordering by anything cost-like is not
available without materialising the computed cost in SQL.

## What the RFC owner needs to decide

- Should the cap stay? If exactness matters more than the bounded fetch,
  remove it; if not, keep it and make the approximation visible in the
  **name** — `cheapest_within_window`, `approximate_cheapest` — rather
  than only in a doc comment.
- Should `cheapest` return whether it was exhaustive, e.g.
  `Option<(Ask, bool /* exact */)>`, so a caller can fall back or warn?
- Should the window order change to `expires_at_unix ASC` (cheapest-first
  candidates)? That would make short-lived cheap asks visible, at the cost
  of excluding long-lived ones — still approximate, differently biased.
- Should `Marketplace::cheapest` and this repo query be reconciled at all?
  The facade reads its book, so this method may be a migration leftover
  (mission `marketplace-repo-trait-decouple`). If it is only kept for
  `fixtures_asks.rs`, an approximate contract is acceptable; if something
  will call it, it should be exact.
- Should a per-model ask cap exist at write time, so no model can exceed
  the query window in the first place?

## Reproduction

```bash
# the bounded, expiry-ordered window
grep -n -B2 -A6 'ORDER BY expires_at_unix DESC LIMIT' crates/quota-router-storage/src/ask_repo.rs
grep -n 'const CHEAPEST_CANDIDATE_LIMIT' crates/quota-router-storage/src/ask_repo.rs

# no production caller
grep -rn '\.cheapest(' --include=*.rs crates/ | grep -v 'marketplace/mod.rs\|marketplace_e2e\|orderbook'

# the dangling citation: the RFC has no such target
grep -niE '1k-provider|perf' rfcs/accepted/economics/0900-ai-quota-marketplace.md   # no matches
grep -n '1k-provider target' crates/quota-router-core/src/marketplace/mod.rs        # the claim
```

## Cross-references

- RFC-0900 Gap 5 (order book) — and its ≤1k-provider performance note
- RFC-0959 — the Ask/rate-table shape the cost computation comes from
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — a third
  implementation of the same question, with its own `select_ask`
- Mission `marketplace-advertised-price-ignores-default-rate-fallback` —
  the other reason a computed cost can mislead a ranking, independent of
  the candidate window
- Mission `marketplace-repo-trait-decouple` — whether this read path is
  still the intended one
