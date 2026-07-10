# Phase 6.0 Implementation Plan — Production wiring + small gaps

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the runtime actually start the WhatsApp Web adapter on `daemon` subcommand, fix the `chats.delete` live-chain gap, and lay the groundwork for Phase 6.1 (multi-account).

**Architecture:** Today the `octo-whatsapp daemon` command brings up the IPC server but never binds a `WhatsAppWebAdapter` — the adapter slot stays `None`, the connection-watcher task never runs, and every send-style RPC fails with `NotConnected` (–32012). Phase 6.0 closes the gap by (a) deriving the adapter config from `WhatsAppRuntimeConfig` via a new `adapter_config()` method, (b) calling `start_bot()` from inside `Command::Daemon` before `Daemon::run()`, (c) handing the constructed adapter to the daemon via the existing `DaemonHandle::set_adapter_for_tests` (renamed to `bind_adapter` for clarity since it's no longer test-only).

**Tech Stack:** Rust 2021 + tokio + anyhow + `octo-adapter-whatsapp` + `arc-swap`. No new crates. No new dependencies. Same hermetic + live test patterns as Phase 6.12.

---

## Context

### Why now

- **Connection-watcher gap**: Phase 6.12.4 added a watcher that translates WA `Event::*` → `BotStateMirror` transitions (`crates/octo-whatsapp/src/daemon.rs`). The watcher is spawned from inside `set_adapter_for_tests`. Since production `daemon` never calls that function, the watcher never runs in production — `status.get` still reports stale `Connected` after `Event::LoggedOut`.
- **`chats.delete` coverage gap**: The handler exists and is registered (handler registry line ~136) but no live chain exercises it. Single-method gap, cheap fix.
- **Phase 6.1 prerequisite**: Multi-account plumbing needs `adapter_config()` to derive a per-account session path. Defining that derivation in 6.0 (even with single-account semantics) avoids a 6.1 refactor that touches the same code paths.

### Architectural decisions

#### A1. `set_adapter_for_tests` → `bind_adapter` (rename + de-misleading)

The method was originally gated on `cfg(test)`/`test-helpers`. Phase 6.12.4 de-gated it. Now Phase 6.0 makes it a real production entrypoint. Rename it to `bind_adapter` so production callers don't have to explain why they're calling a `_for_tests` API.

**Migration**: rename the method, keep an inline `#[deprecated(note = "use bind_adapter")]` alias for one release cycle, then drop it. Inside `octo-whatsapp` itself there are only 2 callers (test fixtures) — both get migrated in T3. External consumers (CLI, MCP) are still using the `&self` form via `Daemon::handle()`, so they need the method on `DaemonHandle` (they don't care about the rename beyond the call site).

#### A2. Adapter construction = sync (not async)

`WhatsAppWebAdapter::new(config)` is sync and returns an unconnected adapter. `start_bot()` is async and returns `Result<()>`. Production `daemon` startup:

1. `let adapter = Arc::new(WhatsAppWebAdapter::new(cfg));` (sync, no I/O)
2. `adapter.start_bot().await?;` (async, may take seconds — initializes stoolap, opens WS)
3. `handle.bind_adapter(adapter);` (sync, binds + spawns watcher)

If `start_bot()` fails (bad session, network down), the daemon exits with an error before the IPC server binds. This matches the existing `start` semantics (the operator must fix the session before starting).

**Alternative considered**: spawn `start_bot()` as a background task and bind the adapter in "starting" state. **Rejected**: the existing test fixtures and `live_chain_i_bad_shape_session` cover the "started but not connected" path; production needs the simpler "fail fast" semantic.

#### A3. `adapter_config()` derives `session_path` from `data_dir + name`

```rust
impl WhatsAppRuntimeConfig {
    pub fn adapter_config(&self) -> WhatsAppConfig {
        let mut session_path = self.data_dir.clone();
        session_path.push(&self.name);
        session_path.push("session.db");
        WhatsAppConfig {
            session_path: session_path.to_string_lossy().into_owned(),
            ws_url: None,
            pair_phone: None,
            pair_code: None,
            groups: vec![],         // populated from runtime groups config (out of scope for 6.0)
            sender_allowlist: Default::default(),
        }
    }
}
```

Rationale: parallels the existing `socket_path()` pattern (`$socket_dir/octo-whatsapp-{name}.sock`). For Phase 6.0, `groups` and `sender_allowlist` stay empty (defaults match current behavior). Phase 6.1 will extend `WhatsAppRuntimeConfig` with a `groups: Vec<String>` and `sender_allowlist: BTreeMap<...>` to wire them through.

#### A4. `chats.delete` live chain addition

Best-effort pattern (matches chain C's existing style). Single `best_effort` call with `inter_call_delay_for("chats.delete")` before. The handler returns `Ok({"status":"deleted"})` on success; on `NotConnected` (bot dead mid-life) the helper swallows the error with a warning. This gives us coverage of (a) the RPC round-trip, (b) the JSON param shape, (c) the response format, without requiring a real chat to delete.

**Alternative considered**: probe `chats.list` first, pick a real chat, archive-then-delete. **Rejected**: adds 30s of round-trip time and depends on the test phone having at least one deletable chat. Best-effort with a warning is enough for smoke coverage.

#### A5. No new YAGNI items

- ❌ No multi-account (Phase 6.1).
- ❌ No agent runner changes (Phase 6.2, blocked on octo-agent RFC).
- ❌ No chaos tests (Phase 6.3).
- ❌ No production caller wiring for `groups` / `sender_allowlist` in `WhatsAppRuntimeConfig` (Phase 6.1 extends the config struct).
- ❌ No auto-reconnect (still deferred).
- ❌ No GraphQL gateway.

### Critical files

**Modify:**
1. `crates/octo-whatsapp/src/config.rs` — add `adapter_config()` method (T1).
2. `crates/octo-whatsapp/src/daemon.rs` — rename `set_adapter_for_tests` → `bind_adapter`, add `#[deprecated]` alias, update test fixtures (T3).
3. `crates/octo-whatsapp/src/cli.rs` — wire `Command::Daemon` to construct adapter + bind (T4).
4. `crates/octo-whatsapp/tests/live_daemon_test.rs` — update 2 call sites to use `bind_adapter`; extend `live_chain_c_messages_chats` with `chats.delete` (T3, T5).

**No new files.**

---

## Step-by-step

### Task T1 — Add `adapter_config()` derivation (S)

**Files:**
- Modify: `crates/octo-whatsapp/src/config.rs:361` (the `impl WhatsAppRuntimeConfig` block, after `socket_path()`)

**Step 1: Write the failing test**

Add a `#[cfg(test)] mod tests` block at the bottom of `config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_config_derives_session_path_from_data_dir_and_name() {
        let cfg = WhatsAppRuntimeConfig {
            name: "work".into(),
            data_dir: PathBuf::from("/var/lib/octo/whatsapp"),
            ..Default::default()
        };
        let ac = cfg.adapter_config();
        assert_eq!(ac.session_path, "/var/lib/octo/whatsapp/work/session.db");
    }

    #[test]
    fn adapter_config_default_name_uses_default_subdir() {
        let cfg = WhatsAppRuntimeConfig::default();
        let ac = cfg.adapter_config();
        assert!(ac.session_path.ends_with("/default/session.db"),
                "got {:?}", ac.session_path);
    }

    #[test]
    fn adapter_config_empty_groups_and_allowlist() {
        let cfg = WhatsAppRuntimeConfig::default();
        let ac = cfg.adapter_config();
        assert!(ac.groups.is_empty());
        assert!(ac.sender_allowlist.is_empty());
        assert!(ac.ws_url.is_none());
        assert!(ac.pair_phone.is_none());
        assert!(ac.pair_code.is_none());
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p octo-whatsapp --lib config::tests::adapter_config -- --nocapture
```

Expected: `error[E0599]: no function or associated item named 'adapter_config' found for struct 'WhatsAppRuntimeConfig'`.

**Step 3: Write the minimal implementation**

Add to `crates/octo-whatsapp/src/config.rs` after the existing `socket_path()` method (around line 379):

```rust
/// Derive the adapter-layer `WhatsAppConfig` from the runtime config.
///
/// `session_path` is computed as `$data_dir/{name}/session.db`, paralleling
/// the socket-path derivation (`$socket_dir/octo-whatsapp-{name}.sock`).
///
/// `groups` and `sender_allowlist` are intentionally empty in Phase 6.0;
/// they will be wired through when `WhatsAppRuntimeConfig` gains those
/// fields in Phase 6.1 (multi-account plumbing).
pub fn adapter_config(&self) -> octo_adapter_whatsapp::WhatsAppConfig {
    use octo_adapter_whatsapp::WhatsAppConfig;
    let mut session_path = self.data_dir.clone();
    session_path.push(&self.name);
    session_path.push("session.db");
    WhatsAppConfig {
        session_path: session_path.to_string_lossy().into_owned(),
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: Vec::new(),
        sender_allowlist: Default::default(),
    }
}
```

Add the import at the top of the file (near the existing `use` block):

```rust
use octo_adapter_whatsapp;
```

Wait — `octo-whatsapp`'s `Cargo.toml` already lists `octo-adapter-whatsapp` as a path dep, but the type alias for `WhatsAppConfig` may need a direct import. Check the existing config.rs imports; if `WhatsAppConfig` isn't already imported, add `use octo_adapter_whatsapp::WhatsAppConfig;` near the top.

**Step 4: Run test to verify it passes**

```bash
cargo test -p octo-whatsapp --lib config::tests::adapter_config -- --nocapture
```

Expected: 3 tests passed.

**Step 5: Commit**

```bash
git add crates/octo-whatsapp/src/config.rs
git commit -m "feat(octo-whatsapp): WhatsAppRuntimeConfig::adapter_config derives session path from data_dir + name"
```

---

### Task T2 — Add hermetic test for `bind_adapter` rename + watcher spawn (S)

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs:286` (rename `set_adapter_for_tests` → `bind_adapter`, add `#[deprecated]` alias)

**Step 1: Write the failing test**

Add a `#[cfg(test)] mod tests` block in `crates/octo-whatsapp/src/daemon.rs` (after the existing tests in that file, or at the bottom of the file):

```rust
#[cfg(test)]
mod bind_adapter_tests {
    use super::*;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn empty_handle() -> DaemonHandle {
        // Use the same construction the test fixtures use
        let cfg = crate::config::WhatsAppRuntimeConfig {
            name: "test-bind".into(),
            ..Default::default()
        };
        let daemon = Daemon::new(cfg);
        daemon.handle()
    }

    #[test]
    fn bind_adapter_stores_adapter_and_returns() {
        let h = empty_handle();
        let adapter = Arc::new(MockAdapter::new_unconnected());
        h.bind_adapter(adapter.clone());
        assert!(h.adapter().is_some(), "adapter slot must be populated after bind_adapter");
    }

    #[test]
    fn bind_adapter_runs_idempotently() {
        let h = empty_handle();
        let adapter = Arc::new(MockAdapter::new_unconnected());
        h.bind_adapter(adapter.clone());
        // MockAdapter returns None from subscribe_raw_events, so the
        // connection-watcher is NOT spawned. Second call should still succeed
        // (single-bind-per-daemon contract).
        h.bind_adapter(adapter.clone());
        assert!(h.adapter().is_some());
    }
}
```

(Note: `MockAdapter::new_unconnected()` may have a different constructor name — check the existing test fixtures in `crates/octo-whatsapp/src/test_mock_adapter.rs`. The fixture pattern at `live_daemon_test.rs:160` is the authoritative source; adapt the constructor name accordingly.)

**Step 2: Run test to verify it fails**

```bash
cargo test -p octo-whatsapp --features test-helpers --lib daemon::bind_adapter_tests -- --nocapture
```

Expected: `error[E0599]: no method named 'bind_adapter' found for struct 'DaemonHandle'`.

**Step 3: Rename `set_adapter_for_tests` → `bind_adapter` + add alias**

In `crates/octo-whatsapp/src/daemon.rs` around line 286:

```rust
/// Bind a live `OctoWhatsAppAdapter` to the daemon. Replaces the
/// placeholder adapter slot and spawns the connection-watcher task that
/// translates WA lifecycle events into `BotStateMirror` transitions.
///
/// **Contract**: single-bind-per-daemon. Calling this a second time
/// aborts the prior connection-watcher and spawns a new one. Production
/// startup should call this exactly once, immediately after `Daemon::new()`,
/// before the IPC server starts accepting connections.
pub fn bind_adapter(&self, a: Arc<dyn OctoWhatsAppAdapter>) {
    let _ = a; // suppress unused warning during incremental migration
    self.bind_adapter_impl(a);
}

#[deprecated(note = "use bind_adapter instead; this alias will be removed in a future phase")]
pub fn set_adapter_for_tests(&self, a: Arc<dyn OctoWhatsAppAdapter>) {
    self.bind_adapter_impl(a);
}

fn bind_adapter_impl(&self, a: Arc<dyn OctoWhatsAppAdapter>) {
    // existing body of set_adapter_for_tests, unchanged
    *self.inner.adapter.write().unwrap_or_else(|p| p.into_inner()) = Some(a.clone());
    if let Some(rx) = a.subscribe_raw_events() {
        let cancel = self.inner.cancel.clone();
        let handle_for_watcher = self.clone();
        if let Some(prev) = self.inner.connection_watcher.lock().replace(
            tokio::spawn(async move { run_connection_watcher(rx, handle_for_watcher, cancel).await }),
        ) {
            prev.abort();
        }
    }
}
```

**Step 4: Update internal callers (test fixtures)**

The two test fixtures at `crates/octo-whatsapp/tests/live_daemon_test.rs:281` and `:432` currently call `set_adapter_for_tests`. Migrate both to `bind_adapter`:

```bash
grep -n "set_adapter_for_tests" crates/octo-whatsapp/tests/live_daemon_test.rs
```

For each line, replace:

```rust
daemon.handle().set_adapter_for_tests(adapter.clone())
```

with:

```rust
daemon.handle().bind_adapter(adapter.clone())
```

**Step 5: Run test to verify it passes**

```bash
cargo test -p octo-whatsapp --features test-helpers --lib daemon::bind_adapter_tests -- --nocapture
```

Expected: 2 tests passed.

Also re-run the existing live_chain_* tests to verify the rename didn't break the fixtures:

```bash
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --test live_daemon_test -- --list
```

Expected: 9 chains listed (no compile errors).

**Step 6: Commit**

```bash
git add crates/octo-whatsapp/src/daemon.rs crates/octo-whatsapp/tests/live_daemon_test.rs
git commit -m "refactor(octo-whatsapp): rename set_adapter_for_tests to bind_adapter (no semantic change)"
```

---

### Task T3 — Wire production `daemon` subcommand to construct + bind adapter (M)

**Files:**
- Modify: `crates/octo-whatsapp/src/cli.rs:1449` (the `Command::Daemon` match arm)

**Step 1: Read the existing arm**

Read `crates/octo-whatsapp/src/cli.rs` lines 1440-1470 to see the current `Command::Daemon` body.

**Step 2: Modify the arm to construct + bind the adapter**

The current body is approximately:

```rust
Command::Daemon => {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(
        crate::daemon::Daemon::new(...).run(),
    )
}
```

Change it to:

```rust
Command::Daemon => {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let daemon = crate::daemon::Daemon::new(config.clone());
        let adapter_cfg = config.adapter_config();
        let adapter = std::sync::Arc::new(
            octo_adapter_whatsapp::WhatsAppWebAdapter::new(adapter_cfg)
        );
        if let Err(e) = adapter.start_bot().await {
            tracing::error!(
                account = %config.name,
                session = %adapter_cfg.session_path,
                "start_bot failed; aborting daemon startup: {e}"
            );
            return Err(anyhow::anyhow!("start_bot failed: {e}"));
        }
        daemon.handle().bind_adapter(adapter);
        daemon.run().await
    })
}
```

Add the `octo_adapter_whatsapp` import if not already present at the top of `cli.rs`:

```rust
use octo_adapter_whatsapp;
```

**Step 3: cargo check**

```bash
cargo check -p octo-whatsapp --features "live-whatsapp test-helpers"
```

Expected: compiles clean.

**Step 4: Verify**

```bash
cargo clippy -p octo-whatsapp --features "live-whatsapp test-helpers" -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: both clean.

**Step 5: Commit**

```bash
git add crates/octo-whatsapp/src/cli.rs
git commit -m "feat(octo-whatsapp): production daemon command constructs + binds WhatsAppWebAdapter on startup"
```

---

### Task T4 — Extend `live_chain_c_messages_chats` with `chats.delete` (S)

**Files:**
- Modify: `crates/octo-whatsapp/tests/live_daemon_test.rs` (inside `live_chain_c_messages_chats`, after the `chats.typing` calls)

**Step 1: Read the existing chain end**

Read the body of `live_chain_c_messages_chats` (lines 838-941 per Phase 6.12 survey). Locate the last `best_effort(... chats.typing ... paused)` call (around line 935).

**Step 2: Add the new call**

Insert after the `chats.typing — paused` call and before the function's closing brace:

```rust
    // 20) inter-call throttle
    inter_call_delay_for("chats.delete").await;

    // 21) chats.delete (best-effort; some accounts may reject deletes)
    let _ = best_effort(
        fix,
        "chats.delete",
        json!({ "jid": group_a.clone() }),
    )
    .await;
}
```

(Note: the closing `}` is the function's closing brace; keep that. The new block is inserted just before it.)

**Step 3: cargo check + run chain C**

```bash
cargo build -p octo-whatsapp --features "live-whatsapp test-helpers" --tests
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" \
    --test live_daemon_test live_chain_c_messages_chats \
    -- --include-ignored --nocapture --test-threads=1
```

Expected: chain C completes; either the delete succeeds or it warns and continues.

**Step 4: Commit**

```bash
git add crates/octo-whatsapp/tests/live_daemon_test.rs
git commit -m "test(octo-whatsapp): live_chain_c exercises chats.delete RPC"
```

---

### Task T5 — Final verification (S)

**Files:** none (just run commands)

**Step 1: Run hermetic test suite**

```bash
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib
```

Expected: all hermetic tests pass (≥635 from Phase 6.12.4 baseline + ~5 new from T1, T2).

**Step 2: Run live chain suite**

```bash
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" \
    --test live_daemon_test \
    -- --include-ignored --nocapture --test-threads=1
```

Expected: all 9 live chains pass (A through I, with chain C now including chats.delete).

**Step 3: clippy + fmt clean**

```bash
cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: both clean.

**Step 4: Workspace check**

```bash
cargo check --workspace --all-features
```

Expected: clean.

**Step 5: Commit (no source changes — only verification)**

If anything was tweaked during verification, commit those individually. Otherwise this step is a no-op commit-wise.

---

## Verification gates

| Check | Command | Expected |
|---|---|---|
| Hermetic tests | `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib` | ≥635 tests, 0 failures |
| Live chains | `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --test live_daemon_test -- --include-ignored --nocapture --test-threads=1` | 9 chains pass |
| clippy | `cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings` | 0 warnings |
| fmt | `cargo fmt --check -p octo-whatsapp` | 0 diff |
| Workspace | `cargo check --workspace --all-features` | clean |

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| `start_bot()` in production blocks startup for 30+ seconds when WA server is slow | Acceptable. Operators are expected to wait for the boot handshake. Document in man page (out of scope here). |
| `start_bot()` fails in CI when WA server is unreachable | Production won't ever run in CI; the live tests already handle this via `connect_adapter_unchecked`. |
| Rename of `set_adapter_for_tests` breaks external callers (CLI, MCP) | The CLI command inside `Command::Daemon` migrates in T3; the MCP server (which runs inside the daemon process) uses the same `DaemonHandle` so no extra migration. |
| `WhatsAppWebAdapter::new` panics on invalid config | Add `cfg.validate()?;` before `new()` — see T3 step 2. |
| Live chain C `chats.delete` deletes a real chat | Best-effort helper swallows errors. The chat JID comes from chain B's created group (synthetic, deletable). |

## Effort estimate

| Task | Size | Time |
|---|---|---|
| T1 adapter_config | S | 30 min |
| T2 bind_adapter rename | S | 30 min |
| T3 production wiring | M | 1 h |
| T4 chain C gap | S | 15 min |
| T5 final verification | S | 30 min |
| **Total** | | **~3 h** |

## Commit message conventions

```
feat(octo-whatsapp): WhatsAppRuntimeConfig::adapter_config derives session path from data_dir + name
refactor(octo-whatsapp): rename set_adapter_for_tests to bind_adapter (no semantic change)
feat(octo-whatsapp): production daemon command constructs + binds WhatsAppWebAdapter on startup
test(octo-whatsapp): live_chain_c exercises chats.delete RPC
```

## YAGNI guard rails

- ❌ No auto-reconnect logic.
- ❌ No multi-account plumbing (Phase 6.1).
- ❌ No `groups`/`sender_allowlist` config fields (Phase 6.1).
- ❌ No agent runner changes (Phase 6.2).
- ❌ No chaos tests (Phase 6.3).
- ❌ No GraphQL gateway.
- ❌ No production-side caller of `bind_adapter` from inside the daemon's hot path — startup only.
- ❌ No change to `MockAdapter` beyond the test fixture.

## After this plan

Phase 6.1 (multi-account plumbing), Phase 6.2 (agent runner scaffolding), and Phase 6.3 (chaos test suite) are separate plans, each with its own task breakdown.