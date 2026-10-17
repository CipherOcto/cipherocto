---
name: the-cache-stuffing-defense-is-absent-from-every-live-pricing-path
description: "RFC-0959 threat A5 rates askers gaming the cache axis as HIGH severity and specifies a five-layer defense, including that a settlement receipt bind axes_consumed.cache_key_hash so a forger cannot cheat without provider cooperation. The guard exists and is correct, in compute_cost, which returns CacheStrategyRequired when a cached axis is consumed with no cache_key_hash. compute_cost has exactly one caller and it is a test. Every real pricing path calls settlement_cost, the sibling function that has no cache-strategy check at all, with six production callers across three crates. The live CLI envelope additionally cannot carry a cache_key_hash: its axes_consumed is Vec of (AxisId, u64) tuples, while the richer AxesConsumed struct that has the field is used only by SettlementEvent. Measured on the built binary: an envelope declaring cached_input_tokens_per_1k with no cache_key_hash passes settle-replay with verify OK, exit 0, against a persistent store. The AntiFraudMonitor and its circuit breaker, which implement A5 layer one, have zero production callers."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
---

# Mission `the-cache-stuffing-defense-is-absent-from-every-live-pricing-path`

## Status

Deferred — routing pricing through `compute_cost` is a breaking change
(`settlement_cost` returns `Dqa` and panics on overflow; `compute_cost`
returns `Result`), and `cache_key_hash` cannot be added to the envelope
without a wire-format change. Both are RFC-owner calls. Nothing was
actioned; the measurement used JSON built outside the tree, so the tree is
unchanged.

## Finding

RFC-0959 identifies cache-axis gaming as a HIGH-severity adversary and
specifies a five-layer mitigation. The layer that makes the receipt
non-forgeable is not reachable from any live pricing path.

### What the RFC requires

> **(iii)** `SettlementReceipt.envelope.event.axes_consumed.cache_key_hash`
> binding — receipt hash includes cache_key_hash; forger cannot cheat without
> provider cooperation

> **(ii)** provider-side `cache_control == HIT` cross-check required to use
> `CachedInputTokensPer1k` axis

> **HIGH** — Adversary cost is trivial; gain is direct revenue bypass;
> defense requires multi-layer mitigation (provider cooperation mandatory).

The gain is direct: `cached_input_tokens_per_1k` is priced differently from
`input_tokens_per_1k`, so claiming the cached axis without proving a cache hit
is a revenue bypass.

### The guard exists

`compute_cost` implements layer (ii)/(iii)'s precondition:

```rust
if axes_consumed.requires_cache_strategy() && axes_consumed.cache_key_hash.is_none() {
    return Err(SettlementError::CacheStrategyRequired);
}
```

That is the right check, in the right place, with a dedicated error variant.
It is also the only such check in the crate.

### It is never called

`compute_cost` has exactly **one** caller, and it is a test:

```text
assert!(matches!(err, SettlementError::CacheStrategyRequired))   // a test
```

(Other `compute_cost` hits in the tree are a different function —
`quota_router_core::pricing::compute_cost`, which takes token counts and has
nothing to do with the cache strategy.)

Every real pricing path calls the sibling `settlement_cost`, which has **no
cache-strategy check at all**:

```text
quota-router-core/src/marketplace/mod.rs   load-on-open hydration
quota-router-core/src/marketplace/mod.rs   Marketplace::put
quota-router-storage/src/marketplace.rs    select_ask
quota-router-storage/src/ask_repo.rs       AskRead::cheapest
quota-router-storage/src/ask_repo.rs       cheapest tie-break
```

Six production callers, none of which can reject a cached axis for want of
proof.

### The envelope cannot carry the proof even in principle

There are two consumption representations, and the live one is the thin one:

```rust
pub type AxisConsumption = (AxisId, u64);          // what the envelope holds

pub struct AxesConsumed {
    pub axes: BTreeMap<String, TokenCount>,
    pub cache_key_hash: Option<[u8; 32]>,          // the proof
}
```

`SettlementEnvelope.axes_consumed` is `Vec<AxisConsumption>` — flat tuples.
`AxesConsumed`, the one with `cache_key_hash`, is used by `SettlementEvent`
and by `compute_cost`.

So A5 layer (iii) holds on the _receipt_ surface, which signs
`canonical_ser(event)` and therefore covers `cache_key_hash`, and is
structurally absent from the _envelope_ surface, which the CLI operates on.

## Measurement

An envelope whose only consumption is `cached_input_tokens_per_1k` — with no
`cache_key_hash`, and **no field in which to put one** — on the built binary,
against a persistent store:

```text
$ quota-router settle-replay --db-path ./cache.stoolap --from cached_stamped.json
settlement_hash = 815e573e90ba5bdb94df833cc24100ebbba9775d3766d7014c677661b2e926d7
verify: OK (hash matches + nonce inserted)
exit=0
```

`SettlementEnvelope::verify` recomputes the hash and consults the nonce index.
It never calls `compute_cost`, so `requires_cache_strategy()` is never
evaluated on this path and `CacheStrategyRequired` cannot be returned.

## Layer one is unwired too

`AntiFraudMonitor` and `circuit_breaker` implement A5 layer (i) —
`MIN_PROMPT_DIVERSITY = 50`, `CACHE_HIT_RATE_TRIP_THRESHOLD`,
`RECOVERY_COOLDOWN_SECS`, and the `Active → Tripped → Recovering` machine the
RFC specifies. `FraudSignal`, `FraudSignalKind` and `MultiLayerCacheStatus`
exist alongside it.

None of it has a production caller. Every reference outside its own file is
the `lib.rs` re-export.

This is the RFC's "**advisory only — does NOT mutate canonical
`axes_consumed`**" design, which is deliberate and correct as a
determinism boundary. It is recorded here because it means the advisory
cannot be the thing that catches a forged cached axis either: there is
nothing feeding it, and nothing consuming its verdict.

## One correction to the RFC

§Roles names the Anti-Fraud Monitor as living at
`crates/quota-router-core/src/anti_fraud.rs`. It is at
`crates/quota-router-storage/src/anti_fraud.rs`.

The file exists and is implemented; only the crate in the citation is wrong.
This audit's first assumption on finding no file at the named path was that
the control was absent, which was wrong — worth recording, because it is the
same failure mode as the `§CLI` and `§Cache Classification` phantom citations
already corrected, running in the opposite direction. A citation that names a
plausible-but-wrong location is worse evidence of absence than a phantom one,
because it invites the conclusion the file was never written.

## Why no test catches it

`compute_cost`'s cache-strategy test at `ask.rs` passes, and is a good test of
the guard. It is the only caller.

The pricing tests exercise `settlement_cost`, which cannot fail this way. So
the guard is covered and the path that bypasses it is covered, and no test
spans them — the same shape as `sign_then_verify_roundtrip`, where the fixture
makes the property untestable.

## What the RFC owner needs to decide

- Should `settlement_cost` gain the cache-strategy precondition, or should all
  six call sites move to `compute_cost`? The second is correct but is a
  breaking change, because `settlement_cost` returns `Dqa` and panics on
  overflow where `compute_cost` returns `Result`. That trade — panicking
  convenience versus a check that can fail — is itself worth deciding, and it
  is recorded separately in
  `settlement-cost-panics-where-compute-cost-errors`.
- Should `SettlementEnvelope.axes_consumed` become `AxesConsumed`? That is
  the only way the envelope can carry A5 layer (iii), and it is a wire-format
  change that moves `settlement_hash` inputs.
- Should `select_ask` and `AskRead::cheapest` check the cache strategy at
  all? They price a synthetic 1000-unit consumption per configured axis and
  never see a `cache_key_hash`, so today they cannot.
- Should `AntiFraudMonitor` be fed anywhere? Its advisory-only design means it
  should not mutate settled axes, but the RFC gives it a state machine and a
  dashboard role, and neither is reachable.
- Should the §Roles citation be corrected to the crate the file is in?

## Reproduction

```bash
# the guard, and its single caller (a test)
grep -n -B3 -A3 'requires_cache_strategy()' crates/quota-router-storage/src/ask.rs
grep -rn 'compute_cost(' --include=*.rs crates/quota-router-storage/

# the six production callers that lack it
grep -rn 'settlement_cost(' --include=*.rs crates/ | grep -v 'pub fn'

# two consumption representations, one of which cannot carry the proof
grep -n 'pub type AxisConsumption' crates/quota-router-storage/src/ask.rs
grep -n -A8 'pub struct AxesConsumed' crates/quota-router-storage/src/ask.rs
grep -n -A3 'pub axes_consumed' crates/quota-router-storage/src/ask.rs

# the envelope verify path never prices
grep -n -A12 'pub fn verify(&self, consumed_index' crates/quota-router-storage/src/ask.rs

# the anti-fraud layer, unwired
grep -rn 'AntiFraudMonitor\|circuit_breaker::' --include=*.rs crates/ \
  | grep -v 'src/anti_fraud.rs\|src/circuit_breaker.rs'

# the RFC's threat model and its own miscited path
grep -n 'A5' rfcs/accepted/economics/0959-ask-settlement-chain.md | head -3
grep -n 'anti_fraud.rs' rfcs/accepted/economics/0959-ask-settlement-chain.md
```

## Cross-references

- RFC-0959 §Adversary A5 — the five-layer mitigation and its HIGH rating
- RFC-0959 §Roles — the Anti-Fraud Monitor role, its state machine, and the
  path this audit found to be miscited
- RFC-0959 §Data Structures — `AxesConsumed.cache_key_hash` and the
  `CACHE_KEY_DOMAIN` const-assert the RFC's checklist also requires
- Mission `settlement-cost-panics-where-compute-cost-errors` — why the two
  cost functions were not simply merged long ago
- Mission `the-cross-chain-settlement-reject-is-never-called` — the same
  shape: a correct verifier with no production caller
- Mission `the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist`
  — the envelope/event split that puts the proof on the unused surface
- Mission `the-settlement-path-records-but-never-settles` — the standing
  reason none of this is exploited today
