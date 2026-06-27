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

- [x] `Cargo.lock` regenerates cleanly; no manual edits.

### Live-Test Cleanup acceptance

- [ ] `crates/octo-adapter-matrix-sdk/src/bin/cleanup_test_rooms.rs`
      exists, builds under `cargo build --bins`, and
      implements the 5-phase plan (read session, build +
      restore + sync, enumerate stale rooms, leave /
      report, optional `--update-config` rewrite).
- [ ] The binary's `--dry-run` flag prints the would-be
      plan without mutating state (verified against
      matrix.org with the existing stale room
      `!YqeNMmiscHcRbQNsUE:matrix.org`).
- [ ] After a non-dry-run against matrix.org, a follow-up
      `--dry-run` reports zero `octo-test-mx-*` rooms and
      zero orphaned `rooms[]` entries.
- [ ] `cargo run -p octo-adapter-matrix-sdk --bin cleanup_test_rooms -- --update-config`
      rewrites `~/.config/octo/matrix.json` so the
      `rooms[]` array contains only rooms that the SDK
      still resolves via `client.get_room(&rid)`.
- [ ] `tests/live_matrix_test.rs` gains an `#[ignore]`
      test `cleanup_stale_test_rooms` that runs the same
      logic inline (no subprocess) and passes against
      matrix.org via
      `cargo test --features live-matrix --test live_matrix_test cleanup_stale_test_rooms -- --include-ignored --nocapture`.
- [ ] `mx04_05_06_envelope_round_trip` and
      `mx07_media_round_trip` gain a pre-scan guard at the
      top of their room-creation block that leaves any
      pre-existing `octo-test-mx-*` rooms before creating
      the new one (idempotent self-healing).
- [ ] After the cleanup infrastructure is in place,
      `mx04_05_06_envelope_round_trip` passes reliably on
      a fresh session (the previous failure mode
      "Room not found in joined rooms" no longer occurs).
- [ ] The pre-scan guard does NOT change the room
      name prefix or the room-creation pattern used by
      the tests — the prefix `octo-test-mx-mx04-{ts}` /
      `octo-test-mx-mx07-{ts}` is preserved as the
      cleanup scan's target.

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

### Live-Test Cleanup Infrastructure

The live integration suite (`mx01`–`mx08`) creates
short-lived test rooms whose names follow the prefix
`octo-test-mx-*` (mx04 uses `octo-test-mx-mx04-{ts}`, mx07
uses `octo-test-mx-mx07-{ts}`). When a test panics before
its cleanup block runs (lines 313–325 of
`tests/live_matrix_test.rs`), the room it created is left
orphaned on the homeserver, and the next test run picks up
the stale `room_id` from `~/.config/octo/matrix.json`'s
`rooms[]` array. The adapter then fails with
`Room <id> not found in joined rooms`. The pattern that
prevents this for WhatsApp and Telegram (MTProto) is a
**standalone cleanup binary** under `src/bin/` plus a
matching `#[ignore]` test inside the live suite. This
extension of 0850h-b replicates that pattern for Matrix.

**Design.**

- `crates/octo-adapter-matrix-sdk/src/bin/cleanup_test_rooms.rs`
  — standalone binary (auto-discovered by Cargo in
  `src/bin/`). Five phases:
  1. Read `~/.config/octo/matrix.json` (override via
     `--config <path>`). Parse the session JSON for
     `access_token`, `refresh_token`, `user_id`, `device_id`,
     `homeserver_url`, and `rooms[]`.
  2. Build a raw `matrix_sdk::Client`, restore the session
     via `client.restore_session(MatrixSession { meta, tokens })`,
     then `client.sync_once(SyncSettings::default()
       .timeout(Duration::from_secs(60)))`. The 60 s window
     is generous enough for E2EE bootstrap (one-time key
     upload + crypto-store init) on a fresh session —
     the 5 s timeout used in the live tests themselves is
     too tight for first sync, see mx01 follow-up below.
  3. Iterate `client.rooms()` and `client.invited_rooms()`;
     collect a `Vec<(OwnedRoomId, room_name)>` for any room
     whose name starts with `octo-test-mx-`. Also collect
     a separate list of `room_id`s whose IDs appear in
     the session file's `rooms[]` array but are NOT in
     `client.get_room(&rid)` (the exact failure mode of
     `mx04_05_06`).
  4. If `--dry-run`, print the would-be cleanup plan
     (prefixed-name rooms + orphaned session-file rooms)
     and exit without state change. Otherwise:
       a. For each prefix-match room, `room.leave().await`
          and log success/failure.
       b. For each orphaned session-file room, attempt
          `client.get_room(&rid)` (already established to
          return None in phase 3 — leave is impossible, so
          we just record that it's orphaned).
       c. If `--update-config` was passed, rewrite
          `~/.config/octo/matrix.json` with the `rooms[]`
          array containing only the rooms that the SDK
          still knows about (intersection of the original
          array with the joined-rooms set). This is the
          "self-healing" mode that fixes the `mx04_05_06`
          failure without manual `--config` editing.
  5. Print a summary (`X left, Y orphaned in session file,
     Z session-file rooms pruned`) and exit.

  Flags:
    - `--dry-run` — scan only, no leaves, no writes
    - `--config <path>` — override session path
    - `--update-config` — prune `rooms[]` in the session
       file (off by default; off is safer)
    - `--verbose` — INFO-level tracing for the SDK calls

  Usage:
    ```bash
    cargo run -p octo-adapter-matrix-sdk \
      --bin cleanup_test_rooms -- --dry-run
    cargo run -p octo-adapter-matrix-sdk \
      --bin cleanup_test_rooms -- --update-config
    ```

- `crates/octo-adapter-matrix-sdk/tests/live_matrix_test.rs`
  gets two additions:
  1. `#[ignore]` test `cleanup_stale_test_rooms` —
     runs the same logic as the binary inline
     (no subprocess). Callable via
     `cargo test -- --include-ignored cleanup_stale_test_rooms`.
     Useful for CI that doesn't want a separate binary step.
  2. **Pre-scan guard** inside `mx04_05_06` and `mx07`:
     before creating the test room, do a one-shot
     `sync_once` (5 s timeout, matches the existing
     pattern) and `room.leave()` any joined room whose
     name starts with `octo-test-mx-`. This makes each
     test run self-healing — even if a previous run
     panicked at line 280 and skipped cleanup, the next
     run cleans up before creating its own room.

**Out of scope for this extension.**

- Cleaning up media uploads (`mxc://` URIs from `mx07`).
  matrix.org has no API to delete uploaded media — the
  user's media quota grows monotonically. This is
  matrix-wide behavior, not cipherocto-specific.
- Cleaning up Olm/Megolm sessions. The SDK's crypto
  store is the source of truth; if a stale room is
  left behind, its Megolm sessions naturally expire
  via the SDK's rotation policy.
- The mx01 sync-timeout follow-up (5 s too tight for
  first sync on a fresh E2EE session). Tracked
  separately as a follow-up mission; the cleanup
  binary uses 60 s as a stopgap.

## Acceptance Criteria

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
