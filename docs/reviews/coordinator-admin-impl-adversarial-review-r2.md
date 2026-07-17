# Adversarial Review: CoordinatorAdmin Implementation (R20 + R21) — Round 2

**Date:** 2026-06-18
**Reviewer:** Adversarial review (Round 2 of N; loop ends when a round finds nothing)
**Scope:** R23b fixes (`HEAD..HEAD~0` working tree) for `crates/octo-adapter-irc/`,
plus the unmodified WhatsApp / coordinator_admin / error.rs files that
R1 flagged but R23b didn't touch.
**Method:** Cross-reference each R1 finding against R23b's diff; trace
every admin path through the new `out_tx` channel and the new TLS
plumbing; spot-check the new code for regressions.

---

## R23b scope (what changed)

| File | Lines added | Topic |
|---|---:|---|
| `crates/octo-adapter-irc/Cargo.toml` | +3 | New deps: `rustls`, `rustls-pki-types`, `webpki-roots` |
| `crates/octo-adapter-irc/src/lib.rs` | +~720 | Real TLS, unified outbound channel, `IrcConfig::validate`, watchdog `mark_disconnected`, runtime channels |

Outside R23b but still in R1 scope (NOT fixed by R23b):

- `crates/octo-adapter-whatsapp/src/adapter.rs` — H1, H2, H6 unaddressed.
- `crates/octo-network/src/dot/adapters/coordinator_admin.rs` — M2 unaddressed.

---

## Status of each R1 finding after R23b

### CRITICAL

| ID | Status | Notes |
|---|---|---|
| **C1** IRC `connect_tls` is a no-op | ✅ FIXED | Real `tokio_rustls::client::TlsStream` handshake via `tls_client_config()` (line 486-500, 574-584). |
| **C2** IRC `send_message` does not write to wire | ✅ FIXED | Now enqueues PRIVMSG lines through `send_raw_line` (line 822-832). |
| **C3** IRC `send_message` doesn't `ensure_connected` | ✅ FIXED | First line of `send_message` calls `ensure_connected` (line 779). |
| **C4** IRC capability / `list_own_groups` / `join_by_invite` inconsistent | ⚠️ HALF-FIXED (REGRESSION) | `runtime_channels` field exists, `channel_for` consults it, `send_message` consults it — **but `join_by_invite` never populates it and `list_own_groups` never merges it**. See **N1** below. |

### HIGH

| ID | Status | Notes |
|---|---|---|
| **H1** WhatsApp `can_join_by_invite: true` but `join_by_invite` is `Unimplemented` | ❌ NOT FIXED | No WhatsApp changes in R23b. Capability still lies. |
| **H2** WhatsApp `create_group` signature disambiguation footgun | ❌ NOT FIXED | Inherent `create_group(subject, &[&str])` at line 652; trait impl at line 1411 calls `self.create_group(subject, &phones)` (where `phones: Vec<&str>`). One refactor away from infinite recursion. |
| **H3** IRC `join_by_invite` doesn't validate channel name | ⚠️ PARTIAL | `send_raw_line` rejects CRLF/NUL (line 285) — closes the injection vector. But the `JOIN 0` / no-prefix / bad-char cases are still wide open. See **N2**. |
| **H4** IRC `send_raw_line` doesn't reject CRLF | ✅ FIXED | Line 285 rejects CR/LF/NUL with a 400 error. |
| **H5** IRC listener has no watchdog | ✅ FIXED | `mark_disconnected` (line 262-265) is called by `send_raw_line` and `receive_messages` when the listener's channels indicate death. See **N3** for a residual issue. |
| **H6** WhatsApp `add_member` partial-success | ❌ NOT FIXED | Still propagates promote-to-admin errors as full errors and lumps partial-failure into a 500 with the inner `r.error` (line 1495-1522). |
| **H7** `IrcConfig::validate` is missing | ✅ FIXED | New `IrcConfig::validate()` (line 79-127) called from `ensure_connected` (line 212). |

### MEDIUM

| ID | Status | Notes |
|---|---|---|
| M1 WhatsApp `set_ephemeral` u64→u32 truncation | ❌ NOT FIXED | |
| M2 `GroupId::new` / `PeerId::new` accept empty | ❌ NOT FIXED | |
| M3 IRC `health_check` ignores `use_tls` | ❌ NOT FIXED | Still `TcpStream::connect` only. |
| M4 WhatsApp initial-admin promote silent | ❌ NOT FIXED | |
| M5 WhatsApp metadata errors swallowed in `create_group` | ❌ NOT FIXED | |
| M6 IRC `can_list_own_groups` misleading | ⚠️ HALF-FIXED | `runtime_channels` field exists, but `list_own_groups` never merges it (see N1). |
| M7 IRC `add_member` requires op status | ❌ NOT FIXED | Still fire-and-forget. |
| M8 IRC `health_check` doesn't call `ensure_connected` | ❌ NOT FIXED | |
| M9 IRC `receive_messages` silently drops on `tx.try_send` failure | ✅ FIXED | Now uses `tx.send().await` (line 708). Trade-off: backpressure vs. ping timeouts. See **N4**. |
| M10 IRC docs/code mismatch on `can_join_by_id` | ❌ NOT FIXED | |
| M11 WhatsApp O(N) lookup per group | ❌ NOT FIXED | |
| M12 `GroupModeFlags` semantics in `set_ephemeral` | ❌ NOT FIXED | |
| M13 WhatsApp N+1 invite URL | ❌ NOT FIXED | |
| M14 `GroupHandle.is_admin` ambiguous | ❌ NOT FIXED | |
| M15 IRC `channel_for` accepts invalid names in `config.channels` | ✅ FIXED | `IrcConfig::validate` rejects non-prefixed names (line 109). |
| M16 WhatsApp `group_to_jid` not robust | ❌ NOT FIXED | |

### LOW

| ID | Status | Notes |
|---|---|---|
| L1 `MAX_PAYLOAD_PER_MSG` doesn't account for channel-name length | ❌ NOT FIXED | Still a hardcoded `PRIVMSG_OVERHEAD = 32`. See **N5**. |
| L2 `decode_message` permissive on fragment `i/n` | ❌ NOT FIXED | |
| L3 Test naming convention | n/a | Cosmetic. |
| L4 `extract_mode_flags` could be a method | n/a | Cosmetic. |
| L5 `adapter_version()` / `platform_type()` constants duplicated | ❌ NOT FIXED | Still `0x0006` hardcoded twice (line 416, 1388). |

### Summary of R1-finding status

| Severity | Fixed | Half-fixed | Not fixed | Total |
|---|---|---|---|---|
| CRITICAL | 3 | 1 | 0 | 4 |
| HIGH     | 2 | 1 | 3 | 7 |
| MEDIUM   | 2 | 1 | 13 | 16 |
| LOW      | 0 | 0 | 1 | 5 (2 n/a) |

---

## NEW findings (introduced by R23b)

### N1 — CRITICAL: `runtime_channels` is never populated (C4 fix is broken)

**File:** `crates/octo-adapter-irc/src/lib.rs:1218-1234, 1278-1291, 172-180`

The R23b diff added a `runtime_channels: StdMutex<Vec<String>>` field
with the doc-comment "Channels the bot has joined at runtime (via
`join_by_invite`). Merged with `config.channels` by `list_own_groups`
and `channel_for`..." — and `channel_for` / `send_message` both
consult the field. **But `join_by_invite` never pushes to it.**

```bash
$ grep -n "runtime_channels\.\(lock\|push\|insert\|extend\|retain\|remove\|pop\)" \
      crates/octo-adapter-irc/src/lib.rs
(no output)
```

`join_by_invite` (line 1278-1291) just sends `JOIN <name>` and returns
a `GroupHandle`; it never appends `invite.0` to `runtime_channels`.
And `list_own_groups` (line 1218-1234) iterates only
`self.config.channels`, not the runtime vec.

So the C4 fix is **non-functional**:

1. `join_by_invite("#foo")` sends `JOIN #foo`, returns a handle.
2. `list_own_groups()` does **not** include `#foo` (doc-comment lies).
3. `channel_for(GroupId("server:#foo"))` falls through to the runtime
   lookup, finds nothing, returns 404 with the message "...nor the
   runtime-joined set" (which is empty).
4. `send_message(domain(server:#foo))` does the same lookup, fails with
   `Unreachable("No channel for domain ...")`.
5. `add_member(server:#foo, ...)` / `remove_member(...)` / all other
   admin actions on `#foo` 404.

**Why the existing tests don't catch it:** `test_list_own_groups_returns_configured_channels`
only checks the static config path. `test_send_raw_line_writes_through_listener`
tests admin on a *configured* channel, not a joined-at-runtime one.

**Impact:** The capability `can_list_own_groups: true` is **still a lie**
for any channel the bot joined outside the static config. The docstring
on `runtime_channels` is **also a lie**. Worse, the runtime path in
`channel_for` and `send_message` makes the failure mode look like
"No channel for domain" rather than "we forgot to track joins", which
will confuse every operator who tries to use `join_by_invite`.

**Fix (minimum):**

```rust
async fn join_by_invite(
    &self,
    invite: &InviteRef,
) -> Result<GroupHandle, PlatformAdapterError> {
    // Validate before sending (see N2).
    validate_channel_name(&invite.0)?;
    self.send_raw_line(&format!("JOIN {}", invite.0)).await?;
    let mut runtime = self.runtime_channels.lock()
        .map_err(|e| transport_err(format!("runtime_channels poisoned: {e}")))?;
    if !runtime.iter().any(|c| c == &invite.0) {
        runtime.push(invite.0.clone());
    }
    Ok(GroupHandle {
        id: self.full_id(&invite.0),
        subject: None,
        invite_url: None,
        is_admin: false,
        member_count: None,
        mode_flags: None,
    })
}

async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
    let mut names: Vec<String> = self.config.channels.clone();
    let runtime = self.runtime_channels.lock()
        .map_err(|e| transport_err(format!("runtime_channels poisoned: {e}")))?;
    for ch in runtime.iter() {
        if !names.iter().any(|c| c == ch) {
            names.push(ch.clone());
        }
    }
    Ok(names.into_iter().map(|ch| GroupHandle {
        id: self.full_id(&ch),
        subject: None,
        invite_url: None,
        is_admin: false,
        member_count: None,
        mode_flags: None,
    }).collect())
}
```

**Fix (test):** Add a unit test that calls `join_by_invite("#newchan")`
then `list_own_groups()` and asserts the new channel appears. This test
will fail until the bug is fixed.

---

### N2 — HIGH: `join_by_invite` does not validate channel name

**File:** `crates/octo-adapter-irc/src/lib.rs:1278-1291`

`send_raw_line` rejects CR/LF/NUL (H4 fix), but `join_by_invite` does
not validate the channel name itself before forming the `JOIN` line:

```rust
self.send_raw_line(&format!("JOIN {}", invite.0)).await?;
```

Concretely, the following `InviteRef` values would be accepted:

1. `InviteRef::new("0")` — IRC's `JOIN 0` makes the client PART all
   channels. Same dismembering risk the R1 H3 review flagged.
2. `InviteRef::new("no-hash-prefix")` — server replies with
   `ERR_NOSUCHCHANNEL` (403) or similar, silently.
3. `InviteRef::new("")` — empty JOIN, server error.
4. `InviteRef::new("#chan,foo")` — comma is a multi-target separator
   in some IRC servers; an attacker could turn one join into many.

**Why this matters:** The `IrcConfig::validate()` function (H7 fix) now
rejects these patterns at config-load time, but `join_by_invite`
accepts an arbitrary `InviteRef` from a caller with no validation.

**Fix:** Extract the per-channel validation into a free function
`validate_channel_name(name: &str) -> Result<(), String>` and call it
from both `IrcConfig::validate()` and `join_by_invite` (and arguably
from `channel_for` as a final safety net).

---

### N3 — HIGH: `mark_disconnected` drops `out_tx` but not the listener task

**File:** `crates/octo-adapter-irc/src/lib.rs:262-265, 531-563`

`mark_disconnected` resets `connected = false` and clears `out_tx`.
The next `ensure_connected` will spawn a **second** listener task. But
nothing tells the original listener to stop.

Today, `irc_listener` loops forever reconnecting. The original
listener task therefore keeps the inbound `tx` alive. When the
adapter is dropped (`shutdown`), only the `out_tx` sender on the
adapter side is dropped; the inbound `tx` sender is still held by the
listener.

**Concrete impact:**

1. `send_raw_line` fails (channel closed) → `mark_disconnected` is
   called → next `ensure_connected` spawns a second listener → we now
   have two listeners racing on the same `tx`/`out_rx` channel pair
   (the *old* listener is still holding `out_rx`, so the new
   listener will share that receiver — meaning both listeners compete
   for admin lines, with one of them getting `None` first and exiting
   "cleanly", while the other keeps running with a half-functional
   channel).

2. `IrcAdapter::shutdown` returns `Ok(())` without dropping the
   listener's `tx` sender or signalling the listener to exit. The
   task outlives the adapter. The `Box<IrcAdapter>` is freed (via the
   C-ABI `destroy_adapter`), but the spawned tokio task continues
   running with a dangling reference to the now-freed `tx`. This is
   **use-after-free** in the FFI path.

   The FFI's `create_adapter` (line 1394) `Box::into_raw`s the
   adapter. `destroy_adapter` (line 1408) `Box::from_raw`s and drops
   it. But the spawned listener task still holds a clone of `tx`
   (the inbound channel sender) and `out_rx`. After the adapter is
   dropped, `tx` becomes a leaked `mpsc::Sender` — its only remaining
   holder is the listener task. Messages sent to `tx` after the
   adapter drop go nowhere. More importantly, the listener task is
   still consuming CPU and TCP sockets.

**Fix:**

1. Give the listener a way to receive a stop signal. Add a
   `shutdown_tx: mpsc::Sender<()>` to the adapter, clone it into the
   listener spawn, and have the listener `select!` between
   `out_rx.recv()` and `shutdown_rx.recv()` (plus the read half).

2. `shutdown()` should drop the outbound sender (`*self.out_tx.lock().await = None;`)
   so `out_rx.recv()` returns `None` and the listener exits its loop
   *and* sends a stop signal. The two together guarantee orderly
   shutdown.

3. Consider `AbortHandle` on the spawned task as a backstop: store
   the `JoinHandle` (or `AbortHandle`) in `IrcAdapter` and call
   `abort()` in `shutdown()`.

---

### N4 — HIGH: `tx.send().await` in the IRC session loop blocks PING handling

**File:** `crates/octo-adapter-irc/src/lib.rs:702-715`

```rust
if tx.send(RawPlatformMessage { ... }).await.is_err() {
    eprintln!("IRC inbound channel closed; listener exiting");
    return Ok(());
}
```

The fix from M9 (`try_send` → `send`) trades silent drops for
backpressure, but it has a knock-on correctness bug: while
`tx.send().await` is awaiting, the entire `tokio::select!` is parked
on that future. The `out_rx` branch can't fire (the writer is held
mutably by the `select!` body), and the `reader.read_line()` branch
can't fire (the biased select! is committed to this branch).

If the inbound channel fills (capacity 4096) and the consumer
(`receive_messages`) stops draining, the listener blocks on
`tx.send()`. The IRC server's PING frames arrive but aren't read
(because we can't reach `reader.read_line()`). After the server's
ping timeout (typically 180-300 seconds), the server disconnects us.
We see `read_line()` return 0 next time around — but by then we've
been disconnected.

**Fix (option A — bounded backpressure):** Use `try_send` and on
`Full`, drain a small batch of messages from `rx` via a side-channel,
then retry. The bounded version drops oldest-first with a counter.

**Fix (option B — async drain task):** Move the `tx.send().await`
into a small dedicated task that owns the writer exclusively for
inbound delivery, while the listener's `select!` keeps the writer
for outbound. The two writers have to share the writer, which
requires an `Arc<Mutex<Writer>>` or an mpsc-of-writes. The cleanest
implementation is probably a `select!` between two separate senders
that share the writer via a mutex; but that brings its own
complexity.

**Fix (option C — accept the trade-off, document it loudly):** Use
`try_send` and increment a `dropped_inbound` counter; log at `warn!`
once per N drops. This restores the old behaviour but with
visibility.

Whichever path is chosen, document the choice on the
`inbound_channel_capacity` field.

---

### N5 — MEDIUM: `PRIVMSG_OVERHEAD = 32` constant breaks for long channel names (L1 not fixed)

**File:** `crates/octo-adapter-irc/src/lib.rs:135-139, 822-832`

```rust
const PRIVMSG_OVERHEAD: usize = 32;
const MAX_PAYLOAD_PER_MSG: usize = IRC_MAX_LINE_BYTES - PRIVMSG_OVERHEAD;
```

The 32-byte overhead is `PRIVMSG ` (8) + ` :\r\n` (4) + channel (20).
A channel name longer than 20 chars (`#a-very-long-channel-name`, 24
chars) produces `PRIVMSG #a-very-long-channel-name :<480 bytes>\r\n`
= 10+24+1+480+2 = **517 bytes > 512 IRC line limit**. The server
truncates or rejects silently.

The `send_message` loop (line 822-832) and the `send_raw_line` path
(line 830) both produce `PRIVMSG <channel> :<payload>` lines without
checking the actual channel name length.

**Fix:** Compute overhead per call:

```rust
const PRIVMSG_OVERHEAD_BASE: usize = 12; // "PRIVMSG " + " :\r\n"
fn max_payload_for_channel(channel: &str) -> usize {
    IRC_MAX_LINE_BYTES.saturating_sub(PRIVMSG_OVERHEAD_BASE + channel.len())
}
```

Use this in `send_message` when computing `chunks` and also validate
that `channel.len() + PRIVMSG_OVERHEAD_BASE + <min fragment size>`
fits. Document the channel-name-length limit in `IrcConfig::validate`.

**Why MEDIUM not HIGH:** The cap is a hard limit, and operators who
use long channel names will see partial-send failures, but the default
configs (e.g. `#cipherocto`) fit comfortably. Still a real bug.

---

### N6 — MEDIUM: Unused `rustls-pemfile` dependency

**File:** `crates/octo-adapter-irc/Cargo.toml:16`

```toml
rustls-pemfile = "2"
```

The R23b diff left `rustls-pemfile` in `Cargo.toml`, but the
implementation uses `webpki-roots` for the trust store, not
`rustls-pemfile`. Grep for `rustls_pemfile` / `pemfile` in the lib.rs
returns no matches.

**Impact:** Compile-time dependency bloat (rustls-pemfile pulls in
additional crates). Future maintainers will be confused about why
it's there.

**Fix:** Remove the line from `Cargo.toml`.

---

### N7 — MEDIUM: `IrcConfig::validate` is invoked on every `ensure_connected` — but only on the spawn path

**File:** `crates/octo-adapter-irc/src/lib.rs:208-217`

`ensure_connected` calls `self.config.validate()` **before** checking
the `connected` flag, so it's re-validated on every public call. Not
a correctness bug — `validate` is pure field-shape and cheap — but
the comment claims it's the pre-flight for "the first call". On the
steady-state hot path it's a redundant scan of the config struct.

**Fix:** Move `validate()` into `IrcAdapter::new` and `from_config_bytes`
so it's a one-time check at construction. `ensure_connected` then
only does the spawn-fast-path.

---

### N8 — MEDIUM: `eprintln!` in `irc_session` instead of `tracing::error!`

**File:** `crates/octo-adapter-irc/src/lib.rs:551, 555, 713`

Three `eprintln!` calls in the listener task:
- line 551: `eprintln!("IRC session error: {e}");`
- line 555: `eprintln!("IRC connect error: {e}");`
- line 713: `eprintln!("IRC inbound channel closed; listener exiting");`

Every other adapter in this repo uses `tracing::{error, warn, info}`.
`eprintln!` bypasses the structured logging pipeline and pollutes
stdout/stderr in production. It also can't be silenced by env-var
log-level configuration.

**Fix:** Replace with `tracing::error!` / `tracing::warn!`. The
`tracing` crate is already a workspace dependency (used by WhatsApp).

---

### N9 — LOW: `IrcConfig::validate` doesn't check `server` is a syntactically valid hostname

**File:** `crates/octo-adapter-irc/src/lib.rs:91-127`

The validate function checks that `server` is non-empty after trim,
but not that it parses as a hostname. So
`server = "::::not a host::::"` would pass `validate()` and only
fail at TCP-connect time. Not a security bug — the server is the
operator's config — but the validate function's docstring says
"Pure field-shape validation" and a "field-shape" check on a hostname
string should at least disallow spaces and `/`.

**Fix:** Add a check that `server` doesn't contain whitespace, `/`, or
control chars. (Strict RFC-1123 validation is overkill; just reject
the obviously-malformed cases.)

---

### N10 — LOW: `runtime_groups` (WhatsApp) — same C4 footgun, parallel concern

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs` (R1 didn't flag this; let me check)

R1's review called out the WhatsApp `runtime_groups` as the model
that IRC should mirror, but didn't independently audit it. While
reading R23b I noticed:

- `self.runtime_groups` (likely a field) is referenced in the WhatsApp
  adapter.
- Need to verify whether `join_by_invite` (which returns Unimplemented)
  populates it. If it does, that's wasted work; if it doesn't, the
  same N1 pattern applies in miniature.

This is out of scope for the IRC-focused R23b review and is therefore
flagged as a **non-IRC** follow-up to verify in a future WhatsApp
review.

---

### N11 — LOW: `shutdown` in `PlatformAdapter::shutdown` is a no-op for IRC

**File:** `crates/octo-adapter-irc/src/lib.rs:900-902`

```rust
async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
    Ok(())
}
```

The shutdown does nothing. Combined with N3, this means the listener
task outlives the adapter in the FFI path. Even without FFI, a
graceful shutdown should at least signal the listener.

**Fix:** Tied to N3 — implementing the stop signal fixes both.

---

### N12 — LOW: `IrcWriter::write_line` has duplicated match arms

**File:** `crates/octo-adapter-irc/src/lib.rs:443-457`

```rust
match self {
    IrcWriter::Plain(w) => {
        w.write_all(line.as_bytes()).await?;
        w.write_all(b"\r\n").await?;
    }
    IrcWriter::Tls(w) => {
        w.write_all(line.as_bytes()).await?;
        w.write_all(b"\r\n").await?;
    }
}
```

Both arms do exactly the same thing. The `IrcWriter` enum has only
the type tag; `write_all` works on both inner types because both are
`AsyncWrite`. A single `_ =>` arm or a trait-object/impl-bounded
helper would do.

**Fix:** Either collapse the match (`match self { _ => { ... } }`) or
extract a small helper that uses `Pin<Box<dyn AsyncWrite + Send>>`
behind a `&mut`. Keeping the typed enum is fine; the duplication
just needs a `_ =>` arm.

---

### N13 — LOW: `IrcConfig::validate`'s `nickname.contains(char::is_whitespace)` matches all Unicode whitespace

**File:** `crates/octo-adapter-irc/src/lib.rs:101`

IRC nicknames can't contain any whitespace by RFC 2812. The check
`nickname.contains(char::is_whitespace)` correctly rejects ASCII
space/tab/newline but also rejects e.g. NBSP (`\u{00A0}`), which an
IRC server might or might not handle consistently.

**Status:** Conservative (rejects more than RFC requires) — that's
fine for safety.

**Action:** Leave as-is; add a test that documents the choice.

---

## Severity legend (same as R1)

- **CRITICAL** — wrong behavior, security/correctness bug, would cause data loss
  or silent failure in production.
- **HIGH** — clear footgun, design flaw, capability-report lie, or missing
  defensive check that the next refactor will break.
- **MEDIUM** — code smell, test gap, or doc inconsistency that the next reader
  will trip on.
- **LOW** — nits, style.

---

## Summary

| Severity | Count | New this round |
|---|---|---|
| CRITICAL | 1  | N1 |
| HIGH     | 3  | N2, N3, N4 |
| MEDIUM   | 4  | N5, N6, N7, N8 |
| LOW      | 5  | N9, N10, N11, N12, N13 |
| **Total** | **13** | — |

The single most important finding is **N1** — the C4 fix from R1 is
broken in a subtle way that the existing tests don't catch. Without
fixing N1, `join_by_invite` is functionally a no-op (the server gets
the JOIN but the adapter state never reflects it), and
`list_own_groups` is still a lie.

The next-most-important are **N3** (use-after-free / dangling
listener in the FFI path) and **N4** (PING timeouts under backpressure).

**Next step:** Fix every CRITICAL and HIGH finding (N1, N2, N3, N4)
in R23d. Re-run this audit (R23e) to verify the fixes and look for
newly-introduced regressions. Close the loop when a round finds
nothing.