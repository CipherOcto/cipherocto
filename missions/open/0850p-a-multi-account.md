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

## Mitigates

Operational scaling; not a security issue.

## Deadline

Pre-public-launch
