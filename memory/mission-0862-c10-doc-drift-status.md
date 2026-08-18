---
name: mission-0862-c10-doc-drift
description: S6c Round 3 doc-drift consolidation LANDED 2026-08-18 — §Atomicity paragraph + v007 comment + v2.0.3 row + v2.0.9 row
metadata:
  type: project
---

# Mission 0862-c10 — §Atomicity doc-drift consolidation (LANDED 2026-08-18)

## Verdict

S6c Round 3 adversarial review (sprint `wf_bd836955-609`, 204 agents, 4 rounds, 106 confirmed findings) surfaced THREE documentary drift findings closed in this mission. Doc-only; no substrate logic change; existing 18/18 TV pass unmodified.

## Drifts closed

### Drift #1 — Substrate §Atomicity paragraph (`stoolap_spend_ledger.rs:9-18` pre-c10)

**Stale claim:** `SELECT ... FOR UPDATE` row-locking + per-statement transaction.

**Reality:** stoolap fork's storage layer returns `NotSupported` for `FOR UPDATE` locking; substrate SQL never carried the clause. Actual mechanism: per-instance `drain_lock: Arc<Mutex<()>>` wrapping explicit stoolap `Transaction` (`db.begin()` -> `query` -> `execute` -> `commit()`), plus cross-process `fs2` flock on `<dsn-dir>/.spend_ledger.lock`.

**Fix:** rewrote §Atomicity paragraph to enumerate the four-step execution (drain_lock acquire, tx begin, query, dqa_to_i64, check, execute, commit, drop) with explicit attribution to missions 0862-c2/c3/c8. Retracted FOR UPDATE references with explanatory note about stoolap fork limitation.

### Drift #2 — Migration `v007__create_spend_ledger.sql` header comment

**Stale claim:** `SELECT ... FOR UPDATE` row-locking + per-statement transaction.

**Reality:** same as Drift #1.

**Fix:** rewrote header comment to describe drain_lock + tx wrapper. Cited `storage/traits/table.rs` for the FOR UPDATE NotSupported limitation. Attribution to missions 0862-c3/c8 + c10 as the correction cite.

### Drift #3 — RFC-0862 v2.0.3 row inversion + phantom TV cites

**Stale claim 1:** `pub type MicroOctoW = Dqa` was ADDED to `determin/src/lib.rs`.

**Reality:** c9 RETIRED killed `MicroOctoW` project-wide via commit `2a610c3d` (same day, EARLIER than the v2.0.3 cross-ref commit `6df7639c`). The kill landed BEFORE the v2.0.3 row was authored; the row's claim was false at the moment of writing.

**Stale claim 2:** cites TV-0862-17 (cross-crate `MicroOctoW` round-trip) + TV-0862-18 (caveat payload bytes).

**Reality:** neither test exists anywhere under `crates/`. Verified via `grep -rn 'TV-0862-17\|TV-0862-18' crates/` (no matches). c9 RETIRED removed them rather than adding them.

**Fix:**
- v2.0.3 row amended in place with RETRACTION clause bracketing the false text. Survives: type invariant `Dqa.scale == 0` everywhere at substrate boundary (per mission 0862-c4 §Scale precondition).
- New sub-row **v2.0.3.1** documents the in-place amend + cites memory card.
- New row **v2.0.9** describes the c10 doc-drift consolidation work.

## AC closeout

- AC-1 ✅ §Atomicity paragraph rewritten
- AC-2 ✅ v007 header comment rewritten
- AC-3 ✅ v2.0.3 row in-place amend + v2.0.3.1 sub-row
- AC-4 ✅ v2.0.9 new row
- AC-5 ✅ 18/18 TV pass without modification
- AC-6 ✅ clippy zero + cargo fmt clean

## Out of scope (filed / deferred)

- **Lock-file hardening** (O_NOFOLLOW + path canonicalization + umask 0600): S6c Round 3 findings 6/7/8 (HIGH). To file as `0871c-lock-file-hardening` (separate mission; substrate TV-0862-20 needed).
- **TV coverage gaps** (cost=0 / scale boundary / macaroon_id edge / InvalidScale boundary): S6c Round 3 TV-coverage findings. To file as `0862-c11-tv-coverage-gap`.
- **Public API tightening** (`raw_query` to `pub(crate)` or `#[cfg(test)]`): MEDIUM convention violation. Deferred — no security impact.
- **Paragraph ordering** in spec body (v2.0.6/v2.0.7/v2.0.8 reordered by file insertion pattern, not by version number): not fixed in c10; future RFC-0862 doc refresh can swap order.

## Files changed

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` — §Atomicity paragraph rewrite
- `crates/quota-router-storage/migrations/v007__create_spend_ledger.sql` — header comment rewrite
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` — v2.0.3 row in-place amend + v2.0.3.1 sub-row + v2.0.9 row
- `missions/open/0862-c10-doc-drift-consolidation.md` → `missions/claimed/0862-c10-doc-drift-consolidation.md` (LANDED)

## Related

- [[mission-0862-c2-clock-trait]] — Clock precondition (v2.0.6).
- [[mission-0862-c3-cross-process-drain]] — Cross-process atomicity (v2.0.8).
- [[mission-0862-c4-assert-to-error-status]] — Scale precondition (v2.0.4).
- [[mission-0862-c6-fixture-keyspace]] — No-DID-validation (v2.0.7).
- [[mission-0862-c7-adjacent-wrap]] — Adjacent u64→i64 wrap (v2.0.1).
- [[mission-0862-c8-seed-hardening]] — Seed hardening (v2.0.2).
- [[mission-0862-c9-micro-octo-w-canonical-alias-status]] — c9 RETIRED kill (commit `2a610c3d`).
- [[2026-08-17-storage-restructure-plan-active]] — parent storage restructure plan.
- [[cipherocto-design-principles]] — Layer A frozen substrate principle.
