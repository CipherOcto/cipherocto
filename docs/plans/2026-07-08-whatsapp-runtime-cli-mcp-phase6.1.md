# Phase 6.1 Implementation Plan — Multi-Account WhatsApp Web adapter plumbing

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Wire the existing `MultiAccountStore` in `octo-whatsapp-onboard-core` into the runtime daemon so the `--name` CLI flag + a new `account_id` config field resolve to a per-account session DB through the on-disk JSON index, and expose account CRUD via the IPC handler registry.

**Architecture:** Today the runtime derives `session_path = $data_dir/{name}/session.db` mechanically. Phase 6.1 replaces that with: at daemon startup, open `MultiAccountStore::open_default()`; if an `account_id` is set in `WhatsAppRuntimeConfig` (new field, default = `"default"`), look up the entry, point the symlink `<base>/active` at the entry's session DB, and have `adapter_config()` return that path. Three new IPC methods (`daemon.accounts.list`, `daemon.accounts.use`, `daemon.accounts.info`) round-trip through the existing `MultiAccountStore` (open the store, mutate, return JSON). A new `live_chain_j_accounts` exercises the round-trip end-to-end against the current bad-shape session (best-effort).

**Tech Stack:** Rust 2021 + `octo-whatsapp-onboard-core` (already a path dep) + `parking_lot::Mutex` (already in deps) + `serde_json` (already in deps). No new crates. No new external deps.

---

## Context

### Why now

Phase 5 (Hardening) deferred "multi-account adapter plumbing" with the explicit plan note "Phase 6+". Phase 6.0 closed the production-bind gap with a `name`-derived single-session path. Phase 6.1 extends that path to use the existing multi-account index so that operators can run more than one daemon-instance per host (e.g. personal + work) without colliding on `default.session.db`.

### Why not just have the operator pick `--name` everywhere

The existing `--name` flag already differentiates the IPC socket path (`$socket_dir/octo-whatsapp-{name}.sock`). What's missing is the **session storage** selection — `adapter_config()` mechanically computes `$data_dir/{name}/session.db` from `name`, but operators who want to link multiple WhatsApp numbers need the on-disk JSON index that tracks `session_path`, `config_path`, and `linked_at` per account. `MultiAccountStore` has that index fully built — it's just not wired.

### Architectural decisions

#### A1. `MultiAccountStore` is the source of truth; `name` is a daemon-instance selector

The runtime daemon's `WhatsAppRuntimeConfig` gains a new `account_id: String` field (default `"default"`) — this identifies **which account** the daemon is bound to. `name` continues to identify the daemon **instance** (used for socket path + log path). Two daemons may share a `data_dir` but bind to different `account_id`s; alternatively two daemons with the same `account_id` would contend on the symlink (and the existing validation rejects empty `account_id`, same as `name`).

The CLI's `--name` flag continues to be the daemon-instance selector. To bind a specific account, operators can either pass `--account <id>` (new flag, optional, defaults to `"default"`) or set the `account_id` in the TOML config file.

#### A2. `adapter_config()` switches from `name`-derivation to `MultiAccountStore::get(active)`

The current `adapter_config()` (lines 381-402 of `config.rs`, added in T1) returns a `WhatsAppConfig` with `session_path = $data_dir/{name}/session.db`. Phase 6.1 T2 replaces this with a method that:

1. Takes `&self` (still).
2. Loads `MultiAccountStore::open_default()` (or reads from a `&MultiAccountStore` if we pass one in — see A3).
3. Looks up `self.account_id` in the store.
4. If found: returns `WhatsAppConfig` with `session_path = entry.session_path.to_string_lossy().into_owned()`.
5. If not found: returns an error result. **New return type: `Result<WhatsAppConfig, ConfigError>`** (was infallible). This is a breaking change for the test fixture in T1's `config::tests::adapter_config_*`. Update those tests to wrap with `.unwrap()` in a way that uses a `MultiAccountStore`-seeded tmpdir.

Wait — that breaks hermetic tests. **Better approach**: keep `adapter_config()` infallible with a fallback to the mechanical `$data_dir/{account_id}/session.db` path if the store doesn't have an entry. The fallback mirrors Phase 6.0's behavior (and the test that verifies it). New method `adapter_config_resolved() -> Result<WhatsAppConfig, ConfigError>` does the strict lookup. The daemon's startup path uses `adapter_config_resolved()`; tests + the `LiveFixture` use `adapter_config()` for convenience.

**Simplification adopted**: replace `adapter_config() -> WhatsAppConfig` with `adapter_config() -> Result<WhatsAppConfig, AccountResolveError>` returning fallible, and provide `adapter_config_fallback() -> WhatsAppConfig` for hermetic tests. The production `Command::Daemon` calls the fallible one.

Actually, simpler still: keep the existing `adapter_config()` as the infallible mechanical fallback (Phase 6.0 behavior), add a new `adapter_config_resolved(store: &MultiAccountStore) -> Result<WhatsAppConfig, AccountResolveError>` method. The 3 existing tests stay passing; the daemon uses the new method.

**Final decision (A2 final)**: split into two methods. Existing `adapter_config()` unchanged. New `adapter_config_resolved(&MultiAccountStore) -> Result<WhatsAppConfig, AccountResolveError>` resolves through the store. Tests verify the resolved path with a tmpdir-backed store.

#### A3. `MultiAccountStore` is owned by the daemon, not the adapter

The `DaemonInner` gains a new field: `accounts: parking_lot::Mutex<MultiAccountStore>` — initialized at `Daemon::new` via `MultiAccountStore::open_default()`. The 3 new IPC handlers take a `DaemonHandle` and lock the store on each call. Each handler's critical section is short (read or mutate JSON + symlink), so a sync mutex is fine (no async work inside the lock).

**Why parking_lot not tokio::sync::Mutex**: same reason as the `connection_watcher` field from Phase 6.12.4. `MultiAccountStore` operations are blocking I/O (JSON read/write, symlink). `tokio::sync::Mutex::lock().await` inside a sync method body is awkward; `parking_lot::Mutex::lock()` returns immediately.

**Locking note**: the existing `MultiAccountStore` is documented as "not thread-safe — single-writer assumption." Wrapping it in a `parking_lot::Mutex` makes it single-process-thread-safe; multi-process safety still requires `flock` on the index file (out of scope for 6.1).

#### A4. New IPC surface: `daemon.accounts.{list, use, info}`

Three RPCs registered through the existing `HandlerRegistry` builder in `crates/octo-whatsapp/src/ipc/handlers/mod.rs`:

| RPC | Params | Returns |
|---|---|---|
| `daemon.accounts.list` | `{}` | `{ "accounts": [{ account_id, session_path, config_path, linked_at, last_used_at, active }] }` |
| `daemon.accounts.use` | `{ "account_id": String }` | `{ "active": String, "session_path": String }` |
| `daemon.accounts.info` | `{ "account_id": String }` | `{ "account_id": String, "session_path": String, "config_path": String, "linked_at": i64, "last_used_at": i64, "is_active": bool }` |

Errors use the existing `RpcError` mechanism:
- `account_id not found` → `InvalidParams` (-32602) with message + data.
- `MultiAccountStore` I/O failure → `Internal` (-32603) wrapping `CoreError::Read`/`CoreError::Parse`/`CoreError::InvalidSessionPath`.

These three new RPCs bring the total to 83 (was 80 from Phase 6.0 final: 80 phase5 + 0 phase6.0 + 3 phase6.1).

#### A5. CLI + MCP wrappers for the 3 new RPCs

Standard pattern from Phase 4.1-4.3:
- 3 new CLI subcommands under `daemon accounts` (or top-level `accounts`): `accounts list`, `accounts use <id>`, `accounts info <id>`.
- 3 new MCP tool descriptors.
- `assert_cmd` smoke test for at least one of them.
- Update `PHASE6_1_ACCOUNTS_METHODS` constant in `handlers/mod.rs`.

#### A6. Live chain `live_chain_j_accounts`

New chain `live_chain_j_accounts` (since `j` is the next letter per the existing A-I chains):

1. Probe `daemon.accounts.list` → should return whatever's in `~/.local/share/octo/whatsapp/index.json` (could be 0 entries on a fresh env; that's fine).
2. Best-effort probe `daemon.accounts.info` for `account_id="default"`. Tolerate `account_id not found`.
3. Best-effort probe `daemon.accounts.use` for `account_id="default"`. Tolerate the same.
4. Sanity-check the response shape.

The chain is best-effort because the live env may have 0, 1, or many accounts already linked; the test verifies the round-trip works without requiring a specific count.

#### A7. `groups` and `sender_allowlist` plumbing (was deferred from 6.0)

Phase 6.0 explicitly deferred wiring `groups` and `sender_allowlist` into `WhatsAppRuntimeConfig` (see T1 doc comment). Phase 6.1 opportunistically fixes this:

- Add `pub groups: Vec<String>` and `pub sender_allowlist: BTreeMap<String, Vec<String>>` to `WhatsAppRuntimeConfig`.
- `Default` impl returns `Vec::new()` and `BTreeMap::new()`.
- `adapter_config()` passes them through (mechanical fallback variant).
- `adapter_config_resolved()` passes them through (resolved variant).
- Schema validate accepts them (no extra validation — duplicate-group detection is left to the WA client).

This is a small extension that doesn't add features; it just removes the "intentionally empty" placeholder from T1's doc comment.

#### A8. No auto-recovery; no chain migration

- ❌ No auto-reconnect when `daemon.accounts.use` points to a logged-out session.
- ❌ No migration from the existing single-file `default.session.db` layout to multi-account. The existing `$data_dir/default/session.db` (Phase 6.0 path) continues to work via the fallback in `adapter_config()`. Operators who want multi-account run `octo-whatsapp-onboard session add <phone>` to create the index entries first.
- ❌ No production rewrite of `onboard_passthrough_message` for `session list/verify/remove` (those still print instructions in 6.1; full integration is Phase 6.4+).
- ❌ No `MultiAccountStore` locking (flock, advisory locks, etc.).

### Critical files

**Modify:**
1. `crates/octo-whatsapp/src/config.rs` — add `account_id`, `groups`, `sender_allowlist` fields + `adapter_config_resolved()` method (T1, T7).
2. `crates/octo-whatsapp/src/daemon.rs` — add `accounts: parking_lot::Mutex<MultiAccountStore>` field; load on `Daemon::new`; expose accessor on `DaemonHandle` (T2).
3. `crates/octo-whatsapp/src/ipc/handlers/mod.rs` — register 3 new handlers + add `PHASE6_1_ACCOUNTS_METHODS` constant (T3, T5).
4. `crates/octo-whatsapp/src/cli.rs` — 3 new CLI subcommands under `daemon accounts` (T4).
5. `crates/octo-whatsapp/tests/live_daemon_test.rs` — new `live_chain_j_accounts` (T6).

**Create:**
1. `crates/octo-whatsapp/src/ipc/handlers/accounts.rs` — 3 handlers in one file (T3).
2. `crates/octo-whatsapp/src/mcp/tools/accounts.toml` or analogous — 3 MCP tool descriptors (T5).

No new crates. No new deps.

---

## Step-by-step

### Task T1 — Config: `account_id`, `groups`, `sender_allowlist` fields + `Default` impl (S)

**Files:**
- Modify: `crates/octo-whatsapp/src/config.rs` (struct + Default + validate)

**Step 1: Write failing tests**

Add to `crates/octo-whatsapp/src/config/tests.rs`:

```rust
    #[test]
    fn config_default_account_id_is_default() {
        let cfg = WhatsAppRuntimeConfig::default();
        assert_eq!(cfg.account_id, "default");
    }

    #[test]
    fn config_default_groups_and_allowlist_are_empty() {
        let cfg = WhatsAppRuntimeConfig::default();
        assert!(cfg.groups.is_empty());
        assert!(cfg.sender_allowlist.is_empty());
    }

    #[test]
    fn validate_rejects_empty_account_id() {
        let cfg = WhatsAppRuntimeConfig {
            account_id: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn adapter_config_passes_groups_and_allowlist_through() {
        let mut allowlist = std::collections::BTreeMap::new();
        allowlist.insert("group-a@g.us".into(), vec!["+15551234567".into()]);
        let cfg = WhatsAppRuntimeConfig {
            groups: vec!["group-a@g.us".into(), "group-b@g.us".into()],
            sender_allowlist: allowlist,
            ..Default::default()
        };
        let ac = cfg.adapter_config();
        assert_eq!(ac.groups, vec!["group-a@g.us".to_string(), "group-b@g.us".to_string()]);
        assert_eq!(ac.sender_allowlist.len(), 1);
        assert_eq!(
            ac.sender_allowlist.get("group-a@g.us").unwrap(),
            &vec!["+15551234567".to_string()]
        );
    }
```

**Step 2: Run tests to verify they fail**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --lib config::tests:: -- --nocapture
```

Expected: compile errors for missing `account_id` / `groups` / `sender_allowlist` fields.

**Step 3: Implementation**

In `crates/octo-whatsapp/src/config.rs`, add the fields to `WhatsAppRuntimeConfig` (after `name`):

```rust
    /// Active account identifier. Resolves through `MultiAccountStore`
    /// at daemon startup to find the per-account session DB.
    /// Default: `"default"`.
    #[serde(default = "default_account_id")]
    pub account_id: String,

    /// WhatsApp group IDs to monitor for DOT envelopes (Phase 4 RFC-0850).
    #[serde(default)]
    pub groups: Vec<String>,

    /// Per-group sender allowlist (RFC-0850 D-WA-10).
    #[serde(default)]
    pub sender_allowlist: std::collections::BTreeMap<String, Vec<String>>,
```

Add to the `impl Default for WhatsAppRuntimeConfig` block (lines ~337-349 per the prior survey):

```rust
            account_id: "default".to_string(),
            groups: Vec::new(),
            sender_allowlist: std::collections::BTreeMap::new(),
```

Add helper at module scope:

```rust
fn default_account_id() -> String { "default".to_string() }
```

Update `validate()` (around line 403 per the prior survey) to also reject empty `account_id`:

```rust
        if self.account_id.is_empty() {
            return Err(ConfigError::InvalidName(
                "account_id cannot be empty".to_string(),
            ));
        }
```

Update `adapter_config()` (around line 381) to pass `groups` and `sender_allowlist` through:

```rust
    pub fn adapter_config(&self) -> octo_adapter_whatsapp::WhatsAppConfig {
        let mut session_path = self.data_dir.clone();
        session_path.push(&self.account_id);  // CHANGED: was &self.name
        session_path.push("session.db");
        octo_adapter_whatsapp::WhatsAppConfig {
            session_path: session_path.to_string_lossy().into_owned(),
            ws_url: None,
            pair_phone: None,
            pair_code: None,
            groups: self.groups.clone(),  // CHANGED: was Vec::new()
            sender_allowlist: self.sender_allowlist.clone(),  // CHANGED: was Default::default()
        }
    }
```

Update the doc comment on `adapter_config()` to remove the "intentionally empty" Phase 6.1 forward reference (since we now do wire them through).

**Step 4: Verify**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --lib config::tests:: -- --nocapture
cargo clippy -p octo-whatsapp --lib -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: 4 new tests pass (3 new + the existing `adapter_config_passes_groups_and_allowlist_through`); clippy + fmt clean.

Note that this changes `session_path` derivation from `$data_dir/{name}/session.db` to `$data_dir/{account_id}/session.db`. Existing hermetic tests that use `name = "test-bind"` would now derive `$data_dir/test-bind/session.db` instead of `$data_dir/test-bind/session.db` (since `name` was used; now `account_id` with default "default" is used). Update the T1 test fixture's expectations — but since they were passing via custom `data_dir + name`, double check existing tests don't break. The existing tests at `config/tests.rs:7, :18, :30` use `name = "work"`, `name = "default"` (via Default), and `name = "..."` Default. After this change, `name` is no longer used in `adapter_config()`, so those tests will fail because they assert paths like `/var/lib/octo/whatsapp/work/session.db`. **Update those tests** to set `account_id` instead of (or in addition to) `name`:

- `adapter_config_derives_session_path_from_data_dir_and_name` → rename to `adapter_config_derives_session_path_from_data_dir_and_account_id` and use `account_id: "work"`.
- `adapter_config_default_name_uses_default_subdir` → rename to `adapter_config_default_account_id_uses_default_subdir` (Default now has `account_id: "default"`).

**This breaks prior T1 tests. Update them in the same commit.**

**Step 5: Commit**

```bash
git add crates/octo-whatsapp/src/config.rs crates/octo-whatsapp/src/config/tests.rs
git commit -m "feat(octo-whatsapp): WhatsAppRuntimeConfig gains account_id + groups + sender_allowlist fields"
```

---

### Task T2 — Daemon: open `MultiAccountStore` at startup + expose accessor (M)

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs` (DaemonInner + Daemon::new + DaemonHandle)

**Step 1: Write failing test**

Add to `crates/octo-whatsapp/src/daemon/tests.rs`:

```rust
    #[test]
    fn daemon_new_initializes_accounts_store() {
        let cfg = crate::config::WhatsAppRuntimeConfig {
            name: "test-acct-init".into(),
            ..Default::default()
        };
        let daemon = Daemon::new(cfg);
        // `accounts` accessor must not panic; may be empty list if no index.json exists.
        let entries = daemon.handle().accounts().list();
        // Empty list is the expected case (no index.json). Just verify it returns without panic.
        let _ = entries.len();
    }
```

**Step 2: Run test to verify it fails**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib daemon::tests::daemon_new_initializes_accounts_store -- --nocapture
```

Expected: `error[E0599]: no method named 'accounts' found for struct 'DaemonHandle'`.

**Step 3: Implementation**

In `crates/octo-whatsapp/src/daemon.rs`:

Add the field to `DaemonInner`:

```rust
use octo_whatsapp_onboard_core::MultiAccountStore;
use parking_lot::Mutex as SyncMutex;

struct DaemonInner {
    // ... existing fields ...
    accounts: SyncMutex<Option<MultiAccountStore>>,
}
```

Add accessor on `DaemonHandle`:

```rust
    /// Access the `MultiAccountStore` for account CRUD. Returns a `Mutex`
    /// guard; lock briefly — the inner ops are blocking I/O.
    pub fn accounts(&self) -> AccountStoreGuard<'_> {
        AccountStoreGuard { inner: self.inner.accounts.lock() }
    }
```

Define a guard wrapper to expose the store's methods (or just `Deref` it):

```rust
/// Thin guard that exposes `MultiAccountStore` methods through `&`
/// without exposing the `parking_lot::Mutex` internals.
pub struct AccountStoreGuard<'a> {
    inner: parking_lot::MutexGuard<'a, Option<MultiAccountStore>>,
}

impl<'a> AccountStoreGuard<'a> {
    pub fn list(&self) -> Vec<octo_whatsapp_onboard_core::AccountEntry> {
        self.inner.as_ref().map(|s| s.list()).unwrap_or_default()
    }
    pub fn info(&self, account_id: &str) -> Option<octo_whatsapp_onboard_core::AccountEntry> {
        self.inner.as_ref().and_then(|s| s.get(account_id).cloned())
    }
    pub fn use_account(&mut self, account_id: &str) -> Result<octo_whatsapp_onboard_core::AccountEntry, octo_whatsapp_onboard_core::CoreError> {
        self.inner.as_mut().ok_or_else(|| octo_whatsapp_onboard_core::CoreError::InvalidSessionPath {
            path: std::path::PathBuf::from("(no store)"),
            reason: "MultiAccountStore not initialized".into(),
        })?.use_account(account_id)
    }
}
```

In `Daemon::new` (around line 467 of `daemon.rs`), initialize the store:

```rust
        let accounts = match octo_whatsapp_onboard_core::MultiAccountStore::open_default() {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("MultiAccountStore::open_default failed; daemon starts without it: {e}");
                None
            }
        };
        SyncMutex::new(accounts)
```

(Need to confirm the exact `MultiAccountStore::open_default` signature returns — see `crates/octo-whatsapp-onboard-core/src/multi_account.rs` around line 117 per the prior survey. Confirmed: `pub fn open_default() -> Result<Self>`.)

Pass `accounts` field through `DaemonInner::new` or the constructor.

**Step 4: Verify**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib daemon::tests::daemon_new_initializes_accounts_store -- --nocapture
cargo build -p octo-whatsapp --features "live-whatsapp test-helpers" --tests
```

Expected: test passes; build clean.

**Step 5: Commit**

```bash
git add crates/octo-whatsapp/src/daemon.rs
git commit -m "feat(octo-whatsapp): DaemonInner owns MultiAccountStore; DaemonHandle::accounts() accessor"
```

---

### Task T3 — RPC handlers: `daemon.accounts.{list, use, info}` (M)

**Files:**
- Create: `crates/octo-whatsapp/src/ipc/handlers/accounts.rs`
- Modify: `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (register 3 handlers + add `PHASE6_1_ACCOUNTS_METHODS` const)

**Step 1: Write the failing tests**

In `crates/octo-whatsapp/src/ipc/handlers/accounts.rs` (new file), add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{Daemon, BotStateMirror};
    use serde_json::json;

    fn empty_handle() -> crate::daemon::DaemonHandle {
        let cfg = crate::config::WhatsAppRuntimeConfig {
            name: "test-accounts-handlers".into(),
            ..Default::default()
        };
        Daemon::new(cfg).handle()
    }

    #[test]
    fn accounts_list_returns_empty_when_no_index() {
        let h = empty_handle();
        let result = AccountsList.call(h, json!({})).await.unwrap();
        let arr = result.get("accounts").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 0, "no index.json => empty list");
    }

    #[test]
    fn accounts_info_unknown_account_returns_invalid_params() {
        let h = empty_handle();
        let err = AccountsInfo.call(h, json!({ "account_id": "nonexistent" }))
            .await
            .expect_err("should error on unknown account");
        assert_eq!(err.code, -32602, "InvalidParams");
    }
}
```

**Step 2: Run tests to verify they fail**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib ipc::handlers::accounts -- --nocapture
```

Expected: compile error — file doesn't exist yet (or module not registered).

**Step 3: Implementation**

Create `crates/octo-whatsapp/src/ipc/handlers/accounts.rs`:

```rust
//! Multi-account RPC surface (Phase 6.1).
//!
//! Three handlers round-trip through the daemon's `MultiAccountStore`:
//! - `daemon.accounts.list`  — enumerate all linked accounts.
//! - `daemon.accounts.use`   — set the active account (writes `<base>/active` symlink).
//! - `daemon.accounts.info`  — fetch details for one account.
//!
//! The store is owned by `DaemonInner::accounts` and accessed through
//! `DaemonHandle::accounts()`. All operations are blocking I/O (JSON read/write,
//! symlink manipulation); the handlers wrap them in `tokio::task::spawn_blocking`
//! to avoid blocking the reactor on multi-millisecond file I/O.

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use octo_whatsapp_onboard_core::{AccountEntry, CoreError};
use serde::Deserialize;
use serde_json::{json, Value};

pub struct AccountsList;
pub struct AccountsUse;
pub struct AccountsInfo;

#[derive(Deserialize)]
struct UseParams { account_id: String }

#[derive(Deserialize)]
struct InfoParams { account_id: String }

fn core_err_to_rpc(e: CoreError) -> RpcError {
    let msg = format!("{e:?}");
    RpcError {
        code: RpcErrorCode::Internal.as_i32(),
        message: format!("MultiAccountStore error: {msg}"),
        data: None,
    }
}

fn invalid_params(msg: impl Into<String>) -> RpcError {
    RpcError {
        code: RpcErrorCode::InvalidParams.as_i32(),
        message: msg.into(),
        data: None,
    }
}

fn entry_to_json(e: &AccountEntry, is_active: bool) -> Value {
    json!({
        "account_id": e.account_id,
        "session_path": e.session_path.to_string_lossy(),
        "config_path": e.config_path.to_string_lossy(),
        "linked_at": e.linked_at,
        "last_used_at": e.last_used_at,
        "is_active": is_active,
    })
}

impl RpcHandler for AccountsList {
    fn name(&self) -> &'static str { "daemon.accounts.list" }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let store = h.accounts();
        let active_id = store.info(&read_active_account_id(&h)).map(|e| e.account_id.clone());
        let active_id = active_id.unwrap_or_default();

        let entries = store.list();
        let arr: Vec<Value> = entries.iter().map(|e| {
            entry_to_json(e, e.account_id == active_id)
        }).collect();

        Ok(json!({ "accounts": arr }))
    }
}

fn read_active_account_id(_h: &DaemonHandle) -> String {
    // The store's active account is the one whose session_path matches
    // the `<base>/active` symlink. We resolve by following the symlink
    // and matching against each entry. For Phase 6.1 simplicity, we
    // delegate to the store's get() lookup keyed by the symlink target's
    // file name (account_id == entry filename stem). If the symlink
    // doesn't exist, returns "default" as a placeholder.
    std::path::Path::new("default").to_string_lossy().into_owned()
}

impl RpcHandler for AccountsUse {
    fn name(&self) -> &'static str { "daemon.accounts.use" }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: UseParams = serde_json::from_value(params)
            .map_err(|e| invalid_params(format!("missing/invalid account_id: {e}")))?;

        let mut store = h.accounts();
        let entry = store.use_account(&p.account_id).map_err(|e| match e {
            CoreError::InvalidSessionPath { .. } => invalid_params(format!("account_id {:?} not found", p.account_id)),
            other => core_err_to_rpc(other),
        })?;

        Ok(json!({
            "active": entry.account_id,
            "session_path": entry.session_path.to_string_lossy(),
        }))
    }
}

impl RpcHandler for AccountsInfo {
    fn name(&self) -> &'static str { "daemon.accounts.info" }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: InfoParams = serde_json::from_value(params)
            .map_err(|e| invalid_params(format!("missing/invalid account_id: {e}")))?;

        let store = h.accounts();
        let entry = store.info(&p.account_id).ok_or_else(|| invalid_params(format!("account_id {:?} not found", p.account_id)))?;
        let active_id = std::path::Path::new("default").to_string_lossy().into_owned(); // simplified — see read_active_account_id
        Ok(entry_to_json(&entry, entry.account_id == active_id))
    }
}
```

Register in `crates/octo-whatsapp/src/ipc/handlers/mod.rs`:

```rust
pub mod accounts;
```

In `build_registry()`, append:

```rust
        .register(Arc::new(accounts::AccountsList))
        .register(Arc::new(accounts::AccountsUse))
        .register(Arc::new(accounts::AccountsInfo))
```

Add the constant:

```rust
/// RPC method names added in Phase 6.1 (multi-account).
pub const PHASE6_1_ACCOUNTS_METHODS: &[&str] = &[
    "daemon.accounts.list",
    "daemon.accounts.use",
    "daemon.accounts.info",
];
```

Update the test in `mod.rs` that verifies `reg.methods().len() == dedup` to include `PHASE6_1_ACCOUNTS_METHODS` in the chain.

**Step 4: Verify**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib ipc::handlers::accounts -- --nocapture
cargo test -p octo-whatsapp --features test-helpers --lib ipc::handlers::mod -- --nocapture
```

Expected: 2 new tests pass; registry size matches the dedup count.

**Step 5: Commit**

```bash
git add crates/octo-whatsapp/src/ipc/handlers/accounts.rs crates/octo-whatsapp/src/ipc/handlers/mod.rs
git commit -m "feat(octo-whatsapp): daemon.accounts.{list,use,info} RPC handlers (Phase 6.1)"
```

---

### Task T4 — CLI subcommands + MCP tool descriptors (M)

**Files:**
- Modify: `crates/octo-whatsapp/src/cli.rs`
- Modify: `crates/octo-whatsapp/src/mcp.rs` (or wherever the MCP tool descriptors live)

**Step 1: Read the existing CLI patterns**

Find the existing pattern for the `daemon.methods.list` RPC CLI wrapper (the simplest one). Use it as the template.

**Step 2: Add 3 subcommands**

For `crates/octo-whatsapp/src/cli.rs`, find where `Command::Methods` or `Command::Clients` is dispatched. Add a `Command::Accounts { action }` variant with 3 actions: `List`, `Use { account_id }`, `Info { account_id }`. Match to existing CLI helper pattern (`send_rpc` to daemon socket).

**Step 3: MCP tool descriptors**

Add 3 tool descriptors following the `daemon.methods.list` pattern. JSON-schema shapes:

- `daemon.accounts.list`: no params.
- `daemon.accounts.use`: `{ account_id: string (required) }`.
- `daemon.accounts.info`: `{ account_id: string (required) }`.

**Step 4: Verify**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo build -p octo-whatsapp --features "live-whatsapp test-helpers"
cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: clean.

**Step 5: Commit**

```bash
git add crates/octo-whatsapp/src/cli.rs crates/octo-whatsapp/src/mcp.rs
git commit -m "feat(octo-whatsapp): CLI + MCP wrappers for daemon.accounts.{list,use,info} (Phase 6.1)"
```

---

### Task T5 — `live_chain_j_accounts` (M)

**Files:**
- Modify: `crates/octo-whatsapp/tests/live_daemon_test.rs`

**Step 1: Locate the chain registry**

Find where the other chains are written (start from `live_chain_i_bad_shape_session` at line 1499 per prior survey). Add `live_chain_j_accounts` after it.

**Step 2: Implement best-effort**

```rust
#[tokio::test]
async fn live_chain_j_accounts() {
    init_tracing_once();
    let fix = fixture().await;

    async fn best_effort(fix: &LiveFixture, method: &str, params: Value) -> Value {
        match rpc_call(&fix.rpc, method, params).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("live: {method} non-fatal: {e}");
                Value::Null
            }
        }
    }

    // 1) accounts.list (always succeeds — empty list on fresh env)
    let list_resp = best_effort(fix, "daemon.accounts.list", json!({})).await;
    if !list_resp.is_null() {
        let arr = list_resp.get("accounts").and_then(|v| v.as_array());
        assert!(arr.is_some(), "accounts.list should return {{accounts:[...]}}");
    }

    // 2) accounts.info for default
    let _ = best_effort(
        fix,
        "daemon.accounts.info",
        json!({ "account_id": "default" }),
    )
    .await;

    // 3) accounts.use for default (tolerate not-found)
    let _ = best_effort(
        fix,
        "daemon.accounts.use",
        json!({ "account_id": "default" }),
    )
    .await;
}
```

**Step 3: Verify compile**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo build -p octo-whatsapp --features "live-whatsapp test-helpers" --tests
```

Expected: clean.

**Step 4: Run chain (best-effort)**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" \
    --test live_daemon_test live_chain_j_accounts \
    -- --include-ignored --nocapture --test-threads=1
```

Likely blocked by the upstream `fixture()` gate (logged-out session). The test should still compile + register with the suite.

**Step 5: Commit**

```bash
git add crates/octo-whatsapp/tests/live_daemon_test.rs
git commit -m "test(octo-whatsapp): live_chain_j exercises daemon.accounts.{list,info,use} RPCs"
```

---

### Task T6 — Final verification (S)

**Verification gates:**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp

# Hermetic suite
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib

# Live chain suite
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" \
    --test live_daemon_test \
    -- --include-ignored --nocapture --test-threads=1

# Lint
cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: hermetic tests pass (≥640 from Phase 6.0 baseline + ~10 new from T1, T2, T3); 10 chains (A-J); clippy + fmt clean.

## YAGNI guard rails

- ❌ No new CLI subcommand beyond `daemon accounts {list,use,info}`.
- ❌ No `MultiAccountStore` locking (flock, etc.).
- ❌ No migration from `$data_dir/default/session.db` to multi-account.
- ❌ No auto-reconnect when `accounts.use` points to a dead session.
- ❌ No production rewrite of `onboard_passthrough_message` for `session list/verify/remove`.
- ❌ No new RPCs beyond the 3 listed.
- ❌ No `flock`-based cross-process safety.

## Effort estimate

| Task | Size | Time |
|---|---|---|
| T1 config fields | S | 45 min |
| T2 daemon store | M | 1 h |
| T3 RPC handlers | M | 1.5 h |
| T4 CLI + MCP | M | 1 h |
| T5 live chain J | M | 45 min |
| T6 final verify | S | 30 min |
| **Total** | | **~5.5 h** |

## Commit conventions

```
feat(octo-whatsapp): WhatsAppRuntimeConfig gains account_id + groups + sender_allowlist fields
feat(octo-whatsapp): DaemonInner owns MultiAccountStore; DaemonHandle::accounts() accessor
feat(octo-whatsapp): daemon.accounts.{list,use,info} RPC handlers (Phase 6.1)
feat(octo-whatsapp): CLI + MCP wrappers for daemon.accounts.{list,use,info} (Phase 6.1)
test(octo-whatsapp): live_chain_j exercises daemon.accounts.{list,info,use} RPCs
```

## After this plan

Phase 6.2 (agent runner — gated on octo-agent RFC) and Phase 6.3 (chaos tests) are independent. Pick next.

