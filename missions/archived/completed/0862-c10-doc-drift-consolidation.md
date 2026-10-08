# Mission: 0862-c10 — §Atomicity doc-drift consolidation (S6c Round 3)

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c3` (cross-process
atomicity LANDED 2026-08-18). Filed per S6c Round 3 adversarial review
(sprint `wf_bd836955-609`, completed 2026-08-18, 204 agents / 4 rounds
/ 106 confirmed findings / loop hit MAX_ROUNDS=4 cap). Consolidates the
doc-drift surface between three sources of truth:

1. `crates/quota-router-storage/src/stoolap_spend_ledger.rs`
   module-level §Atomicity paragraph (lines 9-18 pre-c10)
2. `crates/quota-router-storage/migrations/v007__create_spend_ledger.sql`
   header comment (lines 10-14 pre-c10)
3. `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
   Version History table row v2.0.3 (line 2113 pre-c10) + paragraph
   ordering in v2.0.6..v2.0.8 history

## RFC

- Primary: RFC-0862 v2.0.x history §StoolapSpendLedger §Atomicity
  guarantee (retract stale FOR UPDATE claims)
- Co-RFC: none
- Adjacent: missions 0862-c3 (cross-process atomicity LANDED 2026-08-18)
  + 0862-c8 (seed hardening LANDED 2026-08-17) — both produced real
  mechanism that the doc text pre-dates

## Problem

Pre-c10 module-level §Atomicity paragraph claimed the substrate uses
`SELECT ... FOR UPDATE` row-locking to serialize concurrent drains.
Inspection of substrate line 454: actual SQL is
`SELECT balance FROM spend_ledger WHERE holder_did = ? AND macaroon_id = ?
LIMIT 1` with NO `FOR UPDATE` clause. S6c Round 3 verification confirmed
that the stoolap fork's storage layer returns `NotSupported` for
`FOR UPDATE` locking — the substrate CANNOT add `FOR UPDATE` without
breaking. The actual serialization layers are:

1. Per-instance `drain_lock: Arc<Mutex<()>>` (mission 0862-c8 +
   originally landed in 0862-c1)
2. Cross-process `fs2` advisory flock on `<dsn-dir>/.spend_ledger.lock`
   (mission 0862-c3)
3. Stoolap `Transaction` wrapper in `try_deduct` (mission 0862-c3 AC-2)

Same drift appears in:
- migration v007 header comment (lines 10-14 pre-c10)
- v2.0.3 RFC row (claims `pub type MicroOctoW = Dqa` ADDED to
  `determin/src/lib.rs` AND cites TV-0862-17/TV-0862-18 — neither
  claim survived the same-day c9 RETIRED kill; c9 RETIRED came
  before the v2.0.3 cross-ref commit, so the row was false at the
  moment of writing)
- v2.0.6/v2.0.7/v2.0.8 paragraphs ordered by VERSION number but
  written in opposite physical order due to insertion patterns

## Acceptance Criteria

- [x] AC-1: `stoolap_spend_ledger.rs` §Atomicity paragraph rewritten to
  describe actual mechanism: drain_lock (mission 0862-c8) + tx wrapper
  (mission 0862-c3 AC-2) + cross-process `fs2` flock (mission 0862-c3).
  All FOR UPDATE references RETRACTED; explanatory note that the
  stoolap fork's storage layer returns `NotSupported` for FOR UPDATE
  locking. Phase ordering explicit: drain_lock acquire -> tx begin ->
  query -> dqa_to_i64 -> check -> execute -> commit -> drop.
- [x] AC-2: migration `v007__create_spend_ledger.sql` header comment
  rewritten to match substrate §Atomicity. No FOR UPDATE. Tx wrapper
  and drain_lock documented with mission 0862-c3/c8 attribution.
  Notes c10 as the correction cite.
- [x] AC-3: RFC-0862 v2.0.3 row amended in place to retract the
  inverted "ADD canonical alias" claim (c9 RETIRED killed it).
  Phantom TV-0862-17/TV-0862-18 cites RETRACTED. Sub-row v2.0.3.1
  documents the in-place amend + cites memory card.
- [x] AC-4: New RFC-0862 v2.0.9 row describing the c10 doc-drift
  consolidation work + AC-1/AC-2 substrate changes + AC-3 row amend.
  Status: Draft (follow-on 0862-c10).
- [x] AC-5: 18/18 substrate TV pass without modification (the
  corrected doc accurately describes the as-landed mechanism that the
  existing TV verify).
- [x] AC-6: clippy zero warnings + cargo fmt clean on touched files
  (verified 2026-08-18).

## Cross-reference

- **Parent:** `missions/claimed/0862-c3-cross-process-drain.md` (LANDED 2026-08-18)
- **Audit source:** `plans/sparkling-mapping-kahan.md` Round 3 review output (sprint wf_bd836955-609)
- **Layer direction:** doc-only consolidation (no source code logic change); substrate doc is at the same layer as the implementation it describes.
- **Adjacent:** missions 0862-c9 RETIRED (MicroOctoW kill commit `2a610c3d`), 0862-c4 (InvalidScale typed error), 0862-c6 (no-DID-validation), 0862-c7 (adjacent u64→i64 wrap), 0862-c8 (seed hardening).

## Critical files

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  — §Atomicity paragraph rewrite)
- `crates/quota-router-storage/migrations/v007__create_spend_ledger.sql`
  (modify — header comment rewrite)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — v2.0.3 row in-place amend + v2.0.3.1 sub-row + v2.0.9 row)
- `memory/mission-0862-c10-doc-drift-status.md` (memory card — new)
- `memory/MEMORY.md` (modify — add pointer line)

## Out of scope (filed as separate missions or backlog)

- **Lock-file hardening** (O_NOFOLLOW + path canonicalization + umask
  0600): S6c Round 3 findings 6/7/8 (HIGH severity, lock-bypass /
  path-traversal / TOCTOU-symlink-race). Filed as new mission
  `0871c-lock-file-hardening`. Out of scope for c10 because
  changing the open() path requires a new substrate TV (TV-0862-20)
  + RFC-0862 row describing the lock-file semantics change.
- **Coverage gap TV** (cost=0 / scale boundary / macaroon_id edge /
  InvalidScale boundary): S6c Round 3 TV-coverage findings 13-20.
  Filed as new mission `0862-c11-tv-coverage-gap`.
- **Public-API tightening** (`raw_query` to `pub(crate)` or
  `#[cfg(test)]`): MEDIUM convention-violation finding; deferred —
  no security impact, only testability blast radius.

## Risks

- **RFC v2.0.3 in-place amendment** (LOW): the convention is to add
  new rows, not amend in place. Mitigation: the in-place amend is
  unavoidable because the row's claim is structurally false (ADD
  vs KILL inversion). The v2.0.3.1 sub-row preserves the audit
  trail of the original text + the retraction reason.
- **Paragraph ordering reversal** (LOW): the spec body §Cross-process
  atomicity (v2.0.8) currently appears BEFORE §No-DID-validation
  convention (v2.0.7) in the file due to insertion order. Not fixed
  in c10 (would require a doc-only reorganization commit). Future
  RFC-0862 doc refresh can swap order.
- **Audit reproducibility** (MED): the S6c Round 3 reviewer agents
  wrote scratch files in `/home/mmacedoeu/_w/ai/cipherocto/determin/src/`
  during adversarial verification (rust-analyzer diagnostic noise,
  not a build break since `determin/` is workspace-excluded). Scrubbed
  before commit. Future audit runs should sandbox to a tmpdir.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                 |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-18 | @mmacedoeu | Initial filing per S6c Round 3 adversarial review (sprint wf_bd836955-609). Consolidates 3 doc-drift findings + 1 RFC row inversion. Doc-only mission; no substrate logic change. |
