---
name: marketplace-nan-reputation-threshold-disables-only-the-async-breaker
description: "set_min_reputation accepts NaN without validation. NaN satisfies neither min <= 0.0 nor min > 0.0, so the two surfaces fall through differently: sync cheapest calls is_excluded where success_rate >= NaN is false and so excludes the provider, while cheapest_with_ranking_async gates on min > 0.0 which is false for NaN and skips exclusion entirely. Measured with a 20x-failing provider: sync returned the healthy provider at 90000, async returned the failing one at 1000."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0968
---

# Mission `marketplace-nan-reputation-threshold-disables-only-the-async-breaker`

## Status

Deferred — the right resolution is a policy choice (reject vs normalise vs
change the signature), and `set_min_reputation` returns `()` so there is
no error channel to report through. The **hazard is documented on the
method**; no behaviour changed.

## Finding

`set_min_reputation(&self, min: f64)` stores the value in both registries
with no finiteness check. Its doc defines only two regimes — "`0.0` (or
any negative value) disables the breaker" and "positive values activate
it" — and `NaN` belongs to neither.

`NaN` fails every ordered comparison:

```text
NaN >  0.0  ->  false
NaN <= 0.0  ->  false
```

So the two read paths, which test the threshold differently, diverge:

| Path                            | Test                                                | Result under `NaN`                                                       |
| ------------------------------- | --------------------------------------------------- | ------------------------------------------------------------------------ |
| sync `cheapest` → `is_excluded` | `min <= 0.0` → fall through to `is_above_threshold` | `success_rate >= NaN` is false, so `!false` → **excluded** (fail-closed) |
| `cheapest_with_ranking_async`   | `min > 0.0` gates whether exclusion runs at all     | false, so the gate is **skipped** → **not excluded** (fail-open)         |

## Measurement

Two asks, `good` at 90_000 with no failures and `bad` at 1_000 with twenty
recorded failures, then `set_min_reputation(f64::NAN)`:

```text
min_reputation() reads back = NaN
NaN > 0.0 = false
NaN <= 0.0 = false
SYNC  cheapest -> Some(price 90000)   <- healthy provider, fail-closed
ASYNC cheapest -> Some(price 1000)    <- 20x-failing provider, fail-open
```

The same threshold value makes one path safer and the other less safe. A
corrupted configuration therefore disables the breaker **only on the
canonical RFC-0968 surface**, and does so silently.

## Reachability

Narrow but real. Nothing in the tree passes `NaN` today, and the only way
in is a caller that computes the threshold. That is plausible in
configuration wiring — `ratio / total` with `total == 0`, or a parser that
accepts `"nan"` — and the setter offers nothing to catch it. There is no
other producer of `min_reputation` in the crate.

Latent in the usual sense: the marketplace has no production callers.

## This completes a pattern

Four public marketplace entry points now share one shape — they take a
value that can be meaningless, and they return a non-`Result` type, so
they cannot reject it:

| Entry point               | Bad value | Effect                                                 | Mission                                                   |
| ------------------------- | --------- | ------------------------------------------------------ | --------------------------------------------------------- |
| `place_ask` / `place_bid` | `qty = 0` | phantom `Some(MatchPair { qty: 0 })`, listing consumed | `marketplace-order-book-accepts-zero-quantity-orders`     |
| `with_axes`               | `vec![]`  | every listing advertises free, settles at full rate    | `marketplace-empty-axes-prices-every-listing-free`        |
| `set_provider_score`      | —         | no counterpart on the canonical surface at all         | `marketplace-set-provider-score-has-no-async-counterpart` |
| `set_min_reputation`      | `NaN`     | sync excludes, async selects — surfaces disagree       | this mission                                              |

`Marketplace::put` is the counter-example: it returns `Result`, and that is
why it can carry `InvalidSettlementCost`, `InvalidAskRate` and now
`UnrepresentableExpiry`. **The entries that can reject bad input are the
ones that return `Result`.**

That is the observation worth taking from these four missions. Individually
each looks like a missing guard; together they are one API-shape decision:
the marketplace's infallible constructors and setters have no vocabulary
for "this input is invalid".

## What the RFC owner needs to decide

- Should `set_min_reputation` return `Result` and reject `NaN`/infinity?
  That matches `Marketplace::put` and closes the class rather than the
  instance.
- Or should a non-finite threshold be normalised — to `0.0` ("breaker
  disabled", matching the documented contract), or to the previous value
  (fail-safe)? Both are silent, which is the part that gives me pause.
- Independently of the setter: should the async gate treat a non-finite
  threshold the same way the sync predicate does, so the two surfaces can
  never disagree regardless of what is stored? Right now they can.
- Is a NaN threshold better surfaced as a startup validation error than as
  a per-request behavioural difference?

## Reproduction

```bash
# the unguarded setter, returns ()
grep -n -A4 'pub fn set_min_reputation' crates/quota-router-core/src/marketplace/mod.rs

# the two divergent tests: min <= 0.0 vs min > 0.0
grep -n 'min <= 0.0\|min > 0.0' crates/quota-router-core/src/marketplace/scoring.rs \
  crates/quota-router-core/src/marketplace/reputation_compat.rs \
  crates/quota-router-core/src/marketplace/mod.rs

# the contrast: put returns Result and carries three typed errors
grep -n 'pub fn put(' crates/quota-router-core/src/marketplace/mod.rs
grep -n 'InvalidSettlementCost\|InvalidAskRate\|UnrepresentableExpiry' \
  crates/quota-router-storage/src/ask_repo.rs | head
```

## Cross-references

- RFC-0900 Gap 7.1 — the circuit-breaker threshold this configures
- RFC-0968 — the canonical store behind the async surface that goes inert
- Mission `marketplace-order-book-accepts-zero-quantity-orders`
- Mission `marketplace-empty-axes-prices-every-listing-free`
- Mission `marketplace-set-provider-score-has-no-async-counterpart`
- Commit `2893a7ae` ("the circuit-breaker was inert on the async path") —
  the dual-write fix that made this setter reach both registries. It has no
  mission file; it landed as a direct fix with its test. That fix made
  this divergence _observable_, by putting the same value into both stores
  so the two gates could be compared
- Mission `the-marketplace-facade-has-no-production-caller` — why nothing
  shipping is affected today
