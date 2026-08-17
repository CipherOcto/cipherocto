---
name: mission-0862-c7-adjacent-wrap-status
description: 0862-c7 adjacent u64→i64 wrap mitigation LANDED 2026-08-17 (commit 99fcfcf3). SpendEventError::CostOverflow + cost_u64_to_i64 + 4 narrow sites hardened + 4 TV pass. RFC-0862 §v2.0.1 row + §Adjacent-module u64→i64 wrap mitigation paragraph added.
metadata:
  type: project
---

# 0862-c7 — Adjacent u64→i64 wrap mitigation (LANDED 2026-08-17)

Mission `0862-c7-adjacent-wrap` LANDED 2026-08-17 (commit
`99fcfcf3`). S4 Round 2 -class signed-underflow surface at the
`SpendEvent` boundary + `cost_u64_to_i64` free function. Without
this guard, `cost_amount > i64::MAX` (~9.2e18) silently wraps to
negative via `as i64`, defeating the budget-gate comparison.

## What landed

- `crates/quota-router-core/src/keys/models.rs`:
  - New `SpendEventError` enum (thiserror) with `CostOverflow { cost:
u64, max: i64 }` variant
  - Free function `cost_u64_to_i64(cost: u64) -> Result<i64,
SpendEventError>` (fails closed)
  - `SpendEvent::cost_amount_i64()` method delegating to free fn
- `crates/quota-router-core/src/keys/errors.rs`:
  - `KeyError::SpendEvent(SpendEventError)` variant
  - `From<SpendEventError> for KeyError` impl
- `crates/quota-router-core/src/keys/mod.rs`:
  - Re-export `SpendEventError` + `cost_u64_to_i64`
- `crates/quota-router-core/src/storage.rs`:
  - 3 narrow sites hardened: §budget-gate-deduct-team +
    §budget-gate-deduct-key + §deduct-octo-w-execute
- `crates/quota-router-core/src/cache.rs`:
  - §cache-eviction-budget-gate validated
- `crates/quota-router-core/tests/tv_0862_c7_cost_overflow.rs` (NEW):
  - 4 byte-exact TV: exact-edge overflow + at-max passes + zero
    passes + `SpendEvent` method mirrors free fn
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`:
  - `§Adjacent-module u64→i64 wrap mitigation` paragraph in
    §SpendLedger Substrate
  - `## Version History v2.0.1` row (additive on v2.0.0)

## Verify gate

- `cargo test -p quota-router-core --test tv_0862_c7_cost_overflow`:
  4/4 pass
- `cargo test -p quota-router-core --lib`: 1728/1728 pass (no
  regressions)
- `cargo test -p quota-router-storage --test tv_0862_spend_ledger`:
  10/10 pass (TV-01..08 + 04b + 09 + 09b)
- `cargo test -p octo-vault --test tv_0862_vault_id_cross_ref`:
  3/3 pass
- `cargo clippy --workspace --all-targets --features full -- -D
warnings`: clean
- `cargo fmt --all -- --check`: clean

## Push authorization

Commit `99fcfcf3` queued on `next`. Push user-only per
[[feedback_initiative_user_only]] + [[git-workflow]].

## Cross-reference

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)
- Parent: `missions/open/0862-c1-dqa-vault-bump-amendment.md` (S6c)
- Status card: `memory/mission-0862-c1-dqa-vault-bump-amendment-status.md`
- Pattern: `SpendLedgerError::NegativeCost` (RFC-0862 v2.0
  Round 1 fix) — same shape: typed error at boundary instead of
  silent wrap
- Review source: S6c Round 2 security review finding #3 +
  Round 3 cleanup pass expansion (cache.rs:211)
