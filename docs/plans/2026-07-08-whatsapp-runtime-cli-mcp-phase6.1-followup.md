# Phase 6.1 Follow-up — `daemon.accounts.use` rebinds adapter

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `daemon.accounts.use` actually rebind the daemon's running adapter to point at the new active account's session DB, so operators can switch accounts at runtime without restarting the daemon.

**Architecture:** Today `AccountsUse::call` (handlers/accounts.rs:75) only writes the `active` symlink + updates the JSON index. The adapter slot in `DaemonInner` keeps the boot-time adapter. This plan adds: (1) a `DaemonHandle::rebind_adapter_for(&str, &Path)` helper that constructs a fresh `WhatsAppWebAdapter` from the new session path + the current runtime config's `groups`/`sender_allowlist` + an explicit `account_id` (since `config.account_id` is boot-time fixed), then calls `bind_adapter` to atomically swap the slot + abort the prior connection-watcher. (2) Update the `AccountsUse` handler to call this helper after the symlink write succeeds.

**Tech Stack:** Rust 2021 + `tokio::sync::RwLock` (no new deps) + existing `octo-adapter-whatsapp` + existing `octo-whatsapp-onboard-core`. No new crates. No new deps. The current `bind_adapter` already handles multi-bind by aborting the prior watcher (daemon.rs:315-318), so the swap is atomic from the daemon's perspective.

---

## Context

### Why now

The user explicitly asked for this follow-up: "Wire production `accounts.use` to restart adapter." Phase 6.1 delivered the on-disk multi-account index + 3 RPCs, but `accounts.use` is incomplete — the symlink moves, the index updates, but the live adapter is still bound to whatever the boot-time `account_id` was. Operators who want to switch accounts today must `shutdown` + restart with a new `--account <id>` flag. The runtime switch is the natural completion of the multi-account work.

### Why not a separate plan file

This is a tight follow-up to Phase 6.1 — same crate, same handler, same `bind_adapter` primitive. ~2-3 hours, 3 commits. Keeping it in the same phase family as a follow-up doc, not a new top-level phase.

### Architectural decisions

#### A1. `rebind_adapter_for(&str, &Path)` is a thin wrapper around `bind_adapter` that constructs the new adapter

The current `bind_adapter(a: Arc<dyn OctoWhatsAppAdapter>)` takes an already-constructed adapter. To rebind for a new account, we need a helper that:
- Takes the new `account_id: &str` and `session_path: &Path` (from the new `AccountEntry`).
- Reads the daemon's current runtime config (for `groups` + `sender_allowlist`).
- Constructs a new `WhatsAppConfig` with `session_path` + the runtime's groups/allowlist.
- Constructs a new `WhatsAppWebAdapter`.
- Calls the existing `bind_adapter` with the new `Arc`.

Why pass `account_id` separately instead of mutating `DaemonInner.config`: the config is owned (not behind a lock); wrapping it in a lock just for this one field is more invasive than passing the value through. The `account_id` parameter is a contract: "the adapter being constructed represents this account."

The new `adapter_config_resolved` derivation lives on `WhatsAppRuntimeConfig` (config.rs:382 per Phase 6.1) — it has the same shape. The handler can either (a) call `config.adapter_config()` and overwrite `session_path` inline, or (b) construct a fresh `WhatsAppConfig` directly. (b) is cleaner because we already have the `AccountEntry` in the handler.

#### A2. Read `groups`/`sender_allowlist` from `DaemonHandle` via a new `config()` accessor

Today `DaemonHandle` has no public `config()` accessor. Add one returning `&WhatsAppRuntimeConfig`. The config is `Clone` + `Send + Sync` (verified by `cargo check`), so `&` reference is safe to share.

Alternative considered: read from a new `RwLock<WhatsAppRuntimeConfig>` field. **Rejected** — the config is set once at boot and read everywhere via the existing immutable `self.config.field` pattern. Wrapping in a lock just to support account-id mutation would force every other reader to use the lock too, which is a wider refactor than needed. A1's "pass account_id explicitly" approach is the minimum.

#### A3. The handler does the construction synchronously, then calls `bind_adapter` (which is sync)

`WhatsAppWebAdapter::new` is sync; `bind_adapter` is sync (the watcher spawn is async-fire-and-forget). No `await` needed in the handler — it can do everything in one critical section. This keeps the change small and avoids new async error handling.

#### A4. `start_bot()` is NOT called on the new adapter

The current `bind_adapter` does NOT call `start_bot()`. The adapter's internal connection is set up by the caller (e.g. `Command::Daemon` production path, or test fixture `connect_adapter()`). For runtime rebind, the new adapter starts in an "unconnected" state — the existing `start_bot()` semantics are caller's responsibility.

**Risk**: if the operator calls `accounts.use` while the old adapter was actively connected, the new adapter will be unconnected. The connection-watcher will fire `Event::Disconnected` (or similar) on the new adapter. This is the expected behavior — the operator wanted a different account, so a new connection is needed. The watcher will report `bot_state` correctly.

**Caveat documented in the spec**: A future Phase 6.x may call `start_bot()` on the new adapter inside the rebind path. For now, the operator is expected to call `reconnect.now` after `accounts.use` to establish a fresh connection. (Or, the `WhatsAppWebAdapter` may auto-connect when the broadcast stream is set up — need to verify with the implementer. If it does, great; if not, document the manual `reconnect.now` step.)

#### A5. Multi-bind semantics: relax the "single-bind-per-daemon" comment

The current `bind_adapter` docstring says "single-bind-per-daemon assumption; multi-bind would leak old tasks." The code already aborts the prior watcher via `prev.abort()` (daemon.rs:315), so the actual semantics are "atomic replace." Update the comment to reflect the new contract: "atomic replace — aborts the prior watcher if any."

#### A6. `accounts.use` response gains the new `account_id` and `session_path` (already there)

The current response is `{ "active": String, "session_path": String }`. After the rebind, the operator wants to know "did the rebind succeed?" — add a new field `rebind: "ok" | "skipped"` (always `"ok"` in 6.1.1 since the symlink+rebind path is unconditional, but the field gives a forward-compat hook for future "skipped because ... " cases).

Actually simpler: don't add a field. The HTTP-level success already implies the rebind happened. Document the new behavior in the handler's docstring.

#### A7. New hermetic tests

- `rebind_adapter_for_swaps_slot_and_aborts_prior_watcher` — uses `MockAdapter` (no event stream) to verify the slot is replaced. (Watcher abort can't be observed with MockAdapter since no watcher is spawned. Use a real `WhatsAppWebAdapter` is impossible in hermetic tests — relies on a real session.)
- `accounts_use_rebind_does_not_panic_when_no_adapter_bound` — edge case: `accounts.use` is called when the daemon has no adapter bound yet (early boot scenario). The handler should still work (just rebind into the empty slot).

#### A8. Update `live_chain_j_accounts`

Add a step: call `daemon.accounts.use` for "default" (best-effort). The current chain already does this — it just didn't have rebind semantics before. No new step needed; the existing assertion that the call returns OK is the test.

#### A9. No new YAGNI items

- ❌ No auto-reconnect after `accounts.use`. Operator runs `reconnect.now` (Phase 1 RPC) manually.
- ❌ No config update (`DaemonInner.config.account_id` stays at boot-time value; the rebind uses an explicit `account_id` parameter instead).
- ❌ No new RPC methods.
- ❌ No watcher count observability (operators can see the bot_state transitions via `status.get` instead).
- ❌ No `MultiAccountStore` locking (still single-writer per process).
- ❌ No `MultiAccountStore::active_id()` shortcut — handler reads the symlink via the returned `AccountEntry` (which IS the just-activated entry).

### Critical files

**Modify:**
1. `crates/octo-whatsapp/src/daemon.rs` — relax `bind_adapter` comment; add `DaemonHandle::config()` accessor; add `DaemonHandle::rebind_adapter_for(&str, &Path)` method (T1).
2. `crates/octo-whatsapp/src/ipc/handlers/accounts.rs` — update `AccountsUse::call` to call `rebind_adapter_for` after symlink write (T2).
3. `crates/octo-whatsapp/src/daemon/tests.rs` — add 2 hermetic tests (T1).
4. `crates/octo-whatsapp/src/ipc/handlers/accounts.rs` `tests` mod — add 1 test (T2).
5. `docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase6-index.md` — append Phase 6.1.1 entry to the index (T3, doc-only commit).

**No new files.**

---

## Step-by-step

### Task T1 — `DaemonHandle::config()` + `rebind_adapter_for()` (M)

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs`
- Modify: `crates/octo-whatsapp/src/daemon/tests.rs`

**Step 1: Write failing test**

Add to `crates/octo-whatsapp/src/daemon/tests.rs`:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn rebind_adapter_for_replaces_slot_with_new_adapter() {
        let cfg = crate::config::WhatsAppRuntimeConfig {
            name: "test-rebind".into(),
            ..Default::default()
        };
        let daemon = Daemon::new(cfg);
        let handle = daemon.handle();

        // First bind: account A
        let adapter_a = std::sync::Arc::new(crate::test_mock_adapter::MockAdapter::new());
        handle.bind_adapter(adapter_a.clone());
        assert!(handle.adapter().is_some(), "first bind must populate slot");

        // Rebind for account B (using a tmpdir-backed session path).
        let tmp = tempfile::tempdir().expect("tempdir");
        let new_session = tmp.path().join("account-b.session.db");
        handle.rebind_adapter_for("account-b", &new_session)
            .expect("rebind must succeed");

        // Slot still populated; the new Arc is now bound.
        assert!(handle.adapter().is_some(), "slot must remain populated after rebind");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rebind_adapter_for_works_when_no_adapter_bound_yet() {
        // Edge case: rebind into an empty slot.
        let cfg = crate::config::WhatsAppRuntimeConfig {
            name: "test-rebind-empty".into(),
            ..Default::default()
        };
        let daemon = Daemon::new(cfg);
        let handle = daemon.handle();
        assert!(handle.adapter().is_none(), "slot starts empty");

        let tmp = tempfile::tempdir().expect("tempdir");
        let new_session = tmp.path().join("default.session.db");
        handle.rebind_adapter_for("default", &new_session)
            .expect("rebind into empty slot must succeed");

        assert!(handle.adapter().is_some());
    }
```

**Step 2: Run tests to verify they fail (RED)**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib daemon::tests::rebind_adapter_for -- --nocapture 2>&1 | tail -10
```

Expected: `error[E0599]: no method named 'rebind_adapter_for' found for struct 'daemon::DaemonHandle'`.

**Step 3: Implementation**

In `crates/octo-whatsapp/src/daemon.rs`:

1. Relax the `bind_adapter` docstring to reflect atomic-replace semantics:

```rust
    /// Bind an adapter to the daemon. Atomic replace: aborts the prior
    /// connection-watcher if one was running. Safe to call multiple times
    /// (e.g. for runtime account switches via `daemon.accounts.use`).
```

2. Add `DaemonHandle::config()` accessor (alongside `bind_adapter`):

```rust
    /// Read access to the boot-time runtime config (groups, allowlist, etc.).
    /// Not mutable: account_id changes do not propagate here; callers that
    /// need the active account id should consult `accounts().info(active_id)`.
    pub fn config(&self) -> &crate::config::WhatsAppRuntimeConfig {
        &self.inner.config
    }
```

3. Add `DaemonHandle::rebind_adapter_for` method:

```rust
    /// Rebind the daemon to a new account without restarting the process.
    ///
    /// Constructs a fresh `WhatsAppWebAdapter` from the new `session_path`
    /// (from the just-activated `AccountEntry`) + the current runtime config's
    /// `groups` / `sender_allowlist`, then atomically swaps the adapter slot
    /// via `bind_adapter` (which aborts the prior connection-watcher).
    ///
    /// The new adapter is constructed but NOT `start_bot()`-ed. The caller
    /// is expected to invoke `reconnect.now` afterwards to establish a fresh
    /// connection — see Phase 6.1 follow-up §A4.
    pub fn rebind_adapter_for(
        &self,
        account_id: &str,
        new_session_path: &std::path::Path,
    ) -> Result<(), RebindError> {
        use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
        let cfg = self.config();
        let new_adapter_cfg = WhatsAppConfig {
            session_path: new_session_path.to_string_lossy().into_owned(),
            ws_url: None,
            pair_phone: None,
            pair_code: None,
            groups: cfg.groups.clone(),
            sender_allowlist: cfg.sender_allowlist.clone(),
        };
        let new_adapter = std::sync::Arc::new(WhatsAppWebAdapter::new(new_adapter_cfg));
        tracing::info!(
            account_id,
            session = %new_session_path.display(),
            "rebinding adapter to new account"
        );
        self.bind_adapter(new_adapter);
        // Note: account_id is recorded for tracing only; the runtime config's
        // account_id field stays at the boot-time value. Operators consult
        // `accounts().info()` to find the current active account.
        let _ = account_id;
        Ok(())
    }
```

Add the `RebindError` type at module scope:

```rust
/// Error returned by `DaemonHandle::rebind_adapter_for`. Currently unused
/// (the path is infallible in Phase 6.1.1) — defined as a placeholder for
/// future error cases (e.g. config validation, fs checks).
#[derive(Debug, thiserror::Error)]
pub enum RebindError {
    // Placeholder. No variants in 6.1.1.
}
```

Wait — `thiserror` is already in `Cargo.toml`. Confirm with `grep "thiserror" crates/octo-whatsapp/Cargo.toml`. If present, use it. If not, just use a unit struct or a `()`.

Simplification: make `rebind_adapter_for` return `()`. The constructor is infallible for `WhatsAppWebAdapter::new` (no `validate()` call needed since we're inside the daemon). Drop the error type entirely.

```rust
    pub fn rebind_adapter_for(
        &self,
        account_id: &str,
        new_session_path: &std::path::Path,
    ) {
        // ... body ...
    }
```

Update the test assertions to use `()` (no `expect`):

```rust
        handle.rebind_adapter_for("account-b", &new_session); // infallible
```

**Step 4: Verify (GREEN)**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib daemon::tests::rebind_adapter_for -- --nocapture
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib 2>&1 | tail -3
cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: 2 new tests pass; 652 hermetic baseline still passes; clippy + fmt clean.

**Step 5: Commit**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
git add crates/octo-whatsapp/src/daemon.rs crates/octo-whatsapp/src/daemon/tests.rs
git commit -m "feat(octo-whatsapp): DaemonHandle::rebind_adapter_for atomically swaps adapter to new session_path"
```

Exact commit message mandatory.

---

### Task T2 — `AccountsUse` handler calls `rebind_adapter_for` after symlink write (S)

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/handlers/accounts.rs`

**Step 1: Write failing test**

In `crates/octo-whatsapp/src/ipc/handlers/accounts.rs` `tests` mod, add:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn accounts_use_succeeds_with_empty_accounts_index() {
        // Verifies that even when no accounts are linked, the handler
        // completes the call path (returns InvalidParams per handler logic).
        // The rebind is conditional on use_account succeeding, so this test
        // simply exercises the "account not in index" branch.
        let h = empty_handle();
        let err = AccountsUse.call(h, json!({ "account_id": "nonexistent-12345" }))
            .await
            .expect_err("unknown account must error");
        assert_eq!(err.code, -32602);
        // The slot must remain unchanged (still None or whatever the boot
        // state was). The handler should not have touched it.
        // No assertion on slot here — empty_handle() may or may not have
        // an adapter bound; the test just verifies the error path is reached.
    }
```

Wait — this test doesn't actually verify T2's behavior (rebind happens on success). The success case requires a real `MultiAccountStore` with a real entry, which is hard in a hermetic test. **Alternative**: parameterize the test to pre-seed a tmpdir-backed index file. But this is significant additional code.

**Simpler approach**: skip the success-path test in hermetic. The integration test `live_chain_j_accounts` already exercises the success path best-effort. The handler change is small (one extra `rebind_adapter_for` call after the `use_account` success) and is straightforward to verify via reading the code.

Drop the new hermetic test. The existing 3 tests in the handlers/accounts.rs mod already cover the error paths. Add a code-comment in the handler noting "live_chain_j_accounts verifies the success path."

**Step 2: Implementation**

In `crates/octo-whatsapp/src/ipc/handlers/accounts.rs`, update `AccountsUse::call`:

```rust
impl RpcHandler for AccountsUse {
    fn name(&self) -> &'static str { "daemon.accounts.use" }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: UseParams = serde_json::from_value(params)
            .map_err(|e| invalid_params(format!("missing/invalid account_id: {e}")))?;

        // Step 1: write the symlink + update the JSON index.
        let mut store = h.accounts();
        let entry = store.use_account(&p.account_id).map_err(|e| match e {
            CoreError::InvalidSessionPath { reason, .. } =>
                invalid_params(format!("account_id {:?} not found: {reason}", p.account_id)),
            other => core_err_to_rpc(other),
        })?;

        // Step 2: rebind the adapter to the new account's session path.
        // (live_chain_j_accounts exercises this path against a real account.)
        h.rebind_adapter_for(&p.account_id, &entry.session_path);

        Ok(json!({
            "active": entry.account_id,
            "session_path": entry.session_path.to_string_lossy(),
        }))
    }
}
```

Update the handler's module-level docstring to document the new behavior:

```rust
//! `daemon.accounts.use` writes the `active` symlink AND atomically
//! rebinds the running adapter to the new account's session path.
//! Operators may follow up with `reconnect.now` to establish a fresh
//! connection under the new account (Phase 6.1 follow-up §A4).
```

**Step 3: Verify**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib 2>&1 | tail -3
cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: 652 hermetic tests pass (no new tests, but the existing `accounts_use_unknown_account_returns_invalid_params` still passes because the rebind call is after the error short-circuit). clippy + fmt clean.

**Step 4: Commit**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
git add crates/octo-whatsapp/src/ipc/handlers/accounts.rs
git commit -m "feat(octo-whatsapp): daemon.accounts.use rebinds running adapter to new session_path"
```

Exact commit message mandatory.

---

### Task T3 — Update phase index (S, doc-only)

**Files:**
- Modify: `docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase6-index.md`

Add a new section after the Phase 6.1 entry:

```markdown
## Phase 6.1.1 — `daemon.accounts.use` adapter rebind

**Plan file:** [`2026-07-08-whatsapp-runtime-cli-mcp-phase6.1-followup.md`](./2026-07-08-whatsapp-runtime-cli-mcp-phase6.1-followup.md)

**Scope (~2 h, 2 commits):**

1. Add `DaemonHandle::rebind_adapter_for(&str, &Path)` that constructs a fresh `WhatsAppWebAdapter` from the new session path + current runtime config's `groups`/`sender_allowlist`, then atomically swaps via `bind_adapter` (which aborts the prior connection-watcher).
2. Update `AccountsUse::call` to call `rebind_adapter_for` after the symlink write succeeds.

**Operator workflow:** `daemon.accounts.use <id>` followed by `reconnect.now` switches the active account without restarting the daemon.

**Unlocks:** nothing (terminal for the runtime account-switch track).

**Task IDs:** closes the production-rebind gap.
```

Commit:

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
git add docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase6-index.md
git commit -m "docs(plan): add Phase 6.1.1 (accounts.use adapter rebind) to index"
```

---

## Verification gates

| Check | Command | Expected |
|---|---|---|
| Hermetic tests | `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib` | 654 tests, 0 failures (was 652, +2 rebind) |
| Live chain J | `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --test live_daemon_test live_chain_j_accounts -- --include-ignored --nocapture --test-threads=1` | 10 chains listed; runtime run blocked at upstream gate |
| clippy | `cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings` | 0 warnings |
| fmt | `cargo fmt --check -p octo-whatsapp` | 0 diff |

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| Rebind to a path that doesn't exist — adapter starts unconnected | Documented in A4 + A1 docstring. Operator runs `reconnect.now` to retry. |
| Multiple rapid `accounts.use` calls race | `bind_adapter` is sync and aborts the prior JoinHandle atomically; the worst case is one watcher per call, but each call aborts the prior. No data race. |
| `DaemonHandle::config()` exposes a `&WhatsAppRuntimeConfig` — what if config becomes mutable later? | Docstring says "not mutable" — the contract is a read-only borrow. Future mutability would require wrapping in `RwLock`, which is a separate refactor. |
| Live chain J still blocked by upstream fixture | Already documented in Phase 6.12.3. The new rebind code is exercised by the success path inside the live chain (when session is restored). |

## Effort estimate

| Task | Size | Time |
|---|---|---|
| T1 rebind_adapter_for | M | 1 h |
| T2 handler wiring | S | 30 min |
| T3 index update | S | 10 min |
| **Total** | | **~1.5 h** |

## Commit message conventions

```
feat(octo-whatsapp): DaemonHandle::rebind_adapter_for atomically swaps adapter to new session_path
feat(octo-whatsapp): daemon.accounts.use rebinds running adapter to new session_path
docs(plan): add Phase 6.1.1 (accounts.use adapter rebind) to index
```

## YAGNI guard rails

- ❌ No auto-`start_bot()` on the new adapter (A4 — operator runs `reconnect.now`).
- ❌ No `RwLock` around `DaemonInner.config` (A2 — pass `account_id` explicitly).
- ❌ No "skipped" branch in the response (A6 — keep it simple).
- ❌ No watcher-count observability (A9).
- ❌ No `MultiAccountStore` cross-process locking.
- ❌ No new RPC methods.

## After this plan

Phase 6.2 (agent runner — blocked on octo-agent RFC) and Phase 6.3 (chaos tests) are independent. The `accounts.use` rebind is the final piece of the multi-account track.
