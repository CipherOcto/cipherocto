# Mission: 0862-c2 — StoolapSpendLedger Clock trait injection

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17, commit `2750caa7` + Round 1 fixes). Filed per
S6c Round 1 code review finding #10: `seed()` and `try_deduct()`
write `SystemTime::now()` non-deterministically, masked only because
TV fixtures never read `updated_at_unix_ms`.

## RFC

- Primary: RFC-0862 v2.0 §StoolapSpendLedger substrate (extends API
  surface with `Clock` injection parameter)
- Co-RFC: RFC-0105 (no change required — Clock trait is
  substrate-local)
- Pattern reuse: RFC-0957 (Caveat DSL) `Clock` trait already used in
  `crates/octo-cap-macaroon/src/vault_lookup.rs`

## Dependency edges

| From                                        | To                                     | Why              | Layer direction     |
| ------------------------------------------- | -------------------------------------- | ---------------- | ------------------- |
| `crates/quota-router-storage` (Clock impl)  | `octo-storage-core::Clock` trait       | Trait dependency | lib → lib           |
| `crates/quota-router-storage::tests` (Mock) | `quota-router-storage::SystemClock`    | Test fixture     | test → lib          |
| RFC-0862 v2.0 amendment (subsection update) | RFC-0862 §StoolapSpendLedger substrate | Back-reference   | n/a (RFC text only) |

No new cyclic edges. No new external crate deps (`std::time::SystemTime`
already in stdlib).

## Problem

`StoolapSpendLedger::seed` (§seed method) and `try_deduct`
(§try_deduct method) call `std::time::SystemTime::now()` to populate
`updated_at_unix_ms`. The substrate is non-deterministic across
runs; this is masked only because no TV fixture asserts the column.

The same pattern is already solved in
`crates/octo-cap-macaroon/src/vault_lookup.rs` via a `Clock` trait —
reuse that shape.

## Acceptance Criteria

- AC-1: `trait Clock` defined in `crates/octo-storage-core/` (or
  `quota-router-storage/` per layer decision):
  - `fn now_unix_ms(&self) -> i64`
  - Default impl: `SystemClock` (production)
  - `MockClock` (test fixture)
- AC-2: `StoolapSpendLedger` holds `Arc<dyn Clock>`; constructor
  accepts `Clock` parameter (default `SystemClock`)
- AC-3: `seed()` + `try_deduct()` read clock from injected instance
- AC-4: TV-0862-10 (new) pins deterministic clock: seed + read back,
  assert `updated_at_unix_ms == expected_unix_ms`
- AC-5: Existing TV-0862-01..09b stay byte-stable (no regression)
- AC-6: RFC-0862 §StoolapSpendLedger substrate subsection updated:
  add `Clock` to API surface + `Clock` precondition note

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Pattern:** `crates/octo-cap-macaroon/src/vault_lookup.rs` `Clock` trait
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Critical files

- `crates/octo-storage-core/src/clock.rs` (NEW — `Clock` trait)
- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  — `Arc<dyn Clock>` field + constructor parameter)
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (modify
  — add TV-0862-10 with `MockClock`)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §StoolapSpendLedger subsection `Clock` note)

## Out of scope

- Cross-process clock synchronization (NTP / monotonic guarantee)
- Clock skew correction (out of substrate scope; wallet-node boundary)
- Migration to `quanta` / `clock_t` crate (defer until profiling
  shows `SystemTime::now()` is hot)

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

| Date       | Author     | Change                                                                                                                                                                                      |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 code review finding #10 (SystemTime non-determinism masked by fixture shape).                                                                                |
| 2026-08-17 | @mmacedoeu | Round 2 cleanup: drop line refs in Problem section, add `## RFC` + `## Dependency edges` + `## Critical files` + `## Out of scope` sections consistent with parent 0862-c1, add AC anchors. |
