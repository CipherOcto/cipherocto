# Mission: 0862-c4 — `dqa_to_i64` assert → error variant

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 1 security review
finding #8: `dqa_to_i64`'s `scale == 0` invariant is an unconditional
`assert!` (panic) reachable from a storage path — panic in a drain
handler is an availability surface.

## Problem

`crates/quota-router-storage/src/stoolap_spend_ledger.rs:274-283`:

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

1. `dqa_to_i64` returns `Result<i64, SpendLedgerError>` (or panic
   in debug builds via `debug_assert_eq!` + error in release)
2. New `SpendLedgerError::InvalidScale { expected: u8, actual: u8 }`
   variant (or reuse `Storage`)
3. All callsites in `seed` + `try_deduct` handle the error (no
   silent `unwrap`)
4. New TV-0862-12: passing `Dqa::new(100, 1)` (scale=1) to `seed`
   yields `SpendLedgerError::InvalidScale`, NOT a panic
5. RFC-0862 §StoolapSpendLedger substrate subsection updated:
   add error variant + precondition clause
6. Existing TV-0862-01..09 stay byte-stable

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Pattern:** `octo_determin::DqaError` already covers scale
  invariant violations upstream; the storage layer's
  `SpendLedgerError::Storage` should wrap + propagate
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Risks

- **API churn** (LOW): changing `dqa_to_i64` return type is a
  private fn, no external caller impact.
- **debug_assert vs error** (LOW): per RFC convention, scale
  invariants are debug-only (Dqa is type-safe at the value layer
  in production via `MicroOctoW = Dqa` alias + caller discipline).
  Keep `debug_assert_eq!` AND add release-mode `InvalidScale` error
  return.

## Version history

| Date       | Author     | Change                                                                                         |
| ---------- | ---------- | ---------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 security review finding #8 (`dqa_to_i64` assert panic surface). |
