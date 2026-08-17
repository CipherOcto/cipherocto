# Mission: 0862-c2 — StoolapSpendLedger Clock trait injection

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17, commit `2750caa7` + Round 1 fixes). Filed per
S6c Round 1 code review finding #10: `seed()` and `try_deduct()`
write `SystemTime::now()` non-deterministically, masked only because
TV fixtures never read `updated_at_unix_ms`.

## Problem

`StoolapSpendLedger::seed` (line 128) and `try_deduct` (line 219) call
`std::time::SystemTime::now()` to populate `updated_at_unix_ms`. The
substrate is non-deterministic across runs; this is masked only
because no TV fixture asserts the column.

The same pattern is already solved in `crates/octo-cap-macaroon/src/vault_lookup.rs`
via a `Clock` trait — reuse that shape.

## Acceptance Criteria

1. `trait Clock` defined in `crates/octa-storage-core/` (or
   `quota-router-storage/` per layer decision):
   - `fn now_unix_ms(&self) -> i64`
   - Default impl: `SystemClock` (production)
   - `MockClock` (test fixture)
2. `StoolapSpendLedger` holds `Arc<dyn Clock>`; constructor accepts
   `Clock` parameter (default `SystemClock`)
3. `seed()` + `try_deduct()` read clock from injected instance
4. TV-0862-10 (new) pins deterministic clock: seed + read back,
   assert `updated_at_unix_ms == expected_unix_ms`
5. Existing TV-0862-01..09 stay byte-stable (no regression)
6. RFC-0862 §StoolapSpendLedger substrate subsection updated:
   add `Clock` to API surface + `Clock` precondition note

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Pattern:** `crates/octo-cap-macaroon/src/vault_lookup.rs` `Clock` trait
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Risks

- **Test parallelism** (LOW): adding a Clock param to public
  constructors is a breaking change for any caller constructing
  `StoolapSpendLedger` with `Default::default()`. Need to make
  `Clock` injectable with a `Default` impl so existing callers
  don't churn.
- **Layer direction** (LOW): `Clock` should live in
  `octo-storage-core` (Layer A substrate) so future crates depend
  on a stable trait; `quota-router-storage` re-exports.

## Version history

| Date       | Author     | Change                                                                                                       |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------ |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 code review finding #10 (SystemTime non-determinism masked by fixture shape). |
