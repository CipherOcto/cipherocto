# Chrome reconnect handshake — positive control

**Date**: 2026-07-14
**Run dir**: `/tmp/wa-observer/run-1784043740549/`
**Sources**: `initial.jsonl` + `reconnect.jsonl` (NDJSON, captured by `whatsapp_chrome_reconnect_observer`)

## TL;DR

Chrome's reconnect uses **Noise XX, NOT IK**, even though the cert chain is
cached in WA Web's IndexedDB. Same wire format both phases. The 401 LoggedOut
bug is **not** "we sent IK when Chrome would have sent XX" — Chrome sends XX
every time, on every reconnect.

| Phase | Endpoint | Frames sent/recv | First-frame prefix | Pattern |
|---|---|---|---|---|
| Initial | `wss://web.whatsapp.com:5222/ws/chat` | 8 / 7+ | `57 41 06 03 00 00 24 12 22 0a 20` | Noise XX |
| Reconnect | `wss://web.whatsapp.com:5222/ws/chat` | 6 / 5+ | `57 41 06 03 00 00 24 12 22 0a 20` | Noise XX |

**Same endpoint. Same wire shape. Same Noise pattern.** Different ephemeral
keys + ciphertext (expected).

## Frame structure (deduced from observed payloads)

| idx | dir | len | hex prefix | interpretation |
|---|---|---|---|---|
| 0 | sent | 43B | `57 41 06 03 00 00 24 12 22 0a 20` | WA envelope + `-> e` (ephemeral static pub, 32B) |
| 1 | recv | 350B | `00 01 5b 1a d8 02 0a 20` | `<- e, ee, s, es` (server static pub + cert chain) |
| 2 | sent | 363B | `00 01 68 22 e5 02 0a 30` | `-> s, es` (client static pub + client cert signed by identity) |
| 3 | recv | 698B | `00 02 b7` | `<- payload` (AppState handshake begins) |
| 4 | sent | 37B | `00 00 22` | post-handshake ciphertext ack |
| 5 | sent | 93B | `00 00 5a` | IQ handshake `<handshake>` + `<edge_routing>` + `<read_receipts>` |
| 6 | recv | 66B | `00 00 3f` | IQ handshake response |
| 7+ | s/r 41/47 | `00 00 26` / `00 00 2c` | heartbeat ping/pong (10s interval) |

Notes:
- Frame 0 = WA binary envelope (`V\x13A\x03\x02\x00\x24\x12\x20` — same magic
  as the synthetic envelope `whatsapp_noise_local_capture` already prints).
- Frame 1 protobuf tag `0a 20` = field 1 (e), wire type 2 (length-delimited),
  32 bytes payload = server ephemeral static pub.
- Frame 2 protobuf tag `0a 30` = field 1 (s), 48 bytes payload = client
  identity + signed cert.
- Frames 4+ ciphertext is `0x00` length prefix → small messages, looks like
  app-state sync IV + small ciphertext.
- Heartbeat round-trip is exactly 88B (sent 41 + recv 47) every ~10s.

## What this means for the 401 LoggedOut bug

**The cached server cert chain is dead weight on Chrome's path.** Chrome
pays the full Noise XX handshake on every reconnect (cold IK path never
fired in this run). Our wacore fork's `select_pattern` IK-first logic
(verified by `whatsapp_connect_trace`) is therefore the wrong default.

Two hypotheses, both testable from this run:

1. **Wacore's IK path emits a malformed cert payload that the server 401s.**
   Compare frame[2] from Chrome against wacore's IK output on the same
   session — if Chrome uses XX always, wacore should too.

2. **Wacore emits valid XX but the session secrets are stale.**
   Chrome derives a fresh shared secret per session (localStorage
   encrypted state). Our `server_cert_chain` blob may have been captured
   under different session conditions.

## Next probe

Replay Chrome's frame[0]+[2] bytes against `e.web.whatsapp.com:5222` via a
sibling binary (`whatsapp_replay_chrome_handshake` — proposed). If the
server accepts the wire-shape, the bug is at the post-handshake layer. If
it 401s, the bug is in frame[2] emission (cert encoding).

## Files

- `/tmp/wa-observer/run-1784043740549/initial.jsonl` (Phase 1 raw events)
- `/tmp/wa-observer/run-1784043740549/reconnect.jsonl` (Phase 2 raw events)
- `/tmp/wa-observer/run-1784043740549/summary.txt` (Phase 1 + Phase 2 summary)
- `crates/whatsapp_chrome_reconnect_observer/` (binary that produced this)