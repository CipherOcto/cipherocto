# Mission: 0900-d2 — Stoolap fork upstreaming: native Dqa driver surface

## Status

**OPEN 2026-08-18 (@mmacedoeu).** Filed to unblock 0900-d1 AC-1
(Stoolap fork Dqa driver exposure recon). Closes the third fork
quirk documented in 0900-d (LANDED commit `58c4c2ce`): the fork
lacks `r.get::<Dqa>(idx)` + `tx.set::<Dqa>(idx, value)` codec
surface, forcing i64 bridge at scale=0 with `dqa_to_i64` /
`i64_to_dqa` helpers as the long-term workaround.

## RFC

- Primary: Stoolap fork RFC (specific catalog TBD — file at
  `rfcs/draft/stoolap-fork/dqa-driver.md` in mission scope)
- Co-RFC: RFC-0900 v2.0 (mission 0900-d — chain-aware slash
  ledger substrate)
- Co-RFC: RFC-0105 (Dqa substrate — canonical 16-byte BE
  `DqaEncoding` wire form)

## Dependency edges

| From                                                                | To                         | Why           | Layer direction |
| ------------------------------------------------------------------- | -------------------------- | ------------- | --------------- |
| Stoolap fork `crate::dqa` module (NEW)                              | `octo_determin::Dqa`       | driver impl   | substrate → lib |
| Stoolap fork `Row::get::<Dqa>` + `Statement::set::<Dqa>`            | `octo_determin::Dqa` codec | wire form     | substrate → lib |
| `crates/quota-router-storage/src/slash_store.rs` (modify)           | Stoolap fork `Dqa` driver  | DQA(12) codec | lib → substrate |
| `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify)  | Stoolap fork `Dqa` driver  | DQA(12) codec | lib → substrate |
| `crates/quota-router-storage/src/settlement_event_repo.rs` (modify) | Stoolap fork `Dqa` driver  | DQA(12) codec | lib → substrate |

Dependent: 0900-d (LANDED 2026-08-18, commit `58c4c2ce`).
Unblocks: 0900-d1 AC-1 + AC-2.

## Problem

Stoolap fork at `feat/blockchain-sql` exposes native value types
via `r.get::<T>(idx)` for `i64`, `String`, `Vec<u8>`, `bool`, etc.
The fork does NOT expose `r.get::<Dqa>(idx)` (verified 2026-08-18
at substrate recon; only `r.get::<i64>()` for amount-bearing
slashing/spend-ledger/settlement-event columns).

Forcing i64 bridge at scale=0 via `dqa_to_i64` / `i64_to_dqa`
helpers (LANDED 0900-d) works but:

1. **Substrate invariant leakage** — every consumer that writes
   amount-bearing columns must remember to apply the bridge. Future
   columns at non-zero scale (e.g., fractional OCTO_W fees) need
   the bridge + scale-aware logic. Substrate should own the codec.
2. **Wire form stranding** — canonical `DqaEncoding` 16-byte BE
   wire form is defined in `octo_determin::Dqa::to_bytes()` /
   `Dqa::from_bytes()`. The bridge serializes to i64 text, which
   is a different encoding. Operators reading the column directly
   (via `sqlite3` CLI on the fork) see i64, not the canonical
   16-byte BE form. Substrate-level DQA(12) column type would
   store the canonical form.
3. **Composability cost** — any future column type that needs
   non-zero scale (e.g., `DQA(12)` for fractional fees per
   §Slashing Model penalty percentages) must re-invent the bridge.
   Substrate driver owns the codec once.
4. **Audit verdict Risk #2** — closed at substrate-PK level by
   0900-d. The column-type drift (BIGINT vs Dqa) remains as Risk
   #2 sub-clause until substrate exposes native Dqa.

## Acceptance Criteria

- AC-1: Stoolap fork gains `crate::dqa::Dqa` driver module
  implementing `stoolap::types::SqlValue` (or fork-equivalent
  codec trait) for `octo_determin::Dqa`. Wire form matches
  canonical `DqaEncoding` 16-byte BE.
- AC-2: Stoolap fork exposes `r.get::<Dqa>(idx)` on `Row` /
  `ResultRow` returning `Result<Dqa, stoolap::Error>` (consistent
  with existing `r.get::<i64>(idx)` pattern).
- AC-3: Stoolap fork exposes `tx.set::<Dqa>(idx, value)` (or
  equivalent) for prepared-statement parameter binding. Wire form
  matches `DqaEncoding` 16-byte BE.
- AC-4: Stoolap fork RFC documents the driver surface + wire form
  - integration tests. RFC file location:
    `rfcs/draft/stoolap-fork/dqa-driver.md` (NEW).
- AC-5: Downstream consumers migrate off the i64 bridge:
  - `slash_store.rs::dqa_to_i64` / `i64_to_dqa` → use the substrate
    driver directly
  - `stoolap_spend_ledger.rs` bridge helpers → substrate driver
  - `settlement_event_repo.rs` cost column → Dqa direct
  - (Migration: `v017__dqa_columns.sql` (NEW) to promote
    slasre_ledger/stake/spend_ledger/balance/settlement_event/cost
    columns from BIGINT to DQA(12). Registered in migrations.rs.)
- AC-6: Integration tests in stoolap fork verify codec round-trip:
  - `Dqa::new(900_000, 0)` → column → `Dqa` byte-exact
  - `Dqa::new(900, 5)` round-trip preserves scale=5
  - Empty scale (`Dqa::new(0, 0)`) round-trip
  - Negative values (`Dqa::new(-1, 0)`) round-trip
  - Max value (`Dqa::new(i64::MAX, 0)`) round-trip
- AC-7: No regressions in downstream crates:
  - `cargo test -p quota-router-storage --lib`
  - `cargo test -p quota-router-core --lib --features full`
    (after libpython3.12 infra fix)
  - `cargo test -p octo-vault --lib`
- AC-8: clippy + fmt:
  - `cargo clippy -p quota-router-storage --all-targets -- -D warnings`
  - `cargo clippy -p quota-router-core --all-targets --features full -- -D warnings`
  - `cargo fmt --all -- --check`

## Cross-reference

- **Unblocks:** `missions/open/0900-d1-followon-dqa-codec-hashmap-tuple-outcome.md`
  AC-1 (recon) + AC-2 (DQA(12) column promotion via v016)
- **Pattern:** `octo_determin::Dqa::to_bytes()` /
  `Dqa::from_bytes()` — canonical 16-byte BE wire form already
  defined; substrate driver reuses these.
- **Pattern:** Stoolap fork existing `r.get::<i64>(idx)` /
  `tx.set::<i64>(idx, value)` pattern — Dqa driver follows the
  same shape.
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S6 row 6 (Stream A.1 — RFC-0900 amendment cover, substrate
  layer)
- **Audit source:** 2026-08-17 audit verdict, Risk #2 sub-clause
  (column type drift, HIGH post-PK-fix)

## Critical files

- Stoolap fork `src/dqa.rs` (NEW — Dqa driver impl)
- Stoolap fork `src/row.rs` (modify — add `get::<Dqa>` codec)
- Stoolap fork `src/statement.rs` (modify — add `set::<Dqa>` codec)
- Stoolap fork `Cargo.toml` (modify — add `octo-determin` git dep)
- `rfcs/draft/stoolap-fork/dqa-driver.md` (NEW — fork RFC)
- `crates/quota-router-storage/migrations/v017__dqa_columns.sql`
  (NEW — column promotion, AC-5 migration)
- `crates/quota-router-storage/src/slash_store.rs` (modify — drop
  i64 bridge helpers)
- `crates/quota-router-storage/src/stoolap_spend_ledger.rs`
  (modify — drop i64 bridge helpers)
- `crates/quota-router-storage/src/settlement_event_repo.rs`
  (modify — drop i64 bridge helpers)
- `crates/quota-router-storage/src/migrations.rs` (modify —
  register v017 + close the i64 bridge era)

## Existing patterns reused

- `octo_determin::Dqa::to_bytes()` / `from_bytes()` (canonical
  16-byte BE wire form — driver serializes via these)
- Stoolap fork `r.get::<i64>(idx)` pattern (driver follows the
  same shape modulo the codec impl)
- `crates/quota-router-storage/src/dqa_serde.rs` (consumer-side
  serde for `Dqa` field — substrate driver serializes to the same
  wire form, so in-memory `Dqa` values written via serde are
  byte-identical to substrate-side column reads)

## Risks

- **Fork-side test infra** (MED) — Stoolap fork uses its own test
  harness; integration tests for the driver must land in the fork
  repo, not in cipherocto. Cross-repo coordination overhead.
  Mitigation: file the driver as a self-contained fork module
  with its own test suite.
- **Wire form divergence** (HIGH) — if the fork driver uses a
  different wire form than `DqaEncoding` 16-byte BE, downstream
  consumers reading via substrate and via serde see different
  bytes. Mitigation: AC-1 mandates wire form match
  `Dqa::to_bytes()` byte-exact.
- **Cross-version compatibility** (LOW) — if cipherocto upgrades
  the fork version (e.g., from commit `X` to commit `Y`), the
  driver must still expose `get::<Dqa>` / `set::<Dqa>`. Mitigation:
  integration tests in AC-6 + AC-7 catch breakage.
- **Performance regression** (LOW) — `Dqa` codec via `to_bytes` /
  `from_bytes` is slower than direct i64 read. Mitigation: the
  marketplace slash ledger is not a hot path (slash events are
  rare relative to ask/escrow flows); the perf cost is
  acceptable.

## Version history

| Date       | Author     | Change                                                                                                          |
| ---------- | ---------- | --------------------------------------------------------------------------------------------------------------- |
| 2026-08-18 | @mmacedoeu | Initial filing. Unblocks 0900-d1 AC-1 + AC-2. Closes 0900-d's third documented fork quirk (Dqa driver surface). |

## Out of scope

- Cross-chain slashing coordination (governance-level, separate
  RFC owed)
- Vault-backed stake substrate redesign (RFC-0862 §Future Work F12)
- Fork-side junction to other DFP codecs (DFP is off-limits per
  user constraint; DQA only)
- Substrate-side DFP / DECIMAL / NUMERIC adoption (separate fork
  track; not part of cipherocto substrate requirements)
