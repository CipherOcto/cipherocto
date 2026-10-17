---
name: expired-asks-stayed-matchable-and-hydration-hid-the-same-price-panic
description: "The order book stored no expiry, so an Ask indexed while live stayed a candidate forever — measured, cheapest() still returned it after expires_at_unix passed. Separately, open_path_with_store carried the same unchecked assert and as-u128 cast that were fixed in put, on the read path, where persisted rows make it strictly more exposed."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `expired-asks-stayed-matchable-and-hydration-hid-the-same-price-panic`

## Status

Fixed and verified in both directions.

## Defect 1 — an expired Ask stayed matchable indefinitely

`put` gates on expiry only at insert time:

```rust
if ask.expires_at_unix > now { ... book.place_ask(...) }
```

The order book stored no expiry, so `cheapest_with_ranking` could not filter
on read. `asks_matching` was `self.asks.values().filter(spec_pred)` with no
liveness term.

The existing test `cheapest_via_orderbook_skips_expired` gave false
confidence: it only asserts that an Ask _already_ expired at `put` time was
never indexed. It never covers an Ask that expires _after_ insertion.

Measured. Put an Ask expiring 1s later, wait 2.5s, read:

```
PROBE immediately after put: cheapest=Some(10000)
PROBE after expiry: cheapest=Some(10000) now_expired=true
```

Still returned, after expiry, from both the price-only and latency-ranked
paths. A quota marketplace will route to a provider whose listing has lapsed.

### Fix

- `AskSpec` gains `expires_at_unix`, populated at both construction sites
  (`put` and hydration).
- **Every** reader of the book filters
  `spec.model == model && spec.expires_at_unix > now`.

There are exactly two such readers, and the first pass of this fix caught
only one. `cheapest_with_ranking_async` — the canonical async surface —
omitted the term entirely and was found only by re-grepping for book reads
after the change landed. Both now filter, and a `grep` for `asks_matching`
shows no unfiltered reader remains in `marketplace/`.

The same one-of-two mistake also produced Defect 2 below. Two separate
instances of patching a duplicated logic path without enumerating both
copies is what motivated enumerating readers explicitly rather than
patching the one a test happened to reach.

## Defect 2 — the read path still had the price panic

`put` was fixed in mission
`marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost`. Its
sibling `open_path_with_store` was not, and still carried:

```rust
assert_eq!(cost.scale, 0, "place_ask: MicroOCTO_W is integer-valued ...");
cost.value as u128
```

Only one of two call sites was corrected the first time. Hydration is the
**more** exposed of the two: `put` validates before the repo write, so a bad
Ask never enters storage, whereas hydration reads rows already persisted —
including any written before the storage-boundary guard landed, or by another
writer.

### Fix

Same guard as `put`: return `RepoError::InvalidSettlementCost { scale, value }`
on `scale != 0 || value < 0`. Failing the open is deliberate and consistent
with the read-side storage boundary: silently skipping a poisoned row would
quietly under-report available providers.

## Acceptance Criteria

- [x] Expiry stored in the book and filtered at read
- [x] Both the price-only and latency-ranked paths honour it
- [x] Filtering is per entry — a live sibling ask is still returned
- [x] Hydration refuses an unrepresentable persisted price
- [x] Mutation-verified: both fixes fail when reverted
- [x] No existing test broken

## Verification

| Mutation                                              | Result                                             |
| ----------------------------------------------------- | -------------------------------------------------- |
| Expiry term removed from the read filter              | both expiry tests **FAILED**                       |
| Hydration guard reverted to the original `assert_eq!` | hydration test **FAILED**, panicking at the assert |

Suites: 112 marketplace lib, 31 e2e, 9 parity, 8 storage binaries. Clippy
`-D warnings` clean.

## Method note

The first attempt at mutation 1 was a **silent no-op**: the `sed` expression
used `|` as its delimiter while the pattern itself contained `||`, so it
never matched and the tests passed against unmodified code. Verified by
asserting the target pattern occurred exactly once before replacing it. A
mutation that cannot fail proves nothing, including when it agrees with you.

## Cross-references

- RFC-0900 §Listing Lifecycle, §Market Operations
- Mission `marketplace-put-panics-or-wraps-on-unrepresentable-settlement-cost`
- Mission `negative-axis-rates-are-refused-at-the-storage-boundary`
- Mission `raising-an-asks-rate-silently-leaves-the-old-price-in-the-book` —
  the sibling defect: the book has no removal path, so entries accumulate
  even now that expiry makes them ineligible
