# TLS ClientHello fingerprint gap — Chrome 150 vs our adapter (rustls+ring)

**Date**: 2026-07-14
**Capture method**: Python TLS loopback listener (CTYPES parse + JA3 compute); Chrome driven via headless Playwright; our adapter via `/tmp/rustls_probe` (rustls 0.23 + ring + webpki-roots).

## TL;DR

The TLS ClientHello our adapter sends to WA servers is structurally
different from Chrome's. NOT just at the JA3 hash level (which is
permuted by GREASE + Chrome 150's randomized non-GREASE extension
order) but at the **cipher-suite-list level**, **extension-set level**,
and **extension-order level**. Even after stripping GREASE, the gap is
unmistakable.

This is the strongest remaining candidate for the 401 LoggedOut
reconnect failure mode. The hypothesis: WA server fingerprints TLS
stack and rejects non-Chrome sessions.

## Captured ClientHellos

### Chrome 150.0.7871.46 / Linux x86_64 — TWO connections (GREASE varies)

**Connection 1** (1803 B, GREASE cipher `0x0a0a`, GREASE exts `0x7a7a`, `0x6a6a`):
```
JA3 RAW: 771,2570-4865-4866-4867-49195-49199-49196-49200-52393-52392-49171-49172-156-157-47-53,
            31354-10-18-17613-65037-45-51-35-11-13-16-43-27-23-5-65281-27242,
            23130-4588-29-23-24,0
JA3 MD5: b5967fb3d0cb81c3ac0ec8e3d6c9e33a
Ciphers: 16
Extensions: 17
```

**Connection 2** (1771 B, GREASE cipher `0x8a8a`, GREASE exts `0x3a3a`, `0x5a5a`):
```
JA3 RAW: 771,35466-4865-4866-4867-49195-49199-49196-49200-52393-52392-49171-49172-156-157-47-53,
            14906-23-51-27-17613-5-65281-45-18-11-43-16-13-10-65037-35-23130,
            60138-4588-29-23-24,0
Ciphers: 16
Extensions: 17
```

Note: Chrome's **non-GREASE cipher list is stable** across both connections:
`4865-4866-4867-49195-49199-49196-49200-52393-52392-49171-49172-156-157-47-53`
— but its **non-GREASE extension order is NOT stable** in 150. That's
unusual (older Chrome had stable order). Means JA3 hash differs per
connection from Chrome too.

### Our adapter (rustls 0.23 + ring + webpki-roots) — STABLE across runs

```
JA3 RAW: 771,4866-4865-4867-49196-49195-52393-49200-49199-52392-255,
            51-5-35-0-11-45-10-13-43-23,29-23-24,0
JA3 MD5: 083d9ea75d8edbd62f2222fae4d48c8b
Ciphers: 10
Extensions: 10
```

No GREASE. No randomness. Stable.

## Stripped comparison (remove GREASE bytes from both sides)

| Field | Chrome 150 (non-GREASE) | Our adapter |
|---|---|---|
| Cipher count | 15 | 10 |
| Has GREASE | YES | NO |
| Ciphers | `4865,4866,4867,49195,49199,49196,49200,52393,52392,49171,49172,156,157,47,53` | `4866,4865,4867,49196,49195,52393,49200,49199,52392,255` |
| TLS_EMPTY_RENEGOTIATION_INFO_SCSV (255) | NO (replaced by ext 65281) | **YES** |
| Extension count | 16 | 10 |
| Missing vs Chrome | — | `18 (signed_certificate_timestamp), 17613 (application_settings), 65037 (encrypted_client_hello), 27 (compress_certificate), 65281 (renegotiation_info), 23 (extended_master_secret)` |
| Has status_request (5) | YES | YES |
| Has extended_master_secret (23) | YES | NO |
| Has renegotiation_info (65281) | YES | NO |
| Has application_settings (17513) | YES | NO |

The missing extensions are **security-relevant** (extended_master_secret,
renegotiation_info) and **privacy-relevant** (encrypted_client_hello). A
WA server that checks for any of these as a Chrome fingerprint signal
would reject us.

## Why JA3 alone isn't enough

JA3 is **permuted by GREASE** for Chrome, and Chrome 150 also
re-randomizes its non-GREASE extension order. So JA3 hashes differ
across Chrome connections (`b5967fb3...` vs whatever the next
connection produces). That makes JA3-based fingerprinting brittle for
the attacker side too — but it doesn't save us, because the
**structural gap** (cipher list + extension set + missing GREASE) is
still trivially detectable via JA3S (server-hello) or JA4.

## Implications

The TLS layer is the most likely cause of the 401 LoggedOut on reconnect.
Even a perfect mimic of `device`, `os_version`, `app_version`, and
`props_hash` won't help if the WA server fingerprints at TLS.

## Three fix paths

1. **`boring-sys` Rust binding** to BoringSSL (same TLS lib as Chrome)
   → ~95% JA3/JA4 match. Build is heavy (~5-10 min cold).
2. **Headless Chrome as TLS proxy**: daemon launches Chrome, Chrome
   handles TLS to WA, our Rust logic speaks WS over Chrome's DevTools
   Protocol. 100% fingerprint match. Adds Chrome runtime dep.
3. **Custom rustls fork with Chrome-emulating extensions**: add
   `renegotiation_info`, `extended_master_secret`, `application_settings`,
   `signed_certificate_timestamp`, GREASE generator. ~80-90% match.
   Medium effort, pure-Rust.

## Raw captures

- `2026-07-14-our-adapter-tls-clienthello.txt`
- `2026-07-14-chrome-150-tls-clienthello.txt`