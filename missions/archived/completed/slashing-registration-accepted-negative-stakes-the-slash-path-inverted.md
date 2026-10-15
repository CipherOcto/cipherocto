---
name: slashing-registration-accepted-negative-stakes-the-slash-path-inverted
description: "SlashingLedger::register validated only the Dqa scale, so a negative stake was accepted. Slashing such a provider reported a negative amount lost, jumped cumulative_loss_pct to 1.0 from a single offense, and moved the stake from -1,000,000 to 0 — the provider gained stake by being slashed. Zero stake is deliberately still accepted; that question is separate."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `slashing-registration-accepted-negative-stakes-the-slash-path-inverted`

## Status

Fixed and verified in both directions.

## The defect

`SlashingLedger::register` gated on scale only:

```rust
require_integer_scale(&initial_stake_micro_octo_w, "initial_stake_micro_octo_w")?;
```

The module already had the stronger guard — `require_positive_integer`, whose
doc reads _"guards against negative inputs (which `subtract` would happily add
to a stake)"_ — and both withdraw paths used it (lines 658, 711). Registration
alone used the weaker check.

Measured, registering `-1_000_000` and then slashing once with
`miss_rate = 1.0`:

```
PROBE register(negative) ACCEPTED stake=-1000000 initial=-1000000
PROBE slash(negative-stake) ok: lost=-1000000 cumulative_loss_pct=1
PROBE after slash: stake=0 offense=1 cum_loss=1 banned=true
```

Three distinct harms in one call:

1. **`SlashOutcome.amount_micro_octo_w = -1_000_000`** — a negative _amount
   lost_. Any settlement that sums slashed amounts would credit instead of
   debit.
2. **`cumulative_loss_pct = 1.0` after a single offense**, because
   `loss_delta = amount / initial = (-1e6) / (-1e6) = 1.0`. The provider is
   instantly permanently banned on its first offense rather than crossing the
   cumulative threshold.
3. **Stake moved from `-1_000_000` to `0`** — the provider _gained_ 1,000,000
   micro-OCTO-W by being slashed.

There is no add-stake path: `register` is the only way to create an entry and
its `or_insert_with` never updates an existing one, so a rejected registration
cannot be worked around by retrying.

## Fix

New `require_non_negative_integer` — scale=0 plus `value >= 0` — used at
registration. It composes the existing `require_integer_scale` and mirrors
`require_positive_integer`, so the three guards now read as one ladder.

## Zero is deliberately still accepted

The first attempt used `require_positive_integer` and broke
`register_accepts_scale_zero`, an existing test that explicitly pins
`Dqa::new(0, 0)` as the canonical zero.

That test documents intent, so the fix was narrowed rather than the test
overridden. There is also a coherent economic reading: a provider that stakes
no collateral has nothing to slash, so never being banned costs it nothing.
Whether a zero-stake provider _should_ be bannable is a design question —
recorded in `a-zero-stake-provider-can-never-be-banned`.

## Acceptance Criteria

- [x] Negative stake refused at registration
- [x] No entry created on rejection
- [x] Zero stake still accepted (existing intent preserved)
- [x] Positive stake still accepted and slashes sanely — positive control
- [x] Pre-existing scale rejection still fires
- [x] Mutation-verified: only the negative-stake test fails without the check
- [x] Clippy `-D warnings` clean

## Verification

Removing the sign check from `require_non_negative_integer` fails exactly one
of four tests:

| Test                                                         | With guard | Without    |
| ------------------------------------------------------------ | ---------- | ---------- |
| `register_rejects_negative_stake`                            | ok         | **FAILED** |
| `register_still_accepts_zero_stake`                          | ok         | ok         |
| `register_still_accepts_a_positive_stake_and_slashes_sanely` | ok         | ok         |
| `register_still_rejects_a_fractional_scale_stake`            | ok         | ok         |

The positive control asserts the ledger invariant rather than a guessed
penalty magnitude — `new_stake == initial_stake - amount_lost`. An earlier
version asserted a literal `0` and failed: a first offense slashes 10% of
1,000,000, leaving 900,000.

Suites: 112 marketplace lib, 28 e2e, 9 parity, 8 storage binaries. Clippy
`-D warnings` clean.

## Cross-references

- RFC-0900 §Slashing Model
- Mission `a-zero-stake-provider-can-never-be-banned`
- Mission `negative-axis-rates-are-refused-at-the-storage-boundary` — the same
  missing-sign-check family on the Ask side
