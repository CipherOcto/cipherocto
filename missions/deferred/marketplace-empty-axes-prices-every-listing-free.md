---
name: marketplace-empty-axes-prices-every-listing-free
description: "Marketplace::with_axes accepts an empty pricing-axis set with no guard. Cost is summed across configured axes, so with none configured every Ask is priced 0, cheapest stops comparing prices and returns whichever Ask was placed first, and the repricing and cheapest-comparison logic silently loses its meaning. Measured: the same Ask priced 30000 under default axes priced 0 under with_axes(vec![])."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0959
---

# Mission `marketplace-empty-axes-prices-every-listing-free`

## Status

Deferred — `with_axes` returns `Self`, so rejecting an empty set is a
signature change, and every alternative either changes the API or
fabricates a default. The **hazard is now documented on the method**; no
behaviour changed.

## Finding

```rust
#[must_use]
pub fn with_axes(mut self, axes: Vec<PricingAxis>) -> Self {
    self.axes = axes;
    self
}
```

No validation. An empty vector is accepted and becomes the pricing
configuration.

Pricing derives from `self.axes`:

```rust
fn build_unit_consumed(ask: &Ask, axes: &[PricingAxis]) -> Vec<(String, u64)> {
    axes.iter().map(|a| (a.id.clone(), 1000)).collect()
}
// settlement_cost sums cost_for() over that list
```

With no axes the consumption list is empty, `settlement_cost` returns
zero, and the book stores `price = 0` for every Ask. That passes the
existing representability gate (`scale == 0`, `value >= 0`) trivially —
`Dqa { value: 0, scale: 0 }` is a perfectly valid free price.

## Measurement

Same Ask, priced 30_000, two configurations:

```text
default axes -> cheapest price = 30000
EMPTY axes   -> cheapest price = 0
```

And with two Asks at 10_000 and 90_000 under empty axes:

```text
EMPTY axes, 10_000 vs 90_000 -> winner price 0 asker did:octo:z8s1i1cM…
```

Both price `0`, so the cheapest comparison is decided by the order book's
`(price, seq)` tie-break — that is, by **insertion order**. The result
happened to be the cheaper Ask here only because it was placed first.

## Consequence

Three things lose their meaning at once, silently:

1. **`cheapest` stops being about price.** Every listing ties at zero, so
   the cheapest-comparison returns the first-listed Ask regardless of what
   anyone charges.
2. **The listing advertises free and is charged full price.** This is the
   serious one, and it is the opposite of what an earlier draft of this
   mission claimed.

   I expected settlement to collapse to zero as well, reasoning that an
   empty `axes` leaves `PricingAxis::find_by_id` with nothing to return.
   That is wrong. `cost_for` consults `axes` only as a **fallback**:

   ```rust
   let rate = self.rates.iter()
       .find(|r| r.axis == axis_id)
       .map(|r| r.rate_per_1k)                                  // explicit rate wins
       .or_else(|| PricingAxis::find_by_id(axes, axis_id)...)?; // only if absent
   ```

   An Ask that _lists_ a rate is charged that rate whatever `axes`
   contains. Measured:

   ```text
   ADVERTISED (book)       = 0
   SETTLED with empty axes = 30000
   SETTLED with default axes = 30000
   ```

   So the book quotes **free** while settlement charges the full **30_000**
   — a 30,000x gap produced by configuration alone, and worse than the
   default-axis case, not better. See
   `marketplace-advertised-price-ignores-default-rate-fallback` for the
   non-degenerate version of the same mismatch; this one needs no unusual
   Ask, only an empty vector.

3. **A misconfiguration is indistinguishable from a free market.** There
   is no error, no log, and no test — every listing simply costs nothing.

## Severity today: latent, and it has no callers

`with_axes` has **zero references** anywhere in the tree outside its own
definition. Nothing constructs a `Marketplace` with custom axes today, so
the degenerate configuration cannot arise from existing code. Combined
with the marketplace having no production callers at all, nothing shipping
is affected.

It is worth fixing before any deployment that wants custom axes, because
the failure mode is a marketplace that appears to work and charges
nothing.

## Why it was not simply guarded

`with_axes` returns `Self`, not `Result`. The options:

| Option                                | Effect         | Why not taken                                     |
| ------------------------------------- | -------------- | ------------------------------------------------- |
| Return `Result<Self, _>`              | rejects empty  | breaking public signature change                  |
| Substitute `standard_axes()` on empty | never free     | fabricates a default the caller did not ask for   |
| `assert!` / `panic!`                  | rejects empty  | a library panic on caller input is not a boundary |
| Leave unguarded + document            | footgun marked | chosen here, as the only non-breaking option      |

The first two are reasonable; both are owner calls because they change the
public API or its semantics.

## What the RFC owner needs to decide

- Should `with_axes` return `Result` and reject an empty set with a typed
  error, matching the pattern already used for `InvalidSettlementCost` and
  `InvalidAskRate`?
- Or should an empty axis set be a legitimate "free market" mode, in which
  case it needs an explicit constructor (`Marketplace::free_market()`)
  rather than being an accident of `with_axes(vec![])`?
- Should pricing refuse an Ask whose rates name axes absent from the
  configured set, rather than silently pricing it at zero? Today a
  partially-configured deployment charges nothing for the axes it forgot.
- Should `Marketplace` expose the configured axis ids for a health check,
  so a misconfiguration is observable without reading source?

## Reproduction

```bash
# the unguarded builder
grep -n -A5 'pub fn with_axes' crates/quota-router-core/src/marketplace/mod.rs

# cost is summed over configured axes, so zero axes means zero cost
grep -n -A4 'fn build_unit_consumed' crates/quota-router-core/src/marketplace/mod.rs

# no caller anywhere
grep -rn 'with_axes' --include=*.rs crates/ | grep -v 'pub fn with_axes'
```

## Cross-references

- RFC-0900 §Market Operations — where cheapest lookup is specified
- RFC-0959 §Data Structures — the per-axis rate table being priced
- Mission `marketplace-advertised-price-ignores-default-rate-fallback` —
  the non-degenerate advertised-versus-settled mismatch
- Mission `marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost`
  — the other `put` pricing gate, where `Dqa { 0, 0 }` passes validation
  precisely because it is a legitimate free Ask
- Mission `marketplace-order-book-accepts-zero-quantity-orders` — the same
  shape of problem: a public builder/setter that cannot reject a
  value it should refuse
- Mission `the-marketplace-facade-has-no-production-caller` — why nothing
  shipping is affected today
