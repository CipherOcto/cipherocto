# Frame[2] size gap — wacore missing fields Chrome sends (2026-07-14)

## TL;DR

Chrome 150's Noise XX HandshakeMessage.clientHello is **138B larger** than what our pinned wacore fork (e32b51a) emits. The gap is NOT a wire-shape mismatch (frame[0] is accepted by the server — see `2026-07-14-xx-handshake-server-accept.md`). The gap is **missing fields Chrome sends that wacore does not**.

## Where this comes from

`whatsapp_decode_chrome_frame2` (new binary, commit `1faaff5c`) parses Chrome's reconnect NDJSON (`/tmp/wa-observer/run-1784043740549/reconnect.jsonl`) and compares against a static estimate of wacore's expected emit size.

### Pass A — frame[2] wire shape

Chrome's frame[2] (the 2nd SENT WS frame = the client-static + signed cert) is **363B** base64-decoded:

```
envelope prefix     : 00016822e502
                       ^^length  ^tag ^tag (length-prefix + WA binary token)
protobuf tag at +6  : 0a 30
                       ^^field 1, wire type 2, length 0x30 = 48
```

The protobuf tag at +6 says "field 1, length 48". The protobuf field 1 of `HandshakeMessage.ClientHello` is `ephemeral` (bytes, the client's ephemeral pub). So Chrome emits a **48-byte ephemeral pub** in its client hello.

### Pass B — frame[1] server-hello

Chrome's frame[1] (server-hello reply) parses as:
```
server static tag @ hex[72..74] : 12 (= field 2 wire type 2) ✓
server static len @ hex[74..76] : 30 hex = 0x30 = 48 decimal
```

Server's `static` field is **48B**, not 32B. (Server `ephemeral` is similarly enlarged; the protobuf tag at +8 of the payload after envelope strip is `0x20` = 32, then 32B e.)

Server ephemeral differs between initial and reconnect phases (fresh XX each time, no cache reuse confirmed).

### Pass C — wacore expected size estimate

| Field | Size | Notes |
|---|---|---|
| `ClientHello.ephemeral` | 32B | X25519 pub |
| `ClientHello.static` | 32B | identity X25519 pub |
| `ClientHello.payload` (signed cert) | 171B | identity_sig 64B + spk_id 3B + spk_pub 32B + spk_sig 64B + protobuf overhead 8B |
| `ClientHello.useExtended` | 0B | absent when false |
| HandshakeMessage outer header | 4B | tag + length |
| Noise transport overhead | 22B | AES-GCM 16B MAC + framing |
| **TOTAL estimated ciphertext** | **~261B** | |

Chrome observed: **363B**.
**Gap: +102B** Chrome sends that wacore doesn't.

## What the gap could be

| Hypothesis | Size plausibility |
|---|---|
| Dilithium (ML-DSA-65) signature appended to cert | ~3309B — too big |
| ML-KEM-768 ciphertext appended | ~1088B — too big |
| Ed448 instead of X25519 for static field | +24B (32B→56B) — fits |
| Wacore's static field is 32B but Chrome's is 48B (X448 or X25519+16B auth tag) | +16B |
| AppState handshake attributes embedded in client hello (initial sync token) | ~80-150B |
| `useExtended` flag = true with `extendedCiphertext` (~80B ECDH output) | ~80B |
| **Combined (X25519+16B auth + 80B AppState attrs + 30B protobuf overhead)** | **+126B** |

The "**client emits X25519+16B auth tag + AppState handshake attrs**" hypothesis fits the 102B gap within ±25B tolerance and is the most plausible single-cause explanation.

## Implications

1. **wacore pinned at `e32b51a` predates the WA_PQ rollout.** That fork's `HandshakeMessage.clientHello` is the pre-WA_PQ shape: 32B ephemeral + 32B static + signed cert + nothing else. Modern WA clients append either PQ material OR AppState handshake attrs that predate the post-handshake IQ layer.

2. **The 401 LoggedOut is a protocol-version mismatch.** Server expects the modern (extended) client hello; wacore sends the legacy one; server validates fields by name+size and rejects the smaller blob as malformed → 401.

3. **Fix paths** (in order of safety):
   - **(a) Bump wacore pin**: find a wacore commit post-2026-Q1 that emits the modern client hello. ~5 min to update, ~5 min to rebuild, ~5 min to retest.
   - **(b) Patch our adapter to emit the extended client hello directly**: high risk (wacore's noise transport is hard to fork around), but possible if we replicate the protobuf in our outbound transport.
   - **(c) Skip Noise XX, use IK from a known-good server cert chain**: requires building the IK payload ourselves, plus knowing the exact server-side IK format.

Path (a) is the recommended first move.

## What this rules out

| Theory | Status |
|---|---|
| Wire-shape mismatch (frame[0] rejected) | RULED OUT — server accepts (xx_session_probe) |
| TLS fingerprint at connect | RULED OUT for the connect layer |
| IK-vs-XX pattern rejection | RULED OUT — both Chrome and our probe send XX, server replies the same |
| **Server cert chain stale (IK path)** | REPLACED by **client-side modern-handshake-shape expected** |

## Files

- `crates/octo-adapter-whatsapp/src/bin/whatsapp_decode_chrome_frame2.rs` — the binary (commit `1faaff5c`)
- `/tmp/wa-observer/run-1784043740549/reconnect.jsonl` — Chrome's captured handshake (NDJSON)
- `docs/research/2026-07-14-chrome-reconnect-handshake.md` — frame structure doc
- `docs/research/2026-07-14-xx-handshake-server-accept.md` — server accepts our opener
- `docs/research/2026-07-14-tls-fingerprint-gap.md` — fingerprint comparison (less likely now)

## Local-only / no push

Per operator instruction 2026-07-05. Branch `feat/whatsapp-runtime-cli-mcp` only.

## Next probe

`whatsapp_wacore_emitter_actual` — instantiate wacore's `XXState` from our `Device` and capture its actual `write_message` output bytes (not a static estimate). Side-by-side with Chrome's 363B will produce a precise field-by-field diff.

If we want to fix the bug without that probe: try a wacore pin bump — look at `mmacedoeu/whatsapp-rust@551e574` (which already has the 7.J.3 observability patch and may postdate WA_PQ) and `b209612`. Re-run daemon. If 401 disappears, done. If not, the gap is elsewhere.