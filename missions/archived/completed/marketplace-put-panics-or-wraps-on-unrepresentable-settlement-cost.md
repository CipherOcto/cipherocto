---
name: marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost
description: "Marketplace::put priced an Ask with an assert_eq! and a bare `as u128` cast. A rate whose settlement cost is not an integer micro-OCTO-W amount panicked the process, and a negative rate wrapped silently to roughly 2^128 and was indexed into the order book as a legitimate ask. Both were reachable from a well-typed Ask, and the panic fired after the repo write had already landed."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost`

## Status

Fixed and verified in both directions.

## The defect

`Marketplace::put` is the marketplace's main write path and returns
`Result<(), RepoError>`. It priced an Ask like this:

```rust
self.repo.put(ask)?;                       // repo write happens FIRST
...
book.place_ask(
    AskSpec { .. },
    {
        assert_eq!(cost.scale, 0, "place_ask: MicroOCTO_W is integer-valued ...");
        cost.value as u128                 // unchecked cast
    },
    ...
);
```

The order book stores a `u128` price, so the cost must be a non-negative
integer number of micro-OCTO-W. Two ways to violate that, both reachable from
a plain `Dqa::new(value, scale)` with no special API:

### 1. Fractional scale — panic

`assert_eq!(cost.scale, 0, ...)` aborts the process. Measured reachability
with a `10_000` rate at increasing scale:

| rate scale | `put()` before fix |
| ---------- | ------------------ |
| 0          | `Ok`               |
| 5          | **panic**          |
| 6          | **panic**          |
| 7          | **panic**          |
| 8          | **panic**          |

`settlement_cost` normalises small scales to 0 (`(10_000, 1) → value=1000
scale=0`), so the assert looked unreachable in casual testing. It is not: at
scale 7 the cost comes back as `value=1 scale=3`.

### 2. Negative rate — silent wrap

`cost.value as u128` on a negative cost wraps. Measured, a rate of `-5_000`
became price **340282366920938463463374607431768206456** (~2^128) and was
indexed into the book as a valid ask.

### 3. Both fire after the repo write

`self.repo.put(ask)?` runs _before_ the pricing. So the panic path left the
Ask persisted in the repository with the order book never updated — a caller
that caught the unwind still had a stored Ask behind it, and the market was
left inconsistent.

## Fix

- New `RepoError::InvalidSettlementCost { scale, value }`. Additive: every
  existing `RepoError::` site in the tree is a construction, not a match, so
  no exhaustive match breaks.
- `put()` now prices the Ask **before any mutation**, and rejects
  `scale != 0` or `value < 0` with that error. The `assert_eq!` and the bare
  cast are gone.

## Acceptance Criteria

- [x] Reachability measured for both cases, not assumed
- [x] No panic on the write path
- [x] No silent `as u128` wrap
- [x] Validation precedes the repo write, so nothing half-lands
- [x] Valid scale-0 Asks still accepted
- [x] Rejected Ask absent from both the book and the repo
- [x] Mutation-verified: all three tests fail against the pre-fix `put`
- [x] Clippy clean, full storage + marketplace suites green

## Verification

Reverting `put()` to the pre-fix body (keeping the new error variant so it
compiles) fails all three new tests, each for the right reason:

| Test                                                      | Pre-fix failure                         |
| --------------------------------------------------------- | --------------------------------------- |
| `put_rejects_fractional_scale_rate_instead_of_panicking`  | panic at the `assert_eq!`               |
| `put_rejects_negative_rate_instead_of_wrapping_to_u128`   | `put` returned `Ok`, `expect_err` fired |
| `put_rejects_before_the_repo_write_so_nothing_half_lands` | panic at the `assert_eq!`               |

Suites after the fix: 108 marketplace lib, 28 e2e, 9 parity, 176
`quota-router-storage`. Clippy `-D warnings` clean on both crates.

## Note carried forward

`Dqa` permits fractional scales and negative values, and `ModelRateTable`
accepts them without validation. Whether the _storage_ layer should reject
them on `Ask` construction — rather than at each consumer — is a separate
question and is not addressed here. This mission only makes the marketplace
fail closed at its own boundary, which is where the panic and the wrap were.

## Cross-references

- RFC-0900 §Market Operations, §Order Book
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade`
- Mission `marketplace-e2e-strong-scenarios` (owner of `marketplace_e2e.rs`)
