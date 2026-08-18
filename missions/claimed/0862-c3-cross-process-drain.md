# Mission: 0862-c3 — Cross-process drain + advisory file lock

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 1 security review
finding #4: TV-0862-08 proves nothing security-relevant; two
`open_path()` instances on the SAME file provide zero mutual
exclusion across the SELECT-then-UPDATE window — classic double-spend
attack surface.

Chosen approach: **BOTH AC-1 (advisory file lock via `fs2`) AND AC-2
(stoolap transaction wrapper)** — the two layers are complementary,
not alternative. Advisory lock = cross-process serialization; stoolap
transaction = atomicity + read-your-own-writes. Single-layer choice
leaves a documented gap (see §Risks in original filing).

## RFC

- Primary: RFC-0862 v2.0 §StoolapSpendLedger substrate §Atomicity
  guarantee (clarify cross-process coordination)
- Co-RFC: none
- Adjacent: `0871e-phase5c-1` (`RaftLikeDrainCoordinator` LANDED
  2026-08-11) for the multi-node consensus case

## Dependency edges

| From                                      | To                             | Why                 | Layer direction     |
| ----------------------------------------- | ------------------------------ | ------------------- | ------------------- |
| `crates/quota-router-storage` (file lock) | `fs2` crate (or `fd-lock` alt) | Cross-process coord | lib → ext-dep       |
| RFC-0862 v2.0 §Atomicity guarantee        | RFC-0862 §DrainCoordinator     | Back-reference      | n/a (RFC text only) |

No new cyclic edges. `fs2` is a small external crate (MIT/Apache-2.0
dual); alt is platform-native `flock(2)` via `nix` crate.

## Problem

Per-instance `drain_lock: Arc<Mutex<()>>` (§struct StoolapSpendLedger
field) provides zero cross-process coordination. Two wallet-node
processes on the same DB file interleave read-modify-write →
double-spend. The current impl comment claims "stoolap per-statement
transaction" coordinates it; that claim is untested and the code
uses non-transactional `query` + separate `execute` (no `BEGIN`, no
`FOR UPDATE`).

The cross-instance IN-MEMORY case is already pinned by TV-0862-08
(per-instance lock scope). The cross-process FILE case is the gap.

## Acceptance Criteria

- [x] AC-1: Advisory file lock (`fs2 = "0.4"`) added to
  `open_path_with_clock` via sibling file `<dsn-dir>/.spend_ledger.lock`.
  `try_lock_exclusive` (non-blocking) on File; surfaces
  `SpendLedgerError::LockUnavailable { path, reason }` on
  contention (fail-closed per AC-1; no deadlock on held locks).
  Lock released on File drop. `open_in_memory*` constructors set
  `lock_file: None`.
- [x] AC-2: `try_deduct` SELECT-then-UPDATE wrapped in stoolap
  `Transaction::begin` → `query` → `execute` → `commit`. Atomicity
  + read-your-own-writes within the same process; advisory lock
  serializes across processes.
- [x] AC-3: TV-0862-11 (new) — file-backed single-instance
  concurrent-deduct: 20 threads × 100 cost on 1000 budget → exactly
  10 succeed, 10 fail with `InsufficientBalance`, final balance 0.
  Validates `drain_lock` + stoolap Transaction together on file path
  matching in-memory path per TV-0862-08.
- [x] AC-3b: TV-0862-11b (new) — `open_path` surfaces
  `SpendLedgerError::LockUnavailable` when external `flock` held on
  `.spend_ledger.lock` (fail-closed contract).
- [x] AC-4: RFC-0862 v2.0.8 Version History row + §Cross-process
  atomicity paragraph inserted before §No-DID-validation convention.
  Documents two-layer model + lock target + non-blocking semantics.
- [x] AC-5: Existing TV-0862-08 (per-instance in-memory) stays;
  TV-0862-11 parallels TV-0862-08 on file path. 18/18 TV green.

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Pre-existing:** `crates/octo-storage-core/` migration runner uses
  stoolap transactions; verify reuse pattern
- **Cross-instance:** mission `0871e-phase5c-1`
  (`RaftLikeDrainCoordinator` LANDED 2026-08-11) for the
  multi-node consensus case
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Critical files

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  — `open_path()` advisory lock + `try_deduct` transaction wrapper)
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (modify
  — add TV-0862-11 file-backed concurrent-deduct)
- `crates/quota-router-storage/Cargo.toml` (add `fs2` dep if option
  AC-1 chosen)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §Atomicity guarantee clarify)

## Out of scope

- Multi-node consensus drain (handled by `RaftLikeDrainCoordinator`)
- Cross-host file locking (NFS / SMB semantics) — document Linux/Unix
  behavior only in this mission
- Migration to mandatory locking (kernel `flock` mandatory mode is
  not portable)

## Risks

- **Advisory lock vs mandatory** (MED): `fs2` advisory locks work
  across processes on Linux/Unix but Windows behavior differs.
  Pin the platform-specific behavior in the substrate doc.
- **Transaction + advisory lock both** (MED): picking one may not
  be enough. Stoolap's per-statement transactions provide atomicity
  but NOT serialization across processes; advisory locks provide
  serialization but NOT atomicity. May need both.
- **Test infra** (LOW): TV-0862-11 needs two `StoolapSpendLedger`
  instances on the same file path with concurrent thread spawning
  — use `std::thread::spawn` (already proven in `deduct_is_atomic_under_concurrent_load`).

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                 |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 security review finding #4 (cross-process double-spend).                                                                                                                                                                |
| 2026-08-17 | @mmacedoeu | Round 2 cleanup: fix phantom `0861-c1` → `0862-c1` parent pointer, drop line refs in Problem + Risks sections, add `## RFC` + `## Dependency edges` + `## Critical files` + `## Out of scope` sections consistent with parent 0862-c1, add AC anchors. |
| 2026-08-18 | @mmacedoeu | LANDED. Two-layer model chosen: AC-1 (`fs2` advisory sibling lock `<dsn-dir>/.spend_ledger.lock`, `try_lock_exclusive` fail-closed) + AC-2 (stoolap `Transaction` wrapper in `try_deduct`). TV-0862-11 (20-thread file-backed concurrent-deduct) + TV-0862-11b (LockUnavailable fail-closed) added. RFC-0862 v2.0.8 Version History row + §Cross-process atomicity paragraph. `SpendLedgerError::LockUnavailable { path, reason }` variant added. New `fs2 = "0.4"` dep + rationale comment in `quota-router-storage/Cargo.toml`. 18/18 TV green, clippy zero, cargo fmt clean. |
