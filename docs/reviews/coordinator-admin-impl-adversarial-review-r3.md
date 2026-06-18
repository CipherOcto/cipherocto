# CoordinatorAdmin impl: adversarial review round 3 (R23e)

**Date:** 2026-06-18
**Branch:** `next`
**Scope:** Verify the R23d fixes for `CoordinatorAdmin` in `octo-adapter-irc`,
look for regressions, and identify any issues introduced by the fixes
themselves.

## Verification of R23d fixes (R23c findings)

| ID  | Finding                                                      | R23d fix                                                    | Verified? |
|-----|--------------------------------------------------------------|-------------------------------------------------------------|-----------|
| N1  | `runtime_channels` never populated                           | `join_by_invite` pushes after `send_raw_line` succeeds; `list_own_groups` merges runtime with config | ✅ All 8 new tests + 41 pre-existing tests pass; runtime_channels is now reachable from `channel_for`, `send_envelope`, `list_own_groups` |
| N2  | `join_by_invite` no channel-name validation                  | Extracted `validate_channel_name` free fn used in both `IrcConfig::validate` and `join_by_invite` | ✅ `test_join_by_invite_rejects_join_zero`, `test_join_by_invite_rejects_malformed_channel_names`, `test_validate_channel_name_free_function` all pass |
| N3  | `shutdown()` is a no-op; listener leaks past adapter lifetime | Added `shutdown_tx: Mutex<Option<watch::Sender<bool>>>`, `listener_handle: Mutex<Option<JoinHandle>>`; shutdown now signals stop, drops out_tx, aborts the handle | ⚠️ Mechanically correct, but **see N14**: doc-comment and test contradict each other and don't match the code |
| N4  | `tx.send().await` blocks PING handling under backpressure    | Replaced with `tx.try_send()` + `tracing::warn!` on Full/Closed | ✅ Comment block correctly explains the trade-off (drop on overload vs disconnect); the warn gives visibility |
| N5  | `PRIVMSG_OVERHEAD = 32` constant breaks for long channel names | Added `max_payload_for_channel(channel)` per-call helper; `send_envelope` uses it | ✅ `test_max_payload_for_channel_shrinks_with_longer_names` proves the assembled line stays ≤ 512 bytes for a 48-char channel |
| N6  | Unused `rustls-pemfile` dependency                           | Removed from `Cargo.toml`                                       | ✅ `cargo build -p octo-adapter-irc` succeeds without it |
| N7  | `validate()` called every `ensure_connected`                 | Kept as-is (cost is negligible vs TCP connect)                 | ✅ Acknowledged; out of scope for this round |
| N8  | `eprintln!` instead of `tracing`                             | Replaced with `tracing::warn!`/`tracing::info!` in listener paths | ✅ All listener errors now go through structured logging |
| N9  | (new) `validate_server` helper                              | Added, used in `IrcConfig::validate`                            | ✅ `test_irc_config_validate_rejects_bad_server_names` proves rejection of empty/whitespace/`/`/control/NUL/tab |

Net: 41 → 49 tests, all green. `cargo check` and `cargo fmt` clean.

## New findings introduced by R23d (R23e)

### CRITICAL

**N14 — `shutdown()` doc-comment is self-contradictory and doesn't match the code.**

`crates/octo-adapter-irc/src/lib.rs:1054-1058`:

```rust
// After this returns, `ensure_connected` is a no-op (the
// `connected` flag is false but we don't respawn — the
// operator should construct a fresh adapter). We don't
// reset `connected` here because callers might still
// observe the post-shutdown state.
```

But the actual code (line 1070):

```rust
*self.connected.lock().await = false;
```

The doc claims two contradictory things:
1. "the `connected` flag is false but we don't respawn" — but the only thing
   that prevents `ensure_connected` from respawning when `connected` is `false`
   is… nothing in the current code. The next `ensure_connected` call sees
   `connected == false` and proceeds to spawn a new listener.
2. "We don't reset `connected` here" — but the code explicitly does.

The intent was clearly **hard shutdown**: after `shutdown()`, the adapter is
dead and `ensure_connected` must refuse to respawn. That contract is safer
(the caller knows the adapter is unusable) but the implementation doesn't
enforce it. Under the current code, a caller that calls `shutdown()` and
then forgets to drop the adapter will get a *new* listener spawned and a
*new* `out_tx`/`shutdown_tx`/`listener_handle`, with the old listener
aborting in the background — surprising and bug-prone.

### HIGH

**N15 — `test_shutdown_clears_state_and_listener_can_respawn` doesn't actually verify respawn.**

`crates/octo-adapter-irc/src/lib.rs:2563-2599`. The test name and doc-comment
both promise respawn verification:

```rust
/// ... Then we verify that a subsequent `ensure_connected`
/// can still respawn the listener (i.e., shutdown didn't
/// permanently break the adapter).
async fn test_shutdown_clears_state_and_listener_can_respawn() { ... }
```

But the body only checks post-shutdown state (connected=false, out_tx=None,
shutdown_tx=None, listener_handle=None). There's no second `ensure_connected`
call, no second listener spawn, no second shutdown. The name lies and the
doc-comment lies.

### MEDIUM

**N16 — `test_join_by_invite_records_runtime_channel` doc-comment misdescribes what happens.**

`crates/octo-adapter-irc/src/lib.rs:2334-2372`:

```rust
/// Note: this test uses a *non-existent* server so the listener
/// is never spawned; the `send_raw_line` calls succeed because
/// the validation passes (which is the part under test). The
/// `runtime_channels` mutation happens immediately after the
/// validation, before any socket I/O is attempted.
```

This is wrong on three points:
1. The listener **is** spawned — `join_by_invite` calls `send_raw_line`,
   which calls `ensure_connected`, which always spawns a listener on
   first call. The spawn happens against `127.0.0.1:1` (refused), so the
   listener loops on connect retry.
2. `send_raw_line` **succeeds**, not fails. The listener is alive (just
   stuck in `connect_tls`/`connect_plain` retry); it owns `out_rx` and
   the mpsc buffer has capacity 128, so `tx.send("JOIN #beta").await`
   queues the line and returns `Ok(())`. The line never reaches the wire,
   but the enqueue succeeds.
3. The mutation happens **after** `send_raw_line` succeeds, not before
   any socket I/O. (See `join_by_invite` at line 1477: `send_raw_line(...).await?`
   before the push.) This is the correct ordering — if send fails, we
   don't pollute `runtime_channels`.

The test still passes for the right reason, but the doc misleads the
reader into thinking `send_raw_line` fails and the push happens first.
If a future maintainer reads this test and assumes "send_raw_line always
fails here so the push is the only side effect", they'll be confused when
the test breaks under a refactor.

**N17 — Tests leak listener tasks.**

`test_join_by_invite_records_runtime_channel`, `test_list_own_groups_dedupes_static_and_runtime`,
`test_join_by_invite_rejects_join_zero`, `test_join_by_invite_rejects_malformed_channel_names`
all spawn a listener (via `send_raw_line` → `ensure_connected`) and never
call `shutdown()`. Each test leaves a task looping on connect-refused to
port 1. The tokio test runtime is dropped at end of test, which kills the
task — but it's poor hygiene, and if any of these tests ever runs under
a `current_thread` flavor with a non-default runtime that doesn't get
torn down (e.g., an embedded test scenario), the leak surfaces.

**N18 — `runtime_channels` doc-comment is misleading about async usage.**

`crates/octo-adapter-irc/src/lib.rs:228-236`:

```rust
/// Uses `std::sync::Mutex` (not `tokio::sync::Mutex`) because
/// `channel_for` is a sync helper called from `&self` methods, and
/// the critical sections are tiny string-vec operations.
```

`channel_for` is called from async methods (`leave_group`, `add_member`,
`remove_member`, `destroy_group`, `list_own_groups`, `send_envelope`, etc.).
The lock is held in async context. The `std::sync::Mutex` choice is fine
because the critical section is short (no `.await` inside), but the
rationale cited in the comment is wrong. (If `channel_for` were truly
sync-only, then `tokio::sync::Mutex` would be wrong — but it's not the
async-ness that matters, it's the no-`.await`-inside-section rule.)

### LOW

**N19 — `validate_server` accepts `".."` and other non-hostnames.**

`crates/octo-adapter-irc/src/lib.rs` `validate_server` rejects whitespace,
`/`, and control bytes, but allows `".."`. RFC-952 hostnames require at
least one alphanumeric character and no leading/trailing `-`. For an IRC
adapter this is a stretch goal — servers won't have such names anyway.
Document the limitation or add a `.contains("..")` check.

**N20 — `runtime_channels` push happens after `send_raw_line` succeeds, but if push fails (poisoned mutex), the server-side JOIN has happened.**

`crates/octo-adapter-irc/src/lib.rs:1477-1483`:

```rust
self.send_raw_line(&format!("JOIN {}", invite.0)).await?;
{
    let mut runtime = self.runtime_channels.lock()
        .map_err(|e| transport_err(format!("runtime_channels poisoned: {e}")))?;
    ...
    runtime.push(invite.0.clone());
}
```

If the mutex is poisoned (a previous holder panicked), we return Err but
the JOIN was already sent. The server thinks the bot joined; the adapter
state disagrees. Edge case but worth a `tracing::warn!` so the operator
sees the divergence.

## Still unaddressed from R1

These were in the original R1 review and are not IRC-blocking; they're
WhatsApp-side or pre-existing design choices. They remain deferred to a
future WhatsApp-focused review round:

- **H1:** WhatsApp `can_join_by_invite=true` but `join_by_invite` is `Unimplemented`
- **H2:** WhatsApp `create_group` signature disambiguation footgun
- **H6:** WhatsApp `add_member` partial-success
- **M1, M4, M5, M10-M16:** WhatsApp-side
- **M3:** `health_check` ignores `use_tls` (IRC)
- **M7:** `add_member` doesn't require the bot to be a channel op (IRC) — best-effort by design
- **M8:** `health_check` doesn't call `ensure_connected` (IRC)

## Action plan for R23f (this round's fixes)

1. **N14 (CRITICAL)** — Add `shutting_down: AtomicBool` field; make
   `ensure_connected` refuse to respawn after shutdown; fix the doc-comment
   to be accurate.
2. **N15 (HIGH)** — Rename `test_shutdown_clears_state_and_listener_can_respawn`
   to `test_shutdown_prevents_respawn` and verify that
   `ensure_connected` returns `Err` after `shutdown()`.
3. **N16 (MEDIUM)** — Fix the doc-comment in `test_join_by_invite_records_runtime_channel`
   to explain the actual flow (listener spawned, mpsc buffer accepts the
   line, push happens after send succeeds).
4. **N17 (MEDIUM)** — Add `adapter.shutdown().await.unwrap();` to the
   four leaking tests.
5. **N18 (MEDIUM)** — Fix the doc-comment for `runtime_channels` (rationale
   is no-await-in-section, not sync-only).
6. **N19 (LOW)** — Add `s.contains("..")` check to `validate_server`.
7. **N20 (LOW)** — Add `tracing::warn!` on `runtime_channels` poison.