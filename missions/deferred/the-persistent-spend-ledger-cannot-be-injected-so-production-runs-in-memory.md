---
name: the-persistent-spend-ledger-cannot-be-injected-so-production-runs-in-memory
description: "octo-wallet-node constructs PaidQueryVerifyHandler::new(), which is the in-memory ledger, while its own doc says the injected backend is what production deployments use. The persistent alternative, StoolapSpendLedger, does not implement the SpendLedger trait and its crate does not depend on octo-paid-query, so it cannot be injected at all. Measured: a seeded prepaid balance does not survive a restart. The failure is fail-closed, denying spend rather than granting excess."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0862
    - RFC-0903
---

# Mission `the-persistent-spend-ledger-cannot-be-injected-so-production-runs-in-memory`

## Status

Deferred — closing the gap needs a dependency-edge decision between two
crates, which is an architecture call, plus the schema-ownership question
in `two-crates-define-spend-ledger-with-incompatible-schemas`. The three
misleading doc claims are corrected in this commit; no behaviour changed.
Measurement probe removed, tree matches its prior state.

## Finding

`octo-paid-query` builds a paid-query budget behind a trait, explicitly
so persistence can be swapped in:

```rust
pub struct RateLimitBudget { ledger: Arc<dyn SpendLedger> }

/// All mutations flow through `try_deduct` ... This is the single
/// entry point so persistence can be retrofitted without touching
/// call sites.
```

The injection point exists and is used by the production handler:

```rust
// octo-wallet-node/src/handlers/paid_query.rs
pub fn new() -> Self {
    Self::with_ledger(Arc::new(octo_paid_query::InMemorySpendLedger::new()))
}
pub fn with_ledger(ledger: Arc<dyn SpendLedger>) -> Self { ... }
```

And production takes the default:

```rust
// octo-wallet-node/src/node.rs
PaidQueryVerifyHandler::new().handle(&req)
```

**The only implementation of `SpendLedger` anywhere in the tree is
`InMemorySpendLedger`.** The persistent alternative that was written for
this exact job does not implement the trait:

```text
impl SpendLedger for InMemorySpendLedger      <- octo-paid-query
impl SpendLedger for StoolapSpendLedger       <- does not exist
```

and `quota-router-storage` does not depend on `octo-paid-query`, so it
could not implement the trait even if someone tried.

So the substitution the abstraction was designed for is impossible in the
direction that matters, and every paid query is verified against a
process-local balance.

## Three documents describe a path that does not exist

| Where                                | Claim                                                                                                                              |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| `stoolap_spend_ledger.rs` module doc | "Replaces the in-memory `InMemorySpendLedger` for production deployments where the spend ledger **MUST** survive process restarts" |
| `paid_query.rs::with_ledger`         | "Used by production deployments (Stoolap-backed) and by tests"                                                                     |
| `octo-paid-query` module doc         | "persistence can be retrofitted without touching call sites"                                                                       |

None of the three is achievable today. The persistent ledger cannot be
injected; production uses `new()`, not `with_ledger`; and the trait
substitution the design relies on has exactly one implementation, the
in-memory one.

This is the audit's standing theme in its clearest form: three artefacts
describing a guarantee, and no code path that establishes it.

## Measurement — and the direction matters

Simulating a restart by constructing a second budget:

```text
MEASURE seed_ok=true
MEASURE process1 balance=Some(100000)
MEASURE process1 deduct_30000=Ok(70000)
MEASURE process2 balance=None
MEASURE process2 deduct_30000=Err(no balance record for holder/macaroon; wallet must seed before deduct)
```

**It fails closed.** The balance does not survive the restart, and
spending is refused until the wallet re-seeds. It is not a double-spend
and not an over-spend vector.

> The first hypothesis going in was that losing the ledger would let a
> holder re-spend a drained balance. Measurement says the opposite: the
> seeded entry is simply absent, and `try_deduct` refuses.

That makes this an availability and correctness-of-documentation
defect, not a fund-safety one, and the mission is written that way.

The real consequence is that RFC-0862's persistence requirement is unmet
for a component whose own migration file is titled `create_spend_ledger`
and whose atomicity documentation is extensive.

## Secondary: the parallel implementation drops nominal typing

`StoolapSpendLedger`'s methods carry the same names as the trait's but
different parameter types:

| Method                            | `SpendLedger`             | `StoolapSpendLedger` |
| --------------------------------- | ------------------------- | -------------------- |
| `seed` / `try_deduct` / `balance` | `&WireDid`, `&MacaroonId` | `&str`, `&[u8]`      |

Stated precisely, because the typing gain is smaller than it looks:
`MacaroonId` does encode a fixed 16-byte wire form, but `WireDid::new`
performs no validation, so `WireDid` is a nominal newtype rather than a
checking one. Even so, the persistent implementation accepts raw
primitives where the trait requires named types — which is the direction
§13 "Attenuation invariants cross boundaries" warns about, and it is the
_production_ candidate that has the weaker signature.

## What the RFC owner needs to decide

- Which crate may depend on which? For `StoolapSpendLedger` to be
  injectable, `quota-router-storage` must depend on `octo-paid-query`, or
  the trait must move to a shared Layer B substrate both already reach.
- Should the `spend_ledger` table name be resolved first? Two crates
  define it incompatibly — see `two-crates-define-spend-ledger-with-
incompatible-schemas`. Choosing a persistent backend is meaningless
  before that table has one owner.
- Should `PaidQueryVerifyHandler::new()` remain the production entry
  point? If the in-memory ledger is a deliberate default, its doc should
  stop calling the injected backend the production one.
- Should a fail-closed balance loss after restart be surfaced as an
  operational signal? Today the only symptom is every paid query erroring
  with "wallet must seed before deduct", which reads like a wallet
  problem rather than a node-restart problem.
- If re-seeding on restart is the intended model, that should be
  specified — it is currently nobody's job.

## Reproduction

```bash
# production builds the handler with the in-memory ledger
grep -n 'PaidQueryVerifyHandler::new()' crates/octo-wallet-node/src/node.rs

# which is the in-memory default
grep -n -A4 'pub fn new() -> Self' crates/octo-wallet-node/src/handlers/paid_query.rs

# the only trait impl in the tree
grep -rn 'impl SpendLedger for' --include=*.rs crates/

# the persistent ledger does not implement it, and cannot
grep -n 'octo-paid-query' crates/quota-router-storage/Cargo.toml || echo "no dependency"
grep -n 'impl SpendLedger for' crates/quota-router-storage/src/stoolap_spend_ledger.rs || echo "no impl"

# the signature divergence
grep -n -A5 'pub fn try_deduct' crates/quota-router-storage/src/stoolap_spend_ledger.rs
grep -n -A5 'fn try_deduct' crates/octo-paid-query/src/ledger.rs
```

## Cross-references

- RFC-0862 §StoolapSpendLedger substrate — the atomic drain model whose
  persistence requirement is unmet
- RFC-0903 — the budget-enforcement model this ledger backs
- Mission `two-crates-define-spend-ledger-with-incompatible-schemas` — the
  table-name collision that must be resolved before a backend is chosen
- Mission `the-settlement-path-records-but-never-settles` — the broader
  finding that the settlement path records but does not move value
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the same
  parallel-implementation shape in the read path
