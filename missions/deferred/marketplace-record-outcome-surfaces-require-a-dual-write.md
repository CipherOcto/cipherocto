---
name: marketplace-record-outcome-surfaces-require-a-dual-write
description: "record_outcome writes only the legacy ProviderReputationRegistry and record_outcome_async writes only the RFC-0968 compat store — nothing fans one out to the other, so a caller that migrates to the async surface alone leaves the legacy shadow empty and the RFC-0900 Gap 7.1 circuit breaker inert on the sync path. Measured: a provider that failed 20/20 recorded async-only is still returned by cheapest with min_reputation = 0.5, while cheapest_with_ranking_async correctly excludes it."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0968
---

# Mission `marketplace-record-outcome-surfaces-require-a-dual-write`

## Status

Deferred — the resolution is an RFC-owner decision about the RFC-0968
retirement gate. The evidence is established and reproducible; the _choice_
of how to close the gap is not this repo's to make. Filed under
`missions/deferred/` per BLUEPRINT §Mission Lifecycle ("Upstream blocker —
governance decision").

Documentation and pinning were actioned here. No production logic changed.

## Finding

`Marketplace` exposes two reputation write surfaces and two read surfaces,
and the writes are **not** mutually dual-writing.

| Method                  | Writes                              | Reads                 |
| ----------------------- | ----------------------------------- | --------------------- |
| `record_outcome`        | `reputation` (legacy) only          | —                     |
| `record_outcome_async`  | `reputation_compat` (RFC-0968) only | —                     |
| `provider_score`        | —                                   | `reputation` (legacy) |
| `read_reputation_async` | —                                   | `reputation_compat`   |

There is no fan-out in either direction. `record_outcome` calls
`self.reputation.record(...)` and stops. `record_outcome_async` awaits
`self.reputation_compat.record_with_now(...)` and returns its result. Neither
touches the other's store.

### Why this went unnoticed

Every pre-existing fixture in `tests/marketplace_reputation_async.rs`
invokes **both** methods in the same loop body — e.g.

```rust
m.record_outcome(&did, ok, 100);
m.record_outcome_async(&did, ok, 100, controller, now).await.expect("...");
```

That is correct usage during the dual-read window, and it makes the parity
tests valid. But it means **no test in the tree exercised the async-only or
legacy-only case**, which is the case a migrating caller actually produces.
`open_in_memory_with_store_wires_canonical_reputation_store` observed the
asymmetry in a comment ("Legacy shadow stays empty for this DID") and then
immediately compensated by calling the legacy method, so it measured the gap
and moved past it.

## Consequence: the sync circuit breaker goes inert

This is not a cosmetic `None`. `set_min_reputation` sets the threshold on
**both** registries (corrected in mission work under `set_min_reputation`),
but the sync selection path consults the legacy registry for the provider's
score. A provider with **no** legacy entry is treated as un-excluded, not as
unknown-and-therefore-excluded. So an async-only failing provider passes the
breaker straight through.

Measured with 20 failures recorded through `record_outcome_async` only,
`bad` quoting 1_000 and `good` quoting 90_000, `set_min_reputation(0.5)`:

```text
bad  provider_score         = None
bad  read_reputation_async  = samples=20, success_rate=0.0
SYNC cheapest               = 1000     <-- failing provider, breaker INERT
ASYNC cheapest              = 90000    <-- correctly excluded
```

So the two selection paths disagree about whether a 20/20-failing provider is
selectable, on the same `Marketplace` instance, with the breaker on.

A lesser measurement: ten async-only **successes** at 75 ms give
`provider_score -> None` and `read_reputation_async -> samples=10,
success_rate=1, latency_ms=74`. The success case is benign; the failure case
is the breaker hazard.

## Actioned in this mission (documentation and pinning only)

1. **Doc comments** on `record_outcome` and `record_outcome_async` now state
   the dual-write requirement, the measured breaker-inertness consequence,
   and cross-link to this mission. `record_outcome_async` additionally warns
   that migrating a call site is **not** a drop-in substitution, and that the
   parity fixture family writes to both surfaces precisely because it has to.

2. **Two tests** in `tests/marketplace_reputation_async.rs`:
   - `async_only_recording_leaves_the_sync_breaker_inert` pins the divergence
     in its measured direction. It is a **change-detector**, not an
     endorsement: it passes while the gap is open and fails once the gap is
     closed. Its doc comment says so, so a future reader does not mistake a
     failing run for a regression.
   - `dual_write_activates_the_sync_breaker` is the **positive control**,
     on an identical fixture differing only in that each record also goes
     through the legacy surface. Without it, the hazard test could be
     satisfied by an unrelated sync-path defect (a threshold that ignores the
     registry, or an exclusion keyed on the wrong field).

Both assertions of the hazard test were mutation-verified: making
`record_outcome_async` also write the legacy shadow fails the `is_none`
check, and with that check neutralized fails the breaker check with
`left: 90000, right: 1000`. The mutation was reverted and the suite
re-confirmed green.

No production logic, no RFC, and no archived record was modified.

## Why this was not simply fixed

Three reasons, in order of weight:

1. **It is a real design decision, not a missing line.** Closing the gap has
   at least three defensible shapes, and they differ in cost and in
   retirement-gate semantics:
   - _dual-write_ — both methods fan out to both stores. Simplest, but it
     pins the legacy shadow in place and makes the RFC-0968 retirement gate
     harder to pass, since the retired store can no longer be shown to be
     unused.
   - _single-write with async fan-out_ — `record_outcome_async` also writes
     the legacy shadow (what the mutation tested). Keeps `record_outcome` as
     the legacy path, but means the async surface silently depends on a store
     RFC-0968 is retiring.
   - _drop the legacy read surface_ — retire `provider_score` and the sync
     `cheapest` path together, so there is one store and no window. Largest
     change; invalidates the whole dual-read test family.
     Which one is right depends on the retirement-gate schedule, which is an
     RFC-owner call.
2. **The facade has zero production callers** (mission
   `the-marketplace-facade-has-no-production-caller`). Today the only
   consumers of either write method are tests. There is no live path to
   break, and no live data to migrate.
3. **A mechanical dual-write would silently change the parity numbers** the
   retirement gate is measured against, and the canonical statistic those
   numbers compare is itself unresolved (mission
   `the-reputation-store-implements-a-running-mean-not-the-rfc-ewma`).

## What the RFC owner needs to decide

- Which of the three shapes above retires the legacy shadow: fan-out from
  `record_outcome_async`, dual-write from both, or dropping the sync read
  surface entirely?
- Does the retirement gate require the legacy store to be _unused_, or only
  _unwritten by new callers_? The answer decides whether a dual-write is even
  admissible — a dual-write makes the legacy store permanently written.
- When the sync circuit breaker is served by a provider absent from the
  legacy registry, is "un-excluded" the intended default? If the answer is
  no, the fix belongs in the exclusion predicate itself (treat unknown as
  excluded), which is a strictly safer behaviour change than any fan-out and
  would hold regardless of which retirement shape is chosen.
- Should `record_outcome_async` be `#[deprecated]`-marked during the window,
  so that a migrating call site gets a compile-time warning rather than a
  doc-comment warning nobody reads?

## Reproduction

```bash
# the two write methods touch different stores
grep -n -A3 'pub fn record_outcome(' crates/quota-router-core/src/marketplace/mod.rs
grep -n -A9 'pub async fn record_outcome_async(' crates/quota-router-core/src/marketplace/mod.rs

# every pre-existing parity fixture dual-writes, so none exercises the gap
grep -n 'record_outcome(' crates/quota-router-core/tests/marketplace_reputation_async.rs

# the gap itself, as pinned
cargo test -p quota-router-core --test marketplace_reputation_async \
  async_only_recording_leaves_the_sync_breaker_inert -- --nocapture
```

## Cross-references

- RFC-0900 Gap 7.1 (circuit-breaker threshold — recorded in the version-history
  table; RFC-0900 has no section of its own for it)
- RFC-0968, plus amendments 40 + 44 for the `controller_id` rule. The
  "retirement gate" is this repo's framing for the RFC-0968 → legacy-store
  migration, not an RFC-0968 section heading.
- Mission `set_min_reputation` — the circuit-breaker threshold fix on both
  registries, which is what made this gap measurable
- Mission `the-marketplace-facade-has-no-production-caller` — why no live path
  is affected today
- Mission `the-reputation-store-implements-a-running-mean-not-the-rfc-ewma` —
  the unresolved canonical statistic behind the parity numbers
- Mission `dual-read-parity-asserts-a-divergence-bound-it-never-measures` —
  the parity fixture family that dual-writes throughout
