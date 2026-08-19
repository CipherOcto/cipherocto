# Mission: 0862-c8 — Seed hardening (TOCTOU + asymmetric NegativeCost + scale assert)

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 2 security review
findings #1 + #2: `StoolapSpendLedger::seed()` has two gaps
uncovered by Round 1 fix (which only added NegativeCost to
`try_deduct`). Round 3 security review finding #2 added third gap:
missing explicit scale=0 assert before `dqa_to_i64` call.

1. **TOCTOU race**: `seed()` balance-read then UPDATE-or-INSERT
   runs WITHOUT `drain_lock` held (try_deduct acquires it).
   Concurrent `seed()` on same `(holder_did, macaroon_id)`:
   - Two concurrent INSERTs hit `spend_ledger` PRIMARY KEY
     violation (per §spend_ledger table PRIMARY KEY definition in
     v007 migration) masked as generic `SpendLedgerError::Storage`
   - UPDATE branch: last-write-wins instead of
     last-writer-overwrites-newer-budget
   - `seed` is the wallet mint path; cross-wallet double-mint is
     the threat model

2. **Asymmetric NegativeCost guard**: Round 1 added
   `if cost.value < 0` in `try_deduct` but NOT in `seed`. Negative
   `budget` passes through `dqa_to_i64` and persists as negative
   `balance`, violating the `scale=0` non-negative invariant the
   Round 1 fix is meant to enforce.

3. **Asymmetric scale=0 assert** (Round 3 addition): `seed()` calls
   `dqa_to_i64` WITHOUT prior scale check (relies on internal
   `assert!`); `try_deduct` checks scale=0 BEFORE calling
   `dqa_to_i64`. Add explicit `assert_eq!(budget.scale, 0, ...)`
   for symmetry. (Note: `assert!` panic surface itself is addressed
   by `0862-c4-assert-to-error`; this mission closes the asymmetry
   gap.)

## RFC

- Primary: RFC-0862 §StoolapSpendLedger substrate (seed() hardening —
  apply same guards as `try_deduct`)
- Co-RFC: none

## Dependency edges

| From                                                  | To                               | Why                    | Layer direction |
| ----------------------------------------------------- | -------------------------------- | ---------------------- | --------------- |
| `crates/quota-router-storage` (seed lock acquisition) | `StoolapSpendLedger::drain_lock` | Same per-instance lock | lib → self      |
| `crates/quota-router-storage` (seed precondition)     | `SpendLedgerError::NegativeCost` | Same error variant     | lib → self      |

No new cyclic edges. No new external crate deps.

## Acceptance Criteria

- AC-1: `seed()` acquires `drain_lock` around the balance-read +
  UPDATE-or-INSERT window (same lock `try_deduct` uses)
- AC-2: `seed()` precondition: `if budget.value < 0 { return
Err(SpendLedgerError::NegativeCost { cost: budget }) }` (matches
  `try_deduct` Round 1 fix)
- AC-3: `seed()` precondition: `assert_eq!(budget.scale, 0, ...)`
  before `dqa_to_i64` call (matches `try_deduct` Round 1 symmetry)
- AC-4: New TV-0862-15: two concurrent `seed()` calls on the same
  `(holder_did, macaroon_id)` → both serialize, second observes
  first's UPDATE; no `PRIMARY KEY` violation surfaces
- AC-5: New TV-0862-16: `seed()` with negative `budget` →
  `SpendLedgerError::NegativeCost` (no DB write, no negative balance
  persisted)
- AC-6: Existing TV-0862-01..05 + 07 + 08 + 04b + 09 + 09b stay
  byte-stable (10 substrate TV total — TV-06 lives in octo-vault)
- AC-7: RFC-0862 §StoolapSpendLedger substrate subsection updated:
  seed() acquires drain_lock + NegativeCost precondition + scale=0
  symmetry

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Sibling:** `missions/open/0862-c3-cross-process-drain.md`
  (cross-process coordination — orthogonal; c8 is in-process)
- **Sibling:** `missions/open/0862-c4-assert-to-error.md`
  (panic surface in `dqa_to_i64` — orthogonal; c8 is precondition
  asymmetry)
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Critical files

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  — `seed()` drain_lock acquisition + NegativeCost precondition +
  scale=0 assert)
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (modify
  — add TV-0862-15 concurrent-seed + TV-0862-16 negative-seed)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §StoolapSpendLedger substrate seed() guard docs)

## Out of scope

- Cross-process seed coordination (covered by `0862-c3`)
- Migration of existing seeded rows to new balance invariant
  (rebuild via seed() with positive budget is the migration path)
- `assert!` → typed-error conversion in `dqa_to_i64` (covered by
  `0862-c4`)

## Risks

- **Lock ordering** (LOW): seed() now acquires drain_lock; if
  try_deduct ever calls seed (unlikely but possible), reentrancy
  risk. Verify call graph before landing.
- **Test parallelism** (LOW): TV-0862-15 needs concurrent
  std::thread::spawn — already proven in
  `deduct_is_atomic_under_concurrent_load`. Reuse pattern.
- **Backwards compat** (LOW): negative-seed rejection is a new
  failure mode; existing callers passing non-negative budgets see
  no change.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                   |
| ---------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 2 security review findings #1 + #2 (seed TOCTOU + asymmetric NegativeCost guard).                                                                                           |
| 2026-08-17 | @mmacedoeu | Round 3 expansion: added AC-3 (scale=0 assert) per Round 3 security review finding #2.                                                                                                                   |
| 2026-08-17 | @mmacedoeu | Round 3 cleanup: drop `v007:24` line ref from ## Problem section; drop `RFC-0862 v2.0` version pin from ## RFC section per CLAUDE.md referencing rule; expand AC-6 to clarify TV-06 lives in octo-vault. |
| 2026-08-18 | @mmacedoeu | Close-out audit: substrate guards (drain_lock + NegativeCost precondition + `SpendLedgerError::InvalidScale` for scale=0 per `0862-c4` typed-error conversion) + TV-0862-15/16 (in `tests/tv_0862_spend_ledger.rs`) + RFC-0862 §StoolapSpendLedger seed hardening amend (line 1040-1051) were all already in place. Mission file moved `open/` → `claimed/`. 23/23 quota-router-storage TV green. |
