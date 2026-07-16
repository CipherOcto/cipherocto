# Pin bump at 551e574 — 401 LoggedOut STILL fires (2026-07-14)

## TL;DR

Bumping wacore from `e32b51a` to `551e574` (commit `ef785c86`) does **NOT** fix the 401 LoggedOut reconnect bug. The new commit has the Phase 7.J.3 observability patch but does not contain the modern handshake shape we hypothesized was missing.

The new finding is more decisive: **wacore tries Noise IK (resumeNoiseHandshake), Chrome tries Noise XX on every reconnect. Server 401s wacore's IK before we even get a server-hello to fall back from.**

## What we tested

After bumping all 6 wacore/whatsapp-rust deps from `e32b51a` → `551e574` and adapting the 2 `client.core_device()` → `client.persistence_manager().get_device_snapshot()` API changes in `adapter.rs:1416,1445`:

1. Build clean (`cargo build -p octo-whatsapp --release --features query,tracing-stdout`).
2. Build clean (`cargo build -p octo-whatsapp` without query, avoids Tantivy boot stall).
3. Live run daemon → captured `noise_identity_fp` + 401 trace.

## Live daemon run traces (debug build, without query feature)

```
17:44:59.195  [socket] resumeNoiseHandshake started
17:44:59.196  [socket] resumeNoiseHandshake send hello
17:44:59.198  Sending edge routing pre-intro for optimized reconnection
17:44:59.476  [socket] resumeNoiseHandshake rcv hello
17:44:59.481  [socket] resumeNoiseHandshake deriving secrets
17:45:03.199  Got LoggedOut connect failure, logging out:
                <failure reason="401" location="frc"/>
17:45:03.200  WhatsApp Web logged out
                noise_identity_fp=a3a7ef798e19eda9
                noise_identity_fp_full=a3a7ef798e19eda97db4133042f5bb7bc1fc79fae8b9638cfb8bdc67ce537eb1
                registration_id=1623825540
                reason=LoggedOut
                on_connect=true
```

The 4-second gap between `deriving secrets` (17:44:59.481) and `LoggedOut connect failure` (17:45:03.199) is the Noise IK ClientHello → server reject handshake cycle. Server gives us 401 back without sending a valid Noise server-hello.

## Why IK is wrong here

`src/handshake.rs:159-200` wacore's `do_handshake` selects:
- `HandshakePattern::Ik(server_static_pub)` if `device.server_cert_chain` has a cert
- `HandshakePattern::Xx` otherwise

`select_pattern` (in the upstream) reads our `device.server_cert_chain` blob (the JSON-encoded one we reverse-engineered in `whatsapp_connect_trace`). When it has valid bytes, IK is picked.

`run_ik_handshake` does:
1. Send IK ClientHello (with pre-computed key)
2. Read server-hello
3. Match on `IkServerHelloOutcome::Continue` (encrypted) vs `Fallback` (XX fallback if serverStaticCiphertext non-null)

In our run, server returned **401**, not a Noise server-hello. `ik.read_server_hello(&resp_frame)` fails with a crypto-fatal error. We never reach the Fallback branch. Bot transitions to LoggedOut.

## What Chrome does differently

Chrome's reconnect on tab-close-and-reopen uses **full Noise XX** every time (verified by `whatsapp_chrome_reconnect_observer`, all 8 frames starting with the standard `WA\x06\x03\x00\x00\x24\x12\x22\x0a\x20` opener). Chrome's IndexedDB cache of the server cert chain is **not used** for this code path — fresh XX handshake always.

## What's wrong with our cached server cert chain

`whatsapp_connect_trace` (Phase 7.J, commit `d568625f`) verified:
- our `server_cert_chain` blob has `not_after = 2026-08-09`
- it's not expired

So the cert isn't stale (still ~4 weeks of validity). But the server 401s our IK ClientHello. Possibilities:
1. WA server's **IK path requires additional fields** that wacore's 551e574 doesn't emit (e.g., post-quantum noise attachments, AppState handshake attrs in IK ClientHello)
2. WA server's **noise-protocol version** for IK has been bumped since our cert was cached; server only accepts the modern IK
3. Server **doesn't permit IK at all anymore** for already-paired sessions (Chrome's behavior of fresh-XX suggests this)

The most likely: (3) — the WA server has dropped IK support for already-paired sessions, and Chrome has been quietly updated to always-XX. wacore's `select_pattern` IK-first logic is simply wrong for current WA servers.

## The fix

**Force wacore to use XX (not IK) on reconnect.** Two implementation paths:

### Path A — clear the cached cert chain at startup
Drop the `server_cert_chain` blob from `device` before `do_handshake` runs. Forces `select_pattern` to return `Xx` (the cache-empty branch). One-line code change, no upstream patch needed.

Find: `select_pattern` source:

```
.../whatsapp-rust-551e574/wacore/src/handshake.rs (or similar)
```

Change: clear `device.server_cert_chain` before the first `do_handshake` call, OR make `select_pattern` always return `Xx` for already-paired sessions (matching Chrome's behavior).

### Path B — patch upstream `select_pattern`
The fork `mmacedoeu/whatsapp-rust` already exists. Add a patch that makes `select_pattern` skip IK when the cert would fail (test pre-handshake with a small XX probe, etc). Larger change, harder to verify locally.

Recommended: **Path A** — surgical, easy to revert, only affects our adapter's connect behavior.

## What this rules out

| Theory | Verdict |
|---|---|
| Modern handshake shape missing (frame[2] size gap) | **NOT THE ROOT CAUSE** — pin bump didn't change IK↔XX; the gap is irrelevant because we're not even using XX |
| WA server's IK path accepts modern extensions | **DOES NOT** — server 401s our IK with the SAME size as before |
| TLS fingerprint | RULED OUT earlier (`xx_session_probe`) |
| Server cert chain stale (IK) | RULED OUT (`connect_trace` — valid until 2026-08-09) |

The **only** remaining culprit: wacore tries IK, server has dropped or modified IK, must use XX like Chrome does.

## Next action

Patch `do_handshake` (or the place `select_pattern` is called) to **always return `Xx`** for already-paired sessions. Equivalent to clearing the cert chain pre-handshake. Apply in our adapter so we don't touch the upstream fork.

## Files referenced

- `crates/octo-adapter-whatsapp/src/adapter.rs` — where to apply the IK-bypass patch
- `src/handshake.rs` (upstream) at `551e574` — the `select_pattern` IK-first logic
- `docs/research/2026-07-14-chrome-reconnect-handshake.md` — confirms Chrome always uses XX
- `docs/research/2026-07-14-xx-handshake-server-accept.md` — server accepts our XX opener
- `docs/research/2026-07-14-frame2-size-gap.md` — the size-gap theory that turned out to be a side effect

## Local-only / no push

Per operator instruction 2026-07-05. Branch `feat/whatsapp-runtime-cli-mcp` only.
