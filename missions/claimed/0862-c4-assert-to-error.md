# Mission: 0862-c4 — `dqa_to_i64` assert → error variant

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 1 security review
finding #8: `dqa_to_i64`'s `scale == 0` invariant is an unconditional
`assert!` (panic) reachable from a storage path — panic in a drain
handler is an availability surface.

## Resolution

- Added `SpendLedgerError::InvalidScale { expected: u8, actual: u8 }`.
- `dqa_to_i64` signature: `fn(Dqa) -> i64` → `fn(Dqa) -> Result<i64, SpendLedgerError>`.
- `seed` + `try_deduct` reject scale != 0 at function ENTRY (precede DB hit, mirror NegativeCost guard).
- Inline `assert_eq!` in `dqa_to_i64` removed; replaced with `if v.scale != 0` runtime Err (no `debug_assert!` so the typed-error path is testable under `cargo test`).
- 2 new byte-exact TV: TV-0862-12 (seed) + TV-0862-13 (try_deduct).
- RFC-0862 v2.0.4 entry + new §Scale precondition subsection.
- All 16 TV in `tv_0862_spend_ledger.rs` green; clippy zero warnings; dependents (`octo-wallet`, `octo-paid-query`) build clean.

## RFC

- Primary: RFC-0862 v2.0 §StoolapSpendLedger substrate (adds
  `SpendLedgerError::InvalidScale` variant to documented error
  surface)
- Co-RFC: RFC-0105 v1.9 (DqaEncoding invariant — substrate boundary
  should propagate scale mismatch as typed error)

## Dependency edges

| From                                              | To                           | Why                | Layer direction     |
| ------------------------------------------------- | ---------------------------- | ------------------ | ------------------- |
| RFC-0862 v2.0 (error variant addition)            | RFC-0862 §StoolapSpendLedger | Same-RFC cross-ref | n/a (RFC text only) |
| `crates/quota-router-storage` (InvalidScale impl) | `determin::DqaError`         | Error wrapping     | lib → lib           |

No new cyclic edges. No new external crate deps.

## Problem

`crates/quota-router-storage/src/stoolap_spend_ledger.rs` —
`fn dqa_to_i64(v: MicroOctoW) -> i64` contains:

```rust
fn dqa_to_i64(v: MicroOctoW) -> i64 {
    assert_eq!(
        v.scale, 0,
        "MicroOctoW stored at scale=0; schema invariant violated"
    );
    v.value
}
```

If a caller ever passes a `Dqa` with `scale != 0`, the substrate
panics. The wallet drain handler panic is a denial-of-service
surface. Better: return `SpendLedgerError::Storage` and let the
handler convert at the boundary.

(Same issue noted for the precondition guard added in TV-09 fix:
`assert_eq!(cost.scale, 0, ...)` in `try_deduct`. Both should be
proper error variants.)

## Acceptance Criteria

- AC-1: `dqa_to_i64` returns `Result<i64, SpendLedgerError>` (or
  panic in debug builds via `debug_assert_eq!` + error in release)
- AC-2: New `SpendLedgerError::InvalidScale { expected: u8, actual: u8 }`
  variant (or reuse `Storage`)
- AC-3: All callsites in `seed` + `try_deduct` handle the error
  (no silent `unwrap`)
- AC-4: New TV-0862-12: passing `Dqa::new(100, 1)` (scale=1) to
  `seed` yields `SpendLedgerError::InvalidScale`, NOT a panic
- AC-5: RFC-0862 §StoolapSpendLedger substrate subsection updated:
  add error variant + precondition clause
- AC-6: Existing TV-0862-01..09b stay byte-stable

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Pattern:** `octo_determin::DqaError` already covers scale
  invariant violations upstream; the storage layer's
  `SpendLedgerError::Storage` should wrap + propagate
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Critical files

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  — `dqa_to_i64` return type + new `InvalidScale` variant + callsite
  error handling)
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (modify
  — add TV-0862-12 scale-mismatch rejection)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §StoolapSpendLedger error variant + precondition)

## Out of scope

- Renaming `dqa_to_i64` to `try_dqa_to_i64` (style-only; defer)
- Wider substrate conversion of `unwrap` / `expect` to typed errors
  (audit mission TBD; not S6c scope)

## Risks

- **API churn** (LOW): changing `dqa_to_i64` return type is a
  private fn, no external caller impact.
- **debug_assert vs error** (LOW): per RFC convention, scale
  invariants are debug-only (Dqa is type-safe at the value layer
  in production via `MicroOctoW = Dqa` alias + caller discipline).
  Keep `debug_assert_eq!` AND add release-mode `InvalidScale` error
  return.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                    |
| ---------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 security review finding #8 (`dqa_to_i64` assert panic surface).                                                                                                            |
| 2026-08-17 | @mmacedoeu | Round 2 cleanup: drop `stoolap_spend_ledger.rs:274-283` line ref, add `## RFC` + `## Dependency edges` + `## Critical files` + `## Out of scope` sections consistent with parent 0862-c1, add AC anchors. |
