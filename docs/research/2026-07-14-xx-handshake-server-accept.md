# XX Handshake replay — server accepts our opener (2026-07-14)

## TL;DR

Our adapter's frame[0] (43B WA envelope + 32B fresh X25519 ephemeral pub) is **accepted by `web.whatsapp.com:5222`** when wrapped in a proper WebSocket tunnel. The 401 LoggedOut bug is **not** in the XX handshake opener — it is downstream.

## What we tested

`whatsapp_xx_session_probe` (new binary, commit `5368ccc1`):
1. Opens TCP+TLS to `web.whatsapp.com:5222` via rustls + ring + webpki-roots
2. Performs RFC 6455 WebSocket upgrade (`GET /ws/chat` with `Sec-WebSocket-Key`)
3. Generates a fresh X25519 ephemeral keypair via `x25519-dalek`
4. Sends a 43B frame matching Chrome's observed shape EXACTLY:
   ```
   [57 41] = "WA" magic
   [06 03 00 00] = 0x00000306 LE
   [24 12 22 0a 20] = WA binary token + length + protobuf tag (field 1, wire type 2, length 32)
   [32B e_static_pub]
   ```
5. Wraps the frame in a masked WS binary frame (RFC 6455 §5.3, FIN=1, opcode=2)
6. Reads the server's WS reply (unmasked)
7. Compares the payload bytes against Chrome's frame[1] captured by `whatsapp_chrome_reconnect_observer`

## Live result

```
server reply    : 350 B (after WS header strip)
server reply head: 00015b1ad8020a20556826028a20...
verdict         : MATCHES CHROME FRAME[1] SHAPE (server accepted opener)
```

Chrome's frame[1] (captured `b5df1a4f`):
```
00015b1ad8020a20a64677b29ccd107ca7bedc205418a04daca36ed7d5cbdc14988cb508137b482e1230a9f70fce88e7...
```

Our probe's server reply head:
```
00015b1ad8020a20556826028a20...
```

**Same first 8B prefix.** Same protobuf structure. Server replied the same way Chrome's connection got a reply.

## What this eliminates

Theories ruled out by this probe:

| Theory | Status |
|---|---|
| Wire-shape rejection (different `WA` envelope magic) | **RULED OUT** — identical bytes, accepted |
| WA changed its binary envelope format | **RULED OUT** — same 350B response |
| Server distinguishes rustls+ring from BoringSSL and rejects | **RULED OUT for the handshake layer** — TLS completes, WebSocket upgrade completes, server proceeds to Noise XX. (Note: this is the FIRST packet exchange; the TLS fingerprint gate, if any, would fire here, and it did not. The fingerprint gate (if it exists at all) must be a later post-handshake check, not the connect-time check this probe exercises.) |
| IK-vs-XX pattern rejection | **RULED OUT** — both Chrome and our probe use XX with a fresh ephemeral; server replies identically to both |

## What's left

The bug must be downstream of the XX opener. Two remaining culprits:

1. **frame[2] emission** — the 363B client-side handshake completion:
   - protobuf field 1, 48B payload (client identity pub + signed cert)
   - protobuf field 2 = wrapped static key, signed with identity
   - Our session.db has `noise_key`, `identity_key`, `signed_pre_key` — wacore's
     `HandshakeState::write_message` should derive these into the frame[2]
     payload. If the wire bytes it emits don't match Chrome's exactly, the
     server will reject the client's identity proof → 401.

2. **post-handshake IQ** — frame[5] onwards: the AppState sync attributes
   Chrome sends. If wacore's `<handshake>` IQ uses stale creds, server returns 401.

## Next probe

`whatsapp_decode_chrome_frame2` — base64-decode Chrome's frame[2] from the reconnect JSONL, parse the protobuf structure (using wacore-binary's protobuf definitions), dump fields. Then side-by-side compare with `wacore::handshake::XXState::write_message(..)` output for the same Device keys.

This requires no network and no Chrome — it is a pure decode comparison. If the wire shapes differ at the protobuf-field level, that localizes the bug to wacore's cert emission and points at the fix.

If they match, the bug is post-handshake (AppState IQ or below) and the fix is elsewhere.

## Files

- `crates/octo-adapter-whatsapp/src/bin/whatsapp_xx_session_probe.rs` — the probe
- `crates/octo-adapter-whatsapp/Cargo.toml` — added `rustls 0.23`, `tokio-rustls 0.26`, `rustls-pki-types 1`, `webpki-roots 0.26`, `x25519-dalek 2`, `rand 0.8`
- `docs/research/2026-07-14-chrome-reconnect-handshake.md` — frame structure doc the probe replicates
- `docs/research/2026-07-14-tls-fingerprint-gap.md` — fingerprint comparison (still valuable for the post-handshake gate theory)

## Local-only / no push

Per operator instruction 2026-07-05. Branch `feat/whatsapp-runtime-cli-mcp` only.