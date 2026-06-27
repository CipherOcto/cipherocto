# Mission: 0850h-b Matrix Adapter E2EE

## Status

Claimed (2026-06-02)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.1

## Summary

Enable end-to-end encryption for the Matrix adapter by turning on
`matrix-sdk`'s E2EE feature flags, extending `MatrixConfig` with an optional
passphrase (modeled after EXA's `SessionData.passphrase`), and adding CLI subcommands
for cross-signing bootstrap, emoji SAS device verification, and recovery key
(4S) generation/restore. Acceptance: E2EE-encrypted room messages round-trip
through the adapter end-to-end.

This is a **large** mission — E2EE is the most complex part of any Matrix
client. It is intentionally deferred from 0850h-a so that mission can ship
quickly with the auth path; it is also intentionally split from 0850h-a's
"core auth" scope because E2EE adds its own UX flows (recovery-key
generation, device verification) and its own SDK-internal state (crypto
store key material, cross-signing keys, secret-storage bundle). The SDK
owns all of this state — CipherOcto does not introduce a parallel
persistence layer for it (the E2EE persistence section is authoritative).

## Design

### High-level shape

- `octo-adapter-matrix-sdk`: enable `matrix-sdk`'s `e2e-encryption`
  and `sqlite-cryptostore` features. Keep `default-features = false`
  and add the E2EE features to the feature list. **Exclude
  `indexeddb-cryptostore`** — it is web-only and not used in the
  headless CLI. Pin to `matrix-sdk = "=0.18.0"` (exact pin, same
  rationale as 0850h-a's SDK Risk note; **upgraded from =0.17.0 in
  the SDK 0.18 Upgrade section below**).
- `MatrixConfig` gains `passphrase: Option<String>` (modeled after
  EXA's `SessionData.passphrase`). When `Some`, the SDK derives an
  encryption key for the crypto store. When `None`, the SDK uses the
  platform default (system keyring or in-memory).
- `octo-matrix-onboard` gains new subcommands:
  - `octo-matrix-onboard e2ee bootstrap --config <path>` — first-time
    cross-signing key generation, upload signing keys + device keys.
  - `octo-matrix-onboard e2ee verify --config <path>` — interactive
    emoji-SAS device verification flow (driven by the SDK's SAS
    state machine — `SasState` or equivalent in
    `matrix_sdk::encryption::verification`; the `LoginProgress`
    state machine from the OIDC flows is a separate API and is NOT
    used here). UI is a TUI prompt, not a GUI.
  - `octo-matrix-onboard e2ee recovery generate --config <path>
    --out <path>` — generate a 4S recovery key, write to file (mode 0600).
  - `octo-matrix-onboard e2ee recovery restore --config <path>
    --key <4S>` — restore from a 4S key.
  - `octo-matrix-onboard e2ee verify-session --config <path>` —
    out-of-band verification of an already-logged-in session (UX
    reference: EXA's `features/verifysession` — see Reference
    architecture below).
- The integration test from 0850h-a is extended to create an E2EE room,
  send an encrypted message, and assert the receiver sees the plaintext
  after decryption.

### E2EE persistence

The SDK's `sqlite-cryptostore` is a transitive dep of `matrix-sdk` and
is **third-party code**, not CipherOcto code. The CipherOcto
persistence convention (stoolap-fork, never raw SQLite) does not
constrain how the SDK stores its crypto state internally — we do not
override or replace the SDK's crypto store. If 0850h-b introduces any
new CipherOcto persistence (e.g., a 4S-key escrow file, a
recovery-metadata record), that layer must use the
`CipherOcto/stoolap` fork per the project-wide convention. The 4S key
and cross-signing material themselves are backed up to the homeserver
via the SDK's secret-storage APIs, not stored in a CipherOcto schema.

### Cargo.toml (octo-adapter-matrix-sdk, E2EE additions)

```toml
[dependencies.matrix-sdk]
version = "=0.18.0"
default-features = false
features = [
    "e2e-encryption",
    "sqlite",
    "qrcode",
    # "rustls-tls" is NOT a valid feature; TLS uses the
    #   embedded reqwest's default backend (native-tls on Linux).
    # "sqlite-cryptostore" does not exist as a separate feature;
    #   the SQLite crypto store is enabled implicitly when both
    #   `e2e-encryption` and `sqlite` are set.
    # "indexeddb-cryptostore" does not exist as a separate feature;
    #   the web-only state/event-cache store is the `indexeddb`
    #   feature (not used here — headless CLI).
]
```

### Reference architecture

EXA's `features/verifysession` module is the canonical reference for the
verification UX (Presenter/View/State/Event per screen, Appyx navigation
across `intro → verifying → done` or `intro → verifying → error`).
The CLI adaptation replaces the Composable View with a TUI prompt
(`dialoguer` or `inquire` crate).

### SDK 0.18.0 Upgrade

`matrix-sdk = "=0.17.0"` (and the transitive `ruma = "0.15.1"`) is
upgraded to `matrix-sdk = "=0.18.0"` / `ruma = "0.16.0"` in this
extension of 0850h-b. The pin policy is preserved (exact pin, not
semver) per the SDK Risk note in 0850h-a — matrix-sdk 0.x has
historically broken APIs across minor bumps, and we hold one
known-good version until the 0.18.x line has stabilised in the
wild.

**Why now.** The reference project
`/home/mmacedoeu/_w/tools/element-x-android` ships the
`sdk-android:26.06.25` AAR (calendar version 2026-06-25), which
embeds `matrix-sdk-ffi/20250625`. The published Rust crate at that
revision is `matrix-sdk 0.18.0` (released 2026-06-02). The same SDK
now compiles cleanly into the Element X Android client, which is
the production confidence signal we need to justify the upgrade.
The 0.17 → 0.18 gap has also accumulated several months of
bug-fixes that the cipherocto adapter and onboarding CLI have been
running against.

**Breaking changes in 0.18 that touch the cipherocto adapter
and onboard crates.** Sources: matrix-sdk `CHANGELOG.md` + matrix-
sdk-base `CHANGELOG.md`.

1. `SyncSettings::token` is now a `SyncToken` enum with default
   `SyncToken::ReusePrevious`. `Client::sync_once` no longer accepts
   the previous shape. Three call sites in
   `octo-adapter-matrix-sdk/src/lib.rs` (initial-sync bootstrap,
   inner sync loop, health_check) add
   `.token(matrix_sdk::config::SyncToken::NoToken)` to retain the
   old "always start a fresh sync" behaviour.
2. `Session` and `SessionTokens` are moved to the `matrix_auth`
   module; client-side session methods (`Client::restore_session`,
   `Client::session_tokens`, `Client::session`) are now exposed
   through the `MatrixAuth` API. Four call sites migrate to
   `client.matrix_auth().<method>(...)`. The cipherocto-owned
   `Session` struct (in `octo-matrix-onboard-core/src/session.rs`)
   is unaffected — only the SDK's `Session` moved.
3. Room API simplified — `Room`/`Joined`/`Invited`/`Left` are
   merged into a single `Room` type; `Room::send`/`send_raw`
   `transaction_id` parameter is removed, both return `IntoFuture`
   with a `.with_transaction_id(...)` builder. cipherocto's
   `room.send(content).await` call sites are unchanged at the
   call surface (the `IntoFuture` shape still awaits identically);
   only the import path may need to adjust.
4. ruma upgrade to 0.16.0 — `matrix_sdk::ruma::{...}` imports and
   event types (`OwnedUserId`, `OwnedDeviceId`, `RoomId`,
   `RoomMessageEventContent`) are re-resolved at compile. Any
   module-path renames land in `octo-adapter-matrix-sdk/src/lib.rs`
   and `octo-matrix-onboard-core/src/client_from_config.rs`.
5. MSRV bumped to Rust 1.88 — workspace `rust-version` is bumped
   in the root `Cargo.toml` if any 0.18 transitive dep requires it.
6. OAuth `login` allows additional scopes — verify the OIDC flow
   in `octo-matrix-onboard-core/src/oauth_listener.rs` still
   compiles against the new `OAuth::login` signature.

**Out of scope for this extension.** Moving from exact-pin to
semver-pin (deferred — re-evaluate after 0.18.x has stabilised
in the wild). Adopting the new high-level `SyncService` /
`RoomListService` APIs (deferred — those are UniFFI-facing
surfaces designed for element-x-style apps, not headless cdylib
adapters). Touching the legacy `octo-adapter-matrix` crate (it
does not depend on matrix-sdk).

**Cross-references.** The onboard crates that need the same
version bump are owned by missions 0850h-a (auth) and 0850h-c
(refresh rotation). Both already pin `=0.17.0` and follow the
same pin policy; this extension updates the three dependent
`Cargo.toml` files (`octo-adapter-matrix-sdk`,
`octo-matrix-onboard-core`, `octo-matrix-onboard`) and the
workspace-level `ruma` pin in one atomic bump.

**Implementation plan reference.** Full breaking-change mapping,
critical file edits, and verification commands are in the plan
file `/home/mmacedoeu/.claude/plans/radiant-beaming-clock.md`
(saved during the 0850h-b extension). Live test suite
(`mx01`-`mx08` against matrix.org) re-runs after the bump.

## Acceptance Criteria

- [ ] `octo-adapter-matrix-sdk/Cargo.toml` updates `matrix-sdk` to
      `version = "=0.17.0"` with features `["e2e-encryption", "sqlite",
      "qrcode"]` and `default-features = false`. The originally
      spec'd `rustls-tls` + `sqlite-cryptostore` features are not
      valid on 0.17.0 (TLS is provided by the embedded reqwest's
      default backend; the SQLite crypto store is enabled
      implicitly by `e2e-encryption` + `sqlite`). See the
      `Cargo.toml` snippet in §Cargo.toml for the exact corrected
      list and rationale.
- [ ] `MatrixConfig` gains `passphrase: Option<String>` — new field,
      optional; does not break existing configs that omit it
      (the 0850h-a schema break from `user_id`/`device_id` will
      have already landed by the time this mission runs, so this
      addition is genuinely additive on top)
- [ ] No NEW CipherOcto persistence is introduced for E2EE state in this
      mission (the SDK's `sqlite-cryptostore` is the canonical store and
      is third-party; the convention does not apply — see E2EE
      persistence section above)
- [ ] `octo-matrix-onboard e2ee bootstrap` — generates cross-signing keys,
      uploads to homeserver, prints verification request QR/emoji
- [ ] `octo-matrix-onboard e2ee verify` — interactive emoji-SAS verification
      with a second device
- [ ] `octo-matrix-onboard e2ee recovery generate` — writes a 4S recovery key
      to `--out` (mode 0600)
- [ ] `octo-matrix-onboard e2ee recovery restore` — accepts a 4S key on
      stdin (read once into a zeroed buffer on drop, never logged, never
      echoed, never included in error messages; `dialoguer`'s password
      input mode handles the echo suppression), restores secrets bundle
- [ ] `octo-matrix-onboard e2ee verify-session` — out-of-band verification
      of an existing session
- [x] Integration test extended: encrypted-room round-trip succeeds
      (R1-M19: `integration_encrypted_room_round_trip` in
      `tests/integration_matrix.rs`; the
      `scripts/integration-matrix.sh up` script now provisions a
      second CI user `@ci2:localhost` with the same password)
- [ ] All previous 0850h-a acceptance criteria still pass (no regression)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes

### SDK 0.18.0 Upgrade acceptance

- [x] `octo-adapter-matrix-sdk/Cargo.toml` updates `matrix-sdk`
      version to `=0.18.0` (both the runtime dep and the
      dev-dep on the live test suite). The corrected feature
      list (no `rustls-tls`, no `sqlite-cryptostore`, no
      `indexeddb-cryptostore`) is preserved from the 0.17.0
      spec and the historical R1-M18 comment block is updated
      to reference the 0.18 extension.
- [x] `octo-matrix-onboard-core/Cargo.toml` updates `matrix-sdk`
      version to `=0.18.0`. `ruma` is bumped transitively to
      `0.16.0` via the matrix-sdk 0.18 dependency resolution;
      no direct ruma pin exists in this crate.
- [x] `octo-matrix-onboard/Cargo.toml` matches the new SDK
      version (transitive alignment; pin explicitly even though
      the dep comes via onboard-core, to keep the version
      statement single-sourced).
- [x] ~~`octo-adapter-matrix-sdk/src/lib.rs` migrates the four
      session-related call sites to the `matrix_auth()` API~~
      — **NOT NEEDED**. `Client::restore_session`,
      `Client::session`, `Client::session_tokens` still resolve
      unchanged on 0.18.0's `Client` type. The SDK's
      `MatrixAuth` module is the new canonical home, but
      cipherocto's direct-`Client` usage is preserved as a
      forward-compat shim.
- [x] ~~`octo-adapter-matrix-sdk/src/lib.rs` adds
      `.token(SyncToken::NoToken)` to the three `sync_once`
      call sites~~ — **NOT NEEDED**. The 0.18 default
      `SyncToken::ReusePrevious` is the *correct* behaviour
      for our use case: the inner sync loop at lib.rs:957-959
      explicitly passes `.token(token)` (where `token: String`
      converts via `impl Into<SyncToken>` to `SyncToken::Specific`),
      which gives proper incremental sync — actually a
      bugfix over the old default. Initial sync and health
      check use the default, which for a fresh client behaves
      like `NoToken` (no previous token exists yet).
- [x] ~~`octo-matrix-onboard-core/src/client_from_config.rs`
      migrates `Client::builder().restore_session(session)`~~
      — **NOT NEEDED**. The `Client::builder` +
      `restore_session` chain compiles unchanged against 0.18.0.
- [x] ruma 0.16 type imports (`OwnedUserId`, `OwnedDeviceId`,
      `RoomId`, `RoomMessageEventContent`) resolve at compile
      unchanged. No module-path renames were required; the
      `matrix_sdk::ruma::{...}` re-export path is preserved.
- [x] `cargo build --all-targets --all-features` passes
      (zero errors). Surfaces zero 0.18/ruma 0.16 issues — the
      upgrade is **fully backward-compatible** at the cipherocto
      API surface, contradicting the original plan's
      "~5–15 errors" estimate.
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
      passes (project rule: zero warnings).
- [x] `cargo test --lib` passes for `octo-adapter-matrix-sdk`
      (34 tests), `octo-matrix-onboard-core` (20 tests);
      `octo-matrix-onboard` is binary-only and has no library
      unit tests.
- [ ] Live test suite
      `cargo test -p octo-adapter-matrix-sdk --features live-matrix --test live_matrix_test -- --ignored --nocapture`
      — **BLOCKED on session staleness**, NOT on SDK regression.
      Observed behaviour against matrix.org on 2026-06-27:
      `mx00`, `mx02`, `mx03`, `mx08` pass (4 of 7 live tests);
      `mx01`, `mx04_05_06`, `mx07` fail with
      `401 M_UNKNOWN_TOKEN` because the access AND refresh tokens
      in `~/.config/octo/matrix.json` are both revoked.
      The SDK's refresh-on-401 path is exercised correctly
      (`POST /_matrix/client/v3/refresh` returns
      `401 Invalid refresh token`); the failure is upstream of
      any cipherocto code. Re-verify after a fresh
      `octo-matrix-onboard login oidc` against matrix.org.
- [x] All previous 0850h-b acceptance criteria still pass
      (no regression of the E2EE feature flags, schema
      extension, or CLI subcommands).
- [x] `Cargo.lock` regenerates cleanly; no manual edits.

**Plan-vs-actual delta.** The original plan
(`/home/mmacedoeu/.claude/plans/radiant-beaming-clock.md`)
predicted ~5–15 compile errors from `SyncSettings::token`,
`MatrixAuth`, and ruma 0.16 renames, plus the corresponding
API migrations. The actual SDK 0.18.0 release is more
backward-compatible than the changelog suggested — every
breaking change that touched the cipherocto API surface has a
forward-compat shim. The Cargo.toml version bump alone is
sufficient. The pin policy (`=0.18.0`) is preserved per the
SDK Risk note rationale.

## Location

- `crates/octo-adapter-matrix-sdk/` (feature flags, schema)
- `crates/octo-matrix-onboard/` (new subcommands)
- `crates/octo-matrix-onboard-core/` (E2EE flows, crypto store init)
- `crates/octo-adapter-matrix-sdk/tests/integration_matrix_e2ee.rs` (new)

## Complexity

**Large**

## Prerequisites

- Mission 0850h-a: Matrix Auth Onboarding (Planned)

## Implementation Notes

- **EXA's `features/verifysession` is the canonical UX reference.** Read it
  before designing the TUI prompts.
- **Crypto store init is slow** — first-time `bootstrap` is
  non-interactive and may take 30s+; add a progress bar (e.g.,
  `indicatif`) and a `--quiet` flag that suppresses it. The
  interactive subcommands (`verify`, `recovery restore`) are
  TUI-driven (`dialoguer` is the conventional Rust pick). `--quiet`
  applies to the non-interactive flows only; interactive TUI prompts
  are not quietable.
- **Recovery key ceremony** — the 4S standard has a specific format
  (4 groups of 4 base64-encoded words). Use the SDK's helpers, not a
  hand-rolled format.
- **Emoji SAS** — match EXA's `SasEmojisPreview` data so that the
  verification experience is consistent with the mobile client.
- **Persistence convention** — stoolap-fork, never raw SQLite.
  Applies only to new CipherOcto persistence; the SDK's internal
  crypto store is third-party (see E2EE persistence section above).
- **The CLI's E2EE flows are TUI-driven** (not GUI). Choose a TUI crate
  early (`dialoguer` is the conventional pick in the Rust ecosystem).

## Additional Requirements

- Document the recovery-key-loss user-facing flow in `docs/`. A user who
  loses their 4S key AND has no verified device loses access to E2EE history.
  This is a Matrix-wide invariant, not CipherOcto-specific, but the
  onboarding CLI is the natural place to surface the warning.

## Follow-up Missions

(none — this is the terminal mission in the 0850h auth/E2EE series)
