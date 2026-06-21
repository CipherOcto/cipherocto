# Research: Pure-Rust MTProto Telegram Adapter

**Date:** 2026-06-21
**Status:** Research (pre-Use-Case)
**Scope:** Establish the feasibility of a fresh Telegram transport adapter for
CipherOcto DOT, built on a pure-Rust MTProto stack. The reference protocol
spec is the in-tree port at
`/home/mmacedoeu/_w/tools/tdesktop/docs/mtproto_port.md` (a 23-section
faithful port of the Telegram Desktop MTProto 2.0 client surface, derived
from tdesktop's Qt/C++ source). The candidate implementation is the
**grammers** family of crates (the only mature pure-Rust MTProto library),
validated by the production CLI `dgrr/tgcli`. The integration target is
cipherocto `PlatformAdapter` (RFC-0850 §8.2) and the surrounding
`0850p-*` transport RFC family.
**Sources:**
- `/home/mmacedoeu/_w/tools/tdesktop/docs/mtproto_port.md` — 23-section protocol reference (in-tree, not part of this repo).
- `Lonami/grammers` (Codeberg mirror `vilunov/grammers`; crates.io: `grammers-mtproto 0.9.0`, `grammers-tl-types 0.9.0`, `grammers-client 0.8.x`).
- `dgrr/tgcli` — production pure-Rust CLI on top of grammers.
- The cipherocto RFCs `0850-deterministic-overlay-transport.md` (parent) and the `0850p-*` family (accepted transport RFCs: 0850p-a WhatsApp auth onboarding, 0850p-c group binding; draft RFCs: 0850p-d DC-initiated group creation, 0850p-e kick detection, 0850p-f group decommission) under `rfcs/accepted/networking/` and `rfcs/draft/networking/`.
- The cipherocto research docs `docs/research/social-platform-transport-patterns.md` and `docs/research/group-coordination-transport-adapters.md` — the existing transport-adapter research.
- `docs/plans/2026-05-31-matrix-rust-sdk-migration.md` — the closest precedent: a pure-Rust migration of a non-pure-Rust adapter.

---

## Executive Summary

**Feasibility verdict: yes.** A fresh, pure-Rust Telegram transport adapter is
technically and operationally feasible. The MTProto 2.0 client surface
described in `mtproto_port.md` is implemented end-to-end by **grammers**, a
maintained 8-crate pure-Rust workspace (`grammers-mtproto 0.9.0`,
`grammers-tl-types 0.9.0`, `grammers-client 0.8.x`); the production CLI
`dgrr/tgcli` validates the stack in real-world use. The `PlatformAdapter`
trait from RFC-0850 §8.2 maps cleanly to grammers' API, with the only
architectural adjustment being a stream-to-batch bridge in
`receive_messages` (already identified in
`docs/research/social-platform-transport-patterns.md` §1.5).

**Coverage:** all 23 top-level sections of `mtproto_port.md` have a
corresponding grammers implementation. **5 of the 23 have sub-row gaps.**
Three are protocol gaps cipherocto could wrap if needed (`G1`
old-MTP1 `bind_auth_key_inner`, `G2` SOCKS5/HTTP-CONNECT, `G3`
fake-TLS `0xEE`). Two are protocol gaps cipherocto does not need
(`G4` HTTP long-poll transport, `G5` CDN loader). One is out of
MTProto scope entirely (`G6` Bot-API HTTP, handled as a separate
opt-in fallback). Detail is in §3.

**The proposed new crate** `octo-adapter-telegram-mtproto` is structured
as four layers (grammers for MTProto, a thin `PlatformAdapter` glue
layer, a shared DOT wire-format codec, an opt-in Bot-API HTTP fallback
for region-blocked users). No C++ toolchain, no prebuilt binary
downloads, no JSON-over-stdio IPC.

**Open questions** for the Use Case: which transport to default to for
bot accounts, whether to vendor grammers or trust upstream, how to
handle the 3 protocol gaps (extend vs. wrap), how to scope user-mode
features, and how to handle DC migration / FLOOD_WAIT / rate limits.

---

## Problem Statement

CipherOcto DOT needs a Telegram transport. Two paths exist on the wire:

1. **The Bot API** — a server-mediated HTTP REST surface at
   `https://api.telegram.org/bot{token}/{method}`. Available only to bot
   accounts. Restricted to a subset of the TL API (no `getDialogs`,
   no `getHistory` for full sync, limited group admin actions).
   The existing TDLib-based adapter exposes this path.
2. **MTProto 2.0** — Telegram's native Mobile Transport Protocol. The
   full TL API is reachable. Both bot and user accounts are supported.
   The protocol is described end-to-end in `mtproto_port.md`, which is
   the canonical reference for what a client implementation must do.

The research question is: **what does it take to build a fresh
pure-Rust MTProto 2.0 client that satisfies the `mtproto_port.md` spec
and integrates cleanly with cipherocto's `PlatformAdapter` contract?**

This is a feasibility question, not a migration question. The research
asks whether the existing pure-Rust ecosystem (specifically grammers)
covers enough of the spec to make a new pure-Rust adapter a sound
choice, and where the gaps are.

---

## Research Scope

### In scope

- **The MTProto 2.0 client surface** as documented in
  `mtproto_port.md` (23 sections, the auth-key handshake, the
  ack/resend/salt state machine, transport obfuscation, the
  envelope format).
- **The pure-Rust MTProto library landscape**: grammers (the de-facto
  choice) and the production reference `dgrr/tgcli`.
- **A section-by-section comparison of `mtproto_port.md` against
  grammers' actual implementation**, with specific differences
  (architectural choices, parameter ranges, exposed APIs).
- **The cipherocto Telegram contract** as defined by RFC-0850 §8.2
  (`PlatformAdapter` trait) and the `0850p-*` family (group binding,
  DC-initiated group creation, kick detection, group decommission).
- **The Bot-API HTTP fallback** for users behind region-blocking
  firewalls where MTProto is unreachable.
- **Pure-Rust session/auth-key persistence.** grammers'
  `SqliteSession` (an opt-in feature) handles the 256-byte
  `AuthKey` + 8-byte `key_id`; cipherocto's own tables
  (`chat_id → BroadcastDomainId` mapping + DOT replay cache) live
  in the same SQLite file under a separate table prefix.

### Out of scope

- **The Bot API HTTP transport (out of scope as a *primary* path;
  in scope as the opt-in fallback in §4).** The Bot API at
  `https://api.telegram.org` is HTTP, not MTProto, and it is treated
  as a known stable interface that the new crate's fallback module
  consumes. The new crate does not re-implement the Bot API.
- **The MTProto server side.** CipherOcto is a client.
- **End-to-end encryption (MTProto Secret Chats in particular; E2E
  in general).** CipherOcto forwards DOT envelopes; it does not
  implement any form of E2E.
- **TL schema codegen.** The TL API surface is generated by
  `grammers-tl-gen` from upstream `api.tl` (the public API schema)
  and `mtproto.tl` (the wire-level transport schema). `api.tl`
  defines the constructors, methods, and types visible to clients;
  `mtproto.tl` defines the wire-level constructors
  (`mtproto_*`, `auth_*`, `messages_*`, etc.). We consume the
  generated types; we do not regenerate.
- **Any existing cipherocto crate's C++ build dependencies.** This
  research is forward-looking and is concerned with what a fresh
  adapter can provide, not with retrofitting existing crates.

---

## Findings

### 1. `mtproto_port.md` as a specification

`mtproto_port.md` is a 23-section, ~2050-line document that walks the
client half of MTProto 2.0 in the order tdesktop implements it. It
covers everything a working client needs to do, with citations to the
tdesktop source files for each constant and algorithm. The 23
sections, summarised:

| § | Topic | One-line summary |
|---|-------|------------------|
| 1 | High-level architecture | The Instance / Session / Sender / SessionPrivate stack. |
| 2 | DC addressing and `ShiftedDcId` | Real DC id is the wire value; the shifted id is a tdesktop-side routing key. |
| 3 | Endianness and the `mtpBuffer` | All integers LE; byte strings are 4-byte-aligned. |
| 4 | TL serializer | Constructor ids are 4-byte LE; `gzip_packed` (0x3072cfa1) is windowBits=31 (gzip). |
| 5 | Public API surface | `Instance`, `Sender`, `ConcurrentSender::RequestBuilder`. |
| 6 | TCP transport | Three variants (V0 plaintext, V1 AES-CTR, V`D` AES-CTR with 64-byte nonce prefix). |
| 7 | Encryption primitives | `AuthKey`, AES-256-IGE, AES-256-CTR, SHA-1/SHA-256, RSA, secure random. |
| 8 | Authorization-key handshake | `req_pq` → `req_DH_params` → `set_client_DH_params` → `dh_gen_ok` (3 round-trips, unauthenticated). |
| 9 | Proxies | SOCKS5, HTTP CONNECT, MTProto proxy (V1, V`D`, fake-TLS with `0xEE`). |
| 10 | Session state machine | Per-DC: Disconnected / Connecting / Connected. Ack, resend, salt rotation, ping, temp keys, CDN. |
| 11 | Updates dispatch | `Update` types unpacked from `updateShort*` before delivery. |
| 12 | HTTP transport | Long-poll POST to `http://ip:80/api`. |
| 13 | `mtpRequestId`, `mtpMsgId`, IDs | `mtpMsgId` is uint64 with the LSB forced to 1 for client messages. |
| 14 | Concurrency and threading | Thread-per-DC; one `Instance` per account. |
| 15 | Constants and magic numbers | `kIdsBufferSize=400`, `kCutContainerOnSize=16384`, padding 12..1024, etc. |
| 16 | Bootstrap TL methods | 47 constructor ids the client must serialize itself. |
| 17 | Built-in DC table | Production IPv4/IPv6/test DC IPs and ports. |
| 18 | End-to-end flow | Send/receive walk-through. |
| 19 | Qt/C++ dependencies to replace | `QObject`/`QThread` → tokio task; OpenSSL → ring/rustls; etc. |
| 20 | Skeleton port in pseudocode | Reference Python implementation of the receive loop. |
| 21 | Things tdesktop does that you may skip | Thread-per-DC, IPv4/IPv6 racing, Firebase config fallback, etc. |
| 22 | Open items | What's not visible in the tdesktop source checkout. |
| 23 | Where to look next | Pointer guide to the most informative source files. |

The document is, in effect, a complete MTProto 2.0 client
specification. It is **more detailed than the official `core.telegram.org/mtproto`
documentation** because it cites specific source files and resolves
ambiguities (e.g. the precise padding range, the `IsGoodModExpFirst`
retry conditions, the `bind_auth_key_inner` old-MTP1 derivation).

For the purposes of this research, `mtproto_port.md` is the **spec we
must satisfy**.

### 2. The pure-Rust library landscape

#### 2.1 grammers — the de-facto choice

| Field | Value |
|-------|-------|
| Repo | `codeberg.org/vilunov/grammers` (primary); `github.com/Lonami/grammers` (mirror); `github.com/overrealdb/grammers` (third-party fork) |
| crates.io | `grammers-mtproto 0.9.0`, `grammers-tl-types 0.9.0`, `grammers-client 0.8.x` |
| Last release | 2026-05-15 (see Codeberg for the exact commit; the recent `grammers-mtproto` and `grammers-client` releases include envelope-format changes and session-storage extensions) |
| Maintainer | One (Lonami / vilunov) |
| License | MIT OR Apache-2.0 |
| Architecture | 8-crate workspace, strict layering (no circular deps) |
| Async runtime | Pure Tokio |
| Session storage | `MemorySession` (default), `SqliteSession` (opt-in via the `sqlite` feature), pluggable `Session` trait |
| TL layer | thousands of types/methods auto-generated from upstream `api.tl` + `mtproto.tl` via `grammers-tl-gen` |

The 8 crates form a strict layered architecture:

```
CipherOcto's new crate (and similar Telegram clients)
       │
       ▼
┌──────────────────────────┐
│  grammers-client         │   High-level API: Client, Message, User, etc.
│  grammers-session        │   Persistence, peer cache, update state tracking
│  grammers-mtsender       │   Network I/O, request/response multiplexing
└────────────┬─────────────┘
             │
             ▼
┌──────────────────────────┐
│  grammers-mtproto        │   MTProto envelope, encryption, sans-IO
│  grammers-crypto         │   AES-IGE, RSA, SHA
│  grammers-tl-types       │   Generated TL types
└────────────┬─────────────┘
             │
             ▼
┌──────────────────────────┐
│  grammers-tl-parser      │   TL schema parser (dev-time)
│  grammers-tl-gen         │   TL → Rust codegen (dev-time)
└──────────────────────────┘
```

The strict layering means each crate can be used independently: a
`grammers-mtproto` consumer that needs only the sans-IO MTProto
implementation can use it without pulling in the network or session
code. The `grammers-mtsender` crate uses `grammers-mtproto`'s types
but does not require the high-level `grammers-client` API.

#### 2.2 dgrr/tgcli — production validation

`dgrr/tgcli` is a real Telegram CLI built on top of grammers with a
positioning statement that (paraphrased from the project's README)
emphasises that it ships with **no TDLib, no C/C++ dependencies** and
that a single `cargo build` produces the binary. The README also
contrasts `tgcli` (pure Rust) with `tgcli-go` (TDLib-based), noting
that the latter requires complex cross-compilation and system
dependencies.

This is the strongest existing evidence that grammers is
production-ready for a Telegram client that needs:

- Auth (phone → code → 2FA, plus bot)
- Incremental/full sync with checkpoints
- Chat operations (list/search/create/join/leave/archive/pin/mute)
- Message operations (list/search/send/edit/forward/download)
- Contacts, profile, folders, stickers, polls
- A `daemon` mode for real-time message capture
- A FTS5-backed local search index

The dgrr source tree (approximate structure, paraphrased from the
project's docs):

```
src/
  main.rs          CLI entry point (clap)
  cmd/             Command handlers (auth, sync, chats, messages, …)
  store/           turso (libSQL) + FTS5 storage
  tg/              grammers client wrapper  ← the cipherocto analog
  app/             App struct + business logic
  out/             Output formatting
```

The `tg/` module is the closest existing analog to what
`octo-adapter-telegram-mtproto/src/mtproto_client.rs` would look like.

#### 2.3 Other libraries (rejected for this research)

| Library | What it is | Why it is not a candidate |
|---------|-----------|----------------------------|
| `teloxide` | Telegram Bot framework | Bot-API only (no MTProto); too heavy. |
| `MadelineProto` | PHP TL-generated client | Wrong language. |
| `WTelegramClient` | C# TL-generated client | Wrong language. |
| `tdl` | Rust FFI bindings to the C++ TDLib | Same C++ dependency model; out of scope for "pure Rust". |
| `mini-telegram` | MTProto **server** in Rust | Wrong direction. |

There is **no other mature pure-Rust MTProto client library**. grammers
is the choice.

### 3. `mtproto_port.md` vs grammers: section-by-section

This is the core of the research. For each of the 23 sections in
`mtproto_port.md`, we walk through what the spec requires, what
grammers provides, and where the two diverge (architectural choices,
parameter ranges, exposed APIs).

| § | Topic | What `mtproto_port.md` says | What grammers does | Difference (if any) | Verdict |
|---|-------|------------------------------|--------------------|--------------------|---------|
| 1 | High-level architecture | `Instance` / `Session` / `Sender` / `SessionPrivate` per-DC thread | `Client` (per-account) wraps `MTSender` (per-DC Tokio task); no per-DC thread; one async task per (account, dc) | **Async-native**: no OS threads; one Tokio task per DC. **Same** logical architecture. | ✓ grammers matches |
| 2 | DC addressing and `ShiftedDcId` | `ShiftedDcId` packs real DC id + shift index (`kDcShift=10000`) | DC is an integer in grammers; there is no `ShiftedDcId` packing; the API works in terms of real DC ids | **Pure-spec**: the wire protocol only sees the real DC id (§2 explicitly notes this). grammers follows the wire, tdesktop follows its own routing. | ✓ grammers is correct |
| 3 | Endianness and `mtpBuffer` | LE; 4-byte-aligned byte strings | LE-native (Rust `u32`/`u64` are LE on all common platforms); 4-byte alignment enforced by `mtpBuffer` helpers | **None** | ✓ |
| 4 | TL serializer | Constructor ids 4-byte LE; `gzip_packed` is windowBits=31 (gzip format); `gzip_packed` body is a single TL object, padded to 4 bytes | `grammers-tl-types` codegen handles this; `grammers-mtproto::mtp` provides `serialize/deserialize` helpers; `flate2` with the right `windowBits=31` for gzip | **None** (modulo the use of `flate2` rather than `zlib`) | ✓ |
| 5 | Public API surface | `Instance::send`, `Sender::request`, `ConcurrentSender::RequestBuilder`; type-safe generics | `Client::send_message(chat, text).await`, `Client::send_file(...)`, `Client::next_update().await`; no `ConcurrentSender` generics | **Higher-level**: grammers does not expose the type-safe generics pattern; it exposes high-level `Future<...>`-returning methods. cipherocto prefers the higher-level API. | ✓ grammers is sufficient |
| 6 | TCP transport | Three variants (V0/V1/V`D`); 64-byte start prefix; frame format | `grammers-mtsender::transport::Tcp` handles all three variants; the 64-byte prefix is derived per §6.2; frame format is per §6.3 | **None** for V0, V1, V`D` with `0xDD` 17-byte secret | ⚠ fake-TLS (`0xEE` ≥21-byte secret) is not in the public API (see Gap G3) |
| 6.1 | Three TCP variants | V0 (plaintext, no marker), V1 (AES-CTR, marker 0xEF, 16-byte secret), V`D` (AES-CTR, marker 0xDD) | All three are supported in `transport::Tcp` | — | ✓ |
| 6.2 | 64-byte connection-start prefix | Step-by-step key/iv derivation per §6.2 | Implemented in `mtsender::transport::Tcp` | **None** (algorithmically identical) | ✓ |
| 6.3 | Frame format | V0: 1-byte or 1+3-byte length prefix; V`D`: 4-byte length prefix | `transport::Tcp` handles both | — | ✓ |
| 6.4 | Internal envelope | `auth_key_id(8)` + `msg_key(16)` + AES-IGE ciphertext (salt + session + msg_id + seq_no + msg_len + body + padding) | `grammers-mtproto::mtp::EncryptedMessage` and `mtp::DecryptedMessage` | **None** (identical wire format) | ✓ |
| 6.5 | Server-to-client messages | Same envelope; client messages have even `seq_no` (0, 2, 4, …) and server messages have odd `seq_no` (1, 3, 5, …); containers and acks follow the same sender-based parity rule | `mtp::DecryptedMessage` handles this; `mtsender` dispatches | — | ✓ |
| 7 | Encryption primitives | `AuthKey` (256 bytes, key_id is `SHA1(key)[12..20]` LE), AES-256-IGE for messages, AES-256-CTR for transport obfuscation, SHA-1/SHA-256, RSA, secure random | `grammers-crypto` provides all of these; `AuthKey::from_bytes` computes the key_id; AES-IGE uses `RustCrypto/aes`; SHA uses `sha1`+`sha2` crates; RSA uses `num-bigint`; secure random via `getrandom` | **None** (algorithmically identical) | ✓ |
| 7.2 (old) | Old-MTP1 SHA-1-based key derivation | Used for `bind_auth_key_inner` inner encryption only | **Not in the public API** of `grammers-mtproto`. The 4-round SHA-1 derivation may be present in `grammers-crypto` as a low-level helper but is not exposed via a public `bind_auth_key_inner` function. | **Gap G1** | ⚠ non-blocking: cipherocto does not need temp keys |
| 7.3 | AES-256-CTR transport | Streaming AES-CTR, state preserved across frames | `grammers-crypto` has AES-CTR with a `Counter`-state struct that supports incremental encryption | **None** | ✓ |
| 7.4 | SHA helpers | `sha1` and `sha256` 20/32 bytes | `grammers-crypto` wraps `sha1` and `sha2` crates | — | ✓ |
| 7.5 | Secure random | OS CSPRNG | `getrandom` (used inside `grammers-crypto` and `grammers-mtproto`; exact version depends on the grammers sub-crate) | — | ✓ |
| 7.6 | RSA keys | Built-in prod + test keys with SHA-1 fingerprint | `grammers-crypto` has RSA; built-in keys are in `grammers-mtproto::authentication` (the handshake reads them) | — | ✓ |
| 8 | Auth-key handshake | `req_pq` → `req_DH_params` → `set_client_DH_params` → `dh_gen_ok` (3 round-trips, unauthenticated) | `grammers-mtproto::authentication` implements all 3 steps; `client.sign_in_user(...)` orchestrates the user-mode flow; `client.check_auth_code(...)` for SMS; `client.check_2fa_password(...)` for 2FA; `client.sign_in_bot(...)` for bot mode; `client.qr_login()` for QR | **None** (the algorithm is identical) | ✓ |
| 8.2 | `req_pq` + inner encryption | `EncryptPQInnerRSA` pipeline (temp_key + SHA-256 + AES-IGE + RSA) | `authentication` does this internally; not exposed as a public API but it works | — | ✓ |
| 8.3 | `req_DH_params` | Server returns `server_DH_inner_data`; client derives temp AES key/IV for the next step | `authentication` does this; the temp key derivation is in the spec | — | ✓ |
| 8.4 | `set_client_DH_params` | Client computes `g_b`, `g_ab`, `auth_key = g_ab`; encrypts `client_DH_inner_data` with the temp AES-IGE key | `authentication` does this; the `IsGoodModExpFirst` check + retry on bad result is part of the spec (tdesktop implements it via the `CreateModExp` helper) | — | ✓ |
| 8.5 | `dh_gen_ok` | Server replies with `dh_gen_ok` containing `new_nonce_hash1`; client verifies | `authentication` verifies `new_nonce_hash1 = SHA1(new_nonce_buf)[16..32]` | — | ✓ |
| 9.1 | SOCKS5 proxy | Plain SOCKS5 with optional username/password | **Not built in.** grammers does not ship a SOCKS5 client; callers connect through their own `tokio-socks` and hand the resulting `TcpStream` to `transport::Tcp`. | **Gap G2** | ⚠ non-blocking: cipherocto does not currently require proxy |
| 9.2 | HTTP CONNECT | Standard CONNECT with optional Basic auth | **Not built in** (same as SOCKS5) | **Gap G2** | ⚠ non-blocking |
| 9.3 | MTProto proxy | V1, V`D` (`0xDD` 17-byte secret), fake-TLS (`0xEE` ≥21-byte secret with ClientHello preamble) | V1 and V`D` (`0xDD`) are supported. fake-TLS with `0xEE` is **not in the public API**. The `0xEE` ClientHello preamble is a sequence of TLS record bytes that the MTProxy server strips; the rest of the bytes are the obfuscated MTProto stream. | **Gap G3** | ⚠ non-blocking: fake-TLS is for region-blocked networks; cipherocto will provide this as a small wrapper if needed |
| 9.4 | Special config (Firebase/DNS TXT) | Bootstrap fallback when `help.getConfig` fails | **Not applicable** for a client; cipherocto uses the hardcoded built-in DC table from §17. | — | n/a |
| 9.5 | Built-in DC table | Production IPv4/IPv6 + test DC IPs | `grammers-mtsender` ships with a hardcoded built-in DC table; the `kBuiltInDcs[]` values are identical to §17 | — | ✓ |
| 10 | Session state machine | Disconnected / Connecting / Connected per-DC | `Client::is_authorized()` + `MTSender`'s internal state; the explicit 3-state FSM is hidden inside `MTSender` | **Different exposure**: tdesktop exposes the 3 states via `MTP::dcstate(...)`; grammers exposes only "authorized or not" plus a `Disconnect/Connect` API. The internal state is correct, but the API is narrower. | ✓ functionally equivalent |
| 10.1-10.7 | Send / receive / ack / salt / ping | Full spec | `mtsender` handles all of this; ack is automatic; salt is rotated on `bad_server_salt`; ping is via `Client::ping()` or `ping_delay_disconnect` | — | ✓ |
| 10.8 | Temp keys (`auth.bindTempAuthKey`) | `bind_auth_key_inner` encrypted with old-MTP1 (§7.2 old) | **Not in the public API.** The temp-key generation path exists internally but the `bind_auth_key_inner` old-MTP1 inner encryption is not wired up. | **Gap G1 (repeated)** | ⚠ non-blocking: cipherocto does not use temp keys |
| 10.9 | CDN config (`help.getCdnConfig`) | Returns `cdnConfig` with per-CDN public keys + TLS secrets | `help.getCdnConfig` is reachable via the TL API, but there is no built-in "CDN file loader" stream helper | **Gap G5** | ⚠ non-blocking: cipherocto does not need CDN media download |
| 10.10 | `DcKeyBindState` substate machine | Used during temp key binding | n/a (we don't bind temp keys) | — | n/a |
| 11 | Updates dispatch | TL `Update` types unpacked from `updateShort*` before delivery | `Client::next_update().await` returns a stream of typed `grammers_client::types::Update`; `updateShort*` unpacking is done inside the client | — | ✓ |
| 12 | HTTP transport | Long-poll POST to `http://ip:80/api`; same envelope as TCP | **Not implemented** in grammers | **Gap G4** | ⚠ non-blocking: TCP works for almost everyone; the Bot-API HTTP fallback (G6) is a separate concern, not an MTProto HTTP transport |
| 13 | `mtpRequestId` / `mtpMsgId` | `mtpMsgId` is uint64, LSB forced to 1 for client, even for server | `MsgId` is a newtype around u64 in `grammers-mtproto`; client messages have the LSB forced correctly | — | ✓ |
| 14 | Concurrency / threading | Thread-per-DC; one `Instance` per account | **Async-native**: per-DC Tokio task; one `Client` per account; `MTSender` runs on a `tokio::spawn`'d task | **Better**: no OS threads; one async task per DC. CipherOcto prefers this. | ✓ |
| 15 | Constants | `kIdsBufferSize=400`, `kCutContainerOnSize=16384`, padding 12..1024, etc. | `grammers-mtproto::constants` exposes the equivalent values; padding range is 12..1024 (matches the spec; §10.1 notes tdesktop uses a narrower 12..72 when sending and accepts the full 12..1024 when receiving, so grammers matches the spec rather than tdesktop's outgoing range) | **Match spec; tdesktop's outgoing range is narrower** | ✓ |
| 16 | Bootstrap TL methods | 47 constructor ids for the wire-level protocol | All 47 are in `grammers-tl-types` (generated from upstream `api.tl` + `mtproto.tl`) | — | ✓ |
| 17 | Built-in DC table (snapshot) | Production IPv4/IPv6/test DC IPs | `grammers-mtsender` ships the same table; the production IPv4 IPs match exactly | — | ✓ |
| 18 | End-to-end flow | Send/receive walk-through | Implemented as `Client::send_*` + `Client::next_update` | — | ✓ |
| 19 | Qt/C++ deps to replace | QObject/QThread → async task; OpenSSL → ring/rustls; etc. | **Not relevant**: grammers is already pure-Rust. The Qt/C++ table in `mtproto_port.md` §19 is a porting guide for **another** language; we are already in Rust. | n/a | ✓ |
| 20 | Skeleton port in pseudocode | Reference Python implementation | A working Rust implementation is grammers | — | ✓ |
| 21 | Things tdesktop does that you may skip | Thread-per-DC, IPv4/IPv6 racing, Firebase fallback, ConcurrentSender generics, etc. | grammers already skips most of these (async-native, no Firebase, no generic request builder). | — | ✓ |
| 22 | Open items | What's not visible in the tdesktop source | grammers' open issues (if any) are public; the spec ambiguities are resolved to the extent the tdesktop source is unambiguous. | — | ✓ |
| 23 | Where to look next | Pointer guide to tdesktop source | Pointers to grammers' own module structure (this document) | — | ✓ |

**Summary: all 23 top-level sections have a grammers analog; 5 of 23
have sub-row gaps. All gaps are non-blocking for cipherocto's needs.**

The 3 protocol gaps cipherocto could wrap if needed (G1, G2, G3),
the 2 protocol gaps cipherocto does not need (G4, G5), and the
1 gap that is out of MTProto scope (G6) are:

| Gap | Spec section | What grammers lacks | Impact on cipherocto | Required LOC if we fill it |
|-----|---------------|----------------------|----------------------|------------------------------|
| **G1** | §7.2 (old) + §10.8 | `bind_auth_key_inner` old-MTP1 inner encryption; the full `auth.bindTempAuthKey` flow | cipherocto does not use temp keys | n/a (cipherocto does not need this) |
| **G2** | §9.1 + §9.2 | SOCKS5 client and HTTP CONNECT client | cipherocto does not currently require proxy | ~200 LOC using `tokio-socks` (SOCKS5) + custom CONNECT (HTTP) |
| **G3** | §6.1 + §9.3 | fake-TLS `ClientHello` preamble for `0xEE` ≥21-byte secrets | region-blocked networks; not in scope for v1 | ~300 LOC of TLS record construction that we never parse (server strips it) |

Five top-level sections are affected by these gaps: §6 (G3), §7 (G1),
§9 (G2 + G3), §10 (G1 + G5), §12 (G4). The other 18 top-level
sections are fully covered.

Plus three gaps that are explicitly out of the MTProto scope (or
already-handled by separate modules in the new crate):

| Gap | Spec section | What grammers lacks | Impact on cipherocto |
|-----|---------------|----------------------|----------------------|
| **G4** | §12 | HTTP long-poll transport | n/a (TCP works) |
| **G5** | §10.9 | CDN config + dedicated file loader | n/a (we don't download CDN media) |
| **G6** | n/a (Bot-API is out of MTProto scope) | The `https://api.telegram.org/bot{token}/...` HTTP API | the **Bot-API HTTP fallback** is a separate, opt-in module in the new crate |

### 4. The Bot API fallback

The Bot API at `https://api.telegram.org/bot{token}/{method}` is
HTTP-only and bot-only. It is not part of MTProto and is not part of
`mtproto_port.md`. However, for cipherocto users in region-blocked
networks where Telegram's DCs (on `149.154.175.x:443` etc.) are
unreachable but the api.telegram.org HTTPS endpoint is reachable
(some networks treat these differently), the Bot API is a viable
fallback.

The Bot API has a much smaller surface than MTProto. The cipherocto
new crate can implement it as a small `http_fallback` module:

- `bot.sendMessage(chat_id, text)` → `POST /bot{token}/sendMessage`
- `bot.sendDocument(chat_id, file)` → `POST /bot{token}/sendDocument`
- `bot.getUpdates(offset, timeout)` → long-poll for updates

This is the same Bot-API path that the existing TDLib-based
adapter exposes, preserved as an opt-in module in the new crate.
It is **opt-in** behind a `--transport http` flag, **not** the default.

### 5. cipherocto integration: what the new crate must provide

The cipherocto Telegram contract is defined by RFC-0850 §8.2 (the
`PlatformAdapter` trait) and the `0850p-*` family (group binding,
DC-initiated group creation, kick detection, group decommission —
group-binding is transport-agnostic; the rest are Telegram-specific).
This section maps every cipherocto-required surface to a
corresponding grammers API call or Bot-API HTTP call.

#### 5.1 `PlatformAdapter` trait (RFC-0850 §8.2)

| Trait method | grammers call | Notes |
|--------------|---------------|-------|
| `send_envelope(domain, envelope)` | `client.send_message(chat, text)` for ≤4096 chars; `client.send_file(chat, file)` for >4096 | The DOT envelope is base64-encoded (URL_SAFE_NO_PAD) and sent as a Telegram message; >4096 chars is uploaded as a file with the encoded envelope as the caption. |
| `receive_messages(domain)` | `client.next_update().await` (one-at-a-time) or `client.stream_updates()` (mpsc stream) | Bridge to a `mpsc::Receiver<RawPlatformMessage>`; `receive_messages` drains the channel and returns a batch. This is the same stream-to-batch work that `social-platform-transport-patterns.md` §1.5 already proposes. |
| `canonicalize(raw)` | `Update → DeterministicEnvelope` (pure function) | Pure translation; no I/O. |
| `capabilities()` | hardcoded (limits differ by transport: text 4096 chars on both; upload 50 MB on Bot API, 2 GB on MTProto; download 2 GB on MTProto) | Matches `social-platform-transport-patterns.md` §1.3. |
| `domain_id(chat_id)` | `BLAKE3("telegram:{chat_id}")` | Identical to existing. |
| `platform_type()` | `PlatformType::Telegram` (the exact discriminant value is TBD; see the cipherocto source) | — |
| `replay_protection` | `MessageBox` (grammers internal) + `DotGateway` replay cache | The two are independent layers; `MessageBox` is per-MTProto-session, the cipherocto cache is per-DOT-domain. |
| `health_check` | `client.is_authorized()` | — |
| `shutdown` | `client.sign_out()` then drop the `Client` | — |
| `self_handle` | `client.get_me()` returns `User` with `id()` | Same as existing. |
| `upload_media_to_domain` | `client.send_file(chat, path)` | — |
| `download_media` | `client.download_file(input_location)` | grammers returns `Vec<u8>`; for >50 MB media the new crate will need a streaming wrapper. |

**Net:** every trait method has a grammers analog with at most ~50 LOC
of glue per method (the envelope encoder/decoder in `envelope.rs` is
~200 LOC; the rest is much smaller). The only architectural shift is
the stream-to-batch bridge in `receive_messages`, which is a one-file
change.

#### 5.2 Group binding (RFC-0850p-c)

`PlatformAdapter::send_envelope` already routes to a chat_id
configured in the adapter's `groups` map. The new crate inherits this
mechanism; the underlying `chat_id` is unchanged. Group binding at
the protocol level (resolving chat_id from a t.me link, joining a
group, etc.) is a separate concern in the `0850p-c` mission; grammers
provides `Client::join_chat(...)`, `Client::import_chat_invite(...)`,
and `Client::get_chat(...)` for this.

#### 5.3 DC-initiated group creation (RFC-0850p-d), kick detection
(0850p-e), group decommission (0850p-f)

These are draft RFCs and have no implementation yet. The new crate
should expose grammers' `Client::create_group(...)`,
`Client::delete_chat(...)`, and `Client::kick_participant(...)` as
the underlying primitives; the draft missions will build on top.

#### 5.4 Bot mode vs user mode

`mtproto_port.md` describes both:

- **Bot mode**: `client.sign_in_bot(token)`. Returns a `User` with
  the bot's id. The full TL API is reachable **except** for
  user-facing methods (no `getDialogs`, no `getHistory` for full sync,
  no `messages.search` global). The existing TDLib-based
  adapter's Bot-API code path falls into this category.
- **User mode**: `client.sign_in_user(...)` → receive SMS code
  → `client.check_auth_code(code)` → optional 2FA via
  `client.check_2fa_password(pwd)`. **Or** `client.qr_login()` for
  QR-based login (the QR flow is part of the cipherocto Telegram
  auth onboarding flow in `RFC-0850ab-a`). Returns a `User` with
  the user's id. The full TL API is reachable.

For cipherocto, **bot mode is the right primary**: a DOT gateway
talks to a bot account per group; there is no SIM swap risk; the
onboarding is just "paste the bot token from BotFather". **User mode
is the right escape hatch** for features Telegram forbids for bot
accounts (full dialog sync, large media, certain group admin
actions). The new crate supports both behind a config flag.

#### 5.5 Session storage

`mtproto_port.md` §7 specifies the encryption primitives (the 256-byte
`AuthKey` and its 8-byte `key_id` derived from `SHA1(key)[12..20]`),
and §8.5 covers the auth_key handshake completion step. The full
auth_key lifecycle is distributed across §7, §8, and §10 (session
state). The cipherocto new crate stores:

- The `AuthKey` (managed by grammers' `SqliteSession`).
- The `user_id` and `is_bot` flag (managed by `SqliteSession`).
- The home DC id (managed by `SqliteSession`).
- cipherocto-specific config: `chat_id → BroadcastDomainId` mapping
  (cipherocto's own table).
- DOT envelope replay cache (cipherocto's own table, separate
  concern).

`SqliteSession` and the cipherocto tables live in the same SQLite
file under separate table prefixes. This pattern is already used by
the matrix adapter's session-store crate; the new crate mirrors it
(the exact crate name is TBD; the matrix precedent confirms the
pattern).

### 6. Architecture: the new crate

A fresh crate, `octo-adapter-telegram-mtproto`, structured as four
layers:

```
crates/octo-adapter-telegram-mtproto/
├── Cargo.toml              ← grammers-mtproto, grammers-tl-types, grammers-client,
│                             grammers-session (sqlite feature), grammers-crypto,
│                             tokio, reqwest (Bot-API fallback),
│                             blake3, base64, async-trait, octo-network
├── src/
│   ├── lib.rs              ← re-exports + PlatformAdapter dispatch
│   ├── adapter.rs          ← PlatformAdapter impl (MTProto primary, HTTP fallback)
│   ├── mtproto_client.rs   ← grammers Client wrapper
│   ├── http_fallback.rs    ← Bot-API HTTP path (preserved from the existing TDLib-based adapter)
│   ├── auth.rs             ← sign_in / check_2fa_password / qr_login
│   ├── config.rs           ← TelegramConfig (api_id, api_hash, bot_token, data_dir)
│   ├── envelope.rs         ← DOT wire format (base64 URL_SAFE_NO_PAD; the
│   │                         exact byte layout is defined in `octo-network`
│   │                         and shared with the other adapters)
│   ├── error.rs            ← TelegramError / Result
│   ├── self_handle.rs      ← self-loop filter
│   ├── groups.rs           ← chat discovery
│   ├── cleanup.rs          ← graceful shutdown
│   └── files.rs            ← upload/download via grammers
├── tests/                  ← integration tests against test DC
└── examples/               ← example binaries
```

```
┌─────────────────────────────────────────────────────────────────┐
│                       DotGateway (RFC-0850)                      │
│   version check → signature → replay → flags → forward          │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│           octo-adapter-telegram-mtproto (fresh crate)           │
│                                                                  │
│  adapter.rs                                                      │
│  ├── PlatformAdapter impl (bot + user + DOT envelope)            │
│  │                                                               │
│  │   ┌──────────────────────┐    ┌─────────────────────────┐    │
│  │   │ mtproto_client.rs    │    │ http_fallback.rs        │    │
│  │   │ (grammers wrapper,   │    │ (Bot-API HTTP, opt-in,  │    │
│  │   │  default transport)  │    │  region-blocked users)  │    │
│  │   └──────────┬───────────┘    └──────────┬──────────────┘    │
│  │              │                            │                  │
│  └──────────────┼────────────────────────────┼──────────────────┘
│                 │                            │
│                 ▼                            ▼
│        ┌──────────────────┐         ┌─────────────────┐
│        │  grammers 0.9.0  │         │  reqwest →      │
│        │  (pure Rust,     │         │  api.telegram   │
│        │   8 crates)      │         │  .org           │
│        └──────────────────┘         └─────────────────┘
│                 │
│                 ▼
│        ┌──────────────────┐
│        │  Telegram DCs    │
│        │  (TCP 443)       │
│        └──────────────────┘
└─────────────────────────────────────────────────────────────────┘
```

The new crate is self-contained. The DOT wire format,
`domain_id`, and `self_handle` are shared with the rest of the
cipherocto adapters via the existing `octo-network` crate (no new
crate is needed for these).

### 7. Implementation considerations

#### 7.1 What we build

The new crate's source tree maps to four concerns:

| Concern | LOC estimate | Module |
|---------|--------------|--------|
| PlatformAdapter impl + envelope codec | ~600 | `adapter.rs`, `envelope.rs` |
| grammers wrapper (MTProto transport) | ~1500 | `mtproto_client.rs`, `auth.rs`, `files.rs` |
| Bot-API HTTP fallback | ~400 | `http_fallback.rs` |
| Config / errors / self-loop filter / chat discovery | ~500 | `config.rs`, `error.rs`, `self_handle.rs`, `groups.rs` |
| Tests + examples | ~1000 | `tests/`, `examples/` |
| **Total** | **~4000** | |

The LOC budget is a first-cut estimate; actuals will vary with the
final module boundaries.

#### 7.2 What we don't build

- **MTProto itself** — grammers provides it.
- **TL type generation** — `grammers-tl-gen` does it from upstream
  `api.tl` + `mtproto.tl`.
- **New cryptographic primitives** — `grammers-crypto` provides
  AES-IGE, RSA, SHA.
- **A TDLib replacement layer** — the new crate is its own thing; it
  does not wrap or replace the TDLib-based adapter. They live
  alongside each other.

#### 7.3 The 3 small gaps and how to handle them

For each of G1, G2, G3 (above), the recommended approach is **a
small wrapper, not a grammers fork**:

- **G1 (old-MTP1 `bind_auth_key_inner`):** skip. cipherocto does
  not need temp keys; the 24h-validity temp key path is used by
  tdlib/tdesktop for CDN file downloads/uploads, web previews,
  payments, and other bandwidth-heavy operations that benefit
  from cheap per-request auth. cipherocto uses long-lived auth
  keys and direct file uploads. If a future cipherocto use case
  does need temp keys, the wrapper is ~200 LOC of AES-IGE +
  4-round SHA-1 derivation.

- **G2 (SOCKS5 / HTTP CONNECT):** the wrapper pre-establishes
  the TCP connection through SOCKS5 or HTTP CONNECT and hands
  the resulting `tokio::net::TcpStream` to grammers' transport
  (rather than letting `transport::Tcp::connect(...)` open its
  own connection). The `tokio-socks` crate does the SOCKS5 part.
  The HTTP CONNECT part is ~50 LOC using
  `tokio::io::AsyncWriteExt`. Total: ~200 LOC.

- **G3 (fake-TLS `0xEE` ClientHello):** the wrapper constructs a
  fake-TLS `ClientHello` record with the `0xEE` secret's `secret[1..17]`
  as the AES key material and `secret[17..]` as the SNI domain.
  The `tls_block_*` constants from the `mtproto.tl` schema (the
  same ones tdesktop uses, near the `ProxyInfo` constructors)
  describe the record layout. The cipherocto wrapper does not need
  to parse the response — the MTProxy server strips the preamble
  and forwards the rest. Total: ~300 LOC.

These three wrappers, if and when needed, total ~700 LOC. None is
on the critical path for the cipherocto v1 adapter.

#### 7.4 Build / test / deploy

- **Build time:** pure-Rust, no C++. `cargo build` of the new
  crate is dominated by `grammers-tl-types` codegen (a few
  minutes on cold cache, then incremental on rebuild).
  Cross-compilation is straightforward.
- **Cross-compilation:** straightforward. The crate builds on
  `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`,
  `x86_64-pc-windows-msvc`, etc. with no platform-specific setup.
- **CI:** standard `cargo test` + `cargo clippy --all-targets -- -D warnings`
  + `cargo fmt --all --check`. No `build.rs` is needed because no
  third-party binary is downloaded.
- **Mobile/web:** the same pure-Rust core compiles to iOS and
  Android (via NDK) with `grammers-tl-types` and `grammers-crypto`
  (sans-IO). The network and session crates need minor adapter
  work for non-Tokio runtimes; WASM is not in scope for v1
  (grammers-mtproto and grammers-mtsender are Tokio-bound and
  would need a runtime adapter). This is a future opportunity,
  not in scope for v1.

---

## Recommendations

1. **The new crate is feasible.** All 23 top-level sections of
   `mtproto_port.md` have a corresponding grammers implementation;
   5 of 23 have sub-row gaps (3 protocol-level cipherocto could
   wrap, 2 protocol-level cipherocto does not need). `G6`
   (Bot-API HTTP) is out of MTProto scope and is handled as a
   separate opt-in fallback. All gaps are non-blocking.
2. **Adopt grammers as the new crate's MTProto layer.** It is the
   single mature pure-Rust choice; `dgrr/tgcli` is the production
   validation; the architectural alignment (async-native Tokio) is
   strictly better than tdesktop's thread-per-DC for cipherocto.
3. **Build the new crate around four layers:** grammers (MTProto),
   a thin `PlatformAdapter` glue layer, a shared DOT wire-format
   codec, and an opt-in Bot-API HTTP fallback. The four layers
   correspond to the four modules in §6.
4. **Implement bot mode first.** Bot accounts are the easy path
   (one bot token per group; no SIM swap risk; full TL API except
   for user-only methods). User mode is the escape hatch for
   features Telegram forbids for bots.
5. **Ship the Bot-API HTTP fallback as an opt-in module.** It is
   the right answer for region-blocked users where MTProto is
   unreachable but `api.telegram.org` is reachable.
6. **Do not try to extend grammers for the 3 protocol gaps**
   (`G1`, `G2`, `G3`). Write small wrappers around it; the gaps
   are non-blocking and each is <300 LOC. `G4` and `G5` are
   skipped (cipherocto does not need HTTP long-poll or CDN media);
   `G6` is a separate opt-in module in the new crate and is not
   addressed by extending grammers either.
7. **Trust upstream grammers by default, with a vendoring
   contingency.** The library is one-maintainer but well-maintained
   (2026-05-15 release; production users in `dgrr/tgcli`). If
   upstream goes dormant for >6 months, vendor it under
   `crates/octo-grammers-vendored` (a proposed path; the exact
   naming is up to the implementing mission) with a `vendored`
   feature flag.
8. **Run the 23-section spec checklist** at the start of every
   cipherocto Telegram mission. The table in §3 is the canonical
   reference; a row that loses its `✓` verdict (i.e. a section
   where grammers no longer fully covers the spec) is a regression.
9. **The new crate lives alongside the existing TDLib-based
   adapter.** Both ship; both are maintained; the new one is the
   recommended default. The choice is the user's.

### What we are NOT recommending

- **A custom MTProto implementation from scratch.** grammers is
  already correct; re-implementing it is years of work for no gain.
- **A different MTProto library.** There is no other mature
  pure-Rust option.
- **Using Bot-API HTTP as the primary transport.** Bot-API is
  HTTP-only, bot-only, and lacks the full TL API. It is a
  fallback, not the default.
- **Changing the DOT wire format.** The DOT envelope (defined in
  `octo-network` and shared with all cipherocto adapters) is the
  contract with the `DotGateway`; it is not the new crate's
  concern.

---

## Next Steps

- [x] Research complete (this document)
- [ ] Submit for review under
      `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md`
- [ ] If accepted → Create Use Case at
      `docs/use-cases/pure-rust-telegram-transport.md`
- [ ] Create RFC at
      `rfcs/draft/networking/0850ab-c-pure-rust-mtproto-telegram-adapter.md`
      (or amend RFC-0850ab-a)
- [ ] Create mission at
      `missions/open/0850ab-c-pure-rust-mtproto-telegram-adapter.md`
- [ ] Update `docs/research/README.md` with this report

### Open questions for the Use Case

1. **Bot mode default.** Should the new crate default to **MTProto**
   or **Bot-API HTTP** for bot accounts? Recommendation: **MTProto**
   for parity, with HTTP as the fallback.
2. **Vendoring timing.** Vendor grammers immediately, or wait for
   the first release that breaks cipherocto? Recommendation:
   **trust upstream**; vendor if upstream goes dormant for >6 months.
3. **Session storage location.** Same SQLite DB as grammers'
   `SqliteSession` or a separate cipherocto DB? Recommendation:
   **same DB, separate table prefix**, mirroring the matrix
   adapter's session store.
4. **Multiple accounts per process.** grammers supports this
   natively (one `Client` per account). Should the new crate
   expose it? Recommendation: **yes**, via a `Vec<Arc<...>>` of
   `Client` handles in the adapter, exposed through the existing
   `TelegramConfig` extension.
5. **CDN media (Gap G5).** Skip for v1, or add a small wrapper in
   a later phase? Recommendation: **skip** — no cipherocto use
   case today requires CDN media.
6. **DC migration handling.** When the auth-key's home DC moves
   (Telegram rebalancing), the new crate must re-bind to the new
   DC. grammers handles this internally; the cipherocto
   `health_check` may need to surface a "DC migrating" signal.
7. **FLOOD_WAIT and rate limits.** grammers returns
   `FloodWaitError` on `FLOOD_WAIT_X` responses; the cipherocto
   adapter should either pause-and-retry internally or surface the
   wait time to the `DotGateway`. The matrix adapter uses the
   former; the new crate should match.
8. **MTProxy support (Gap G3).** fake-TLS `0xEE` is the only
   MTProxy variant not in grammers' public API. If cipherocto
   users behind region-blocking firewalls become a real
   population, the small wrapper (~300 LOC, see §7.3) lands in a
   later mission. v1 ships without it.

### Related research / RFCs / missions

- `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` —
  the parent RFC for the DOT (transport adapters are one family
  within it).
- `rfcs/accepted/networking/0850ab-a-telegram-auth-onboarding.md` —
  the cipherocto Telegram auth onboarding RFC.
- `rfcs/accepted/networking/0850p-a-whatsapp-auth-onboarding.md` —
  the WhatsApp analog.
- `rfcs/accepted/networking/0850p-c-transport-group-binding.md` —
  group binding (transport-agnostic).
- `rfcs/draft/networking/0850p-d.md`, `0850p-e.md`, `0850p-f.md` —
  the three draft RFCs for DC-initiated group creation, kick
  detection, and group decommission. The exact filenames in
  `rfcs/draft/networking/` should be confirmed before referencing
  from a downstream doc; this research lists them as
  `0850p-{d,e,f}.md` based on the cipherocto RFC naming convention.
- `docs/research/social-platform-transport-patterns.md` — the
  2026-05-28 transport research that first enumerated the
  20-adapter landscape.
- `docs/research/group-coordination-transport-adapters.md` — the
  2026-06-17 follow-up that audited the 20 adapters.
- `docs/plans/2026-05-31-matrix-rust-sdk-migration.md` — the closest
  precedent: a pure-Rust migration of a non-pure-Rust adapter.

---

## References

### cipherocto (this repo)

- `crates/octo-adapter-telegram/` — the existing TDLib-based
  adapter; lives alongside the new crate.
- `crates/octo-network/src/dot/adapters/mod.rs` — `PlatformAdapter`
  trait (RFC-0850 §8.2).
- `crates/octo-network/src/dot/fragment.rs` — DOT wire-format
  handling (the exact purpose of the `fragment` module is to be
  confirmed in the cipherocto source; it is part of the shared
  DOT layer).
- `rfcs/accepted/networking/0850-deterministic-overlay-transport.md`.
- `rfcs/accepted/networking/0850ab-a-telegram-auth-onboarding.md`.
- `rfcs/accepted/networking/0850p-c-transport-group-binding.md`.
- `docs/research/social-platform-transport-patterns.md`.
- `docs/research/group-coordination-transport-adapters.md`.
- `docs/plans/2026-05-31-matrix-rust-sdk-migration.md`.

### External

- `/home/mmacedoeu/_w/tools/tdesktop/docs/mtproto_port.md` — the
  23-section protocol reference (in-tree).
- `codeberg.org/vilunov/grammers` — primary grammers repo (last
  commit 2026-05-15).
- `github.com/Lonami/grammers` — github mirror.
- `github.com/overrealdb/grammers` — third-party fork.
- `crates.io/crates/grammers-mtproto` (0.9.0),
  `grammers-tl-types` (0.9.0), `grammers-client` (0.8.x).
- `docs.rs/grammers-mtproto` — current API reference.
- `deepwiki.com/Lonami/grammers/3-core-architecture` — architecture
  deep-dive.
- `github.com/dgrr/tgcli` — production pure-Rust CLI on top of
  grammers.
- `github.com/dgrr/tgcli-go` — the Go/TDLib version that dgrr
  explicitly contrasts tgcli against.
- `core.telegram.org/mtproto` — official MTProto spec.

### Anti-references (libraries we considered and rejected)

- `tdlib-rs` 1.4.x — TDLib FFI (a C++ alternative that lives
  alongside the new crate).
- `teloxide` — Bot-API framework, too heavy.
- `MadelineProto` — PHP, wrong language.
- `WTelegramClient` — .NET, wrong language.
- `mini-telegram` — server-side, wrong direction.
