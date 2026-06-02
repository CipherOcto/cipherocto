# Mission: 0850h-d Persistent Session Storage (stoolap)

## Status

Implemented (2026-06-02) — awaits PR

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Replace the file-per-session model of 0850h-a (one `MatrixConfig` JSON file
per identity) with a `CipherOcto/stoolap`-backed multi-account store,
mirroring the pattern already established in
`crates/quota-router-core/src/secret_manager.rs` and `storage.rs`. This
unblocks multi-identity deployments (one CipherOcto node handling multiple
Matrix accounts) and gives us a single, queryable session inventory.

The storage backend is the **`CipherOcto/stoolap` fork on branch
`feat/blockchain-sql`** — per CipherOcto's project-wide persistence
convention, raw SQLite is **never** used in new persistence layers.

## Design

### Dependency

```toml
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
```

(Pin matches `Cargo.lock` entry
`1ca5d1ae21cf1cfef24899f8fe6a3020ba433687`.)

### Crate layout

A new lib crate `octo-matrix-session-store` (stoolap-backed) plus a thin
integration in `octo-matrix-onboard-core` for `whoami` to read from either
the file (legacy) or the store (new).

```
crates/
  octo-matrix-session-store/           # new, lib only
    src/
      lib.rs
      schema.rs                        # CREATE TABLE, migrations
      store.rs                         # SessionStore trait + impl
      models.rs                        # SessionRow, etc.
  octo-matrix-onboard-core/            # extended to use the store
    src/
      whoami.rs                        # reads from store OR file
  octo-matrix-onboard/                 # new subcommands for store ops
    src/
      modes/
        session_list.rs                # `octo-matrix-onboard session list`
        session_use.rs                 # `octo-matrix-onboard session use <user_id>`
        session_remove.rs              # `octo-matrix-onboard session remove <user_id>`
  octo-adapter-matrix-sdk/             # adapter reads from store by default; file is legacy
    src/
      session_loader.rs                # decides file vs store via MatrixConfig.use_session_store
```

### Schema (one row per `(user_id, device_id)`)

| Column | Type | Notes |
|---|---|---|
| `user_id` | TEXT PRIMARY KEY (composite) | Matrix `@user:server` |
| `device_id` | TEXT PRIMARY KEY (composite) | Matrix device ID |
| `homeserver_url` | TEXT NOT NULL | |
| `access_token` | TEXT NOT NULL | secret — redaction rules apply |
| `refresh_token` | TEXT NULL | |
| `login_type` | TEXT NOT NULL | `password`, `oidc`, `sso`, `qr` |
| `login_timestamp` | INTEGER | epoch seconds |
| `last_used` | INTEGER | epoch seconds, updated on adapter start |
| `position` | INTEGER | for stable multi-account ordering |
| `display_name` | TEXT NULL | cached for UI |
| `avatar_url` | TEXT NULL | cached for UI |

The schema is modeled after EXA's `SessionData` (see
`element-x-android/libraries/session-storage/api/.../SessionData.kt`)
— same conceptual model (one row per `(user_id, device_id)`, columns
for tokens, homeserver, login type, last-used, position) — with
adjustments for the CipherOcto schema naming (`user_id` / `device_id`
vs EXA's `userId` / `deviceId`) and to fit the stoolap type system
(stoolap uses `INTEGER` for epoch seconds vs EXA's `Long`). The
schemas are not wire-compatible; they are structurally analogous.

### Reference architecture

- `crates/quota-router-core/src/secret_manager.rs` — stoolap-backed secret
  store (the closest analogue; re-use its connection-pooling pattern).
- `crates/quota-router-core/src/schema.rs` — stoolap migration pattern.
- `element-x-android/libraries/session-storage/api/SessionStore.kt` —
  the shape of the public interface (`addSession`, `updateData`,
  `getSession`, `getAllSessions`, `getLatestSession`,
  `numberOfSessions`, `setLatestSession`, `removeSession` — the
  Flow-based observers `loggedInStateFlow` / `sessionsFlow` are
  intentionally NOT adopted; see Acceptance Criteria for the
  direct-getter list).

## Acceptance Criteria

- [ ] New crate `crates/octo-matrix-session-store/` (lib, no binary)
- [ ] `Cargo.toml` declares `stoolap` via the CipherOcto fork on
      `feat/blockchain-sql` (pinned commit); no `rusqlite` /
      `sqlx-sqlite` / `diesel-sqlite` / raw `sqlite` crate in
      `[dependencies]` or `[dev-dependencies]`. Verified by
      `cargo tree | grep -iE "sqlite|rusqlite"` returning no
      first-party matches (stoolap's transitive SQLite usage is
      third-party and exempt). A CI step enforces this.
- [ ] Schema migration runs on first init (`CREATE TABLE IF NOT EXISTS
      sessions ...`)
- [ ] `SessionStore` trait + `StoolapSessionStore` impl with: `add_session`,
      `update_data`, `get_session`, `get_all_sessions`,
      `get_latest_session`, `number_of_sessions`, `set_latest_session`,
      `remove_session` (the Flow-based observers from EXA's
      `SessionStore.kt` are replaced by direct getters — a CLI does
      not need a reactive stream)
- [ ] `octo-matrix-onboard session list|use|remove|import` subcommands;
      `import <file>` reads a legacy 0850h-a JSON config and inserts a
      row into the store (refuses to overwrite an existing
      `(user_id, device_id)` row unless `--force` is set)
- [ ] `octo-matrix-onboard whoami` reads from the store (with file fallback
      for legacy configs)
- [ ] `octo-adapter-matrix-sdk` can be configured to load from the
      store; new field `MatrixConfig.use_session_store: bool`
      (default `true` after this mission lands; `false` reads from
      the file path for backward compatibility with 0850h-c-deployed
      configs). `session_loader::load(config)` checks this field.
- [ ] Integration test: store two sessions, list them, switch, remove
      one; also assert `position` is monotonic across inserts,
      `set_latest_session` does not change `position`, and
      `login_timestamp` is immutable after insert
- [ ] All previous 0850h-a acceptance criteria still pass (no regression)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes

## Location

- `crates/octo-matrix-session-store/` (new, lib)
- `crates/octo-matrix-onboard-core/src/whoami.rs` (extended)
- `crates/octo-matrix-onboard/src/modes/session_*.rs` (new subcommands)
- `crates/octo-adapter-matrix-sdk/src/session_loader.rs` (new)

## Complexity

Medium

## Prerequisites

- Mission 0850h-a: Matrix Auth Onboarding (Planned)

## Implementation Notes

- **Persistence convention (project-wide)**: any new persistence uses
  `CipherOcto/stoolap` fork, not raw SQLite. See
  `crates/quota-router-core/src/secret_manager.rs` and `schema.rs` for
  the canonical pattern. Prior mission `0914-a-stoolap-persistence`
  documents the convention.
- **Multi-account ordering**: the `position` and `last_used` columns
  support stable ordering across devices. Pattern from EXA's
  `DatabaseSessionStore.addSession()` — on `add_session`, set
  `position = max(position) + 1` (strictly greater than any existing
  position). On `add_session`, also set `login_timestamp = now` (this
  is immutable after insert) and `last_used = now` (updated on every
  adapter start that successfully loads the session). On
  `set_latest_session`, update only `last_used`; never modify
  `position` on a "latest" update — this preserves the chronological
  multi-account ordering across sessions.
- **Atomic writes**: the store handles its own concurrency; no need for
  the flock pattern from 0850h-c.
- **Migration path from 0850h-a**: the CLI gains
  `octo-matrix-onboard session import <file>` to import existing
  single-file configs into the store.

## Additional Requirements

(none)

## Follow-up Missions

(none — terminal mission in the 0850h auth/storage series)
