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
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
completed_at: 2026-08-28
completed_by: mmacedoeu
landing_commit: e16306b2
spec_cycle_dry_closed: 2026-08-28
review_dry_closed: 2026-08-28
---

# 0011-identity-commands — Identity subcommands (whoami, identity show/rotate/revoke)

**Status:** Completed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28. Implementation review loop R1-R32 closed 2026-08-28. User owns push/PR/promotion per [[feedback_initiation_user_only]] + [[git-workflow]].
**Landing commit:** `e16306b2 feat(octo-cli): M2 identity commands — whoami + identity show/rotate/revoke` (2 files, +770 lines; 8 test vectors TV-ID1..ID8)
**Substrate:** RFC-0011 §Subcommand Taxonomy (IdentityAction), RFC-0009 Identity substrate
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — provides `OutputEnvelope<T>` + `OctoCliError` + clap root
  **Blocks:** `0011-deprecation-stub-removal` (final integration)

## Status

Completed 2026-08-28 (mmacedoeu) — landed `e16306b2`.

## RFC

RFC-0011 §Subcommand Taxonomy IdentityAction table (rfcs/draft/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [x] `octo whoami` implemented + unit-tested (TV-ID1 pass)
- [x] `octo identity show` implemented + unit-tested (TV-ID2 pass)
- [x] `octo identity rotate` implemented + unit-tested (TV-ID3, TV-ID4, TV-ID6, TV-ID7, TV-ID8 pass)
- [x] `octo identity revoke` implemented + unit-tested (TV-ID5 pass)
- [x] `RedactedHex` newtype implemented + unit-tested (defense in depth on `signature_proof`)
- [x] Confirmation + dry-run gates implemented + unit-tested
- [x] Cross-mission AC: identity commands integrate with core mission's `OutputEnvelope<T>` + clap root
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green
- [x] No new INVALID cites introduced (Guard 2 cite validator 84/84 PASS)

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

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`RedactedHex` wrapper is internal to `octo-cli` and is reused by capability mission for `holder_sig` redaction.

## Scope

Land 4 identity subcommands per RFC-0011 §Subcommand Taxonomy (full scope per original mission YAML — preserved verbatim).

## Test Vectors (per RFC-0011 §Test Vectors — identity group)

8 TV (TV-ID1..TV-ID8) — all passing per `cargo test -p octo-cli --test identity`.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `IdentityAction` dispatch + 4 output structs +
  `RedactedHex` wrapper
- `octo-wallet` (Layer B) — extended with `[ADD]` `WalletStore` +
  `IdentityRecord` + `IdentityRotationEvent` + `Did` types and 4 wrapper fns

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `octo-wallet` extended per `[ADD]` declarations above; no
  breaking changes to existing public API per RFC migration etiquette
- CLI exit codes match RFC-0011 §Exit Code table (no shell script breakage
  beyond exit 1 clap errors which scripts SHOULD have handled)
- `RedactedHex` wrapper is internal to `octo-cli` (no external visibility)

## Cross-references

- RFC-0011 §Subcommand Taxonomy IdentityAction table
- RFC-0009 §Identity Lifecycle State Machine — substrate state machine (Designated /
  Active / Rotating / Revoked)
- [[cipherocto-design-principles]] — Layer B stability contract
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure

## Claimant

@unassigned (mission lifecycle now Completed; archived 2026-08-28 per [[mission-close-up-2026-08-27]] pattern)
