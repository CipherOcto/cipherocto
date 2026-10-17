---
name: four-of-rfc-0959s-six-acceptance-gates-are-not-met
description: "RFC-0959 states six acceptance gates, G1 to G6, as testable claims about the settlement chain. Checking each against the tree: G3 fails by measurement. Selecting from an index of exactly 100,000 active Asks, the gate's stated scale, takes an average of 14977 microseconds and a worst case of 20128, against a stated limit of 5000 microseconds p99, in an optimised release build. select_ask calls prune_expired first, which scans the entire index, so it is O(n) per selection by construction and the gate cannot be met by tuning. G5 fails because the TOML axis parser exists and is publicly re-exported but has zero consumers, so adding an axis still requires a code change, which is the one thing the gate says it does not. G1 and G2 fail as written, recorded in the settlement-hash and content-address missions. G6 fails by measurement. G4 passes, with no float anywhere in the settlement path. Only one of six gates is met."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
---

# Mission `four-of-rfc-0959s-six-acceptance-gates-are-not-met`

## Status

Deferred — G3 is a performance defect needing an indexing strategy change, G5
needs a decision about whether TOML is the intended extension mechanism, and
G1/G2/G6 are Layer A. All are owner calls. Nothing was actioned; the
measurement probe was removed and the tree matches its prior state.

This is the second systematic sweep in this audit, after
`the-rfc-0959-role-table-is-not-a-map-of-the-implementation`. The roles table
answers "who is responsible"; the gates answer "what must be true". Both are
structures the document itself points at, and both were worth checking.

## Finding

RFC-0959 states six gates as testable claims. One is met.

| Gate   | Claim                                                            | Verdict              |
| ------ | ---------------------------------------------------------------- | -------------------- |
| **G1** | settlement hash deterministic across independent implementations | **fails as written** |
| **G2** | Ask identity collision resistance, 2^-128                        | **fails as written** |
| **G3** | index selection latency ≤ 5ms p99 over 100K active Asks          | **fails, measured**  |
| **G4** | integer-only settlement math, no float anywhere                  | **passes**           |
| **G5** | adding an axis needs only a TOML entry + parser bump             | **fails**            |
| **G6** | replay defense rejects duplicates deterministically              | **fails, measured**  |

## G3 — measured, and structural

This is the new finding. Measured on the built code at exactly the gate's
stated scale, in an optimised release build:

```text
indexed=100000   build_ms=241
runs=50          avg_us=14977   worst_us=20128   (gate: <=5000us p99)
```

**Three times over on average, four times on the worst case**, after
optimisation. The unoptimised figures are 52,070 µs average and 68,367 µs
worst — an order of magnitude over — and are recorded only to show the
optimisation was applied before the number was taken.

### Why it cannot be tuned into compliance

`select_ask` begins with:

```rust
// Best-effort eviction (cheap when few expired).
self.prune_expired(now_unix);
```

and `prune_expired` is a full scan:

```rust
let expired: Vec<AskId> = self
    .indexed
    .iter()
    .filter(|(_, a)| a.expires_at_unix < now_unix)
    .map(|(id, _)| *id)
    .collect();
```

That is O(n) over every entry in the index, executed on **every selection
call**, to find entries that are usually none. At the gate's 100K scale that
is 100,000 `BTreeMap` iterations to answer a question that is usually "none".

The source comment calls it "cheap when few expired". It is cheap in expired
_count_, which is the wrong axis — the scan costs the same whether it finds
zero or ten thousand, and it is paid per call.

So G3 is not a tuning gap. The structure cannot meet it without either an
expiry structure (a time-ordered index, or a periodic sweep decoupled from
selection) or an explicit decision to prune on a schedule instead of inline.

### The gate is also unmeasured in CI

`quota-router-core` has a `benches/` directory containing exactly one file,
`key_hash_storage_bench.rs`, which does not touch the marketplace. No
benchmark anywhere covers `select_ask`, so a regression here would be silent.

## G5 — the parser exists and nothing calls it

The gate says extending the pricing axis set requires "only TOML entry +
parser bump; no consensus change".

`crates/quota-router-storage/src/axis_registry_toml.rs` implements exactly
that, exporting `is_snake_case`, `load_from_str` and `load_from_path`, and
`lib.rs` re-exports them publicly as `load_axis_registry_from_str` and
`load_axis_registry_from_path`.

**Neither has a consumer.** The registry actually used is
`PricingAxis::standard_axes()` or `PricingAxisRegistry::default()`. So adding
an axis today requires a code change — precisely what the gate says it does
not.

The parser is also, notably, the one piece of this thread's work that was
built to spec and then simply never wired, rather than specified and not
built. That is a different failure from G3 and worth distinguishing: the
capability exists and is public, so the gap is a missing call, not a missing
feature.

## G1 and G2 — fail as written, carried from other missions

- **G1** requires the settlement hash to be "deterministic across independent
  implementations", but the RFC's own test vectors supply inputs and write
  `[32-byte digest — implementer computes]`. There is no expected value to
  agree on, so the gate cannot fail. Independently, the tree contains **two**
  settlement-hash derivations with different preimages. See
  `settlement-hash-does-not-bind-the-cost`.
- **G2** states a collision-resistance property for "Ask identity", as though
  there were one. There are two, and they disagree — see
  `the-same-ask-has-two-different-content-addresses`. The storage-side one is
  a delimiter-free concatenation; an attempted collision was measured and did
  **not** reproduce, which is recorded as unresolved rather than as a pass.

## G6 — fails, measured

The gate requires that the same `(event_tuple, nonce)` from the same router
yields a distinct `receipt_id` and that the consumed-receipt index rejects
duplicates deterministically.

`receipt_id` does not exist on the RFC-0959 receipt surface, and the index
is a flat `HashMap` keyed on the envelope nonce with no per-router scoping.
Measured: three submissions of one economic event differing only in nonce all
returned `verify: OK` against a **persistent** store. See
`replay-defense-is-a-no-op-in-the-default-cli-configuration` (v1.2) and
`the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist`.

## G4 passes, and it should be said

`crates/quota-router-storage/src/ask.rs` contains **no `f32` and no `f64`**.
Cost arithmetic is `Dqa` (i64 value, i32 scale) throughout, which is the gate's
requirement.

That is not a small thing: the same crate's slashing ledger does use `f64`
for `cumulative_loss_pct`, and the audit has a record of a float-adjacent
precision problem elsewhere in the settlement path
(`dqa-cmp-and-dqa-sub-disagree-on-canonicalization`). The core pricing
arithmetic being integer-only is deliberate and intact.

## What the RFC owner should do

- **G3**: the gate cannot be met with the current structure. Either the gate's
  budget is wrong, or `select_ask` needs to stop doing a full expiry scan
  inline. Those are different projects and the second is worth doing on its
  own merits — an O(n) cost on every selection is a capacity ceiling
  independent of any latency target.
- **G3**: should a benchmark exist? A gate with no benchmark is a claim
  nothing checks, which is the same shape as G1's unusable digest.
- **G5**: is TOML the intended extension mechanism? If yes, who calls
  `load_axis_registry_from_str`? If no, the gate should be rewritten, because
  it currently promises a property the shipped mechanism does not have.
- **G1/G2**: supply expected digests in the test vectors, or restate the
  gates as properties that can actually fail. A gate that cannot fail is
  worse than no gate, because it reads as coverage.
- **All six**: is a gates table that is one-sixth met, with no CI enforcing
  any of them, the right instrument? The honest alternative is to mark each
  gate's status in the RFC the way the A1–A5 threat table already does with
  ACCEPTED / LOW / MEDIUM / HIGH.

## Reproduction

```bash
# the gates
grep -nE 'G[0-9]+ +\|' rfcs/accepted/economics/0959-ask-settlement-chain.md

# G3: the full scan on every selection
grep -n -A3 'pub fn select_ask' crates/quota-router-storage/src/marketplace.rs
grep -n -A10 'pub fn prune_expired' crates/quota-router-storage/src/marketplace.rs

# G3: no benchmark covers the marketplace
ls crates/quota-router-core/benches/
grep -rln 'select_ask\|Marketplace' crates/quota-router-core/benches/ || echo none

# G4: integer-only holds
grep -cE 'f32|f64' crates/quota-router-storage/src/ask.rs

# G5: the parser exists, publicly re-exported, and unused
grep -n 'pub fn load_from_str\|pub fn load_from_path' \
  crates/quota-router-storage/src/axis_registry_toml.rs
grep -rn 'load_axis_registry_from_str\|load_axis_registry_from_path' \
  --include=*.rs crates/ | grep -v 'src/lib.rs'

# G1: no expected digest to agree on
grep -n '32-byte digest' rfcs/accepted/economics/0959-ask-settlement-chain.md

# G6: receipt_id absent from the RFC-0959 receipt
grep -n -A10 'pub struct SettlementReceipt' crates/quota-router-storage/src/ask.rs
```

## Cross-references

- RFC-0959 §Acceptance criteria — G1 through G6, the subject of this sweep
- Mission `the-rfc-0959-role-table-is-not-a-map-of-the-implementation` — the
  first systematic sweep, over §Roles; together these two are the document's
  own self-describing structures
- Mission `settlement-hash-does-not-bind-the-cost` — G1
- Mission `the-same-ask-has-two-different-content-addresses` — G2
- Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration` — G6
- Mission `the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist`
  — the absent `receipt_id` behind G6
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the index
  whose selection path G3 measures
- Mission `documentation-that-describes-a-mechanism-nobody-built` — G3 and G5
  are instances; the distinction drawn above (capability built but unwired,
  versus capability specified but unbuilt) is the useful refinement
