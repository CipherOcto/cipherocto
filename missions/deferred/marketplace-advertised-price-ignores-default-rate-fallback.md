---
name: marketplace-advertised-price-ignores-default-rate-fallback
description: "The order book advertises the sum of an Ask's explicitly listed rates, but settlement charges ModelRateTable::cost_for, which falls back to PricingAxis::default_rate_per_1k for any consumed axis the Ask omits. Two Asks that cost exactly the same at settlement are advertised 7x apart, and the cheaper-advertised one wins every cheapest comparison. Measured: rates [input 10_000] advertises 10_000 and settles 70_000, while rates [input 10_000, output 60_000] advertises 70_000 and settles 70_000. At v1.1: RFC-0959 requires UnknownAxis for an unpriced consumed axis, so the fallback is a conformance deviation rather than a settled convention."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0900
    - RFC-0959
---

# Mission `marketplace-advertised-price-ignores-default-rate-fallback`

## Status

Deferred — the defect is unambiguous but the _correct_ advertised price is
an economics decision. Evidence measured; an experimental fix was written
and reverted. Nothing actioned.

**v1.1** adds the RFC-0959 conformance basis and withdraws two claims from
v1.0: that the fallback was settled and reasonable, and that removing it
would make unpriced axes free. Both were wrong.

## Finding

The marketplace computes two prices from the same `Ask`, through two
different paths, and they disagree.

**Advertised** — what the order book stores and what
`MarketplaceEntry::cost_per_1k` reports, which is what `cheapest` ranks on:

```rust
fn build_unit_consumed(ask: &Ask, axes: &[PricingAxis]) -> Vec<(String, u64)> {
    axes.iter()
        .filter(|a| ask.rates.rates.iter().any(|r| r.axis == a.id))  // <-- explicit only
        .map(|a| (a.id.clone(), 1000))
        .collect()
}
```

**Settled** — what `settlement_cost` charges for real consumption:

```rust
let rate = self.rates.iter()
    .find(|r| r.axis == axis_id)
    .map(|r| r.rate_per_1k)
    .or_else(|| PricingAxis::find_by_id(axes, axis_id).map(|a| a.default_rate_per_1k))?;
```

The fallback is deliberate and documented on `ModelRateTable`:
"falls back to `PricingAxis::default_rate_per_1k` if axis missing". It
exists so an unpriced axis is charged at the default rather than being
free.

> **Corrected at v1.1.** An earlier version of this mission presented the
> fallback as settled and reasonable, and ruled out removing it as an
> option. Both were wrong, and the RFC settles it. See §RFC-0959 requires
> fail-closed below.

The advertised path filters those unpriced axes **out**, so it never sees
the fallback the buyer will actually be charged.

## Measurement

Two Asks for the same model, same real cost, standard axes
(input 30_000, output 60_000, cached-input 3_000):

| Ask   | Rates listed                  | Advertised | Settled    |
| ----- | ----------------------------- | ---------- | ---------- |
| **A** | `input 10_000`                | **10_000** | **70_000** |
| **B** | `input 10_000, output 60_000` | **70_000** | **70_000** |

```text
A (input only)      advertised=10000 settled=70000
B (input+output)    advertised=70000 settled=70000
advertised differ: true
settled    same  : true
```

A and B cost the buyer identically. A is advertised **7x cheaper**, so A
wins every `cheapest` / `cheapest_with_ranking` comparison and is the
listing the router selects.

## Consequence

`MarketplaceEntry::cost_per_1k` is not the number the buyer is charged.
It is not even an estimate — for a single-axis Ask it under-reports by the
sum of every omitted axis default (here 60_000 + 3_000 = 7x).

Two things follow:

1. **Ranking is systematically biased.** A provider who lists fewer axes
   appears cheaper than one who states the same economics explicitly. The
   cheapest-comparison picks the least transparent listing, which is the
   opposite of what a market wants to reward.
2. **The advertised figure is the likely dispute trigger.** RFC-0900
   §Dispute Evidence Challenge anticipates exactly this shape — a buyer
   who expected one price and is charged another. The escrow is unwired
   today, so nothing catches it.

The "free Ask" case is **not** this bug. An Ask that explicitly lists
`rate_per_1k = 0` is genuinely free, and there is an existing test pinning
that it must be accepted. The defect is only that _omitting_ a rate is
treated as free in the advertised figure while being charged in settlement.

## Severity today: latent

The marketplace has zero production callers, so no listing is currently
mispriced in production. This becomes live the moment `put` feeds a
settlement path.

## What was tried and reverted

The fix is to make the advertised path see the same fallback:

```rust
fn build_unit_consumed(ask: &Ask, axes: &[PricingAxis]) -> Vec<(String, u64)> {
    axes.iter().map(|a| (a.id.clone(), 1000)).collect()
}
```

i.e. price every configured axis, letting `cost_for` supply defaults. That
makes advertised equal settled for the unit case.

It was implemented and **reverted**, because it is an economics decision
rather than a bug fix:

- It changes `cost_per_1k` for **every** listing. Four existing tests in
  `marketplace::tests` assert absolute advertised prices
  (`put_indexes_active_ask_in_orderbook` moved 30_000 → 93_000).
- It raises the question of which axes belong in a universal advertised
  figure. `cached_input_tokens_per_1k` has a 3_000 default, so every Ask
  would be advertised as charging for cached input whether or not it was
  ever priced.
- Operators can extend `axes` via `with_axes`, so a custom axis added for
  some deployments would silently enter the advertised price of every Ask
  in those deployments.
- Relative ranking is preserved (every Ask receives the same default
  additions), so this is purely a change to the absolute figure and to what
  the field means.

## RFC-0959 requires fail-closed

This is the part that was missing at v1.0, and it changes the framing of
the whole decision.

RFC-0959 defines the Ask's signed rate table as `AskUnsignedPayload.axes:
BTreeMap<String, MicroOCTO_W>`, and §Compute cost resolves a consumed axis
against **that map**:

```rust
let rate = ask.payload.axes
    .get(axis_id)
    .ok_or_else(|| SettlementError::UnknownAxis(axis_id.clone()))?;
```

There is no registry default and no second lookup in the RFC's cost path.
An Ask that does not price a consumed axis is an **error**, not a default.
§Error Handling treats `UnknownAxis` as recoverable with a publish-time
remedy ("asker publishes axis in TOML registry first"), and Appendix C
states the principle directly: "Verifiers reading old TOML reject unknown
axes (**fail-closed**)."

The implementation substitutes the registry default for the Ask's own rate
whenever the Ask's map misses, so `UnknownAxis` now fires only when the
axis is in **neither** the Ask's rate table **nor** the registry.

Measured directly: an Ask pricing only `input_tokens_per_1k: 90_000`,
charged for 1000 units of `output_tokens_per_1k` (a standard axis, present
in the registry, absent from the Ask), returns `Ok(60000)` where the RFC
requires `UnknownAxis`. The existing `compute_cost_unknown_axis_rejected`
test misses this because it uses an Ask with an **empty** rate table and
an id in neither place, pinning only the narrow case.

So the fallback is a conformance deviation, not a deliberate safety net.
That does not make it obviously wrong — a default is defensible as a
product decision — but it is a deviation the owner has to make knowingly.

## The one thing that is not ambiguous

> **Corrected at v1.1.** v1.0 claimed that dropping the fallback would
> mean "making unpriced axes genuinely free", and used that to rule the
> option out. That is simply false: removing the fallback does not yield
> zero, it yields `SettlementError::UnknownAxis` — settlement fails loudly.
> The RFC-conformant option was incorrectly excluded from the decision.

Whichever convention is chosen, an advertised price **below** the settled
price is wrong — the two describe the same obligation and are computed
from the same `Ask` in the same crate.

What survives from v1.0 is the genuine free-Ask case: an Ask that
explicitly lists `rate_per_1k = 0` is legitimately free, and an existing
test pins that it must be accepted. Omitting an axis and listing it at
zero are different acts and should stay different.

So the owner's choice is not only about _composition_ — which axes form the
universal figure — but about whether settlement keeps the registry-default
fallback at all.

## What the RFC owner needs to decide

- **Should settlement keep the registry-default fallback at all?** RFC-0959
  §Compute cost says no: an unpriced consumed axis is `UnknownAxis`. This
  is the decision that was missing at v1.0, and the advertised-price
  question cannot be answered independently of it — if the fallback goes,
  every Ask must price every configured axis or fail to settle.
- Should the advertised figure price **all** configured axes, with defaults
  filling the gaps? That is the only option measured here that makes the
  two agree without under-charging, and it presupposes the fallback stays.
- Or should `cost_per_1k` be redefined as something explicitly _not_ a
  settlement estimate — e.g. a lexicographic sort key over listed rates —
  and the field renamed so callers cannot mistake it for a price?
- Should `cached_input_tokens_per_1k` (and any future default) be part of a
  universal advertised figure, given many completions consume no cached
  tokens at all?
- Should custom axes from `with_axes` enter the advertised figure, or only
  axes an Ask explicitly lists?
- Should the facade expose the advertised price and the settled price
  separately, so a buyer-facing quote is never confused with a rank key?

## Reproduction

```bash
# advertised path filters to explicit rates
grep -n -A6 'fn build_unit_consumed' crates/quota-router-core/src/marketplace/mod.rs

# settled path falls back to the axis default
grep -n -A7 'pub fn cost_for' crates/quota-router-storage/src/ask.rs

# the defaults that get skipped
grep -n -A6 'default_rate_per_1k: Dqa::new(60_000' crates/quota-router-storage/src/ask.rs

# absolute advertised-price assertions that would move
grep -rn 'cost_per_1k, 30_000\|best.price, 30_000' crates/quota-router-core/src/marketplace/mod.rs
```

## Cross-references

- RFC-0959 §Compute cost — the fail-closed lookup the fallback narrows
- RFC-0959 Appendix C — "reject unknown axes (fail-closed)"
- RFC-0900 §Settlement Model — the obligation the advertised price
  describes
- RFC-0900 §Dispute Evidence Challenge — anticipates buyer/seller price
  disagreement as a dispute category
- RFC-0959 — the Ask and rate-table shape this pricing comes from
- Mission `ask-axis-ids-are-never-validated-against-the-axis-registry` —
  the produce-side half; a typo becomes an omission, and the omission is
  what this fallback silently prices
- Mission `marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost`
  — the other `put` pricing defect, fixed; this one is about what the
  price _means_
- Mission `raising-an-asks-rate-silently-leaves-the-old-price-in-the-book` —
  the other way a listed price stops reflecting reality
- Mission `the-marketplace-facade-has-no-production-caller` — why nothing
  shipping is mispriced today
