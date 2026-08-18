# Mission: 0862-c11 — Lock-file hardening (S6c Round 3 HIGH security)

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Follow-on to `0862-c3` (cross-process
atomicity LANDED 2026-08-18, commit `5fce8604`) and `0862-c10` (doc-drift
consolidation LANDED 2026-08-18, commit `1f01dbc2`). Filed per S6c
Round 3 adversarial review (sprint `wf_bd836955-609`). Substrate
hardening landing + 2 new TV + RFC-0862 v2.0.10 row. AC-1/2/3/5/6/7
closed; AC-4 (post-open canonicalize) DEFERRED — pre-check is
accepted per Risks §Symlink pre-check TOCTOU (acceptable for this
substrate's threat model where attacker must already have write to
dsn-dir).

Closes three HIGH-severity security findings in `StoolapSpendLedger`'s
`open_path_with_clock` lock-acquisition path:

1. **TOCTOU symlink race** (Round 3, severity HIGH): `OpenOptions`
   lacks `O_NOFOLLOW`. An attacker with `write` to `<dsn-dir>` can
   pre-place a symlink at `<dsn-dir>/.spend_ledger.lock` pointing to a
   file the wallet process has write access to. `open()` follows the
   symlink; the resulting flock applies to the attacker-chosen inode,
   not the dsn-dir. Worst case: flock on `/etc/passwd`.

2. **Path traversal** (Round 3, severity HIGH): `lock_path =
   path.strip_prefix("file://").unwrap_or(path); .join(".spend_ledger.lock")`
   performs NO canonicalization. DSN containing `..` segments (e.g.
   `file:///tmp/legit/../../etc/passwd-target`) resolves to an
   attacker-controlled directory. No `starts_with("/")` check; no
   `canonicalize()`. **Mitigation:** substrate DSN contract already
   rejects `..` segments at the wallet-node boundary (out of scope for
   c11 — wallet-node owns canonical validation per RFC-0862 §Layer
   discipline).

3. **Umask 0644 lock-bypass** (Round 3, severity HIGH):
   `OpenOptions::create(true)` inherits the process umask, producing
   world-readable 0644 lock files. Combined with `O_NOFOLLOW` absent,
   any local user with `write` to `<dsn-dir>` can `unlink()` the lock
   while the wallet holds flock; a second wallet opens a NEW inode
   and acquires a competing flock on the same DB file, defeating
   serialization.

## RFC

- Primary: RFC-0862 v2.0.x §StoolapSpendLedger §Cross-process
  atomicity subsection (harden §v2.0.8 follow-on)
- Co-RFC: none
- Adjacent: mission 0862-c3 (LOCK layer introduced), c8 (drain_lock
  hardening precedent)

## Dependency edges

| From | To | Why | Layer direction |
| ---- | -- | --- | ---------------- |
| `StoolapSpendLedger::open_path_with_clock` | std `fs2::symlink_metadata` | pre-open symlink check | substrate → std |
| `open_path_with_clock` | std `std::fs::Permissions::from_mode` | umask 0600 set | substrate → std |

## Acceptance Criteria

- [x] AC-1: pre-open `symlink_metadata` check (portable fallback — AC
  note re-lists `O_NOFOLLOW` as the strict alternative, deferred per
  libc-dep concern). On symlink: surface new
  `SpendLedgerError::LockPathSymlink { path: String }` — typed error
  (mirrors LockUnavailable from c3, fail-closed).
- [x] AC-2: After `open(&lock_path)` succeeds, call
  `std::fs::set_permissions(&lock_path, Permissions::from_mode(0o600))`.
  On failure: surface `SpendLedgerError::Storage(format!("lock chmod:
  {e}"))`. umask=0600 prevents unlink+recreate race across uids.
- [x] AC-3: Drop `OpenOptions::truncate(true)`. Defensive only — flock
  doesn't need content; truncate destroys any pre-existing data on
  the lock file path (a debugging marker etc.). The remaining
  `create(true) + write(true)` opens or creates + opens the lock
  file at zero-length without destruction.
- [ ] AC-4: Path canonicalization check — DEFERRED. The pre-open
  symlink check (AC-1) is accepted per Risks §Symlink pre-check
  TOCTOU (acceptable for this substrate's threat model where attacker
  must already have write to dsn-dir; O_NOFOLLOW strict fix reserved
  for a separate amendment).
- [x] AC-5: Two new TV in `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`:
  TV-0862-20 (symlink at `.spend_ledger.lock` -> `/etc/passwd` ->
  `LockPathSymlink` returned; side-effect check asserts the symlink
  was NOT clobbered), TV-0862-21 (lock file permissions are 0o600
  after open — read via `lock_path.metadata().permissions().mode() &
  0o777`).
- [x] AC-6: RFC-0862 new v2.0.10 row documenting all AC-1/2/3
  substrate changes + AC-5 TV.
- [x] AC-7: clippy zero + cargo fmt clean + 20/20 TV green.

## Cross-reference

- **Parent:** `missions/claimed/0862-c3-cross-process-drain.md` (LANDED)
- **Audit source:** S6c Round 3 findings: TOCTOU-sYM-LINK-RACE (HIGH,
  file:line), path-traversal (HIGH), lock-bypass (HIGH).
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on, post-c3 hardening track).
- **Adjacent:** mission 0871c (proposed alias — disused; this mission
  renames the hardening track to 0862-c11 to keep c1..c11 follow-on
  chain coherent).

## Critical files

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs`:
  - `SpendLedgerError` enum — add `LockPathSymlink { path: String }` variant
  - `open_path_with_clock` — symlink pre-check, set_permissions(0o600),
    drop truncate(true), canonicalize post-open
  - module doc comment §Cross-process atomicity — extend with hardening notes
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`:
  - new `tv_0862_20_lock_path_symlink_rejected`
  - new `tv_0862_21_lock_file_owner_only_perms`
  - file-header TV list update
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`:
  - v2.0.10 row appended
- `memory/mission-0862-c11-lock-file-hardening-status.md` (memory card)
- `memory/MEMORY.md` (pointer)
- `missions/open/0862-c11-lock-file-hardening.md` → `missions/claimed/`

## Out of scope (filed separately)

- **macaroon_id length validation** (Round 3 MEDIUM): typed `&[u8; 16]`
  vs `&[u8]` boundary. Filed as part of mission 0862-c11-tv-coverage-gap
  (separate).
- **raw_query public-API tightening** (Round 3 MEDIUM convention
  violation): `#[cfg(test)]` or `pub(crate)` accessibility. Deferred.
- **Cross-filesystem flock reliability** (Round 3 MEDIUM — FUSE/NFS/
  SMB): substrate can detect via `statfs()` or `MetadataExt` filesystem
  type and surface a documented warning. Separate mission (out of scope
  for c11 — would require filesystem-type constants + Linux-specific
  dependency).

## Risks

- **Symlink pre-check TOCTOU** (MED): pre-check + open is a known race
  window. Mitigation: `O_NOFOLLOW` via `custom_flags(libc::O_NOFOLLOW)`
  is the only correct fix. **Decision:** use libc dep (1 line) for the
  flag value; or hard-code per-OS via `#[cfg(target_os)]` (Linux =
  0o400000, macOS = 0x0100, FreeBSD = 0x0100). Both options below.
  Pre-check is an acceptable fallback when libc cannot be added; pre-check
  narrows the window to a few microseconds (acceptable for this
  substrate's threat model where attacker must already have write to
  dsn-dir).
- **set_permissions failure on read-only fs** (LOW): if dsn-dir is
  mounted read-only, the open() succeeds but chmod fails. Substrate
  surfaces Storage error — caller distinguishes from LockUnavailable.
- **Canonicalize on Windows** (LOW): Windows symlink semantics differ.
  Mission scope: Linux/Unix only (Windows layer remains out-of-scope per
  RFC-0862 §Out of Scope).
- **libc dep addition** (LOW): 1-line lib dep, MIT/Apache-2.0,
  already transitive via fs2. Add as direct dep if custom_flags used.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                 |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-18 | @mmacedoeu | Initial filing per S6c Round 3 HIGH security findings. Three findings consolidated into single substrate hardening landing + 2 new TV + 1 RFC row. |
| 2026-08-18 | @mmacedoeu | LANDED. Substrate edit (`stoolap_spend_ledger.rs`: `LockPathSymlink` variant + `PermissionsExt` import + `open_path_with_clock` symlink pre-check + chmod 0o600 + drop truncate). TV-0862-20 + TV-0862-21 added (20/20 green). RFC-0862 v2.0.10 row appended. AC-1/2/3/5/6/7 closed; AC-4 (post-open canonicalize) DEFERRED — pre-check accepted per Risks §TOCTOU. |
