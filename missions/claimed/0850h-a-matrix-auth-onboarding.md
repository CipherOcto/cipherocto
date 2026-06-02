# Mission: 0850h-a Matrix Auth Onboarding

## Status

Claimed (2026-06-02)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none yet — in progress on branch `next`)

## Summary

Standalone `octo-matrix-onboard` binary + sibling `octo-matrix-onboard-core`
lib that authenticates a human operator against any Matrix homeserver in
four modes (password, OIDC, SSO, QR) and writes a JSON config file matching
the extended `MatrixConfig` schema in `octo-adapter-matrix-sdk` (consumed
by the adapter without modification). Closes the auth gap that mission
0850h explicitly left open, unblocking 0850h's unchecked acceptance
criterion: *Integration test against Synapse/Conduit test homeserver*.

## Design

See `docs/plans/2026-06-02-matrix-auth-onboarding-design.md` (full design).

## Acceptance Criteria

- [ ] `crates/octo-matrix-onboard/` (binary) + `crates/octo-matrix-onboard-core/`
      (lib) compile cleanly with `cargo build --release`
- [ ] clap subcommand tree: `login {password,oidc,sso,qr}`, `whoami`, `version`
- [ ] `octo-matrix-onboard login password` — `m.login.password` via
      `Client::login_username`; password via `--password-stdin` only;
      clap-level rejection of `--password <value>` flag form
- [ ] `octo-matrix-onboard login oidc` — OAuth 2.0 Authorization Code flow
      via `OAuth::login_with_authorization_code()`; localhost callback
      listener on `127.0.0.1:port`; `--no-listener` mode for headless servers
- [ ] `octo-matrix-onboard login sso` — modern Matrix SSO via
      `OAuth::login_sso()` (MSC 2964 / MSC 3861); same listener pattern as oidc
- [ ] `octo-matrix-onboard login qr` — `LoginWithGeneratedQrCode` from the
      SDK's lower-level API (CLI generates, existing client scans);
      rendered to terminal via the `qrcode` crate (unicode half-block);
      `--timeout` enforced, default 300s
- [ ] `octo-matrix-onboard whoami --config <path>` — load config, call
      `/whoami` via `matrix-sdk`'s `Client`, print user/device
      (the CLI is a standalone binary and does **not** call through the
      adapter cdylib; the adapter is loaded by a host process, not by
      the CLI)
- [ ] CLI captures `access_token`, `refresh_token` (when present), `user_id`,
      `device_id` from the SDK result
- [ ] Output: JSON to `--out` (default `~/.config/octo/matrix.json` on
      Unix, `%APPDATA%\octo\matrix.json` on Windows — detected via
      `dirs::config_dir()` from the `dirs` crate) or `--stdout`; refuses
      to overwrite existing file unless `--force` set
- [ ] `MatrixConfig` extended: `user_id: String`, `device_id: String`,
      `refresh_token: Option<String>`; all existing fields preserved;
      `Option` fields use `#[serde(skip_serializing_if = "Option::is_none")]`.
      Note: 0850h-c will add two more fields (`config_path: PathBuf`,
      `force_writeback: bool`); the 0850h-a changes must be additive
      in a way that does not constrain 0850h-c's additions (e.g., do
      not reorganize field order, do not change serde tag/untagged
      behavior, do not introduce a sealed marker).
- [ ] Adapter uses extended config: `client.restore_session(MatrixAuth::UserSession
      { user_id, device_id, access_token })`
- [ ] In-memory refresh on 401: when SDK returns `Http(Unauthorized)` and
      `refresh_token.is_some()`, call `oauth.refresh_token(...)`, hold
      rotated pair in memory, retry the request; on-disk config is NOT
      rewritten mid-process (deferred to mission 0850h-c)
- [ ] Output file mode `0600` on Unix; documented Windows caveat
- [ ] tracing-subscriber with a token-redaction layer: custom
      `Layer<S>` impl that redacts event fields whose names match
      `access_token` / `refresh_token` / `password` / `secret`
      (case-insensitive) and event messages that contain such
      substrings, before forwarding to the inner `fmt::Layer`
- [ ] Exit codes: 0 success, 1 generic, 2 auth-rejected, 3 homeserver-unreachable,
      4 user-cancelled, 5 bad-config
- [ ] Integration test in
      `crates/octo-adapter-matrix-sdk/tests/integration_matrix.rs`,
      feature-gated `integration-matrix`
- [ ] `scripts/integration-matrix.sh up|down` driver takes
      `--homeserver {synapse|conduit}` (a test-fixture switch, not the
      same as the CLI's `--homeserver <URL>`); Synapse in Docker by
      default, Conduit via the flag
- [ ] Integration test asserts: whoami, room-join, envelope round-trip
- [ ] Unit tests for each mode's token-capture logic
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] No regression in mission 0850h's existing unit tests (the test
      count may grow but no test that was passing at 0850h's
      `Implemented` status may fail or be removed)
- [ ] Integration test passes against both Synapse (default) and Conduit
      (`--homeserver conduit` flag) homeserver variants; the
      `scripts/integration-matrix.sh` driver supports both

## Location

- `crates/octo-matrix-onboard/` (new, binary)
- `crates/octo-matrix-onboard-core/` (new, lib)
- `crates/octo-adapter-matrix-sdk/src/lib.rs` (schema extension)
- `crates/octo-adapter-matrix-sdk/tests/integration_matrix.rs` (new)
- `scripts/integration-matrix.sh` (new)

## Complexity

Medium-High (4 auth modes, schema migration, integration test setup)

## Prerequisites

- Mission 0850h: DOT Matrix Adapter (Implemented)
- Mission 0850e: DOT Adapter Registry & Plugin ABI (Implemented)

## Implementation Notes

- **SDK QR caveat**: matrix-rust-sdk 0.17.0's
  `src/authentication/oauth/qrcode/mod.rs` says *"This currently only
  implements the case where the new device is scanning the QR code"*, but
  the lower-level `LoginWithGeneratedQrCode` type IS exposed. We use the
  lower-level API directly. If a future SDK release moves these types,
  this mission is the only thing that breaks.
- **MSC 4108** — QR code login (rendezvous channel + device authorization grant)
- **Schema is a breaking change** — old configs without `refresh_token`/
  `user_id`/`device_id` fail to load (forces explicit re-onboarding after
  upgrade). The "additive" framing is misleading: a config file produced
  by mission 0850h's old CLI cannot be consumed by 0850h-a's new code.
  Document the migration ("re-run `octo-matrix-onboard login` after
  upgrade") in CHANGELOG. See design doc §RFC Cross-References for the
  three options considered and why option 2 (documented breaking change
  + RFC amendment as follow-up) was chosen.
- **Refuse to overwrite** `--out` unless `--force` set
- **PII rule**: never log secrets, passwords, keys. Matrix IDs (`user_id`,
  `device_id`) are safe to log.
- **OIDC listener uses axum** (or hyper directly if a leaner dep is needed);
  bound to `127.0.0.1`, never `0.0.0.0`
- **QR rendering** uses the `qrcode` crate's unicode half-block output
- **Cargo deps for the new crates**:
  - `clap = { version = "4.5", features = ["derive"] }`
  - `tokio = { version = "1.35", features = ["full"] }`
  - `matrix-sdk = { version = "=0.17.0", default-features = false,
    features = ["rustls-tls", "api"] }` — `api` is required for the
    lower-level auth APIs (`LoginWithGeneratedQrCode`,
    `OAuth::login_*`); the binary does **not** need the cdylib
    adapter's full feature set. Exact pin (`=0.17.0`) to avoid
    patch drift; the SDK Risk note flags that an automatic patch
    bump to `0.17.x` could break the QR module API.
  - `qrcode = "0.14"`
  - `serde`, `serde_json`, `anyhow`, `tracing`, `tracing-subscriber`
  - `axum` (OIDC listener) — feature-gated if binary size matters

## Additional Requirements

- (none beyond the design)

## Follow-up Missions

- **0850h-b Matrix Adapter E2EE** (Large) — Enable `matrix-sdk` E2EE features;
  CLI gains cross-signing bootstrap, emoji SAS device verification, recovery
  key (4S) flow; `MatrixConfig` gains `passphrase` (modeled after EXA's
  `SessionData.passphrase`); acceptance: E2EE-encrypted room messages
  round-trip
- **0850h-c File-based refresh rotation** (Medium) — Adapter writes rotated
  tokens back to disk (atomic rename + lockfile)
- **0850h-d Persistent session storage (stoolap)** (Medium) — Multi-account
  store backed by `CipherOcto/stoolap` fork (`feat/blockchain-sql` branch);
  pattern from `quota-router-core/src/secret_manager.rs`; no raw SQLite

## Persistence Convention

Any new persistence in CipherOcto uses the `CipherOcto/stoolap` fork
(branch `feat/blockchain-sql`); canonical pattern in
`crates/quota-router-core/src/`. **Never** raw SQLite.

## SDK Risk

matrix-rust-sdk 0.17 qrcode module is documented as "scan-side only" but
lower-level `LoginWithGeneratedQrCode` works. We rely on lower-level types.
If a future SDK release moves them, mission 0850h-a is the only thing that
breaks. Track SDK release notes for `src/authentication/oauth/qrcode/`.

## RFC Status

RFC-0850 is in `rfcs/draft/`. This mission is in `missions/open/`
following the 0850h series' actual practice (mission 0850h is
`Implemented` while RFC-0850 is still `Draft`). See design doc §RFC
Cross-References for the full rationale. The schema-breaking change
above implies a follow-up RFC-0850 amendment once 0850h-a lands.
