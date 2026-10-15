---
name: the-reputation-store-implements-a-running-mean-not-the-rfc-ewma
description: "RFC-0968 section 6 specifies update_ewma(prev, delta, alpha) with a caller-supplied alpha validated to (0,1] plus PrevNonFinite and DeltaOutOfRange guards. No such function exists in the codebase, and none of those three error variants do either. What the three stores actually run is an unweighted running mean with alpha derived internally as 1/(n+1)."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0968
---

# Mission `the-reputation-store-implements-a-running-mean-not-the-rfc-ewma`

## Status

Deferred — divergence between an Accepted RFC and the implementation. The
choice between implementing the RFC and amending the RFC is an owner
decision, not a bug fix.

## What the RFC specifies

RFC-0968 §6 gives `update_ewma` in full:

```rust
pub fn update_ewma(
    prev: octo_determin::Dfp,
    delta: octo_determin::Dfp,
    alpha: octo_determin::Dfp,
) -> Result<octo_determin::Dfp, ReputationError>
```

with range checks that return `ReputationError::AlphaOutOfRange` (alpha in
`(0, 1]`), `PrevNonFinite`, and `DeltaOutOfRange` (delta in `[-1, 1]`), plus a
weight of `clamp(|delta|, 0, 1)`. It is described as bit-deterministic Dfp
arithmetic per RFC-0104.

## What exists

| Artifact               | Status                                       |
| ---------------------- | -------------------------------------------- |
| `update_ewma` function | **does not exist** anywhere in the workspace |
| `AlphaOutOfRange`      | **does not exist**                           |
| `PrevNonFinite`        | **does not exist**                           |
| `DeltaOutOfRange`      | **does not exist**                           |

The only mentions of `update_ewma` in Rust source are two doc comments in
`determin/src/dcmp.rs` and `determin/src/dfp_compare.rs`, both saying the
helpers are _"kept for callers — including RFC-0968's `update_ewma`"_. The
determin crate reserves the primitives; the function was never written.

What the three persistence/compat paths actually compute:

| Site               | Code                                                              |
| ------------------ | ----------------------------------------------------------------- |
| `store/memory.rs`  | `let alpha = 1.0 / (n + 1.0);` then `cur*(1-alpha) + delta*alpha` |
| `store/stoolap.rs` | same                                                              |
| `compat/legacy.rs` | same                                                              |

`alpha = 1/(n+1)` is the classic incremental **running arithmetic mean**. It
converges exactly to the mean of all deltas and weights a 100-event-old
observation equally with the newest.

## Measured consequence

Writing only through the compat path (no legacy write), 10 events, first N
successful:

| successes | compat `success_rate` | ratio |
| --------- | --------------------- | ----- |
| 8         | 0.8                   | 0.8   |
| 7         | 0.7                   | 0.7   |
| 5         | 0.5                   | 0.5   |

Exactly the ratio, every time — the signature of a running mean, not an EWMA.

Against the legacy registry's genuine decayed EWMA (`EWMA_ALPHA = 0.3`) on the
same inputs:

| successes | legacy EWMA | compat running mean | Δ         |
| --------- | ----------- | ------------------- | --------- |
| 8         | 0.490       | 0.800               | 0.310     |
| 7         | 0.343       | 0.700               | **0.357** |
| 5         | 0.168       | 0.500               | 0.332     |

## This is the root cause of the recorded parity divergence

Mission `dual-read-parity-asserts-a-divergence-bound-it-never-measures`
measured this divergence and attributed it to "two EWMA formulas with slightly
different shapes". That description is wrong, and this mission corrects it:
one side is a decayed EWMA, the other is a running mean. They are different
statistics, not different parameterisations of the same one.

The parity suite's own module header carries the same wrong description. It is
corrected as part of the parity mission; this mission records why.

## Why deferred

Two legitimate resolutions, and the choice is not mine:

1. **Implement §6 as written** — caller-supplied alpha, validated inputs,
   delta-weighted. Requires adding `update_ewma` and the three error variants,
   and deciding the alpha value and the re-aggregation story for aggregates
   already written by the running mean.
2. **Amend the RFC** to describe the running mean, which is internally
   consistent across all three sites and simpler.

Option 1 additionally has to reckon with §7 parity, which was written against
the EWMA semantics.

## What the RFC owner needs to decide

- Is the canonical reputation statistic a decayed EWMA or an unweighted
  running mean? Everything else keys off that answer.
- If EWMA: what is alpha, is it configurable per signal, and how are existing
  running-mean aggregates migrated?
- If running mean: RFC-0968 §6 and the `determin` doc comments need amending,
  and the `update_ewma` reservation in `determin` becomes dead weight.

## Reproduction

```bash
# the RFC's function and error variants do not exist
grep -rn 'fn update_ewma' --include=*.rs crates/ determin/
grep -rn 'AlphaOutOfRange\|DeltaOutOfRange\|PrevNonFinite' --include=*.rs crates/

# what actually runs
grep -rn '1.0 / (n + 1.0)' --include=*.rs crates/
```

## Cross-references

- RFC-0968 §6 EWMA Algorithm, §7 Adapter Mapping Rules
- Mission `dual-read-parity-asserts-a-divergence-bound-it-never-measures`
- Mission `the-marketplace-facade-has-no-production-caller`
