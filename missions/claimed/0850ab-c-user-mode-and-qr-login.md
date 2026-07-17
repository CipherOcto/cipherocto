# Mission: Pure-Rust MTProto Telegram Adapter — User Mode + QR Login

## Status

Claimed (2026-06-21, agent-assisted)

## RFC

RFC-0850ab-c (Networking): Pure-Rust MTProto Telegram Adapter (Accepted v1.10)
— §"Algorithms / Algorithm 2: User-mode sign-in"
— §"Algorithms / Algorithm 3: QR login"
— §"Roles and Authorities / 3. TelegramUserSigner"
— §"Data Structures / UserAuthLifecycle"

## Parent Mission

[0850ab-c-pure-rust-mtproto-telegram-adapter.md](./0850ab-c-pure-rust-mtproto-telegram-adapter.md)
(Phase 1 Core, claimed and Phase-1-hardened in commit `de48054a`)

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none yet)

## Summary

Implement the user-mode sign-in flow and the QR login flow on top of the
Phase 1 core (`crates/octo-adapter-telegram-mtproto`). Bot mode (Phase 1)
remains the primary auth path; user mode and QR login are the escape
hatches. This sub-mission closes out the items that the parent mission
defers to "Phase 2 / 0850ab-c-user".

The user-mode flow is the standard Telegram two-step login:

```
1. sign_in_user(phone)        → request_login_code (Telegram SMSes a code)
2. submit_code(code)          → may transition to PasswordRequired (2FA)
3. submit_password(password)  → finalises to SignedIn
```

The QR login flow uses Telegram's `tg://login?token=...` URL:

```
1. qr_login()                 → ExportLoginToken → (token, url)
2. caller displays QR         → user scans with phone
3. poll ExportLoginToken      → detects when user is signed in
4. if 2FA                     → submit_password
```

Both flows are stateful. RFC-0850ab-c §"Lifecycle Requirements / UserAuthLifecycle
State Machine" pins the state names so the operator UI can reuse RFC-0850ab-a's
interactive prompts without translation.

## Scope

**In scope:**

- `AuthMode` enum (`BotToken(String)`, `UserCredentials { phone: String }`,
  `QrLogin`) per RFC §"Data Structures". Exposed from the crate root.
- `BotAuthLifecycle` enum (5 states per RFC §"Lifecycle Requirements") —
  `NoToken`, `Validating`, `SignedIn`, `SigningOut`, `SignedOut`. The
  existing `AuthStateKey` (a unified 5-state summary used by
  `AdapterLifecycle::transition`) stays as-is for backward compatibility;
  the new `BotAuthLifecycle` is a more granular role-specific view.
- `UserAuthLifecycle` enum (10 states per RFC §"Data Structures") —
  `NoCredentials`, `PhoneProvided`, `SmsCodeSent`, `SmsCodeProvided`,
  `PasswordRequired`, `PasswordProvided`, `SignedIn`, `SigningOut`,
  `SignedOut`, `QrLoginPending`, `QrLoginConfirmed`.
- User-mode state machine in `auth.rs`: transition function
  `next_user_auth_state(action, current) -> Result<UserAuthLifecycle,
  MtprotoAuthError>` with explicit transition table mirroring
  RFC-0850ab-a §"User Auth State Machine".
- Real grammers wiring in `real_client.rs::RealTelegramMtprotoClient`:
  - `request_login_code`: call `Client::request_login_code(phone,
    api_hash)`, store the returned `LoginToken` in the adapter's user-mode
    state, transition to `SmsCodeSent`.
  - `submit_code`: call `Client::sign_in(&token, code)`. On
    `SignInError::PasswordRequired(_)`, store the `PasswordToken` and
    transition to `PasswordRequired`. On `Ok(User)`, transition to
    `SignedIn` and cache `SelfUserInfo`.
  - `submit_password`: call `Client::check_password(password_token,
    password)`, transition to `SignedIn` on `Ok(User)`.
  - `qr_login`: call `Client::invoke(&auth::ExportLoginToken{...})`,
    return `QrLoginHandle { token: Vec<u8>, url: String }`.
    `poll_qr_login(handle)` re-invokes `ExportLoginToken`; on `Ok(User)`
    transition to `SignedIn` (or `PasswordRequired` if 2FA was set up on
    the primary device).
- `MtprotoTelegramConfig::auth_mode(&self) -> Result<AuthMode, String>`
  helper that derives an `AuthMode` from the existing flat
  `mode: Option<String>` field plus `bot_token` / `phone`. This is
  additive: existing JSON configs (`{"mode": "bot", ...}`,
  `{"mode": "user", ...}`) keep working.
- Mock implementations of the user-mode flow (already in `client.rs`):
  wire up `MockTelegramMtprotoClient::request_login_code /
  submit_code / submit_password` to drive the user-mode state machine in
  tests. Existing mock behaviour (return `Ok(())` / `Ok(SelfUserInfo)`)
  is sufficient; we add a `MockUserModeState` injectable knob for
  2FA-required simulation.
- Unit tests for the state machine (every valid transition,
  representative invalid transitions).
- Integration tests gated on `INTEGRATION_TESTS=1` (TV-3 happy path,
  TV-4 invalid SMS code, TV-5 2FA flow, TV-9 QR login happy path).
- README + CHANGELOG entries noting the Phase 2 user-mode and QR-login
  status.
- RFC version bump to v1.11 noting the `AuthMode` enum, `BotAuthLifecycle`,
  `UserAuthLifecycle` are now first-class types.

**Out of scope (deferred to sub-missions):**

- SOCKS5 / HTTP CONNECT wrappers (Gap G2) → Mission `0850ab-c-wrappers`
  (conditional).
- Fake-TLS `0xEE` wrapper (Gap G3) → Mission `0850ab-c-wrappers`
  (conditional).
- Temp-key support (Gap G1) → not planned (cipherocto does not need it).
- Bot-API HTTP fallback → Mission `0850ab-c-http`.
- Account-ban detection beyond the existing `RpcError` surface.
- Multi-account management (RFC-0850p-a-multi-account covers that).

## Acceptance Criteria

### AuthMode enum

- [ ] `AuthMode` is a public enum with variants `BotToken(String)`,
      `UserCredentials { phone: String }`, `QrLogin`. Re-exported from
      the crate root.
- [ ] `MtprotoTelegramConfig::auth_mode() -> Result<AuthMode, String>`
      returns:
        - `AuthMode::BotToken(token)` when `mode == "bot"` and
          `bot_token` is set;
        - `AuthMode::UserCredentials { phone }` when `mode == "user"`
          and `phone` is set;
        - `AuthMode::QrLogin` when `mode == "qr"` (or `"qr_login"`).
        - `Err(_)` if the mode string is unknown or required fields are
          missing.
- [ ] Existing JSON configs (without the `auth_mode` field) continue to
      work — `auth_mode()` derives from `mode` + flat fields. No
      on-disk format change.

### BotAuthLifecycle enum

- [ ] `BotAuthLifecycle` is a `#[repr(u8)]` enum with 5 variants
      matching RFC line 329 (`NoToken = 0x00`, `Validating = 0x01`,
      `SignedIn = 0x02`, `SigningOut = 0x03`, `SignedOut = 0x04`).
- [ ] Display + Debug impls, no `#[non_exhaustive]` (fixed shape).
- [ ] At least 1 unit test that every variant round-trips through
      `Display` and `Debug`.

### UserAuthLifecycle enum

- [ ] `UserAuthLifecycle` is a `#[repr(u8)]` enum with 10 variants
      matching RFC line 344-356 (`NoCredentials = 0x00`,
      `PhoneProvided = 0x01`, `SmsCodeSent = 0x02`,
      `SmsCodeProvided = 0x03`, `PasswordRequired = 0x04`,
      `PasswordProvided = 0x05`, `SignedIn = 0x06`,
      `SigningOut = 0x07`, `SignedOut = 0x08`, `QrLoginPending = 0x09`,
      `QrLoginConfirmed = 0x0A`).
- [ ] Display + Debug impls.
- [ ] At least 1 unit test verifying the variant order + repr values
      match the RFC.

### User-mode state machine

- [ ] `next_user_auth_state(action: UserAuthAction, current:
      UserAuthLifecycle) -> Result<UserAuthLifecycle, MtprotoAuthError>`
      is the single source of truth for user-mode transitions. Same
      shape as the existing `MtprotoAuthAction` / `AuthStateKey`
      transition function for bot mode.
- [ ] `UserAuthAction` enum with variants `RequestCode { phone }`,
      `SubmitCode { code }`, `SubmitPassword { password }`,
      `QrLoginStart`, `QrLoginConfirm`, `SignOut`.
- [ ] Transition table covers at minimum:
        - `NoCredentials` → `PhoneProvided` on `RequestCode`
        - `PhoneProvided` → `SmsCodeSent` on request to server success
        - `SmsCodeSent` → `SmsCodeProvided` on `SubmitCode`
        - `SmsCodeProvided` → `PasswordRequired` on server 2FA signal
        - `SmsCodeProvided` → `SignedIn` on server success
        - `PasswordRequired` → `PasswordProvided` on `SubmitPassword`
        - `PasswordProvided` → `SignedIn` on server success
        - `NoCredentials` → `QrLoginPending` on `QrLoginStart`
        - `QrLoginPending` → `QrLoginConfirmed` on server scan
        - `QrLoginConfirmed` → `SignedIn` (or `PasswordRequired`) on
          server auth success
        - any `SignedIn` → `SigningOut` on `SignOut`
        - `SigningOut` → `SignedOut` on session wipe
- [ ] Invalid transitions return `MtprotoAuthError::InvalidTransition
      { from, action }`.
- [ ] Unit tests cover: every valid transition succeeds; every
      representative invalid transition returns `InvalidTransition`.

### Real grammers wiring

- [ ] `RealTelegramMtprotoClient::request_login_code` calls
      `Client::request_login_code(phone, api_hash)` (the grammers API
      takes the phone and api_hash; api_id is internal to the SenderPool).
      Stores the returned `LoginToken` in
      `RealTelegramMtprotoClient::user_login_token` for the subsequent
      `submit_code` call.
- [ ] `RealTelegramMtprotoClient::submit_code` calls
      `Client::sign_in(&login_token, code)`. On
      `SignInError::PasswordRequired(password_token)`, stores the
      password token and returns
      `MtprotoTelegramError::Auth("2FA_REQUIRED".into())` so the
      caller (the adapter) knows to call `submit_password`.
- [ ] `RealTelegramMtprotoClient::submit_password` calls
      `Client::check_password(password_token, password.as_bytes())`.
      On `Ok(User)`, populates `SelfUserInfo` and returns it.
- [ ] `RealTelegramMtprotoClient::qr_login` calls
      `Client::invoke(&tl::functions::auth::ExportLoginToken { api_id,
      api_hash, except_ids: vec![] })`. Returns
      `MtprotoTelegramError::QrLoginHandle { token: Vec<u8>, url:
      String }` (new variant) on success. The URL is built from the
      token's base64 as `tg://login?token=<base64>`.
- [ ] `RealTelegramMtprotoClient::poll_qr_login` re-invokes
      `ExportLoginToken` periodically; returns
      `MtprotoTelegramError::QrLoginHandle` with the new token
      (refreshed for re-display if needed) and `is_authorized()` for the
      success check.
- [ ] On any of the user-mode methods, the bot_token and api_hash are
      NEVER logged at any level (TV-11/12 redaction property is
      preserved).
- [ ] All user-mode methods run without panicking on transient network
      errors; they return `MtprotoTelegramError::Rpc { code, message }`
      on RPC failure.

### Mock + tests

- [ ] Mock already implements `request_login_code / submit_code /
      submit_password` as `Ok(())` / `Ok(SelfUserInfo {..})`. Add a
      `MockUserModeSpec { two_fa_required: bool, expected_code: Option<String>,
      expected_password: Option<String> }` knob to drive more
      realistic adapter tests (invalid code, 2FA flow).
- [ ] At least 5 new unit tests covering: state-machine happy path,
      state-machine 2FA path, state-machine invalid transition,
      config-mode → AuthMode mapping (bot / user / qr), AuthMode
      serde round-trip.
- [ ] Integration tests gated on `INTEGRATION_TESTS=1`:
        - `tv3_user_mode_sign_in_happy_path` — uses mock client with
          `two_fa_required: false`; verifies the adapter transitions
          `NoCredentials → PhoneProvided → SmsCodeSent →
          SmsCodeProvided → SignedIn`.
        - `tv4_invalid_sms_code_returns_error` — uses mock with
          `expected_code: Some("wrong")`; verifies the adapter surfaces
          an error and stays in `SmsCodeSent`.
        - `tv5_2fa_flow` — uses mock with `two_fa_required: true`;
          verifies the adapter transitions `SmsCodeProvided →
          PasswordRequired → PasswordProvided → SignedIn`.
        - `tv9_qr_login_happy_path` — uses mock with a deterministic
          QR token; verifies the adapter transitions `NoCredentials →
          QrLoginPending → QrLoginConfirmed → SignedIn`.
- [ ] `cargo build -p octo-adapter-telegram-mtproto --features
      real-network,integration-test --tests` compiles cleanly.
- [ ] `cargo test -p octo-adapter-telegram-mtproto` passes (52 existing
      tests + ≥5 new = ≥57 tests pass).
- [ ] `cargo clippy -p octo-adapter-telegram-mtproto --all-targets
      --features real-network,integration-test -- -D warnings` is clean.

### Documentation

- [ ] README updated to reflect Phase 2 status (user mode + QR login
      shipped). Existing Phase 1 quick-start remains valid.
- [ ] CHANGELOG entry under `0.2.0`: user-mode sign-in, QR login,
      `AuthMode` / `BotAuthLifecycle` / `UserAuthLifecycle` enums.
- [ ] RFC-0850ab-c bumped to v1.11 with a note that
      `AuthMode` / `BotAuthLifecycle` / `UserAuthLifecycle` are now
      first-class types in the adapter.

### Adversarial review

- [ ] Mission-claim PR includes multi-round adversarial review of the
      implementation (protocol expert + architect + impl engineer +
      security + ops lenses). Reuse the same rubric as the Phase 1
      review (`docs/reviews/`).
- [ ] All review issues fixed before merge.
- [ ] PR description cites RFC-0850ab-c section numbers for each
      design choice.

### Type Coverage (delta vs Phase 1)

For each RFC type in RFC-0850ab-c §"Data Structures", note which
mission implements it. Phase 2 closes the gap on `AuthMode`,
`BotAuthLifecycle`, `UserAuthLifecycle`, and the user-mode/QR-login
algorithms:

| RFC-0850ab-c Type | Implemented By | Status |
|-------------------|----------------|--------|
| `MtprotoTelegramConfig` | Phase 1 | Closed (`config.rs`) |
| `AuthMode` | **This mission (Phase 2)** | Open → Closed |
| `AdapterLifecycle` | Phase 1 | Closed (`lifecycle.rs`) |
| `BotAuthLifecycle` | **This mission (Phase 2)** | Open → Closed |
| `UserAuthLifecycle` | **This mission (Phase 2)** | Open → Closed |
| `TelegramCapabilities` | Phase 1 | Closed (`adapter.rs`) |
| `MtprotoTelegramError` | Phase 1 | Closed (`error.rs`); Phase 2 adds `QrLoginHandle` variant |
| `ProxyConfig` / `ProxyKind` | Phase 1 (type skeleton) → Phase 4 | Partial (types only; impls deferred to `0850ab-c-wrappers`) |
| `StoolapSession` | Phase 1 | Closed (`session.rs`) |
| `MtprotoTelegramAdapter` | Phase 1 | Closed (`adapter.rs`) |
| `SelfHandleFilter` | Phase 1 | Closed (`self_handle.rs`) |
| Algorithm 1 (bot sign-in) | Phase 1 | Closed (`real_client.rs::sign_in_bot`) |
| Algorithm 2 (user sign-in) | **This mission (Phase 2)** | Open → Closed |
| Algorithm 3 (QR login) | **This mission (Phase 2)** | Open → Closed |
| Algorithm 4 (receive batch) | Phase 1 | Closed (`adapter.rs`) |
| Algorithm 5 (send envelope) | Phase 1 | Closed (`adapter.rs`) |
| Bot-API HTTP fallback (`HttpFallbackConfig`) | `0850ab-c-http` | Deferred |

## Location

| Path | Change |
|------|--------|
| `crates/octo-adapter-telegram-mtproto/src/auth.rs` | Add `AuthMode` enum (or new `auth_mode.rs` module); add `UserAuthAction` enum; add `next_user_auth_state` transition function; expand unit tests |
| `crates/octo-adapter-telegram-mtproto/src/lifecycle.rs` | Add `BotAuthLifecycle` (5 states) and `UserAuthLifecycle` (10 states) enums with `Display` + `Debug` impls |
| `crates/octo-adapter-telegram-mtproto/src/real_client.rs` | Replace `NotReady` stubs in `request_login_code / submit_code / submit_password` with actual grammers wiring; add `qr_login` and `poll_qr_login` using `tl::functions::auth::ExportLoginToken` |
| `crates/octo-adapter-telegram-mtproto/src/error.rs` | Add `MtprotoTelegramError::QrLoginHandle { token, url }` variant |
| `crates/octo-adapter-telegram-mtproto/src/client.rs` | Extend `MockTelegramMtprotoClient` with `MockUserModeSpec` to drive user-mode tests |
| `crates/octo-adapter-telegram-mtproto/src/config.rs` | Add `auth_mode(&self) -> Result<AuthMode, String>` helper; existing fields unchanged |
| `crates/octo-adapter-telegram-mtproto/src/lib.rs` | Re-export `AuthMode`, `BotAuthLifecycle`, `UserAuthLifecycle`, `UserAuthAction` |
| `crates/octo-adapter-telegram-mtproto/tests/integration_telegram_mtproto.rs` | Add TV-3, TV-4, TV-5, TV-9 (all `#[ignore]`-gated on `INTEGRATION_TESTS=1`) |
| `crates/octo-adapter-telegram-mtproto/README.md` | Note Phase 2 status (user mode + QR login shipped) |
| `crates/octo-adapter-telegram-mtproto/CHANGELOG.md` | 0.2.0 entry |
| `rfcs/accepted/networking/0850ab-c-pure-rust-mtproto-telegram-adapter.md` | Bump to v1.11; record Phase 2 type additions |

## Complexity

**Medium-Large.** Estimated ~600-1000 LOC of Rust including tests.
Drivers:

- 6 source files touched (`auth`, `lifecycle`, `real_client`, `error`,
  `client`, `config`, `lib`).
- New `UserAuthAction` enum + transition function is a state machine
  with ~15 valid transitions (Phase 1's `MtprotoAuthAction` is similar
  but with 5 actions; this is 6 actions on a 10-state machine).
- Real grammers wiring: 4 RPC entry points (request_login_code,
  submit_code, submit_password, qr_login) + 1 polling entry point
  (poll_qr_login).
- 4 new ignored integration tests gated on `INTEGRATION_TESTS=1`.
- Adversarial review of the implementation (protocol expert +
  architect + impl engineer + security + ops).

## Dependencies

**Required missions (MUST be completed before claim):**

- Mission 0850ab-c (Phase 1) — Phase 1 complete and committed
  (`de48054a`).
- RFC-0850ab-c (Accepted v1.9/v1.10) — exists.

**Required upstream crates (MUST exist in workspace):**

- `octo-network` — for `DeterministicEnvelope`, `BroadcastDomainId`,
  `PlatformAdapter` trait.
- `octo-adapter-telegram-mtproto` Phase 1 — the Phase 1 core this
  mission extends.

**External dependencies (Cargo.toml):**

No new dependencies. All Phase 2 work uses crates already pulled in
by Phase 1: `grammers-client 0.9.0`, `grammers-tl-types 0.9.0`,
`tokio`, `async-trait`, `tracing`, `thiserror`.

The QR login flow uses `tl::functions::auth::ExportLoginToken` /
`tl::functions::auth::ImportLoginToken`. Both TL definitions are
present in `grammers-tl-types-0.9.0/tl/api.tl`; we call them via
`Client::invoke()`. We do NOT add any new dependency.

> **Dependency Validation Rules:**
> 1. Phase 2 depends on Phase 1 (sequential).
> 2. No upstream dependency cycles: Phase 2 does not block Phase 3
>    (`0850ab-c-http`) or Phase 4 (`0850ab-c-wrappers`); they depend
>    on Phase 2.
> 3. No new external dependencies beyond Phase 1.

## Implementation Notes

### 1. AuthMode in config vs auth.rs

`MtprotoTelegramConfig.mode` stays as `Option<String>` (the on-disk
form). Phase 2 adds `AuthMode` as a runtime type in `auth.rs`. The
config exposes `auth_mode() -> Result<AuthMode, String>` which
constructs `AuthMode` from the flat fields. Existing JSON configs
(`{"mode": "bot", "bot_token": "..."}`,
`{"mode": "user", "phone": "..."}`) work without modification.

### 2. Grammers user-mode wiring

Grammers 0.9 exposes:

- `Client::request_login_code(phone, api_hash) -> LoginToken`
- `Client::sign_in(&LoginToken, code) -> Result<User, SignInError>`
  where `SignInError::PasswordRequired(PasswordToken)` is the 2FA
  signal.
- `Client::check_password(PasswordToken, password) -> Result<User,
  SignInError>`.

`RealTelegramMtprotoClient` wraps the `LoginToken` + `PasswordToken`
internally so the trait method signatures (which take `&str` for the
code / password) stay simple. The trait does not need to leak
grammers types.

### 3. Grammers QR login wiring

Grammers 0.9 does NOT expose a `Client::qr_login()` method. The TL
functions `auth.exportLoginToken` and `auth.importLoginToken` are
present in `grammers-tl-types-0.9.0/tl/api.tl`. We call them via
`Client::invoke()`:

```rust
let resp: tl::enums::auth::LoginToken = client.invoke(
    &tl::functions::auth::ExportLoginToken {
        api_id,
        api_hash: api_hash.to_string(),
        except_ids: vec![],
    }
).await?;
```

`ExportLoginToken` returns a 16-byte token + a `expires_at` Unix
timestamp. We expose it as:

```rust
pub struct QrLoginHandle {
    pub token: Vec<u8>,
    pub url: String, // "tg://login?token=<base64>"
    pub expires_at: i64,
}
```

Polling: re-invoke `ExportLoginToken` (idempotent; Telegram rotates
the token on each successful scan). Success = `client.is_authorized()
== true`.

### 4. State-machine design

Same shape as Phase 1's `MtprotoAuthAction` / `AuthStateKey`. The
transition function is pure (no I/O), so it's exhaustively unit-testable:

```rust
pub fn next_user_auth_state(
    action: UserAuthAction,
    current: UserAuthLifecycle,
) -> Result<UserAuthLifecycle, MtprotoAuthError> {
    use UserAuthAction::*; use UserAuthLifecycle::*;
    match (current, action) {
        (NoCredentials, RequestCode { .. }) => Ok(PhoneProvided),
        (PhoneProvided, SubmitCode { .. }) => Ok(SmsCodeSent),
        (SmsCodeSent, SubmitCode { .. }) => Ok(SmsCodeProvided),
        (SmsCodeProvided, SubmitPassword { .. }) => Ok(PasswordRequired),
        (PasswordRequired, SubmitPassword { .. }) => Ok(PasswordProvided),
        (PasswordProvided, SubmitCode { .. }) => Ok(SignedIn),
        // QR login
        (NoCredentials, QrLoginStart) => Ok(QrLoginPending),
        (QrLoginPending, QrLoginConfirm) => Ok(QrLoginConfirmed),
        (QrLoginConfirmed, SubmitCode { .. }) => Ok(SignedIn),
        // Sign out
        (SignedIn, SignOut) => Ok(SigningOut),
        (SigningOut, _) => Ok(SignedOut),
        // All others: InvalidTransition
        (from, action) => Err(MtprotoAuthError::InvalidTransition {
            from: AuthStateKey::from(from),
            action: MtprotoAuthAction::from(action),
        }),
    }
}
```

(`AuthStateKey` / `MtprotoAuthAction` get new `From` impls for the
user-mode types so the unified `AdapterLifecycle::transition(adapter,
auth: AuthStateKey)` API continues to work.)

### 5. Sign-out semantics in user mode

Per RFC §"Security Considerations":

> **sign_out flow (TV-13)** deletes the auth_key row from
> `mtproto_auth_keys` AND the `mtproto_user` row (not just drops the
> in-memory Client); otherwise the SigningOut → SignedOut transition
> is a UX lie.

The Phase 1 implementation already does this for bot mode (calls
`StoolapSession::reset()` which wipes both tables). User mode reuses
the same path.

### 6. 2FA password is never stored

Per RFC §"Security Considerations":

> **2FA passwords are not stored** | User mode auth | Operator must
> re-enter on each sign-in | Documented; matches RFC-0850ab-a
> behavior.

The trait's `submit_password(&self, password: &str)` takes the
password by reference; the password is consumed inside the method and
zeroized after `check_password` returns. No caching, no
`password.to_string()` copy.

### 7. CI matrix

The Phase 2 work does not require new CI targets. The same matrix
that builds Phase 1 also builds Phase 2.

## Reference

### Primary

- `rfcs/accepted/networking/0850ab-c-pure-rust-mtproto-telegram-adapter.md`
  — RFC for this sub-mission (Phase 1 §"Lifecycle Requirements" +
  §"Algorithms 2 and 3").
- `missions/claimed/0850ab-c-pure-rust-mtproto-telegram-adapter.md`
  — parent Phase 1 mission.
- `crates/octo-adapter-telegram-mtproto/CHANGELOG.md` — Phase 1
  release notes.
- `docs/BLUEPRINT.md` — process architecture.

### Cross-RFC

- RFC-0850 (Networking): Deterministic Overlay Transport — for
  `PlatformAdapter` trait.
- RFC-0850ab-a (Networking): Telegram Auth Onboarding CLI — defines
  the user-mode state machine that Phase 2 mirrors (no
  redefinition).
- grammers-client-0.9.0 source
  (`~/.cargo/registry/src/.../grammers-client-0.9.0/src/client/auth.rs`)
  — for the user-mode API surface
  (`request_login_code`, `sign_in`, `check_password`).
- grammers-tl-types-0.9.0 (`tl/api.tl`) — for
  `auth.exportLoginToken` / `auth.importLoginToken`.

### Existing CipherOcto code

- `crates/octo-adapter-telegram/src/auth.rs` — the TDLib-based user-mode
  state machine for cross-reference (TDLib has its own state machine;
  Phase 2 mirrors the names, not the implementation).
- `crates/octo-adapter-telegram-mtproto/src/real_client.rs::sign_in_bot`
  — Phase 1 grammers wiring as the template for user-mode wiring.
- `crates/octo-adapter-telegram-mtproto/src/auth.rs::MtprotoAuthAction`
  — Phase 1 action enum as the template for `UserAuthAction`.

## Sub-Missions (Future)

| Sub-Mission | Phase | Status | Depends On |
|-------------|-------|--------|------------|
| 0850ab-c-http | Phase 3 | Planned | This mission (Phase 2) |
| 0850ab-c-wrappers | Phase 4 (conditional) | Optional | This mission (Phase 2) |

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-21 | Initial sub-mission; Phase 2 user-mode + QR-login implementation. Derived from RFC-0850ab-c §"Algorithms / Algorithm 2 & 3" and the parent mission's Phase-2 deferral list. |

---

**Mission Created:** 2026-06-21
**Parent Mission:** 0850ab-c-pure-rust-mtproto-telegram-adapter (Phase 1, claimed and Phase-1-hardened in commit `de48054a`)
**Parent RFC:** RFC-0850ab-c (Accepted v1.10)
**Estimated Effort:** ~600-1000 LOC including tests; 3-5 days for an experienced Rust contributor with grammers familiarity.
**Implementation Status:** Ready to start — Phase 1 is closed, RFC is Accepted.
