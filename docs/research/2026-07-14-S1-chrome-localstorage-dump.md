# Session 1 — Chrome localStorage full dump (2026-07-14)

## Goal

Extend `whatsapp_chrome_session_extract` to dump FULL localStorage values (not just head+len) and classify crypto-relevant keys. Find the auth bundle plaintext that WA Web uses to derive the IndexedDB AES key.

## TL;DR

**Found `WANoiseInfo` in localStorage with 3 base64 fields (privKey/pubKey/recoveryToken).** Most likely the WA Noise identity keypair material, base64-encoded. Plus `WANoiseInfoIv` with 4 IVs and `WAWebEncKeySalt` / `WebEncKeySalt` (174B each, identical strings) for the IndexedDB encryption KDF. This is the master key set we need for Session 2's KDF reverse-engineering + Session 3's IndexedDB decryption.

## Implementation

Replaced the localStorage JS dump in `crates/whatsapp_chrome_session_extract/src/main.rs`:
- Full value dump (capped at 8KB per entry, total ≤ 256KB CDP limit)
- Key classification: matches `(secret|bundle|key|crypt|sign|wau|token|pk|cert|iv|salt|priv)` → `crypto`, otherwise `config`
- Added sessionStorage dump (same shape, no classification)
- Added `document.cookie` dump (HttpOnly cookies won't appear, but flag in commit)
- Added `signal-storage/signal-meta-store` re-read via IndexedDB (cross-check)

## Live result

12 localStorage keys dumped (full values saved to `/tmp/wa-observer/run-1784043740549/indexeddb-summary.json`):

| Key | Kind | Len | Notes |
|---|---|---|---|
| `WAUnknownID` | crypto | 20B | `unknown-6843790256` — non-crypto flag, user-agent probe ID |
| `WAWebEncKeySalt` | crypto | 174B | IndexedDB salt for `wawc_db_enc` (base64) |
| `WebEncKeySalt` | crypto | 174B | Duplicate of `WAWebEncKeySalt` (identical strings) — possibly for `signal-storage` |
| `WANoiseInfoIv` | crypto | 109B | 4 IVs (AES-GCM 16B each, base64-encoded) |
| **`WANoiseInfo`** | (config classification) | **217B** | **JSON: `{privKey, pubKey, recoveryToken}` all base64** |
| `WALangPhonePref` | config | 7B | `pt_BR` |
| `Session` | config | 20B | session marker |
| `banzai:last_storage_flush` | config | 17B | timestamp |
| `RSTData` | config | 2B | `{}` |
| `WALangPrefDidMismatchWithCookie` | config | 5B | `false` |
| `WAWebWAMBeaconingSettings` | config | 58B | JSON array |
| `whatsapp-mutex` | config | 31B | mutex token |

## Decoded `WANoiseInfo` fields

| Field | Decoded size | Hex (first 16 bytes) |
|---|---|---|
| `privKey` | 48 bytes | `fd4f1ab40db55714b545f78ed865af7e8d8bd0592c0e62d6ca3e90061803d72a` |
| `pubKey` | 48 bytes | `c837cd79d445e55590ae9d9cb9c3f134995af574510c2a80188023f34f5ef7b0` |
| `recoveryToken` | 32 bytes | `fdda182e878d27c2635eab49c34f55dda810f2f25b59d5524a8a700e49652922` |

The 48B `privKey` and `pubKey` are **NOT plain X25519 (32B)**. Could be:
- Curve25519 (32B) + Ed25519 (32B) stitched — common WA bundle format
- AES-256-GCM key (32B) + IV (16B) — but unlikely for a Noise identity
- (32B X25519 + 16B metadata/tag) — bespoke

`recoveryToken` is 32B → matches X25519.

## Cookies

Only `wa_web_lang_pref` (5B). HttpOnly cookies (`wau=`, `wa_ul=`) NOT visible via `document.cookie` — need CDP `Network.getCookies` for that, or a different extraction method. **Note for Session 2**: operator may need to grab cookies from Chrome's `Cookies` SQLite DB directly.

## Signal-storage IndexedDB

Re-read confirms structure:
```
signal_last_spk_id: 1
signal_reg_id: 6691
signal_static_privkey: { encKey: {}, value: {} }   ← encrypted at rest
signal_static_pubkey: { encKey: {}, value: {} }    ← encrypted at rest
```

(registration_id 6691 this run vs 12121 prior run — Chrome regenerated IndexedDB store on fresh load? Or different account?)

## What's needed for Session 2

To reverse-engineer the IndexedDB KDF, we need to determine:
- What is the **48B `WANoiseInfo.pubKey`/`privKey`** layout?
- Is `WebEncKeySalt` an HKDF salt for the AES key derivation, or a direct AES key?
- What's the relationship between `WANoiseInfoIv` IVs and `WebEncKeySalt`?
- Does the AES key = HKDF(WANoiseInfo.privKey, salt=WebEncKeySalt, info=...)? Or some other KDF?

The cleanest path forward is **Session 2's plan B**: use `Runtime.evaluate` to read the WA Web IndexedDB encryption module's JS source directly from the loaded bundle. That gives us the exact KDF code. Independent of our ability to guess.

## Files

- `crates/whatsapp_chrome_session_extract/src/main.rs` — extended JS dump
- `/tmp/wa-observer/run-1784043740549/indexeddb-summary.json` — full dump
- `/tmp/wa-observer/run-1784043740549/chrome-profile/Default/IndexedDB/` — original LevelDB store (untouched)

## Local-only / no push

Per operator instruction 2026-07-05. Branch `feat/whatsapp-runtime-cli-mcp` only.
