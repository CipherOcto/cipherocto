# Mission: Pure-Rust MTProto Telegram Adapter (Core)

## Status

Open

## RFC

RFC-0850ab-c (Networking): Pure-Rust MTProto Telegram Adapter (Draft)

## Summary

Implement the `octo-adapter-telegram-mtproto` crate per RFC-0850ab-c §"Implementation Phases / Phase 1: Core". Deliver a self-contained pure-Rust Telegram transport built on the grammers family of crates, implementing the `PlatformAdapter` trait from RFC-0850 §8.2 with bot-mode auth as the primary path. This is the Phase 1 (Core) mission; Phase 2 (User Mode + QR Login) and Phase 3 (Bot-API HTTP Fallback) ship as separate sub-missions.

The new crate co-exists with the existing TDLib-based `octo-adapter-telegram`. No breaking changes to the existing adapter. A new config field `octo.telegram.adapter = mtproto | tdlib` (default `tdlib`) selects between them at runtime.

**Scope:**

- Bot-mode sign-in (`AuthMode::BotToken(String)`) with a custom `StoolapSession` impl of `grammers_session::Session` (NO use of grammers' built-in `SqliteSession`, per the project-wide cipherocto persistence convention)
- `PlatformAdapter` trait methods: `send_envelope`, `receive_messages`, `canonicalize`, `capabilities`, `domain_id`, `platform_type`, `replay_protection`, `health_check`, `shutdown`, `self_handle`, `upload_media_to_domain`, `download_media`
- Self-handle filter (drop self-originated messages)
- Three lifecycles: `AdapterLifecycle`, `BotAuthLifecycle`, `UserAuthLifecycle` (UserAuthLifecycle skeleton only; full state machine in Phase 2)
- DOT wire-format codec (shared with `octo-network`)
- Session storage in `data_dir/sessions.db` via CipherOcto's stoolap fork on `feat/blockchain-sql` (the project-wide cipherocto persistence convention; closest Accepted RFC precedent: RFC-0914 (Economics): Stoolap-Only Quota Router Persistence); separate FILE from the TDLib adapter's `data_dir/database` (no shared SQLite file, no table-prefix trick); the crate ships with NO `rusqlite`/`sqlx`/`sqlite` dependency
- Error type (`thiserror` enum) covering auth, network, RPC, session, and configuration failures
- Integration tests against the Telegram test DC

**Out of scope (deferred to sub-missions):**

- User-mode sign-in and 2FA flow → Mission `0850ab-c-user`
- QR login → Mission `0850ab-c-user` (combined with user mode)
- Bot-API HTTP fallback → Mission `0850ab-c-http`
- SOCKS5 / HTTP CONNECT wrappers (Gap G2) → Mission `0850ab-c-wrappers` (conditional)
- Fake-TLS `0xEE` wrapper (Gap G3) → Mission `0850ab-c-wrappers` (conditional)
- Temp-key support (Gap G1) → future if a cipherocto use case emerges

## Acceptance Criteria

### Crate structure

- [ ] `crates/octo-adapter-telegram-mtproto/Cargo.toml` exists with grammers deps (`grammers-mtproto 0.9.0`, `grammers-tl-types 0.9.0`, `grammers-client 0.8.x`, `grammers-session 0.9.x` with **NO** `sqlite` feature, `grammers-crypto`) + CipherOcto stoolap fork (`stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }`, per the project-wide persistence convention; closest Accepted RFC: RFC-0914 (Economics))
- [ ] Workspace `Cargo.toml` updated to include the new crate in `members`
- [ ] Crate compiles in isolation: `cargo build -p octo-adapter-telegram-mtproto`
- [ ] Workspace still compiles: `cargo build --workspace`
- [ ] No transitive C/C++ dependencies added (verified by `cargo tree --target x86_64-unknown-linux-gnu --no-default-features | grep -v '^[[:space:]]*├\|└' | grep -E '\.so|\.a|\.dll'` returns empty)

### Module skeleton

- [ ] `src/lib.rs` exposes the public API (`MtprotoAdapter`, `MtprotoAdapterConfig`, `AuthMode`, `AdapterLifecycle`, `BotAuthLifecycle`, `UserAuthLifecycle`, `TelegramCapabilities`, `TelegramError`, `ProxyConfig`, `ProxyKind`)
- [ ] `src/config.rs` defines `MtprotoAdapterConfig` with serde derives (Serialize/Deserialize) matching RFC-0850ab-c §"Data Structures"
- [ ] `src/error.rs` defines `TelegramError` enum with thiserror derives (variants: `AuthKeyUnregistered`, `FloodWait`, `NetworkError`, `RpcError`, `SessionError`, `ConfigError`, `Unsupported`)
- [ ] `src/stoolap_session.rs` implements the `grammers_session::Session` trait backed by CipherOcto's stoolap fork (project-wide persistence convention; closest Accepted RFC: RFC-0914 (Economics)); the schema (`mtproto_auth_keys`, `mtproto_dc_config`, `mtproto_user` tables) matches RFC-0850ab-c §"Specification / Session Storage Schema"; the API follows the `octo-matrix-session-store/src/store.rs::StoolapSessionStore::new` canonical pattern (`stoolap::Database::open(&dsn)` where `dsn = format!("file://{}", path)`, NOT `Database::open(path)`; NB: stoolap's `open` takes a DSN string, not a bare path)
- [ ] `src/mtproto_client.rs` wraps `grammers_client::Client` with cipherocto-specific convenience methods
- [ ] `src/auth.rs` implements `sign_in_bot(token: &str) -> Result<User, TelegramError>` per RFC-0850ab-c §"Algorithms / Algorithm 1" (uses `stoolap_session` for persistence)
- [ ] `src/envelope.rs` implements DOT wire-format codec using `base64::encode_config(URL_SAFE_NO_PAD)` and `octo_network::DeterministicEnvelope::bytes()`
- [ ] `src/adapter.rs` defines `MtprotoAdapter` struct and implements all `PlatformAdapter` trait methods
- [ ] `src/self_handle.rs` implements `SelfHandleFilter` with deterministic integer comparison
- [ ] `src/groups.rs` implements chat discovery (list groups, get chat_id from group_jid)
- [ ] `src/files.rs` stubs out upload/download with `unimplemented!()` deferred to Phase 2 (user mode required for large uploads) — methods return `TelegramError::Unsupported`
- [ ] `src/cleanup.rs` implements graceful shutdown (drop mtsender task, persist session via `stoolap_session`, close stoolap DB)

### PlatformAdapter trait implementation

- [ ] `fn platform_type(&self) -> &'static str { "telegram-mtproto" }` returns the canonical identifier
- [ ] `fn domain_id(&self) -> BroadcastDomainId` computes `BLAKE3("telegram-mtproto:" || adapter_config_hash)` deterministically per RFC-0850ab-c §"Roles and Authorities / 1. TelegramPlatformAdapter"
- [ ] `fn capabilities(&self) -> PlatformCapabilities` returns `TelegramCapabilities` with text_max_chars=4096, upload_max_bytes=2GB (MTProto), download_max_bytes=2GB, user_mode_enabled=false (Phase 1), http_fallback_enabled=false (Phase 3)
- [ ] `fn send_envelope(&self, domain_id, envelope) -> Result<MessageId, SendError>` implements Algorithm 5 (text or file based on encoded length)
- [ ] `fn receive_messages(&self, domain_id) -> Vec<RawPlatformMessage>` implements Algorithm 4 (drain channel, canonicalize, self-filter)
- [ ] `fn canonicalize(&self, update) -> Result<DeterministicEnvelope, CanonicalizeError>` is a pure function of `update`
- [ ] `fn replay_protection(&self, msg_id) -> bool` delegates to grammers' `MessageBox`
- [ ] `fn health_check(&self) -> bool` returns `client.is_authorized()`
- [ ] `fn shutdown(&self) -> Result<(), ShutdownError>` is idempotent and persists session before returning
- [ ] `fn self_handle(&self, sender_id) -> bool` returns `sender_id == self.bot_id`

### Bot-mode auth

- [ ] TV-1 passes: valid bot token signs in, persists auth_key, transitions `NoToken → Validating → SignedIn`, `Uninitialized → Authenticated → Connected`
- [ ] TV-2 passes: invalid bot token returns `Err(AuthKeyUnregistered)`, transitions `NoToken → Validating → Failed`, no auth_key persisted
- [ ] Session is persisted to `data_dir/sessions.db` (CipherOcto stoolap fork) in the `mtproto_auth_keys` table, keyed on `dc_id`; **separate FILE** from the TDLib adapter's `data_dir/database` (no shared SQLite file); re-authentication is required if the operator switches adapters because auth_keys are not portable between TDLib and grammers
- [ ] Subsequent `sign_in_bot` calls with the same config restore from SQLite (no re-authentication needed if auth_key is valid)

### Envelope send/receive

- [ ] TV-6 passes: small envelope (≤4096 chars encoded) sent as Telegram text message
- [ ] TV-7 passes: large envelope (>4096 chars encoded) sent as Telegram file with caption
- [ ] TV-8 passes: 3 incoming updates (1 self) result in 2 `RawPlatformMessage`s returned by `receive_messages`
- [ ] Self-handle filter is deterministic: same input → same output
- [ ] Reconnect logic implemented: network error → `Disconnected` → reconnect with backoff (1s, 2s, 4s, 8s, 16s, 30s, max 5 attempts)
- [ ] TV-10 passes: TCP drop triggers reconnect sequence

### Error handling

- [ ] `TelegramError` is the single error type returned by all public methods
- [ ] `FLOOD_WAIT_X` responses trigger internal pause-and-retry (matrix adapter pattern)
- [ ] `AuthKeyUnregistered` transitions adapter to `Failed` and surfaces to caller
- [ ] `RpcError` is logged and surfaced as `SendError`
- [ ] `SessionError` (corrupted auth_key) deletes the stoolap DB row and transitions to `Failed`
- [ ] `sign_out` flow (TV-13) deletes the auth_key row from `mtproto_auth_keys` AND the `mtproto_user` row (not just drops the in-memory Client); otherwise the SigningOut → SignedOut transition is a UX lie

### Security invariants

- [ ] **Log redaction test (TV-11, TV-12)** passes: capturing tracing output at INFO+ for any test scenario and grepping for known secret patterns (`[0-9]+:[A-Za-z0-9_-]+` for bot tokens, 32-char hex strings for api_hash, `auth_key`, `password`) returns ZERO matches
- [ ] **sign_out DB cleanup test (TV-13)** passes: after `sign_out()`, `mtproto_auth_keys` has zero rows for the account and `mtproto_user` has zero rows for the account
- [ ] **File permissions test** passes: after init, `data_dir/sessions.db` has mode `0600` (or `0o600` on Unix; equivalent on Windows)
- [ ] **No `rusqlite`/`sqlx`/`sqlite` in `cargo tree`** (verifies no transitive SQLite dep slipped in)
- [ ] tracing crate is used (NOT `println!`/`eprintln!`) for all output; CI grep enforces this

### Integration

- [ ] `crates/octo-adapter-telegram/src/config.rs` accepts `adapter_kind: AdapterKind` enum (`Mtproto`, `Tdlib`) with default `Tdlib` (no breaking change)
- [ ] Existing `octo-adapter-telegram` tests still pass (no regression)
- [ ] Integration test `tests/integration_telegram_mtproto.rs` signs in against Telegram test DC, sends a message, receives a message, verifies replay protection
- [ ] CI runs `cargo build -p octo-adapter-telegram-mtproto` and `cargo test -p octo-adapter-telegram-mtproto` on linux, macOS, Windows

### Documentation

- [ ] `crates/octo-adapter-telegram-mtproto/README.md` with quick-start (bot mode happy path), crate-level architecture diagram, config schema, limitations
- [ ] `.gitignore` template for `TelegramConfig` files containing bot tokens (template commented out to avoid accidental enforcement)
- [ ] Inline docs for every public type and function (`cargo doc -p octo-adapter-telegram-mtproto --no-deps` produces no warnings)
- [ ] CHANGELOG entry noting the crate is `0.1.0` (initial release; user mode and HTTP fallback deferred)

### Adversarial review

- [ ] Mission-claim PR includes multi-round adversarial review of the implementation (same rigor as the research report: protocol expert + architect + impl engineer + security + ops lenses)
- [ ] All review issues fixed before merge
- [ ] PR description cites RFC-0850ab-c section numbers for each design choice

### Type Coverage

For each RFC type defined in RFC-0850ab-c §"Specification / Data Structures", note which mission implements it. Per BLUEPRINT.md Mission template, **no RFC type may be unaccounted for**:

| RFC-0850ab-c Type | Implemented By | Status |
|-------------------|----------------|--------|
| `MtprotoAdapterConfig` | **This mission (Phase 1)** | Open |
| `AuthMode` (enum: `BotToken`, `UserCredentials`, `QrLogin`) | This mission (`BotToken` only); `UserCredentials` + `QrLogin` deferred to sub-mission 0850ab-c-user | Partial |
| `AdapterLifecycle` | **This mission (Phase 1)** | Open |
| `BotAuthLifecycle` | **This mission (Phase 1)** | Open |
| `UserAuthLifecycle` | This mission (Phase 1, enum skeleton only); full state machine in sub-mission 0850ab-c-user | Partial |
| `TelegramCapabilities` | **This mission (Phase 1)** | Open |
| `TelegramError` | **This mission (Phase 1)** | Open |
| `ProxyConfig` / `ProxyKind` | This mission (Phase 1, type skeleton only); SOCKS5/HTTP CONNECT impl in sub-mission 0850ab-c-wrappers (conditional) | Partial |
| `StoolapSession` (custom impl of `grammers_session::Session`) | **This mission (Phase 1)** | Open |
| `MtprotoAdapter` (struct + `PlatformAdapter` impl) | **This mission (Phase 1)** | Open |
| `SelfHandleFilter` | **This mission (Phase 1)** | Open |
| Algorithm 1 (bot sign-in), 4 (receive batch), 5 (send envelope) | **This mission (Phase 1)** | Open |
| Algorithm 2 (user sign-in), 3 (QR login) | Sub-mission 0850ab-c-user | Deferred |
| Bot-API HTTP fallback types (`HttpFallbackConfig`) | Sub-mission 0850ab-c-http | Deferred |
| Wrapper 1 (Gap G1: temp keys) | Skipped for v1 (not needed); future if cipherocto use case emerges | N/A |
| Wrapper 2 (Gap G2: SOCKS5/CONNECT) | Sub-mission 0850ab-c-wrappers (conditional) | Deferred |
| Wrapper 3 (Gap G3: fake-TLS) | Sub-mission 0850ab-c-wrappers (conditional) | Deferred |

**Coverage check:** Every type and algorithm in RFC-0850ab-c §"Specification" is accounted for above. Nothing is unassigned.

## Location

| Path | Change |
|------|--------|
| `crates/octo-adapter-telegram-mtproto/Cargo.toml` | New file; grammers deps (no `sqlite` feature on `grammers-session`) + CipherOcto stoolap fork (`stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }`, per the project-wide persistence convention) + serde + tokio + reqwest (for future use) + base64 + blake3 + thiserror + tracing |
| `crates/octo-adapter-telegram-mtproto/src/lib.rs` | New file; re-exports + dispatch |
| `crates/octo-adapter-telegram-mtproto/src/config.rs` | New file; `MtprotoAdapterConfig` + `AuthMode` + `ProxyConfig` + `ProxyKind` |
| `crates/octo-adapter-telegram-mtproto/src/adapter.rs` | New file; `MtprotoAdapter` + `PlatformAdapter` impl |
| `crates/octo-adapter-telegram-mtproto/src/mtproto_client.rs` | New file; `grammers_client::Client` wrapper |
| `crates/octo-adapter-telegram-mtproto/src/auth.rs` | New file; bot-mode sign-in + lifecycle transitions |
| `crates/octo-adapter-telegram-mtproto/src/envelope.rs` | New file; DOT codec (base64 encode + canonicalize) |
| `crates/octo-adapter-telegram-mtproto/src/error.rs` | New file; `TelegramError` |
| `crates/octo-adapter-telegram-mtproto/src/self_handle.rs` | New file; loop filter |
| `crates/octo-adapter-telegram-mtproto/src/groups.rs` | New file; chat discovery (list groups, lookup chat_id) |
| `crates/octo-adapter-telegram-mtproto/src/files.rs` | New file; upload/download stubs (return `TelegramError::Unsupported` until Phase 2) |
| `crates/octo-adapter-telegram-mtproto/src/cleanup.rs` | New file; graceful shutdown |
| `crates/octo-adapter-telegram-mtproto/src/lifecycle.rs` | New file; `AdapterLifecycle` + `BotAuthLifecycle` + `UserAuthLifecycle` enums |
| `crates/octo-adapter-telegram-mtproto/src/stoolap_session.rs` | New file; custom `StoolapSession` impl of `grammers_session::Session` trait, backed by CipherOcto's stoolap fork (project-wide persistence convention; closest RFC: RFC-0914); ~150 LOC |
| `crates/octo-adapter-telegram-mtproto/README.md` | New file; quick-start + architecture + config |
| `crates/octo-adapter-telegram-mtproto/CHANGELOG.md` | New file; 0.1.0 entry |
| `crates/octo-adapter-telegram-mtproto/tests/integration_telegram_mtproto.rs` | New file; integration test |
| `Cargo.toml` (workspace) | Add `crates/octo-adapter-telegram-mtproto` to `members` |
| `crates/octo-adapter-telegram/src/config.rs` | Add `adapter_kind: AdapterKind` with default `Tdlib` (backward-compatible additive change) |

## Complexity

**Large.** Estimated ~1700-2200 LOC of Rust, excluding tests. Drivers:

- 14 source files (one per module listed above), including the new `src/stoolap_session.rs` (custom `StoolapSession` impl of the `grammers_session::Session` trait, ~150 LOC, per the project-wide persistence convention)
- `PlatformAdapter` trait implementation requires understanding of the 23 sections of `mtproto_port.md` and the grammers analogs
- Bot-mode auth is the happy path but must integrate with the TDLib-style state machine for future user mode
- Error handling must cover 6 distinct error categories
- Session storage requires careful SQLite table isolation
- Three lifecycles (Adapter, BotAuth, UserAuth) require explicit state machine documentation
- Cross-platform CI (linux, macOS, Windows)
- Adversarial review of the implementation (protocol expert + architect + impl engineer + security + ops)

## Dependencies

**Required missions (MUST be completed or in-progress before claim):**

- Mission 0850ab (DOT Telegram Adapter TDLib rewrite) — must be Accepted or superseded by this mission's RFC
- RFC-0850 (DOT parent) — already Accepted
- RFC-0850ab-a (Telegram Auth Onboarding CLI) — already Accepted; defines `TelegramConfig` schema
- RFC-0914 (Economics): Stoolap-Only Quota Router Persistence — Accepted; closest precedent for the project-wide cipherocto persistence convention (the convention itself is documented informally in `crates/octo-matrix-session-store/Cargo.toml` and `crates/octo-matrix-session-store/src/lib.rs`; this mission conforms via the canonical `octo-matrix-session-store/src/store.rs::StoolapSessionStore::new` pattern: `stoolap::Database::open(&dsn)` where `dsn = format!("file://{}", path)`, plus `db.execute(sql, params)` + `db.query(sql, params) -> stoolap::Rows`)

**Required upstream crates (MUST exist in workspace):**

- `octo-network` — for `DeterministicEnvelope`, `BroadcastDomainId`, `PlatformAdapter` trait, `DotGateway`
- `octo-adapter-telegram` (existing TDLib-based) — for `TelegramConfig` schema and `AdapterKind` enum integration

**External dependencies (Cargo.toml):**

- `grammers-mtproto 0.9.0`
- `grammers-tl-types 0.9.0`
- `grammers-client 0.8.x`
- `grammers-session 0.9.x` (NO `sqlite` feature; we ship a custom `StoolapSession` impl of the `Session` trait backed by CipherOcto's stoolap fork per the project-wide persistence convention; closest Accepted RFC: RFC-0914 (Economics))
- `grammers-crypto` (workspace-internal)
- `tokio` (runtime)
- `reqwest` (for future HTTP fallback; unused in Phase 1)
- `base64 0.22`
- `blake3 1.5`
- `thiserror 2.x`
- `tracing 0.1`
- `serde 1.0` + `serde_json 1.0`
- `stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }` (per the project-wide cipherocto persistence convention; closest Accepted RFC: RFC-0914 (Economics); **NOT** `rusqlite` / `sqlx` / `sqlite` — those are reserved for legacy libraries that require them)

> **Dependency Validation Rules:**
> 1. The mission's RFC (0850ab-c) depends on RFC-0850, RFC-0850ab-a, RFC-0850p-c, RFC-0851p-a. All four are Accepted.
> 2. No upstream dependency cycles: this mission does not block any other mission (Phase 2/3 sub-missions depend on this one).
> 3. grammers crates are at the version documented in RFC-0850ab-c Appendix B; pin in `Cargo.toml` and bump only with explicit mission amendment.

## Implementation Notes

### 1. Architecture (per RFC-0850ab-c §"Specification / System Architecture")

The 4-layer architecture maps to 4 modules:

| Layer | Module | LOC estimate |
|-------|--------|--------------|
| grammers MTProto | `mtproto_client.rs` | 200 |
| PlatformAdapter glue | `adapter.rs` | 400 |
| DOT codec | `envelope.rs` | 100 |
| (Bot-API HTTP — deferred) | `http_fallback.rs` | 0 (Phase 3) |

Plus supporting modules: `config.rs` (150), `error.rs` (100), `lifecycle.rs` (100), `stoolap_session.rs` (150; custom `StoolapSession` impl of `grammers_session::Session`, backed by CipherOcto stoolap fork per the project-wide persistence convention), `auth.rs` (250), `envelope.rs` (100), `self_handle.rs` (50), `groups.rs` (100), `files.rs` (50), `cleanup.rs` (50).

Total: ~1700 LOC excluding tests.

### 2. Bot-mode sign-in is the happy path

Phase 1 does NOT implement user-mode sign-in or QR login. The `UserAuthLifecycle` enum exists for forward-compatibility but only `BotAuthLifecycle` is fully transitioned.

This is the matrix adapter's pattern: ship bot mode first, add user mode in a separate mission. Bot mode covers ~90% of cipherocto's use cases (DOT envelopes are typically sent by bots, not user accounts).

### 3. Session storage isolation

The existing `octo-adapter-telegram` (TDLib-based) uses `data_dir/database` for TDLib's own auth database (TDLib is a legacy C++ library that manages its own SQLite file internally; cipherocto does not own that file). The new crate uses `data_dir/sessions.db`, backed by **CipherOcto's stoolap fork on `feat/blockchain-sql`** (per the project-wide persistence convention; closest Accepted RFC: RFC-0914 (Economics)).

Both files live in the same `data_dir` but are **completely separate**. No shared SQLite file, no table-prefix trick. The new crate ships with **no** `rusqlite` / `sqlx` / `sqlite` dependency.

The canonical pattern from `octo-matrix-session-store` applies:

```rust
// src/stoolap_session.rs (sketch — full impl in the mission PR)
use stoolap::Database;

pub struct StoolapSession {
    db: Database,  // opens via Database::open(&dsn); stoolap owns the connection
}

impl StoolapSession {
    pub fn new(path: &Path) -> Result<Self, TelegramError> {
        // NB: stoolap's Database::open takes a DSN string like `file:///path/to.db`,
        // not a bare path. This is the canonical pattern from
        // crates/octo-matrix-session-store/src/store.rs::StoolapSessionStore::new.
        let path_str = path.to_string_lossy().to_string();
        let dsn = if path_str.contains("://") {
            path_str
        } else {
            format!("file://{}", path_str)
        };
        let db = Database::open(&dsn)
            .map_err(|e| TelegramError::SessionError(e.to_string()))?;
        init_schema(&db)?;  // idempotent CREATE TABLE IF NOT EXISTS
        Ok(Self { db })
    }

    fn init_schema(db: &Database) -> Result<(), TelegramError> {
        // NB: stoolap's `db.execute` takes a params argument that is either
        // `()` (empty tuple) for no-params queries, or `Vec<stoolap::core::Value>`
        // for parameterized queries. The canonical pattern from
        // crates/quota-router-core/src/storage.rs and
        // crates/octo-matrix-session-store/src/schema.rs uses this form.
        db.execute(
            "CREATE TABLE IF NOT EXISTS mtproto_auth_keys (
                dc_id INTEGER NOT NULL PRIMARY KEY,
                auth_key BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER NOT NULL
             )",
            (),
        ).map_err(|e| TelegramError::SessionError(e.to_string()))?;
        // ... mtproto_dc_config, mtproto_user ...
        Ok(())
    }

    /// Load the auth_key for a given DC. Used by grammers' transport.
    /// Returns `Ok(None)` if no auth_key is stored for this DC.
    pub fn load_auth_key(&self, dc_id: i32) -> Result<Option<[u8; 256]>, TelegramError> {
        // db.query returns Result<stoolap::Rows, Error>; Rows is iterable.
        // Iterate with `for row in rows` or `rows.next()` (Option<Result<stoolap::ResultRow>>).
        let mut rows = self.db.query(
            "SELECT auth_key FROM mtproto_auth_keys WHERE dc_id = $1",
            vec![stoolap::core::Value::integer(dc_id as i64)],
        ).map_err(|e| TelegramError::SessionError(e.to_string()))?;
        // ... extract auth_key bytes from the first row ...
        // (full impl in the mission PR)
        unimplemented!()
    }
}

// Implement grammers_session::Session trait by mapping load/save calls to db.query/db.execute.
impl grammers_session::Session for StoolapSession { /* ... */ }
```

**Migration note.** Auth_keys are NOT portable between TDLib and grammers (different key derivation, different storage layout). An operator who switches from the TDLib adapter to the mtproto adapter must re-authenticate once. The reverse direction (mtproto → TDLib) is the same. This is documented in the crate README.

### 4. Error type design

```rust
#[derive(thiserror::Error, Debug)]
pub enum TelegramError {
    #[error("auth key unregistered: token revoked or invalid")]
    AuthKeyUnregistered,

    #[error("flood wait: {seconds}s remaining")]
    FloodWait { seconds: u32 },

    #[error("network error: {0}")]
    NetworkError(#[from] std::io::Error),

    #[error("RPC error: code={code:?} message={message}")]
    RpcError { code: Option<i32>, message: String },

    #[error("session error: {0}")]
    SessionError(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("unsupported operation in current mode: {0}")]
    Unsupported(&'static str),
}
```

### 5. Reconnect backoff

```
backoff = [1s, 2s, 4s, 8s, 16s, 30s]
max_attempts = 5
```

After 5 failed reconnect attempts, transition to `Failed` (operator intervention required).

### 6. Self-handle filter

```rust
pub struct SelfHandleFilter {
    self_id: i64,
}

impl SelfHandleFilter {
    pub fn is_self(&self, sender_id: i64) -> bool {
        sender_id == self.self_id
    }
}
```

Integer equality. Deterministic. No allocation. O(1).

### 7. DOT envelope codec

```rust
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

pub fn encode_envelope(envelope: &DeterministicEnvelope) -> String {
    URL_SAFE_NO_PAD.encode(envelope.bytes())
}

pub fn decode_envelope(s: &str) -> Result<DeterministicEnvelope, TelegramError> {
    let bytes = URL_SAFE_NO_PAD.decode(s)
        .map_err(|e| TelegramError::ConfigError(e.to_string()))?;
    DeterministicEnvelope::from_bytes(&bytes)
        .ok_or_else(|| TelegramError::ConfigError("invalid envelope".into()))
}
```

### 8. Integration test strategy

The integration test runs against Telegram's test DC (`149.154.167.50:443`). It:

1. Creates a `MtprotoAdapter` with a test bot token (from a CI secret, not committed).
2. Signs in (TV-1 happy path).
3. Sends a message to a test group.
4. Receives a message from another test account.
5. Verifies self-handle filter drops self-originated messages (TV-8).
6. Verifies health check returns true.
7. Shuts down gracefully.

The test is gated on the `INTEGRATION_TESTS=1` env var (matches existing cipherocto convention) so it doesn't run on every CI build.

### 9. No breaking changes to existing adapter

`crates/octo-adapter-telegram/src/config.rs` adds a new optional field:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TelegramConfig {
    // ... existing fields ...

    /// Adapter kind (mtproto or tdlib). Default: tdlib.
    #[serde(default = "default_adapter_kind")]
    pub adapter_kind: AdapterKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AdapterKind {
    Tdlib,
    Mtproto,
}

fn default_adapter_kind() -> AdapterKind {
    AdapterKind::Tdlib
}
```

Existing TOML/JSON configs without `adapter_kind` continue to use `Tdlib`. New configs can opt in via `adapter_kind = "mtproto"`.

### 10. CI matrix

Add the following to the existing CI workflow (per target):

```yaml
strategy:
  matrix:
    target:
      - x86_64-unknown-linux-gnu
      - aarch64-unknown-linux-gnu
      - x86_64-apple-darwin
      - aarch64-apple-darwin
      - x86_64-pc-windows-msvc
```

Build: `cargo build -p octo-adapter-telegram-mtproto --target ${{ matrix.target }}`
Test: `cargo test -p octo-adapter-telegram-mtproto --target ${{ matrix.target }} --no-run`

## Reference

### Primary

- `rfcs/draft/networking/0850ab-c-pure-rust-mtproto-telegram-adapter.md` — RFC for this mission
- `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md` — research report (6 review rounds, 54 issues fixed, accepted)
- `docs/BLUEPRINT.md` — process architecture; this mission follows §"Implementation" lifecycle

### Cross-RFC

- RFC-0850 (Networking): Deterministic Overlay Transport — for `PlatformAdapter` trait
- RFC-0850ab-a (Networking): Telegram Auth Onboarding CLI — for `TelegramConfig` schema (future Phase 2 will reuse interactive prompts)
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — for `GroupState`, `domain_id` semantics
- RFC-0914 (Economics): Stoolap-Only Quota Router Persistence — Accepted precedent for the project-wide cipherocto stoolap-fork persistence convention (the convention itself is informal; this mission conforms via the canonical `octo-matrix-session-store/src/store.rs` pattern: `stoolap::Database::open(&dsn)` with `dsn = format!("file://{}", path)`, plus `db.execute(sql, params)` + `db.query(sql, params) -> stoolap::Rows`)

### Existing CipherOcto code

- `crates/octo-adapter-telegram/` — the existing TDLib-based adapter (uses `rusqlite` ONLY because TDLib requires it; legacy)
- `crates/octo-matrix-session-store/` — canonical pattern for stoolap-backed session storage; reference for `Database::open(path)` + `db.execute(sql, params)` + `db.query(sql, params) -> stoolap::Rows` API usage
- `crates/octo-adapter-matrix/` — architectural reference (similar adapter pattern; uses matrix-rust-sdk)
- `crates/octo-network/` — for `DeterministicEnvelope`, `BroadcastDomainId`, `PlatformAdapter` trait

### External

- grammers book: https://loers.github.io/grammers/ (in-progress; current best reference is `grammers-client/examples/`)
- dgrr/tgcli: https://git.hipsterbrown.com/misc/tgcli — production Telegram CLI on grammers
- `crates/octo-adapter-telegram/CHANGELOG.md` — for the existing adapter's release notes (avoid duplicate mention)

## Sub-Missions (Future)

| Sub-Mission | Phase | Status | Depends On |
|-------------|-------|--------|------------|
| 0850ab-c-user | Phase 2 | Planned | This mission (Phase 1) |
| 0850ab-c-http | Phase 3 | Planned | This mission (Phase 1) |
| 0850ab-c-wrappers | Phase 4 (conditional) | Optional | This mission (Phase 1) |

Each sub-mission follows the same template (Status, RFC, Summary, Acceptance Criteria, etc.) and references the parent RFC-0850ab-c.

---

**Mission Created:** 2026-06-21
**Parent RFC:** RFC-0850ab-c (Draft, awaiting acceptance)
**Source Research:** `docs/research/2026-06-21-telegram-pure-rust-mtproto-adapter.md` (accepted)
**Estimated Effort:** ~1700-2200 LOC, 1-2 weeks for an experienced Rust contributor with grammers familiarity; 3-4 weeks for someone new to MTProto
