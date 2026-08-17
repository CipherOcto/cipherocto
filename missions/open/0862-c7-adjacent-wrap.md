# Mission: 0862-c7 — Adjacent quota-router-core u64→i64 wrap (S4 Round 2 -class)

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 2 security review
finding #3: same S4 Round 2 -class signed-underflow bug at adjacent
module `quota-router-core/src/storage.rs`, NOT covered by Round 1
S6c audit (out of `StoolapSpendLedger` scope but reachable via
`StoolapSpendLedger` callers).

## RFC

- Primary: RFC-0862 v2.0 §StoolapSpendLedger substrate (cross-ref
  adjacent-module signed-underflow mitigation)
- Co-RFC: RFC-0105 v1.9 (Dqa overflow boundary — same shape)
- Adjacent module: `crates/quota-router-core/src/storage.rs`

## Dependency edges

| From                                                 | To                             | Why                 | Layer direction |
| ---------------------------------------------------- | ------------------------------ | ------------------- | --------------- |
| `crates/quota-router-core` (SpendEvent constructor)  | `determin::Dqa` / `try_into`   | Boundary type check | lib → lib       |
| `crates/quota-router-core::storage.rs` (budget gate) | `SpendLedgerError` / typed err | Wire error type     | lib → lib       |

No new cyclic edges. No new external crate deps (`try_into` is stdlib).

## Problem

`crates/quota-router-core/src/storage.rs` — three sites narrow
`cost_amount: u64` to `i64`:

- Line 744: budget gate `if current + cost_i64 > budget`
- Line 911: budget gate (additional cost path)
- Line 1083: write `(cost_amount as i64)` into the INTEGER column

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
- AC-2: All three narrow sites (storage.rs:744/911/1083) use
  `try_into::<i64>()` and map to the new error variant
- AC-3: New TV (TV-CORE-COST-OVERFLOW): passing
  `cost_amount = i64::MAX as u64 + 1` to `SpendEvent` constructor
  yields `SpendEventError::CostOverflow`, NOT a silent wrap
- AC-4: Existing TV in `quota-router-core` stay byte-stable
- AC-5: RFC-0862 v2.0 §StoolapSpendLedger substrate subsection
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
  `SpendEvent` constructor validation; line 131 `cost_amount: u64`
  definition site)
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

| Date       | Author     | Change                                                                                                            |
| ---------- | ---------- | ----------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 2 security review finding #3 (S4 Round 2 -class adjacent wrap in quota-router-core). |
