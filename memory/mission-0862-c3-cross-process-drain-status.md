---
name: mission-0862-c3-cross-process-drain
description: 0862-c3 cross-process atomicity via fs2 advisory file lock + stoolap transaction wrapper — LANDED 2026-08-18
metadata:
  type: project
---

# Mission 0862-c3 — Cross-process drain + advisory file lock (LANDED 2026-08-18)

## Verdict

S6c Round 1 finding #4 closed. Two-layer cross-process atomicity
landed in `crates/quota-router-storage/src/stoolap_spend_ledger.rs`:

1. **Advisory file lock** — sibling file `<dsn-dir>/.spend_ledger.lock`
   opened `create + read + write`, `fs2::FileExt::try_lock_exclusive`
   (non-blocking). On contention surfaces
   `SpendLedgerError::LockUnavailable { path, reason }` (fail-closed).
   Lock released on File drop. `open_in_memory*` sets
   `lock_file: None`. Field: `lock_file: Option<Arc<std::fs::File>>`
   (`Arc` because File: !Clone needed `#[derive(Clone)]`).

2. **Stoolap transaction** — `try_deduct` SELECT-then-UPDATE wrapped
   in `db.begin() -> tx.query -> tx.execute -> tx.commit()`. On
   rollback (InsufficientBalance etc.) `tx` drops, no commit.

Why both layers, not one: advisory lock = cross-process
serialization; transaction = atomicity + read-your-own-writes.
Single-layer choice leaves documented gap.

## AC closeout

- AC-1 ✅ fs2 advisory sibling lock + `LockUnavailable` variant
- AC-2 ✅ stoolap `Transaction` wrapper in `try_deduct`
- AC-3 ✅ TV-0862-11 (20-thread × 100 on 1000 → 10 succeed / 10 fail)
- AC-3b ✅ TV-0862-11b (external flock → `LockUnavailable` surfaced)
- AC-4 ✅ RFC-0862 v2.0.8 Version History row + §Cross-process
  atomicity paragraph (inserted before §No-DID-validation convention)
- AC-5 ✅ TV-0862-08 preserved (per-instance in-memory contract);

18/18 TV green, clippy zero, cargo fmt clean.

## Implementation notes

- **DSN is a DIRECTORY, not a file.** stoolap `file://<dir>` DSN
  expects a directory for WAL + snapshots. Lock target is sibling
  `<dir>/.spend_ledger.lock`, NOT `<dir>/.spend_ledger.lock` inside.
  Initial attempt opened `<path>` directly → `Is a directory (os 21)`.
- **`lock_exclusive` blocks → deadlock.** Initial substrate used
  blocking `fs2::FileExt::lock_exclusive`. TV-11b held external lock
  → hang. Switched to `try_lock_exclusive` (non-blocking) for
  fail-closed semantics. No deadlock on contended locks.
- **`File: !Clone` broke `#[derive(Clone)]`.** Wrapped field as
  `Option<Arc<std::fs::File>>`. `Arc::clone` gives `Clone` for free.

## Out of scope (explicit)

- Multi-node consensus drain — `RaftLikeDrainCoordinator` from
  mission 0871e-phase5c-1 LANDED 2026-08-11 handles that.
- NFS / SMB cross-host — platform-specific; Linux/Unix `flock(2)`
  semantics only.
- Mandatory kernel locks — not portable.

## Files changed

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` —
  `LockUnavailable` variant + `lock_file` field + advisory lock
  in `open_path_with_clock` + tx wrapper in `try_deduct` +
  module-level `## Cross-process atomicity` doc comment.
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` —
  TV-0862-11 + TV-0862-11b + `TV_0862_MACAROON_ID_11` constant
  + file-header TV list update.
- `crates/quota-router-storage/Cargo.toml` — `fs2 = "0.4"` +
  rationale comment (Layer B storage → cross-process coord dep).
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` —
  v2.0.8 Version History row + §Cross-process atomicity paragraph.
- `missions/open/0862-c3-cross-process-drain.md` →
  `missions/claimed/0862-c3-cross-process-drain.md` (LANDED).

## Related

- [[mission-0862-c2-clock-trait]] — Clock precondition (v2.0.6).
- [[mission-0862-c6-fixture-keyspace]] — no-DID-validation (v2.0.7).
- [[mission-0862-c1-dqa-vault-bump-amendment]] — parent mission.
- [[stoolap-general-purpose-db]] — fork persistence discipline.
- [[feedback_stoolap_persistence]] — CipherOcto fork only.
