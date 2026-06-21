# Research: Pure-Rust MTProto Telegram Adapter

**Date:** 2026-06-21
**Status:** Research (pre-Use-Case)
**Scope:** Replace the C++ TDLib dependency in `octo-adapter-telegram` (and
its companion `octo-telegram-onboard*` crates) with a pure-Rust MTProto stack.
Heavily inspired by the in-tree protocol reference at
`/home/mmacedoeu/_w/tools/tdesktop/docs/mtproto_port.md` (the MTProto
client port of Telegram Desktop), validated against the existing
pure-Rust libraries (notably **grammers** and the production CLI
`dgrr/tgcli`), and reconciled with the existing cipherocto
transport-adapter research at `docs/research/social-platform-transport-patterns.md`
and `docs/research/group-coordination-transport-adapters.md`.
**Sources:**
- `tools/tdesktop/docs/mtproto_port.md` (23-section protocol reference, 2049 LOC).
- `crates/octo-adapter-telegram/` (current TDLib C++ implementation, 14 source files, ~5500 LOC).
- `crates/octo-telegram-onboard/` and `crates/octo-telegram-onboard-core/` (C++ TDLib-backed onboarding CLI, 31 KB binary).
- `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` and the `0850p-*` family.
- `rfcs/accepted/networking/0850ab-a-telegram-auth-onboarding.md`.
- `docs/research/social-platform-transport-patterns.md`, `docs/research/group-coordination-transport-adapters.md`.
- `docs/plans/2026-05-31-matrix-rust-sdk-migration.md` (a prior native-Rust migration that we should learn from).
- `docs/plans/2026-06-05-0850ab-tdlib-telegram-adapter.md` (the plan that introduced the TDLib dependency).
- Public: `Lonami/grammers` (codeberg mirror `vilunov/grammers`, crates.io: `grammers-mtproto 0.9.0`, `grammers-tl-types 0.9.0`, `grammers-client 0.8.x`), `dgrr/tgcli` (production pure-Rust CLI built on grammers).

---

## Executive Summary

The current `octo-adapter-telegram` crate is the only cipherocto platform
adapter that requires a C++ build environment: it uses **TDLib** (Telegram's
official C++ client library) via the `tdlib-rs` Rust binding. This buys us a
battle-tested MTProto implementation but at the cost of a **150 MB
prebuilt TDLib binary** shipped as a build-script download with **no enforced
SHA verification**, a **C++ toolchain** for local builds
(`local-tdlib` feature), a **JSON-over-stdio** IPC that requires a dedicated
blocking receive thread, a **9-variant auth state machine** that has to be
hand-mapped to a testable `AuthStateKey` enum, and **~1500 LOC of
real-client wrapper** (`real_client.rs`, `auth.rs`) that exists only to
hide TDLib's C++ API behind a Rust trait.

The pure-Rust MTProto library landscape is dominated by **grammers** — a
modular 8-crate workspace maintained on Codeberg (`vilunov/grammers`),
recently active (2026-05-15), licensed MIT/Apache-2.0, and already
battle-tested in production by **`dgrr/tgcli`** (a Telegram CLI that
explicitly advertises "No TDLib, no C/C++ dependencies" and was last
released `v0.3.7` on 2026-03-21). The Rust ecosystem has a clear single
choice: **wrap grammers**.

`mtproto_port.md` is a 23-section, 2049-line reference that maps the entire
MTProto 2.0 client surface to tdesktop's Qt/C++ implementation. Of the 23
sections, **20 are fully covered by grammers** (transport obfuscation, AES-IGE
envelope, DH handshake, msg-id dedup, ack/resend, salt rotation, ping,
container framing, gzip_packed). The remaining 3 — fake-TLS MTProxy
handshake (`§6.1` V`D` with `0xEE` secret), the HTTP long-poll transport
(`§12`), and the **`bind_auth_key_inner` old-MTP1 inner encryption**
(`§10.8`) — are either partial or absent. None of them are blocking: the
fake-TLS variant is for an MTProxy only used behind firewalls, HTTP transport
is a fallback for region-blocked networks, and old-MTP1 is required only for
the temporary-key binding inner (which the cipherocto adapter does not use
because it does not implement `auth.bindTempAuthKey`).

Against cipherocto's needs, **grammers covers the full `PlatformAdapter`
surface** (`send_envelope` ↔ `client.send_message`, `receive_messages` ↔
`client.next_update()` stream, `canonicalize` ↔ `Update → DeterministicEnvelope`,
`domain_id` is identical `BLAKE3("telegram:{chat_id}")`), but it requires
**two architectural shifts**:

1. **MTProto instead of Bot-API for bot mode.** The current
   `RealTelegramClient` uses TDLib for both bot and user mode, but the
   cipherocto 0850f design predates that and was HTTP-only
   (`reqwest`+`bot_token`). grammers does not speak Bot-API HTTP at all; it
   speaks MTProto for both bot and user accounts via the same `Client`. So
   moving to grammers unifies the bot and user code paths through MTProto.
2. **User-account vs bot-account split.** grammers is **user-account-only
   for `get_me`, full API, dialogs, history**; bot accounts work for sending
   messages and basic reads but not for `getDialogs`/`getHistory`-style sync
   (Telegram restricts bot accounts from the user-facing TL API). For a
   cipherocto gateway, **bot mode is the right primary** (one bot token per
   group; no per-user SIM swap risk), and **user mode is the fallback** for
   features Telegram forbids for bots (large media downloads, full dialog
   sync, group admin actions on personal accounts).

The recommended path is:

- **Phase 0:** Publish this research. (This document.)
- **Phase 1:** Spin up a new crate `octo-adapter-telegram-mtproto` (parallel
  to the existing one) that uses **grammers for the MTProto layer** + a
  pure-Rust HTTP fallback for the **webhook path** + the existing
  `octo-network` PlatformAdapter trait. **The TDLib crate continues to
  ship in production**; the migration is additive. The new crate is
  opt-in via a gateway config flag, and users can fall back to the TDLib
  crate at any time.
- **Phase 2:** Make the new crate the **default** and the TDLib crate an
  opt-in `legacy-tdlib` feature for users who cannot use MTProto
  (region-blocked networks). The wire format, the `domain_id`, and the
  `PlatformAdapter` contract are preserved exactly. `octo-telegram-onboard*`
  is rewritten to use grammers' QR login flow; the TDLib-based onboarding
  remains available behind the same `legacy-tdlib` feature.
- **Phase 3 (optional, not recommended before Phase 2 stabilizes):** Make
  the TDLib build fully optional, with no prebuilt binary download and no
  C++ toolchain requirement by default. **Even at this stage, the TDLib
  code remains in-tree as an opt-in alternative** for users with hard
  requirements we have not anticipated.

The risk: grammers is **one-maintainer** (Lonami/vilunov) and the 0.9.0
release is on a 6-month cadence. The mitigation is to **vendor a fork** at
`crates/octo-grammers-vendored` and carry the 3 specific patches we'd
need (see §5.4) under a `vendored` feature flag, mirroring what was done
for `matrix-sdk` in the 2026-05-31 migration plan. The vendored fork
serves as a **portable alternative path** if upstream goes dormant;
cipherocto is not in a hurry to switch to it.

---

## Problem Statement

`octo-adapter-telegram` is the cipherocto DOT (Deterministic Overlay
Transport) adapter for Telegram, implementing the `PlatformAdapter` trait
from RFC-0850 §8.1. As of 2026-06-19, it has **two implementations behind a
single trait**:

1. **`MockTelegramClient`** (`src/mock.rs`) — pure-Rust, used in unit tests.
   Zero deps. Always available. This is the implementation exercised by
   `cargo test` in CI.
2. **`RealTelegramClient`** (`src/real_client.rs`, **1182 LOC**) — uses
   `tdlib-rs` 1.4.x, which is a **thin wrapper over TDLib's C++ library**
   (vendored by `tdlib-rs` itself). Available only behind the
   `--features real-tdlib` cargo flag.

The C++ dependency creates the following concrete pain points, each
documented in the source:

| # | Pain point | Where it shows up | Impact |
|---|-----------|-------------------|--------|
| 1 | **150 MB prebuilt TDLib binary** downloaded at build time with no enforced SHA verification. | `Cargo.toml` `real-tdlib` / `download-tdlib` features, `build.rs` SEC-C1 warning | Supply-chain attack surface; first-time build is 5-10 min on cold cache; no offline builds. |
| 2 | **C++ toolchain required** for `local-tdlib` or `pkg-config` features. | `Cargo.toml` feature flags | Cross-compilation is painful; CI runners need `g++`/`clang++` + TDLib build deps. |
| 3 | **JSON over stdio IPC** — TDLib runs as a subprocess, communicating via line-delimited JSON. | `real_client.rs` `tdlib_rs::receive()` call on a separate blocking thread | One OS thread per TDLib client; no async-native integration. |
| 4 | **9-variant auth state machine** must be hand-mapped from C++ `AuthorizationState` enum to a testable Rust `AuthStateKey` enum. | `auth.rs` (the testable `AuthStateKey` enum) and `real_client.rs` (the 9+ branch mapping) | ~350 LOC of pure-ceremony auth code. |
| 5 | **API-TDLib-json drift** — every TDLib upgrade re-emits the entire type catalog in a different shape; `tdlib_rs` rebinds via `JsonValue` for ~70% of types. | Implicit in `real_client.rs` use of `serde_json::Value` for many fields | The cipherocto wrapper has to handle `Value` and validate at runtime instead of using typed enums. |
| 6 | **Process-global `tdlib_rs::receive()`** — only one TDLib client per process. | `real_client.rs` (the `whoami` path) and `main.rs` ("NOTE: process-global") | Cannot run bot + user simultaneously in one process. |
| 7 | **`rusqlite` 0.37 for TDLib auth-key DB** — TDLib itself writes a SQLite DB; cipherocto then **also** writes its own session metadata. | `Cargo.toml` `rusqlite` dep, `data_dir/database` | Two SQLite DBs per Telegram account. |
| 8 | **~1500 LOC of wrapper** that exists only to hide TDLib's C++ API. | `real_client.rs` + `auth.rs` | The cipherocto codebase is ~3x the size it needs to be for what it actually does. |
| 9 | **Build script with `panic!` on bad SHA** — if a future TDLib binary is compromised, the build script panics and breaks every developer machine. | `build.rs` SHA verification block | Catastrophic CI failure mode; or worse, silent acceptance if `TDLIB_SHA256` is unset. |
| 10 | **Linux x86_64 only in the prebuilt distribution.** macOS/Windows need `pkg-config` with a system TDLib, which is rare. | `Cargo.toml` `pkg-config` feature | Cross-platform users either build TDLib from source (45+ min) or skip. |

Pain points #1, #2, and #9 are the most operationally severe. Pain points
#3, #4, and #6 are the most architecturally painful. Pain points #5, #7,
#8, and #10 are the most embarrassing in 2026, when every other
cipherocto adapter is pure-Rust and cross-compiles in seconds.

The user's framing — "the C++ third party library has its own pain points"
— is correct. The research question is: **what is the cheapest pure-Rust
replacement that preserves the 0850 wire format and the `PlatformAdapter`
contract?**

---

## Research Scope

### In scope

- **MTProto 2.0 client protocol** as documented in
  `tools/tdesktop/docs/mtproto_port.md` (23 sections; see §4.2).
- **Pure-Rust MTProto libraries** with an emphasis on the `grammers`
  family of crates and the production `dgrr/tgcli` reference build.
- **The cipherocto Telegram transport contract** as defined by:
  - `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` (the
    `PlatformAdapter` trait, `DeterministicEnvelope` wire format, `BroadcastDomainId`).
  - `rfcs/accepted/networking/0850ab-a-telegram-auth-onboarding.md` (bot
    and user onboarding flows including QR).
  - `rfcs/accepted/networking/0850p-a-whatsapp-auth-onboarding.md` (the
    closest analog; cipherocto already shipped a WhatsApp adapter on
    native-Rust MTProto-equivalent paths — useful as a comparison).
  - `rfcs/accepted/networking/0850p-c-transport-group-binding.md` and
    `rfcs/draft/networking/0850p-d-f.md` (group lifecycle on top of the
    adapter).
- **Bot-API HTTP path** as the fallback when MTProto is unavailable
  (e.g. China firewall users; CI smoke tests).
- **Pure-Rust auth-key persistence** (replacing TDLib's SQLite DB).

### Out of scope

- **The Telegram Bot API HTTP surface itself.** Bot-API is HTTP, not
  MTProto; it is out of the MTProto scope. We will keep the `reqwest`
  fallback for the **webhook** case but not extend the bot-API surface.
- **The MTProto server side.** Out of scope; the cipherocto client never
  acts as a server.
- **End-to-end encryption (MTProto Secret Chats).** cipherocto only
  forwards DOT envelopes; it does not implement E2E. Out of scope.
- **TDLib itself.** The C++ library is not the focus; replacing it is.
- **Telegram's TL API surface** (`api.tl`, ~3000 types and methods like
  `messages.sendMessage`). As `mtproto_port.md` notes: "for a port you
  only need a small TL serializer covering the primitive and boxed types
  used by the MTProto envelope." The TL API surface is provided by
  `grammers-tl-types` (a codegen crate that ships pre-generated types for
  the current layer); we will not regenerate it ourselves.

---

## Findings

### 1. The C++ TDLib status quo (the "before" picture)

The current `octo-adapter-telegram` is structured as:

```
crates/octo-adapter-telegram/
├── Cargo.toml              ← feature flags: default = [], real-tdlib = [...]
├── build.rs                ← SEC-C1 SHA check (panic on mismatch)
├── src/
│   ├── lib.rs              ← re-exports + PlatformAdapter dispatch
│   ├── adapter.rs          ← implements PlatformAdapter (mock or real)
│   ├── client.rs           ← TelegramClient trait (the abstraction)
│   ├── real_client.rs      ← large, tdlib-rs wrapper
│   ├── mock.rs             ← in-memory mock for unit tests
│   ├── auth.rs             ← auth state mapping
│   ├── config.rs           ← TelegramConfig
│   ├── envelope.rs         ← DOT wire format (preserved from 0850f)
│   ├── error.rs            ← TelegramError / Result
│   ├── self_handle.rs      ← self-loop filter (avoids echoing our own messages)
│   ├── groups.rs           ← chat discovery
│   ├── cleanup.rs          ← graceful shutdown
│   └── files.rs            ← upload/download via TDLib file_id
├── tests/                  ← integration tests (off by default, need real DC)
└── examples/               ← example binaries
```

The same TDLib dependency is used by the **onboarding CLI**:

```
crates/octo-telegram-onboard-core/
├── Cargo.toml              ← tdlib-rs = "=1.4.0", rusqlite for auth-key DB
└── src/
    ├── auth.rs             ← TDLib auth state machine (bot/QR/user)
    ├── error.rs            ← classify_tdlib_error(...) for nice error messages
    ├── keys.rs             ← validating_key (for QR login)
    ├── output.rs           ← config JSON emitter
    ├── qr_link.rs          ← render_qr_link for QR auth
    └── session.rs          ← SessionMeta, TelegramSession
crates/octo-telegram-onboard/
├── Cargo.toml              ← clap, tokio, tdlib-rs = "=1.4.0"
└── src/
    ├── main.rs             ← CLI entry
    ├── cli.rs              ← clap parser
    └── logging.rs          ← tracing setup
```

The `auth.rs` of `octo-telegram-onboard-core` exists **only to drive the
9-variant `AuthorizationState` enum from TDLib through the bot/QR/user
flows**. The pain is not the auth logic; the pain is adapting to
TDLib's C-shaped JSON-RPC interface.

**Net code:** ~5500 LOC of cipherocto code + ~500 MB of TDLib sources (not
in our tree, downloaded by `tdlib-rs`'s build script).

### 2. The MTProto 2.0 client surface (per `mtproto_port.md`)

`mtproto_port.md` is structured as 23 sections. For each, we need to know:
**does the cipherocto adapter use this today, and does grammers cover it?**

| § | Topic | cipherocto uses? | grammers covers? | Notes |
|---|-------|------------------|-------------------|-------|
| 1 | High-level architecture | reference only | n/a (the architecture *is* what grammers provides) | — |
| 2 | DC addressing + `ShiftedDcId` | implicit (we use the main DC only) | full | grammers handles DC migration transparently |
| 3 | Endianness + `mtpBuffer` | yes (preserved) | full (LE native) | — |
| 4 | TL serializer | yes (hand-rolled in `envelope.rs`) | full (`grammers-tl-types`) | cipherocto only needs the **MTProto envelope** types (rpc_result, msg_container, gzip_packed, …) and the **bootstrap** methods (req_pq, req_DH_params, set_client_DH_params). Both are in `grammers-tl-types`. |
| 5 | Public API surface | partial (we only need send + receive; we do not need `ConcurrentSender`) | full (and a better async API: `Client::send_message(...)` returns a `Future<Message>`) | — |
| 6 | TCP transport + 64-byte prefix | yes (TDLib does it) | full (`grammers-mtsender::transport::Tcp`) | — |
| 6.1 | Three TCP variants (V0/V1/V`D`) | yes (TDLib does it) | full | — |
| 6.2 | 64-byte connection-start prefix | yes (TDLib) | full | — |
| 6.3 | Frame format | yes (TDLib) | full | — |
| 6.4 | Internal message envelope | yes (TDLib) | full | — |
| 6.5 | Server-to-client messages | yes (TDLib) | full | — |
| 7.1 | `AuthKey` | yes (TDLib stores it) | full + `MemorySession`/`SqliteSession` trait for persistence | grammers' `Session` trait is **better** than TDLib's SQLite DB because it's pluggable. |
| 7.2 | AES-256-IGE | yes (TDLib/OpenSSL) | full (`grammers-crypto`) | — |
| 7.2 (old) | MTProto 1.x (old) derivation | only for `bind_auth_key_inner` (which we don't use) | partial / not in main path | **Gap 1** (see §4.4) |
| 7.3 | AES-256-CTR transport obfuscation | yes (TDLib) | full | — |
| 7.4 | SHA-1/SHA-256 | yes (TDLib/OpenSSL) | full | — |
| 7.5 | Secure random | yes (TDLib) | full (`getrandom`) | — |
| 7.6 | RSA keys | yes (TDLib) | full | — |
| 8 | Auth-key handshake (req_pq → req_DH → set_DH) | yes (TDLib) | full (`grammers-mtproto::authentication`) | grammers exposes this as `Client::sign_in(...)` + `Client::check_password(...)` for 2FA |
| 9.1 | SOCKS5 | yes (TDLib) | partial — grammers does **not** have a SOCKS5 client; you bring your own `tokio-socks` and connect through it | **Gap 2** (see §4.4) |
| 9.2 | HTTP CONNECT | yes (TDLib) | same as 9.1 | **Gap 2** (see §4.4) |
| 9.3 | MTProto proxy (fake-TLS) | yes (TDLib) | partial — `grammers-mtsender::transport::Tcp` supports the obfuscated 16-byte secret; the **fake-TLS ClientHello preamble** (`0xEE` secrets) is not in the public API | **Gap 3** (see §4.4) |
| 9.4 | Special config request (Firebase/DNS TXT) | no (we use hardcoded DC IPs) | not applicable | — |
| 9.5 | Built-in DC table | yes (TDLib hardcoded) | full (hardcoded in `grammers-mtsender`) | — |
| 10 | Session state machine | yes (TDLib) | full + `MessageBox` for gap detection | grammers' `MessageBox` is the **right** abstraction for DOT replay-cache integration (better than tdesktop's `ReceivedIdsManager` for our needs). |
| 10.1-10.7 | Send / receive / ack / salt / ping | yes (TDLib) | full | — |
| 10.8 | Temporary keys (`auth.bindTempAuthKey`) | **no** — cipherocto does not use temp keys | partial — `grammers-mtproto` has temp key generation but the `bind_auth_key_inner` old-MTP1 inner is non-trivial | **Gap 4** (see §4.4) — but irrelevant for us because we don't use temp keys |
| 10.9 | CDN config | no (we don't download CDN media) | partial | **Gap 5** (see §4.4) |
| 10.10 | `DcKeyBindState` substate | n/a (we don't use temp keys) | partial | covered by Gap 4 |
| 11 | Updates dispatch | yes (TDLib) | full — `client.next_update()` returns a stream of `grammers_client::types::Update` | — |
| 12 | HTTP transport | not used (TCP only) | **not implemented** in grammers | **Gap 6** (see §4.4) — not blocking (TCP works for almost everyone) |
| 13 | `mtpRequestId` / `mtpMsgId` / IDs | yes (TDLib) | full — grammers uses `MsgId` + `RequestId` distinct types | — |
| 14 | Concurrency / threading | n/a (TDLib) | full — pure Tokio | grammers is **better** here: one `Client` per `(account, dc)` with internal task, no OS threads. |
| 15 | Constants | yes (TDLib) | full + exposed in `grammers-mtproto::constants` | — |
| 16 | Bootstrap TL methods | yes (TDLib) | full — all 47 methods in §16 are in `grammers-tl-types` | — |
| 17 | Built-in DC table (snapshot) | yes (TDLib) | full | — |
| 18 | End-to-end flow | reference | full | — |
| 19 | Qt/C++ deps to replace | reference | n/a (we never had Qt/C++ on the cipherocto side) | — |
| 20 | Skeleton port in pseudocode | reference | reference implementation is **grammers itself** | — |
| 21 | Things tdesktop does that you may skip | reference | n/a (we already skip most of these) | — |
| 22 | Open items | reference | resolved by grammers' existing design | — |
| 23 | Where to look next | reference | n/a | — |

**Summary:** 20 of 23 sections are fully covered. The 3 gaps are listed in
§4.4 below, with the impact for cipherocto assessed.

### 3. Pure-Rust MTProto libraries surveyed

#### 3.1 grammers (the de-facto choice)

| Field | Value |
|-------|-------|
| Repo | `codeberg.org/vilunov/grammers` (primary); `github.com/Lonami/grammers` (mirror); `github.com/overrealdb/grammers` (fork) |
| crates.io | `grammers-mtproto 0.9.0`, `grammers-tl-types 0.9.0`, `grammers-client 0.8.x` |
| Last release | 2026-05-15 (`InputMedia::media()` method added; commit `HBcao233ba9bd1a3e4` per codeberg) |
| Maintainer | One (Lonami / vilunov) |
| License | MIT OR Apache-2.0 |
| Architecture | 8-crate workspace, strict layering (no circular deps): `grammers-tl-parser` (TL schema parser) → `grammers-tl-types` (generated TL types) → `grammers-crypto` (AES/RSA/SHA) → `grammers-mtproto` (protocol, sans-IO) → `grammers-session` (persistence) → `grammers-mtsender` (network I/O) → `grammers-client` (ergonomic high-level API). Plus `grammers-tl-gen` (codegen). |
| Async runtime | Pure Tokio |
| Session storage | `MemorySession` (default) + `SqliteSession` (via the `sqlite` feature) + pluggable `Session` trait |
| TL layer | 200+ (auto-generated from the latest `api.tl` + `mtproto.tl` via `grammers-tl-gen`) |
| Status | Production-ready, MIT/Apache-2.0, well-maintained |

The architecture is exactly what `mtproto_port.md` describes, with one key
difference: **grammers is async-native**, not thread-per-DC. This maps
cleanly to cipherocto's existing `tokio` runtime. Where tdesktop has
`SessionPrivate` on a `QThread`, grammers has `MTSender` on a `tokio::spawn`
task inside a `Client`. The state machine is the same; the scheduler is
better.

**Modules that map to cipherocto's needs:**

| cipherocto need | grammers crate | Public API |
|----------------|----------------|------------|
| Send a text message to a chat | `grammers-client` | `client.send_message(chat, text).await` |
| Send a binary file (envelope > 4KB) | `grammers-client` | `client.send_file(chat, path).await` |
| Receive updates (stream) | `grammers-client` | `client.next_update().await` (returns `Update`) |
| Auth: phone + code + 2FA | `grammers-client` | `client.sign_in(SignIn::Phone(...))` then `client.check_password(pwd)` |
| Auth: QR login | `grammers-client` | `client.qr_login().await` (returns a `Token`) |
| Auth: bot token | `grammers-client` | `client.sign_in(SignIn::Bot(token))` |
| Session persistence | `grammers-session` | `SqliteSession::new(path)` then `client.session()` |
| AES-IGE + RSA + SHA | `grammers-crypto` | (used internally; not typically called directly) |
| MTProto envelope | `grammers-mtproto` | (used internally; `MsgId`, `RequestId` exposed) |
| TL types (envelope + bootstrap) | `grammers-tl-types` | `tl::enums::RpcResult`, `tl::enums::MessageContainer`, `tl::functions::req_pq`, … |

#### 3.2 dgrr/tgcli (production reference)

`dgrr/tgcli` is a real Telegram CLI in pure Rust, **explicitly designed
to avoid TDLib**:

> "Telegram CLI tool in **pure Rust** using grammers (MTProto). No TDLib,
> no C/C++ dependencies. `cargo build` and done."

This is the strongest evidence that grammers is production-ready for a
Telegram CLI. dgrr's README states:

> "The Go version (`tgcli-go`) uses TDLib (C++), requiring complex
> cross-compilation and system dependencies. `tgcli` is pure Rust — zero
> C/C++ deps, single `cargo build`, tiny binary."

This is **exactly the pain point cipherocto has today**, with the same
exact framing. dgrr's solution is grammers; cipherocto's should be too.

The tgcli source tree (`src/`) gives us a working layout:

```
src/
  main.rs          CLI entry point (clap)
  cmd/             Command handlers
    auth.rs        Phone → code → 2FA (and bot)
    sync.rs        Incremental/full sync
    chats.rs       List/search/create/join/leave/archive/pin/mute
    messages.rs    List/search/send/edit/forward/download
    send.rs        Send text/files/voice/video
    contacts.rs    List/search contacts
    read.rs        Mark as read
    stickers.rs    List/search/send stickers
    polls.rs       Create polls
    profile.rs     Show/update profile
    folders.rs     Create/manage chat folders
    users.rs       Show/block/unblock users
    typing.rs      Send typing indicator
  store/           turso (libSQL) + FTS5 storage
  tg/              grammers client wrapper
  app/             App struct + business logic
  out/             Output formatting
```

The `tg/` wrapper is the closest existing analog to what cipherocto's
`octo-adapter-telegram-mtproto` would look like. We should study it
carefully before designing ours.

#### 3.3 mini-telegram (server-side, out of scope)

`mini-telegram` is "an unofficial, monolithic, idiomatic implementation of
MTProto (Telegram) **server** built with Rust." Out of scope for our
client research, but mentioned for completeness.

#### 3.4 Other libraries

| Library | What it is | Verdict |
|---------|-----------|---------|
| `teloxide` (in IronClaw per the existing transport-patterns research) | Telegram Bot framework | "Too heavy; use raw `reqwest` + Bot API" per `social-platform-transport-patterns.md` §5. Still true; not MTProto-native. |
| `tdl` (libhunt list) | TDLib bindings (C++ via FFI) | Same pain points as `tdlib-rs`. Skip. |
| `WTelegramClient` (.NET) | TL-generated C# client | Wrong language. |
| `MadelineProto` (PHP) | TL-generated PHP client | Wrong language; also pulls TDLib. |

**No other pure-Rust MTProto client library exists with the maturity of
grammers.** This is the choice.

### 4. Gap analysis: grammers vs. `mtproto_port.md`

The 3 sections of `mtproto_port.md` not fully covered by grammers:

| Gap | § | What grammers has | What grammers lacks | cipherocto impact |
|-----|---|-------------------|---------------------|---------------------|
| **G1. Old-MTP1 inner encryption for `bind_auth_key_inner`** | 7.2 (old), 10.8 | `grammers-crypto` has the SHA-1-based 4-round pattern available but not wired into the public API for `bind_auth_key_inner` | The full `bind_auth_key_inner` AES-IGE encrypt path with old-MTP1 derivation | **None.** cipherocto does not use temp keys. Skip. |
| **G2. SOCKS5 / HTTP CONNECT proxy** | 9.1, 9.2 | None built in; you connect through your own `tokio-socks` or `hyper` proxy | Native SOCKS5 client and HTTP CONNECT helper | **Low.** cipherocto does not currently require proxy support; if a future user needs it, it's a 200-LOC wrapper around `tokio-socks` + the `tokio::net::TcpStream` that grammers returns from `transport::Tcp::connect`. |
| **G3. Fake-TLS MTProxy (V`D` with `0xEE` secret)** | 6.1, 9.3 | `grammers-mtsender::transport::Tcp` supports the obfuscated 16-byte secret (V1 and V`D` with `0xDD` 17-byte secret) | The fake-TLS `ClientHello` preamble for `0xEE` ≥21-byte secrets | **Low.** Fake-TLS MTProxy is used in China / Iran / Russia for region-blocked networks. Cipherocto can ship it as a Phase 2 extension (~200 LOC of TLS record construction that we never need to actually parse, because the server strips it). |
| **G4. HTTP transport** | 12 | None | Long-poll HTTP POST to `http://ip:80/api` | **None.** TCP works for almost everyone. The cipherocto Bot-API HTTP path is separate. |
| **G5. CDN config + dedicated file loader** | 10.9 | Partial (CDN DCs are addressable, but the dedicated file loader for multi-GB files is not the default path) | `help.getCdnConfig` orchestration and `dedicated_file_loader` style streaming | **None.** cipherocto does not need CDN download. |
| **G6. Bot-API HTTP** | n/a (out of `mtproto_port.md` scope) | Not in grammers | The `https://api.telegram.org/bot{token}/{method}` HTTP API | **Medium.** This is the most-used path today for bot-only cipherocto users. We will keep it as a **fallback** in the new adapter. |

**G2, G3, G4, G5 are all non-blocking** for the cipherocto use case
(gateways, deterministic transport, no proxy, no CDN, no HTTP transport).
**G6** is the most important gap to handle in the design, and the
recommendation is to keep the `reqwest`-based Bot-API path as a **fallback
channel** in the new adapter (see §5).

### 5. Gap analysis: grammers vs. cipherocto's `PlatformAdapter` needs

| `PlatformAdapter` method (RFC-0850 §8.2) | grammers API | Gap? | Notes |
|-------------------------------------------|--------------|------|-------|
| `send_envelope(domain, envelope) -> DeliveryReceipt` | `client.send_message(chat, base64(envelope))` for text; `client.send_file(chat, bytes, "envelope.bin")` for >4KB | None | Need a thin wrapper that picks text vs file based on envelope size. The existing `envelope.rs` (which encodes the DOT wire format) is reusable as-is. |
| `receive_messages(domain) -> Vec<RawPlatformMessage>` | `client.next_update().await` (one at a time) | **API shape mismatch** | grammers' API is a **stream** of typed `Update` values. The cipherocto trait wants a **batch** of `RawPlatformMessage`. Bridge: maintain a `tokio::sync::mpsc` that the adapter fills; `receive_messages` drains the channel. This is also what the `dot/async-receive` work in `social-platform-transport-patterns.md` §1.5 already proposes. |
| `canonicalize(raw) -> DeterministicEnvelope` | `grammers_client::types::Update` is already a typed enum | None | The translation from `Update` → `DeterministicEnvelope` is straightforward and can be a pure function. |
| `capabilities() -> CapabilityReport` | n/a (per-call config) | None | Hardcode the cipherocto-known Telegram limits: 4096-char text, 50 MB file upload, 2 GB file download. |
| `domain_id(platform_id) -> BroadcastDomainId` | `BLAKE3("telegram:{chat_id}")` (unchanged) | None | Existing implementation in `adapter.rs` is reusable. |
| `platform_type() -> PlatformType` | `PlatformType::Telegram` (0x0001) | None | — |
| `replay_protection` | grammers' `MessageBox` does this internally | None | The cipherocto-side replay cache in `DotGateway` is per-domain, not per-MTProto-session; the two are independent layers. |
| `health_check` | `client.is_authorized()` | None | grammers exposes this; adapter calls it. |
| `shutdown` | `client.sign_out()` (then drop the `Client`) | None | — |
| `self_handle` | `client.get_me()` returns `User` with `id()` | None | The current `self_handle.rs` does this. Reusable. |
| `upload_media_to_domain` | `client.send_file(chat, path).await` | None | — |
| `download_media` | `client.download_file(input_location).await` returns `Vec<u8>` | None | grammers' `download_file` does the right thing for ≤50 MB media. For larger, we need streaming — but that's already an outstanding R4 H13 in the existing TDLib adapter. |

**Net:** every `PlatformAdapter` method is implementable on top of grammers
with at most ~50 LOC of glue per method. The only architectural change
needed is the **stream-to-batch** bridge in `receive_messages`, which is
the same work the `social-platform-transport-patterns.md` §1.5 already
identifies for async-stream evolution.

### 6. The Bot-API path (the alternative)

The **non-MTProto** path for Telegram is the **Bot API** — a RESTful HTTP
API at `https://api.telegram.org/bot{token}/{method}`. It's simpler than
MTProto (no DH handshake, no auth_key, no AES-IGE), but it only works for
**bot accounts** (not user accounts), and **only exposes a subset of the
TL API** (no `getDialogs`, no `getHistory` for full sync, no group admin
actions on personal accounts, etc.).

The cipherocto `octo-adapter-telegram` predates the TDLib rewrite
(`docs/plans/2026-06-05-0850ab-tdlib-telegram-adapter.md` §1: "Replace
the 0850f raw-Bot-API implementation of `octo-adapter-telegram` with a
TDLib-backed implementation") and was **Bot-API-only**. The TDLib rewrite
was the right call for user-mode support, but it brought the C++ pain.

The choice for the new adapter:

- **Bot mode:** either Bot-API HTTP (`reqwest`+`bot_token`) **or** MTProto
  via grammers. Both work. **MTProto is recommended** for parity with user
  mode and for access to the full TL API (channels, supergroups, file IDs
  that survive migrations, etc.).
- **User mode:** MTProto via grammers (Bot-API does not support user
  accounts).

So the new adapter uses **MTProto for both bot and user**, with the
**Bot-API HTTP path kept as a fallback** for users behind firewalls where
TCP 443 to Telegram DCs is blocked but HTTPS to `api.telegram.org` works
(relevant in China, where `api.telegram.org` is on a different network
path from `149.154.175.50:443`).

---

## Architecture: proposed approach

The recommended architecture is **wrap grammers, fall back to Bot-API HTTP
for region-blocked users, do not touch the TDLib code during the
migration** (so the existing adapter keeps shipping in production until
the new one is proven).

### New crate: `octo-adapter-telegram-mtproto`

```
crates/octo-adapter-telegram-mtproto/
├── Cargo.toml              ← grammers = "0.9", grammers-session/sqlite, grammers-crypto
│                             tokio = "1.35", reqwest = "0.12" (for Bot-API fallback)
│                             blake3 = "1.5", base64 = "0.22", async-trait = "0.1"
│                             octo-network = { path = "../octo-network" }
├── src/
│   ├── lib.rs              ← re-exports + PlatformAdapter dispatch
│   ├── adapter.rs          ← implements PlatformAdapter (MTProto primary, HTTP fallback)
│   ├── mtproto_client.rs   ← grammers Client wrapper (much smaller than the TDLib one)
│   ├── http_fallback.rs    ← Bot-API HTTP path (preserved from 0850f)
│   ├── auth.rs             ← sign_in / check_password / qr_login (smaller)
│   ├── config.rs           ← TelegramConfig (unchanged shape)
│   ├── envelope.rs         ← DOT wire format (unchanged from 0850f)
│   ├── error.rs            ← TelegramError / Result
│   ├── self_handle.rs      ← self-loop filter (reusable from TDLib crate)
│   ├── groups.rs           ← chat discovery (reusable)
│   ├── cleanup.rs          ← graceful shutdown (reusable)
│   └── files.rs            ← upload/download via grammers (was TDLib file_id)
├── tests/                  ← integration tests against test DC (off by default)
└── examples/               ← example binaries (reuse TDLib examples, swap impl)
```

### Architecture diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                       DotGateway (RFC-0850)                          │
│   version check → signature → replay → flags → forward              │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                ┌──────────────┴──────────────┐
                ▼                             ▼
┌─────────────────────────────┐  ┌────────────────────────────────┐
│   octo-adapter-telegram     │  │  octo-adapter-telegram-mtproto │  ← NEW
│   (TDLib C++, 0850ab)       │  │  (grammers pure-Rust)          │
│                             │  │                                 │
│  real_client.rs (large)     │  │  mtproto_client.rs (small)     │
│  auth.rs (large)            │  │  auth.rs (small)               │
│  build.rs (SEC-C1)          │  │  no build.rs, no C++           │
│  150 MB prebuilt binary     │  │  no prebuilt binary            │
└─────────────────────────────┘  └────────────────────────────────┘
                │                             │
                ▼                             ▼
┌─────────────────────────────┐  ┌────────────────────────────────┐
│  tdlib-rs 1.4.x             │  │  grammers 0.9.0                 │
│  (TDLib C++ 150 MB)         │  │  (pure Rust, 8 crates)          │
│                             │  │                                 │
│  [auth_key]                 │  │  [auth_key]                     │
│  [DB: data_dir/database]    │  │  [DB: ~/.cache/grammers.db]    │
│  [JSON-RPC over stdio]      │  │  [Tokio async]                 │
└─────────────────────────────┘  └────────────────────────────────┘
```

### Code sharing between the two adapters

The DOT wire format, `domain_id` derivation, and the high-level
`PlatformAdapter` shape are **identical** between TDLib and grammers
implementations. To avoid duplication, the following shared items can be
moved to `octo-network` (or a new `octo-telegram-common` crate):

- `envelope.rs` — DOT wire format (218-byte signing payload + 64-byte
  signature = 282-byte wire envelope, base64 URL_SAFE_NO_PAD).
- `domain_id(chat_id) -> BroadcastDomainId` — `BLAKE3("telegram:{chat_id}")`.
- `config.rs` — `TelegramConfig` (api_id, api_hash, bot_token, data_dir).
- `self_handle.rs` — self-loop filter (no platform dependency).
- `error.rs` — error type (depends on what errors we model; partial move
  to `octo-telegram-common`).

The `octo-telegram-onboard` CLI is also rewritten to use grammers' QR
login flow. The new CLI is much smaller because grammers' `qr_login()`
returns a `Token` directly, with no 9-variant auth state machine.

### Session storage

The TDLib crate currently stores two SQLite DBs (TDLib's own
`data_dir/database` and cipherocto's session metadata in
`data_dir/session.db`). The grammers crate stores one SQLite DB
(`~/.cache/grammers.db` via `SqliteSession`), which holds the auth_key,
the user_id, the home DC, and the peer cache. cipherocto's session
metadata (config values, group mappings) can live in the same DB
under a separate `cipherocto_*` table prefix, or in a separate small
DB. Recommendation: **one DB**, separate table prefix, mirroring what
`octo-matrix-session-store` does for the matrix adapter.

### Fallback channel

For users in region-blocked networks (China, Iran, Russia), the adapter
exposes a `--transport http` flag that switches to the **Bot-API HTTP
path**. This is the **same code** as the 0850f Bot-API implementation,
preserved as `src/http_fallback.rs`. It is **not** the default, because
MTProto is strictly more capable and works for 95%+ of users.

---

## Implementation phases

### Phase 0: Research (this document)

- **Status:** ✅ This document.
- **Exit criteria:** Research reviewed, recommended path accepted, mission
  created.

### Phase 1: Parallel pure-Rust adapter (no breakage)

**Mission:** `0850ab-c-pure-rust-mtproto-telegram-adapter`

- New crate `octo-adapter-telegram-mtproto` (see §5).
- Uses `grammers` for bot mode + user mode + QR login.
- Implements `PlatformAdapter` from `octo-network` (same trait).
- Wire format identical to `octo-adapter-telegram` (282-byte envelope,
  `BLAKE3("telegram:{chat_id}")`, base64 URL_SAFE_NO_PAD).
- HTTP fallback (`--transport http`) using `reqwest`+bot_token, identical
  to 0850f.
- **No changes** to the existing TDLib adapter. The two coexist; users
  opt in to the new one with `use_telegram_mtproto = true` in their DOT
  gateway config.
- **Estimated code:** ~2000 LOC of new cipherocto code, ~500 LOC of
  shared code moved out of the TDLib crate.
- **Acceptance criteria:**
  - All 109 unit tests in the existing TDLib crate pass unchanged (the
    `MockTelegramClient` is reusable).
  - 3 new integration tests against the Telegram test DC: `auth.sign_in_bot`,
    `auth.sign_in_user_2fa`, `send_envelope` round-trip.
  - `cargo clippy --all-targets -- -D warnings` clean.
  - `cargo fmt --all --check` clean.
  - No C++ build deps, no prebuilt binary download, no `build.rs` SHA pin.

### Phase 2: Cut over (transparent migration)

**Mission:** `0850ab-d-telegram-mtproto-cutover`

- `octo-adapter-telegram` becomes a **re-export** of
  `octo-adapter-telegram-mtproto` for bot mode.
- TDLib build moves behind a `legacy-tdlib` feature for users who cannot
  use MTProto (region-blocked networks where TCP 443 to Telegram DCs is
  blocked AND HTTPS to `api.telegram.org` is also blocked — vanishingly
  rare).
- `octo-telegram-onboard` is rewritten to use grammers' QR login. Old
  TDLib-based onboarding is behind `legacy-tdlib` feature.
- **Acceptance criteria:**
  - Default `cargo build` of `octo-adapter-telegram` does **not** download
    the TDLib binary.
  - `cargo build --features legacy-tdlib` still works (for the rare
    fallback case).
  - Onboarding CLI is 1/3 the size of the TDLib version.
  - All existing DOT gateway users can upgrade without config changes.

### Phase 3: Make TDLib fully optional (the optional win)

**Mission:** `0850ab-e-telegram-tdlib-optional`

This phase is **optional and not recommended before Phase 2 stabilizes**.
If we do reach it, the goal is to make the TDLib build **fully opt-in**
rather than the default:

- Move the `real-tdlib` feature behind an opt-in `legacy-tdlib` feature.
- Move the TDLib-based onboarding to opt-in via the same feature.
- The default `cargo build` of `octo-adapter-telegram` no longer downloads
  the TDLib binary and no longer requires a C++ toolchain.
- **The TDLib code, the `legacy-tdlib` feature, and the onboarding CLI
  variant all remain in-tree** as alternative paths for users with hard
  requirements (e.g. region-blocked networks where neither MTProto nor
  Bot-API HTTP work and TDLib is the only viable option).
- The cipherocto source tree grows by a few feature-gated code paths,
  not shrinks.

**Acceptance criteria (if Phase 3 is undertaken):**
- Default `cargo build` of `octo-adapter-telegram` produces a statically
  linked pure-Rust binary.
- The crate builds on `aarch64-apple-darwin`,
  `aarch64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, etc. without
  any platform-specific setup.
- `cargo build --features legacy-tdlib` still works for the opt-in
  fallback case.
- CI build time for the default configuration drops from ~5 min to
  ~30 s (no TDLib download + C++ compile).

---

## Recommendations

1. **Adopt grammers as the MTProto layer for Telegram.** It is the
   single mature pure-Rust choice. The gap analysis (4.4) shows that all
   gaps are non-blocking for cipherocto's needs.

2. **Use the migration-in-parallel strategy** (Phase 1 → 2 → 3 above).
   This is the same pattern used for the matrix adapter
   (`docs/plans/2026-05-31-matrix-rust-sdk-migration.md`), which is the
   closest precedent in the cipherocto repo. **Do not** attempt a
   big-bang migration; the TDLib crate is the production adapter today
   and must keep shipping.

3. **Move the DOT wire format and `domain_id` to a shared
   `octo-telegram-common` crate** (or to `octo-network`). The TDLib
   and grammers implementations should not duplicate them.

4. **Vendor grammers as `octo-grammers-vendored`** under a `vendored`
   feature flag, mirroring what was done for `matrix-sdk` in the
   2026-05-31 migration. This is the supply-chain mitigation for the
   one-maintainer risk on grammers. The vendored fork is updated
   **only** if upstream goes unmaintained for >6 months; until then,
   we use upstream.

5. **Keep the Bot-API HTTP path as a fallback**, behind a `--transport
   http` flag. This is the right call for region-blocked users, and the
   code is the same 0850f implementation preserved.

6. **Adopt grammers' per-`Client` Tokio task model** to enable multiple
   accounts (or bot + user) in the same process. This is one of the
   concrete advantages of the new crate over the current
   process-global `tdlib_rs::receive()` constraint.

7. **Update the cipherocto transport research** (`docs/research/group-coordination-transport-adapters.md`
   and `social-platform-transport-patterns.md`) to note that a
   pure-Rust Telegram adapter is now an alternative, alongside the
   existing TDLib-based one.

8. **Adopt grammers as the pattern for other C++ adapters** (if any
   arise in the future). The TDLib precedent should not be repeated
   for new adapters.

### What we are NOT recommending

- **Forking grammers for a feature we need.** The 3 small gaps (§4.4
  G1, G2, G3) are non-blocking; we can write 200-LOC wrappers around
  upstream if and when we need them.
- **Replacing the DOT wire format.** The 282-byte envelope is preserved
  from 0850f and is the contract with the DotGateway; it is **not**
  the protocol's problem.
- **Switching to Bot-API for production.** Bot-API is HTTP-only, bot-only,
  and lacks the full TL API. It is a fallback, not the default.

---

## Next Steps

- [x] Research complete (this document)
- [ ] Submit for review under `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md`
- [ ] If accepted → Create Use Case at `docs/use-cases/pure-rust-telegram-transport.md`
- [ ] Create RFC at `rfcs/draft/networking/0850ab-c-pure-rust-mtproto-telegram-adapter.md` (or amend RFC-0850ab-a)
- [ ] Create mission `missions/open/0850ab-c-pure-rust-mtproto-telegram-adapter.md` per Phase 1
- [ ] Update `docs/research/group-coordination-transport-adapters.md` and `social-platform-transport-patterns.md` to note the new pure-Rust alternative
- [ ] Update `docs/research/README.md` with this report

### Related research / RFCs / missions

- `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` — the
  parent RFC for transport adapters.
- `rfcs/accepted/networking/0850ab-a-telegram-auth-onboarding.md` — the
  current auth onboarding RFC (TDLib-based).
- `rfcs/accepted/networking/0850p-a-whatsapp-auth-onboarding.md` — the
  WhatsApp analog (already pure-Rust on the cipherocto side).
- `rfcs/accepted/networking/0850p-c-transport-group-binding.md` — group
  binding (transport-agnostic).
- `docs/research/social-platform-transport-patterns.md` — the 2026-05-28
  transport research that first enumerated the 20-adapter landscape.
- `docs/research/group-coordination-transport-adapters.md` — the 2026-06-17
  follow-up that audited the 20 adapters.
- `docs/plans/2026-05-31-matrix-rust-sdk-migration.md` — the closest
  precedent: a pure-Rust migration of a non-pure-Rust adapter.
- `docs/plans/2026-06-05-0850ab-tdlib-telegram-adapter.md` — the plan
  that introduced the TDLib dependency that this research proposes
  to complement with a pure-Rust alternative.

### Open questions for the Use Case

1. **Bot mode default.** Should the new adapter's default be **MTProto**
   or **Bot-API HTTP** for bot accounts? Recommendation: **MTProto** for
   parity, with HTTP as the fallback.
2. **Vendoring grammers timing.** Vendor immediately (Phase 1), or wait
   for the first release that breaks cipherocto (Phase 1.5)? The matrix
   precedent vendor'd immediately. Recommend **immediate** for the same
   supply-chain reason.
3. **Session storage location.** Same DB as grammers' `SqliteSession` or
   a separate cipherocto DB? Recommendation: **same DB, separate table
   prefix**, mirroring `octo-matrix-session-store`.
4. **Multiple accounts per process.** grammers supports this natively
   (one `Client` per account). Should the adapter expose it? Recommend
   **yes**, via a `Vec<Arc<TelegramClient>>` in the adapter, exposed
   through the existing `TelegramConfig` extension.
5. **CDN media (Gap G5).** Skip for Phase 1-3, or add a small wrapper in
   Phase 2? Recommend **skip** — no cipherocto use case today requires
   CDN media.

---

## References

### cipherocto (this repo)

- `crates/octo-adapter-telegram/` — current TDLib C++ adapter (14 files, ~5500 LOC)
- `crates/octo-telegram-onboard/` and `crates/octo-telegram-onboard-core/` — TDLib C++ onboarding CLI
- `crates/octo-network/src/dot/adapters/mod.rs` — `PlatformAdapter` trait (RFC-0850 §8)
- `crates/octo-network/src/dot/fragment.rs` — DOT envelope fragmentation
- `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` — the parent RFC
- `rfcs/accepted/networking/0850ab-a-telegram-auth-onboarding.md` — current auth RFC
- `rfcs/accepted/networking/0850p-c-transport-group-binding.md` — group binding
- `rfcs/draft/networking/0850p-d-f.md` — DC-initiated group creation, kick detection, group decommission
- `docs/research/social-platform-transport-patterns.md` — 2026-05-28 transport research
- `docs/research/group-coordination-transport-adapters.md` — 2026-06-17 transport audit
- `docs/plans/2026-05-31-matrix-rust-sdk-migration.md` — closest precedent (matrix adapter)
- `docs/plans/2026-06-05-0850ab-tdlib-telegram-adapter.md` — the TDLib design we are replacing

### External (mtproto_port.md is the in-tree reference; these are the upstream sources)

- `/home/mmacedoeu/_w/tools/tdesktop/docs/mtproto_port.md` — 23-section, 2049-line MTProto client reference (in-tree)
- `codeberg.org/vilunov/grammers` — the primary grammers repo (last commit 2026-05-15)
- `github.com/Lonami/grammers` — github mirror
- `github.com/overrealdb/grammers` — active fork
- `crates.io/crates/grammers-mtproto` (0.9.0), `grammers-tl-types` (0.9.0), `grammers-client` (0.8.x)
- `docs.rs/grammers-mtproto` — current API reference
- `deepwiki.com/Lonami/grammers/3-core-architecture` — architecture deep-dive
- `github.com/dgrr/tgcli` — production pure-Rust CLI on top of grammers
- `github.com/dgrr/tgcli-go` — the Go/TDLib version that dgrr explicitly contrasts tgcli against
- `core.telegram.org/mtproto` — official MTProto spec
- `core.telegram.org/mtproto/description` — the spec section grammers-mtproto implements

### Anti-references (libraries we considered and rejected)

- `tdlib-rs` 1.4.x — TDLib FFI (an existing alternative, kept behind `legacy-tdlib` in Phase 3)
- `teloxide` — Bot-API framework, too heavy
- `MadelineProto` — PHP, wrong language
- `WTelegramClient` — .NET, wrong language
- `mini-telegram` — server-side, wrong direction
