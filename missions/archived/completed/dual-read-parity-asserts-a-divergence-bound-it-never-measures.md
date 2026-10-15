---
name: dual-read-parity-asserts-a-divergence-bound-it-never-measures
description: "The dual-read parity suite in marketplace_reputation_async.rs asserts its numeric divergence bound on two degenerate fixtures (all-success, all-failure) where the legacy and compat paths saturate to identical values by construction, so the assertion cannot fail from EWMA drift. On mixed input — where the two formulas genuinely differ — the measured divergence reaches 0.357, three and a half times the 0.10 the module header claims, and nothing measures it."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0968
---

# Mission `dual-read-parity-asserts-a-divergence-bound-it-never-measures`

## Status

Implemented and verified in both directions.

## The defect

`crates/quota-router-core/tests/marketplace_reputation_async.rs` opens with a
dual-read parity contract, then asserts it only where the contract cannot fail.

The two reputation paths are structurally different:

- legacy `scoring::ProviderReputationRegistry` — a decayed `f64` EWMA over
  `success_rate`
- compat `ProviderReputationRegistryCompat` — per-signal `score_ewma` via
  `Dfp`, exposed as a plain success ratio

Both numeric-divergence assertions sit on **all-success** and **all-failure**
fixtures. On those, the legacy EWMA saturates to exactly `1.0` / `0.0` and the
compat ratio is exactly `10/10` / `0/10`, so `delta == 0.0` identically — the
two agree for a reason that has nothing to do with parity.

Three different tolerances were documented for one assertion pair:

| Source        | Claimed bound               |
| ------------- | --------------------------- |
| module header | `parity > 0.999`            |
| module header | `Δ ≤ 0.10 after 10 records` |
| code          | `Δ ≤ 0.5`                   |

## Measured reality

A probe over mixed fixtures, feeding both paths identical records:

| successes / 10 | legacy | compat | delta     |
| -------------- | ------ | ------ | --------- |
| 8              | 0.490  | 0.800  | 0.310     |
| 7              | 0.343  | 0.700  | **0.357** |
| 5              | 0.168  | 0.500  | 0.332     |
| 3              | 0.082  | 0.300  | 0.218     |
| 1              | 0.040  | 0.100  | 0.060     |

Two consequences:

1. The header's `Δ ≤ 0.10` is **false by a factor of 3.6**. Measured worst case
   on mixed input is 0.357.
2. `compat.success_rate` equals `successes / total` exactly in every row — it is
   a plain ratio, not an EWMA. The header's rationale ("slightly different
   formulas ... not bitwise equality") describes an intent the code does not
   exhibit.

## Fix

- Add `dual_read_mixed_outcome_divergence_within_measured_bound`, which runs
  the mixed fixtures where the two formulas actually differ and asserts the
  bound against the measured worst case rather than a documented aspiration.
- Re-describe the two saturation fixtures for what they genuinely prove: that
  both paths agree at the extremes. They are kept — that is a real property —
  but they are no longer presented as the parity proof.
- Correct the module header to state the measured relationship instead of the
  `0.999` / `0.10` figures it never enforced.

## Acceptance Criteria

- [x] Measure the real legacy-vs-compat divergence on mixed input
- [x] Add a mixed-input assertion that fails when the two paths drift apart
- [x] Mutation-verify the new bound is load-bearing (a forced drift fails it)
- [x] Header no longer claims `0.999` parity or `Δ ≤ 0.10`
- [x] Saturation fixtures retained and re-described
- [x] Full marketplace suite green

## Note carried forward — NOT actioned here

RFC-0968 §Parity metric gates retirement of the legacy stores on
`parity_score > 0.999`, where `matches` counts `(did, kind, layer)` triples
whose in-memory and persisted aggregates agree **within `1e-6`**, sustained 24h.

On this surface the two paths differ by up to `0.357` on ordinary mixed
outcomes, so they would not count as a match under that definition. The RFC's
own checkboxes for both ACs are still unchecked, so this is consistent with
the RFC being incomplete — but the gap is structural, not a rounding margin,
and closing it is an RFC-owner decision, not a test change.

No RFC, mission status, or archived record was modified for this mission.

## Cross-references

- RFC-0968 §Parity metric, §RFC-0968-A1.19
- Mission `marketplace-facade-reputation-async-migration` (the compat adapter
  this suite covers)
- Mission `marketplace-cheapest-with-ranking-async` (the mixed ranking fixture
  that deliberately desynchronises the stores via `set_provider_score`)
