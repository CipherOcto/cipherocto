# Mission: 0850h-c File-based Refresh Token Rotation

## Status

Claimed (2026-06-02)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Mission 0850h-a handles token refresh in-memory: when the SDK returns
`Http(Unauthorized)` and `refresh_token.is_some()`, the adapter refreshes
and holds the rotated pair in memory, but does NOT rewrite the on-disk
config. For long-running daemons that outlive a single token TTL, this
means a process restart requires re-onboarding or forces the operator to
schedule a graceful restart before the access token expires.

Mission 0850h-c closes that gap: the adapter writes rotated tokens back to
the on-disk config file (atomic rename + lockfile). The on-disk config
becomes the source of truth across restarts.

## Design

- Adapter gains a small `config_writer` module initialized with
  the `MatrixConfig` (which carries `config_path: PathBuf`). On a 401
  + successful refresh, the module writes the new tokens back to
  `config_path`. The `force_writeback: bool` field (also on
  `MatrixConfig`) controls whether the before-snapshot check is
  skipped.
- Write protocol:
  1. Acquire an exclusive `flock` lockfile at `<config>.lock`.
  2. Write the new contents to `<config>.tmp` (mode 0600).
  3. `fs::rename(<config>.tmp, <config>)` (atomic on POSIX, mostly-atomic
     on Windows since Rust 1.65+).
  4. Release the lock.
- A `before` snapshot is taken; if the file changed between the read at
  startup and the writeback, refuse to overwrite and log a warning.
  `MatrixConfig` gains a `force_writeback: bool` field (default
  `false`); when set by the host process, the snapshot check is
  skipped. The default protects against concurrent processes editing
  the same config.
- No new dependency on the `fs2` crate — use the `fs4` crate (which
  wraps `flock` on Unix and `LockFileEx` on Windows) and `tempfile` for
  the temp file.

## Acceptance Criteria

- [ ] `octo-adapter-matrix-sdk` gains a `config_writer` module
- [ ] `MatrixConfig` gains two new fields: `config_path: PathBuf`
      (the on-disk location; empty disables writeback) and
      `force_writeback: bool` (default `false`; when `true`, the
      snapshot check is skipped)
- [ ] On 401 + successful refresh, the rotated `access_token` and
      `refresh_token` are written back to the config file
- [ ] Write uses `fs4` flock + `tempfile` + `fs::rename` for atomicity
- [ ] Concurrent-write protection: a second process attempting to
      acquire `<config>.lock` (via `fs4::FileExt::lock_exclusive`) when
      the lock is held by another process returns
      `Err(WritebackError::LockHeld)`, logs a structured warning naming
      the lockfile path, and does NOT modify `<config>`. The adapter
      surfaces this error to the host process which decides whether to
      retry, log, or fail.
- [ ] Config file mode preserved at 0600 on Unix
- [ ] Integration test from 0850h-a extended: snapshot the config
      file to a tempdir path, truncate the access token in the working
      copy, trigger a 401, assert the working copy is rewritten with
      the new pair. On test exit (pass or fail), restore the snapshot.
      The test must not leave the original config in a truncated state
      under any failure mode.
- [ ] All previous 0850h-a acceptance criteria still pass (no regression)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes

## Location

- `crates/octo-adapter-matrix-sdk/src/config_writer.rs` (new)
- `crates/octo-adapter-matrix-sdk/tests/integration_matrix.rs` (extended)
- `crates/octo-adapter-matrix-sdk/Cargo.toml` adds `fs4` and `tempfile`
  dependencies: `fs4 = { version = "0.7", features = ["sync"] }` and
  `tempfile = "3.8"` in `[dependencies]` (or in the workspace
  `[workspace.dependencies]` table if one exists, with the crate
  referencing the workspace entry)

## Complexity

Medium

## Prerequisites

- Mission 0850h-a: Matrix Auth Onboarding (Planned)

## Implementation Notes

- The `before`-snapshot check is important: without it, two long-running
  processes pointing at the same config could clobber each other's
  refreshes. The lockfile guards against that, but the snapshot check is
  the second line of defense.
- `fs4` is the modern fork of `fs2` and supports both Unix and Windows.
  We use `fs4 = "0.7"` (per the Location section) for the
  `lock_exclusive` API.
- `MatrixConfig` gains a `config_path: PathBuf` field (alongside the
  `force_writeback: bool` field added above). The host process sets
  this when constructing the config; the adapter stores it and the
  `config_writer` module reads it on refresh. If the field is empty,
  the writeback is disabled (useful for in-memory or read-only
  deployments).

## Additional Requirements

(none)

## Follow-up Missions

(none — purely a refinement of 0850h-a)
