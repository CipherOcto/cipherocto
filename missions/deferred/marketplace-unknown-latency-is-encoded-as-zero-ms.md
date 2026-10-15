---
name: marketplace-unknown-latency-is-encoded-as-zero-ms
description: "Both selection paths resolved a provider with no latency observation to 0 ms, the fastest value on the scale, so an unmeasured provider scored a perfect latency component and won LatencyRanking::prefer_latency outright. Measured: a provider quoting 90000 with no history beat a proven 50 ms provider quoting 1000, on both the sync and async paths, inverting cheapest. Encoding corrected to worst-observed (fail closed); the midpoint and exclusion readings remain an RFC-owner question because they change cold-start behaviour."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `marketplace-unknown-latency-is-encoded-as-zero-ms`

## Status

Partially actioned. The encoding defect is **fixed** — absent latency no
longer resolves to `0`. The _replacement policy_ for cold-start providers is
an RFC-owner question and remains open; see §What the RFC owner needs to
decide.

## Finding

`LatencyRanking::prefer_latency()` weights latency at 0.7 against price at
0.3, after min-max normalizing both across the candidate set. Every
provider was assigned a latency before normalization, and the assignment
for a provider with no reputation history was:

- sync `cheapest_with_ranking` — `self.reputation.score(did).map(|s| s.latency_ms).unwrap_or(0)`
- async `cheapest_with_ranking_async` — `latencies.get(did).copied().unwrap_or(0)`,
  where the stored `u64` had itself already been folded to `0` inside
  `reputation_compat::score`

`0` is the **fastest value the scale admits**. An unmeasured provider
therefore received `latency_norm = 0.0` — a perfect latency score — for
the exact property it had no evidence about.

### Measured consequence

Two providers, same model, breaker off:

| Provider   | Price  | Latency               |
| ---------- | ------ | --------------------- |
| `unknown`  | 90_000 | no records            |
| `measured` | 1_000  | 10 successes at 50 ms |

```text
SYNC  cheapest()          -> 1_000    (measured)   correct
SYNC  prefer_latency()    -> 90_000   (unknown)    inverted
ASYNC prefer_latency()    -> 90_000   (unknown)    inverted
```

`prefer_latency` selected the provider costing **90x more**, because the
only thing it had in common with the winner was having never been measured.
Normalization bounds: `min_lat = 0, max_lat = 50`, so `unknown` scored
`0.3 * 1.0 + 0.7 * 0.0 = 0.3` against `measured`'s
`0.3 * 0.0 + 0.7 * 1.0 = 0.7`.

This is a **Sybil-shaped** property: the cheapest way to win the
latency-weighted ranking is to have no history. It also made the two
selection paths agree with each other while both were wrong, so the
sync/async parity fixtures could never have caught it — every one of them
gives every candidate an explicit score.

## Actioned in this mission

1. **`scoring::resolve_ranking_latencies`** — one shared helper, used by
   both paths, taking `&[Option<u64>]` and returning the resolved vector
   plus normalization bounds. Absent observations resolve to the **worst
   latency observed** among the candidates. When no candidate has an
   observation, all resolve to `0` with degenerate bounds and the
   composite degrades to price-only.

   One implementation rather than two: the defect existed precisely
   because two call sites each open-coded the same `.unwrap_or(0)`, and a
   parallel helper would reintroduce the failure mode this fixes.

2. **`reputation_compat::observed_latency_ms`** — new accessor returning
   `Option<u64>`, so the async path can tell "no observation" from
   "measured 0 ms". It matches on `AggregateNotFound` only and propagates
   every other error, so a connectivity fault is never misreported as
   "no data" — the same discipline `read_outcome` and
   `read_latency_aggregate` already used.

3. **Both call sites** now thread `Option<u64>` through to normalization
   instead of collapsing to `0` early. The async `latencies` map changed
   from `HashMap<String, u64>` to `HashMap<String, Option<u64>>`, and
   `entry_from_order_async` now receives `None` rather than a fabricated
   `Some(0)` for an unmeasured provider.

4. **Doc comment on `cheapest_with_ranking_async`** — corrected two claims
   that were never established and are demonstrably false:
   - "returns the same ordering ... when the dual-read parity >= 0.999
     holds in prod" — no 0.999 bound exists anywhere; the measured worst
     `success_rate` divergence is **0.357**.
   - "On synthetic fixtures the two paths agree by construction" — mixed
     fixtures diverge by construction, which is why
     `dual_read_mixed_outcome_divergence_within_measured_bound` exists.
   - "the mapping `(score_ewma.to_f64() as u64)` matches the legacy shadow
     for non-pathological scores" — measured latency deltas on identical
     input: 1 ms (uniform 75), 4 ms (0..9 ramp), **14 ms** (alternating
     100/300), because legacy is a decayed EWMA and compat is an
     `alpha = 1/(n+1)` running mean.

## Verification

Both new tests were mutation-verified against the pre-fix encoding.

- `unmeasured_provider_does_not_rank_as_zero_latency` **fails** when
  `resolve_ranking_latencies` is mutated back to `unwrap_or(0)`, with
  `left: 90000, right: 1000` — the exact inversion, on the sync path.
  Reverted, suite re-confirmed green.
- `unmeasured_provider_still_wins_on_price_when_cheapest` is the
  **positive control**, and it kept passing under that same mutation. That
  matters: it is what distinguishes "absent data no longer outranks
  measured data" from the degenerate alternative "providers without an
  observation are refused entirely", which would lock every cold-start
  provider out of the book while also making the hazard test pass.

No pre-existing test pinned the old encoding — every prior
`prefer_latency` fixture assigns an explicit score to every candidate — so
no test documenting intent was overridden.

Full `quota-router-core` suite green (1755 lib + all integration binaries);
clippy with warnings denied clean; `run-local-checks.sh` 9 of 9.

## Why worst-observed, and not the alternatives

Three encodings were available for "no observation". They are not
equivalent, and the choice is visible in cold-start behaviour:

| Encoding           | Effect                                                 | Sybil-safe | Cold-start |
| ------------------ | ------------------------------------------------------ | ---------- | ---------- |
| **worst** (chosen) | unknown ranks last on latency, still eligible on price | yes        | preserved  |
| midpoint           | unknown is neutral on latency, decided on price        | no         | preserved  |
| exclusion          | unknown is dropped from latency ranking entirely       | yes        | **lost**   |
| `0` (previous)     | unknown ranks first on latency — the defect            | no         | inverted   |

Worst-observed was chosen because it is the only encoding under which
**having no reputation history is not an advantage**, which is the
property the defect violated. It also keeps `prefer_latency` from silently
becoming price-only for cold-start providers, which exclusion would do.

Midpoint was the closest runner-up and is arguably fairer to new providers,
but it leaves a provider that has _never been served_ competing on equal
terms with one that has. Exclusion was rejected because it would make
`prefer_latency` unusable for exactly the providers that most need a first
routing decision.

This is a policy choice with a defensible alternative, which is why it is
recorded rather than treated as settled.

## Severity

Real, and worse than the other reputation defects found in this area,
because it is **not** a sync/async divergence — both paths were equally
wrong, so no parity test could ever surface it. It becomes live the moment
`prefer_latency` is used for routing, and its effect is silent: the
marketplace looks like it is routing on latency and is in fact routing on
price-inverted-by-absence-of-data.

Mitigating: the facade has zero production callers (mission
`the-marketplace-facade-has-no-production-caller`), and the live routing
path uses `scorer.rs` / `ProviderCapacity` rather than the marketplace.

## What the RFC owner needs to decide

- Is worst-observed the right fail-closed default, or should unmeasured
  providers be neutral (midpoint) so cold-start is not penalised twice —
  once for having no data and again for ranking last?
- Should `prefer_latency` refuse to rank at all until every candidate has
  an observation, or rank on price alone with an explicit
  "insufficient data" signal on the returned `MarketplaceEntry`?
- Should `MarketplaceEntry` carry the distinction explicitly — e.g. an
  `Option<u64>` latency is already implied, but is a consumer expected to
  treat `None` as worst, as unknown, or as an error?
- Is there an onboarding requirement for new listings — must a provider
  serve N completions before it enters the latency-weighted ranking? That
  would make the question moot by guaranteeing an observation for every
  candidate.

## Reproduction

```bash
# the two call sites that open-coded the same collapse
grep -n -B2 -A6 'resolve_ranking_latencies' \
  crates/quota-router-core/src/marketplace/mod.rs
grep -n -A12 'pub fn resolve_ranking_latencies' \
  crates/quota-router-core/src/marketplace/scoring.rs

# the accessor that distinguishes absent from measured-zero
grep -n -A12 'pub async fn observed_latency_ms' \
  crates/quota-router-core/src/marketplace/reputation_compat.rs

# the inversion, and its positive control
cargo test -p quota-router-core --test marketplace_reputation_async \
  unmeasured_provider -- --nocapture
```

## Cross-references

- RFC-0900 Gap 7.2 (latency-aware ranking — recorded in the version-history
  table; RFC-0900 has no section of its own for it)
- Mission `the-reputation-store-implements-a-running-mean-not-the-rfc-ewma` —
  why the two latency aggregates differ at all, and the source of the
  corrected 14 ms claim
- Mission `dual-read-parity-asserts-a-divergence-bound-it-never-measures` —
  the fixtures that assign explicit scores, and so cannot catch this
- Mission `the-marketplace-facade-has-no-production-caller` — why nothing
  shipping is affected today
