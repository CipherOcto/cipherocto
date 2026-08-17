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

- **Initial LANDED (commit `2750caa7`)**:
  - RFC-0862 v1.4.0 → v2.0 amendment
  - TV-0862-01..08 + TV-0862-04b byte-exact fixtures
    (9 tests in `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`)
- **Round 1 fixes (commit `b20c37dc`)**:
  - **Substrate hardening**: new
    `SpendLedgerError::NegativeCost` + `try_deduct` negative-cost
    precondition guard (defense-in-depth against signed underflow
    per S4 Round 2 + S6c Round 1 security finding #3)
  - **RFC text corrections** (drop phantom line refs + phantom
    RFC-0960 §20.3 + fix crate path `octo-determin/src/dqa.rs:104` →
    `determin/src/dqa.rs:510` + drop non-load-bearing version pins
    - fix 8 → 9 count + clarify Dqa storage form vs wire-form)
  - **Test corrections** (remove MIGRATION_LOCK no-op, TV-05
    schema column round-trip via substrate, TV-04b dedicated
    macaroon_id, TV-07 explicit 16-byte BE byte-array pin + magic
    literal constants, new TV-09 + TV-09b negative-cost rejection)
  - **TV-06 moved**: `crates/octo-vault/tests/tv_0862_vault_id_cross_ref.rs`
    (NEW, 3 tests) — vault_id derivation cross-ref owned by
    octo-vault, uses production `octo_vault::vault_id(...)` not
    local BLAKE3 reimplementation
- **5 follow-on missions filed** (LOW findings deferred):
  - `0862-c2-clock-trait` (SystemTime injection)
  - `0862-c3-cross-process-drain` (file lock + transaction)
  - `0862-c4-assert-to-error` (dqa_to_i64 panic → error)
  - `0862-c5-domain-sep` (untagged hash prefixes sweep)
  - `0862-c6-fixture-keyspace` (test DID production collision risk)

## Round 1 multi-round adversarial review

4 reviewers (spec/code/drift/security) returned:

- 12 HIGH (drift: 3 + spec: 4 + code: 6 + security: 4 — some
  overlap between drift and spec)
- 11 MED (drift: 1 + spec: 3 + code: 5 + security: 4 — some
  overlap)
- 8 LOW (drift: 2 + spec: 3 + code: 3 + security: 4 — some
  overlap)

All HIGH + MED resolved in commit `b20c37dc`. All LOW deferred to
5 follow-on missions (c2..c6) for next session.

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
