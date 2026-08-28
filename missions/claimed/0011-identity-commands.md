---
name: 0011-identity-commands
description: Land identity subcommands (whoami, identity show/rotate/revoke) per RFC-0011
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0009
    - mission 0011-core-output-envelope-redaction
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
spec_cycle_dry_closed: 2026-08-28
---

# 0011-identity-commands — Identity subcommands (whoami, identity show/rotate/revoke)

**Status:** Claimed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28 (5-round loop-until-DRY closure). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]].
**Substrate:** RFC-0011 §Subcommand Taxonomy (IdentityAction), RFC-0009 Identity substrate
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — provides `OutputEnvelope<T>` + `OctoCliError` + clap root
  **Blocks:** `0011-deprecation-stub-removal` (final integration)

## Status

Claimed 2026-08-28 (mmacedoeu) — spec cycle DRY-closed 2026-08-28

## RFC

RFC-0011 §Subcommand Taxonomy (rfcs/draft/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [ ] `octo whoami` implemented + unit-tested (TV-ID1 pass)
- [ ] `octo identity show` implemented + unit-tested (TV-ID2 pass)
- [ ] `octo identity rotate` implemented + unit-tested (TV-ID3, TV-ID4, TV-ID6, TV-ID7, TV-ID8 pass)
- [ ] `octo identity revoke` implemented + unit-tested (TV-ID5 pass)
- [ ] `RedactedHex` newtype implemented + unit-tested (defense in depth on `signature_proof`)
- [ ] Confirmation + dry-run gates implemented + unit-tested
- [ ] Cross-mission AC: identity commands integrate with core mission's `OutputEnvelope<T>` + clap root
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test --workspace --lib green
- [ ] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011 type          | Sub-step                   | Notes                                                 |
| ---------------------- | -------------------------- | ----------------------------------------------------- |
| `WhoamiOutput`         | Sub-step 1 (output types)  | Layer C/D; wraps `IdentityKey` + `IdentityRecord`     |
| `IdentityShowOutput`   | Sub-step 1 (output types)  | Layer C/D; includes `IdentityRotationEvent` history   |
| `IdentityRotateOutput` | Sub-step 1 (output types)  | Layer C/D; `signature_proof` field uses `RedactedHex` |
| `IdentityRevokeOutput` | Sub-step 1 (output types)  | Layer C/D; `terminal: true` flag                      |
| `Did` newtype          | Sub-step 2 (identity show) | Layer B; canonical DID type per RFC-0010 alignment    |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Identity Subcommands for Rust snippets + clap wiring patterns.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Notes

`RedactedHex` wrapper is internal to `octo-cli` and is reused by capability mission for `holder_sig` redaction.

## Scope

Land 4 identity subcommands per RFC-0011 §Subcommand Taxonomy:

1. **`octo whoami`** — `commands/identity.rs::whoami`. Reads active identity
   via `[ADD] octo_wallet::active_identity(&WalletStore) -> Result<IdentityKey, WalletError>`
   (takes an explicit `&WalletStore` handle; NO ambient global state). To
   render the full `WhoamiOutput`, the CLI chains `active_identity(&store)?`
   to fetch the `IdentityKey` (DID + pubkey), then `identity_record(&store, did)?`
   (RFC §Subcommand Taxonomy [ADD] entry #3) to surface
   `lifecycle_state`, `hsm_slot`, `registered_at`, and `RotationHistory`
   fields. Substrate contract is reconciled to the canonical RFC entry #1
   (`Result<IdentityKey, WalletError>`); `IdentityRecord` is obtained
   separately via `identity_record` for commands that need full record
   fields (e.g., `octo identity show`).
   Output:
   `WhoamiOutput { did, pubkey_hex, lifecycle_state, hsm_slot,
registered_at }`. Exit 0 on success; exit 2 on `NoActiveIdentity`. No
   side effects. Read-only. Note: `WalletError::NoActiveIdentity` is a new
   `[ADD]` variant per RFC-0011 §Subcommand Taxonomy; substrate today
   exposes `WalletError::NotActive { current_state: LifecycleState }`
   (different name/shape) which the CLI remaps to exit 2 via a
   `require_confirm`-style adapter. `lifecycle_state` is rendered via the
   substrate's `impl fmt::Debug for LifecycleState` (already present at
   in `octo-wallet::lifecycle` — substrate-truth string set
   (`Designated`/`Active`/`Rotating`/`Revoked`). The previously-claimed
   `stable_label()` method on `LifecycleState` is NOT in substrate and is
   dropped from the [ADD] surface (Layer B must not carry presentation per
   CLAUDE.md §Architectural Principles).

2. **`octo identity show [DID]`** — `commands/identity.rs::show`. Reads via
   `[ADD] octo_wallet::identity_record(&WalletStore, did: &Did) -> Result<IdentityRecord, WalletError>`.
   Output:
   `IdentityShowOutput { did, pubkey_hex, lifecycle_state,
rotation_history: Vec<IdentityRotationEvent>, hsm_slot }` —
   `IdentityRotationEvent` is a new `[ADD]` struct in octo-wallet Layer B,
   distinct from the existing `RotationEvent` in
   `crates/octo-wallet/src/vault_rotation.rs` (different type, different
   purpose — vault rotation vs identity rotation). Per RFC-0011
   §Subcommand Taxonomy entry #4, the substrate struct shape is
   `{ rotation_id: [u8;32], started_at_unix: i64, grace_expires_at_unix: i64, successor_did: Did, signature_proof: [u8;64] }`.
   The CLI's `governance_snapshot_ref` field is REMOVED from the v1.0
   output schema (it was a R0 over-claim — substrate today has no
   governance-snapshot concept on identity records; deferred per Status
   header amendment chain — governance). The `Did` type itself is
   also a `[ADD]` declaration (canonical DID type per RFC-0010 alignment;
   see RFC-0011 §Subcommand Taxonomy entry #2: `pub struct Did(pub String)`
   - `IdentityKey::did(&self) -> Did`).
     Exit 0 on success; exit 4 on `IdentityNotFound`. No side effects.

3. **`octo identity rotate`** — `commands/identity.rs::rotate`. Flags:
   `--confirm`, `--confirm-acknowledge` (atomic pastejacking gate per RFC-0011
   §Security 1a), `--dry-run`. No `--grace-hours` flag — substrate
   `IdentityKey::begin_rotation` hard-codes 24h grace internally as
   `ROTATION_GRACE_PERIOD_SECS`. Substrate call:
   `[ADD] octo_wallet::begin_rotation(&mut IdentityKey, successor: IdentityKey, now_unix_secs: u64) -> Result<[u8;64], WalletError>`
   (thin wrapper around existing `IdentityKey::begin_rotation(&mut self, ...)`
   instance method per substrate signature).
   Output:
   `IdentityRotateOutput { new_did, old_did, grace_expires_at,
signature_proof: RedactedHex }`. Exit 0 / 3 / 4 / 5 / 11 / 64 (11 = `SigningFailed` per `OctoCliError::SigningFailed` variant, matches capability mission + impl-guide). `signature_proof`
   MUST be rendered as `RedactedHex` per RFC-0011 §Redaction Layer (never raw). `--confirm`
   required in human mode; `--allow-write` required in ci mode.
   **Race note:** if `revoke` is invoked DURING the rotation grace window, BOTH
   old and new keys are invalidated immediately per RFC-0009 §Identity Lifecycle;
   the CLI does not need to special-case this — substrate enforces.

4. **`octo identity revoke --reason <str>`** —
   `commands/identity.rs::revoke`. Substrate call:
   `[ADD] octo_wallet::revoke(&mut IdentityKey, now_unix_secs: u64) -> Result<(), WalletError>`
   (thin wrapper around `IdentityKey::revoke(&mut self, ...)` instance method
   per substrate signature). Flags: `--confirm`,
   `--confirm-acknowledge`, `--reason <str>` (REQUIRED).
   Output: `IdentityRevokeOutput { did, revoked_at, terminal: true }`. Exit 0
   / 4 / 6 / 64. `--reason` REQUIRED (clap enforces; absent → exit 2 with clap
   usage error per POSIX convention). **AlreadyRevoked mapping note (per R1 substrate alignment review):**
   substrate `IdentityKey::revoke` is idempotent from `Revoked` state — it
   does NOT raise a `WalletError::AlreadyRevoked` variant. CLI exit 6
   (`AlreadyRevoked`) is achieved via a CLI-level pre-check: read the
   current lifecycle via `WalletStore::identity_record(...)` BEFORE calling
   `revoke()`; if `record.lifecycle == LifecycleState::Revoked`, return
   `OctoCliError::AlreadyRevoked` (exit 6) without invoking substrate.

### Sub-steps

1. **Output types** — `crates/octo-cli/src/commands/identity.rs`. Per RFC-0011
   §Subcommand Taxonomy, four `#[derive(Serialize, schemars::JsonSchema)]` structs:
   `WhoamiOutput`, `IdentityShowOutput`, `IdentityRotateOutput`,
   `IdentityRevokeOutput`. Each derives `Debug + Clone`. The `signature_proof`
   field on `IdentityRotateOutput` uses a `RedactedHex` newtype wrapper that
   renders only `[REDACTED:sig]` regardless of inner value (defense in depth —
   even if substrate returns raw, the wrapper prevents leak).

2. **Dispatch** — `dispatch(action, &Octo) -> Result<(), OctoCliError>`. Matches
   on `IdentityAction` enum. Each arm constructs its `OutputEnvelope` + calls
   `env.render(cli.output.json, cli.output.no_color)`.

3. **`RedactedHex` newtype** — `crates/octo-cli/src/redact.rs` (add to existing
   module). `pub struct RedactedHex(pub Vec<u8>)` with `Serialize` impl that
   emits `"[REDACTED:sig]"` regardless of contents. `Debug` impl same.
   `Display` impl same.

4. **Confirmation gate** — extract to `commands/identity.rs::require_confirm`.
   If `cli.mode.mode == OperatorMode::Human && !cli.mode.confirm &&
!cli.mode.dry_run`, return `OctoCliError::ConfirmationRequired { command }`
   (new variant, exit 2 — POSIX usage-error convention since it is a missing
   flag). CI mode requires `--allow-write`; Auditor mode is rejected outright
   (read-only). Reused by capability / policy mutating commands.

5. **Dry-run gate** — extract to `commands/identity.rs::dry_run_or`. If
   `cli.mode.dry_run`, return `OutputEnvelope::preview_only(output, 0)` with
   `preview_only: true` flag set; otherwise invoke substrate. `dry_run_or` lives in
   `commands/mod.rs` (reused by capability / policy).

### Cargo deps added (cumulative with mission 0011-core)

None new. All deps added by mission `0011-core-output-envelope-redaction`.

## Test Vectors (per RFC-0011 §Test Vectors — identity group)

8 new TV (TV-ID1..TV-ID8):

- `tv_id1_whoami_success` — `octo whoami` → exit 0; stdout contains
  `"did":`, `"pubkey_hex":`, `"lifecycle_state":`; stderr empty
- `tv_id2_identity_show_not_found` — `octo identity show did:octo:nonexistent`
  → exit 4; stderr `identity not found`
- `tv_id3_identity_rotate_confirm_required` — `octo identity rotate` →
  exit 2 (POSIX usage-error convention); stderr `ConfirmationRequired: --confirm required for mutating command identity rotate in human mode`
- `tv_id4_identity_rotate_grace_hours_flag_absent` — `octo identity rotate
--confirm --confirm-acknowledge` (no `--grace-hours` flag — substrate
  `ROTATION_GRACE_PERIOD_SECS` is internal and not operator-configurable)
  → exit 0 (the flag is NOT exposed by clap; see RFC-0011 §Binary Surface
  where `--grace-hours` was removed in the substrate amendment that
  hard-codes 24h grace)
- `tv_id5_identity_revoke_already_revoked` — wallet in Revoked state; `octo
identity revoke --confirm --confirm-acknowledge --reason test` → exit 6;
  stderr `already revoked`
- `tv_id6_already_rotating` — wallet in `Rotating` state; `octo identity rotate
--confirm --confirm-acknowledge` → exit 3; stderr `already rotating` — NEW
- `tv_id7_hsm_missing` — HSM slot unreachable; `octo identity rotate --confirm
--confirm-acknowledge` → exit 5 — NEW
- `tv_id8_rotate_dry_run` — `octo identity rotate --confirm
--confirm-acknowledge --dry-run` → exit 0; stdout contains `"preview_only":true`,
  `"new_did":`; no state mutation verified by post-call wallet-state assertion
  — NEW

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `IdentityAction` dispatch + 4 output structs +
  `RedactedHex` wrapper
- `octo-wallet` (Layer B) — extended with `[ADD]` `WalletStore` +
  `IdentityRecord` + `IdentityRotationEvent` + `Did` types and 4 wrapper fns
  (`WalletStore::open()`, `WalletStore::active_identity(&self)`,
  `WalletStore::identity_record(&self, &Did)`, `octo_wallet::begin_rotation(&mut IdentityKey, ...)`,
  `octo_wallet::revoke(&mut IdentityKey, ...)`)
  per RFC-0011 §Subcommand Taxonomy. The previously-claimed
  `LifecycleState::stable_label()` + `impl Display for LifecycleState` are
  NOT substrate additions — substrate already has `impl fmt::Debug for
LifecycleState` returning the stable strings (in `octo-wallet::lifecycle`,
  and the CLI uses that as substrate-truth.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-cli --lib --all-features
cargo test -p octo-cli --test identity --all-features
```

## Backward compat

- Additive only: `octo-wallet` extended per `[ADD]` declarations above; no
  breaking changes to existing public API per RFC migration etiquette
- CLI exit codes match RFC-0011 §Exit Code table (no shell script breakage
  beyond exit 1 clap errors which scripts SHOULD have handled)
- `RedactedHex` wrapper is internal to `octo-cli` (no external visibility)

## Cross-references

- RFC-0011 §Subcommand Taxonomy IdentityAction table
- RFC-0009 §Identity Lifecycle — substrate state machine (Designated /
  Active / Rotating / Revoked)
- (Future work, no RFC filed yet): WhatsApp/Telegram Auth Onboarding redaction
  pattern (applied to `signature_proof`) — see whatsapp/telegram CLI substrate
  RFC when filed
- [[cipherocto-design-principles]] — Layer B stability contract

## Claimant

@unassigned
