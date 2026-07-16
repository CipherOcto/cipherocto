# Session 2 — IndexedDB KDF reverse-engineering (FAILED) + pivot

**Date**: 2026-07-14
**Branch**: `feat/whatsapp-runtime-cli-mcp`
**Author**: Phase 7.J diagnostic chain
**Status**: S2 reported FAILED on hook approach. Pivoted to direct dump + CryptoKey export attempts. Conclusion: **decrypting Chrome's IndexedDB is a dead end** — bypass entirely.

## TL;DR

Tried three approaches to extract WA Web's IndexedDB encryption KDF or the plaintext Noise keys:

1. **Webpack module capture** via `Runtime.evaluate` + `Page.addScriptToEvaluateOnNewDocument` hook on `webpackChunkwhatsapp_web_client.push()` — **FAILED**. Modern WA Web 2.3000.1043140922 does NOT populate `webpackChunk.*` via the legacy push pattern. `chunkLen` stays at 0 even after 18s wait. Hook captured 0 modules.
2. **WPP/Debug globals probe** (walks 6 levels deep looking for crypto functions matching `decrypt|derive|unwrapKey|exportKey`) — **FAILED**. `window.WPP` only has `VERSION`. No crypto APIs reachable.
3. **Direct IndexedDB dump + CryptoKey export** — **PARTIAL**. Found key stores but ALL CryptoKey objects are stored with `extractable: false`. Only Ed25519 signatures (stored as raw bytes, not CryptoKey) are extractable. Cannot read the X25519 private identity keys.

## IndexedDB structure discovered

Chrome 150 / WA Web 2.3000.1043140922 spawns 9 IndexedDB databases:

| DB | Version | Interesting stores |
|---|---|---|
| `signal-storage` | 70 | `signal-meta-store` (4 keys: signal_reg_id, signal_last_spk_id, signal_static_privkey, signal_static_pubkey), `signed-prekey-store` (1 key, signed prekey+keyPair), `identity-store` (empty), `baseKey-store`, `prekey-store`, `senderkey-store`, `session-store` |
| `wawc_db_enc` | 20 | `keys` (1 entry — master AES encryption key for whole IDB), `fts_hmac_keys` (1 entry — HMAC key for FTS) |
| `model-storage` | 1980 | 99 stores, mostly empty. Notable: `direct-connection-keys`, `encrypted-mutations`, `blocklist`, `chat`, `contact`, `message`, `lid-pn-mapping`, `lid-display-name-mapping` |
| `wawc` | 140 | `wam` (1 key), `user`, `ps_meta`, `l10n` |
| `wawdb` | 1 | (empty) |
| `responsiveness-db` | 4 | (empty) |
| `pb_detect` | 1 | (empty) |
| `status-storage` | 10 | (empty) |
| `worker-storage` | 20 | (empty) |

`signal-static-pubkey` and `signal-static-privkey` are stored as opaque `{encKey: CryptoKey, value: CryptoKey}` structures with `extractable: false`. Calling `crypto.subtle.exportKey('raw', key)` or `('jwk', key)` both throw "This Key cannot be exported" because the keys were imported with `extractable=false` in WA Web's JS (deliberate hardening).

`signed-prekey-store[1]` has structure `{keyId, keyPair, signature}` where:
- `keyPair.privKey/pubKey` = CryptoKey, non-extractable
- `signature` = 64 raw bytes, **EXTRACTABLE** (Ed25519 signature)

Got the signature: `75abaaadf7d992b10eefe40cc089be0b7a4e54b81c78c32507e1d71b7061d93484884972a014c73ac14910f74f3f2ed4334ae1379a301be9685e33768ed30907`

This is one piece — useless without the rest of the keypair, which is locked behind extractable=false.

## Why the IndexedDB approach is fundamentally a dead end

Chrome's crypto.subtle non-extractable CryptoKey is enforced by the browser process. Even CDP `Runtime.evaluate` cannot extract these — Chrome refuses at the C++ layer. The only ways to read the raw bytes of these keys would be:

1. **Patch Chrome** to ignore the `extractable` flag (outsourced attack surface — way too much work)
2. **Read the raw IndexedDB LevelDB** + know the master AES key + AES-GCM decrypt

But the master AES key (in `wawc_db_enc/keys[1]`) is ALSO stored as non-extractable CryptoKey. AND it's only set up at first login. Browsers reset IndexedDB on profile re-creation.

3. **HKDF derive the master key** from the auth bundle (`wawc-secret-bundle`) + `WebEncKeySalt` — but `wawc-secret-bundle` is NOT in our localStorage. Only set during active session. May have been cleared.

We have:
- `WAWebEncKeySalt` (174B) + `WebEncKeySalt` (174B, identical)
- `WANoiseInfo` (217B: 48B privKey + 48B pubKey + 32B recoveryToken)
- `WANoiseInfoIv` (109B: 4 IVs)

We're missing the IKM for any plausible KDF.

## The pivot: we don't need Chrome's keys

Re-read of the problem statement:
- Our wacore generates a Noise XX ClientHello that is **261B**
- Chrome 150 generates a Noise XX ClientHello that is **363B**
- Gap = **+102B**

The missing fields are present in the proto schema:
```
message ClientHello {
  optional bytes ephemeral = 1;        // 32B (have)
  optional bytes static = 2;            // 32B (have)
  optional bytes payload = 3;           // ~145B signed cert (have)
  optional bool useExtended = 4;        // 1B  (MISSING)
  optional bytes extendedCiphertext = 5;// ~80B ECDH-encrypted (MISSING)
  optional HandshakePqMode pqMode = 9;  // varint (MISSING)
  optional bytes extendedEphemeral = 10;// 32B random (MISSING)
}
```

**We do not need Chrome's identity keys to emit the modern shape.** Wacore has its OWN identity keys in its session.db. The fix is to populate those 4 missing fields with computed values and test against the server.

If the server accepts, we don't need to know Chrome's actual values. If the server rejects, we know which field's value is wrong.

## Updated plan

Old: S2 → S3 → S4 → S5 → S6 → S7 → S8
New: **S2 done (failed)** → **S6 (patch wacore with inferred field values)** → **S6.5 (test against server in whatsapp_xx_session_probe)** → **if server rejects: iterate field values** → **S7 (live daemon run)** → **S8 (land)**

Skipped: S3 (decrypt IndexedDB), S4 (decrypt Chrome frame[2]), S5 (field-by-field diff vs Chrome). These were the path to KNOWING Chrome's exact field values — but we can infer them from the proto schema + test against server.

## Files

- `crates/whatsapp_chrome_session_extract/src/bin/whatsapp_kdf_dump.rs` — S2 webhook module capture (didn't capture anything; kept for forensic record)
- `crates/whatsapp_chrome_session_extract/src/bin/whatsapp_idb_decrypt_attempt.rs` — S2 attempted KDF brute force + CryptoKey export (confirmed non-extractable; kept for forensic record)
- `/tmp/wa-observer/run-1784043740549/idb-decrypt-attempts.json` — Full IDB dump output
- Output: `signal-static-privkey`/`signal-static-pubkey` are non-extractable CryptoKey; `wawc_db_enc/keys[1]` is non-extractable CryptoKey. Cannot decrypt.

## Next step

Skip to S6: patch wacore's `HandshakeUtils::build_client_hello` to populate `useExtended=true`, `extendedCiphertext=<ECDH(x25519_priv=random, server_static_ecdh_pub)>`, `pqMode=WA_PQ`, `extendedEphemeral=<random 32B>`. Test against server with `whatsapp_xx_session_probe` extended to send the new frame[2] shape. If server returns a valid frame[3], we've cracked it.

If server rejects: iterate field values.

## Local-only / no push

Per operator instruction 2026-07-05. Branch `feat/whatsapp-runtime-cli-mcp` only.
