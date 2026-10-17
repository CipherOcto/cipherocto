---
name: negative-axis-rates-are-refused-at-the-storage-boundary
description: "ModelRateTable and cost_for perform no validation, so a negative rate_per_1k was accepted on every path and propagated into pricing. Such an Ask credits the buyer instead of charging them, wins every cheapest comparison, and passes every budget ceiling by construction. Refused now on write and on read, alongside the ask_id and nonce checks that were already there."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `negative-axis-rates-are-refused-at-the-storage-boundary`

## Status

Fixed and verified in both directions.

## The hazard

`ModelRateTable` is a plain struct of public fields and `cost_for` simply
multiplies. Nothing rejects a negative `rate_per_1k` anywhere in the pricing
path, and `Dqa` permits it — `Dqa::new(-5_000, 0)` constructs fine.

Measured consequences before the fix:

| Consumer                                                | Effect of a negative rate                                         |
| ------------------------------------------------------- | ----------------------------------------------------------------- |
| `Marketplace::put`                                      | refused (`InvalidSettlementCost`, earlier mission)                |
| `quota-router-storage/src/marketplace.rs` budget filter | `dqa_cmp(cost, budget_ceiling) > 0` is false, so it always passes |
| `StoolapAskRepository::cheapest`                        | negative cost always wins                                         |

A negative rate would credit the buyer rather than charge them, and would be
preferred over every honest ask by construction.

**There is nothing to exploit today.** Per mission
`the-marketplace-facade-has-no-production-caller`, the marketplace has no
production caller and the settlement path is not wired, so this is a trap
disarmed before it could be sprung — not an active exploit. Fixing it before
the settlement work lands is the point.

## Fix

- `ModelRateTable::first_negative_rate()` — a small, reusable accessor, with
  the rationale on the doc comment.
- `RepoError::InvalidAskRate { axis, value }`.
- Refused in **both** directions:
  - `StoolapAskRepository::put` — bad data never enters storage
  - `TryFrom<AskRow> for Ask` — protects rows written by another writer or
    an older build, sitting alongside the `ask_id` / `nonce` length checks
    that already failed closed there

Deliberately **not** enforced on fractional scale at this boundary. Storage
compares with `dqa_cmp`, which handles scale correctly; the unrepresentable
case only arises where a `u128` price is required, and
`Marketplace::put` already guards that.

## Acceptance Criteria

- [x] Negative rate refused on write
- [x] Negative rate refused on read, for rows this process did not write
- [x] Zero rate still accepted — a free Ask is legitimate and wins `cheapest`
- [x] Mutation-verified: both rejection tests fail with the guards removed
- [x] No existing storage or marketplace test broken
- [x] Clippy `-D warnings` clean on both crates

## Verification

Removing both guards fails exactly the two rejection tests and leaves the
zero-rate test passing — which is what an anti-over-rejection guard is for:

| Test                                                  | Guards present | Guards removed |
| ----------------------------------------------------- | -------------- | -------------- |
| `put_rejects_negative_rate_on_write`                  | ok             | **FAILED**     |
| `get_rejects_negative_rate_written_by_another_writer` | ok             | **FAILED**     |
| `zero_rate_is_still_accepted`                         | ok             | ok             |

Suites: 8 binaries green in `quota-router-storage`, 108 marketplace lib,
28 e2e, 9 parity. `run-local-checks.sh` 9/9.

## Cross-references

- Mission `the-marketplace-facade-has-no-production-caller` (hazard table)
- Mission `marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost`
  (the fractional-scale half of the same family)
- RFC-0900 §Market Operations
