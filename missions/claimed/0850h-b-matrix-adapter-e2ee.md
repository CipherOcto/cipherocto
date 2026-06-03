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
  headless CLI. Pin to `matrix-sdk = "=0.17.0"` (exact pin, same as
  mission 0850h-a; the SDK Risk note in 0850h-a flags that an
  automatic patch bump could break the QR module API).
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
version = "=0.17.0"
default-features = false
features = [
    "e2e-encryption",
    "sqlite",
    "qrcode",
    # "rustls-tls" is NOT a valid 0.17.0 feature; TLS uses the
    #   embedded reqwest's default backend (native-tls on Linux).
    # "sqlite-cryptostore" does not exist on 0.17.0; the SQLite
    #   crypto store is enabled implicitly when both
    #   `e2e-encryption` and `sqlite` are set.
    # "indexeddb-cryptostore" does not exist on 0.17.0; the
    #   web-only state/event-cache store is the `indexeddb`
    #   feature (not used here — headless CLI).
]
```

### Reference architecture

EXA's `features/verifysession` module is the canonical reference for the
verification UX (Presenter/View/State/Event per screen, Appyx navigation
across `intro → verifying → done` or `intro → verifying → error`).
The CLI adaptation replaces the Composable View with a TUI prompt
(`dialoguer` or `inquire` crate).

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
