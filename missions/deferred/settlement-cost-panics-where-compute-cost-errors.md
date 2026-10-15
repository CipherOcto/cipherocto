---
name: settlement-cost-panics-where-compute-cost-errors
description: "ask.rs defines settlement_cost and compute_cost over the same Ask, and they disagree on overflow: settlement_cost returns a bare Dqa so an addition overflow panics at its local expect, while compute_cost returns SettlementError::Overflow naming the axis and partial sum. Measured with one Ask: settlement_cost panics, compute_cost returns a diagnostic error. Both marketplace facades and ask_repo use the panicking one, so the advertised-price and cheapest-selection paths cannot report overflow at all."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0900
    - RFC-0105
---

# Mission `settlement-cost-panics-where-compute-cost-errors`

## Status

Deferred — `settlement_cost` returning `Dqa` rather than `Result` is a
public signature change, and its call sites span two crates. No behaviour
changed; the finding is documented at the site. Measurement probe
removed, tree matches its prior state.

## Finding

`crates/quota-router-storage/src/ask.rs` contains two functions that
compute the same thing from the same `Ask`, with incompatible error
handling.

```rust
pub fn settlement_cost(ask: &Ask, consumed: &[AxisConsumption], axes: &[PricingAxis]) -> Dqa {
    let mut total: Dqa = Dqa::new(0, 0).expect("zero");
    for (axis_id, units) in consumed {
        if let Some(c) = ask.rates.cost_for(axis_id, *units, axes) {
            total = dqa_add(total, c);     // <- local wrapper, expects internally
        }
    }
    total
}
```

```rust
pub fn compute_cost(ask: &Ask, axes_consumed: &AxesConsumed, registry: &[PricingAxis])
    -> Result<Dqa, SettlementError>
{
    ...
    total = octo_determin::dqa_add(total, rate).map_err(|_| SettlementError::Overflow {
        axis_id: axis_id.clone(),
        partial_sum: total,
    })?;
}
```

The difference is a local helper defined a few hundred lines above:

```rust
pub fn dqa_add(a: Dqa, b: Dqa) -> Dqa {
    octo_determin::dqa_add(a, b).expect("Dqa addition overflow")
}
```

Its doc says the crate "prefers the `expect` form for known-bounded
supply-side calculations". **The bounds are not known.** `cost_for`
multiplies units by a rate, both caller-supplied, and the sum is over
every consumed axis.

Because `settlement_cost` returns a bare `Dqa`, a caller has no way to
handle an overflow. The only two outcomes are a panic or a wrong number,
and the implementation picks the panic.

## Measurement

One `Ask` with two axes at `9_000_000_000_000_000_000` each, consumed at
1000 units. True total `1.8e19`, above `i64::MAX`.

```text
MEASURE settlement_cost_panicked=true
  thread panicked at crates/quota-router-storage/src/ask.rs:116:34
MEASURE compute_cost=Err(overflow in compute_cost for axis
  `output_tokens_per_1k` (partial sum = Dqa { value: 9000000000000000000, scale: 0 }))
```

Same `Ask`, same axes, same consumption. One panics with a bare
`Dqa addition overflow`; the other names the offending axis and the
partial sum.

## Which one the marketplace uses

`settlement_cost` — the panicking one — is what every pricing surface
calls:

| Call site                                                             | Surface                                           |
| --------------------------------------------------------------------- | ------------------------------------------------- |
| `quota-router-core/src/marketplace/mod.rs` (two sites)                | facade cost, `cheapest` / `cheapest_with_ranking` |
| `quota-router-storage/src/marketplace.rs`                             | the storage-side marketplace facade               |
| `quota-router-storage/src/ask_repo.rs` (two sites, in one comparison) | cheapest-ask selection                            |

`compute_cost` — the one that reports overflow — is used only by the
settlement receipt path.

So the advertised price and the cheapest-selection ranking both run
through a function whose signature makes overflow unreportable. `Settled`
reasons that a listed price is wrong; here the mechanism by which it
would be wrong is a process abort rather than a returned error.

That last call site is worth noting: `ask_repo.rs` computes
`settlement_cost` for both sides of one comparison, so a single
overflowing listing can abort a comparison in progress.

## Also: `dqa_add` shares the defect from the Layer A finding

The `expect` is reachable even without a genuine numeric overflow,
because `dqa_add` routes through the same raw-scale `align_scales` that
`dqa_cmp` avoids:

```text
MEASURE canonical dqa_add => Err(Overflow)
MEASURE local_dqa_add_wrapper_panicked=true
```

for operands `10 @ scale 0` and `9e18 @ scale 18` — a true sum of **19**.
The second operand is non-canonical (it canonicalizes to `9 @ 0`), and
`Dqa::new` permits it.

That route is **not** reachable through `cost_for`, because
`Dqa::multiply` delegates to `dqa_mul`, which canonicalizes its result.
So the spurious-overflow path is blocked at the marketplace entry and
the genuine-overflow path is what is live. Stated precisely, because the
distinction matters: the panic is not reachable by feeding an Ask a
malformed rate and getting a spurious overflow; it is reachable by
legitimately large rates, because nothing bounds them.

See `dqa-cmp-and-dqa-sub-disagree-on-canonicalization` for the substrate
defect this shares. `dqa_add` and `dqa_sub` are the only two callers of
`align_scales`, and neither canonicalizes its operands.

## Severity today: latent, but for a different reason than usual

The marketplace facade has no production callers, so no listing is being
mispriced today. That is the standing caveat and it still holds.

What is different here is that the failure is a **panic rather than a
wrong number**. Every other marketplace finding in this audit degrades
to a wrong-but-quiet result. This one aborts the process, which in a
node serving other requests means losing the whole node, not just
mispricing one listing.

## What the RFC owner needs to decide

- Should `settlement_cost` return `Result<Dqa, SettlementError>` to match
  `compute_cost`? That is the obvious fix and it touches four call sites
  across two crates.
- Or should the two functions be unified into one? They differ in input
  shape (`&[AxisConsumption]` vs `&AxesConsumed`) and in error handling,
  and having both is the §11 "no parallel abstractions" problem.
- Should rates be bounded at publication so an Ask can never carry a rate
  that overflows a per-1k sum? RFC-0959 constrains the wire type to a
  `u128` scaled value; whether a per-listing ceiling is warranted is an
  economics decision.
- If both functions are kept, should the panicking wrapper
  (`dqa_add` -> `Dqa`) be removed from `ask.rs` so callers are forced to
  handle the error? Its doc asserts the bounds are known; nothing
  establishes that.
- Should the local `dqa_add` / `dqa_mul` wrappers exist at all, given the
  crate also uses `octo_determin::dqa_add` directly a few lines away with
  opposite error handling?

## Reproduction

```bash
# the panicking wrapper
grep -n -B3 -A3 'pub fn dqa_add(a: Dqa, b: Dqa) -> Dqa' \
  crates/quota-router-storage/src/ask.rs

# settlement_cost has no error path
grep -n -A8 'pub fn settlement_cost' crates/quota-router-storage/src/ask.rs

# compute_cost has one
grep -n -B2 -A5 'octo_determin::dqa_add(total, rate)' \
  crates/quota-router-storage/src/ask.rs

# who calls which
grep -rn 'settlement_cost(' --include=*.rs crates/quota-router-core/src/marketplace/
grep -rn 'settlement_cost(' --include=*.rs crates/quota-router-storage/src/

# both are the only align_scales callers, both uncanonicalized
grep -n 'align_scales(' determin/src/dqa.rs
```

## Cross-references

- RFC-0959 §Compute cost — the algorithm both functions implement
- RFC-0900 §Market Operations — where cheapest selection consumes the cost
- RFC-0105 — the numeric substrate, and the source of the shared defect
- Mission `dqa-cmp-and-dqa-sub-disagree-on-canonicalization` — the
  substrate half, which this extends to `dqa_add`
- Mission `marketplace-advertised-price-ignores-default-rate-fallback` —
  the other reason an advertised price can disagree with a settled one
- Mission `the-marketplace-facade-has-no-production-caller` — the caveat
  that keeps this latent
