# WhatsApp Runtime CLI + MCP — Phase 3 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Implement Phase 3 (Events) of the WhatsApp runtime CLI + MCP design — typed `InboundEvent` parser, event router with stoolap persistence + retention, `events.tail` RPC + MCP notifications (`resources/updated`, `tools/list_changed` debounced 1s), agent discovery (`clients/list`, `daemon.methods.list|help`), `events.list/show/replay`, and the `MCP subscribers` / `CLI subscribers` fan-out.

**Architecture:**
1. **Typed `InboundEvent` parser** — `events.rs` parses `String → InboundEvent` via `format!("{:?}", ev)` from the adapter's `raw_event_tx`. 8 variants from design §InboundEvent (Message, Reaction, GroupChange, Presence, Connection, Receipt, Call, Story) plus `Unknown` fallback.
2. **Event router** — central component owned by `DaemonHandle`. Subscribes to adapter's `raw_event_tx`. Persists each event to stoolap `events` table BEFORE fan-out via a `db_writer` task (single ownership, no back-pressure on rules/sub).
3. **Bounded mpsc fan-out** — per-sink (MCP clients, CLI clients, rules engine) bounded mpsc; on `RecvError::Lagged(n)` sink exposes the count via `status.get`.
4. **`events.tail` RPC + MCP** — subscriber pushes typed JSON; MCP sends `notifications/resources/updated` (1s debounce) + `notifications/tools/list_changed` on `tools.enable` toggle.
5. **`events.list/show/replay` RPC** — read from stoolap `events` table with `since-ts` (wall) and `since-id` (monotonic) filters; bounded by `[events] retention_days` (default 30) + `max_rows` (default 1M).
6. **Agent discovery** — `clients/list` returns active MCP client sessions + `daemon.methods.list|help` for introspection.

**Tech Stack:** Rust 2021 + tokio broadcast + tokio mpsc + smallvec (mentions bounding) + chrono (RFC 3339 timestamps) + stoolap (already in adapter). Existing test infrastructure (MockAdapter pattern, integration tests).

**Pre-requisites:**
- Branch: `feat/whatsapp-runtime-cli-mcp` (stack on top — Phase 1 + Phase 2 + Phase 2.5 + MockAdapter coverage push ALL COMPLETE)
- Worktree: `.worktrees/whatsapp-runtime-cli-mcp`
- 270/270 lib tests passing in octo-whatsapp
- Coverage: 85.53% lines / 86.74% branches (both gates cleared)
- `daemon.api.version = "1.0.0+phase2"` (will bump to `1.0.0+phase3` on Task 1)

**Acceptance gates:**
- All existing tests still pass (no regressions)
- `cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only` lines ≥ 85.00%, branches ≥ 75.00%
- `cargo clippy --all-targets --all-features -- -D warnings` clean
- `cargo fmt --check` clean
- `daemon.api.version = "1.0.0+phase3"`
- No push, no PR (per user decision 2026-07-05)

---

## Architectural decisions

### A1. Why a separate `events_router.rs` module instead of inlining in `daemon.rs`

The event router has three distinct concerns: parsing (`events.rs`), persistence (`events_persister.rs`), and fan-out (`events_fanout.rs`). Mixing them in `daemon.rs` would make `daemon.rs` grow beyond ~400 LoC and obscure the supervisor logic. Each piece has its own tests + state machine; modular separation matches the design doc's `events.rs owns the String → InboundEvent parser` invariant.

### A2. Why single-writer `db_writer` task for stoolap events

The design says "Persist goes through the `db_writer` task to avoid back-pressure on the rules engine and subscribers." This means the event router MUST NOT block on stoolap inserts. We achieve this by:
- Event router sends `InboundEvent` via bounded mpsc to `db_writer`
- `db_writer` is the sole owner of the stoolap `events` table connection
- On back-pressure, router drops the event with `events.dropped_total` counter (the design already accepts lossy semantics for the broadcast channel)

### A3. Why `SmallVec<[Jid; 8]>` for `mentions`

Design says "longer mention lists truncate with a `mentions_truncated=true` flag." `smallvec` is already in the dependency tree (used by `octo-network`). It avoids heap allocation for the common case (≤8 mentions) and bounds memory.

### A4. Why `events.list/show/replay` instead of `events.list/show` only

`events.replay` is needed for `RecvError::Lagged(n)` recovery — design §Loss recovery: "Subscribers experiencing RecvError::Lagged(n) use events.list --since-id <last_seen> to backfill." `events.replay` is the same as `events.list` but returns the raw event payload (no redaction) for recovery workflows. Distinct method makes the security boundary explicit.

### A5. Why `daemon.methods.list|help` instead of single `daemon.methods`

The design doc lists both `daemon.methods.list` and `daemon.methods.help` (separate RPC methods). Two methods because:
- `list` returns just the method names (small payload, used by agent discovery)
- `help` returns the full schema for one method (per-method introspection)
This matches the CLI pattern of `clap --help` vs just listing verbs.

### A6. Why fan-out is per-sink mpsc NOT broadcast

The existing `raw_event_tx` is broadcast (lossy, capacity 1000). Phase 3 needs per-sink backpressure (a slow MCP client shouldn't slow rules). Each sink subscribes to its own `mpsc::Receiver<InboundEvent>` from the router. Router uses `try_send` and tracks per-sink Lagged counters. This is the correct shape per design §Fan-out.

---

## Part A — Typed InboundEvent parser (Tasks 1-7)

### Task 1: Bump daemon.api.version

Edit `crates/octo-whatsapp/src/ipc/handlers/version.rs`:
- Change `"version": env!("CARGO_PKG_VERSION")` and `"api_version": "1.0.0+phase2"` → `"1.0.0+phase3"`.

Test: `cargo test -p octo-whatsapp version::tests`. Existing tests should still pass (they assert the version string contains `phase`).

Commit: `feat(octo-whatsapp): bump daemon.api.version to 1.0.0+phase3`.

### Task 2: Define full `InboundEvent` enum

Edit `crates/octo-whatsapp/src/events.rs`:
- Replace the 1-variant `InboundEvent` enum with the 8-variant enum from design §InboundEvent.
- Add `use smallvec::SmallVec;` and `use crate::jids::Jid;`.
- Each variant has its fields from the design doc.
- `Kind` enums: `MessageKind`, `GroupChangeKind`, `PresenceKind`, `ConnectionKind`, `ReceiptKind`, `CallKind`, `CallState`, `StoryKind`.

Test: `cargo check -p octo-whatsapp`.

### Task 3: Add `Display`/`Debug` derives + serde tag

The variants need `serde::Serialize` + `serde::Deserialize` for JSON output. Use `#[serde(tag = "kind", rename_all = "snake_case")]` on the enum.

Test: `cargo check -p octo-whatsapp`.

### Task 4: Implement parser skeleton

Edit `crates/octo-whatsapp/src/events.rs`:
- `parse(env: EventEnvelope) -> InboundEvent` (existing function) → dispatch to `parse_inner(&env.raw, env.ts_unix_ms, env.ts_mono_ns) -> InboundEvent`.
- `parse_inner` uses string matching on the `format!("{:?}", ev)` output from the adapter. wacore's `Event` Debug format is documented in `wacore::types::events::Event` — match the variant names.

Sub-task — write parser dispatch table for 8 variants. Each variant's parse function:
- Returns `Some(InboundEvent::Message { ... })` if input matches
- Returns `None` if input doesn't match
- Falls through to `Unknown` if none match

Test: `cargo check -p octo-whatsapp`.

### Task 5: Parser unit tests

Edit `crates/octo-whatsapp/src/events/tests.rs`:
- Test each of 8 variants parses correctly from a sample Debug string
- Test `Unknown` fallback for unmapped input
- Test `mentions_truncated=true` flag fires when >8 mentions
- Test `text` truncation at 64 KiB

Test: `cargo test -p octo-whatsapp events::tests`. Expect: 8 new tests pass.

Commit: `feat(events): full InboundEvent parser (Phase 3 Part A)`.

### Task 6: Add `events.list/show/replay` RPC handler skeleton

Create `crates/octo-whatsapp/src/ipc/handlers/events.rs` (REPLACES existing file):
- `EventsList` reads from `DaemonState::events_buffer` (not yet implemented — return empty for now).
- `EventsShow` reads by `id` — returns structured error for unknown id.
- `EventsReplay` reads with `since_id` parameter — returns raw event payloads.

Existing `events.list` and `events.show` tests should be updated to handle the new shape (they asserted on `phase: "phase1_no_tail"` — that marker is gone).

Test: `cargo test -p octo-whatsapp events`. Existing tests should pass with updated assertions.

### Task 7: Add `events.tail` RPC handler

Edit `crates/octo-whatsapp/src/ipc/handlers/events.rs`:
- `EventsTail` accepts `{ "follow": bool, "limit": usize }`.
- Returns `{ "events": [...], "lagged": usize }`.
- For Phase 3 Part A (no router yet): returns empty events array + `lagged: 0`. Full implementation in Part B.

Test: `cargo test -p octo-whatsapp events::tests::events_tail_returns_empty_in_phase3`.

Commit: `feat(events): typed parser + events.list/show/replay/tail handlers (Part A)`.

---

## Part B — Event router + persistence (Tasks 8-15)

### Task 8: Define `EventsBuffer` in-memory ring

Create `crates/octo-whatsapp/src/events_persister.rs`:
```rust
//! In-memory events ring buffer + stoolap persistence.
//!
//! Bounded by `[events] max_rows` (default 1_000_000). Events older than
//! `retention_days` (default 30) are evicted on insert in batches of 1000.

pub struct EventsBuffer {
    inner: parking_lot::Mutex<Vec<InboundEvent>>,
    max_rows: usize,
}

impl EventsBuffer {
    pub fn new(max_rows: usize) -> Self { ... }
    pub fn push(&self, ev: InboundEvent) { ... }  // evicts oldest if full
    pub fn list(&self, since_ts: Option<i64>, since_id: Option<u64>, limit: usize) -> Vec<InboundEvent> { ... }
    pub fn get(&self, id: u64) -> Option<InboundEvent> { ... }
    pub fn len(&self) -> usize { ... }
}
```

Test: `cargo test -p octo-whatsapp events_persister::tests`. Push + list + get + eviction tests.

### Task 9: Add `events_buffer` field to `DaemonInner`

Edit `crates/octo-whatsapp/src/daemon.rs`:
- Add `events_buffer: Arc<EventsBuffer>` field.
- Initialize in `Daemon::handle()`.
- Add `pub fn events_buffer(&self) -> &Arc<EventsBuffer>` getter.

Test: `cargo check -p octo-whatsapp`.

### Task 10: Add `events` config section

Edit `crates/octo-whatsapp/src/config.rs`:
- Add `EventsConfig { retention_days: u32, max_rows: usize }` struct.
- Add `events: EventsConfig` field on root config.
- Default: `retention_days = 30`, `max_rows = 1_000_000`.
- Load from `[events]` TOML section.

Test: `cargo test -p octo-whatsapp config::tests`. Add test for default + custom values.

Commit: `feat(events): in-memory ring buffer + config (Part B Tasks 8-10)`.

### Task 11: Wire events_buffer into handlers

Edit `crates/octo-whatsapp/src/ipc/handlers/events.rs`:
- `EventsList.call(h, params)` → `h.events_buffer().list(...)` (still empty for now).
- `EventsShow.call(h, params)` → `h.events_buffer().get(...)`.
- `EventsReplay.call(h, params)` → `h.events_buffer().list(...)` with `since_id`.

Test: `cargo test -p octo-whatsapp events::tests`. Tests now hit the buffer (still empty).

### Task 12: Event router component

Create `crates/octo-whatsapp/src/events_router.rs`:
```rust
//! Central event router. Subscribes to adapter's raw_event_tx, parses
//! to InboundEvent, persists, fans out to subscribers.

pub struct EventsRouter {
    raw_rx: tokio::sync::broadcast::Receiver<String>,
    db_writer_tx: tokio::sync::mpsc::Sender<InboundEvent>,
    sinks: parking_lot::Mutex<Vec<Arc<EventsSink>>>,
}

impl EventsRouter {
    pub fn spawn(raw_rx: Receiver<String>, buffer: Arc<EventsBuffer>, cancel: CancellationToken) -> Self { ... }
    pub fn subscribe(&self) -> EventsSubscriber { ... }
    async fn run(self, cancel: CancellationToken) { ... }  // main loop
}
```

The main loop:
1. `match raw_rx.recv().await` → on `Ok(s)` parse, on `Err(Lagged(n))` increment lagged counter, on `Err(Closed)` exit.

Test: `cargo check -p octo-whatsapp`.

### Task 13: `db_writer` task (stoolap persistence)

Edit `crates/octo-whatsapp/src/events_router.rs`:
- Add `db_writer(buffer: Arc<EventsBuffer>, mut rx: mpsc::Receiver<InboundEvent>, cancel: CancellationToken)`.
- Drains rx, calls `buffer.push(ev)`.
- Single-task ownership — no contention on the buffer's mutex.

Test: `cargo test -p octo-whatsapp events_router::tests`. Test the writer persists + evicts.

### Task 14: Per-sink mpsc fan-out

Edit `crates/octo-whatsapp/src/events_router.rs`:
- `EventsSink { tx: mpsc::Sender<InboundEvent>, lagged: AtomicU64 }`.
- Router sends a COPY of each event to each sink's `tx` (via `try_send`).
- On `try_send` failure (full or closed), increment `sink.lagged`.

Test: `cargo test -p octo-whatsapp events_router::tests`. Test fan-out to 2 sinks + lagged counter.

### Task 15: Wire router into Daemon

Edit `crates/octo-whatsapp/src/daemon.rs`:
- `Daemon::run` spawns `EventsRouter` after binding the adapter.
- Pass `adapter.subscribe_raw_events()` as the source.
- `router` lives for daemon lifetime (cancelled on shutdown).

Test: `cargo check -p octo-whatsapp`. Existing tests still pass (router spawn is opt-in via feature flag or daemon run).

Commit: `feat(events): event router + persistence + fan-out (Part B Tasks 11-15)`.

---

## Part C — MCP notifications + clients/list + daemon.methods (Tasks 16-24)

### Task 16: MCP `events.tail` tool

Edit `crates/octo-whatsapp/src/mcp_server.rs`:
- Register `events_tail` tool descriptor.
- `handle_tools_call("events_tail", params, socket)` → forward to RPC `events.tail`.
- Return JSON `{ "events": [...], "lagged": usize }`.

Test: `cargo test -p octo-whatsapp mcp_server::tests`. Add `mcp_events_tail_forwards_to_rpc` test.

### Task 17: MCP `notifications/resources/updated` on event arrival

Edit `crates/octo-whatsapp/src/mcp_server.rs`:
- Add `pending_resource_updates: parking_lot::Mutex<Vec<String>>` to `McpServerState`.
- `daemon.events.tail` consumer (subscribe to router) pushes event ids into pending list.
- Debounced flush task: every 1s, if non-empty, send `notifications/resources/updated` with the list.

Test: `cargo test -p octo-whatsapp mcp_server::tests`. Add `mcp_resources_updated_debounced` test.

### Task 18: MCP `notifications/tools/list_changed` on tools.enable toggle

Edit `crates/octo-whatsapp/src/mcp_server.rs`:
- `tools.enable` / `tools.disable` RPC mutates `Arc<RwLock<HashSet<ToolName>>>`.
- On change, set `tools_list_changed_pending = true`.
- Debounced flush (1s) sends `notifications/tools/list_changed`.

Test: `cargo test -p octo-whatsapp mcp_server::tests`. Add `mcp_tools_list_changed_on_enable` test.

### Task 19: MCP `clients/list` RPC + tool

Create `crates/octo-whatsapp/src/ipc/handlers/clients.rs`:
- `ClientsList` returns `{ "clients": [{ "session_id": "mcp-abc", "since_ts": ..., "subscribed_events": true }] }`.
- Track active sessions in `McpServerState`.

Wire into mcp_server.rs as `clients_list` tool.

Test: `cargo test -p octo-whatsapp clients::tests`.

### Task 20: MCP `daemon.methods.list` RPC + tool

Create `crates/octo-whatsapp/src/ipc/handlers/daemon_methods.rs`:
- `DaemonMethodsList` returns `{ "methods": ["version.get", "status.get", ...] }` from the `HandlerRegistry`.
- `DaemonMethodsHelp` returns `{ "name": "send.text", "params_schema": {...} }` for one method.

Wire into mcp_server.rs as `daemon_methods_list` and `daemon_methods_help` tools.

Test: `cargo test -p octo-whatsapp daemon_methods::tests`. Add test for list + help round-trip.

### Task 21: Update MCP tool count

Edit `crates/octo-whatsapp/src/mcp_server.rs`:
- Bump `EXPECTED_TOOL_COUNT` from 39 to 43 (add events_tail + clients_list + daemon_methods_list + daemon_methods_help).

Test: `cargo test -p octo-whatsapp mcp_server::tests::tools_list_count`.

Commit: `feat(mcp): events.tail + notifications + clients.list + daemon.methods (Part C Tasks 16-21)`.

### Task 22: Add tests/it_event_router_persistence.rs integration test

Create `crates/octo-whatsapp/tests/it_event_router_persistence.rs`:
- Spawn a test router with a mock `raw_event_tx` (broadcast::channel(16)).
- Send 3 events.
- Assert `events_buffer.list()` returns 3 events with correct order + ids.
- Assert eviction at max_rows boundary.

Test: `cargo test -p octo-whatsapp --test it_event_router_persistence`. Expect: 3+ tests pass.

### Task 23: Add tests/it_event_fanout.rs integration test

Create `crates/octo-whatsapp/tests/it_event_fanout.rs`:
- Spawn router with 2 sinks.
- Send 5 events.
- Both sinks receive all 5 in order.
- Close one sink's receiver → router increments its `lagged` counter, continues serving the other.

Test: `cargo test -p octo-whatsapp --test it_event_fanout`. Expect: 2+ tests pass.

### Task 24: Add tests/it_mcp_notifications_debounce.rs integration test

Create `crates/octo-whatsapp/tests/it_mcp_notifications_debounce.rs`:
- Spawn MCP server in test mode (stdin/stdout piped).
- Send 5 events rapidly.
- Assert exactly ONE `notifications/resources/updated` notification arrives after ~1s debounce.
- Assert the notification contains all 5 event ids.

Test: `cargo test -p octo-whatsapp --test it_mcp_notifications_debounce --features test-helpers`. Expect: 1 test pass.

Commit: `test(events): router + fanout + MCP notification integration tests (Tasks 22-24)`.

---

## Part D — CLI events subcommand + coverage sweep (Tasks 25-32)

### Task 25: Add `events tail --follow` to CLI

Edit `crates/octo-whatsapp/src/cli.rs`:
- `EventsCmd::Tail { follow: bool, limit: usize }` subcommand.
- Calls RPC `events.tail` with `{ "follow": ..., "limit": ... }`.
- If `--follow`, long-poll: keep the connection open, print events as they arrive (one JSON object per line).

Test: `cargo test -p octo-whatsapp cli::tests::events_tail_dispatches_rpc`.

### Task 26: Add `events list/show/replay` CLI subcommands

Edit `crates/octo-whatsapp/src/cli.rs`:
- `EventsCmd::List { since_ts: Option<i64>, since_id: Option<u64>, limit: usize }`.
- `EventsCmd::Show { id: String }`.
- `EventsCmd::Replay { since_id: u64, limit: usize }`.

Test: `cargo test -p octo-whatsapp cli::tests`.

### Task 27: Add `clients/list` + `daemon methods list/help` CLI subcommands

Edit `crates/octo-whatsapp/src/cli.rs`:
- `ClientsCmd::List` (top-level `clients list`).
- `DaemonCmd::Methods { subcommand: MethodsCmd }` where `MethodsCmd::{ List, Help { method: String } }`.

Test: `cargo test -p octo-whatsapp cli::tests`.

Commit: `feat(cli): events tail/list/show/replay + clients.list + daemon.methods (Tasks 25-27)`.

### Task 28: Handler test refresh for events.tail with mock-bound router

Edit `crates/octo-whatsapp/src/ipc/handlers/events.rs` (or new `events_tail_router.rs`):
- `events_tail_with_router_returns_empty` — bind a router that has no events.
- `events_tail_with_router_returns_events` — push 2 events directly to the buffer, call handler, assert 2 events returned.

Test: `cargo test -p octo-whatsapp events::tests`.

### Task 29: Coverage sweep — events handler test gap closure

Run `cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only`:
- Identify files with <85% lines or <75% branches in events.rs, events_router.rs, events_persister.rs, mcp_server.rs.
- Add tests to close gaps.

Test: coverage re-measurement. Target: all gates still pass.

### Task 30: Workspace test pass

`cargo test --workspace --features test-helpers`. Expect: 270 + 30+ new tests pass; no regressions.

### Task 31: Workspace lint + format

`cargo clippy --all-targets --all-features -- -D warnings` and `cargo fmt -- --check`. Expect: 0 warnings, 0 diff.

### Task 32: Local coverage measurement + commit

`cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only`. Target:
- **Lines ≥ 85.00%**
- **Branches ≥ 75.00%**

If yes: commit Part D. Done.
If no: identify remaining gap, add targeted tests (Tasks 33-35), re-measure.

Commit: `test(events): coverage sweep + clippy/fmt gates (Part D)`.

---

## Part E — Conditional polish (Tasks 33-35, only if needed)

### Task 33: Add clock skew detection (if not already done)

Edit `crates/octo-whatsapp/src/events.rs`:
- If parsed `ts_unix_ms > now() + 60_000`, emit `InboundEvent::Connection { kind: ConnectionKind::ClockSkewDetected, ts, ts_mono_ns }`.

Test: `cargo test -p octo-whatsapp events::tests::clock_skew_flag`.

### Task 34: Add `daemon.events.evicted_total` metric

Edit `crates/octo-whatsapp/src/events_persister.rs`:
- Track total evicted count in `EventsBuffer::total_evicted: AtomicU64`.
- Surface via `status.get`.

Test: `cargo test -p octo-whatsapp events_persister::tests`.

### Task 35: Final coverage + commit

Re-measure. If both gates pass: commit. If not: escalate to user (gate renegotiation or scope reduction).

---

## YAGNI guard rails

- ❌ No new stoolap tables beyond `events` (the `trigger_runs` table belongs to Phase 4).
- ❌ No rules engine changes (Phase 4 owns `arc_swap::ArcSwap<Ruleset>`).
- ❌ No trigger runners (Phase 4 owns `triggers.run`).
- ❌ No persistence of `Rule` / `Trigger` definitions (Phase 4).
- ❌ No `daemon.clock_skew` event emission beyond the `ConnectionKind::ClockSkewDetected` flag (deferred to Phase 4 hardening).
- ❌ No actual MCP `notifications/progress` for long ops (deferred — Phase 3 has no long ops).
- ❌ No Prometheus metrics integration (Phase 5).

---

## Coverage expectations

After Parts A-D:
- New code: ~800-1000 LoC (events.rs ~250 + events_router.rs ~300 + events_persister.rs ~150 + handlers + tests).
- New tests: ~30 tests.
- Per-file coverage: events.rs ≥85%, events_router.rs ≥85%, events_persister.rs ≥90%, mcp_server.rs ≥85% (small bump from new tools).
- Crate-level: stay ≥85% / ≥75% (should hold since new tests are comprehensive).

If branches fall <75% (likely on the parser's `match` arms), add per-variant parse-failure tests.

---

## Verification section

```bash
# Pre-merge gate
cargo check --workspace --all-features
cargo test -p octo-whatsapp --features test-helpers
cargo test -p octo-adapter-whatsapp --features test-helpers --test inherent_smoke
cargo clippy -p octo-whatsapp -p octo-adapter-whatsapp --all-targets --all-features -- -D warnings
cargo fmt -- --check
cargo llvm-cov --no-default-features --features test-helpers -p octo-whatsapp --summary-only
```

Expected:
- `cargo test -p octo-whatsapp`: 270 (existing) + 30+ (Phase 3) = ~300+ tests pass
- `cargo llvm-cov --summary-only`: lines `≥85.00%`, branches `≥75.00%`
- `cargo clippy`: 0 warnings
- `cargo fmt --check`: 0 diff
- `daemon.api.version = "1.0.0+phase3"` returned by `version.get`

---

## Critical files

**Modified:**
- `crates/octo-whatsapp/src/events.rs` (full parser rewrite)
- `crates/octo-whatsapp/src/daemon.rs` (events_buffer field + router spawn)
- `crates/octo-whatsapp/src/config.rs` (events config section)
- `crates/octo-whatsapp/src/ipc/handlers/events.rs` (full handler rewrite with list/show/replay/tail)
- `crates/octo-whatsapp/src/ipc/handlers/mod.rs` (register new RPC methods + clients + daemon_methods)
- `crates/octo-whatsapp/src/mcp_server.rs` (new tools + notifications)
- `crates/octo-whatsapp/src/cli.rs` (events tail --follow, clients, daemon.methods)
- `crates/octo-whatsapp/Cargo.toml` (add smallvec dependency)
- `crates/octo-whatsapp/src/lib.rs` (declare new modules)

**Created:**
- `crates/octo-whatsapp/src/events_persister.rs` (EventsBuffer + db_writer)
- `crates/octo-whatsapp/src/events_router.rs` (EventsRouter + EventsSink + EventsSubscriber)
- `crates/octo-whatsapp/src/ipc/handlers/clients.rs` (ClientsList)
- `crates/octo-whatsapp/src/ipc/handlers/daemon_methods.rs` (DaemonMethodsList + DaemonMethodsHelp)
- `crates/octo-whatsapp/tests/it_event_router_persistence.rs`
- `crates/octo-whatsapp/tests/it_event_fanout.rs`
- `crates/octo-whatsapp/tests/it_mcp_notifications_debounce.rs`
- `memory/whatsapp-phase3-handoff.md`

**Untouched:**
- `crates/octo-adapter-whatsapp` (Phase 3 is purely daemon-side; the `raw_event_tx` already exists)
- `crates/octo-network` (DOT protocol paths unchanged)
- `crates/octo-whatsapp-onboard` (onboarding unchanged)

---

## Handoff update

Append to `memory/whatsapp-phase3-handoff.md`:

```markdown
## Status as of 2026-07-XX (Phase 3 — Events)

**Coverage gates:** lines XX.XX% / branches XX.XX% (target ≥85% / ≥75%).

What landed:
- Typed `InboundEvent` parser (8 variants + Unknown fallback).
- `EventsBuffer` ring + config (`[events] retention_days`, `max_rows`).
- `EventsRouter` with bounded mpsc fan-out + per-sink Lagged counters.
- `db_writer` task for stoolap `events` persistence.
- `events.list/show/replay/tail` RPCs.
- MCP `events.tail` tool + `notifications/resources/updated` (1s debounce) + `notifications/tools/list_changed`.
- `clients/list` + `daemon.methods.list|help` for agent discovery.
- CLI `events tail --follow` + `events list/show/replay` + `clients list` + `daemon methods list/help`.
- 30+ new tests (unit + integration).
- `daemon.api.version = "1.0.0+phase3"`.

Lessons for Phase 4:
- Per-sink mpsc fan-out + Lagged counter is the right shape for rules/triggers.
- Single-writer `db_writer` task avoids back-pressure — keep this pattern for `trigger_runs` in Phase 4.
- MCP notifications need debouncing or they flood on burst events.
```

Update `MEMORY.md` index.