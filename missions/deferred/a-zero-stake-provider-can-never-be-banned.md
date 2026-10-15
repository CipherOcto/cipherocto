---
name: a-zero-stake-provider-can-never-be-banned
description: "SlashingLedger::slash guards loss_delta to 0.0 when initial stake is zero, so cumulative_loss_pct never rises for a zero-stake provider and is_banned can never fire no matter how many offenses accrue. Zero registration is currently intended behaviour, so this is a design question rather than a bug."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `a-zero-stake-provider-can-never-be-banned`

## Status

Deferred — design question. Not actioned.

## The observation

`SlashingLedger::slash` computes the cumulative loss contribution as:

```rust
let loss_delta = if stake.initial_stake_micro_octo_w.value == 0 {
    0.0
} else {
    amount.value as f64 / stake.initial_stake_micro_octo_w.value as f64
};
stake.cumulative_loss_pct += loss_delta;
```

For a zero-stake provider the guard pins `loss_delta` to `0.0` on every call —
the division is correctly avoided. Consequently `cumulative_loss_pct` stays
`0.0` forever, and since the ban test is
`self.cumulative_loss_pct >= rules.permanent_ban_at`, a zero-stake provider is
**never permanently banned**, regardless of offense count.

`offense_count` does climb, so the record shows the misbehaviour; only the
ban does not fire.

There is no add-stake path — `register` creates the entry once and its
`or_insert_with` never updates it — so the state is permanent.

## Why this is not simply fixed

Zero registration is currently **intended**:
`register_accepts_scale_zero` pins `Dqa::new(0, 0)` as the canonical zero.
A provider that stakes no collateral has nothing to slash, so never being
banned costs it nothing under a collateral reading of the slashing model.

That makes "should a zero-stake provider be bannable?" a genuine policy
question rather than a defect:

- **No** (current): banning someone with no stake is toothless. Cost of
  misbehaviour falls on the buyer, which is exactly what staking is meant to
  prevent.
- **Yes**: a reputation/offense-count ban should apply independently of
  collateral — the provider is removed from routing, it simply has no stake
  to confiscate.

Note this cuts against the module's own fail-closed posture: an unknown
`verifiability` returns `0.0` ("fail-closed", per the registry doc), and a
zero-stake provider gets a permanent, silent exemption from a different
mechanism.

## What the RFC owner needs to decide

- Is the slashing ban purely collateral-based (current behaviour is correct),
  or is there a parallel reputation ban that should fire on offense count?
- If collateral-only, should `register` require a positive stake so the
  permanent exemption is impossible rather than silent? That is a behaviour
  change and would fail `register_accepts_scale_zero`, which is why it was not
  made unilaterally.
- If a reputation ban is intended, does it belong in `SlashingLedger` or in
  the reputation surface (`scoring.rs` / `reputation_compat.rs`)?

## Cross-references

- RFC-0900 §Slashing Model
- Mission `slashing-registration-accepted-negative-stakes-the-slash-path-inverted`
  — the sign half of the same registration guard, fixed there
- Mission `negative-axis-rates-are-refused-at-the-storage-boundary`
