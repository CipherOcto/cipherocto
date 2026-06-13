# Mission: 0850p-a WhatsApp Auth Onboarding CLI

## Status

Open (2026-06-12)

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding CLI (`rfcs/draft/networking/0850p-a-whatsapp-auth-onboarding.md`)

## Dependencies

- **Mission 0850p:** DOT WhatsApp Adapter (Implemented) — the `WhatsAppConfig` schema this tool produces, and the `WhatsAppWebAdapter` runtime methods (`start_bot`, `self_handle`, `Event::Connected` handler)
- **Mission 0850e:** DOT Adapter Registry & Plugin ABI (Implemented)
- **Mission 0850h-a:** Matrix Auth Onboarding (Implemented — architectural reference for binary+core split, clap surface, redaction layer, exit code table)
- **Mission 0850ab-a:** Telegram Auth Onboarding (Claimed — architectural reference for TDLib-style state machine mapping; this mission is the event-driven analog)

## Claimant

@unclaimed (RFC drafted by @mmacedoeu, agent-assisted)

## Pull Request

(none)

## Summary

Standalone `octo-whatsapp-onboard` binary + sibling `octo-whatsapp-onboard-core` lib that authenticates a CipherOcto operator against WhatsApp Web via the `whatsapp-rust` protocol crate in two modes (qr-link, pair-link), verifies sessions (whoami), and writes a JSON config file matching the `WhatsAppConfig` schema in `octo-adapter-whatsapp`. Closes the auth tooling gap for the second-largest CipherOcto adapter by transport priority, bringing WhatsApp to parity with Matrix's `octo-matrix-onboard` and Telegram's `octo-telegram-onboard` patterns. Adapts the auth-onboarding template to WhatsApp's event-driven model (`Bot::on_event` emits `PairingQrCode` / `PairingCode` / `Connected` / `LoggedOut`, no stdin prompts).

## Design

See RFC-0850p-a (`rfcs/draft/networking/0850p-a-whatsapp-auth-onboarding.md`) for the full specification. Companion doc with code-level patterns: this mission's "Implementation Guide" §.

## Acceptance Criteria

### Phase 1: Core + Session Management

#### Workspace setup

- [ ] `crates/octo-whatsapp-onboard/Cargo.toml` (binary) — depends on `octo-whatsapp-onboard-core`, `octo-adapter-whatsapp = { path = "../octo-adapter-whatsapp" }`, `clap = { version = "4.5", features = ["derive"] }`, `tokio = { version = "1", features = ["full"] }`, `tracing`, `tracing-subscriber`, `serde`, `serde_json`, `anyhow`, `chrono = { version = "0.4", features = ["clock"] }`, `tempfile = "3"`
- [ ] `crates/octo-whatsapp-onboard-core/Cargo.toml` (lib) — depends on `octo-adapter-whatsapp = { path = "../octo-adapter-whatsapp" }` (for `WhatsAppConfig`, `WhatsAppWebAdapter`, `StoolapStore`), `tokio`, `tracing`, `serde`, `serde_json`, `anyhow`, `chrono`, `parking_lot`
- [ ] Verify workspace `Cargo.toml` uses `members = ["crates/*"]` (auto-includes the new crates) — no manual edit required
- [ ] `cargo build --release` passes for both new crates

#### Adapter change (additive)

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs`: add `impl WhatsAppConfig { pub fn validate(&self) -> std::result::Result<(), String> { ... } }` (analogous to `TelegramConfig::validate()` at `octo-adapter-telegram/src/config.rs:94-110`)
- [ ] `validate()` checks: `pair_phone` is E.164 if set, `ws_url` starts with `ws://` or `wss://` if set, `groups` entries are non-empty strings (R1-H1: filesystem writability of `session_path` is a **CLI pre-flight check** in `pair_link::run` / `qr_link::run`, NOT part of `validate()` — `validate()` is a pure in-memory field-shape check analogous to `TelegramConfig::validate()` at `octo-adapter-telegram/src/config.rs:94-110`)
- [ ] All existing `octo-adapter-whatsapp` unit tests pass unchanged (additive change)
- [ ] New unit tests for `validate()` (3-5 cases: malformed phone, malformed ws_url, valid config, empty groups is OK [R1-L1: empty groups is OK because the operator may have no chats to monitor yet; groups can be added later by editing the config], non-empty groups are not validated for JID format — that's a runtime concern for the adapter)

#### Binary structure

- [ ] `crates/octo-whatsapp-onboard/src/main.rs` — clap entry point with subcommand dispatch
- [ ] `crates/octo-whatsapp-onboard/src/cli.rs` — arg structs:
  - `Cli` (root): `verbose: bool`, `command: Command`
  - `Command` enum: `QrLink(QrLinkArgs)`, `PairLink(PairLinkArgs)`, `Whoami(WhoamiArgs)`, `Session { action: SessionAction }`, `Version`
  - `QrLinkArgs`: `session_path: PathBuf`, `groups: Vec<String>`, `ws_url: Option<String>`, `out: Option<PathBuf>`, `stdout: bool`, `force: bool`, `timeout: u64` (default 300)
  - `PairLinkArgs`: as `QrLinkArgs` + `phone: String` (required; or `$OCTO_WHATSAPP_PHONE`) + `custom_code: Option<String>` (R1-M3: renamed from `custom_pair_code` to match the SDK's `PairCodeOptions::custom_code` at `octo-adapter-whatsapp/src/adapter.rs:261`; the flag is `--pair-code` and the env var is `$OCTO_WHATSAPP_PAIR_CODE` for operator familiarity, but the field name is `custom_code` to match the SDK)
  - `WhoamiArgs`: `config: PathBuf`
  - `SessionAction` enum: `List(SessionListArgs)`, `Verify(SessionVerifyArgs)`, `Remove(SessionRemoveArgs)`
  - `OutputArgs` (flattened into QrLinkArgs / PairLinkArgs): `out`, `stdout` (conflicts_with `out`), `force` (requires `out`)
- [ ] `crates/octo-whatsapp-onboard/src/logging.rs` — tracing-subscriber init with redaction layer
  - Custom `Layer<S>` marker `RedactLayer` (mirrors `octo-matrix-onboard/src/logging.rs:RedactLayer`)
  - Custom `FormatEvent` impl `RedactingFormat` that walks event fields, applies redaction, writes formatted output
  - `REDACT_KEYS = &["session_path", "pair_phone", "pair_code", "ws_url", "access_token", "pn", "noise_key", "identity_key", "signed_pre_key", "prekey"]` (case-insensitive substring match)
  - Auto-generated pair codes from `Event::PairingCode` are NOT redacted (60s TTL, operator-visible by design)
  - Resolved `self_phone` from `Event::Connected` is NOT redacted (logged with `+E164` prefix, e.g., `+1 555 123 4567`)
- [ ] `crates/octo-whatsapp-onboard/src/error.rs` — `OnboardError` enum:
  ```rust
  pub enum OnboardError {
      Generic(anyhow::Error),           // exit 1
      AuthRejected(String),             // exit 2 — Event::LoggedOut during link
      Unreachable(String),              // exit 3 — WebSocket connect fail, DNS
      Cancelled(String),                // exit 4 — timeout, Ctrl-C
      BadConfig(String),                // exit 5 — unwritable path, invalid phone
      RateLimited(String),              // exit 6 — WhatsApp backoff (rare)
      SessionExpired(String),           // exit 7 — Event::LoggedOut after link
  }
  ```
  - `pub fn exit_code(&self) -> u8` mapping (matches RFC table)
  - `pub fn as_exit_code(&self) -> std::process::ExitCode`
  - 7 distinct exit codes (one more than matrix/telegram's 6)
- [ ] `crates/octo-whatsapp-onboard/src/output.rs` — config writer (atomic, 0600, --stdout, --force)
  - `pub fn write(args: &OutputArgs, session: &WhatsAppSession) -> Result<()>`
  - `write_atomic()` uses `tempfile::NamedTempFile` + `persist` (same as `octo-matrix-onboard/src/output.rs:65-118`)
  - `default_path()` returns `~/.config/octo/whatsapp.json` (uses `dirs::config_dir()`)

#### Core library structure

- [ ] `crates/octo-whatsapp-onboard-core/src/lib.rs` — public API + data structures:
  ```rust
  // R1-C2 / R1-M1: WhatsAppSession does NOT derive Serialize/Deserialize.
  // The custom pair code (operator-typed) is NOT a field here — it
  // lives only in pair_link::run()'s local scope. This mirrors
  // octo-matrix-onboard-core::Session making access_token private.
  pub struct QrLinkArgs { pub session_path: PathBuf, pub groups: Vec<String>, pub ws_url: Option<String>, pub timeout_secs: u64 }
  pub struct PairLinkArgs { /* as QrLinkArgs + phone: String + custom_code: Option<String> */ }
  #[derive(Debug, Clone)]
  pub struct WhatsAppSession { pub self_phone: Option<String>, pub session_path: PathBuf, pub groups: Vec<String>, pub pair_phone: Option<String> }
  pub struct SessionInfo { pub session_path: PathBuf, pub self_phone: Option<String>, pub is_valid: bool, pub last_linked_at: Option<chrono::DateTime<chrono::Utc>> }
  pub enum CoreError { /* Read, Parse, InvalidPhone, ClientBuild, SessionExpired, ... */ }
  pub async fn qr_link(args: QrLinkArgs) -> Result<WhatsAppSession, CoreError>
  pub async fn pair_link(args: PairLinkArgs) -> Result<WhatsAppSession, CoreError>
  pub async fn whoami(session_path: &Path) -> Result<WhatsAppSession, CoreError>
  pub async fn list_sessions(base_dir: &Path) -> Result<Vec<SessionInfo>, CoreError>
  pub async fn verify_session(session_path: &Path) -> Result<bool, CoreError>
  ```
- [ ] `crates/octo-whatsapp-onboard-core/src/qr_link.rs` — `pub async fn run(args: QrLinkArgs) -> Result<WhatsAppSession, CoreError>`
  - Validate inputs (`session_path` parent dir creatable, `groups` non-empty strings, `ws_url` starts with `ws://` or `wss://` if set)
  - Build `WhatsAppConfig` stub `{ session_path, groups, ws_url: None, pair_phone: None, pair_code: None }` (R1-H2: `pair_phone` may be pre-set from `$OCTO_WHATSAPP_PHONE`; `pair_code` is set by `pair-link` from `--pair-code` or `$OCTO_WHATSAPP_PAIR_CODE` and is never persisted to disk)
  - Call `WhatsAppWebAdapter::new(config).start_bot().await`
  - Poll `adapter.self_handle()` every 250ms with `--timeout` deadline
  - On success: return `WhatsAppSession { self_phone, session_path, groups, .. }`
- [ ] `crates/octo-whatsapp-onboard-core/src/pair_link.rs` — `pub async fn run(args: PairLinkArgs) -> Result<WhatsAppSession, CoreError>`
  - Validate phone with regex `^\+[1-9]\d{6,14}$` (E.164) — reject with `CoreError::InvalidPhone` otherwise
  - Build `WhatsAppConfig` stub with `pair_phone: Some(phone)`, `pair_code: Some(custom_code_from_arg_or_env)` (R1-C2: the `custom_code` is passed to `WhatsAppWebAdapter` for the link, then **dropped** after `Event::Connected`. It never enters the on-disk config, the sidecar, or the `WhatsAppSession` struct)
  - Same `start_bot` + poll-`self_handle` flow
- [ ] `crates/octo-whatsapp-onboard-core/src/session.rs` — `pub async fn wait_for_connected(adapter: &WhatsAppWebAdapter, timeout: Duration) -> Result<String, CoreError>`
  - Polling loop with 250ms granularity, `tokio::time::sleep` between polls
  - On `Event::LoggedOut` (observable via `self_handle() == None AND bot_handle == None` after deadline): return `Err(CoreError::SessionExpired)`
- [ ] `crates/octo-whatsapp-onboard-core/src/output.rs` — `pub fn to_disk_json(session: &WhatsAppSession) -> serde_json::Value`
  - Field-by-field `serde_json::Map` (mirrors `octo-matrix-onboard-core/src/lib.rs:161-187`)
  - Omits `pair_phone` when `None`, `ws_url` when `None` (matches adapter's `#[serde(default)]` behavior)
  - NEVER serializes `pair_code` (operator-typed, ephemeral)
- [ ] `crates/octo-whatsapp-onboard-core/src/sidecar.rs` — `pub fn write_sidecar(session_path: &Path, session: &WhatsAppSession) -> Result<()>`
  - Writes `session_meta.json` next to the stoolap DB with `{ self_phone, linked_at, mode: "qr-link" | "pair-link", groups }`
  - Atomic write via `tempfile::NamedTempFile` + `persist`
  - Mode 0600 on Unix
- [ ] `crates/octo-whatsapp-onboard-core/src/error.rs` — typed `CoreError` enum (mirrors `octo-matrix-onboard-core/src/lib.rs:22-59`)
  - Variants: `Read { path, source }`, `Parse { path, source }`, `InvalidPhone { value, reason }`, `InvalidSessionPath { path, reason }`, `ClientBuild`, `SessionExpired`, `Timeout { secs }`, `Adapter { source }`

#### Per-subcommand behavior

- [ ] `octo-whatsapp-onboard qr-link --session-path DIR --groups ID,ID --out CONFIG --timeout 300`
  - Validates inputs, builds adapter, starts bot
  - QR code rendered to stderr (via adapter's existing `eprintln!` on `Event::PairingQrCode`)
  - Blocks on `wait_for_connected` until `Event::Connected` or `--timeout`
  - On success: writes `session_meta.json` sidecar + atomic config JSON to `--out`
  - Exits 0
- [ ] `octo-whatsapp-onboard pair-link --phone +15551234567 --out CONFIG`
  - Validates phone (E.164), builds adapter with `pair_phone`, starts bot
  - Pair code printed to stderr (via adapter's existing `eprintln!` on `Event::PairingCode`)
  - Blocks on `wait_for_connected` until `Event::Connected` or `--timeout`
  - On success: writes sidecar + atomic config JSON
  - Exits 0
- [ ] `octo-whatsapp-onboard whoami --config CONFIG`
  - Loads config JSON, extracts `session_path`
  - Builds `WhatsAppWebAdapter` against `session_path`
  - Calls `wait_for_connected` with 10s timeout
  - On success: prints `+{self_phone}` and exits 0
  - On `Event::LoggedOut` / timeout: prints "Session expired or invalid" and exits 7 (`SessionExpired`)
- [ ] `octo-whatsapp-onboard session list --base-dir DIR`
  - Scans `~/.local/share/octo/whatsapp/` (or `--base-dir`)
  - For each `*.session.db`: check for `session_meta.json` sidecar
  - If sidecar exists: parse with `serde_json` and print directly (fast path)
  - If sidecar missing: build adapter, call `self_handle()` with 5s timeout, print result with `<unknown>` for missing fields
  - Tabular output: `SESSION_PATH`, `SELF_PHONE`, `LINKED_AT`, `VALID` columns
  - Exits 0
- [ ] `octo-whatsapp-onboard session verify <DB-PATH>`
  - Builds adapter against `<DB-PATH>`, calls `wait_for_connected` with 10s timeout
  - Prints "valid" or "expired" and exits 0 / 7
- [ ] `octo-whatsapp-onboard session remove <DB-PATH>`
  - Prints "Remove session at <DB-PATH>? [y/N]"
  - Reads confirmation from stdin (single line, case-insensitive)
  - On 'y' / 'yes': deletes the file, prints "removed", exits 0
  - On 'n' / EOF: prints "aborted", exits 0
  - On any other: prints "aborted", exits 0
- [ ] `octo-whatsapp-onboard version`
  - Prints `octo-whatsapp-onboard {CARGO_PKG_VERSION}` and exits 0

#### Tracing redaction

- [ ] Custom `Layer<S>` impl `RedactLayer` — marker, exists for spec compliance (mirrors `octo-matrix-onboard/src/logging.rs:21-25`)
- [ ] Custom `FormatEvent` impl `RedactingFormat` — walks event fields, applies redaction
  - Field name match (case-insensitive) against `REDACT_KEYS`
  - Substring match against event messages
  - Redaction format: `first ≤8 chars + ***` (mirrors `octo-matrix-onboard/src/logging.rs:redact_value`)
  - Char-boundary-walked byte slice (non-ASCII safe)
  - Auto-generated pair codes (e.g., `"ABCD-EFGH"`) are NOT redacted — they're in messages but not in the redaction list
  - Resolved `self_phone` (e.g., `+15551234567`) is NOT redacted
- [ ] `init(verbose: bool)` — installs `RedactingFormat` on the `fmt::Layer`, sets `EnvFilter` to `info` or `debug` based on `--verbose`
- [ ] Unit tests: redaction layer (8-12 cases covering each key, short keys, long keys, case-insensitive match, non-ASCII safety)

#### Unit tests

- [ ] `output.rs`: atomic write, refuse overwrite without `--force`, force overwrite, file mode 0600, bare-filename path (`matrix.json` in cwd)
- [ ] `error.rs`: 7-variant enum tests, `exit_code()` mapping, `as_exit_code()` conversion
- [ ] `cli.rs`: clap parse tests (valid args, missing required, conflicting flags)
- [ ] `logging.rs`: redaction layer tests (8-12 cases)
- [ ] `core/output.rs`: `to_disk_json` round-trip with `WhatsAppConfig`, omit-when-None, never-include-`pair_code` (defense-in-depth: even if a future maintainer adds a `pair_code` field to the in-memory `WhatsAppSession`, the `to_disk_json` function must NOT include it; pin with a unit test)
- [ ] `core/sidecar.rs`: write+read sidecar JSON, atomic write, mode 0600
- [ ] `core/qr_link.rs` / `core/pair_link.rs`: input validation tests (no integration with real WhatsApp — those go in the integration test below)
- [ ] Adapter change: `WhatsAppConfig::validate()` tests (3-5 cases: malformed phone [e.g., `"5551234"`, `"+0123456789"`, `"+1-555-123-4567"`], malformed ws_url [e.g., `"http://example.com"`, `"ftp://example.com"`], valid config with all fields, valid config with empty `groups`, valid config with `ws_url = None` and `pair_phone = None`)
- [ ] `core/session.rs`: `wait_for_connected` polls `self_handle()` every 250ms (constant `POLL_INTERVAL_MS`); unit test pins the constant to 250ms ± 10ms to catch accidental changes (R1-M2)
- [ ] `cli.rs`: `PairLinkArgs` accepts `--phone` from CLI arg OR `$OCTO_WHATSAPP_PHONE` env var; CLI arg wins if both are set; unit test for env-var-only form (R1-H2)

#### Adapter compatibility

- [ ] Onboard output deserializes successfully into `WhatsAppConfig` (round-trip test)
- [ ] Adapter loads the config in `octo-adapter-whatsapp` unit tests (existing tests pass; new test verifies config-from-onboard → adapter instantiation)
- [ ] No regression in `octo-adapter-whatsapp` existing unit tests (test count may grow but no previously-passing test may fail or be removed)

#### Integration test (feature-gated `integration-whatsapp`)

- [ ] `crates/octo-adapter-whatsapp/tests/integration_whatsapp.rs` — feature-gated `#[cfg(feature = "integration-whatsapp")]`
- [ ] Requires `--ws-url` to a test WebSocket fixture (not a real WhatsApp server)
- [ ] Asserts: `qr-link` exits 0, config JSON matches `WhatsAppConfig` schema, sidecar JSON parses, adapter can load the config
- [ ] Driver: `scripts/integration-whatsapp.sh up|down` — `up` starts a fixture WebSocket server (e.g., `websocat -s 8080`), `down` stops it
- [ ] Run: `cargo test -p octo-adapter-whatsapp --features integration-whatsapp --test integration_whatsapp -- --nocapture`

#### Quality gates

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes for both new crates
- [ ] `cargo fmt -- --check` passes
- [ ] No regression in `octo-adapter-whatsapp` existing tests (the adapter change is additive; the 13 existing tests must all still pass)
- [ ] No regression in `octo-matrix-onboard`, `octo-matrix-onboard-core`, `octo-telegram-onboard`, `octo-telegram-onboard-core` (the workspace build must succeed for all crates)
- [ ] Binary size <20 MB stripped (target from RFC §Performance Targets)

### Type Coverage

| RFC-0850p-a Type | Implemented By |
|------------------|----------------|
| `QrLinkArgs` struct | This mission (onboard-core) |
| `PairLinkArgs` struct | This mission (onboard-core) |
| `WhatsAppSession` struct | This mission (onboard-core) |
| `SessionInfo` struct | This mission (onboard-core) |
| `CoreError` enum | This mission (onboard-core) |
| `OnboardError` enum | This mission (onboard binary) |
| qr-link mode | This mission (onboard-core, reuses `WhatsAppWebAdapter::start_bot` + polls `self_handle`) |
| pair-link mode | This mission (onboard-core, reuses `WhatsAppWebAdapter::start_bot` with `pair_phone` config) |
| Session extraction via `Event::Connected` | This mission (onboard-core, calls `adapter.self_handle()` after adapter's own handler populates it) |
| Config output writer + sidecar | This mission (onboard-core) |
| CLI arg structs (clap) | This mission (onboard binary) |
| Tracing redaction layer | This mission (onboard binary) |
| `WhatsAppConfig::validate()` | Adapter change (additive) |
| `WhatsAppConfig` schema | Mission 0850p (existing, consumed) |

## Implementation Guide

Companion guide for code-level patterns:

- RFC-0850p-a §Algorithms (wait_for_connected pseudocode, sidecar shape, config JSON shape)
- `crates/octo-matrix-onboard/` (binary+core split, clap surface, redaction layer, exit code table — primary reference)
- `crates/octo-telegram-onboard/` (state-machine auth model — TDLib analog, useful for the polling-vs-Notify tradeoff)
- `crates/octo-adapter-whatsapp/src/adapter.rs` (`WhatsAppWebAdapter::start_bot`, `Event::Connected` handler at lines 226-237, `self_handle` at line 434-436)
- `crates/octo-adapter-whatsapp/src/adapter.rs:25-36` (`WhatsAppConfig` schema — the on-disk JSON must deserialize into this)
- `crates/octo-adapter-telegram/src/config.rs:94-110` (`TelegramConfig::validate()` — template for the adapter change)

## Location

- `crates/octo-whatsapp-onboard/` (new, binary)
- `crates/octo-whatsapp-onboard-core/` (new, lib)
- `Cargo.toml` (workspace members update)
- `crates/octo-adapter-whatsapp/src/adapter.rs` (additive: `WhatsAppConfig::validate()`)

## Complexity

Medium (2 auth modes, event-driven wait loop, sidecar JSON, adapter change, integration test with WebSocket fixture)

## Prerequisites

- Mission 0850p: DOT WhatsApp Adapter (Implemented)
- Mission 0850e: DOT Adapter Registry & Plugin ABI (Implemented)
- Mission 0850h-a: Matrix Auth Onboarding (Implemented — architectural reference)
- Mission 0850ab-a: Telegram Auth Onboarding (Claimed — architectural reference for the auth-onboarding template)

## Notes

### Why binary+core split?

Mirrors `octo-matrix-onboard` / `octo-matrix-onboard-core` and `octo-telegram-onboard` / `octo-telegram-onboard-core`. The core library can be reused by integration tests, CI scripts, and a future session-rotation daemon without depending on clap. The binary is a thin CLI shell.

### Why reuse the adapter's `WhatsAppWebAdapter`?

The adapter's `Event::Connected` handler (adapter.rs:226-237) already resolves `device.pn` into `self_phone` and persists the noise-key handshake. The onboard tool reuses this exact flow via `start_bot()` to guarantee:
1. The session DB is created with the same storage backend (`StoolapStore`), transport factory, http client, and device-props override the adapter will use on next start
2. The `device.pn` resolution logic is shared (no drift between onboard-resolved and adapter-resolved phone numbers)
3. The `Event::PairingQrCode` / `Event::PairingCode` rendering is shared (no drift between onboard-rendered and adapter-rendered QR/codes)

Reimplementing the bot in the onboard core would mean duplicating all of the above. Any drift would produce sessions the adapter cannot load on next start (catastrophic CI cost).

### Why polling for `Event::Connected`?

The adapter exposes `self_handle()` as `pub fn self_handle(&self) -> Option<String>` (adapter.rs:434-436) backed by `parking_lot::Mutex<Option<String>>` (adapter.rs:92). There is no signal exposed. A 250ms polling loop is acceptable for an operator-driven wait (typically 2-30s). Adding a `tokio::sync::watch` to the adapter is a cross-crate refactor that belongs in a follow-up mission, not in the auth-onboarding PR.

### Why one extra exit code (`SessionExpired = 7`)?

WhatsApp's `Event::LoggedOut` is ambiguous: it fires for both "link rejected outright" and "session later expired." A single exit code would conflate two operator-recovery paths:
- Code 2 (`AuthRejected`): "check your phone, the link was rejected" → operator retries `qr-link`
- Code 7 (`SessionExpired`): "the session you had is no longer valid" → operator re-links

The matrix/telegram tables don't have this ambiguity because their error models are explicit (HTTP 401 vs. SDK state `Closed`; bot token revoked vs. unregistered phone).

### Why is the `WhatsAppConfig::validate()` change additive (not breaking)?

The `serde` representation is unchanged — existing config files deserialize identically. Adding a method to an `impl` block is **purely additive**: no exhaustive match in the codebase breaks, no `Debug` impl, no `JsonSchema`, no `Eq`/`Hash` impl. This is the **first auth-onboarding-series adapter change** but it is non-breaking. The reason it warrants an RFC amendment is the API surface change (callers can now invoke `validate()` and get field-shape errors earlier), not a backwards-incompatibility. Document the new method in CHANGELOG as "Added `WhatsAppConfig::validate()` for early field-shape validation."

### Why sidecar JSON?

`session list` would otherwise have to build a `WhatsAppWebAdapter` per stoolap DB in the base dir to call `self_handle()` (5s timeout each). For an operator with 10 accounts, that's 50s of waiting. The sidecar is written by `qr-link` / `pair-link` immediately after `Event::Connected`, so `session list` is a fast directory scan + JSON parse. The fallback (bot startup) handles the "operator has a stoolap DB but no sidecar" case (e.g., the DB was copied from another host).

### Persistence Convention

The on-disk config is a JSON file (NOT stoolap). The session storage is the stoolap DB. The sidecar is a JSON file. This matches the matrix/telegram pattern: JSON for config, native DB for session storage, JSON sidecar for fast metadata lookup.

### SDK Risk

`whatsapp-rust` and `wacore` are pinned to the same rev as the adapter (`9734fb2ec544e22b7055147aa3e73b6889e3ff0d` per `octo-adapter-whatsapp/Cargo.toml:11-15`). The onboard core and adapter share the same stoolap session DB format, so a SDK version mismatch would cause the adapter to fail to load a session the onboard tool created. Pin both to the same rev in `Cargo.toml`.

### RFC Status

RFC-0850p-a is in `rfcs/draft/networking/`. Per the BLUEPRINT "implementation requires accepted RFC" rule, this mission is placed in `missions/open/` (not `pending/`) matching the 0850h and 0850ab series' actual practice, where implementation has proceeded in parallel with RFC maturation. The 0850h mission is `Implemented` while RFC-0850 is still `Draft`; the 0850ab-a mission is `Claimed` while RFC-0850ab-a is `Accepted`. This mission follows the same pattern: RFC drafted, mission opened, implementation claimed in parallel.

---

**Mirrors:** `missions/claimed/0850ab-a-telegram-auth-onboarding.md` (Telegram Auth Onboarding), `missions/claimed/0850h-a-matrix-auth-onboarding.md` (Matrix Auth Onboarding)
