# Matrix Auth Onboarding — Design

**Date:** 2026-06-02
**Mission:** 0850h-a (extends 0850h)
**RFC:** RFC-0850 §8.1

## Context

Mission 0850h implemented `octo-adapter-matrix-sdk` (a `cdylib` plugin wrapping
matrix-rust-sdk 0.17.0) but explicitly deferred authentication. The adapter's
`MatrixConfig` accepts a pre-provisioned `access_token` and the mission notes
read: *"Access token: long-lived, obtained via login or registration API"* —
i.e. auth is someone else's problem. The unchecked acceptance criterion in
0850h is *"Integration test against Synapse/Conduit test homeserver"*, which
is impossible to satisfy without a real auth path that produces a real token.

This mission (0850h-a) closes that gap by introducing a standalone CLI that
authenticates a human operator against a Matrix homeserver and writes a
config file the existing adapter can consume. Once the CLI works, the
integration test becomes implementable.

## Goal

Standalone `octo-matrix-onboard` binary + sibling lib that:

1. Authenticates a human operator against any Matrix homeserver in four modes
   (password, OIDC, SSO, QR).
2. Captures the full set of session material the SDK returns: `access_token`,
   `refresh_token` (when issued), `user_id`, `device_id`.
3. Writes a JSON config file matching the extended `MatrixConfig` schema
   consumed by `octo-adapter-matrix-sdk`.
4. Drives a real-homeserver integration test that exercises the adapter with
   the config the CLI produced.

## Design

### 1. Crate Layout

```
crates/
  octo-matrix-onboard/                # binary crate
    src/
      main.rs                          # clap entry, subcommand dispatch
      cli.rs                           # arg structs (clap derive)
      output.rs                        # config writer (JSON, --stdout)
      modes/
        mod.rs
        password.rs
        oidc.rs                        # also handles sso (MSC3861 SSO is OIDC)
        qr.rs
  octo-matrix-onboard-core/            # lib crate, no binary
    src/
      lib.rs
      session.rs                       # session capture from SDK result
      qrcode_render.rs                 # terminal QR via `qrcode` crate
      oauth_listener.rs                # localhost callback server (axum/hyper)
  octo-adapter-matrix-sdk/
    src/lib.rs                         # extended MatrixConfig
    tests/integration_matrix.rs        # gated by `integration-matrix` feature
```

The CLI binary depends on the core lib, which depends on
`matrix-sdk = "=0.17.0"` (exact pin, same as the adapter). The
integration test also depends on the core lib so it can call the
same auth code without spawning a subprocess.

### 2. Subcommand Tree

```
octo-matrix-onboard login password   --homeserver <URL> --user <ID>
                                    [--password-stdin] [--out <path>]
octo-matrix-onboard login oidc       --homeserver <URL>
                                    [--device-name <NAME>]
                                    [--no-listener] [--port <PORT>]
                                    [--out <path>]
octo-matrix-onboard login sso        --homeserver <URL>
                                    [--device-name <NAME>]
                                    [--no-listener] [--port <PORT>]
                                    [--out <path>]
octo-matrix-onboard login qr         --homeserver <URL>
                                    [--device-name <NAME>]
                                    [--timeout <SECS>]
                                    [--out <path>]
octo-matrix-onboard whoami           --config <path>
octo-matrix-onboard version
```

Defaults: `--out` is `~/.config/octo/matrix.json`; `--port` for the OIDC
listener is 8080; `--timeout` for QR is 300s (matches MSC 4108's default
rendezvous-channel TTL). `--no-listener` is the headless-server path: print
the OIDC URL and the expected redirect URI; operator opens it on any machine,
then pastes the final redirect URL on stdin.

### 3. Schema Change

The adapter's `MatrixConfig` in `crates/octo-adapter-matrix-sdk/src/lib.rs:30-36`
is extended additively:

```rust
pub struct MatrixConfig {
    pub homeserver_url: String,
    pub user_id: String,                // NEW — required by restore_session
    pub device_id: String,              // NEW — required by restore_session
    pub access_token: String,
    pub refresh_token: Option<String>,  // NEW — None if homeserver didn't issue
    pub rooms: Vec<String>,
}
```

Adapter behavior change: call
`client.restore_session(MatrixAuth::UserSession { user_id, device_id, access_token })`
once at startup. If `refresh_token` is `Some`, register a 401-refresh handler
that calls `oauth.refresh_token(...)`, holds the rotated pair in memory, and
retries the request. The on-disk config is **not** rewritten mid-process
(that's mission 0850h-c).

All `Option` fields use `#[serde(skip_serializing_if = "Option::is_none")]`
so the JSON stays clean when a homeserver doesn't issue a refresh token
(some password flows don't).

### 4. Per-Mode Flows

**Password.** Build `Client` against `homeserver_url`, call
`client.login_username(user, password, None, None)`. On success, the SDK
returns a session; `session_capture::extract()` pulls the four fields.

**OIDC.** Build `Client::with_oauth()`. Call `OAuth::login_with_authorization_code()`
to get a `LoginProgress` stream + an authorization URL. Print the URL.
Start the `oauth_listener` on `127.0.0.1:port`. The listener awaits the
redirect, extracts the auth code, and feeds it back to the SDK. Await the
progress stream's terminal event. Extract session.

**SSO.** Same as OIDC, but via `OAuth::login_sso()` (MSC 2964 / MSC 3861).
In modern Matrix (MSC 3861 world), SSO is OIDC with a different
authorization flow; the listener pattern is identical.

**QR (the interesting one).** Build `Client`. Use the SDK's lower-level API:

```rust
let qr_login = client.login_with_qr_code(Some(registration_data));
match qr_login {
    QrCodeData::QrCode(generated) => {
        // New device (us) generates the QR
        let login = generated;
        let bytes = qrcode_render::to_terminal(&login.qr_code_data);
        println!("Scan with Element Android (Settings → Link new device):\n{bytes}");
        login.await_done().await?  // await LoginProgress::Done
    }
    QrCodeData::QrCodeFromBytes(_) => {
        // New device scans — not used in this mission
    }
}
```

Important SDK caveat: matrix-rust-sdk 0.17.0's
`src/authentication/oauth/qrcode/mod.rs` module-docstring claims
*"This currently only implements the case where the new device is scanning
the QR code"*. This is misleading: `LoginWithGeneratedQrCode` IS exposed
via the lower-level types, and that's what we use. The high-level
`OAuth::login_with_qr_code()` only wraps the scan side. We rely on the
lower-level types directly. **Risk:** if a future SDK release moves these
types, mission 0850h-a is the only thing that breaks.

The QR flow uses MSC 4108 (QR code login) with rendezvous channel + device
authorization grant. The CLI generates a QR containing a rendezvous URL and
the OAuth device authorization grant. The operator opens Element Android
(already logged in), goes to "Link new device" (the grant side, implemented
in EXA as `GrantLoginWithScannedQrCode`), scans, and approves. The CLI
receives the session, including access token, refresh token, device ID,
user ID.

### 5. Error Handling

Exit codes:

| Code | Meaning | Retryable? |
|---|---|---|
| 0 | Success | — |
| 1 | Generic (catch-all, paired with human-readable error) | depends |
| 2 | Auth rejected (wrong password, OAuth denied, QR grant cancelled) | no |
| 3 | Homeserver unreachable / DNS / TLS | yes |
| 4 | User cancelled (Ctrl-C, `--no-listener` empty stdin, QR timeout) | no |
| 5 | Bad config (output path unwritable, malformed existing config on `--force`) | no |

Per-mode specifics:

- **OIDC listener**: if `--port` is busy → exit 1 with "port 8080 already in
  use; pass --port to override". If the listener gets a non-200 from the
  IdP → exit 2 with the IdP's error description.
- **QR**: rendezvous channel has a server-defined TTL (MSC 4108, typically
  5 min). CLI imposes its own `--timeout` (default 5 min). On expiry → exit
  4 + "QR expired before scan; re-run".
- **Password**: SDK returns `Http(Unauthorized)` on bad creds → exit 2; the
  SDK distinguishes `M_FORBIDDEN` from network errors, so we don't conflate.

### 6. PII / Secrets

- **Password**: never echoed, never logged. `--password-stdin` is the only
  documented form for password mode; flag-form is rejected at clap level to
  prevent shell-history leaks.
- **OIDC redirect URLs** contain auth codes — printed only in `--no-listener`
  mode (where the operator needs them) and never to a log file.
- **Output file**: written with mode `0600` on Unix
  (`OpenOptions::new().mode(0o600)` on Unix; on Windows we set the default
  user ACL and document that the operator must harden it).
- **Token redaction**: extend the adapter's existing pattern
  (`#[serde(skip_serializing)]` + first-8-chars redaction in `Debug`,
  `lib.rs:42-50`) to the CLI. `tracing-subscriber` follows `RUST_LOG`;
  `--verbose` flips INFO → DEBUG. DEBUG-level messages must still redact
  tokens — enforced by wrapping the SDK's `Client` in a small adapter that
  intercepts `Debug` output.

### 7. Observability

This CLI is one-shot by design, so no metrics, no telemetry. The
`octo-matrix-onboard whoami --config <path>` subcommand is the only live
observability: it loads the config, calls `/whoami` via `matrix-sdk`'s
`Client` directly (not through the adapter cdylib — the CLI is a
standalone binary, see the 0850h-a acceptance criterion for `whoami`),
and prints the resolved user/device. The integration test uses it as a
pre-flight assertion ("config the CLI just wrote is actually valid")
before running the real assertions.

### 8. Re-runnability

Default behavior refuses to overwrite an existing `--out` file. `--force`
overwrites. This catches operator mistakes (re-running against the wrong
homeserver, leaving a stale token) without making the happy path annoying.

### 9. Integration Test

- Location: `crates/octo-adapter-matrix-sdk/tests/integration_matrix.rs`,
  feature-gated `#[cfg(feature = "integration-matrix")]`.
- Driver: `scripts/integration-matrix.sh up|down` — `up` starts Synapse in
  Docker (config: registration enabled, password-only, no rate limits for
  test). Conduit variant is a separate `--homeserver conduit` flag.
- Test flow:
  1. `octo-matrix-onboard login password --homeserver http://localhost:8008
     --user @ci:localhost --password-stdin` (password piped from a fixture
     file).
  2. `octo-matrix-onboard whoami --config ./target/matrix-onboard.json` —
     assert user matches `@ci:localhost`.
  3. Spawn the adapter cdylib with the same config, run `receive_messages`
     for 3s, send a test envelope into a pre-created room, assert it
     round-trips.
- Run: `cargo test -p octo-adapter-matrix-sdk --features integration-matrix
  --test integration_matrix -- --nocapture`. The cargo command itself does
  not gate on branch or label — the gating lives in the CI workflow
  (`.github/workflows/integration.yml` or equivalent), which conditionally
  runs the command based on branch (`next`) or PR label (`[integration]`).
  Per-PR builds skip the entire step.

## Follow-up Missions

| Mission | Scope | Size |
|---|---|---|
| **0850h-b** Matrix Adapter E2EE | Enable `matrix-sdk` E2EE features; CLI gains cross-signing bootstrap, emoji SAS device verification, recovery key (4S) flow; `MatrixConfig` gains `passphrase` (modeled after EXA's `SessionData.passphrase`); acceptance: E2EE-encrypted room messages round-trip | **Large** |
| **0850h-c** File-based refresh rotation | Adapter writes rotated tokens back to disk (atomic rename + lockfile). Needed for long-running daemons that outlive a single token TTL. | Medium |
| **0850h-d** Persistent session storage (stoolap) | Multi-account store backed by `CipherOcto/stoolap` fork (`feat/blockchain-sql` branch) — same dependency line and pattern as `quota-router-core/src/secret_manager.rs`. Schema: one row per `(user_id, device_id)`, columns for tokens, homeserver, login_type, last_used, position. **No raw SQLite.** | Medium |

## Persistence Convention (project-wide)

Any new persistence in CipherOcto uses the `CipherOcto/stoolap` fork (branch
`feat/blockchain-sql`); canonical pattern in `crates/quota-router-core/src/`.
**Never** raw SQLite. The `Cargo.lock` pin is commit
`1ca5d1ae21cf1cfef24899f8fe6a3020ba433687`. Prior mission
`0914-a-stoolap-persistence` documents the convention.

## SDK Risk

matrix-rust-sdk 0.17's qrcode module is documented as "scan-side only" but
lower-level `LoginWithGeneratedQrCode` works. We rely on lower-level types.
If a future SDK release moves them, mission 0850h-a is the only thing that
breaks. Track SDK release notes for `src/authentication/oauth/qrcode/`.

## RFC Cross-References

- **RFC-0850** §8.1 — Matrix adapter contract (0850h is the implementation;
  0850h-a extends the auth boundary that 0850h leaves open).

### RFC Amendment Required

The `MatrixConfig` schema extension (adding `user_id: String`,
`device_id: String`, `refresh_token: Option<String>`) is **not** purely
additive: old configs without the new required fields fail to load, which
is a breaking change to any deployed config file. Three options:

1. **RFC-0850 amendment** — formally update §8.1's config schema to include
   the new fields. Cleanest, but blocks 0850h-a's progress on RFC
   maturation.
2. **Document as breaking change in CHANGELOG** — ship 0850h-a with a
   documented migration step ("re-run `octo-matrix-onboard login` after
   upgrading"). The RFC's schema is then updated in a follow-up amendment.
3. **Defer schema-required fields to a v2 config format** — keep the
   existing file format as v1, introduce v2 alongside, support both
   during transition.

This mission (0850h-a) ships with **option 2** (breaking change documented
in CHANGELOG + one-line migration command). The RFC amendment is a
follow-up. Mission 0850h-d's session-storage migration path can be
revisited under option 3 if multi-version config support becomes
important.

RFC-0850 is currently in `rfcs/draft/`. Per the BLUEPRINT
"implementation requires accepted RFC" rule, mission 0850h-a is placed in
`missions/open/` (not `pending/`) matching the 0850h series' actual
practice, where implementation has proceeded in parallel with RFC
maturation. The 0850h mission is `Implemented` while RFC-0850 is still
`Draft`, so this mission follows the same pattern.
