# Mission: Telegram Auth Onboarding CLI

## Status

Claimed (2026-06-11)

## RFC

RFC-0850ab-a (Networking): Telegram Auth Onboarding CLI

## Dependencies

- **Mission 0850ab:** DOT Telegram Adapter (TDLib rewrite) -- `TelegramConfig` schema (Implemented)
- **Mission 0850e:** DOT Adapter Registry & Plugin ABI (Implemented)
- **Mission 0850h-a:** Matrix Auth Onboarding -- architectural reference (Implemented)

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none)

## Summary

Standalone `octo-telegram-onboard` binary + sibling `octo-telegram-onboard-core` lib that authenticates a CipherOcto operator against Telegram via TDLib in two modes (bot-setup, user-login), verifies sessions (whoami), and writes a JSON config file matching the `TelegramConfig` schema in `octo-adapter-telegram`. Closes the auth tooling gap identified in the adversarial review of the Telegram adapter, bringing Telegram to parity with Matrix's `octo-matrix-onboard` pattern.

## Design

See RFC-0850ab-a (`rfcs/draft/networking/0850ab-a-telegram-auth-onboarding.md`) for the full specification.

## Acceptance Criteria

### Phase 1: Core + Session Management

- [ ] `crates/octo-telegram-onboard/` (binary) + `crates/octo-telegram-onboard-core/` (lib) compile cleanly with `cargo build --release`
- [ ] clap subcommand tree: `bot-setup`, `user-login`, `whoami`, `session {list, verify, remove}`, `version`
- [ ] `octo-telegram-onboard bot-setup` -- non-interactive bot auth via TDLib:
  - Accepts `--bot-token`, `--api-id`, `--api-hash` (CLI args or env vars)
  - Creates TDLib client, drives auth state machine to `Ready`
  - Calls `tdlib_rs::functions::get_me()` to extract identity
  - Writes `TelegramConfig`-compatible JSON to `--out` (default `~/.config/octo/telegram.json`)
  - Exits 0 on success
- [ ] `octo-telegram-onboard user-login` -- interactive user-account auth via TDLib:
  - Accepts `--api-id`, `--api-hash`, `--phone` (CLI args or env vars)
  - Reads verification code as a single line from stdin (supports pipe input)
  - Reads 2FA password from stdin with echo disabled (via `rpassword` crate) if TDLib emits `WaitPassword`
  - Handles `WaitRegistration` with clear error message + exit 2
  - `--timeout` (default 300s) for interactive flow
  - Writes config + exits 0 on success
- [ ] `--verifying-key <BASE64>` flag (or `$TELEGRAM_VERIFYING_KEY`) on `bot-setup` and `user-login`; included in output JSON when provided
- [ ] Output JSON is loadable by `TelegramConfig` without adapter modification:
  - `TelegramConfig::validate()` passes on output
  - Field names and types match `TelegramConfig` schema exactly
  - `serde_json::to_string_pretty` produces deterministic field order
  - `verifying_key` included when provided, omitted/null otherwise
  - `password` field is NOT written to disk (ephemeral)
- [ ] Output file mode 0600 on Unix; documented Windows caveat
- [ ] `data_dir` default: `~/.local/share/octo/telegram/default/`; resolved before auth begins (TDLib requires it at `set_tdlib_parameters` time)
- [ ] `data_dir` created with mode 0700 on Unix before TDLib initialization
- [ ] Atomic config write via `tempfile::NamedTempFile` + `persist` (same pattern as `octo-matrix-onboard`)
- [ ] `session_meta.json` sidecar written alongside TDLib database (user_id, username, mode) for fast `session list`
- [ ] `whoami` subcommand: loads config JSON, creates TDLib client against existing `data_dir`, calls `get_me()`, prints identity; handles expired sessions (exit code 2)
- [ ] `session list`: scans `~/.local/share/octo/telegram/`, reads sidecar files (fast) or falls back to `get_me()` with 5s timeout, prints account info
- [ ] `session verify <dir>`: checks if a TDLib database has a valid session
- [ ] `session remove <dir>`: deletes a TDLib database directory after confirmation
- [ ] Exit codes: 0 success, 1 generic, 2 auth-rejected, 3 telegram-unreachable, 4 user-cancelled, 5 bad-config, 6 rate-limited
- [ ] Tracing redaction layer: custom `Layer<S>` impl that redacts fields named `bot_token`, `api_hash`, `phone`, `password`, `access_token`, `verifying_key` (case-insensitive) and messages containing such substrings
- [ ] `--verbose` enables DEBUG-level tracing; secrets stay redacted at every level
- [ ] stdin-only for secrets: verification code (line read, echo enabled) and 2FA password (line read, echo disabled via `rpassword`)
- [ ] Auth module imports types from `octo-adapter-telegram::auth` (`AuthStateKey`, `AuthAction`, `AuthError`); user-mode auth additionally uses `UserAuth::decide_key`
- [ ] Unit tests for auth state machine driver (mock TDLib client, reuse adapter's `UserAuth::decide_key` test coverage)
- [ ] Unit tests for config output writer (same test pattern as `octo-matrix-onboard`)
- [ ] Unit tests for error classification and exit codes
- [ ] Unit tests for redaction layer
- [ ] Integration test (feature-gated `integration-telegram`): real bot-setup against Telegram test DC, verify config output, verify adapter can load config
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] No regression in `octo-adapter-telegram` existing tests (test count may grow but no previously-passing test may fail or be removed)

### Type Coverage

| RFC-0850ab-a Type | Implemented By |
|-------------------|---------------|
| `TelegramSession` struct | This mission (onboard-core) |
| `AuthResult` struct | This mission (onboard-core) |
| `SessionInfo` struct | This mission (onboard-core) |
| `OnboardError` enum | This mission (onboard-core) |
| `Credentials` struct | This mission (onboard-core) |
| Auth state machine driver | This mission (onboard-core, bot mode: direct TDLib calls; user mode: reuses adapter's `UserAuth`) |
| Session extraction via `get_me` | This mission (onboard-core, calls `tdlib_rs::functions` directly) |
| Config output writer + sidecar | This mission (onboard-core) |
| CLI arg structs (clap) | This mission (onboard binary) |
| Tracing redaction layer | This mission (onboard binary) |
| `TelegramConfig` schema | Mission 0850ab (existing, consumed) |

## Implementation Guide

Companion guide for code-level patterns:

- RFC-0850ab-a §Algorithms (auth state machine driver pseudocode)
- `crates/octo-matrix-onboard/` (structural reference -- same binary+core split, same CLI patterns)
- `crates/octo-adapter-telegram/src/client.rs` (TDLib client wrapper -- reuse auth state handling)
- `crates/octo-adapter-telegram/src/config.rs` (TelegramConfig schema -- output must match)

## Location

- `crates/octo-telegram-onboard/` (new, binary)
- `crates/octo-telegram-onboard-core/` (new, lib)
- `Cargo.toml` (workspace members update)

## Complexity

Medium (2 auth modes, TDLib state machine driver, session management, tracing redaction)

## Prerequisites

- Mission 0850ab: DOT Telegram Adapter (Implemented)
- Mission 0850e: DOT Adapter Registry & Plugin ABI (Implemented)
- Mission 0850h-a: Matrix Auth Onboarding (Implemented -- architectural reference)

## Notes

### Why binary+core split?

Mirrors `octo-matrix-onboard` / `octo-matrix-onboard-core`. The core library can be reused by integration tests, CI scripts, and future tools without depending on clap. The binary is a thin CLI shell.

### Why reuse adapter's auth module?

The adapter already has a complete, tested auth state machine in `auth.rs` (`UserAuth`, `AuthStateKey`, `AuthAction`). The onboard core imports these types directly rather than reimplementing the state machine. This ensures auth decision logic is tested once and shared. The `decide_key` function is pure (no I/O, no TDLib feature gates) and is unit-testable without a real TDLib client.

### Why call TDLib functions directly?

The onboard core calls `tdlib_rs::functions::*` directly (not through the adapter's `TelegramClient` trait) because:
1. The `TelegramClient` trait has no `get_me()` method
2. The onboard tool needs a short-lived auth-only client, not the adapter's long-running receive-loop wrapper
3. The `TelegramClient` trait is designed for the adapter's message-sending/receiving lifecycle

### TDLib auth state machine

TDLib's auth flow is callback-driven. The onboard tool uses different strategies per mode:

**Bot mode** (no `UserAuth` needed):
1. `WaitTdlibParameters` → call `set_tdlib_parameters()` directly
2. `WaitPhoneNumber` → call `check_authentication_bot_token(creds.bot_token)` directly
3. `Ready` → call `get_me()`, write config

**User mode** (uses `UserAuth::decide_key`):
1. `WaitTdlibParameters` → `SetParameters` → call `set_tdlib_parameters()`
2. `WaitPhoneNumber` → `SendPhone` → call `set_authentication_phone_number()`
3. `WaitCode` → `AwaitCode` → read line from stdin, call `check_authentication_code()`
4. `WaitPassword` → `UsePassword` → read password (echo disabled), call `check_authentication_password()`
5. `WaitRegistration` → `Error(RegistrationRequired)` → print error, exit 2
6. `Ready` → `Ready` → call `get_me()`, write config

### CI-mode bot-setup

For CI pipelines, `bot-setup` is fully non-interactive: all credentials via env vars, no stdin prompts, deterministic exit codes. This is the primary CI use case.

### data_dir timing

TDLib requires `database_directory` at `set_tdlib_parameters` time (the first auth step). The `data_dir` is resolved *before* auth begins, using either the `--data-dir` flag or the default `~/.local/share/octo/telegram/default/`. The operator may rename the directory after auth completes if they want a different name.

### Risk register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| TDLib auth callback races | Low | High | Notify-based sync with timeout; same pattern as adapter |
| getMe() fails after Ready | Low | Medium | Fail fast, don't write config |
| TDLib flood-wait on repeated attempts | Medium | Low | Exit code 6, operator must wait |
| Config schema drift between onboard and adapter | Low | Medium | Shared test: onboard output → adapter validate() |
| C++ build dep for onboard binary | Medium | Low | Already paid by adapter; onboard inherits same tdlib-rs dep |

### SDK Risk

`tdlib-rs` version must match the adapter's pinned version. The onboard tool and adapter share the same TDLib database format, so version mismatch could cause database corruption. Pin both to the same `tdlib-rs` version in `Cargo.toml`.

### Persistence Convention

Any new persistence in CipherOcto uses the `CipherOcto/stoolap` fork (branch `feat/blockchain-sql`). The Phase 3 multi-account session store follows this convention. Phase 1 (this mission) uses filesystem only (TDLib SQLite database + JSON config), which is acceptable for the initial implementation.

---

**Mirrors:** `missions/claimed/0850h-a-matrix-auth-onboarding.md` (Matrix Auth Onboarding)
