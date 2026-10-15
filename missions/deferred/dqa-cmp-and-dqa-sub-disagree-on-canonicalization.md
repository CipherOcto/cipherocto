---
name: dqa-cmp-and-dqa-sub-disagree-on-canonicalization
description: "dqa_cmp canonicalizes both operands before comparing, but dqa_sub feeds raw operands to align_scales and canonicalizes only its result. So dqa_cmp(a,b) >= 0 does not guarantee dqa_sub(a,b).is_ok(). Three production sites rely on that guarantee through expect('guarded by dqa_cmp >= 0'), including the capability caveat payment drain. Measured end to end: RateLimitBudget::try_deduct panics at ledger.rs. Trigger is a non-canonical Dqa, which Dqa::new permits since it validates only the scale."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0105
    - RFC-0862
---

# Mission `dqa-cmp-and-dqa-sub-disagree-on-canonicalization`

## Status

Deferred — the fix belongs to the Layer A numeric substrate, which is
RFC-frozen and semver-major. Three sites carry an `expect` whose stated
justification is measurably false; all three are annotated here so the
next reader is not misled. No behaviour changed. Measurement probe
removed, tree matches its prior state.

## Finding

Two functions over the same type disagree about what "the same number"
means.

`dqa_cmp` canonicalizes **both operands** first:

```rust
pub fn dqa_cmp(a: Dqa, b: Dqa) -> i8 {
    let a_canonical = canonicalize(a);
    let b_canonical = canonicalize(b);
    ...
}
```

`dqa_sub` canonicalizes only its **result**. Its operands go into
`align_scales` raw:

```rust
pub fn dqa_sub(a: Dqa, b: Dqa) -> Result<Dqa, DqaError> {
    let (a_val, b_val, result_scale) = align_scales(a, b)?;   // raw scales
    ...
    Ok(canonicalize(result))                                    // result only
}
```

and `align_scales` multiplies the lower-scale operand by `10^diff`,
returning `DqaError::Overflow` if that leaves the i64 range:

```rust
let intermediate = (a.value as i128) * (power as i128);
if intermediate > i64::MAX as i128 || intermediate < i64::MIN as i128 {
    return Err(DqaError::Overflow);
}
```

Its own comment says `diff <= 18 after canonicalization` — but nothing in
`dqa_sub` canonicalizes before calling it.

**`Dqa::new` does not canonicalize.** It validates only the scale:

```rust
pub fn new(value: i64, scale: u8) -> Result<Self, DqaError> {
    if scale > MAX_SCALE { return Err(DqaError::InvalidScale); }
    Ok(Self { value, scale })
}
```

So non-canonical values are constructible, and on one of the two paths
the comparison and the subtraction disagree about magnitude.

## The claim that is false

Three production sites assert:

```rust
dqa_sub(x, cost).expect("guarded by dqa_cmp >= 0")
```

| Site                                                    | Path                                                           |
| ------------------------------------------------------- | -------------------------------------------------------------- |
| `quota-router` — `crates/octo-paid-query/src/ledger.rs` | `InMemorySpendLedger::try_deduct`, the production ledger drain |
| `crates/octo-paid-query/src/lib.rs`                     | caveat budget drain                                            |
| `crates/octo-cap-macaroon/src/caveat/payment.rs`        | capability caveat payment drain                                |

The comment states an unconditional guarantee. It holds only when both
operands are canonical.

## Measurement

Two `Dqa` values, one canonical and one not — `(9e18, 18)` canonicalizes
to `(9, 0)`:

```text
MEASURE dqa_cmp(a,b)=1
MEASURE dqa_sub(a,b)=Err(Overflow)
MEASURE guard_holds=false
```

`dqa_cmp` says `a > b`, so the guard passes. `dqa_sub` returns
`Overflow`, because it aligns the **raw** scale-18 operand and lands
outside i64.

End to end, through the production entry point:

```text
MEASURE drain_panicked=true
thread '...' panicked at crates/octo-paid-query/src/ledger.rs:191:44
```

The panic is at the `expect` line, not anywhere else.

> **A first attempt at this measurement did not trigger it** and would
> have produced a false negative. The initial construction used
> `(9_000_000_000_000_000_001, 18)` against `(10, 0)`; `dqa_cmp` returned
> −1, because `(10, 0)` is already canonical and so is the comparison —
> the trigger needs the non-canonical value on the side whose **raw**
> scale is the larger one. The construction was corrected and the
> behaviour confirmed.

## Reachability — stated narrowly

The trigger is a **non-canonical `Dqa` on either operand**. `Dqa::new`
permits one, so the value is constructible and the guard is
demonstrably unsound.

What was **not** established is whether any production path currently
produces a non-canonical `Dqa` reaching one of these three drains. The
arithmetic helpers that mint amounts (`dqa_mul`, `dqa_add`) canonicalize
their results, so computed costs are canonical; values parsed from the
wire were not traced. That question is open, and the mission does not
claim otherwise.

The defect stands regardless of reachability: it is in the Layer A
substrate, and three call sites encode a guarantee that does not hold.

## Why this is the audit's standing theme, exactly

An `expect` whose comment asserts a mathematical invariant that turns out
to be conditional on an unstated precondition. The invariant is stated as
though it were a property of the API. It is not — it is a property of the
_callers_, and the type system does not enforce it.

This is the same shape as the infallible-setter findings: the vocabulary
for "this operand is not in canonical form" does not exist, so the check
cannot be expressed and the invariant is asserted in a comment instead.

## What the RFC owner needs to decide

- Should `dqa_sub` canonicalize its operands, matching `dqa_cmp`? That is
  the one-line fix that makes the three guards sound, and it changes a
  Layer A numeric function.
- Or should `align_scales` canonicalize internally, so every caller is
  safe by construction?
- Or should `Dqa::new` canonicalize, making non-canonical values
  unrepresentable? That is the strongest option and the most expensive,
  since it changes construction semantics everywhere.
- Should the three `expect`s become `Result` propagation regardless? A
  ledger drain that can panic on a malformed amount is fragile even if
  the reachability question is closed.
- Should `align_scales`' "after canonicalization" comment be corrected,
  since it describes a normalisation the caller does not perform?

## Reproduction

```bash
# dqa_cmp canonicalizes inputs
grep -n -A6 'pub fn dqa_cmp' determin/src/dqa.rs

# dqa_sub does NOT — it canonicalizes only the result
grep -n -A12 'pub fn dqa_sub' determin/src/dqa.rs

# the overflow the raw alignment can hit
grep -n -A22 'fn align_scales' determin/src/dqa.rs

# Dqa::new validates scale only
grep -n -A6 'pub fn new(value: i64, scale: u8)' determin/src/dqa.rs

# the three sites asserting the false guarantee
grep -rn 'expect("guarded by dqa_cmp' --include=*.rs crates/
```

## Cross-references

- RFC-0105 — the numeric substrate these types belong to
- RFC-0862 §StoolapSpendLedger substrate — the atomic drain the ledger
  performs, where the panic lands
- Mission `the-persistent-spend-ledger-cannot-be-injected-so-production-runs-in-memory` —
  why `InMemorySpendLedger` is the backend production actually runs
- Mission `two-crates-define-spend-ledger-with-incompatible-schemas` — the
  other blocker on wiring a persistent ledger
