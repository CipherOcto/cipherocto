---
name: mission-0862-c8-seed-hardening-status
description: 0862-c8 seed hardening LANDED 2026-08-17 (commit c5a3105d). seed() acquires drain_lock + NegativeCost guard + scale-0 assert mirror try_deduct. 2 new TV (15 concurrent + 16 negative). RFC-0862 §v2.0.2 row + §Seed hardening paragraph.
metadata:
  type: project
---

# 0862-c8 — Seed hardening (LANDED 2026-08-17)

Mission `0862-c8-seed-hardening` LANDED 2026-08-17 (commit
`c5a3105d`). Closes the asymmetric gap in the Round 1 fix (only
`try_deduct` had the NegativeCost guard + `scale == 0` assert +
`drain_lock` acquisition). `seed()` is the wallet mint path; cross-
wallet double-mint is the threat model.

## What landed

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs`:
  - `seed()` acquires `drain_lock` around the balance-read +
    UPDATE-or-INSERT window (mirrors `try_deduct` lock acquisition)
  - `assert_eq!(budget.scale, 0, ...)` precondition (mirrors
    `try_deduct` symmetry)
  - `NegativeCost` precondition guard rejects `budget.value < 0`
    with `SpendLedgerError::NegativeCost { cost: budget }`
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`:
  - TV-0862-15: concurrent `seed()` on same
    `(holder_did, macaroon_id)` serializes via `drain_lock`; no
    PRIMARY KEY violation surfaces; last-writer-wins
  - TV-0862-16: `seed()` with negative budget yields
    `NegativeCost`; no row persisted
  - 2 new `TV_0862_MACAROON_ID_15` + `TV_0862_MACAROON_ID_16`
    byte-pinned fixtures
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`:
  - `§Seed hardening` paragraph in §SpendLedger Substrate
  - `## Version History v2.0.2` row (additive on v2.0.0)

## Verify gate

- `cargo test -p quota-router-storage --test tv_0862_spend_ledger`:
  12/12 pass (10 prior + 15 + 16)
- `cargo test -p quota-router-storage --lib`: 191/191 pass (no
  regressions)
- `cargo test -p quota-router-core --test tv_0862_c7_cost_overflow`:
  4/4 pass
- `cargo test -p octo-vault --test tv_0862_vault_id_cross_ref`:
  3/3 pass
- `cargo clippy --workspace --all-targets --features full -- -D
warnings`: clean
- `cargo fmt --all -- --check`: clean

## Push authorization

Commit `c5a3105d` queued on `next`. Push user-only per
[[feedback_initiative_user_only]] + [[git-workflow]].

## Cross-reference

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)
- Parent: `missions/open/0862-c1-dqa-vault-bump-amendment.md` (S6c)
- Pattern: `StoolapSpendLedger::try_deduct` precondition guards
  (RFC-0862 v2.0 + S4 Round 2) — same shape mirrored
- Sibling: `memory/mission-0862-c7-adjacent-wrap-status.md` (LANDED
  2026-08-17, commit `99fcfcf3`)
- Review source: S6c Round 2 security review findings #1 + #2
  (TOCTOU + asymmetric NegativeCost) + Round 3 cleanup #1 (scale-0
  symmetry)
