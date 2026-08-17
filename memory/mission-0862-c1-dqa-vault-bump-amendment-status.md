---
name: mission-0862-c1-dqa-vault-bump-amendment-status
description: S6c RFC-0862 v2.0 (Dqa + vault bump) + TV-0862-01..08 byte-exact spend_ledger fixtures LANDED 2026-08-17 (commit 2750caa7). 9/9 TV pass; pre-reqs S3/S4/S5/S6a/S6b all LANDED.
metadata:
  type: project
---

# S6c — RFC-0862 v2.0 Dqa + vault bump (LANDED 2026-08-17)

Mission `0862-c1-dqa-vault-bump-amendment` LANDED 2026-08-17
(commit `2750caa7`). Third S6 sub-session per user split-by-RFC
decision (after S6a RFC-0870 + S6b RFC-0957).

## What landed

- **RFC-0862 v1.4.0 → v2.0 amendment**:
  `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  - §StoolapSpendLedger substrate subsection (new H3 after §DrainCoordinator)
  - §Version History v2.0 row added (Draft status pending review)
  - Back-fills the production substrate spec that v1.4.0 left implicit
    at line 171 + line 1801
- **TV-0862-01..08 byte-exact fixtures**:
  `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (NEW,
  ~480 lines, 9 tests pass):
  - `tv_0862_01_seed_creates_row` — seed inserts new row + balance round-trip
  - `tv_0862_02_balance_read_unknown_returns_none` — pre/post seed None/Some
  - `tv_0862_03_seed_idempotent_last_wins` — upsert semantics (re-mint)
  - `tv_0862_04_try_deduct_atomic_decrement` — happy path decrement
  - `tv_0862_04b_try_deduct_unknown_holder_errors` — UnknownHolder contract
  - `tv_0862_05_dqa_encoding_round_trip` — 16-byte BE DqaEncoding
  - `tv_0862_06_vault_id_derivation_blake3` — BLAKE3 derivation per
    RFC-0960 §20.3
  - `tv_0862_07_dqa_v2_wire_form_pinned` — V2 wire-form on substrate
    side (positive + deduct + byte-stability)
  - `tv_0862_08_multi_instance_in_memory_lock_isolation` — per-instance
    drain_lock scope (cross-instance = 0871e-phase5c-1 territory)

## Pre-reqs (all LANDED 2026-08-17)

- S3 (octo-vault crate) — LANDED
- S4 (Dqa codemod) — LANDED (`19faf380` + `4ab400bd` + `18edbe0d`)
- S5 (verify-time invariant) — LANDED (`d007de54`)
- S6a (RFC-0870 + 1 TV) — LANDED (`c7f99a47` + `ab2b57b4` + Round 2)
- S6b (RFC-0957 + 22 TV) — LANDED (`c9149128` + `4ec9779f` +
  `e5138420` + `57676533`)

## Verify gate (this session)

- `cargo test -p quota-router-storage --test tv_0862_spend_ledger` →
  9/9 pass
- `cargo clippy -p quota-router-storage --all-targets -- -D warnings` →
  clean
- `cargo fmt --all -- --check` → clean
- `cargo test --workspace --lib` → 3 pre-existing S4 DFP
  quota-router-cli::commands::tests::settle_* failures
  (AC #4 explicit exclusion) + 2 pre-existing
  quota-router-storage::stoolap_idempotent_alter failures
  (tests expect v011 but `v012__create_slash_ledger.sql` is now in
  migrations/ — pre-existing test drift, not in S6c scope)

## Plan reference

- `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c) + §4 S6 verify gate (8 spend_ledger TV;
  mission delivers 9 fixtures including 04b regression)

## Round 1 adversarial review (next step)

Per loop-until-dry pattern from S6a + S6b: dispatch 4 parallel
reviewers (spec/code/drift/security) in Round 1, apply findings,
converge when a new round returns zero NEW.

## Push authorization

Commit `2750caa7` queued on `next`. Push user-only per
[[feedback_initiative_user_only]] + [[git-workflow]].

## Next sub-session

S6d RFC-0900 (chain-aware bump) + 10 TV — pending per plan §3 S6 row.

## Cross-reference

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6
- Mission: `missions/open/0862-c1-dqa-vault-bump-amendment.md`
  (status: LANDED 2026-08-17)
- Pre-reqs: `memory/mission-0957-g-verify-time-invariant-status.md` (S5)
  - `memory/mission-0870-c1-version-tag-amendment-status.md` (S6a)
  - `memory/mission-0957-c1-verify-time-amendment-status.md` (S6b)
- Review source: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1 + §20.6.1 + §20.3
