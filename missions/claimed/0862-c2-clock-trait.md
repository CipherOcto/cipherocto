# Mission: 0862-c2 — StoolapSpendLedger Clock trait injection

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17, commit `2750caa7` + Round 1 fixes). Filed per
S6c Round 1 code review finding #10: `seed()` and `try_deduct()`
write `SystemTime::now()` non-deterministically, masked only because
TV fixtures never read `updated_at_unix_ms`.

### Resolution

- `StoolapSpendLedger` gains `clock: Arc<dyn Clock>` field; the
  existing `crates/quota-router-storage::clock::Clock` trait
  (`unix_millis() -> u64`, with `SystemClock` + `FixedClock`
  impls wired in mission 0957-c) is reused — no new trait
  definition, no API churn for 0957-c consumers.
- Default constructors (`open_in_memory` / `open_path`) inject
  `Arc::new(SystemClock)`; new `_with_clock(clock: Arc<dyn Clock>)`
  variants accept any caller-supplied clock (production wiring
  may reuse the existing wallet-node clock substrate).
- Two `SystemTime::now()` sites replaced with
  `self.clock.unix_millis() as i64` (cast at use site).
- New test-only `pub fn raw_query(&self, sql: &str,
  params: (Vec<u8>, Vec<u8>)) -> Result<stoolap::Rows,
  SpendLedgerError>` accessor on the substrate so the column
  write can be asserted by TV.
- New TV-0862-10 in `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`
  byte-pins `updated_at_unix_ms = 1_700_000_000_000` via injected
  `FixedClock::new(...)`.
- RFC-0862 v2.0.6 row + `§Clock precondition` paragraph added.

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

- [x] AC-1: `Clock` trait available at the substrate boundary
  (reused `crates/quota-router-storage::clock::Clock::unix_millis() -> u64`,
  not a new trait; `SystemClock` production + `FixedClock` test
  fixture both already exist from mission 0957-c)
- [x] AC-2: `StoolapSpendLedger` holds `clock: Arc<dyn Clock>` field;
  default constructors inject `SystemClock`; `_with_clock` variants
  accept caller-supplied clock
- [x] AC-3: `seed()` + `try_deduct()` read clock from injected
  instance (`self.clock.unix_millis() as i64`)
- [x] AC-4: TV-0862-10 byte-pins `updated_at_unix_ms` via injected
  `FixedClock(1_700_000_000_000)` + raw SQL round-trip
- [x] AC-5: Existing 16 TV stay byte-stable (no regression)
- [x] AC-6: RFC-0862 v2.0.6 row + `§Clock precondition` paragraph
  added to `§StoolapSpendLedger`

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Pattern:** `crates/quota-router-storage::clock::Clock` trait (mission
  0957-c wired the `SystemClock` + `FixedClock` impls)
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
