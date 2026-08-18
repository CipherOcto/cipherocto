---
name: mission-0105-x-s4-deferred-codemod-sites-status
description: 0105-x u128→Dqa migration LANDED 2026-08-18; scope narrower than mission file claimed (2 source + 2 test files; 5 AC-1 files already Dqa by S4 codemod Round 2)
metadata:
  type: mission
  status: LANDED
  date: 2026-08-18
---

# Mission 0105-x — S4 deferred-codemod u128→Dqa sites

## Status

**LANDED 2026-08-18** on `next` (commit pending user push per [[feedback_initiation_user_only]]).

## What landed

2 source files + 2 test files migrated from `u128` amount-bearing field
types to `octo_determin::Dqa` at `scale = 0`:

- `crates/quota-router-core/src/marketplace/escrow.rs` — `Escrow::amount_micro_octo_w`
  + `EscrowSnapshot::amount_micro_octo_w` + `Escrow::new` /
  `Escrow::with_arbitrator` signatures
- `crates/quota-router-core/src/task_market/escrow.rs` —
  `TaskEscrow::amount_micro_octo_w` + `TaskEscrow::new` /
  `TaskEscrow::with_arbitrator` signatures
- `crates/quota-router-core/tests/marketplace_e2e.rs` — 5 fields
  (`let amount = dqa(...)` + 4 `Escrow::new` / `with_arbitrator`
  calls)
- `crates/quota-router-core/tests/task_market.rs` — 8 `100_000,`
  literals + 3 `matched.price * qty as u128` → `dqa(...)` + 2
  `assert_eq!(escrow.base.amount_micro_octo_w, ...)` → `dqa(90)` /
  `dqa(100)`. Added `fn dqa(n: i64) -> octo_determin::Dqa` helper
  (test file had duplicate `fn dqa` definition at line 535; removed
  duplicate).

## Scope discovery (vs mission file claim)

Mission file AC-1 listed 7 files needing migration. Audit 2026-08-18
during implementation surfaced that **5 of the 7 files were already
Dqa** (S4 codemod Round 2 migrated them):

- `crates/quota-router-core/src/marketplace/slashing.rs` — `amount_micro_octo_w: octo_determin::Dqa` (line 375)
- `crates/quota-router-core/src/task_market/slashing.rs` — `initial_stake_micro_octo_w: octo_determin::Dqa` (line 36)
- `crates/quota-router-storage/src/slash_store.rs` — `_amount_micro_octo_w: octo_determin::Dqa` (line 70)
- `crates/quota-router-storage/src/settlement_event_repo.rs` — `cost_micro_octo_w: octo_determin::Dqa` (line 277) + `dqa_serde::dqa_from_bytes` boundary decode (line 335)
- `crates/quota-router-cli/src/cli.rs` + `commands.rs` — no u128 amount-bearing fields; `commands.rs:6` `use octo_determin::Dfp` is for unrelated price-rounding surface

**Only 2 source files actually needed migration**:
`marketplace/escrow.rs` + `task_market/escrow.rs`. The mission file
caught the file-level deferred sites that the S4 codemod missed — the
biggest gap was the `Escrow` struct's `amount_micro_octo_w` field
which had a downstream-blast-radius through `TaskEscrow::base`.

## Why this matters

Closes audit verdict 2026-08-17 Risk #4 HIGH (parallel-model
field-type drift). The substrate invariant: **all amount-bearing
fields in production code use `octo_determin::Dqa` at `scale = 0`**,
matching the `StoolapSpendLedger` precedent (RFC-0862 v2.0.0). After
0105-x landing, the only remaining `u128` amount-bearing fields in
workspace are in `cipherocto-encoding/src/lib.rs` +
`quota-router-sm-engine/src/lib.rs` + `quota-router-core/src/settle.rs`
(RFC-0862 canonical on-wire form — 16-byte BE `DqaEncoding` — byte
form is unchanged; in-memory field type stays `u128` per RFC-0105
wire-form §Caveat Payload Type Coherence).

## Test verification

- `cargo test -p quota-router-core --features full --test task_market` — **32/32 pass**
- `cargo test -p quota-router-core --features full --test marketplace_e2e` — **24/24 pass**
- `cargo clippy -p quota-router-core --features full --all-targets -- -D warnings` — **zero warnings**
- `cargo fmt --all -- --check` — **clean**
- `cargo build -p quota-router-core --tests` — **green**

`cargo test -p quota-router-core --lib` BLOCKED on pre-existing
`libpython3.12.so.1.0` missing (pyo3 build env issue at workspace
level, not 0105-x regression — only the external `quota_router_core`
test binary links to pyo3; the integration tests `marketplace_e2e`
+ `task_market` don't).

## Cross-references

- **Audit verdict 2026-08-17 risk #4**: closed for marketplace + task_market escrow surface
- **RFC-0862 v2.0.12**: §Adjacent-substrate u128→Dqa (additive on v2.0.11) — describes the 2-file + 5-already-migrated file disposition
- **Co-mission**: `0862-c9-micro-octow-type-unification` (LANDED 2026-08-17) — closed Risk #1 CRITICAL (type-alias split); x-mission closes Risk #4 HIGH (field-type drift)
- **Pattern reference**: `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (canonical Dqa substrate)
- **S4 codemod receipt**: `S4-codemod-2026-08-17-LANDED.md` (155 sites covered; this mission catches the 2 file-level sites missed)

## Out of scope (still deferred)

- `cipherocto-encoding/src/lib.rs` `amount_micro: u128` — canonical wire form (RFC-0105 §Caveat Payload Type Coherence)
- `quota-router-sm-engine/src/lib.rs` `amount_micro: u128` — `Reservation::mint` derivation
- `quota-router-core/src/settle.rs` `amount_micro: u128` — `mint_reservation` step
- `quota-router-storage/src/slash_store.rs` `SlashRecord.tx_amount_micro_octo_w` (cargo `unused` field — already Dqa type but field unused)
- `quota-router-storage/src/settlement_event_repo.rs` `cost_micro_octo_w` storage column — `BLOB` stays u128 wire until S6e RFC-0959 amendment promotes to `DQA(12)`

## User push

Awaits explicit user instruction per [[feedback_initiation_user_only]].
Local commit only; no remote write.
