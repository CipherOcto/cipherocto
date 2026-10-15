---
name: marketplace-set-provider-score-has-no-async-counterpart
description: "set_provider_score writes only the legacy ProviderReputationRegistry and the gap cannot be closed by fanning the write out, because ProviderReputationRegistryCompat exposes no score setter at all — its only write is record_with_now, which appends events. An operator override is therefore invisible to read_reputation_async and cheapest_with_ranking_async, leaving the RFC-0900 Gap 7.1 circuit breaker inert on the async path. Measured: a provider recorded healthy then overridden to success_rate 0.0, breaker at 0.5, is correctly excluded by cheapest (90_000) but still selected by cheapest_with_ranking_async (1_000)."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0968
---

# Mission `marketplace-set-provider-score-has-no-async-counterpart`

## Status

Deferred — the resolution is an RFC-owner decision about the RFC-0968
retirement gate. Documentation and pinning were actioned here; no
production logic changed.

## Finding

`set_provider_score` is the mirror image of the dual-write gap recorded in
`marketplace-record-outcome-surfaces-require-a-dual-write`, and the crucial
difference is that **this one cannot be fixed by fanning the write out.**

### Why fanning out is impossible

`ProviderReputationRegistryCompat`'s complete public surface:

| Method                | Kind  | Writes              |
| --------------------- | ----- | ------------------- |
| `new`                 | ctor  | —                   |
| `set_min_reputation`  | sync  | threshold           |
| `min_reputation`      | sync  | —                   |
| `score`               | async | — (read)            |
| `read_outcome`        | async | — (read)            |
| **`record_with_now`** | async | **sole data write** |
| `is_excluded_async`   | async | — (read)            |

There is no score setter. The store is event-sourced: `record_with_now`
appends `SignalEvent`s and the aggregate is derived. So unlike
`set_min_reputation` — where both registries had a threshold setter and the
fix was a two-line dual-write — there is nowhere on the canonical surface
for an override to go.

The only ways to represent `success_rate = 0.0, samples = 10` there would
be to **fabricate** ten failure events, or to add a non-event override
channel to an event-sourced store. Both are design changes to RFC-0968's
storage model, not marketplace-local fixes.

## Consequence: the async circuit breaker is inert

The async selection gate in `cheapest_with_ranking_async` consults
`self.reputation_compat` only. An override lands in `self.reputation` and
is never seen.

Measured — provider recorded healthy asynchronously, then overridden to
`success_rate = 0.0` via `set_provider_score`, breaker at 0.5, the provider
quoting 1_000 against a healthy one at 90_000:

```text
sync  provider_score = Some(success_rate 0.0, latency 900, samples 10)
async compat        = samples=1, success_rate=1.0, latency 50
SYNC  cheapest      = 90_000   (override honoured, provider excluded)
ASYNC cheapest      = 1_000    (override invisible, provider selected)
```

So the two selection paths disagree about whether a reputation-gated
provider is selectable, on the same `Marketplace` instance, with the
breaker on — the same disagreement as the dual-write gap, with the sides
reversed.

## The unifying invariant

Taken together, the two defects state one rule about the facade's two
reputation surfaces:

> **Whichever reputation surface a caller writes to exclusively, the other
> surface's circuit breaker is inert.**

Every reputation write in the public API falls into one of two buckets:

| Write                  | Legacy | Canonical  | Gap closable by fan-out? |
| ---------------------- | ------ | ---------- | ------------------------ |
| `set_min_reputation`   | yes    | yes        | yes — fixed              |
| `record_outcome`       | yes    | no         | yes — documented         |
| `record_outcome_async` | no     | yes        | yes — documented         |
| `set_provider_score`   | yes    | **no API** | **no** — this mission    |

`set_min_reputation` was the one that had already been repaired. The three
remaining gaps are the subject of this mission and
`marketplace-record-outcome-surfaces-require-a-dual-write`.

## Actioned in this mission (documentation and pinning only)

1. **Doc comment** on `set_provider_score` now states that the write is
   legacy-only, explains _why_ it cannot be fanned out (no counterpart
   setter; the store is event-sourced), gives the measured consequence, and
   states the unifying invariant alongside the dual-write requirement on
   `record_outcome`.

2. **Two tests** in `tests/marketplace_reputation_async.rs`:
   - `score_override_is_invisible_to_the_async_breaker` pins the asymmetry in
     its measured direction. It is a **change-detector**, not an
     endorsement: it passes while the gap is open and fails once the
     override reaches the canonical surface.
   - `async_breaker_excludes_on_its_own_recorded_failures` is the
     **positive control** — same fixture shape, but the provider is given
     real async failure data instead of an override. Without it, the hazard
     test could be satisfied by a broken async exclusion gate rather than by
     the unpropagated override.

The hazard test's final assertion was **mutation-verified**: making the
async gate additionally consult `self.reputation.is_excluded` — a two-line
simulation of the override reaching the canonical surface — fails it with
`left: 90000, right: 1000`. The mutation was reverted and the suite
re-confirmed green.

No production logic, no RFC, and no archived record was modified.

## Why this was not simply fixed

The obvious mechanical fix — fabricate `samples` failure events through
`record_with_now` so the aggregate matches the override — was rejected on
purpose. It writes fabricated observations into an audit-logged,
append-only reputation store. `SignalEvent` carries `score_delta` and is
part of RFC-0968's audit trail; synthesizing history to match a UI-level
override would make the ledger lie about what was observed, and the
aggregate would no longer be reconstructible from its own events. That is a
worse failure mode than a known-inert breaker on a path with no production
callers.

## Severity in practice today

Low, and lower than the dual-write gap. `set_provider_score` is documented as
an "operator / test fixture" hook, and the facade has **zero production
callers** (mission `the-marketplace-facade-has-no-production-caller`), so
neither write surface is reached by anything shipping.

It becomes live the moment settlement is wired, and at that point it is the
_more_ dangerous of the two: `record_outcome` is called on every completion
and a missed dual-write degrades reputation gradually, whereas
`set_provider_score` is the emergency lever an operator reaches for when a
provider is misbehaving. An emergency brake that silently does nothing on
the canonical selection path is worse than a missing brake.

## What the RFC owner needs to decide

- Is a direct score override compatible with RFC-0968's event-sourced store
  at all? If not, should `set_provider_score` be removed rather than
  documented?
- If overrides must exist on the canonical surface, does RFC-0968 need an
  override/adjustment event type — a distinct `SignalEvent` kind that
  records "operator set score to X" as a fact rather than synthesizing ten
  observations that never happened?
- Should an override on one surface emit a canonical invalidation for the
  other, so the breaker fails closed on both paths even though the score
  itself cannot be transferred?
- Is the fail-closed predicate the right place to fix instead — should an
  operator override be represented as an exclusion rather than as a score,
  so it needs no counterpart setter at all?

## Reproduction

```bash
# the override writes only the legacy registry
grep -n -A2 'pub fn set_provider_score' crates/quota-router-core/src/marketplace/mod.rs

# the compat registry has no score setter
grep -nE 'pub (async )?fn ' crates/quota-router-core/src/marketplace/reputation_compat.rs

# the async gate reads only the compat registry
grep -n -B2 -A6 'is_excluded_async(&order.spec.asker_did)' \
  crates/quota-router-core/src/marketplace/mod.rs

# the gap, as pinned
cargo test -p quota-router-core --test marketplace_reputation_async \
  score_override_is_invisible_to_the_async_breaker -- --nocapture
```

## Cross-references

- RFC-0900 Gap 7.1 (circuit-breaker threshold — recorded in the
  version-history table; RFC-0900 has no section of its own for it)
- RFC-0968 §5 Storage Schema and §11 Audit Trail — why synthesizing events to
  match an override is not an acceptable fix
- Mission `marketplace-record-outcome-surfaces-require-a-dual-write` — the
  inverse gap, and the closing half of the unifying invariant
- Mission `set_min_reputation` — the one reputation write that _could_ be
  dual-written, and was
- Mission `the-marketplace-facade-has-no-production-caller` — why nothing
  shipping is affected today
- Mission `the-reputation-store-implements-a-running-mean-not-the-rfc-ewma` —
  the unresolved canonical statistic on the surface that cannot receive
  overrides
