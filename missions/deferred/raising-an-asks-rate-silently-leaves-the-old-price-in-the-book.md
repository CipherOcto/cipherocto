---
name: raising-an-asks-rate-silently-leaves-the-old-price-in-the-book
description: "Marketplace::put inserts into an order book keyed on (price, seq) with no removal of any prior entry, and no cancel, remove, supersede or replace method exists on Marketplace or OrderBook. A provider that raises its rate keeps being returned by cheapest() at the old lower price, and every re-price leaves a permanent stale entry."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `raising-an-asks-rate-silently-leaves-the-old-price-in-the-book`

## Status

Deferred — the replacement semantics are an RFC question, not a bug fix.
Measurement below is reproducible.

## Measurement

Put an Ask at `10_000`, then put the same asker's ask for the same model at
`50_000`:

```
PROBE after cheap put: cost=10000
PROBE ask ids equal: false
PROBE after dear put: cost=10000 ask_id_equal_to_cheap=true
```

After the provider **raises** their rate by 5x, `cheapest()` still returns
`cost_per_1k = 10000`, and it is the _old_ `ask_id`. The new 50,000 entry was
added; the old 10,000 entry was not removed.

## Mechanism

`OrderBook::place_ask` inserts on `(price, seq)`:

```rust
self.asks.insert((price, seq), order);
```

A changed rate yields a changed `price`, hence a **new key**. Nothing removes
the previous key. `compute_ask_id_static` includes the rates, so the new ask
also gets a new `ask_id` — and nothing keys on `ask_id` for removal either.

There is no escape hatch:

```bash
grep -rE 'pub fn (cancel|remove|delete|supersede|replace|purge)' \
  crates/quota-router-core/src/marketplace/     # no matches
```

## Consequences

1. **A price increase never takes effect.** The provider continues to be
   matched at the stale lower rate until the old ask expires. On the
   buyer's side that is overpayment by whatever the raise was; on the
   provider's side, a rate increase is unenforceable.
2. **The book grows without bound.** Every re-price leaves a permanent stale
   entry. `best_ask()` is `self.asks.values().next()` — O(1), but the map it
   reads keeps accumulating entries no query can clear.
3. **RFC-0900 §Listing Lifecycle is unimplementable.** The state machine is
   `CREATED → ACTIVE → (EXHAUSTED | CANCELLED)`. `CANCELLED` has no
   implementation, and there is no `SUPERSEDED` state at all — the RFC has no
   concept of a listing being repriced.

## Why deferred

A minimal fix requires choosing semantics the RFC does not specify:

- supersede by `(asker_did, model)` — the natural reading, but it means one
  active ask per provider per model, which may conflict with multi-model or
  multi-rate listings
- supersede by `ask_id` — impossible as-is, since `ask_id` changes when the
  rate changes
- add an explicit cancel API and require the caller to invoke it — changes
  the write contract rather than `put`'s behaviour

Each is a listing-lifecycle decision. Recorded here rather than guessed.

## What the RFC owner needs to decide

- Should `put` supersede a provider's previous ask for the same model, or
  should supersession be an explicit operation?
- If superseding: is the key `(asker_did, model)`, and does that forbid a
  provider listing several rates for one model?
- What is `CANCELLED` in practice — a remove-from-book call, a state flag
  checked at read, or a delete from the repo?
- Should stale entries be swept on a timer, or is supersession sufficient to
  bound growth?

## Reproduction

The probe is a two-`put` sequence; `ask_with_rate(seed, value, scale)` already
exists in `marketplace_e2e.rs`.

```bash
grep -n 'pub fn put' -A 30 crates/quota-router-core/src/marketplace/mod.rs
grep -n 'pub fn place_ask' -A 16 crates/quota-router-core/src/marketplace/orderbook.rs
```

## Cross-references

- RFC-0900 §Listing Lifecycle
- Mission `the-marketplace-facade-has-no-production-caller` — no production
  caller today, so this is latent rather than live
- Mission `marketplace-e2e-strong-scenarios` (owner of `marketplace_e2e.rs`)
