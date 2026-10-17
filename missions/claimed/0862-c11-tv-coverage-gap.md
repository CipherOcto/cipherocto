# Mission: 0862-c11-tv-coverage-gap — TV-coverage edge cases (S6c Round 3)

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c11` (lock-file
hardening LANDED 2026-08-18, commit `7d653f7e`). Filed per S6c
Round 3 adversarial review (sprint `wf_bd836955-609`) TV-coverage
findings, originally out-of-scope'd in `0862-c10` doc-drift mission.
Closes the remaining S6c Tier-3 backlog. 3 new TV landed (TV-22,
TV-24, TV-25); TV-23 dropped at discovery (`Dqa::new` enforces scale
upper boundary at type layer). AC-1/3/4/5/6 closed; AC-2 dropped.

## RFC

- Primary: RFC-0862 v2.0.x §StoolapSpendLedger §Test Vectors
  (additive on v2.0.10)
- Co-RFC: none
- Adjacent: missions 0862-c2/c4/c6/c11

## Coverage gaps closed

1. **cost=0 edge** (Round 3 TV-coverage finding #13): `try_deduct`
   with `cost = Dqa(0, 0)` MUST succeed and leave balance unchanged.
   Not covered by any existing TV — TV-04 uses `cost = 100`, TV-09
   uses `cost = -1` (rejection path). Zero-cost deduction is a
   meaningful no-op that wallet-node handler may invoke for
   free-tier queries or as a sanity ping.

2. **scale boundary at u8::MAX** (Round 3 TV-coverage finding #14):
   `try_deduct` / `seed` with `cost.scale = 255` (u8::MAX) MUST
   surface `InvalidScale { expected: 0, actual: 255 }`. TV-12 +
   TV-13 cover `scale = 1` only; the upper boundary is the actual
   attack surface for `as u8` truncation bugs in callers.

3. **macaroon_id edge** (Round 3 TV-coverage finding #15): substrate
   takes `macaroon_id: &[u8]` (per mission 0862-c6 — no length /
   format check at substrate). Empty / single-byte / 16-byte /
   >16-byte / binary-garbage all MUST be accepted; substrate contract
   is "any bytes; canonical validation lives at wallet-node boundary."
   TV-14 covers the holder_did axis; no parallel macaroon_id axis TV.

4. **seed() with cost=0** (Round 3 TV-coverage finding #16): paired
   with #13 for the seed side — `seed(holder, mac, Dqa(0, 0))` MUST
   succeed and persist balance=0 row. Distinct from #13 because seed
   takes a `budget` (Dqa) not a `cost` (Dqa) and exercises the
   UPDATE-or-INSERT branch.

## Acceptance Criteria

- [x] AC-1: TV-0862-22 (`tv_0862_22_try_deduct_zero_cost_no_op`):
  pre-seed balance=1000, call `try_deduct(holder, mac, Dqa(0, 0))`,
  assert returned balance = 1000 (unchanged), assert no error.
- [ ] AC-2: ~~TV-0862-23 (`tv_0862_23_try_deduct_scale_u8_max_rejected`)~~
  **DROPPED at discovery.** `Dqa::new(100, 255)` itself rejects at
  construction (`Result::Err(DqaError::InvalidScale)`); substrate
  unreachable for scale=255. The scale upper boundary is enforced at
  the `Dqa` type layer (RFC-0105 + determin-layer tests), not at the
  substrate. AC-2 closed-by-dropping; documented in RFC-0862 v2.0.11
  row under TV-0862-23 (DROPPED).
- [x] AC-3: TV-0862-24 (`tv_0862_24_macaroon_id_accepts_any_bytes`):
  for 4 representative `macaroon_id` shapes (empty slice, single
  byte, canonical 16-byte, 64-byte binary garbage), call `seed +
  balance` and assert the row persists independently per shape.
  Mirrors TV-14 (holder_did axis) for the macaroon_id axis.
- [x] AC-4: TV-0862-25 (`tv_0862_25_seed_zero_budget_persists`):
  `seed(holder, mac, Dqa(0, 0))` succeeds + balance returns Some(0).
  Pairs with AC-1 for seed-side zero edge.
- [x] AC-5: RFC-0862 v2.0.11 row documenting AC-1/3/4 TV additions +
  AC-2 DROPPED note. No new substrate surface; TV-only addition.
- [x] AC-6: clippy zero + cargo fmt clean + 23/23 TV green.

## Cross-reference

- **Parent:** `missions/claimed/0862-c11-lock-file-hardening.md` (LANDED 2026-08-18)
- **Audit source:** S6c Round 3 TV-coverage findings (#13..#16), out-of-scope'd in c10 doc-drift mission
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` §3 row 6 (Stream A.1 S6c follow-on, TV-coverage track)
- **Adjacent:** missions 0862-c2 (Clock precondition), 0862-c4 (InvalidScale), 0862-c6 (no-DID-validation), 0862-c11 (lock-file hardening)

## Critical files

- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`:
  - new `tv_0862_22_try_deduct_zero_cost_no_op`
  - new `tv_0862_23_try_deduct_scale_u8_max_rejected`
  - new `tv_0862_24_macaroon_id_accepts_any_bytes`
  - new `tv_0862_25_seed_zero_budget_persists`
  - file-header TV list update
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`:
  - v2.0.11 row appended
- `memory/mission-0862-c11-tv-coverage-gap-status.md` (memory card)
- `memory/MEMORY.md` (pointer)

## Out of scope (filed separately)

- **raw_query public-API tightening** (Round 3 MEDIUM convention
  violation): `#[cfg(test)]` or `pub(crate)` accessibility. Deferred.
- **Cross-filesystem flock reliability** (Round 3 MEDIUM — FUSE/NFS/
  SMB): substrate can detect via `statfs()` or `MetadataExt` filesystem
  type and surface a documented warning. Separate mission.
- **`Dqa::new` bounds testing** (Round 3 LOW): exhaustively probe
  `value = i64::MIN / MAX`, `scale = 0 / 1 / u8::MAX`. Most covered
  by existing TV (scale=0 + 1) + AC-2 (scale=u8::MAX). Not filed.

## Risks

- **TV-22 cost=0 no-op semantics** (LOW): wallet-node handler at
  `crates/octo-paid-query/src/handlers/` may treat zero-cost as
  "always-allow" without invoking substrate; if it does, TV-22
  asserts substrate behavior independent of handler logic. The TV
  pins the substrate contract, not the handler — both must agree
  for the no-op path to be meaningful.
- **Dqa::new upper-scale bounds** (LOW): per RFC-0862 §Scale
  precondition the substrate enforces scale=0 only; non-zero scale
  rejection is unconditional. AC-2 adds `scale = 255` as the upper
  boundary; `scale = 128`, `64`, etc. would be redundant.
- **macaroon_id canonical length** (LOW): substrate takes raw bytes;
  canonical macaroon_id is 16 bytes per RFC-0957. AC-3 pins the
  substrate's contract (any bytes), not the canonical form. The
  handler enforces canonical length at the boundary.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                 |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-18 | @mmacedoeu | Initial filing per S6c Round 3 TV-coverage findings. Out-of-scope'd in c10 doc-drift mission; closes S6c Tier-3 backlog. 4 new TV + 1 RFC row. |
| 2026-08-18 | @mmacedoeu | LANDED. TV-22 (zero-cost no-op) + TV-24 (macaroon_id edge) + TV-25 (seed zero-budget) added (23/23 green); TV-23 DROPPED at discovery (`Dqa::new(100, 255)` rejects at construction — scale upper boundary enforced at type layer). AC-1/3/4/5/6 closed; AC-2 closed-by-dropping. RFC-0862 v2.0.11 row (incl DROPPED note). |