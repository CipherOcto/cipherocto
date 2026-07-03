# Adversarial Review: CoordinatorAdmin Implementation (R20 + R21)

**Date:** 2026-06-18
**Reviewer:** Adversarial review (Round 1 of N; loop ends when a round finds nothing)
**Scope:** All changes from `52c5db1..d658d16` that touch the `CoordinatorAdmin`
trait, the `PlatformAdapter` bridge, and the WhatsApp / IRC platform implementations.
**Method:** Read the in-scope files end-to-end; cross-reference trait contract vs.
implementations; trace the admin path through socket I/O; spot-check error propagation,
concurrency, capability report honesty, and CLI/security/IRC-protocol hazards.

---

## Files in scope

| File | Lines | Description |
|---|---|---|
| `crates/octo-network/src/dot/adapters/coordinator_admin.rs` | 797 | New `CoordinatorAdmin` trait + types + tests |
| `crates/octo-network/src/dot/adapters/mod.rs` | 181 | `PlatformAdapter::as_coordinator_admin` bridge |
| `crates/octo-network/src/dot/error.rs` | 83 | New `PlatformAdapterError::Unimplemented` variant |
| `crates/octo-network/src/dot/mod.rs` | 323 | Re-exports of new admin types |
| `crates/octo-adapter-whatsapp/src/adapter.rs` | 2616 | WhatsApp `CoordinatorAdmin` impl + inherent group-setup API |
| `crates/octo-adapter-irc/src/lib.rs` | 1857 | IRC `CoordinatorAdmin` impl + admin channel plumbing |
| `docs/research/coordinator-admin-actions.md` | 458 | Design rationale doc (R19-era) |

---

## Severity legend

- **CRITICAL** — wrong behavior, security/correctness bug, would cause data loss
  or silent failure in production.
- **HIGH** — clear footgun, design flaw, capability-report lie, or missing
  defensive check that the next refactor will break.
- **MEDIUM** — code smell, test gap, or doc inconsistency that the next reader
  will trip on.
- **LOW** — nits, style.

---

## CRITICAL findings

### C1 — IRC `connect_tls` is a no-op

**File:** `crates/octo-adapter-irc/src/lib.rs:361-365`

```rust
async fn connect_tls(server: &str, port: u16) -> Result<TcpStream, String> {
    // For simplicity, use plain TCP with a note that TLS should be added
    // In production, use tokio-rustls with a proper TLS configuration
    connect_plain(server, port).await
}
```

The function name lies. It says "tls" but it returns a plain `TcpStream`. So the
`IrcConfig::use_tls = true` default (port 6697) silently downgrades to plaintext
when the bot connects. The IRC server is listening for TLS at 6697, so a
plaintext connect will either be rejected immediately or, worse, succeed at the
TCP level and the IRC server will wait forever for the TLS handshake that
never comes. The session will time out silently.

**Evidence the bug is real:**
- `IrcConfig::use_tls` defaults to `true` (line 68-70).
- `IrcConfig::port` defaults to `6697` (line 65-67) — the TLS port.
- `ensure_connected` (line 130-160) calls `connect_tls` when `use_tls = true`.
- `connect_tls` ignores `use_tls` and calls `connect_plain`.
- `health_check` (line 651-660) also doesn't use TLS.

**Why existing tests don't catch it:** The IRC tests in
`test_send_raw_line_writes_through_listener` and
`test_coordinator_admin_kick_writes_correct_line` all set
`use_tls: false`, so the no-op branch isn't exercised.

**Impact:** Production deployments that ship the default config will either
fail to connect or send credentials in cleartext. The whole IRC adapter is
non-functional for any operator who trusts the `use_tls: true` default.

**Fix:** Either implement TLS (tokio-rustls is the standard choice in this
workspace's other adapters, e.g. `octo-adapter-telegram`) or flip the default
to `use_tls: false` and update the config docs to say "TLS not yet supported".

---

### C2 — IRC `send_message` does not actually write to the wire

**File:** `crates/octo-adapter-irc/src/lib.rs:540-585`

The `send_message` impl builds a string of bytes that *would* be written:

```rust
let mut sent_bytes = Vec::new();
for (i, chunk) in chunks.iter().enumerate() {
    let line = if total > 1 {
        Self::encode_fragment(i as u16, total, chunk.as_bytes())
    } else {
        chunk.clone()
    };
    let irc_msg = format!("PRIVMSG {} :{}\r\n", channel, line);
    sent_bytes.extend_from_slice(irc_msg.as_bytes());
}

Ok(DeliveryReceipt {
    platform_message_id: format!("irc-{}", epoch_millis()),
    delivered_at: epoch_millis(),
})
```

…but `sent_bytes` is never used. The `OwnedWriteHalf` lives in the listener
task and the adapter has no handle to it. The comment at line 565-567 admits
this: *"For now, return the encoded envelope as a 'send instruction'. In
production, this would write to the IRC socket"*.

This is a stub that returns a fake `DeliveryReceipt` with a `platform_message_id`
of `epoch_millis()` (line 582). The gateway will believe the message was
delivered; the IRC server never sees it. Consensus operations built on this
will silently no-op.

**Impact:** Every outbound DOT envelope on the IRC adapter is lost.

**Fix:** Add a `send_tx: mpsc::Sender<String>` to the adapter (mirroring the
admin `cmd_tx` pattern), install it in `ensure_connected`, and use it in
`send_message`. Then add a test that asserts the bytes hit the local TCP
listener the same way `test_send_raw_line_writes_through_listener` does for
admin commands. Bonus: write a `DeliveryReceipt` only after the line is
enqueued (currently fine — but make sure the API doesn't claim a server-confirmed
ID).

---

### C3 — IRC `send_message` does not call `ensure_connected`

**File:** `crates/octo-adapter-irc/src/lib.rs:540-585`

Compounding C2: `send_message` does NOT call `ensure_connected`, so the
listener task is never spawned, the admin `cmd_tx` is `None`, and the message
goes nowhere. Compare WhatsApp's pattern (which gates on `self.client` being
populated) and IRC's own `send_raw_line` (which calls `ensure_connected` at
line 174).

A subsequent call to `receive_messages` (which DOES call `ensure_connected`,
line 591) would spawn the listener — but the message sent *before* any receive
is lost, and the first send returns Ok-without-sending regardless.

**Fix:** Add `self.ensure_connected().await?;` as the first line of
`send_message`, before the channel lookup. (Same prerequisite as
`send_raw_line`.)

---

### C4 — IRC capability report / `list_own_groups` / `join_by_invite` are inconsistent

**File:** `crates/octo-adapter-irc/src/lib.rs:707-740, 965-1040`

The capability report claims:

```rust
can_join_by_id: false,       // bot's channels are pre-configured
can_join_by_invite: true,    // JOIN #channel (best-effort)
can_list_own_groups: true,   // configured channels
```

But `join_by_invite` (line 1025-1038) calls `self.send_raw_line("JOIN ...")` and
returns a synthetic `GroupHandle` — and crucially does NOT update
`self.config.channels` to include the newly-joined channel. So:

- The bot is now a member of the channel on the server.
- `list_own_groups` (line 965-981) returns only the statically configured
  channels, not the runtime-joined ones.
- The `can_list_own_groups: true` capability is misleading because the *true*
  set of groups the bot is in (configured + runtime-joined) is invisible to
  the API.

**Impact:** Callers that use `join_by_invite` + `list_own_groups` will see an
incomplete group inventory. The capability report contradicts the
implementation.

**Fix:** Add a `runtime_channels: Arc<Mutex<Vec<String>>>` field to `IrcAdapter`
(mirroring the WhatsApp `runtime_groups` pattern), populate it from
`join_by_invite`, and merge it in `list_own_groups`. Update the `channel_for`
helper to also accept runtime-joined channels.

---

## HIGH findings

### H1 — WhatsApp `can_join_by_invite: true` but `join_by_invite` is Unimplemented

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1381, 1728-1742`

```rust
can_join_by_invite: true, // `join_with_invite_code` exists in whatsapp-rust
```

```rust
async fn join_by_invite(
    &self,
    _invite: &InviteRef,
) -> Result<GroupHandle, PlatformAdapterError> {
    Err(PlatformAdapterError::Unimplemented { ... })
}
```

The capability report comment (`join_with_invite_code exists in whatsapp-rust`)
admits the underlying SDK call exists. The capability is set to `true`. But the
method is `Unimplemented`. The trait's documented contract is "capability
report must truthfully reflect which methods the adapter overrides"
(`coordinator_admin.rs:351-356`). This is the **aspirational** vs
**truthful** split — the bit is set but the method does nothing.

**Impact:** Callers checking `caps.can_join_by_invite` and then calling
`join_by_invite` will get an `Unimplemented` error at runtime. The capability
report contract is broken.

**Fix:** Either:
1. Implement `join_by_invite` by calling `client.groups().join_with_invite_code(...)`,
   **or**
2. Set `can_join_by_invite: false` in the WhatsApp capability report.

---

### H2 — WhatsApp `create_group` trait impl relies on fragile signature disambiguation

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1411-1427`

```rust
async fn create_group(
    &self,
    subject: &str,
    initial_members: &[GroupMemberSpec],
) -> Result<GroupHandle, PlatformAdapterError> {
    let phones: Vec<&str> = initial_members.iter().map(|m| m.handle.as_str()).collect();
    let output = self.create_group(subject, &phones).await ...   // <-- inherent
```

The trait `create_group` takes `&[GroupMemberSpec]` and the inherent
`create_group` (line 652-712) takes `&[&str]`. The trait impl calls
`self.create_group(subject, &phones)` where `phones: Vec<&str>`, so `&phones`
matches the inherent signature. Rust's method resolution picks the inherent
over the trait in this case (a known but easy-to-miss rule).

The risk: if anyone adds an `impl From<&[GroupMemberSpec]> for &[&str]` (or
similar) — or if the inherent's signature is refactored to accept
`&[impl AsRef<str>]` — the call would silently resolve to the *trait*
`create_group`, which would infinite-recurse. The `leave_group` precedent in
the same file (line 1767-1796) shows the maintainers already know about this:
they renamed the inherent to `leave_group_str` to avoid the collision.

**Impact:** A future refactor that loosens the inherent's signature would
introduce a runtime infinite loop, and the existing tests (which all run
without a connected client) wouldn't catch it because the early `client.lock()`
failure short-circuits.

**Fix:** Apply the same pattern as `leave_group_str`: rename the inherent
`create_group` to `create_group_str` (or `create_group_impl`) and update all
callers. The trait impl then calls the unambiguous name. This is mechanical
and removes the foot-gun.

---

### H3 — IRC `join_by_invite` doesn't validate the channel name

**File:** `crates/octo-adapter-irc/src/lib.rs:1025-1038`

```rust
async fn join_by_invite(
    &self,
    invite: &InviteRef,
) -> Result<GroupHandle, PlatformAdapterError> {
    self.send_raw_line(&format!("JOIN {}", invite.0)).await?;
    ...
}
```

IRC channel names must start with `#`, `&`, `+`, or `!` and must not contain
spaces, commas, colons, or NUL. The current code passes `invite.0` straight
to `JOIN`. Worse, IRC's special token `0` (`JOIN 0`) makes the client PART all
channels. So a `JoinByInvite(InviteRef::new("0"))` would silently disconnect
the bot from every channel it's in.

The research doc at `coordinator-admin-actions.md:99` calls out "Join via
invite code: IRC `JOIN #chan`" — but doesn't mention the validation gap.

**Impact:** A malicious or malformed `InviteRef` can dismember the bot's
channel set. Even benign typos (`InviteRef::new("#cipherocto ")` with a
trailing space) would make the JOIN fail server-side.

**Fix:** Validate `invite.0`:
- Must be non-empty
- Must start with `#`, `&`, `+`, or `!`
- Must not contain CR, LF, NUL, space, comma, colon

Return `PlatformAdapterError::ApiError` with a clear message on violation
rather than letting the server reply with a numeric.

Bonus: reject `0` explicitly with a message like "use `leave_group` to leave
all channels".

---

### H4 — IRC `send_raw_line` does not reject CRLF in the line

**File:** `crates/octo-adapter-irc/src/lib.rs:173-184`

```rust
async fn send_raw_line(&self, line: &str) -> Result<(), PlatformAdapterError> {
    self.ensure_connected().await?;
    ...
    tx.send(line.to_string()).await ...;
    ...
}
```

The `line` is sent verbatim to the server. If a `format!` in any caller
produces a `&str` containing `\r\n` (e.g. a future `KICK #ch user :reason with
\r\nNICK pwned` — sloppier, but possible), the line is split server-side.
Most IRC servers reject this at the protocol level with a `ERR_UNKNOWNERROR`
or simply ignore the second line, but a strict IRC server could be tricked.

Defense-in-depth: validate at the adapter boundary.

**Fix:** Add a precondition:
```rust
if line.contains('\r') || line.contains('\n') || line.contains('\0') {
    return Err(PlatformAdapterError::ApiError {
        code: 400,
        message: format!("admin line contains illegal byte: {line:?}"),
    });
}
```

Apply the same check inside `irc_session` as a belt-and-suspenders guard.

---

### H5 — IRC listener task has no watchdog: panic ⇒ silent adapter death

**File:** `crates/octo-adapter-irc/src/lib.rs:130-160, 308-353`

`ensure_connected` spawns the listener once and sets `connected = true`. The
listener has an outer `loop` for TCP reconnect, but if the *task itself*
panics or is cancelled (e.g. by `tokio::spawn` runtime shutdown, or a
malformed line that triggers a buggy parser), the task is gone forever.
Future calls to `ensure_connected` see `connected = true` and return
immediately. `receive_messages` returns an empty `Vec` forever; `send_raw_line`
returns "admin channel closed".

**Impact:** One unhandled panic and the adapter becomes a black hole.

**Fix:** Wrap the listener spawn in a watchdog:
```rust
let cmd_tx_clone = cmd_tx.clone();
let adapter_weak = ... // need a back-reference
tokio::spawn(async move {
    irc_listener(...).await;
    // If the listener exits cleanly it's because all senders were dropped.
    // If it exits with an error, the adapter should reset its state so
    // ensure_connected respawns.
});
```
or: add a `tokio::select!` outside the listener that respawns it if the
listener returns. Either way, reset `connected = false` on listener exit
(except for the clean-shutdown case where the adapter itself is dropping).

---

### H6 — WhatsApp `add_member` partial-success surfaces as full error

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1495-1522`

```rust
async fn add_member(...) -> Result<(), PlatformAdapterError> {
    let responses = self.add_members(...).await.map_err(...)?;
    if let Some(r) = responses.first() {
        if !r.is_ok() {
            return Err(PlatformAdapterError::ApiError { ... });  // full error
        }
    }
    if member.is_admin {
        self.promote_to_admin(group_id, &PeerId::new(member.handle.clone())).await?;  // ?
    }
    Ok(())
}
```

Two issues:
1. The error returned when the per-participant response is non-OK is the
   inner `r.error` (or a generic message), with code 500. The caller can't
   distinguish "add was rejected by server" from "transport failed".
2. The `promote_to_admin` `?` propagates the promote error, but the add
   already succeeded. The caller can't retry just the promote.

**Fix:** Either (a) document the "partial success" semantics in the trait
docs and add a structured `AddMemberOutput { added: bool, promoted: Result<...> }`,
or (b) atomically: best-effort promote, return Ok(()) if add succeeded even
if promote failed, and log the promote failure. The current code mixes both
styles.

---

### H7 — `IrcConfig::validate` is missing

**File:** `crates/octo-adapter-irc/src/lib.rs:47-70`

`WhatsAppConfig::validate()` exists (line 97-121 in whatsapp adapter); the IRC
config has no equivalent. So an operator can configure:
- `channels: vec!["".into()]` — empty channel name
- `channels: vec!["no-hash-prefix".into()]` — invalid IRC channel
- `nickname: "".into()` — empty nick (server will reject; the failure is
  silent)
- `server: "".into()` — empty server (TCP connect will fail)

The `from_config_bytes` constructor at line 123-127 deserializes without
validation, then `new()` stores the bad config. The first `ensure_connected`
will fail with an opaque error string.

**Fix:** Add an `IrcConfig::validate()` method (modeled on WhatsApp's), call
it from `from_config_bytes`, and return a structured error on failure.

---

## MEDIUM findings

### M1 — WhatsApp `set_ephemeral` truncates u64 seconds to u32 silently

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1645-1655`

```rust
let secs = ttl.map(|d| d.as_secs() as u32).unwrap_or(0);
```

`Duration::as_secs() as u32` truncates if the duration exceeds `u32::MAX`
seconds (~136 years). The trait method is `set_ephemeral(Option<Duration>)`,
so a caller could legitimately pass a `Duration::from_secs(MAX)` and expect
either "set max" or a structured error. The current code silently rounds
down. WhatsApp's actual max is 7 days, but the trait doesn't say that.

**Fix:** Document the contract on the trait ("adapters may clamp `ttl` to a
platform-specific maximum; callers should not assume `as u32` precision"), or
return `ApiError { code: 400, message: "ttl exceeds platform max" }` when the
duration overflows the adapter's limit.

---

### M2 — `GroupId::new` and `PeerId::new` accept empty strings

**File:** `crates/octo-network/src/dot/adapters/coordinator_admin.rs:80-82, 116-118`

Both constructors are `pub fn new(impl Into<String>) -> Self` with no
validation. `GroupId::new("")` produces an empty `GroupId`; downstream code
in IRC's `channel_for` (line 1075-1098) would treat it as a "bare channel"
(see the comment at line 1078) and try to find it in `config.channels` —
which never matches an empty string. The error would be opaque.

`PeerId::new("")` would produce an empty handle. IRC's `INVITE {} #chan`
would fail server-side silently (and the test at line 1761 doesn't exercise
this path).

**Fix:** Add a precondition to both constructors (or expose a
`try_new` variant) that rejects empty strings:
```rust
pub fn new(handle: impl Into<String>) -> Self {
    let s: String = handle.into();
    debug_assert!(!s.is_empty(), "GroupId handle must not be empty");
    Self(s)
}
```
The `debug_assert!` is cheap and fires loudly in tests. Production code can
add a `try_new` for hard validation if needed.

---

### M3 — IRC `health_check` ignores `use_tls`

**File:** `crates/octo-adapter-irc/src/lib.rs:651-660`

```rust
async fn health_check(&self) -> Result<(), PlatformAdapterError> {
    let timeout = std::time::Duration::from_secs(5);
    let addr = format!("{}:{}", self.config.server, self.config.port);
    match tokio::time::timeout(timeout, TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => Err(transport_err(format!("Health check: {e}"))),
        Err(_) => Err(transport_err("Health check timed out")),
    }
}
```

The health check does a plain TCP `connect`, not a TLS handshake, even when
`use_tls = true`. So the check passes even if TLS is broken. Same root cause
as C1: the adapter doesn't actually do TLS.

**Fix:** Once C1 is fixed (real TLS), update `health_check` to attempt the
TLS handshake (or at least validate the cert chain) when `use_tls = true`.

---

### M4 — WhatsApp `create_group` initial-admin promotion failure is silent

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1434-1450`

```rust
if let Err(e) = self.promote_participants(&output.group_jid, &to_promote).await {
    tracing::warn!(
        group_jid = %output.group_jid,
        error = %e,
        "failed to promote initial admins on create; caller should retry"
    );
}
```

The `tracing::warn!` is the only signal. The function returns
`Ok(GroupHandle { is_admin: true, ... })` regardless. The caller has no
structured way to know the initial admins weren't promoted.

The `is_admin: true` claim in the returned handle is true *for the bot*
(the bot is always admin after `create_group`) but misleading about the
*initial members* — the user might reasonably interpret `is_admin` as
"all members listed in the create spec are admins".

**Fix:** Add an `initial_admins_promoted: bool` (or richer result struct) to
`GroupHandle` so the caller can detect partial state. Or document the
"bot is admin; initial member admin status is best-effort" semantics
explicitly in the trait.

---

### M5 — WhatsApp `get_group_metadata` and `get_invite_link` errors swallowed in `create_group`

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1452-1455`

```rust
let metadata = self.group_metadata(&output.group_jid).await.ok();
let invite_url = self.get_invite_link(&output.group_jid, false).await.ok();
```

Both `.ok()` calls swallow transient errors. The `GroupHandle` will have
`member_count: None, mode_flags: None, invite_url: None` on transient
failure. The caller can't tell apart "platform doesn't supply this" from
"the network is down".

**Fix:** Log the error at `tracing::debug!` (not `warn!`, since it's a
soft-fail path) and continue, but document the semantics in the trait:
"transient failures in metadata fetch degrade the returned handle to a
minimal form; callers that need strong guarantees should call
`get_group_metadata` separately".

---

### M6 — `can_list_own_groups: true` on IRC is misleading

**File:** `crates/octo-adapter-irc/src/lib.rs:707-740, 965-981`

Same as C4: the capability is true but the implementation only returns
statically-configured channels. After `join_by_invite`, the newly-joined
channel is invisible to `list_own_groups`. Either fix the implementation
(see C4) or document "list_own_groups returns only channels in
`config.channels`, not runtime-joined channels".

---

### M7 — IRC `add_member` (INVITE) doesn't require op status

**File:** `crates/octo-adapter-irc/src/lib.rs:784-796`

The `INVITE` command requires the bot to be a channel operator. If the bot
joined via `JOIN` (no op), the server replies with `ERR_CHANOPRIVSNEEDED`
which the listener discards silently. The trait method returns `Ok(())` even
though the invite didn't happen.

**Impact:** A coordinator that assumes `add_member` succeeded will be
confused when the target user says "I never got an invite".

**Fix:** Capture server replies to the most recent command (a small
"pending reply" buffer in the listener keyed by the command timestamp) and
return `ApiError` if the server replies with a non-success numeric. This is
a larger refactor; in the meantime, document the fire-and-forget semantics
prominently in the doc-comment.

---

### M8 — IRC `health_check` doesn't call `ensure_connected`

**File:** `crates/octo-adapter-irc/src/lib.rs:651-660`

The health check validates the TCP path but not the authenticated IRC
session. A bot that has lost its IRC connection (e.g. ping timeout) reports
"healthy" until the next `receive_messages` cycle discovers the empty
inbound channel.

**Fix:** Track the time of the last successful PING/PONG in the listener
and report unhealthy if it's older than `PING_INTERVAL_SECS * 3`. Or
expose a `is_authenticated: AtomicBool` flag set on `RPL_WELCOME` (001) and
cleared on disconnect.

---

### M9 — IRC `receive_messages` silently drops on `tx.try_send` failure

**File:** `crates/octo-adapter-irc/src/lib.rs:482-486`

```rust
let _ = tx.try_send(RawPlatformMessage { ... });
```

`try_send` fails when the channel is full (capacity 4096, but reachable
under burst). The error is silently dropped. For a transport adapter, lost
envelopes = consensus violations.

**Fix:** Switch to `tx.send(...).await` and let the listener backpressure
(but make sure the listener can still process admin commands via `biased;`).
Or use `tokio::sync::mpsc::error::TrySendError` to increment a metric and
log at `warn!` when it happens.

---

### M10 — IRC docs/code mismatch on `can_join_by_id`

**File:** `docs/research/coordinator-admin-actions.md:99`,
`crates/octo-adapter-irc/src/lib.rs:712`

The research doc says IRC can join by ID (`JOIN #channel`). The capability
report says `can_join_by_id: false`. The truth: `JOIN` is the IRC primitive
and it works on a channel name (which is essentially an ID). So the
capability is conservative-but-wrong.

**Fix:** Either flip `can_join_by_id` to `true` (and add a `join_by_id` method
that wraps `join_by_invite` — they're the same IRC op), or update the docs to
say "we model this as `join_by_invite` because the input is a channel name,
not a numeric ID".

---

### M11 — WhatsApp `list_own_groups` does an O(N) lookup per group

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1679-1684`

```rust
let is_admin = meta
    .participants
    .iter()
    .find(|p| p.jid.user == self_phone)
    .map(|p| p.is_admin())
    .unwrap_or(false);
```

For each group, scan all participants. Total cost: O(groups × participants).
For a bot in 100 groups of 50 members each, that's 5000 comparisons on
every `list_own_groups` call. Not a hot path, but worth noting.

**Fix:** Build a `HashSet<String>` of `self_phone`'s possible forms (with /
without country code, with / without `@s.whatsapp.net`) once, before the
iter. Or use `participants.iter().any(|p| p.is_admin() && p.jid.user == self_phone)`.

---

### M12 — `GroupModeFlags` semantics in `set_ephemeral` are platform-defined

**File:** `crates/octo-network/src/dot/adapters/coordinator_admin.rs:536-546`

The trait doc says "`ttl = None` disables ephemeral mode". WhatsApp
implements that (0 = disabled). But the trait doesn't enforce it. IRC
returns `Unimplemented`, which is correct. Telegram TDLib's
`setChatMessageAutoDeleteTime` *also* accepts 0 = disabled. So the current
contract is consistent. But a future adapter might interpret
`ttl = None` as "1 second" (off-by-one). Add a stronger test or stronger
docs to the trait.

**Fix:** Add a doc-comment example: "for `ttl = None`, the adapter must
disable ephemeral mode (equivalent to a TTL of 0)". And add a unit test
in the coordinator_admin tests module that verifies a noop admin's
`set_ephemeral(..., None)` returns `Unimplemented` (covered) **and** that
the docs say "implementations should interpret `None` as disable".

---

### M13 — `invite_url: None` in WhatsApp `list_own_groups` is N+1 query materialization

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1688`

```rust
invite_url: None, // would require a per-group `get_invite_link` call
```

`get_invite_link` does a per-group round trip to the WhatsApp server.
`list_own_groups` returns N groups, so a caller that wants invite URLs
needs to make N additional calls. For a coordinator UI that wants to
display "here are my groups, here's the invite link for each", that's
N+1 round trips.

**Fix:** Add a `list_own_groups_with_invites` method (or an
`include_invite_url: bool` parameter) that materializes the invite URLs in
parallel. Document the N+1 cost in the trait.

---

### M14 — CoordinatorAdmin `GroupHandle.is_admin` is ambiguous for non-create paths

**File:** `crates/octo-network/src/dot/adapters/coordinator_admin.rs:217-236`

The struct's `is_admin` doc says "Whether the calling adapter is the group
admin (true after `create_group`; depends on the invite-link / join path for
`resolve_invite`)". But it's not clear for `list_own_groups` (per-group
membership, not "the calling adapter is admin of *this* group"), or for
`join_by_invite`. WhatsApp sets `is_admin: false` for `resolve_invite`
(line 1722), which matches the doc. IRC sets `is_admin: false` for
`join_by_invite` (line 1036), which is correct (the bot isn't yet a member).

But what does `is_admin: false` mean for a group in `list_own_groups` that
the bot is in but doesn't admin? The current WhatsApp impl correctly
inspects the participant list (line 1679-1684). IRC always returns `false`
(line 976) because it doesn't track op status. Both are correct given the
limits, but a caller that sees `is_admin: false` for a group the bot is
clearly a member of (e.g. via `add_member`) will be confused.

**Fix:** Document the semantic more precisely: "`is_admin: true` means
the calling adapter can perform admin actions (e.g. `set_locked`,
`promote_to_admin`) on this group at this moment. `false` means either
the adapter is not an admin, or the platform doesn't expose admin
status for the bot".

---

### M15 — IRC `channel_for` accepts any string in `config.channels`, including invalid names

**File:** `crates/octo-adapter-irc/src/lib.rs:1075-1098`

If the operator configures `channels: vec!["no-hash-prefix".into()]`, the
`channel_for` helper accepts it (it's in `config.channels`), and any admin
command tries to use it. The IRC server replies with `ERR_NOSUCHCHANNEL`
silently. The bot appears to work but no actions take effect.

**Fix:** Validate channel names in `IrcConfig::validate()` (see H7).
Reject names that don't start with `#`, `&`, `+`, or `!`.

---

### M16 — WhatsApp `group_to_jid` JID parsing not robust to user-tagged groups

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:332-338`

```rust
fn group_to_jid(group_id: &str) -> String {
    if group_id.contains('@') {
        group_id.to_string()
    } else {
        format!("{}@g.us", group_id)
    }
}
```

If the operator misconfigures `groups: vec!["1234@newsletter".into()]`
(newsletter JID suffix), the adapter passes it through verbatim. The
downstream `accept_message` JID-match logic (line 372-375) might
accidentally match it.

**Fix:** Validate JID format in `WhatsAppConfig::validate()`: each `groups`
entry should either be digits-only or end with `@g.us` (after stripping
digits, the suffix must be `@g.us`).

---

## LOW findings

### L1 — IRC `MAX_PAYLOAD_PER_MSG = 480` doesn't account for channel-name length

**File:** `crates/octo-adapter-irc/src/lib.rs:78-81`

```rust
const PRIVMSG_OVERHEAD: usize = 32;
const MAX_PAYLOAD_PER_MSG: usize = IRC_MAX_LINE_BYTES - PRIVMSG_OVERHEAD;
```

The 32-byte overhead assumes a ~20-char channel name. For long channel
names (e.g. `#a-very-long-channel-name-for-a-specific-purpose`),
`PRIVMSG #a-very-long... :<message>` exceeds 512 bytes and gets truncated
by the server.

**Fix:** Compute overhead at send time as `8 (PRIVMSG) + channel.len() + 3 (" :\r\n")`,
or document the limit in the channel name as part of the config.

---

### L2 — IRC `decode_message` is permissive on fragment `i/n:base64`

**File:** `crates/octo-adapter-irc/src/lib.rs:248-260`

The `i/n` header is parsed but `i` and `n` are discarded (`let _header = ...`).
The decoder only returns the base64-decoded bytes. So a fragment with `i=99`
of `n=3` is silently accepted. The reassembly logic in the gateway would
later need to validate `i < n`.

**Fix:** Return `DecodeMessage::Fragment { index, total, bytes }` from
`decode_message` so the caller can validate the header. Backwards-compat:
keep the current `Vec<u8>` return for non-fragment messages.

---

### L3 — Test naming convention: snake_case `test_xxx_yyy`

**File:** various

The test names are mostly consistent (`test_xxx_yyy`) but a few in the
WhatsApp adapter use `xxx_yyy_fails_when_not_connected` which is fine
but mixes the negative-test prefix style. Not a bug, just inconsistent.

**Action:** leave as-is for now; a rename sweep is a separate PR.

---

### L4 — `extract_mode_flags` and `extract_group_metadata` could be methods

**File:** `crates/octo-adapter-whatsapp/src/adapter.rs:1806-1839`

These are free functions in the file. They could be `impl
WhatsAppWebAdapter` methods, but the file already has a "Helpers for
CoordinatorAdmin impl" section header (line 1761) explaining the design.
This is fine.

**Action:** leave as-is.

---

### L5 — IRC `adapter_version() = 1` and `platform_type() = 0x0006` constants are duplicated

**File:** `crates/octo-adapter-irc/src/lib.rs:289-294, 1110-1118`

`PLATFORM_TYPE = 0x0006` and the `platform_type()` ABI export both
hard-code the same value. If someone changes one and forgets the other,
the FFI surface and the Rust constant disagree.

**Fix:** `pub const fn platform_type() -> u16 { Self::PLATFORM_TYPE }` for
the ABI export, single source of truth.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 4 (C1, C2, C3, C4) |
| HIGH     | 7 (H1, H2, H3, H4, H5, H6, H7) |
| MEDIUM   | 16 (M1–M16) |
| LOW      | 5 (L1–L5) |
| **Total** | **32** |

The most important findings are the IRC adapter's broken `send_message`
and `connect_tls` (C1, C2, C3) — these are correctness bugs that would
silently lose every outbound envelope and fail to establish TLS
connections. The capability report lie (H1) and the trait/inherent
disambiguation footgun (H2) on WhatsApp are the next-priority fixes.

**Next step:** Address every CRITICAL and HIGH finding in R23b. Then
re-run this audit (R23c) to check for newly-introduced regressions
and to see whether any MEDIUM finding has matured into a HIGH/CRITICAL.
