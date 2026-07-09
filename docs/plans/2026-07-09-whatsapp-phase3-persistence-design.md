# WhatsApp Phase 3 — Events Persistence + Restart-Survives Live Test

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

## Context

Phase 3 of the WhatsApp runtime CLI + MCP project (`docs/plans/2026-07-06-whatsapp-runtime-cli-mcp-phase3.md`) shipped Parts A (typed `InboundEvent` parser) and B (in-memory `EventsBuffer` + `events.list/show/replay/tail` RPCs). Part D — disk persistence — never landed. Today, `EventsBuffer` is misleadingly named: it lives in `events_persister.rs` but does no I/O. Every daemon restart wipes the event history.

13 live chains exercise the daemon; none verify that events survive a restart. The operator cannot use `events.list` to inspect history from before a daemon bounce.

This plan ships Part D (disk persister) and Part F (restart-survives live test). It is local-only and stack on top of phase 6.1.1.

## Architecture

Mirror the proven `rules/persister.rs` pattern (deployed, working). Differences:

- **No debounce** — events are time-sensitive. Use a coalescing interval (default 5s) for fsync.
- **Append-only NDJSON** — `events.ndjson` not `rules.toml`. Each line = `{id, ts_unix_ms, ts_mono_ns, event}`. Append-friendly, crash-safe.
- **WAL = the file itself** — single append-only file, no `.wal` + `.toml` split.
- **Reload on startup** — `EventsBuffer::load_from_disk` reads all lines, validates, hydrates the buffer.
- **No coalesce** — events are unique records; can't collapse. Recent-window flush amortizes I/O.
- **Backpressure** — bounded mpsc (capacity 4096). If full, drop with counter (already lossy in `raw_event_tx`).

### On-disk format (NDJSON)

```jsonl
{"id":1,"ts_unix_ms":1752345678901,"ts_mono_ns":1234567890123456,"event":{"kind":"Message",...}}
{"id":2,"ts_unix_ms":1752345679123,"ts_mono_ns":1234567891234567,"event":{"kind":"Unknown",...}}
```

- `id`: u64 monotonic, replayed as-is on reload.
- `ts_unix_ms`: u64 wall clock. Used for `since_ts` filtering.
- `ts_mono_ns`: u64 monotonic clock. Used for ordering.
- `event`: full `InboundEvent` JSON (existing serde derived).

### Crash-safety

- Per-event fsync too slow. Windowed: actor `flush_interval_ms` (default 5s) calls `file.flush().await` (fsync).
- On actor exit, flush once more.
- Crash → lose up to 5s of events. Same risk profile as rules persister.

### Partial line handling

- Writer: `write_all(line)` + `write_all(b"\n")` per event. Each `write_all` is one syscall; the kernel won't split.
- If process is SIGKILL'd mid-line, trailing bytes are detectable (no trailing newline, or JSON.parse fails).
- Reader: scan file, parse line. On `serde_json::Error` or missing trailing newline → truncate file to last good offset (`ftruncate`) and log warning.

## Files

| File | Change |
|---|---|
| `crates/octo-whatsapp/src/events_buffer.rs` (new) | Move `EventsBuffer` struct + 9 unit tests verbatim from `events_persister.rs`. Add `hydrate_from_entries` method. |
| `crates/octo-whatsapp/src/events_persister.rs` (rewritten) | `EventsPersister` actor + `EventsPersisterHandle` + `PersistError` + helpers. |
| `crates/octo-whatsapp/src/events.rs` (unchanged) | InboundEvent enum already serializes correctly. |
| `crates/octo-whatsapp/src/events_router.rs` (unchanged) | `db_writer` keeps calling `buffer.push(ev)`; new actor will live alongside. |
| `crates/octo-whatsapp/src/daemon.rs` (small) | Spawn `EventsPersister` at boot, drain on shutdown, stash handle in `PERSISTER_HANDLES`. |
| `crates/octo-whatsapp/src/config.rs` (small) | Add `events.persistence_enabled` + `events.flush_interval_ms` + `events.resolved_persistence_path()`. |
| `crates/octo-whatsapp/tests/it_event_persistence.rs` (new) | 10 integration tests. |
| `crates/octo-whatsapp/tests/live_daemon_test.rs` (extended) | New `live_chain_l_restart_survives`. |

## Component shape

```rust
// events_persister.rs
pub struct EventsPersisterHandle {
    tx: mpsc::Sender<InboundEvent>,
    flush: mpsc::Sender<()>,
    join: tokio::task::JoinHandle<()>,
}

impl EventsPersisterHandle {
    pub fn spawn(
        buffer: Arc<EventsBuffer>,
        path: Option<PathBuf>,
        flush_interval: Duration,
        cancel: CancellationToken,
    ) -> Result<Self, PersistError>;

    pub async fn flush_sync(&self) -> Result<(), PersistError>;
}
```

Actor loop (`tokio::select!` over 3 branches):

```rust
loop {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            while let Ok(ev) = rx.try_recv() { persist_one(&mut file, &ev); }
            file.flush().await.ok();
            break;
        }
        _ = flush_ticker.tick() => {
            file.flush().await.ok();
        }
        Some(ev) = rx.recv() => {
            buffer.push(ev.clone());
            persist_one(&mut file, &ev).await;
        }
        else => break,
    }
}
```

`persist_one` does `file.write_all(line.as_bytes()).await?; file.write_all(b"\n").await?;`. No per-event fsync.

## Reload semantics

`fn load_initial_events(path: &Path, buffer: &EventsBuffer) -> Result<LoadStats, PersistError>`:

1. Open file with `std::fs::File`, read_to_end
2. Split on `b'\n'` (last empty element ignored)
3. For each line: `serde_json::from_slice::<PersistedEvent>`. On error: log warning, increment `dropped_malformed`, continue.
4. `buffer.hydrate_from_entries([(id, ev)])` — appends one at a time.
5. Detect trailing partial: if file does NOT end with `b'\n'`, truncate to last good offset, log "dropped trailing partial line of N bytes".
6. Return `LoadStats { loaded, skipped, dropped_partial_bytes }`.

`hydrate_from_entries` is a new method on `EventsBuffer`:

```rust
pub fn hydrate_from_entries(&self, entries: impl IntoIterator<Item = (u64, InboundEvent)>) {
    let mut g = self.inner.lock();
    for (id, ev) in entries {
        // Update next_id if this is higher than what we have.
        let _ = self.next_id.fetch_max(id + 1, Ordering::Relaxed);
        g.push_back((id, ev));
    }
    self.total_pushed.store(g.len() as u64, Ordering::Relaxed);
}
```

`LoadStats` is logged at boot, exposed via `daemon.status.get`.

## Config knobs

```rust
pub struct EventsConfig {
    pub max_rows: usize,            // existing
    pub retention_days: u32,        // existing, advisory
    pub persistence_enabled: bool,  // NEW, default true
    pub flush_interval_ms: u64,     // NEW, default 5000
}

impl EventsConfig {
    pub fn resolved_persistence_path(&self) -> PathBuf {
        self.persistence_path
            .clone()
            .unwrap_or_else(|| default_data_dir().join("events").join("events.ndjson"))
    }
}
```

For dev, default path: `~/.local/share/octo/whatsapp/events/events.ndjson`.
For live tests, redirected under `XDG_RUNTIME_DIR` (per phase 6.1.1 hermeticity fixup).

## Tests

### `tests/it_event_persistence.rs` (10 tests)

| Test | Asserts |
|---|---|
| `append_then_reload_round_trips` | Push 3 events, flush, kill actor. New actor + buffer loads 3 events with same ids. |
| `append_writes_one_ndjson_line_per_event` | After 5 events, file has 5 lines, each parseable JSON, last byte is `\n`. |
| `reload_truncates_partial_trailing_line` | Write 2 valid lines + 1 partial `{"id":3,"ev...` (no newline). Reload returns 2 events, file is truncated to last good offset. |
| `reload_skips_malformed_middle_lines` | 3 valid + 1 `{garbage` in middle. Reload returns 3 events, `skipped_malformed == 1`. |
| `reload_assigns_next_id_after_max` | After 5 events with ids 1..=5, reload + push 1 new event → id = 6, no collision. |
| `eviction_to_disk_truncates_file` | Push 10 events to buffer with `max_rows=3`, force flush. File has 3 lines (only the surviving ids). Reload returns 3 events. |
| `persistence_disabled_creates_no_file` | `path: None` actor. Push 100 events, no file exists. |
| `flush_sync_blocks_until_disk` | Push event, immediately call `flush_sync()` — returns only after fsync completes. |
| `shutdown_drain_writes_pending` | Spawn actor with short flush interval. Push 5 events. Cancel. Drain phase writes remaining before exit. |
| `concurrent_push_and_reload_safe` | Spawn actor, push in background. After 50ms, spawn second actor + new buffer reading the same path — sees all events pushed so far, plus its own new ones start at `max+1`. |

### `live_chain_l_restart_survives` (live_daemon_test.rs)

```
1. fixture() — operator already authed
2. status.get → assert bot_state == Connected (gate)
3. capture baseline: events.list (count = baseline)
4. send.text to self (peer_to_jid("+552199554474325")) with body "phase3-persist-<ts>"
5. wait up to 30s polling events.list until count > baseline
6. assert that event list contains an event with text == "phase3-persist-<ts>"
7. KILL daemon (cancel token via daemon.shutdown, OR drop the runtime)
8. SPAWN new daemon in same process with same data_dir
9. wait for Connected
10. events.list → assert count >= baseline, and that text matches
    (the persisted event survived restart)
```

No `_MEMBER` requirement (sends to self only). Reuses `phase612` group only if multi-member tests are also running.

## Verification gates

- `cargo test -p octo-whatsapp --lib` — 270 + ~10 new tests pass
- `cargo test -p octo-whatsapp --test it_event_persistence` — 10 new integration tests pass
- `cargo clippy --all-targets --all-features -- -D warnings` — clean
- `cargo fmt --check` — clean
- Live: `cargo test -p octo-whatsapp --test live_daemon_test live_chain_l_restart_survives` — green

## Commit plan

1. `refactor(octo-whatsapp): extract EventsBuffer to events_buffer.rs`
2. `feat(octo-whatsapp): disk persistence for EventsBuffer (events.ndjson)`
3. `feat(octo-whatsapp): persistence config knobs + daemon spawn/drain`
4. `test(octo-whatsapp): integration tests for EventsPersister reload`
5. `test(octo-whatsapp): live_chain_l_restart_survives`

5 commits. ~6-10h work. Local-only, no push per standing rule.

## Tradeoffs

- **NDJSON over sqlite/sled/redb** — debuggable with `head`/`jq`, no extra dep, schema migration = optional field
- **Append-only, no compaction** — 1M events × ~500 bytes ≈ 500 MB; 30-day retention default. Compaction / archival can come later.
- **At-most-once persistence** — process crash between mpsc recv and fsync loses up to 5s of events. Matches `raw_event_tx` lossy contract.
- **5s default flush window** — operator-tunable via `events.flush_interval_ms` if 5s of event loss is unacceptable.
- **Hermeticity** — `data_dir` redirect covers `events.ndjson` automatically. Verify with a hermetic-test before declaring done.
