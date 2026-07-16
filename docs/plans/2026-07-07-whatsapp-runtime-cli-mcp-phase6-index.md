# Phase 6 — Implementation Index

Phase 6 is split into four sequential sub-phases. Each has its own plan file, its own subagent-driven execution cycle, and its own commit log. They are ordered by dependency: lower-numbered phases unlock prerequisites for higher-numbered ones.

## Phase 6.0 — Production wiring + small gaps

**Plan file:** [`2026-07-07-whatsapp-runtime-cli-mcp-phase6.0.md`](./2026-07-07-whatsapp-runtime-cli-mcp-phase6.0.md)

**Scope (~3 h, 4 commits):**

1. Add `WhatsAppRuntimeConfig::adapter_config()` derivation (`$data_dir/{name}/session.db`).
2. Rename `DaemonHandle::set_adapter_for_tests` → `bind_adapter` (with `#[deprecated]` alias).
3. Wire `Command::Daemon` to construct the live `WhatsAppWebAdapter` + `start_bot()` + `bind_adapter` before `Daemon::run()`.
4. Add `chats.delete` coverage to `live_chain_c_messages_chats`.

**Unlocks:** Phase 6.1 (multi-account builds on `adapter_config()` + `bind_adapter`).

**Task IDs:** adds #202 (production binding) and closes #161's prerequisite plumbing.

## Phase 6.1 — Multi-account WhatsApp Web adapter plumbing

**Plan file:** [`2026-07-08-whatsapp-runtime-cli-mcp-phase6.1.md`](./2026-07-08-whatsapp-runtime-cli-mcp-phase6.1.md)

**Scope (~5.5 h, ~5 commits):**

1. Add `WhatsAppRuntimeConfig::account_id` (default `"default"`), `groups: Vec<String>`, `sender_allowlist: BTreeMap<…>` fields.
2. `DaemonInner` owns a `parking_lot::Mutex<Option<MultiAccountStore>>`; opens via `MultiAccountStore::open_default()` at `Daemon::new`. `DaemonHandle::accounts()` returns a guard.
3. Add `daemon.accounts.list`, `daemon.accounts.use`, `daemon.accounts.info` RPC methods.
4. CLI subcommands `accounts {list,use,info}` + MCP tool descriptors.
5. `live_chain_j_accounts` exercises the 3 new RPCs (best-effort).

**Unlocks:** nothing (terminal for the multi-account track).

**Task IDs:** closes #161.

**Scope (~8 h, ~6 commits):**

1. Extend `WhatsAppRuntimeConfig` with `groups: Vec<String>` + `sender_allowlist: BTreeMap<...>` fields.
2. Wire `MultiAccountStore` from `octo-whatsapp-onboard-core` into `Daemon::new` so `--name` resolves to the active account's session path (via the existing `use_account` symlink mechanism).
3. Add `daemon.accounts.list`, `daemon.accounts.use`, `daemon.accounts.info` RPC methods.
4. Add CLI subcommands + MCP tool descriptors for those 3 RPCs.
5. Add `live_chain_j_accounts` covering account list + use + info.
6. Hermetic tests for `MultiAccountStore`-driven path resolution + CLI/MCP wrappers.

**Unlocks:** nothing (terminal for the multi-account track).

**Task IDs:** closes #161.

## Phase 6.1.1 — `daemon.accounts.use` adapter rebind

**Plan file:** [`2026-07-08-whatsapp-runtime-cli-mcp-phase6.1-followup.md`](./2026-07-08-whatsapp-runtime-cli-mcp-phase6.1-followup.md)

**Scope (~1.5 h, 2 commits):**

1. Add `DaemonHandle::rebind_adapter_for(&str, &Path)` — constructs a fresh `WhatsAppWebAdapter` from the new session path + the runtime config's `groups`/`sender_allowlist`, then atomically swaps via `bind_adapter` (which aborts the prior connection-watcher).
2. Update `AccountsUse::call` to call `rebind_adapter_for` after the symlink write succeeds.

**Operator workflow:** `daemon.accounts.use <id>` followed by `reconnect.now` switches the active account without restarting the daemon.

**Unlocks:** nothing (terminal for the runtime account-switch track).

**Task IDs:** closes the production-rebind gap from #161.

## Phase 6.2 — Agent runner scaffolding (deferred to land after octo-agent RFC)

**Plan file:** TBD — plan will be drafted only after the octo-agent RFC is accepted in the RFC repo. Until then, this slot is intentionally empty.

**Scope (provisional):**

1. Add `octo-agent` crate as a workspace dependency (currently does not exist).
2. Replace `TriggerStore::run()` synthetic stub with a real `match RunnerSpec { Shell => ..., Http => ..., Agent => ... }` dispatch.
3. Wire the `Agent { agent_id, input_template }` fields into the dispatcher.
4. Add hermetic tests with a mock agent that echoes `input_template`.
5. Extend `live_chain_f_admin` with an agent-runner smoke call (best-effort, requires the agent server to be reachable in the test env).

**Blocker:** `octo-agent` crate does not exist in the workspace or as a published dep. Phase 6.2 cannot start until either:
- The octo-agent RFC is accepted and a draft implementation lands, OR
- A vendor copy is added under `vendor/octo-agent/` as a workspace member.

**Task IDs:** closes #162.

## Phase 6.3 — Chaos test suite (Part H)

**Plan file:** [`2026-07-07-whatsapp-runtime-cli-mcp-phase6.3.md`](./2026-07-07-whatsapp-runtime-cli-mcp-phase6.3.md) *(to be drafted, building on Phase 5 plan §Part H lines 816+)*

**Scope (~6 h, ~5 commits):**

1. Add `chaos` feature gate to `octo-whatsapp/Cargo.toml` (off by default).
2. Implement chaos tests per Phase 5 plan §Part H: WS disconnect mid-handshake, token rotation under load, rules_persister crash recovery, trigger runner timeout, event stream lag, audit hash chain reorg.
3. Gate tests on `OCTO_WHATSAPP_CHAOS=1` env (per Phase 5 A6).
4. Toxiproxy integration (process spawn + proxy management).
5. CI config: chaos runs nightly, not on every PR.

**Unlocks:** nothing (terminal for the chaos track).

**Task IDs:** closes #165.

## What is NOT in any Phase 6 sub-plan

The following Phase 6 candidates remain deferred to Phase 7+:

| ID | Item | Reason for deferral |
|---|---|---|
| #163 | TLS SPKI pin rotation | Needs a security RFC + review of pinning UX trade-offs. Not Phase 6 scope. |
| #164 | Wasm sandbox for Shell runner | Depends on Wasmtime integration design (separate RFC). |
| #166 | Distribution via apt repo + signed releases | Packaging was landed in Phase 5 Part G; apt repo is a separate ops track. |
| #167 | Landlock 0.5+ Ruleset API wiring | Landlock 0.5 builder API still stabilizing upstream; revisit in 6+ months. |
| #168 | seccompiler BpfProgram concrete rules | Phase 5 Part D wired a permissive stub; tightening needs a security review. |
| #169 | PID fd child watcher | Phase 5 Part E shipped the SIGCHLD fallback; pidfd_open optimization is a perf follow-up. |
| #170 | Per-rule soft-delete + audit replay | Storage migration needed; not a runtime-only change. |
| #171 | rules.test dry-run simulation | Depends on rule execution engine refactor (out of scope). |

## Execution order

```
6.0 (production wiring + chats.delete)
  └─ 6.1 (multi-account)
       ├─ 6.2 (agent runner — gated on octo-agent RFC)
       └─ 6.3 (chaos tests — independent, can run parallel with 6.2)
```

6.0 and 6.3 have no dependency on each other; they could in principle run in parallel. 6.1 depends on 6.0. 6.2 is gated on an external RFC.

## Resource budget

| Phase | Effort | Commits | New test surface | Risk |
|---|---|---|---|---|
| 6.0 | ~3 h | 4 | ~5 hermetic + 1 live chain addition | Low — pure glue code, no new semantics |
| 6.1 | ~8 h | 6 | ~10 hermetic + 1 live chain | Medium — touches adapter config + storage path |
| 6.2 | ~6 h | 5 | ~8 hermetic + 1 live chain | High — gated on external crate; deferred |
| 6.3 | ~6 h | 5 | ~12 hermetic + 1 chaos integration | Medium — needs toxiproxy setup |
| **Total** | **~23 h** | **20** | **~35 tests + 3 live chains** | |