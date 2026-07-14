# Plan — Chrome reconnect observer (Phase 7.J)

**Date**: 2026-07-14
**Binary**: `whatsapp_chrome_reconnect_observer` (new crate, commit `ff06a09e`)
**Goal**: positive control for the 401 LoggedOut reconnect bug. Show what a real logged-in Chrome session does during connect + reconnect, so we can mirror it from our daemon.

## Why

Our daemon's reconnect path 401 LoggedOuts. We have two competing theories:
1. **wacore bug** — our handshake code drifts from what Chrome sends; wacore's reconnect logic is wrong.
2. **TLS fingerprint gap** — even a perfect wire-shape replica gets rejected because our rustls+ring ClientHello lacks 6 critical extensions (GREASE, encrypted_client_hello, etc).

Both theories are observable from Chrome. This binary is the observation tool — not a fix.

## What it does

Spawns `google-chrome --headless=new --incognito --remote-debugging-port=9224` against a fresh `/tmp/wa-observer-<ts>` profile, navigates to `web.whatsapp.com`, captures **Phase 1 (initial login)**, then runs a **Phase 2 (close+reopen reconnect drill)**:

```
[Phase 1 — initial login, 90s default]
  - CDP: Network.enable, Page.enable, Storage.enable
  - CDP: Page.navigate https://web.whatsapp.com
  - Capture: Network.webSocketCreated (the WA WS endpoint)
  - Capture: Network.webSocketFrameSent/Received (full base64)
  - Capture: Network.cookiesAdded/cookieChanged (cookie jar)
  - Capture: Network.requestWillBeSent (UA, Sec-CH-UA)
  - Output: initial.jsonl (NDJSON per event)

[Phase 2 — reconnect drill, 60s default]
  - CDP: Target.closeTarget on initial tab
  - CDP: Target.createTarget + Page.navigate https://web.whatsapp.com
  - Capture: same event set as Phase 1
  - Output: reconnect.jsonl (NDJSON per event)

[Summary]
  - Both phases: WS endpoint, frames sent/recv, cookies pre/nav,
                 first-frame hex heads (decoded from base64)
  - Output: summary.txt (human readable)
```

## Outputs

```
/tmp/wa-observer/run-<ts>/
├── chrome-profile/          # Chrome's profile dir (ephemeral)
├── initial.jsonl            # Phase 1 events
├── reconnect.jsonl          # Phase 2 events
└── summary.txt              # human summary
```

NDJSON event shape:
```json
{
  "ts": "2026-07-14T12:34:56.789Z",
  "phase": "initial" | "reconnect",
  "method": "Network.webSocketFrameSent",
  "summary": "sent frame b64=144B decoded=108B",
  "params": { ...full CDP params... },
  "payload_head_hex": "561341..."
}
```

`payload_head_hex` is the first 48 decoded bytes of the frame, hex-encoded — enough to spot the WA WS envelope magic (`V\x13A\x03\x02\x00`) and the start of any Noise XX HandshakeInit.

## Run

```bash
cargo run -p whatsapp_chrome_reconnect_observer --release -- \
      --login-window 120 \
      --reconnect-window 60 \
      --log-dir /tmp/wa-observer
```

Default behaviour: 90s login + 60s reconnect, port 9224, profile at `/tmp/wa-observer-<ts>`.

Operator scans QR with the phone paired to the account that the daemon's `default.session.db` was registered to. After login settles (~5s), the binary auto-runs the reconnect drill.

## What we'll learn (running theory → predicted observation)

| Theory | Predicted observation |
|---|---|
| WS endpoint flips on reconnect | `:443/ws/chat` initially → `:5222/ws/chat` on reconnect, or vice versa |
| Noise pattern flips | XX initially (fresh keys) → IK on reconnect (cached cert chain) |
| AppState sync only on reconnect | initial = no AppState attributes; reconnect = IQ handshake w/ all attributes |
| Same endpoint + same pattern + same frame count | neither theory wins; bug is elsewhere (e.g. wacore's retry loop) |

Compare `initial.jsonl` and `reconnect.jsonl` line-by-line: any divergence between the two is a candidate bug surface for our daemon's reconnect path.

## Why this binary is a new crate

Per operator instruction 2026-07-14: "we are not touching any current binary... for binary I mean a brand new investigation binary just like we did".

- No `octo-*` deps → doesn't entangle feature unification graph
- No `wacore` dep → doesn't pull in the heavy Noise/transport stack for an observation-only tool
- Standalone workspace member → can be deleted without breaking anything else

## After run: comparison script (future)

A follow-up binary `whatsapp_reconnect_diff` could diff the two NDJSON files on:
- WS endpoint match/mismatch
- frame count delta
- first-frame hex prefix overlap (signal: same envelope structure vs different)
- cookie set delta (signal: any cookie present on reconnect that wasn't initially)

Out of scope for this binary — keep scope tight.

## Verification

- `cargo build -p whatsapp_chrome_reconnect_observer` ✅ (compile clean)
- `cargo clippy -p whatsapp_chrome_reconnect_observer --all-targets -- -D warnings` ✅
- `cargo fmt -p whatsapp_chrome_reconnect_observer -- --check` ✅
- `cargo run -p whatsapp_chrome_reconnect_observer -- --help` ✅ (clap works)
- Live run deferred — needs operator + real Chrome + real phone pair + live session.

## Local-only / no push

Per operator instruction 2026-07-05: no `git push`, no PR. Branch `feat/whatsapp-runtime-cli-mcp` only.