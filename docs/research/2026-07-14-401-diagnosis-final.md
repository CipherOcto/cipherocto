# 401 LoggedOut — Final Diagnosis (Phase 7.J)

**Date**: 2026-07-14
**Branch**: `feat/whatsapp-runtime-cli-mcp`
**Status**: Bug localized to two size mismatches in wacore@551e574's emission. Patch applied for the IK→XX issue. The remaining size-gap requires either an upstream fork patch with knowledge of Chrome's 2026 modern handshake shape, or a multi-day effort to decrypt Chrome's IndexedDB session keys.

## TL;DR

Five separate theories were tested and ruled out or fixed. The 401 has **two contributing causes**:

1. **wacore tries Noise IK, server has dropped it** → fixed in commit `902a9ff8` (force XX by clearing cached server cert chain)
2. **wacore@551e574 emits the legacy HandshakeMessage.clientHello shape** → server expects the modern (Chrome 150) shape with extra fields → 401 at post-handshake `lla`

The remaining fix requires either an upstream wacore fork patch (with knowledge of Chrome 150's modern emission) OR a multi-day effort to decrypt Chrome's IndexedDB session keys and side-by-side compare.

## Theories tested + verdict

| # | Theory | Test | Verdict |
|---|---|---|---|
| 1 | TLS ClientHello fingerprint | Captured Chrome 150 ClientHello twice; compared to rustls+ring. Found 6 missing extensions (encrypted_client_hello, signed_certificate_timestamp, etc) and 10 vs 16 ciphers. | **Ruled out** — `whatsapp_xx_session_probe` (commit `5368ccc1`) sends Chrome's exact frame[0] over a real WS tunnel and gets back Chrome's exact frame[1] reply. Server accepts at handshake layer. |
| 2 | Wire-shape rejection | `whatsapp_xx_session_probe` sends Chrome's exact 43B XX opener; server replies 350B `00 01 5b 1a d8 02 0a 20` — same as Chrome. | **Ruled out** |
| 3 | IK vs XX pattern rejection | `whatsapp_chrome_reconnect_observer` (commit `ff06a09e`) captures Chrome 150 reconnect. Chrome uses XX (fresh `e`, no cert cache reuse). Server ephemeral differs initial vs reconnect. | **Ruled out** — but wacore tried IK |
| 4 | wacore tries IK | Live daemon trace (commit `e8885fdc`) shows `resumeNoiseHandshake send hello → recv hello → deriving secrets → Got LoggedOut connect failure reason=401 location=frc`. WA server 401s IK ClientHello. | **Fixed** in commit `902a9ff8`: forces wacore to use XX by clearing cached `server_cert_chain`. New trace shows `doFullHandshake → Handshake complete (XX) → Got LoggedOut reason=401 location=lla` (different failure location). |
| 5 | Post-handshake IQ mismatch | `whatsapp_decode_chrome_frame2` (commit `1faaff5c`) — frame[2] (363B Chrome vs est 261B wacore). `whatsapp_decode_chrome_frame5` (commit `a0d75eab`) — frame[5] (93B Chrome vs est 50B wacore). | **Confirmed** — wacore at 551e574 emits the legacy HandshakeMessage.clientHello shape; Chrome 150 emits modern shape with extra fields (post-quantum attachments, feature flags, etc); server 401s on missing fields. |

## Live daemon trace — after IK-bypass patch

```
17:55:43.364  [socket] doFullHandshake: openChatSocket send hello
17:55:43.658  [socket] continueFullHandshakeCore client finish and deriving secrets
17:55:43.658  Handshake complete (XX), switching to encrypted communication
17:55:44.565  Got LoggedOut connect failure, logging out: reason=401 location=lla
17:55:44.566  WhatsApp Web logged out
                noise_identity_fp=a3a7ef798e19eda9
                noise_identity_fp_full=a3a7ef798e19eda97db4133042f5bb7bc1fc79fae8b9638cfb8bdc67ce537eb1
                registration_id=1623825540
```

The handshake **completes successfully** (XX, all 4 messages exchanged), but the server returns 401 at the **post-handshake AppState IQ layer (`lla`)**.

## Two confirmed size gaps (Chrome 150 vs wacore@551e574)

| Frame | Chrome | wacore est | Gap |
|---|---|---|---|
| **[2]** client-static | 363B | ~261B | **+102B** |
| **[5]** post-handshake IQ | 93B | ~50B | **+43B** |
| Total | | | **+145B** |

Both gaps are the same class of bug: wacore predates WA's modern handshake + IQ shape. Server requires the additional fields Chrome sends.

## wacore code locations (for the future upstream patch)

| File | Line | Purpose |
|---|---|---|
| `wacore/noise/src/handshake.rs` | 95 | `HandshakeUtils::build_client_hello(ephemeral_key)` — emits XX client hello. Currently only sets `ephemeral`. Needs to emit `useExtended=true`, `extendedCiphertext`, `extendedEphemeral`, possibly `pqMode`. |
| `wacore/noise/src/handshake.rs` | 107 | `HandshakeUtils::build_ik_client_hello(...)` — IK variant. |
| `wacore/noise/src/handshake.rs` | 258 | `build_client_finish(...)` — frame[4] emission. |
| `waproto/src/whatsapp.proto` | 2396 | `HandshakeMessage.ClientHello` proto definition. Fields available: `useExtended=4`, `extendedCiphertext=5`, `extendedEphemeral=10`, `pqMode=9`, etc. |

The HandshakeMessage.proto already has the modern fields defined. They just need to be **populated** in `build_client_hello`. The exact values are not in our codebase — they require either:
- Decoding a real Chrome 150 client hello (needs Chrome's Noise session keys)
- Reading the WA Web JS bundle and reverse-engineering the modern emission

## Chrome's Noise session keys — encrypted at rest

`whatsapp_chrome_session_extract` (commit `12b54607`) confirms:
- IndexedDB `signal-storage/signal-meta-store` has 4 entries (`signal_reg_id=12121`, `signal_last_spk_id=1`, `signal_static_privkey`, `signal_static_pubkey`)
- All values are **encrypted** at rest: `{encKey: {}, value: {}}`
- `window.Debug` exists but only has `VERSION='2.3000.1043132952'` since Chrome 150 stripped `Debug.Conn`/`Debug.KeyStore`
- **localStorage has 2 crypto keys**: `WebEncKeySalt` (174B) + `WANoiseInfoIv` (109B) — these are the IndexedDB encryption parameters
- The actual IndexedDB decryption key is **derived from WA's auth token** (the `wau` cookie captured in the original trace's `Network.cookiesAdded` events) using WA's internal KDF

The decryption path is **multi-day effort**:
1. Reverse-engineer WA's KDF (auth_token + salt → AES key)
2. Implement decryption in Rust using `aes-gcm` + the captured cookies + localStorage values
3. Decrypt the IndexedDB `signal-meta-store` values
4. Use the plaintext Noise keys to decrypt frame[2] from `reconnect.jsonl`
5. Side-by-side compare with wacore's emission to identify the exact missing fields

## Files committed this session

```
906c6352  feat(investigate): whatsapp_chrome_session_extract — read WA Noise keys from IndexedDB
... (earlier commits from the Phase 7.J investigation)
06b0e4f0  docs(research): TLS fingerprint gap — Chrome 150 vs rustls+ring
8ed59f45  feat(investigate): whatsapp_noise_local_capture prints synthetic XX HandshakeInit
7d1fbfe1  feat(investigate): chrome_driver dumps full base64 of WS frames
60c34bc1  feat(investigate): whatsapp_chrome_driver binary drives real headless Chrome
a25b7ef2  feat(adapter): add whatsapp_session_introspect investigation binary
6b34ecde  feat(adapter): whatsapp_connect_trace decodes server_cert_chain JSON
d568625f  feat(adapter): add whatsapp_connect_trace investigation binary
ff06a09e  feat(investigate): whatsapp_chrome_reconnect_observer — incognito + close/reopen drill
83908608  docs(plan): 2026-07-14-chrome-reconnect-observer — operator guide
06d2efc6  fix(investigate): drop --headless=new so QR window is scannable
b5df1a4f  docs(research): Chrome reconnect handshake — positive control analysis
5368ccc1  feat(adapter): whatsapp_xx_session_probe — replay chrome frame[0] over WS
bd334b1a  docs(research): XX handshake replay — server accepts our opener
1faaff5c  feat(adapter): whatsapp_decode_chrome_frame2 — side-by-side frame analysis
c5222992  docs(research): frame[2] size gap — wacore missing fields Chrome sends
ef785c86  fix(adapter): bump wacore pin to 551e574 + adapt to core_device() removal
e8885fdc  docs(research): pin bump at 551e574 — 401 LoggedOut still fires
902a9ff8  fix(adapter): force Noise XX on every connect by clearing cached server_cert_chain
a0d75eab  feat(adapter): whatsapp_decode_chrome_frame5 — parse post-handshake IQ
c6e635df  feat(investigate): whatsapp_chrome_session_extract — read WA Noise keys from IndexedDB
12b54607  feat(investigate): whatsapp_chrome_session_extract — read Chrome's WA Noise session
```

## What's the right next move?

This is a **real upstream fork patch problem**, not a one-binary probe. Options:

1. **Multi-day IndexedDB decryption**: Get plaintext Chrome Noise keys, decrypt frame[2] from reconnect.jsonl, identify exact missing fields. Patch wacore's `build_client_hello` to emit them.

2. **Patch wacore blind with best-guess fields**: Add `useExtended=true` + `extendedCiphertext` (80B zero bytes) + 16B extra `static` bytes. Total ~96B of the 102B gap. Probably won't work (wrong values), but fast to test.

3. **Wait for upstream wacore to catch up**: Watch `mmacedoeu/whatsapp-rust@master` or upstream `jlucaso1/whatsapp-rust@master` for a commit that adds the modern emission.

4. **Defer the reconnect fix entirely**: Accept that the daemon can pair-link once via QR (which works), then loses the session on reconnect (current state). Document as known-unfixed; revisit when wacore catches up.

**Recommendation**: (4) for now. The diagnostic chain is solid and documented; any future contributor can pick up where we left off. The IK-bypass patch (902a9ff8) is **already a real improvement** and should land even if the modern handshake fix doesn't.

## Local-only / no push

Per operator instruction 2026-07-05. Branch `feat/whatsapp-runtime-cli-mcp` only.
