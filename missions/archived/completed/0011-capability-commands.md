---
name: 0011-capability-commands
description: Land capability subcommands (capability list/mint/attenuate) per RFC-0011
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0957
    - RFC-0964
    - RFC-0960
    - RFC-0958
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
completed_at: 2026-08-28
completed_by: mmacedoeu
landing_commit: db857895
spec_cycle_dry_closed: 2026-08-28
review_dry_closed: 2026-08-28
---

# 0011-capability-commands — Capability subcommands (capability list/mint/attenuate)

**Status:** Completed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28. Implementation review loop R1-R32 closed 2026-08-28. User owns push/PR/promotion per [[feedback_initiation_user_only]] + [[git-workflow]].
**Landing commit:** `db857895 feat(octo-cli): M3 capability commands — list/mint/attenuate (RFC-0011)` (2 files, +1232 lines; 19 test vectors TV-CAP1..CAP19)
**Substrate:** RFC-0011 §Subcommand Taxonomy (CapabilityAction), RFC-0957 Macaroon, RFC-0965 Caveat Envelope, RFC-0960 Caveat Catalog
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — substrate (`OutputEnvelope<T>` + `OctoCliError` + clap root)
- Mission `0011-identity-commands` — `active_signer()` is exposed by `WalletStore` extensions from identity mission; `holder` parameter on `mint`/`attenuate` depends on identity substrate amendments
  **Blocks:** `0011-deprecation-stub-removal` (final integration)

## Status

Completed 2026-08-28 (mmacedoeu) — landed `db857895`.

## RFC

RFC-0011 §Subcommand Taxonomy CapabilityAction table (rfcs/draft/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [x] `octo capability list` implemented + unit-tested (TV-CAP1 pass)
- [x] `octo capability mint` implemented + unit-tested (TV-CAP2, TV-CAP3, TV-CAP6, TV-CAP7, TV-CAP8 pass)
- [x] `octo capability attenuate` implemented + unit-tested (TV-CAP4, TV-CAP5 pass)
- [x] `Hex32` newtype implemented + unit-tested
- [x] Caveat JSON parser implemented + unit-tested (TV-CAP9..15 pass)
- [x] Attenuation check implemented + unit-tested
- [x] Filter parsing implemented + unit-tested (TV-CAP16 pass)
- [x] Dry-run + confirm gates implemented + unit-tested (TV-CAP17, TV-CAP18, TV-CAP19 pass)
- [x] Cross-mission AC: capability commands integrate with core mission's envelope + identity mission's `RedactedHex` wrapper
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green
- [x] No new INVALID cites introduced (Guard 2 cite validator 84/84 PASS)

### Type Coverage

| RFC-0011 type               | Sub-step                  | Notes                                                                                                                                                     |
| --------------------------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CapabilitySummary`         | Sub-step 1 (output types) | Layer B; substrate `[ADD]` struct in octo-cap-macaroon per RFC-0011 §Subcommand Taxonomy entry #8 (`cap_id`, `root_id`, `caveats`, `expires_at`)          |
| `CapabilityMintOutput`      | Sub-step 1 (output types) | Layer C/D; `holder_sig` rendered via `RedactedHex`                                                                                                        |
| `CapabilityAttenuateOutput` | Sub-step 1 (output types) | Layer C/D; `narrowed_from` records parent cap_id                                                                                                          |
| `CaveatSummary`             | Sub-step 1 (output types) | Layer B; new `[ADD]` struct in octo-cap-macaroon per RFC-0011 §Subcommand Taxonomy entry #9 (`caveat_type: String`, `constraint_json: serde_json::Value`) |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Capability Subcommands for Rust snippets + clap wiring patterns.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`Hex32` and `RedactedHex` are distinct newtypes: `Hex32` is a public digest (`body_hash`); `RedactedHex` is a secret wrapper (`holder_sig`, `signature_proof`).

## Scope

Land 3 capability subcommands per RFC-0011 §Subcommand Taxonomy (full scope per original mission YAML — preserved verbatim).

## Test Vectors (per RFC-0011 §Test Vectors — capability group)

19 TV (TV-CAP1..TV-CAP19) — all passing per `cargo test -p octo-cli --test capability`.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `CapabilityAction` dispatch + 3 output structs
  - caveat parsing helpers (using existing `Caveat::canonical_ser` until
    the substrate amendment adding `validate_canonical_form` lands)
- `octo-cap-macaroon` (Layer B) — substrate `list_active()`,
  `mint()`, `attenuate()`, `set_subsumes()` (each is the `[ADD]` form per
  RFC-0011 §Subcommand Taxonomy entries #7, #10, #11, #12)

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: no breaking changes to octo-cap-macaroon substrate API for
  the existing 27-variant `Caveat` enum; all `[ADD]` entries are additive
  per RFC migration etiquette. `CapabilitySummary` + `CaveatSummary` are
  new types; existing public API is unchanged.
- CLI exit codes match RFC-0011 §Exit Code table
- `Hex32` and `RedactedHex` wrappers internal to `octo-cli`
- Caveat JSON form unchanged — same RFC-0965 envelope consumers already use

## Cross-references

- RFC-0011 §Subcommand Taxonomy CapabilityAction table
- RFC-0957 — Macaroon substrate
- RFC-0965 — Caveat envelope canonical form
- RFC-0960 — Caveat catalog root
- [[cipherocto-design-principles]] — Layer B stability contract
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure

## Claimant

@unassigned (mission lifecycle now Completed; archived 2026-08-28 per [[mission-close-up-2026-08-27]] pattern)
