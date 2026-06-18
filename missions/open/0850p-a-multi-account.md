# Mission: 0850p-a — multi-account session store (stoolap-backed)

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work"

## Summary

Replace the current single-DB-per-host model with a stoolap-backed multi-account session store. Operators running multiple WhatsApp accounts (e.g., a personal bot + a business bot on the same gateway) currently have to manage session paths manually. The multi-account store provides `session {list, use, import, export}` and a `whoami --store PATH` flag for multi-account lookups, mirroring the `octo-matrix-onboard` (0850h-a) pattern.

## Design

New `crates/octo-whatsapp-onboard-core/src/multi_account.rs` module:

```rust
pub struct MultiAccountStore {
    db: stoolap::Connection,  // index DB at ~/.local/share/octo/whatsapp/index.stdb
}

pub struct AccountEntry {
    pub account_id: String,          // e.g., "+15551234567"
    pub session_path: PathBuf,        // path to per-account stoolap session DB
    pub config_path: PathBuf,         // path to the per-account WhatsAppConfig.json
    pub linked_at: i64,               // epoch secs
    pub last_used_at: i64,
}

impl MultiAccountStore {
    pub fn list(&self) -> Result<Vec<AccountEntry>, CoreError>;
    pub fn use_account(&mut self, account_id: &str) -> Result<(), CoreError>;
    pub fn import(&mut self, session_db: &Path) -> Result<AccountEntry, CoreError>;
    pub fn export(&self, account_id: &str, out: &Path) -> Result<(), CoreError>;
    pub fn remove(&mut self, account_id: &str) -> Result<(), CoreError>;
}
```

CLI subcommands:
- `session list` (already exists) — extended to read from the index DB
- `session use <ACCOUNT_ID>` — sets the active account (writes to a symlink at `~/.local/share/octo/whatsapp/active`)
- `session import <DB>` — registers an existing session DB
- `session export <ACCOUNT_ID> --out <BUNDLE>` — produces a portable bundle (DB + sidecar + config)
- `whoami --store <PATH>` — overrides the active account for this invocation

## Acceptance Criteria

- [ ] `MultiAccountStore` type with the API above
- [ ] `session {list,use,import,export,remove}` subcommands
- [ ] `whoami --store <PATH>` flag
- [ ] The single-DB-per-host path (`--session-path`) is preserved for backward compatibility
- [ ] Unit tests: store CRUD operations
- [ ] Integration test: import → list → use → whoami round-trip
- [ ] Documentation: migration guide for operators currently using `--session-path`


### Implementation Guide

Companion guide needed: `docs/07-developers/whatsapp-multi-account-implementation-guide.md`. Stoolap schema for the account index, store migration plan, and sidecar management.


### Type Coverage

| RFC-0850p-a Type | Implemented By |
|-----------------|----------------|
| `MultiAccountStore` struct | This mission |
| `session {list,use,import,export}` CLI subcommands | This mission |
| `whoami --store <PATH>` | This mission |

## Dependencies

Depends on:
- RFC-0850p-a status: Accepted
- Stoolap `CipherOcto/stoolap` fork (branch `feat/blockchain-sql`) — required for the multi-account index
- The `whoami` subcommand (mission 0850p-a base, not yet created)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-whatsapp-onboard-core/src/multi_account.rs` (new); `crates/octo-whatsapp-onboard/src/cli.rs` (new subcommands).

## Complexity

Medium (~600 lines; stoolap integration, migration logic, 4 new CLI subcommands).

## Prerequisites

- Stoolap `CipherOcto/stoolap` fork at branch `feat/blockchain-sql`
- Phase 1 (this mission) uses filesystem; Phase 3 (future) migrates to stoolap

## Notes

### Why stoolap?

`docs/BLUEPRINT.md` §'Persistence Convention' mandates any new persistence in CipherOcto uses the `CipherOcto/stoolap` fork. Phase 1 (this mission) uses filesystem only as a transitional step.

### Why not a separate `octo-whatsapp-multi-account` binary?

The `session` subcommands fit naturally in `octo-whatsapp-onboard`; the operator already has this binary installed. A separate binary would split the user-facing surface.

### Why per-host index?

Each host has its own account set; the index is per-host (not global). Cross-host account sharing is a separate concern (out of scope; see mission 0850p-a-session-export).

## Mitigates

Operational scaling; not a security issue.

## Deadline

Pre-public-launch
