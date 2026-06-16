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

## Mitigates

IA D-WA-4 (symlink attack assumption — `MISSING` in v1.15 audit)

## Deadline

Pre-public-launch
