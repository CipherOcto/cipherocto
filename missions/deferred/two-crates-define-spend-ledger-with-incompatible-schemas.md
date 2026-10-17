---
name: two-crates-define-spend-ledger-with-incompatible-schemas
description: "quota-router-storage migration v007 and quota-router-core schema.rs both issue CREATE TABLE IF NOT EXISTS spend_ledger with zero column overlap, so whichever initialises first wins and the second fails. Measured in both orders: storage first then core fails with column 'key_id' not found, core first then storage fails at migration v7 with a generic message pointing at substrate logs. Latent today because StoolapSpendLedger has no production callers."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0862
    - RFC-0903
    - RFC-0909
---

# Mission `two-crates-define-spend-ledger-with-incompatible-schemas`

## Status

Deferred — renaming a table is a storage-schema decision with migration
and back-compat consequences, and neither owner should be assumed. The
collision is documented in both files; no behaviour changed. Measurement
probe removed, tree matches its prior state.

## Finding

Two crates each declare the table `spend_ledger`, with no column in
common.

**quota-router-storage**, migration `v007__create_spend_ledger.sql`:

```sql
CREATE TABLE IF NOT EXISTS spend_ledger (
    holder_did BLOB NOT NULL,
    macaroon_id BLOB NOT NULL,
    balance INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (holder_did, macaroon_id)
);
```

**quota-router-core**, `schema.rs`, created for RFC-0903 budget
enforcement:

```sql
CREATE TABLE IF NOT EXISTS spend_ledger (
    event_id BLOB(32) NOT NULL,
    request_id BLOB(32) NOT NULL,
    key_id BLOB(16) NOT NULL,
    UNIQUE(key_id, request_id),
    team_id BLOB(16),
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_amount INTEGER NOT NULL,
    pricing_hash BLOB NOT NULL,
    token_source TEXT NOT NULL CHECK (...),
    ...
);
```

Neither is a test artefact. v007 is applied by
`quota_router_storage::migrations::apply_pending`, and core's schema is
written by `quota_router_core::schema::init_database`, which is a
**public export** from `quota-router-core`. Core's
`record_spend_ledger` — the method that inserts spending events — is
production code.

Because both use `IF NOT EXISTS`, the second initialisation is a silent
no-op at the DDL level, and the conflict surfaces only when the loser
queries a column that does not exist.

## Measurement

Both orders, on one in-memory database:

**storage first, then core:**

```text
MEASURE storage_migrations_is_ok=true
MEASURE core_init_database_is_ok=false
MEASURE core_err=Storage error: column 'key_id' not found
MEASURE spend_ledger.event_id  query_ok=false
MEASURE spend_ledger.cost_amount query_ok=false
MEASURE spend_ledger.pricing_hash query_ok=false
```

**core first, then storage:**

```text
MEASURE core_init_database_is_ok=true
MEASURE storage_migrate_err=Some("substrate storage error: migration v7
   failed: migration SQL failed; see substrate logs for details")
MEASURE spend_ledger.holder_did       query_ok=false
MEASURE spend_ledger.macaroon_id      query_ok=false
MEASURE spend_ledger.balance          query_ok=false
MEASURE spend_ledger.updated_at_unix_ms query_ok=false
```

So the collision is **symmetric and always loud**. Whichever crate runs
second cannot use the table at all.

> **Corrected at v1.0.** The first hypothesis going in was that
> `IF NOT EXISTS` would make this a silent-failure landmine — first
> migration wins, second no-ops, and the conflict only appears at query
> time. Measurement says otherwise: the second initialiser **errors
> immediately**, in both directions. Nothing is silently corrupted. The
> defect is a naming collision and a bad error message, not a silent
> one.

## What the operator actually experiences

The two error messages are the finding.

Adopting `StoolapSpendLedger` against a database quota-router-core has
already initialised yields:

```text
Storage error: column 'key_id' not found
```

That names a column from a schema the operator did not write, in a
crate they may not know owns this table. Nothing points at the
collision, and nothing says another crate already claimed the name.

The reverse direction is worse in a small way:

```text
migration v7 failed: migration SQL failed; see substrate logs for details
```

"see substrate logs" is a pointer away from the cause rather than
toward it.

## Why it matters when settlement is wired

`StoolapSpendLedger`'s module doc describes itself as the replacement
for the in-memory ledger "for production deployments where the spend
ledger MUST survive process restarts". `record_spend_ledger` is already
production in the other crate. When someone wires the settlement path
to a persistent ledger, this is the first thing they hit, and the error
gives them nothing to work from.

It also violates two standing principles directly: **no parallel
abstractions** — two components each owning a storage surface for the
same concept — and **storage is not a protocol**, since the business
meaning of "spend ledger" differs completely between the two (a prepaid
balance to drain versus a record of quota spend events).

Neither file acknowledges the other. `v007` says "Cipherocto-side
migration per [[stoolap-general-purpose-db]]: schema lives in
cipherocto crate, NOT in the stoolap fork" and stops there.

## Severity today: latent

`StoolapSpendLedger` has no production constructor or caller anywhere in
the tree — only doc comments in other crates and its own tests. Nothing
opens a database with both schemas applied today.

That is the same standing caveat as the marketplace facade, and it is
what makes this cheap to fix now and expensive after first deployment.

## What the RFC owner needs to decide

- Which crate owns the `spend_ledger` name? The two models are not
  variants of one thing: a drainable per-macaroon balance and an
  append-only quota-spend event log.
- Should the other be renamed — `holder_balance` and `spend_events`,
  say — or should they be deliberately unified into one table?
- If unified, what is the relationship? Does deducting a balance also
  append a spend event, and who guarantees both or neither?
- Should either migration fail fast with a message that names the
  conflict, rather than surfacing a raw missing-column error?
- Should the migration runner detect an existing table with an
  unexpected shape and report it, rather than letting `IF NOT EXISTS`
  mask it until a later statement fails?

## Reproduction

```bash
# the two definitions
cat crates/quota-router-storage/migrations/v007__create_spend_ledger.sql
grep -n -A22 'CREATE TABLE IF NOT EXISTS spend_ledger' \
  crates/quota-router-core/src/schema.rs

# both are production-reachable
grep -rn 'apply_pending' --include=*.rs crates/quota-router-storage/src/lib.rs
grep -rn 'pub use schema::init_database' crates/quota-router-core/src/lib.rs

# core's writer is production code
grep -n -B2 'INSERT INTO spend_ledger' crates/quota-router-core/src/storage.rs

# the storage-side reader has no production caller
grep -rn 'StoolapSpendLedger' --include=*.rs crates/ | grep -v stoolap_spend_ledger.rs
```

## Cross-references

- RFC-0862 §StoolapSpendLedger substrate — the atomic drain model the
  storage side implements
- RFC-0903 — the budget-enforcement event schema core implements
- Mission `the-marketplace-facade-has-no-production-caller` — the standing
  caveat that keeps this latent
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the same
  shape in the read path rather than the storage path
- Mission `the-settlement-path-records-but-never-settles` — the path whose
  wiring would surface this collision
