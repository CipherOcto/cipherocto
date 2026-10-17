---
name: reputation-score-cannot-express-a-provider-worse-than-neutral
description: "The 0-100 presentation formula is (clamp(score, -1, 1) + 1) * 50, which assumes a signed score. It is fed the legacy unsigned success_rate, which only ever occupies [0, 1], so the clamp never engages and the observable floor is 50. A provider that has failed every recorded request presents as 50/100."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0968
---

# Mission `reputation-score-cannot-express-a-provider-worse-than-neutral`

## Status

Deferred — depends on which reputation statistic is canonical. The false
doc claim it surfaced _is_ corrected here.

## The mismatch

`entry_from_order` computes the presentation score as:

```rust
let clamped = s.success_rate.clamp(-1.0, 1.0);
let raw = (clamped + 1.0) * 50.0;
```

`(x + 1) * 50` maps a **signed** `[-1, 1]` onto `[0, 100]`. That is correct
for RFC-0968's `score_ewma`, which is a signed EWMA over outcome deltas.

But the value fed in is `scoring::ProviderScore::success_rate`, from the
legacy in-memory registry. In `scoring.rs::record`:

```rust
let s = if success { 1.0 } else { 0.0 };
entry.success_rate = EWMA_ALPHA * s + (1.0 - EWMA_ALPHA) * entry.success_rate;
```

`s` is only ever `0.0` or `1.0`, so `success_rate` only ever occupies
`[0, 1]`. **The `clamp(-1.0, 1.0)` never engages**, and the observable range
is `[50, 100]`, not `[0, 100]`.

## Measured

Through `cheapest`:

| provider state           | `reputation_score_0_100` |
| ------------------------ | ------------------------ |
| no recorded outcome      | `None`                   |
| first outcome a success  | `Some(100)`              |
| first outcome a failure  | `Some(50)`               |
| 200 consecutive failures | `Some(50)` — floored     |

A provider that has failed every single recorded request presents to a
listing UI as **50 / 100**: mid-reputation, not bad.

The lower half of the advertised "0-100 Reputation Score" scale is
unreachable. RFC-0968 §22 presents it as a 0-100 score; a consumer reading
that would read 50 as "average", when it is the floor.

## Why deferred

The formula is not wrong — it is being fed the wrong quantity. Two
consistent end states:

1. **Feed it a signed score.** Once the compat path is canonical, source
   `score_ewma` (`Dfp`, signed `[-1, 1]`) and the existing formula is
   correct as written. This is the intended end state, and it is blocked on
   `the-reputation-store-implements-a-running-mean-not-the-rfc-ewma` —
   which statistic is canonical decides what gets fed in.
2. **Rescale for the unsigned source.** Map `[0, 1]` onto `[0, 100]` directly.
   Quick, but discards the signed semantics the RFC intends and would need
   redoing at cutover.

Picking between them is a representation decision for the reputation owner.

Note also that the canonical async path emits `None` for this field entirely
(`entry_from_order_async`), with an honest comment naming mission
`marketplace-caller-await-migration`. So the score that listing UIs would
render is, today, produced only by the legacy shadow.

## Corrected here

The `MarketplaceEntry` doc claimed `Some(100)` for unknown providers, "legacy
unknown = perfect semantics; a non-zero presentation keeps the field
populated". Measured, an unobserved provider yields `None` and the field is
skipped in serialisation. The doc now states the measured behaviour.

## Acceptance Criteria

- [x] The false `Some(100)` doc claim corrected to the measured values
- [x] Pinned by `reputation_score_population_matches_the_documented_range`
- [x] The 50-floor pinned by
      `reputation_score_cannot_represent_a_provider_below_neutral`
- [ ] Decide: feed a signed `score_ewma`, or rescale the unsigned source

## Cross-references

- RFC-0968 §22 (0-100 presentation), §6 EWMA Algorithm
- Mission `the-reputation-store-implements-a-running-mean-not-the-rfc-ewma`
- Mission `dual-read-parity-asserts-a-divergence-bound-it-never-measures`
- Mission `marketplace-caller-await-migration` (async field still `None`)
