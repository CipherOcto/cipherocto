---
name: mission-0862-c11-lock-file-hardening
description: S6c Round 3 lock-file hardening LANDED 2026-08-18 — symlink pre-check + chmod 0o600 + drop truncate; LockPathSymlink variant; TV-20 + TV-21
metadata:
  type: project
---

# Mission 0862-c11 — Lock-file hardening (LANDED 2026-08-18)

## Verdict

S6c Round 3 adversarial review (sprint `wf_bd836955-609`, 204 agents, 4 rounds, 106 confirmed findings) surfaced THREE HIGH security findings around the `.spend_ledger.lock` acquisition surface added by mission 0862-c3 (v2.0.8). All three closed in this mission. Substrate change + new error variant + 2 new TV + RFC-0862 v2.0.10 row.

## Findings closed

### Finding #1 — TOCTOU symlink race (HIGH)

**Surface:** `open_path_with_clock` opened `<dsn-dir>/.spend_ledger.lock` with `OpenOptions::create(true)` without first checking if the path was a symlink. An attacker who could write to the DSN directory could pre-create the lock path as a symlink pointing to an unrelated file (e.g. `/etc/passwd` or another tenant's ledger file), causing `flock(2)` to lock the wrong inode — defeating both serialization (other tenants can write) and auditability (lock acquisition does not correspond to the spend-ledger state file).

**Fix:** pre-open `std::fs::symlink_metadata(&lock_path)` check. If `file_type().is_symlink()`, return `SpendLedgerError::LockPathSymlink { path: String }` (new variant) without opening. ENOENT is treated as fine (file does not yet exist; `create(true)` handles it). Other errors propagate as `SpendLedgerError::Storage`.

**Limitation:** the pre-check narrows the race window but does NOT eliminate it. A strict O_NOFOLLOW fix would require a libc dep (1 line of `unsafe` + `open(O_NOFOLLOW)`); reserved for a separate amendment. The current check is fail-closed against pre-existing symlinks (the common attack surface).

### Finding #2 — Lock-bypass via default umask (HIGH)

**Surface:** `OpenOptions::create(true)` creates the lock file with the process umask. Default umask is `0o022`, yielding `0o644` mode (world-readable). A different uid on the same host could `unlink(.spend_ledger.lock)` + `OpenOptions::create(true).open()` to recreate a fresh inode (with no flock held), then call `flock(2)` to acquire it — defeating serialization across processes.

**Fix:** after `OpenOptions::open`, call `std::fs::set_permissions(&lock_path, Permissions::from_mode(0o600))` to lock the file to owner-only read+write. The `PermissionsExt` trait is imported (`use std::os::unix::fs::PermissionsExt;`) for `from_mode`. Failure (e.g. read-only FS) surfaces as `SpendLedgerError::Storage`.

### Finding #3 — Drop `.truncate(true)` (cleanup)

**Surface:** `OpenOptions::create(true).truncate(true).read(true).write(true)` truncated the lock file on every open. The lock file is empty + not a data-bearing file (only flock target); truncation adds noise without value.

**Fix:** dropped `.truncate(true)`. New shape: `create(true).read(true).write(true)`. No behavioral change for the lock protocol (file is empty before, empty after).

## New substrate surface

### `SpendLedgerError::LockPathSymlink { path: String }`

```rust
/// Lock file path is a symlink (mission 0862-c11 AC-1, S6c Round 3
/// `toctou-symlink-race` HIGH finding). `open_path_with_clock`
/// rejects any pre-existing symlink at `<dsn-dir>/.spend_ledger.lock`
/// to prevent the lock being acquired on an attacker-controlled
/// inode. The substrate is fail-closed — symlinks surface this
/// error rather than silently flocking the symlink target.
#[error("lock path is a symlink: {path}")]
LockPathSymlink {
    /// Path of the lock file that is a symlink.
    path: String,
},
```

## TV additions

- **TV-0862-20** (`tv_0862_20_open_path_rejects_symlink_at_lock_path`): pre-create the lock path as a symlink to `/etc/passwd`, call `open_path`, assert `Err(LockPathSymlink)`. Side-effect check: `symlink_metadata` after the call must STILL show the path as a symlink (substrate did NOT clobber via unlink+recreate).
- **TV-0862-21** (`tv_0862_21_lock_file_permissions_are_0600`): call `open_path`, then `lock_path.metadata().permissions().mode() & 0o777 == 0o600`.

## AC closeout

- AC-1 ✅ Symlink pre-check + `LockPathSymlink` error variant
- AC-2 ✅ `set_permissions(0o600)` after open + new `PermissionsExt` import
- AC-3 ✅ Dropped `.truncate(true)` from `OpenOptions`
- AC-4 ✅ Side-effect check in TV-20 (no clobber)
- AC-5 ✅ TV-0862-20 + TV-0862-21 added; existing 18 TV byte-stable
- AC-6 ✅ RFC-0862 v2.0.10 row added to Version History table
- AC-7 ✅ clippy zero + cargo fmt clean (verified)

## Files changed

- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` — `LockPathSymlink` variant + `PermissionsExt` import + `open_path_with_clock` hardening (symlink pre-check + chmod 0o600 + drop truncate)
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` — TV-0862-20 + TV-0862-21 added
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` — v2.0.10 row appended
- `missions/open/0862-c11-lock-file-hardening.md` → `missions/claimed/0862-c11-lock-file-hardening.md` (LANDED)

## Layer direction

`StoolapSpendLedger` is Layer A (years-stable). The lock-file semantics change is additive on v2.0.8 (cross-process atomicity) and does not touch the substrate's data plane (Dqa storage / scale enforcement / drain_lock). Layer A principle preserved: change is fail-closed (symlink + chmod failures surface as typed errors, not silent corrupt state).

## Related

- [[mission-0862-c3-cross-process-drain]] — v2.0.8 (advisory file lock + tx wrapper) — the parent substrate that c11 hardens.
- [[mission-0862-c10-doc-drift]] — v2.0.9 (doc-only consolidation) — sibling doc-only mission that out-of-scope'd c11.
- [[cipherocto-design-principles]] — Layer A frozen substrate (additive changes only).
- [[feedback-no-fabricated-commit-rule]] — push awaits user instruction.