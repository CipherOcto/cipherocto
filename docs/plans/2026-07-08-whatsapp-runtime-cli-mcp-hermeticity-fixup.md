# Hermeticity Fixup Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make every hermetic test that calls `Daemon::new` operate against a tmpdir-backed filesystem instead of touching the developer's real `~/.local/share/octo/whatsapp/`. Fix the leak flagged in the Phase 6.1.3 code review.

**Architecture:** Add `Daemon::new_for_tests(tmpdir: &Path) -> (Daemon, DaemonHandle)` constructor that returns a daemon whose `data_dir`, `rules.resolved_storage_path`, `wal_path`, `socket_dir`, and `MultiAccountStore` all point inside `tmpdir`. Migrate every `Daemon::new(cfg).handle()` test site to use it. The production `Daemon::new` is unchanged (still uses real paths from the config).

**Tech Stack:** Rust 2021 + `tempfile::TempDir` (already a dev-dep) + `std::env` for the optional `XDG_DATA_HOME` redirect. No new crates. No new deps.

---

## Context

### Why now

The T6.1.3 code review (and a broader static analysis) flagged: every call to `Daemon::new(cfg).handle()` reads/writes the developer's real `~/.local/share/octo/whatsapp/` because:

1. `DaemonInner::accounts` opens via `MultiAccountStore::open_default()` (daemon.rs:656), which reads `$XDG_DATA_HOME` / `$HOME`.
2. `DaemonInner::rules` spawns `RulesPersister` at the config's resolved paths (daemon.rs:664), which defaults to `~/.local/share/octo/whatsapp/rules.toml`.
3. Other paths (`socket_dir`, `data_dir`, `tokens/grace.json`, observability logs, media buffer) all resolve against the config and can leak similarly.

The leak has been present since the test suite was first written. It's silent — tests pass — but it pollutes the developer's filesystem on every `cargo test`. Fixing it now (after the multi-account plumbing lands in 6.1) is important because the multi-account `accounts.use` path will start writing symlinks in real locations on test runs.

### Why a new constructor instead of an env var hack

Two options:
- **Env var hack**: have tests set `XDG_DATA_HOME` (and `RULES_STORAGE_PATH`, etc.) before each test. This is fragile (race conditions in parallel tests, doesn't cover the socket dir which is read via a different code path).
- **Constructor injection**: `Daemon::new_for_tests(tmpdir)` returns a daemon with all paths redirected. Cleaner, race-free, covers every leak in one shot.

Going with constructor injection.

### Architectural decisions

#### A1. `Daemon::new_for_tests(tmpdir: &Path) -> (Daemon, DaemonHandle)` constructor

```rust
/// Hermetic test constructor. Builds a Daemon whose filesystem paths
/// (data_dir, socket_dir, MultiAccountStore index, rules.toml + wal)
/// all live inside `tmpdir`. Returns the Daemon + its handle.
///
/// The returned Daemon is fully usable but should be dropped at end of
/// test (the TempDir should also be dropped to clean up). Does NOT
/// spawn a real WhatsApp adapter — callers bind one if needed.
#[cfg(any(test, feature = "test-helpers"))]
pub fn new_for_tests(tmpdir: &std::path::Path) -> (Self, DaemonHandle) {
    use crate::config::*;
    use octo_whatsapp_onboard_core::MultiAccountStore;

    let data_dir = tmpdir.join("data");
    std::fs::create_dir_all(&data_dir).expect("data_dir");
    let socket_dir = tmpdir.join("sock");
    std::fs::create_dir_all(&socket_dir).expect("socket_dir");

    let rules_path = data_dir.join("rules.toml");
    let wal_path = data_dir.join("rules.wal");
    let cfg = WhatsAppRuntimeConfig {
        name: "test".into(),
        data_dir: data_dir.clone(),
        log_dir: tmpdir.join("logs"),
        socket_dir: socket_dir.clone(),
        media_buffer: MediaBufferConfig { root: tmpdir.join("media"), ..Default::default() },
        events: EventsConfig::default(),
        security: SecurityConfig { grace_path: Some(data_dir.join("grace.json")), ..Default::default() },
        observability: ObservabilityConfig::default(),
        rules: RulesConfig {
            storage_path: rules_path,
            wal_path: Some(wal_path),
            ..Default::default()
        },
        account_id: "default".into(),
        groups: Vec::new(),
        sender_allowlist: std::collections::BTreeMap::new(),
    };

    // Open the store directly at tmpdir — NOT via open_default().
    let accounts = MultiAccountStore::open(data_dir.join("index.json"))
        .expect("MultiAccountStore::open");

    let daemon = Self::new_internal(cfg, Some(accounts));
    let handle = daemon.handle();
    (daemon, handle)
}
```

`Self::new_internal` is the existing `Daemon::new` body, lifted into a private helper that takes an optional pre-opened store. The existing public `Daemon::new(config)` becomes a thin wrapper that calls `Self::new_internal(config, None)` and logs a warning if the store open fails.

This is the minimum invasive change: the production `Daemon::new` path is unchanged for callers (still infallible wrt the store), but a test-only constructor with full path control exists.

#### A2. The 16 test sites get migrated to `Daemon::new_for_tests`

Every `Daemon::new(cfg).handle()` in test code becomes `Daemon::new_for_tests(&tmp).1`. The 16 sites per the survey:
- `src/ipc/server/tests.rs` (2)
- `src/ipc/handlers/domain_compute_hash.rs:117`
- `src/ipc/handlers/envelope_send.rs:108`
- `src/ipc/handlers/send_delete.rs:84`
- `src/ipc/handlers/messages_search.rs:69, 74, 101` (3)
- `src/ipc/handlers/actions_escalate.rs:65`
- `src/ipc/handlers/events.rs:154`
- `src/ipc/handlers/chats_unpin.rs:60`
- `src/ipc/handlers/chats_delete.rs:57`
- `src/ipc/handlers/groups.rs:938, 945, 1148, 1240` (4)
- `src/ipc/handlers/health.rs:81, 97` (2)
- `src/ipc/handlers/accounts.rs:134`
- `src/daemon/tests.rs:48, 70, 82` (3)

That's ~20 sites total. Mechanical migration.

**Migration pattern** — find a pattern like:
```rust
fn empty_handle() -> DaemonHandle {
    let cfg = WhatsAppRuntimeConfig { name: "x".into(), ..Default::default() };
    Daemon::new(cfg).handle()
}
```

Replace with:
```rust
fn empty_handle() -> DaemonHandle {
    let tmp = tempfile::tempdir().expect("tempdir");
    Daemon::new_for_tests(tmp.path()).1
}
```

(Some test files already have a `tempdir()` for other reasons — share the existing one where possible.)

For tests that don't yet use `tempfile`, add a `let tmp = tempfile::tempdir().expect("tempdir");` at the top.

**Sub-agent task**: split the 20 migrations across 4-5 subagents (grouped by file) to avoid one giant commit.

#### A3. Keep the existing `Daemon::new` public for non-test usage

`Daemon::new(config)` stays public. It calls `MultiAccountStore::open_default()` (which may log a warning on failure but doesn't fail the constructor). Tests move to `Daemon::new_for_tests`. Production stays on `Daemon::new`.

The `new_for_tests` constructor is `#[cfg(any(test, feature = "test-helpers"))]` gated (same as the existing `set_adapter_for_tests` deprecation alias was). Actually — since `set_adapter_for_tests` was de-gated in Phase 6.0, `new_for_tests` should probably also be de-gated. But it's hermetic by intent (uses tmpdir) — no risk of accidental production misuse. De-gate it; production never calls it because the tmpdir path is nonsensical for production.

Wait — the `new_for_tests` constructor takes a `&Path` for tmpdir. Production has no tmpdir to pass. So it's automatically not-callable from production. De-gate freely.

#### A4. `MultiAccountStore::open(path)` is the right injection point

The store's `open(path)` method (multi_account.rs:126) takes an explicit path and returns `Result<Self>`. This is the documented public API for non-default locations. Test constructor uses it directly.

#### A5. No env-var manipulation

No `std::env::set_var("XDG_DATA_HOME", ...)` in tests — that's the "env var hack" approach which has race-condition issues in parallel test execution. Constructor injection is cleaner.

### Critical files

**Modify:**
1. `crates/octo-whatsapp/src/daemon.rs` — extract `new_internal` private helper; add `new_for_tests` constructor (T1).
2. `crates/octo-whatsapp/src/ipc/server/tests.rs` — migrate 2 sites (T2a).
3. `crates/octo-whatsapp/src/ipc/handlers/{domain_compute_hash,envelope_send,send_delete,messages_search,actions_escalate,events,chats_unpin,chats_delete,groups,health,accounts}.rs` — migrate 14 sites (T2b).
4. `crates/octo-whatsapp/src/daemon/tests.rs` — migrate 3 sites (T2c).

**No new files.**

---

## Step-by-step

### Task T1 — Add `Daemon::new_for_tests` + extract `new_internal` (S)

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs`

**Step 1: Write failing test**

Add to `crates/octo-whatsapp/src/daemon/tests.rs`:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn new_for_tests_creates_daemon_with_no_home_dir_touch() {
        // Before: Daemon::new(cfg).handle() would call MultiAccountStore::open_default()
        //         which reads $HOME/.local/share/octo/whatsapp/ — leaking.
        // After:  Daemon::new_for_tests(tmpdir) opens the store at tmpdir/index.json.
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
        // The store must be open and queryable. Empty index.
        assert_eq!(handle.accounts().list().len(), 0);
        // The index file must exist at tmpdir, NOT under the user's home dir.
        let expected_index = tmp.path().join("data/index.json");
        assert!(expected_index.exists(), "store must live at tmpdir/data/index.json");
    }
```

**Step 2: Run to verify it fails (RED)**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib daemon::tests::new_for_tests -- --nocapture 2>&1 | tail -10
```

Expected: `error[E0599]: no associated function 'new_for_tests' found for struct 'daemon::Daemon'`.

**Step 3: Implementation**

In `crates/octo-whatsapp/src/daemon.rs`:

1. Rename the existing `Daemon::new` body to `Daemon::new_internal` (private, takes an additional `Option<MultiAccountStore>` parameter).
2. The existing public `Daemon::new(config)` becomes a thin wrapper:
```rust
pub fn new(config: WhatsAppRuntimeConfig) -> Self {
    // Existing path: try to open the default multi-account store.
    // Failures are non-fatal (store stays None).
    let accounts = match MultiAccountStore::open_default() {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!(
                "MultiAccountStore::open_default failed; daemon starts without accounts API: {e}"
            );
            None
        }
    };
    Self::new_internal(config, accounts)
}
```
3. Add the new test-only constructor:
```rust
/// Hermetic test constructor. Builds a Daemon whose filesystem paths
/// (data_dir, socket_dir, MultiAccountStore index, rules.toml + wal,
/// media buffer, observability logs) all live inside `tmpdir`.
/// Returns `(Daemon, DaemonHandle)`.
///
/// The returned Daemon is fully usable; no adapter is bound. Tests
/// that need an adapter call `handle.bind_adapter(...)` after
/// construction.
pub fn new_for_tests(tmpdir: &std::path::Path) -> (Self, DaemonHandle) {
    use crate::config::*;
    let data_dir = tmpdir.join("data");
    let _ = std::fs::create_dir_all(&data_dir);
    let socket_dir = tmpdir.join("sock");
    let _ = std::fs::create_dir_all(&socket_dir);

    let rules_path = data_dir.join("rules.toml");
    let wal_path = data_dir.join("rules.wal");
    let cfg = WhatsAppRuntimeConfig {
        name: "test".into(),
        data_dir: data_dir.clone(),
        log_dir: tmpdir.join("logs"),
        socket_dir,
        media_buffer: MediaBufferConfig {
            root: tmpdir.join("media"),
            ..Default::default()
        },
        events: EventsConfig::default(),
        security: SecurityConfig {
            grace_path: Some(data_dir.join("grace.json")),
            ..Default::default()
        },
        observability: ObservabilityConfig::default(),
        rules: RulesConfig {
            storage_path: rules_path,
            wal_path: Some(wal_path),
            ..Default::default()
        },
        account_id: "default".into(),
        groups: Vec::new(),
        sender_allowlist: std::collections::BTreeMap::new(),
    };

    // Open the store directly at tmpdir/data/index.json — NOT via open_default().
    let accounts = MultiAccountStore::open(data_dir.join("index.json"))
        .expect("MultiAccountStore::open(tmpdir)");

    let daemon = Self::new_internal(cfg, Some(accounts));
    let handle = daemon.handle();
    (daemon, handle)
}
```

**Step 4: Verify (GREEN)**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib daemon::tests::new_for_tests -- --nocapture
```

Expected: 1 test passes.

**Step 5: Commit**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
git add crates/octo-whatsapp/src/daemon.rs crates/octo-whatsapp/src/daemon/tests.rs
git commit -m "feat(octo-whatsapp): Daemon::new_for_tests redirects all filesystem paths to tmpdir (hermetic test constructor)"
```

Exact commit message mandatory.

---

### Task T2a — Migrate `ipc/server/tests.rs` (S)

**Files:**
- Modify: `crates/octo-whatsapp/src/ipc/server/tests.rs`

**Workflow:**

1. Find both `Daemon::new(cfg).handle()` sites (lines 21, 37 per survey).
2. For each:
   - Add `let tmp = tempfile::tempdir().expect("tempdir");` at the top of the test function.
   - Replace `Daemon::new(cfg).handle()` with `Daemon::new_for_tests(tmp.path()).1`.
   - The function may already have a `cfg` variable; remove it.
3. Run tests to verify they still pass.

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib ipc::server -- --nocapture 2>&1 | tail -3
```

**Commit:**

```bash
git add crates/octo-whatsapp/src/ipc/server/tests.rs
git commit -m "test(octo-whatsapp): migrate ipc/server tests to Daemon::new_for_tests (hermetic)"
```

---

### Task T2b — Migrate `ipc/handlers/*` test sites (M)

**Files:**
- Modify: 11 handler test files (`domain_compute_hash.rs`, `envelope_send.rs`, `send_delete.rs`, `messages_search.rs`, `actions_escalate.rs`, `events.rs`, `chats_unpin.rs`, `chats_delete.rs`, `groups.rs`, `health.rs`, `accounts.rs`)

**Workflow:** For each file:

1. Find every `Daemon::new(cfg).handle()` (or `Daemon::new(cfg)`) site.
2. Add `tempfile::tempdir()` at the top of the test.
3. Replace with `Daemon::new_for_tests(tmp.path()).1`.
4. Remove the `cfg` local variable if it's no longer needed.
5. Run tests to verify they still pass.

If a file already has a `tempdir()` for other reasons (e.g. for a socket), reuse that tmpdir for `new_for_tests`.

Run tests after each file:

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib <module_path> -- --nocapture 2>&1 | tail -3
```

**Subagent approach:** group the 11 files across 2-3 subagents (5+5+1) to keep each commit small.

**Commit:** one commit per file (or per group of 2-3 files if they're tightly coupled). Each commit's message:

```
test(octo-whatsapp): migrate <module_name> tests to Daemon::new_for_tests (hermetic)
```

---

### Task T2c — Migrate `daemon/tests.rs` (S)

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon/tests.rs`

**Workflow:**

The 3 sites at lines 48, 70, 82 already use `Daemon::new(cfg).handle()`. Migrate to `Daemon::new_for_tests(tmp.path()).1` exactly like T2a.

The `bind_adapter_stores_adapter` and `bind_adapter_is_idempotent_when_no_events_stream` tests already exist; the 2 `rebind_adapter_for_*` tests from T6.1.1.1 use `tempfile::tempdir()` already (for the session_path argument) — share that tmpdir with `new_for_tests`.

Run tests:

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features test-helpers --lib daemon::tests -- --nocapture 2>&1 | tail -3
```

**Commit:**

```bash
git add crates/octo-whatsapp/src/daemon/tests.rs
git commit -m "test(octo-whatsapp): migrate daemon tests to Daemon::new_for_tests (hermetic)"
```

---

### Task T3 — Final verification (S)

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp

# Hermetic suite (full regression sweep)
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib

# Confirm no test still touches $HOME
strace -f -e trace=openat -o /tmp/strace.log cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib 2>&1 | tail -3
grep "octo/whatsapp" /tmp/strace.log | grep -v "data/octo" | head -20
```

Expected: hermetic suite still passes (654+ tests); strace shows NO opens under the user's actual `~/.local/share/octo/whatsapp/` (only under tmpdirs created during the test run).

```bash
cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings
cargo fmt --check -p octo-whatsapp
```

Expected: clippy + fmt clean.

## Verification gates

| Check | Command | Expected |
|---|---|---|
| Hermetic tests | `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --lib` | 654+ tests pass (no regression) |
| No home-dir touch | strace shows no opens under `~/.local/share/octo/whatsapp/` | clean |
| clippy | `cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings` | 0 warnings |
| fmt | `cargo fmt --check -p octo-whatsapp` | 0 diff |

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| `Daemon::new_internal` rename accidentally changes behavior | The body is unchanged; only the signature gains `Option<MultiAccountStore>`. All existing tests on `Daemon::new` go through the wrapper which keeps the same observable behavior. |
| Some test depends on a real `$XDG_DATA_HOME` value | Unlikely — tests should be environment-agnostic. If a test breaks, debug to find the dependency + fix the test (don't disable the hermeticity check). |
| `tempfile::TempDir` drop ordering — does the test see data written by the daemon before teardown? | `TempDir` lives until end of `fn`. Daemon drops first (last expression in test). No race. |
| Migration breaks a test that secretly relies on real filesystem state | The first sign of trouble will be test failure. Document the failure, fix the test (e.g. set the required state on the tmpdir), don't bypass the constructor. |

## Effort estimate

| Task | Size | Time |
|---|---|---|
| T1 new_for_tests | S | 30 min |
| T2a server/tests.rs | S | 15 min |
| T2b 11 handler files | M | 1.5 h (parallelizable across subagents) |
| T2c daemon/tests.rs | S | 15 min |
| T3 final verify | S | 30 min |
| **Total** | | **~3 h** |

## Commit message conventions

```
feat(octo-whatsapp): Daemon::new_for_tests redirects all filesystem paths to tmpdir (hermetic test constructor)
test(octo-whatsapp): migrate ipc/server tests to Daemon::new_for_tests (hermetic)
test(octo-whatsapp): migrate <module> tests to Daemon::new_for_tests (hermetic)
... (one per handler file)
test(octo-whatsapp): migrate daemon tests to Daemon::new_for_tests (hermetic)
```

## YAGNI guard rails

- ❌ No env-var manipulation (`XDG_DATA_HOME` overrides) — constructor injection only.
- ❌ No migration of integration tests in `crates/octo-whatsapp/tests/` (those use `LiveFixture` / `BadLiveFixture` and don't create `Daemon::new` directly).
- ❌ No change to `MultiAccountStore::open_default()` — production still uses it.
- ❌ No "fix all the other leaks" (tokens grace, observability logs) — `new_for_tests` already routes them to tmpdir via the config struct.
- ❌ No shared `Daemon::new_for_tests_with_mock_adapter()` helper — each test binds its own.

## After this plan

The leak is closed. Phase 6.2 (agent runner) and Phase 6.3 (chaos tests) remain independent.