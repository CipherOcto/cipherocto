# Mission: DOT Telegram Adapter (TDLib rewrite)

## Status

Claimed (in progress)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1 (Platform Adapters)

## Summary

Replace the existing `octo-adapter-telegram` raw-Bot-API implementation (`missions/archived/0850f-dot-telegram-adapter.md`) with a TDLib-backed implementation via the `tdlib-rs` Rust binding. Enables general file transfer (≤2 GB), unified user-account and bot-auth modes, server-push updates with sub-100ms latency, and access to MTProto-only features (E2E chats, secret chats, voice/video calls) when CipherOcto gateways operate as user accounts rather than bots. The `PlatformAdapter` contract from RFC-0850 §8.1 is preserved — only the underlying transport changes.

## Supersedes

- `missions/archived/0850f-dot-telegram-adapter.md` (raw `reqwest`+`serde` Bot API, 9 tests, ~744 LOC)
  - On acceptance of this mission: 0850f is moved to `missions/archived/superseded/` and 0850ab becomes the canonical Telegram adapter.

## Dependencies

- **RFC-0850:** Deterministic Overlay Transport, §8.1 (Platform Adapters trait, `CapabilityReport`, `BroadcastDomainId`).
- **Mission 0850e:** DOT Adapter Registry & Plugin ABI — the `PlatformAdapter` trait and C ABI shims are already in place.
- **Mission 0850:** Core envelope types and the DOT wire format.
- **TDLib build toolchain:** C++ compiler (gcc/clang/MSVC), CMake ≥ 3.18, and either:
  - Network access at build time for `tdlib-rs`'s `download-tdlib` feature to fetch prebuilt binaries, OR
  - Local TDLib source tree at `$LOCAL_TDLIB_PATH` with the `local-tdlib` feature.

## Motivation

The current raw-Bot-API adapter (`0850f`) is correct for small text envelopes but has three structural limits that block CipherOcto feature goals:

1. **Bot API upload ceiling: 50 MB per document** — `sendDocument` rejects anything larger. The DOT envelope with rich attachments (e.g., a model checkpoint slice, a ZK proof transcript, a multi-MB dataset fingerprint) can exceed this. TDLib/MTProto supports file transfer up to 2 GB with parallel chunked upload and automatic resumability on disconnect.
2. **No access to user-account features** — Bots cannot read a user's own chat history, join groups without admin invitation, access E2E-encrypted chats, or send voice/video. A user-account-style integration unlocks the "personal transport" use case (CipherOcto gateway as a Telegram user, not as a BotFather-created bot).
3. **Polling latency floor** — `getUpdates?timeout=30` round-trips every ≤30s. For a gateway that reacts to incoming messages, sub-100ms push delivery is materially better. TDLib holds a persistent MTProto connection with server-push updates.

The cost of these gains is heavier build (C++ toolchain, ~150 MB TDLib binary) and larger cdylib. For CipherOcto gateways that need only small text envelopes, the existing raw-Bot-API approach is still the right choice — and this mission explicitly does not propose removing the smaller alternative; the `tgt` and `tg` reference implementations both maintain a Bot API path for exactly this reason. This mission adds a TDLib path as the **default** for adapters that need general file transfer, with the raw-Bot-API path retained for low-dependency deployments.

## Design

### Stack

| Layer                         | Choice                                                                                                          | Rationale                                                                                                                                     |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Telegram client               | `tdlib-rs` 1.4.x (Rust binding to TDLib C++)                                                                    | Maintained, ~10K LOC Rust, MIT, used by `tg` and `tgt` reference implementations                                                              |
| TDLib delivery                | `tdlib-rs` `download-tdlib` feature (auto-fetch prebuilt) **or** `local-tdlib` (build from `$LOCAL_TDLIB_PATH`) | Matches `tg`/`tgt` build patterns; the `download-tdlib` feature is the recommended default for first-time builds                              |
| Async runtime                 | `tokio` 1.x with `rt-multi-thread` and `time` features                                                          | Already in workspace; TDLib's blocking `client_receive` calls are dispatched to a dedicated blocking thread via `tokio::task::spawn_blocking` |
| JSON                          | `serde` + `serde_json`                                                                                          | Already in workspace; TDLib returns JSON values that map directly via `tdlib_rs::types`                                                       |
| Crypto (auth_key persistence) | `rusqlite` 0.31+ with bundled feature                                                                           | TDLib requires persisting the 2048-bit `auth_key` to disk; SQLite is the canonical choice (matches `tg`)                                      |
| Hashing                       | `blake3` 1.5                                                                                                    | Already in workspace; needed for `domain_id()` (preserved from `0850f`)                                                                       |
| HTTP (webhook fallback)       | `reqwest` 0.12 with `rustls-tls`                                                                                | Already in workspace; for the optional public-webhook path                                                                                    |

### Architecture

The adapter is split into three layers, each independently testable:

1. **Telegram client wrapper** (`src/client.rs`) — Owns the TDLib `Client`, runs the receive loop on a dedicated OS thread, and exposes an async API to the rest of the adapter. Persists auth state to `$data_dir/tdlib/<bot_id>/database`. Surfaces typed Rust enums for: `Update::NewMessage { chat_id, message }`, `Update::MessageEdited { ... }`, `Update::FileDownloaded { file_id, local_path, size }`, etc.
2. **DOT envelope layer** (`src/envelope.rs`) — Preserves the exact 0850f wire format: 218-byte signing payload + 64-byte signature (282-byte wire envelope) (BLAKE3-256 domain hash, `BLAKE3("telegram:" + chat_id)` per the RFC-0850 spec). The TDLib `Message` content is parsed to extract the envelope, which is then validated against the `BroadcastDomainId`.
3. **`PlatformAdapter` impl** (`src/adapter.rs`) — Implements the trait from RFC-0850 §8.1. The `send_envelope` path packs the envelope into either:
   - `sendMessage` (≤4096 chars total, default for 282-byte envelopes)
   - `sendDocument` (multi-MB up to 2 GB via TDLib's `messages.sendMultiMedia` + `inputFile::LocalFile`)
   - `messages.sendEncryptedFile` (for E2E-encrypted chats, optional future work)
     The `receive_messages` path subscribes to TDLib's update stream and yields each new message as a `RawPlatformMessage`.

### Config

```yaml
# config/telegram.yaml (or via `TelegramConfig` config struct)
telegram:
  # Required: either bot_token (Bot API mode) OR phone + api_id + api_hash (user-account mode)
  mode: bot # "bot" | "user" (default: bot)
  bot_token: "123456:ABC-DEF..." # required if mode=bot
  api_id: 12345 # required if mode=user (from my.telegram.org)
  api_hash: "0123456789abcdef..." # required if mode=user
  phone: "+1234567890" # required if mode=user on first auth
  data_dir: "~/.local/share/cipherocto/telegram" # TDLib auth_key persistence
  groups: [-1001234567890, -1009876543210] # list of chat IDs to monitor (Bot mode)
  # Optional: 2FA password for user mode
  password: null
  # Optional: webhook fallback (matches 0850f's webhook_port)
  webhook_port: null # if set, exposes an HTTP server for update delivery
  # Optional: feature gates
  features:
    e2e_chats: false # enable access to secret chats (user mode only)
    voice_video: false # enable voice/video call hooks (user mode only)
```

### File Layout

```
crates/octo-adapter-telegram/
├── Cargo.toml                    # cdylib + rlib. Deps: tdlib-rs (TDLib binding), tokio (async), reqwest (webhook), rusqlite (auth_key SQLite), serde+serde_json (TDLib JSON), async-trait (PlatformAdapter trait), base64 (envelope encoding), blake3 (domain_id), octo-network (workspace)
├── build.rs                      # tdlib-rs build orchestration
├── src/
│   ├── lib.rs                    # re-exports + crate-level docs
│   ├── client.rs                 # TDLib client wrapper (async over blocking receive thread)
│   ├── envelope.rs                # DOT envelope pack/unpack (preserved from 0850f)
│   ├── adapter.rs                # PlatformAdapter impl (preserved contract)
│   ├── config.rs                 # TelegramConfig (bot vs user mode, groups, data_dir)
│   ├── error.rs                  # thiserror error types (TelegramError, AuthError, FileError)
│   ├── auth.rs                   # phone/api_id/api_hash load + 2FA prompt stub
│   ├── files.rs                  # send_document / download_file (TDLib InputFile + getFile)
│   ├── groups.rs                 # chat_id resolution by name/username
│   └── self_handle.rs            # self-loop prevention via getMe + cache
├── tests/
│   ├── mock_tdlib.rs            # mock TDLib client (matches tg's `MockClient` pattern)
│   ├── envelope_tests.rs         # round-trip 282-byte envelope
│   ├── file_upload_tests.rs      # 100MB upload (10x the Bot API 10MB limit)
│   ├── file_download_tests.rs    # 100MB download (5x the Bot API 20MB limit)
│   ├── user_mode_tests.rs        # phone + api_id auth flow (mocked) — covers AC "User mode test"
│   ├── self_loop_tests.rs        # drop self-authored messages
│   ├── auth_key_migration_tests.rs # detect TDLib auth_key schema drift across `tdlib-rs` version bumps
│   └── integration_matrix.rs     # feature-gated: full round-trip with real Telegram test DC
```

### Migration Path

This is a **rewrite**, not an additive feature. The migration plan is:

1. **Phase 1 (this mission):** Replace `crates/octo-adapter-telegram/src/lib.rs` (the 744-LOC `0850f` implementation) with the new TDLib-backed structure. Move `0850f-dot-telegram-adapter.md` to `missions/archived/superseded/0850f-dot-telegram-adapter.md` with a `Superseded by: missions/open/0850ab-dot-telegram-tdlib-adapter.md` link.
2. **Phase 2 (future):** Add a `--features bot-api-compat` Cargo feature that re-exposes the 0850f raw-Bot-API path under a `BotApiTelegramAdapter` type, for low-dependency deployments that don't need TDLib's file transfer or user-mode features.
3. **Phase 3 (future):** Add E2E chat support (TDLib's `tde2e` layer) for cipherocto channels that need to participate in secret chats.

## Acceptance Criteria

- [ ] `crates/octo-adapter-telegram/` crate compiles to `cdylib` and `rlib` with default features
- [ ] With `--features download-tdlib`, a fresh build (no local TDLib) succeeds on Linux x86_64, Linux aarch64, macOS x86_64, macOS arm64, Windows x86_64
- [ ] With `--features local-tdlib`, a build against `$LOCAL_TDLIB_PATH` succeeds
- [ ] Implements `PlatformAdapter` trait with all methods (6 required + 6 optional: `replay_protection`, `health_check`, `shutdown`, `self_handle`, `upload_media`, `download_media`; `self_handle` must override the default to return the bot's user_id, `upload_media`/`download_media` required for the TDLib file transfer feature)
- [ ] `send_envelope()` writes the 282-byte envelope via `sendMessage` for the small case (preserved from 0850f)
- [ ] `send_envelope()` writes larger envelopes via `sendDocument` / TDLib's file upload (up to 2 GB)
- [ ] `receive_messages()` consumes TDLib's update stream (sub-100ms push latency, not polling)
- [ ] `canonicalize()` extracts envelope from both text and document messages
- [ ] Fragmentation: large envelopes sent as multi-part documents (preserved from 0850f)
- [ ] `CapabilityReport`: `max_payload_bytes=2_000_000_000` (2 GB), `rate_limit_per_second=30` (unchanged), `supports_fragmentation=true` (via document attachments), `supports_encryption=false` (0850f value; user-mode may set true for E2E chats), `supports_raw_binary=false` (Telegram is a chat app, requires DOT/1/ encoding), `media_capabilities=Some(MediaCapabilities { max_upload_bytes: 2_000_000_000, supported_mime_types: vec!["application/octet-stream".into(), "image/*".into(), "video/*".into(), "audio/*".into()] })` (TDLib file transfer)
- [ ] `domain_id()`: `BroadcastDomainId(0x0001, BLAKE3("telegram:" + chat_id))` (preserved from 0850f)
- [ ] Config: `mode` (`bot` | `user`), `bot_token` (bot mode), `api_id`+`api_hash`+`phone` (user mode), `data_dir`, `groups`, `webhook_port` (optional), `password` (optional, user mode 2FA), `features` (optional: `e2e_chats` (default `false`, user mode only — feature gate for the Phase 3 E2E mission), `voice_video` (default `false`, user mode only))
- [ ] Error handling: rate limiting (429 retry, exponential backoff), auth expiry (re-prompt), file transfer failure (resumable upload)
- [ ] Exponential backoff: initial=1s, max=120s, jitter=0-500ms (preserved from 0850f)
- [ ] Self-loop prevention: `self_handle()` returns the bot's user_id (or user_id for user mode) to drop self-authored messages
- [ ] Auth persistence: `data_dir/database` is created on first run, reused on subsequent runs
- [ ] **100 MB file upload** test (10× Bot API's 10 MB photo limit, 2× Bot API's 50 MB document limit) — must succeed
- [ ] **100 MB file download** test (5× Bot API's 20 MB `getFile` limit) — must succeed
- [ ] Unit tests use a mock TDLib client (no real TDLib instance required for `cargo test`)
- [ ] Integration test (feature-gated) round-trips a real envelope against Telegram's test DC
- [ ] User mode test: `phone + api_id + api_hash` auth flow with mocked TDLib (no real Telegram account needed for `cargo test`)
- [ ] Auth-key migration test: detects TDLib `auth_key` schema drift across `tdlib-rs` version bumps (covers Risk register row 4 mitigation)
- [ ] Binary size on Linux x86_64 release with default features: ≤ 30 MB stripped (excluding the TDLib C++ shared library)
- [ ] Build time on Linux x86_64 release: ≤ 3 min (excluding TDLib download)
- [ ] Cross-compile support: `cargo build --target aarch64-unknown-linux-gnu` succeeds via `cross`

### Type Coverage

| RFC-0850 §8.1 Type                                        | Implemented By                                    |
| --------------------------------------------------------- | ------------------------------------------------- |
| `PlatformAdapter` trait impl                              | This mission                                      |
| `CapabilityReport` struct                                 | This mission                                      |
| `BroadcastDomainId` (BLAKE3-256 of "telegram:" + chat_id) | This mission (preserved from 0850f)               |
| `DeterministicEnvelope` pack/unpack                       | This mission (preserved from 0850f)               |
| Telegram-specific `sendMessage` integration               | **Supersedes** 0850f                              |
| Telegram-specific `getUpdates` polling                    | **Superseded by** TDLib push updates (no polling) |
| Telegram-specific `sendDocument` integration              | This mission (via TDLib `sendDocument`)           |
| Telegram-specific file download (≤ 2 GB)                  | This mission (new)                                |
| Telegram-specific `getMe` (self-loop)                     | This mission (preserved from 0850f)               |
| Telegram-specific 2FA auth (user mode)                    | This mission (new)                                |
| Telegram-specific E2E chat (secret chats)                 | Deferred to Phase 3 (future mission)              |

## Implementation Guide

Companion guide for code-level patterns:

- `docs/07-developers/octo-adapter-telegram-tdlib-implementation-guide.md` (to be created as part of this mission)
  - Module tree (exact `mod.rs` layout)
  - Compilable Rust code for the TDLib client wrapper (async over blocking `client_receive` thread)
  - Error type definitions with `thiserror`
  - Config schema (YAML/TOML, bot vs user mode)
  - Testing strategy: mock TDLib client, real Telegram test DC for integration
  - TDLib build orchestration: `download-tdlib` vs `local-tdlib` feature flags

## Reference Implementations

The following open-source projects were studied for the design (see `docs/reviews/telegram-architecture-comparison-r1.md` (to be created as part of this mission) for the full analysis):

- **`tg` (larskluge/tg)** — 10,625 LOC Rust CLI; uses `tdlib-rs` 1.3.0; same hybrid user/bot auth pattern. Reference for the receive loop + auth persistence.
- **`tgt` (FedericoBruzzone/tgt)** — 21,916 LOC Rust TUI; uses `tdlib-rs` 1.4.0 with static linking. Reference for the Cargo.toml `static-download` feature pattern.
- **`tdesktop`** — Official C++/Qt desktop client; the canonical TDLib user. Reference for the `mtproto_dc_options` DC selection logic (not relevant for bot mode, but needed for user mode).
- **`octo-adapter-telegram` (current, from 0850f)** — 744 LOC; the existing raw-Bot-API implementation that this mission supersedes. Preserved as the "low-dependency fallback" Phase 2 plan.

## Claimant

@claude-code (claimed 2026-06-05, RFC-0850 accepted v1.1.0 in commit `e2fd062`)

## Pull Request

# (to be assigned on PR submission)

## Notes

### Build complexity tradeoff

This mission deliberately accepts heavier build complexity (C++ toolchain, ~150 MB TDLib binary) in exchange for the four pros the user identified:

1. **General file transfer (≤ 2 GB)** — Bot API's 50 MB document ceiling is too low for rich attachments (model checkpoints, ZK proofs, dataset fingerprints).
2. **Lower maintenance burden in the long run** — TDLib is maintained by Telegram itself and tracks their schema changes. The current raw-Bot-API approach requires us to manually update when Telegram deprecates methods.
3. **Feature rich** — User-account mode unlocks E2E chats, voice/video, story reactions, and the full MTProto method set.
4. **Binary-compatible** — `tdlib-rs` with `rustls-tls` is pure Rust except for the TDLib C++ library, which `tdlib-rs` packages with platform-specific prebuilt binaries. Cross-compiles cleanly on Linux/macOS/Windows for x86_64 and aarch64. The only platform gap is Windows ARM (TDLib upstream doesn't ship Windows ARM; `tgt`'s README confirms this gap).

### Why not `grammers` (pure MTProto in Rust)?

`grammers` (~50K LOC) is the alternative pure-Rust MTProto client. It's lighter than TDLib (no C++ dependency) but heavier than Bot API in every other dimension:

| Aspect                 | TDLib (this mission)           | `grammers` (alternative) | Raw Bot API (0850f, superseded) |
| ---------------------- | ------------------------------ | ------------------------ | ------------------------------- |
| C++ dep                | Yes (TDLib)                    | No                       | No                              |
| Pure-Rust              | No                             | Yes                      | Yes                             |
| Transitive Rust deps   | ~25                            | ~30                      | ~5                              |
| Build time (cold)      | ~3-5 min                       | ~3-5 min                 | ~30s                            |
| Binary size (stripped) | 30-50 MB                       | 30-50 MB                 | 5-10 MB                         |
| User-account features  | Yes (TDLib)                    | Yes (grammers)           | No (Bot API only)               |
| Bot API features       | Yes (TDLib also wraps Bot API) | No (pure MTProto)        | Yes                             |
| Media upload limit     | 2 GB                           | 2 GB                     | 50 MB                           |
| Schema maintenance     | Telegram (TDLib)               | Community (grammers)     | us (manually)                   |

TDLib wins on:

- **Schema maintenance**: Telegram maintains the protocol binding. We don't.
- **Bot API coverage**: TDLib wraps both MTProto and Bot API; `grammers` is pure MTProto (no Bot API shortcuts).
- **Battle-tested**: TDLib powers Telegram's own iOS/Android/desktop clients, plus dozens of third-party clients. `grammers` is community-maintained.

TDLib loses on:

- **C++ build dependency**: requires gcc/clang/MSVC + CMake at build time. Pure-Rust is easier to cross-compile.

The C++ build cost is a one-time setup; the schema-maintenance savings compound over the life of the project. TDLib wins.

### Why not stay on raw `reqwest`+`serde` (the `0850f` path)?

The 0850f path is correct for the small-envelope use case. This mission's Phase 1 replaces it with TDLib, but the overall plan re-adds it as a `bot-api-compat` Cargo feature in Phase 2 (future) so the raw-Bot-API path is preserved. The plan is:

- **Default path for CipherOcto gateways** (this mission): TDLib. Gets general file transfer, user-mode, push updates.
- **Low-dep path for resource-constrained deployments** (Phase 2, future mission): retain the 0850f raw-Bot-API implementation as a `bot-api-compat` Cargo feature.

Both paths share the same `PlatformAdapter` contract from RFC-0850 §8.1, so the rest of CipherOcto doesn't care which one a particular gateway uses.

### Risk register

| Risk                                                   | Likelihood | Impact | Mitigation                                                                                                                    |
| ------------------------------------------------------ | ---------- | ------ | ----------------------------------------------------------------------------------------------------------------------------- |
| TDLib schema drift breaks the wrapper                  | Low        | High   | TDLib is maintained by Telegram; we follow the upstream `tdlib-rs` crate's release cadence                                    |
| C++ build fails on a contributor's machine             | Medium     | Medium | Document the C++ toolchain requirement in the README; provide a pre-built `static-download` feature                           |
| Windows ARM gap (TDLib upstream)                       | High       | Low    | Document; defer to upstream TDLib fix. Pure-Rust `grammers` is the long-term fallback if needed                               |
| TDLib auth_key persistence schema changes              | Low        | High   | Pin `tdlib-rs` version; write a migration test (see `tests/auth_key_migration_tests.rs`) that detects auth_key schema changes |
| `tokio` runtime conflict with TDLib's blocking receive | Medium     | Medium | Dedicated `spawn_blocking` thread for `client_receive`; covered in the implementation guide                                   |

### Success criteria

- [ ] A CipherOcto gateway can send a 100 MB attachment as a DOT envelope fragment through a Telegram group
- [ ] A CipherOcto gateway can receive a 100 MB attachment as a DOT envelope fragment from a Telegram group
- [ ] A CipherOcto user-mode gateway can read its own DMs (E2E-encrypted chat support is Phase 3 future mission, not this)
- [ ] The adapter's `cargo test` passes without requiring a real TDLib instance (uses mock client)
- [ ] The adapter's binary is ≤ 30 MB stripped on Linux x86_64 release
- [ ] The adapter cross-compiles for Linux aarch64, macOS arm64, and Windows x86_64

---

**Supersedes:** `missions/archived/0850f-dot-telegram-adapter.md` (raw Bot API, 744 LOC, 9 tests)
