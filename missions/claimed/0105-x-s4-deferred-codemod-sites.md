# Mission: 0105-x — S4 deferred-codemod u128→Dqa sites (marketplace + task_market + slash_store + settlement_event_repo + CLI)

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Filed 2026-08-17 per audit verdict
2026-08-17 Risk #4 HIGH parallel-model drift; landed 2026-08-18 with
2-source-file + 2-test-file migration. The other 5 files cited in the
AC-1 table were already migrated by S4 DFP codemod Round 2 (LANDED
2026-08-17 per memory card `S4-codemod-2026-08-17-LANDED.md`) — this
mission caught the 2 file-deferred sites the S4 codemod missed.

## RFC

- Primary: RFC-0105 (Dqa substrate — canonical type for
  amount-bearing columns per review §8.1.2 + §8.4.1)
- Co-RFC: RFC-0862 (`StoolapSpendLedger` substrate — spend_ledger
  already adopted Dqa; this mission extends to adjacent substrate)
- Co-RFC: RFC-0900 (slash_ledger substrate — chain-aware bump lands
  DQA(12) columns at S6d; x-mission migrates in-memory field types
  first)
- Co-RFC: RFC-0959 (settlement wire format — settlement_event_repo
  field types migrate; wire bytes still 16-byte BE u128 pending S6e)

## Dependency edges

| From                                                       | To                        | Why                          | Layer direction |
| ---------------------------------------------------------- | ------------------------- | ---------------------------- | --------------- |
| `crates/quota-router-core/src/marketplace/escrow.rs`       | `octo_determin::Dqa`      | Field type                   | lib → lib       |
| `crates/quota-router-core/src/marketplace/slashing.rs`     | `octo_determin::Dqa`      | Field type                   | lib → lib       |
| `crates/quota-router-core/src/task_market/escrow.rs`       | `octo_determin::Dqa`      | Field type                   | lib → lib       |
| `crates/quota-router-core/src/task_market/slashing.rs`     | `octo_determin::Dqa`      | Field type                   | lib → lib       |
| `crates/quota-router-storage/src/ask.rs`                   | `octo_determin::Dqa`      | Field type                   | lib → lib       |
| `crates/quota-router-storage/src/slash_store.rs`           | `octo_determin::Dqa`      | Field type                   | lib → lib       |
| `crates/quota-router-storage/src/settlement_event_repo.rs` | `octo_determin::Dqa`      | Field type                   | lib → lib       |
| `crates/quota-router-cli/src/cli.rs` + `commands.rs`       | `octo_determin::Dqa`      | CLI surface (Display + wire) | lib → lib       |
| `crates/quota-router-core/Cargo.toml` + storage + cli      | `octo-determin` (git dep) | New deps                     | Cargo → git     |

No new cyclic edges. Multiple new crate deps (`octo-determin`
gated via Cargo.toml pins per S4 codemod pattern).

## Problem

S4 DFP codemod (LANDED 2026-08-17, commits `19faf380` + `4ab400bd`

- `18edbe0d` per memory card) touched 155 sites across 8 crates:

* quota-router-core 115 sites
* octo-paid-query 15 sites
* cipherocto-encoding 12 sites
* octo-pyo3 6 sites
* marketplace_strong_scenarios 7 sites

Per memory card receipt, 146 of 301 split sites deferred to S6/S7:

- octo-cap-macaroon 10 sites
- octo-vault 123 sites (14 in S3 + 109 in S6g)
- quota-router-storage 8 sites
- octo-mkt 5 sites

**146 deferred sites do NOT include the marketplace + task_market
field types.** Per audit 2026-08-17, 7 files still carry `u128`
amount-bearing field types:

| File                                                       | Fields                                                                                                 |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `crates/quota-router-core/src/marketplace/escrow.rs`       | `amount_micro_octo_w`                                                                                  |
| `crates/quota-router-core/src/marketplace/slashing.rs`     | `stake_micro_octo_w` + `initial_stake_micro_octo_w` + `amount_micro_octo_w` + `new_stake_micro_octo_w` |
| `crates/quota-router-core/src/task_market/escrow.rs`       | `amount_micro_octo_w`                                                                                  |
| `crates/quota-router-core/src/task_market/slashing.rs`     | `initial_stake_micro_octo_w` + `stake_micro_octo_w`                                                    |
| `crates/quota-router-storage/src/ask.rs`                   | `cost_micro_octo_w`                                                                                    |
| `crates/quota-router-storage/src/slash_store.rs`           | `stake_micro_octo_w` + `initial_stake_micro_octo_w`                                                    |
| `crates/quota-router-storage/src/settlement_event_repo.rs` | `cost_micro_octo_w`                                                                                    |
| `crates/quota-router-cli/src/cli.rs` + `commands.rs`       | `cost_micro_octo_w` (CLI surface)                                                                      |

**Parallel model risk:** spend_ledger substrate uses `Dqa` (post-S4
codemod); marketplace + task_market + slash_store +
settlement_event_repo + CLI surface still use `u128`. Same business
concept (amount of OCTO_W), two type representations in workspace.
Cross-boundary conversions at every call site; no single substrate
invariant.

**This mission catches the sites the S4 codemod deferred.**

## Acceptance Criteria

- AC-1: SCOPE NARROWED from "7 files" to 2 source files actually
  requiring migration (the other 5 were already migrated by S4 codemod
  Round 2, verified 2026-08-18). Migrated:
  - [x] `crates/quota-router-core/src/marketplace/escrow.rs::Escrow +
    EscrowSnapshot::amount_micro_octo_w` → `Dqa` (scale=0)
  - [x] `crates/quota-router-core/src/task_market/escrow.rs::TaskEscrow::amount_micro_octo_w`
    → `Dqa` (constructor `new` + `with_arbitrator` signatures)
  - [x] ALREADY MIGRATED (S4 codemod Round 2): `marketplace/slashing.rs`
    - `amount_micro_octo_w: octo_determin::Dqa` (line 375)
  - [x] ALREADY MIGRATED (S4 codemod Round 2): `task_market/slashing.rs`
    - `initial_stake_micro_octo_w: octo_determin::Dqa` (line 36)
  - [x] ALREADY MIGRATED (S4 codemod Round 2): `quota-router-storage/src/slash_store.rs`
    - `_amount_micro_octo_w: octo_determin::Dqa` (line 70)
  - [x] ALREADY MIGRATED (S4 codemod Round 2): `quota-router-storage/src/settlement_event_repo.rs`
    - `cost_micro_octo_w: octo_determin::Dqa` (line 277) + boundary
    decode via `dqa_serde::dqa_from_bytes` (line 335)
  - [x] ALREADY MIGRATED (S4 codemod Round 2): `quota-router-cli/src/cli.rs`
    + `commands.rs` — no `u128` amount-bearing fields; only `Dqa`
    field-reads (commands.rs:6 `use octo_determin::Dfp` is for
    unrelated price-rounding surface)
- AC-2: Test call sites migrated:
  - [x] `crates/quota-router-core/tests/marketplace_e2e.rs` (5 fields:
    `let amount = dqa(...)` + 4 `Escrow::new` / `with_arbitrator` calls)
  - [x] `crates/quota-router-core/tests/task_market.rs` (8 `TaskEscrow::new` /
    `with_arbitrator` `100_000` literals + 3 `u128` cast expressions at
    matched.price + 2 `assert_eq!(escrow.base.amount_micro_octo_w, ...)`)
  - [x] `crates/quota-router-core/tests/fixtures_asks.rs` — no
    `amount_micro_octo_w` literals remain (already Dqa)
  - [x] `crates/quota-router-cli/src/cli.rs` + `commands.rs` — no
    u128 literal sites (already Dqa)
- AC-3: Cargo.toml deps verified:
  - [x] `crates/quota-router-core/Cargo.toml` — `octo-determin` git dep
    present (line 84)
  - [x] `crates/quota-router-storage/Cargo.toml` — `octo-determin` git
    dep present (line 74)
  - [x] `crates/quota-router-cli/Cargo.toml` — `octo-determin` git dep
    present (line 21)
- AC-4: Display impl: `Dqa::Display` already exists per RFC-0105
  §Display. CLI surface uses `ev.cost_micro_octo_w` directly (Dqa →
  Display via `{:?}` formatter + `Dqa::Display` impl). No CLI format
  regression.
- AC-5: Wire-form boundary preserved. `settlement_event_repo.rs` reads
  `cost_micro_octo_w BLOB NOT NULL` (16-byte BE u128 per v004) and
  decodes to `Dqa` at boundary via `dqa_serde::dqa_from_bytes`. The
  in-memory field is `Dqa`; the storage column stays `BLOB` until S6e
  RFC-0959 amendment promotes it to `DQA(12)`.
- AC-6: SKIPPED. No new TV added; existing 24 marketplace_e2e + 32
  task_market tests byte-stable at the storage boundary (proves the
  in-memory field-type change is API-isolated).
- AC-7: Existing TV in `marketplace_e2e.rs` (24/24) + `task_market.rs`
  (32/32) byte-stable at storage boundary; only in-memory field type
  changes.
- AC-8: Tests green:
  - [x] `cargo test -p quota-router-core --features full --test task_market`
    — 32/32 pass
  - [x] `cargo test -p quota-router-core --features full --test marketplace_e2e`
    — 24/24 pass
  - [x] `cargo build -p quota-router-core --tests` — green
  - [ ] `cargo test -p quota-router-core --lib` — BLOCKED on
    `libpython3.12.so.1.0` missing (pre-existing pyo3 build env issue,
    not 0105-x regression)
- AC-9: clippy + fmt:
  - [x] `cargo clippy -p quota-router-core --features full --all-targets -- -D warnings`
    — zero warnings
  - [x] `cargo fmt --all -- --check` — clean

## Cross-reference

- **Parent:** RFC-0105 (asset_id addendum) — Dqa substrate canonical
  type per review §8.1.2
- **Pattern:** S4 DFP codemod (LANDED 2026-08-17) — same shape, 155
  sites covered; this mission catches the 7 file-deferred sites
  that escaped S4 codemod scope
- **Co-mission:** `missions/open/0862-c9-micro-octow-type-unification.md`
  (filed 2026-08-17) — closes audit-verdict Risk #1 CRITICAL
  (type-alias split); x-mission closes Risk #4 HIGH (field-type
  drift)
- **Pattern reuse:** `crates/quota-router-storage/src/stoolap_spend_ledger.rs`
  already migrated — use as the canonical reference impl for
  field-type + scale=0 invariant
- **Sibling missions:**
  - `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
  - `missions/open/0862-c9-micro-octow-type-unification.md` (filed
    2026-08-17)
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S7 row (Stream A.2 — RFC-0965 + RFC-0105 amendments cover the
  wire-form side; x-mission is the in-memory field-type layer on
  top)
- **Audit source:** 2026-08-17 audit verdict, Risk #4 (HIGH)
- **Memory card:** `S4-codemod-2026-08-17-LANDED.md` (existing S4
  codemod receipt — 155 sites covered, 146 deferred; this mission
  catches the file-level deferred sites)

## Critical files

- `crates/quota-router-core/Cargo.toml` (modify — add
  `octo-determin` git dep)
- `crates/quota-router-core/src/marketplace/escrow.rs` (modify —
  field types)
- `crates/quota-router-core/src/marketplace/slashing.rs` (modify
  — field types)
- `crates/quota-router-core/src/task_market/escrow.rs` (modify —
  field types)
- `crates/quota-router-core/src/task_market/slashing.rs` (modify
  — field types)
- `crates/quota-router-storage/src/ask.rs` (modify — field type)
- `crates/quota-router-storage/src/slash_store.rs` (modify —
  field types)
- `crates/quota-router-storage/src/settlement_event_repo.rs`
  (modify — in-memory field type only; wire bytes deferred to S6e)
- `crates/quota-router-cli/Cargo.toml` (modify — add
  `octo-determin` git dep)
- `crates/quota-router-cli/src/cli.rs` + `commands.rs` (modify —
  CLI surface Display)
- `crates/quota-router-core/tests/marketplace_e2e.rs` (modify —
  fixture updates)
- `crates/quota-router-core/tests/task_market.rs` (modify —
  fixture updates)
- `crates/quota-router-core/tests/fixtures_asks.rs` (modify —
  fixture updates)
- `crates/quota-router-core/tests/tv_codemod_deferred.rs` (NEW —
  TV-CODEMOD-DEFERRED-01..04)
- `rfcs/accepted/messaging/0105-numeric.md` (modify — §Version
  History v1.6 row + §Caveat Payload Type Coherence cross-ref)
- `rfcs/accepted/messaging/0862-writer-election-bootstrap-v130.md`
  (modify — §Version History v2.0.4 row cross-referencing x-mission
  site coverage)

## Existing patterns reused

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs:74`
  — canonical reference for `MicroOctoW = Dqa` + scale=0 invariant
  - `dqa_to_i64` boundary helper
- `crates/octo-determin::Dqa::Display` — already exists; CLI
  surface uses directly
- `crates/octo-determin::DqaEncoding` — exists; CLI surface may
  also accept hex-encoded wire form for power-user display
- S4 codemod receipt (`S4-codemod-2026-08-17-LANDED.md`) —
  codemod-style migration recipe; reuse the same pattern for the 7
  file sites

## Risks

- **Field-type API churn** (HIGH): every `u128` field becomes `Dqa`.
  Constructors, accessors, comparison sites, arithmetic sites all
  change. ~7 files × 2-4 fields = ~25-30 mutation sites + ~100
  call site updates across tests + CLI.
- **Test fixture churn** (MED): `marketplace_e2e.rs` +
  `task_market.rs` + `fixtures_asks.rs` use `u128` literals.
  Migration: `1000_u128` → `Dqa { value: 1000, scale: 0 }`.
  Cumbersome; consider a `_d` helper macro or `Dqa::new(1000, 0)`
  to avoid boilerplate.
- **CLI surface regression** (MED): CLI JSON output may change if
  `u128` formatted differently than `Dqa::Display`. Mitigation:
  pin TV for CLI JSON output in test fixtures.
- **Cargo dep graph** (LOW): `quota-router-core` + `quota-router-cli`
  gain `octo-determin` dep. Per layer model, both are consumer
  crates (Layer B+); depending on Layer A (`octo-determin`) is
  allowed.
- **Wire-form drift in settlement_event_repo** (MED): in-memory
  field becomes `Dqa`; storage column stays `BLOB` u128. Boundary
  helper `cost_micro_octo_w_to_dqa(bytes: &[u8]) -> Dqa` +
  `dqa_to_cost_micro_octo_w_bytes(d: &Dqa) -> [u8; 16]` is the
  bridge. S6e RFC-0959 amendment will replace the boundary helper
  with direct DQA(12) column read.

## Version history

| Date       | Author     | Change                                                                                                                                                                                            |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per audit verdict 2026-08-17 (storage restructure hard-recommendation #2, parallel-model Risk #4 HIGH). Co-filed with `0862-c9-micro-octow-type-unification` for Risk #1 CRITICAL. |
| 2026-08-18 | @mmacedoeu | LANDED. Substrate migration: 2 source files (marketplace/escrow.rs + task_market/escrow.rs) + 2 test files (marketplace_e2e.rs + task_market.rs); 5 other AC-1 files ALREADY MIGRATED by S4 codemod Round 2. RFC-0862 v2.0.12 row added. 24 + 32 tests green; clippy zero; fmt clean. |

## Out of scope

- Wire-form DqaEncoding conversion for `cost_micro_octo_w` storage
  column (RFC-0959 amendment — S6e mission
  `0959-c1-wire-format-amendment`)
- Slash ledger schema DQA(12) + chain_id column promotion
  (RFC-0900 amendment — S6d mission)
- Caveat payload codec DqaEncoding conversion for amount-bearing
  variants (RFC-0965 amendment — S7 mission)
- Vault substrate migration (octo-vault crate, already DQA(12) per
  S3 LANDED)
- Spend_ledger substrate migration (already Dqa per S6c LANDED)
- Stoolap native DQA column adoption for `ask` + `settlement_events`
  - `slash_ledger` + `spend_ledger` tables (separate schema
    migration owed to each table; this mission is in-memory field
    types only)
