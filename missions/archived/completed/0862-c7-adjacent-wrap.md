# Mission: 0862-c7 — Adjacent quota-router-core u64→i63 wrap (S4 Round 2 -class)

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 2 security review
finding #3: same S4 Round 2 -class signed-underflow bug at adjacent
module `quota-router-core/src/storage.rs`, NOT covered by Round 1
S6c audit (out of `StoolapSpendLedger` scope but reachable via
`StoolapSpendLedger` callers).

## RFC

- Primary: RFC-0862 (cross-ref adjacent-module signed-underflow
  mitigation)
- Co-RFC: RFC-0105 (Dqa overflow boundary — same shape)
- Adjacent module: `crates/quota-router-core/src/storage.rs` +
  `crates/quota-router-core/src/cache.rs`

## Dependency edges

| From                                                 | To                             | Why                 | Layer direction |
| ---------------------------------------------------- | ------------------------------ | ------------------- | --------------- |
| `crates/quota-router-core` (SpendEvent constructor)  | `determin::Dqa` / `try_into`   | Boundary type check | lib → lib       |
| `crates/quota-router-core::storage.rs` (budget gate) | `SpendLedgerError` / typed err | Wire error type     | lib → lib       |
| `crates/quota-router-core::cache.rs` (budget gate)   | `try_into` validation          | Boundary type check | lib → lib       |

No new cyclic edges. No new external crate deps (`try_into` is stdlib).

## Problem

`crates/quota-router-core/src/storage.rs` — three sites narrow
`cost_amount: u64` to `i64`:

- §budget-gate-deduct-team: budget gate `if current + cost_i64 > budget`
- §budget-gate-deduct-key: budget gate (additional cost path)
- §deduct-octo-w-execute: write `(cost_amount as i64)` into the
  INTEGER column

`crates/quota-router-core/src/cache.rs` — one additional site:

- §cache-eviction-budget-gate: budget gate
  `if current + estimated_max_cost as i64 > key_budget`

If `cost_amount > i64::MAX` (~9.2e18), the cast wraps to negative;
`current + cost_i64` decreases; gate passes. Same shape as the
S4 Round 2 / S6c Round 1 `NegativeCost` attack path that landed
`SpendLedgerError::NegativeCost` for the spend_ledger substrate.

This is an adjacent-module S4 Round 2 -class surface that the S6c
Round 1 audit did not cover (audit was scoped to
`quota-router-storage`).

## Acceptance Criteria

- AC-1: `cost_amount: u64` validated at the `SpendEvent` constructor
  boundary: `cost_amount <= i64::MAX as u64` returns error if exceeded
  (typed `SpendEventError::CostOverflow { cost: u64, max: i64 }`)
- AC-2: All four narrow sites (§budget-gate-deduct-team +
  §budget-gate-deduct-key + §deduct-octo-w-execute +
  §cache-eviction-budget-gate) use `try_into::<i64>()` and map to
  the new error variant
- AC-3: New TV (TV-CORE-COST-OVERFLOW): passing
  `cost_amount = i64::MAX as u64 + 1` to `SpendEvent` constructor
  yields `SpendEventError::CostOverflow`, NOT a silent wrap
- AC-4: Existing TV in `quota-router-core` stay byte-stable
- AC-5: RFC-0862 §StoolapSpendLedger substrate subsection
  cross-references the new error variant + budget-gate invariant
  in adjacent module

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Pattern:** S6c Round 1 fix `SpendLedgerError::NegativeCost` —
  same shape: typed error at boundary instead of silent wrap
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Critical files

- `crates/quota-router-core/src/storage.rs` (modify — three narrow
  sites use `try_into` + new error variant)
- `crates/quota-router-core/src/keys/models.rs` (modify —
  `SpendEvent` constructor validation; `cost_amount: u64` definition
  site)
- `crates/quota-router-core/src/cache.rs` (modify —
  §cache-eviction-budget-gate narrow site per AC-2)
- `crates/quota-router-core/tests/` (add TV-CORE-COST-OVERFLOW)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §StoolapSpendLedger substrate cross-ref new error)

## Out of scope

- Wider audit of all `as i64` casts in `crates/` (defer to
  dedicated cast-safety audit mission; not S6c scope)
- Refactor of `cost_amount: u64` to `Dqa` at the
  `SpendEvent` boundary (separate type-system change; defer)

## Risks

- **API churn** (MED): adding a new error variant to `SpendEventError`
  is a wire-type change visible to all `SpendEvent` consumers. Pin
  via exhaustive match — non-exhaustive match should fail compile.
- **Backwards compat** (LOW): existing callers that pass
  `cost_amount < i64::MAX` see no behavior change. Only the
  overflow path is new.
- **Wire format** (LOW): no on-wire change — the persisted INTEGER
  column still receives `i64`. Only the in-memory boundary is
  hardened.

## Version history

| Date       | Author     | Change                                                                                                                                                                           |
| ---------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 2 security review finding #3 (S4 Round 2 -class adjacent wrap in quota-router-core).                                                                |
| 2026-08-17 | @mmacedoeu | Round 3 expansion: added §cache-eviction-budget-gate to AC-2 (per Round 3 security review finding #1, `cache.rs:211` is same S4 Round 2 -class adjacent wrap missed by Round 2). |
| 2026-08-18 | @mmacedoeu | Close-out audit: `SpendEvent::cost_amount_i64()` boundary guard (in `crates/quota-router-core/src/keys/models.rs` §2-3) + `cost_u64_to_i64` free fn + 4 narrow sites (storage.rs §budget-gate-deduct-team + §budget-gate-deduct-key + §deduct-octo-w-execute + cache.rs §cache-eviction-budget-gate) + 4 TV (at-boundary / at-max / zero / SpendEvent method mirrors free fn) in `tests/tv_0862_c7_cost_overflow.rs` + RFC-0862 v2.0.1 amend at line 1029 + version history v2.0.1 row at line 2111 all already landed. Mission file moved `open/` → `claimed/`. 4/4 TV green. |
| 2026-08-17 | @mmacedoeu | Round 3 cleanup: drop line refs from ## Problem + ## Critical files sections; drop version pins from ## RFC section per CLAUDE.md referencing rule.                              |
