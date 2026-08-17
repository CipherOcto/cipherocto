# Mission: 0105-x — S4 deferred-codemod u128→Dqa sites (marketplace + task_market + slash_store + settlement_event_repo + CLI)

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Filed per audit verdict 2026-08-17
(storage restructure hard-recommendation #2). Closes parallel-model
risk surfaced by audit: 4 distinct amount-bearing field type
representations in production code (`u128`, `Dqa`, `BLOB` u128 wire,
`BIGINT`) at sites NOT covered by S4 DFP codemod (LANDED 2026-08-17
per memory card `S4-codemod-2026-08-17-LANDED.md`).

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

- AC-1: All 7 files migrated from `u128` amount-bearing field types
  to `octo_determin::Dqa` (scale 0 invariant preserved per
  RFC-0862 §StoolapSpendLedger pattern):
  - `marketplace/escrow.rs::amount_micro_octo_w: u128` →
    `Dqa { value, scale }` at scale=0
  - `marketplace/slashing.rs::stake_micro_octo_w + initial_stake_micro_octo_w + amount_micro_octo_w + new_stake_micro_octo_w`
    → `Dqa` fields
  - `task_market/escrow.rs::amount_micro_octo_w` → `Dqa`
  - `task_market/slashing.rs::initial_stake_micro_octo_w + stake_micro_octo_w` → `Dqa`
  - `quota-router-storage/src/ask.rs::cost_micro_octo_w` → `Dqa`
  - `quota-router-storage/src/slash_store.rs::stake_micro_octo_w + initial_stake_micro_octo_w` → `Dqa`
  - `quota-router-storage/src/settlement_event_repo.rs::cost_micro_octo_w`
    → `Dqa` (in-memory); wire bytes still 16-byte BE u128 pending
    S6e RFC-0959 amendment
- AC-2: All call sites of these fields in tests + fixtures migrated:
  - `crates/quota-router-core/tests/marketplace_e2e.rs` (4 fields)
  - `crates/quota-router-core/tests/task_market.rs` (2 fields)
  - `crates/quota-router-core/tests/fixtures_asks.rs`
    (`expected_cost_micro_octo_w`)
  - `crates/quota-router-cli/src/cli.rs` + `commands.rs`
    (CLI surface)
- AC-3: `Cargo.toml` updates:
  - `crates/quota-router-core/Cargo.toml` — add `octo-determin` git dep
  - `crates/quota-router-storage/Cargo.toml` — already has
    `octo-determin`; verify version pin
  - `crates/quota-router-cli/Cargo.toml` — add `octo-determin` git dep
- AC-4: Display impl: `Dqa::Display` already exists per review
  §8.1.2 ("canonical decimal string"). CLI surface switches to
  `Dqa::Display`. No CLI format regression (existing test fixtures
  continue to print canonical decimal form).
- AC-5: Wire-form boundary preserved: `settlement_event_repo.rs` reads
  `cost_micro_octo_w BLOB NOT NULL` (16-byte BE u128 per v004) and
  decodes to `Dqa` at boundary. The in-memory field becomes `Dqa`;
  the storage column stays `BLOB` until S6e RFC-0959 amendment
  promotes it to `DQA(12)`.
- AC-6: New TV (TV-CODEMOD-DEFERRED-01..04): byte-exact
  `u128`-literal → `Dqa`-literal → storage round-trip across
  marketplace escrow, marketplace slashing, task_market escrow,
  settlement_event_repo. 4 fixtures.
- AC-7: Existing TV in `marketplace_e2e.rs` + `task_market.rs` stay
  byte-stable at the storage boundary; only the in-memory field
  type changes
- AC-8: No regressions:
  - `cargo test -p quota-router-core --lib` (115+ existing tests
    pass)
  - `cargo test -p quota-router-storage --lib`
  - `cargo test -p quota-router-cli --lib`
  - `cargo test -p marketplace_strong_scenarios` (e2e)
- AC-9: clippy + fmt:
  - `cargo clippy --workspace --all-targets --features full -- -D warnings`
  - `cargo fmt --all -- --check`

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
