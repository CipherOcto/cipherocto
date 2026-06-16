# Mission: 0850p-a — symlink-resolution check on session_path

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work"

## Summary

Reject `session_path` values that resolve to a symlink whose target differs from the user-requested location. Closes the symlink-attack gap where an attacker pre-creates `~/.local/share/octo/whatsapp/session.db` as a symlink to a path the attacker controls, causing the CLI to write the Signal session keys (and thus the WhatsApp identity) into attacker-readable storage.

## Design

In `crates/octo-whatsapp-onboard/src/cli.rs`, after the existing dir-creation step (mode 0700), add:

```rust
fn check_session_path_safe(session_path: &Path) -> Result<(), CoreError> {
    let canon = std::fs::canonicalize(session_path)?;
    let requested = std::fs::canonicalize(session_path.parent().unwrap())?;
    if canon.parent() != Some(requested.as_path()) {
        return Err(CoreError::SessionPathSymlink {
            requested: session_path.display().to_string(),
            resolved: canon.display().to_string(),
        });
    }
    Ok(())
}
```

`CoreError::SessionPathSymlink` is a new variant carrying both paths for diagnostic output. The check runs in `cli.rs` before the adapter is constructed.

## Acceptance Criteria

- [ ] `CoreError::SessionPathSymlink { requested, resolved }` variant added
- [ ] `check_session_path_safe` is called for all session-path-accepting subcommands (qr-link, pair-link, whoami, session verify, session remove, serve-qr)
- [ ] Returns `Err(CoreError::SessionPathSymlink)` with both paths when the canonical path differs from the requested parent
- [ ] Unit test: `tempdir/symlink_to_evil` is rejected with both paths in the error
- [ ] Unit test: a normal `tempdir/session.db` is accepted
- [ ] Integration test: the CLI exits non-zero with the new error when the path is a symlink

## Dependencies

Depends on the base 0850p-a WhatsApp Auth Onboarding RFC being Accepted. No prerequisite missions; this is a security hardening of the existing `octo-whatsapp-onboard` binary.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-whatsapp-onboard-core/src/session.rs` (add canonicalize check after `fs::create_dir_all`).

## Complexity

Low (~30 lines; one new error variant + one new check).

## Prerequisites

- RFC-0850p-a status: Accepted

## Notes

### Why canonicalize and not just check for symlinks?

`fs::metadata` follows symlinks; the check is `canonicalize(target) != canonicalize(parent)`. If the target is a symlink, the canonicalize returns the real path which differs from the parent. A plain `symlink_metadata` check would miss TOCTOU races (the attacker swaps the symlink between the check and the use).

### Why before `start_bot`?

The session DB is opened by TDLib after the symlink check passes. If the check fails, the user gets a clear error before the bot is initialized.

### Type Coverage

| RFC-0850p-a Type | Implemented By |
|-----------------|----------------|
| Symlink resolution check | This mission |
| `CoreError::SessionPathSymlink` error variant | This mission |

### Implementation Guide

Reference: `crates/octo-whatsapp-onboard-core/src/session.rs` (where session_path is created); `std::fs::metadata` and `std::fs::canonicalize` from std.

## Mitigates

IA D-WA-4 (symlink attack assumption — `MISSING` in v1.15 audit)

## Deadline

Pre-public-launch
