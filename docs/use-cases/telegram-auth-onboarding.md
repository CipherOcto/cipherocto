# Use Case: Telegram Auth Onboarding

## Problem

CipherOcto operators deploying a Telegram-backed gateway must authenticate against Telegram before the adapter can send or receive DOT envelopes. Currently, authentication is embedded inside the `octo-adapter-telegram` adapter constructor: the operator provides credentials via environment variables (`TELEGRAM_BOT_TOKEN`, `TELEGRAM_API_ID`, `TELEGRAM_API_HASH`, etc.), the adapter spawns a TDLib client, runs the auth state machine to completion, and only then becomes operational.

This coupling creates three problems:

1. **No standalone auth verification.** An operator cannot test whether their credentials are valid without starting the full gateway runtime. If authentication fails (expired bot token, wrong api_hash, 2FA lockout), the failure surfaces as a gateway crash rather than a clear CLI error.

2. **User-mode auth requires a running gateway.** TDLib's user-account authentication is an interactive state machine: WaitPhoneNumber → WaitCode → WaitPassword → Ready. The adapter handles this via `submit_verification_code()` and `submit_password()` calls on a live client, but the operator has no standalone tool to drive this flow. They must start the gateway, observe the auth state, and interact through the gateway's API -- a fragile, undocumented process.

3. **No session management.** Matrix has `octo-matrix-onboard` with `whoami`, `session list`, and multi-account support. Telegram has nothing equivalent. Operators cannot verify that a persisted TDLib session is still valid, list available sessions, or cleanly switch between bot and user accounts.

The result: Telegram gateway onboarding is the hardest adapter to deploy, with no tooling support.

## Stakeholders

- **Primary:** CipherOcto gateway operators deploying Telegram-backed DOT transports
- **Secondary:** CI/CD pipelines that need non-interactive bot-token validation
- **Affected:** `octo-adapter-telegram` maintainers (reduced support burden once tooling exists)

## Motivation

Every other major adapter in CipherOcto has auth separated from transport:

| Adapter    | Auth mechanism                | Standalone tool             |
|------------|-------------------------------|-----------------------------|
| Matrix     | OAuth / password / SSO / QR   | `octo-matrix-onboard`       |
| Discord    | Bot token (env var only)      | N/A (non-interactive)       |
| IRC        | NickServ (in-adapter)         | N/A (trivial)               |
| Telegram   | TDLib state machine           | **None** (this use case)    |

Matrix's `octo-matrix-onboard` proved the pattern: a dedicated CLI binary that runs the auth flow, captures the session, writes a config file, and exits. The adapter then loads the config and restores the session without touching credentials. This separation enables:

- **Pre-flight validation**: test credentials before deploying the gateway
- **Interactive auth in a terminal**: phone + code + 2FA without gateway involvement
- **Session auditing**: `whoami` to verify which account is active
- **Multi-account management** (future): switch between bot and user accounts via a session store

Telegram needs the same.

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Operator can authenticate a bot without starting the gateway | Yes/No | `octo-telegram-onboard bot-setup` exits 0 |
| Operator can authenticate a user account without starting the gateway | Yes/No | `octo-telegram-onboard user-login` exits 0 |
| Operator can verify an existing session | Yes/No | `octo-telegram-onboard whoami` prints identity |
| Config file is loadable by the adapter | Yes/No | `TelegramConfig::validate()` passes on output |
| Auth flow produces no plaintext secrets in logs | Yes/No | tracing redaction layer test |
| CLI exit codes distinguish auth failure from network failure | Yes/No | Exit code table matches matrix-onboard |

## Constraints

- **Must not:** require the `octo-adapter-telegram` cdylib to be loaded (standalone binary)
- **Must not:** store plaintext secrets in the config file beyond what `TelegramConfig` already allows (bot_token, api_hash are plaintext in the adapter's JSON; the onboard tool inherits this contract)
- **Must not:** expose TDLib's internal C++ API to the CLI (all TDLib interaction goes through `tdlib-rs`)
- **Limited to:** TDLib's auth capabilities (no custom Telegram API extensions)
- **Limited to:** the same credential types the adapter already consumes (`TelegramConfig` schema)

## Non-Goals

- **E2E encryption setup** (deferred to a future mission under the Phase 3 E2E chats feature)
- **Secret chat management** (deferred to Phase 3)
- **Multi-account session store** (deferred to a follow-up mission, analogous to `0850h-d`)
- **Webhook configuration** (adapter-level concern, not auth)
- **Group discovery** (adapter-level concern after auth)

## Impact

### What changes if this is implemented

- **New crate:** `crates/octo-telegram-onboard/` (binary) + `crates/octo-telegram-onboard-core/` (library)
- **No adapter changes:** The adapter's `TelegramConfig` schema is already sufficient. The onboard tool writes configs the adapter can consume without modification.
- **Operator workflow change:** Instead of `export TELEGRAM_BOT_TOKEN=... && gateway start`, operators run `octo-telegram-onboard bot-setup` once, then `gateway start --config ~/.config/octo/telegram.json`.

### Breaking changes

None. This is additive tooling.

### Migration path

N/A. Existing env-var-based deployment continues to work. The onboard tool is an opt-in improvement.

## Related RFCs

- RFC-0850: Deterministic Overlay Transport, §8.1 (Platform Adapters)
- RFC-0850ab-a (Networking): Telegram Auth Onboarding (Accepted)

## Related Missions

- Mission 0850ab: DOT Telegram Adapter (TDLib rewrite) -- the adapter this tool serves
- Mission 0850h-a: Matrix Auth Onboarding -- the pattern this tool mirrors
