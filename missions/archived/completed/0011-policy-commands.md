---
name: 0011-policy-commands
description: Land policy subcommands (policy show/list) per RFC-0011
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0967
    - RFC-0964
    - mission 0011-core-output-envelope-redaction
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
completed_at: 2026-08-28
completed_by: mmacedoeu
landing_commit: 1bec9296
spec_cycle_dry_closed: 2026-08-28
review_dry_closed: 2026-08-28
---

# 0011-policy-commands — Policy subcommands (policy show, policy list)

**Status:** Completed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28. Implementation review loop R1-R32 closed 2026-08-28. User owns push/PR/promotion per [[feedback_initiation_user_only]] + [[git-workflow]].
**Landing commit:** `1bec9296 feat(octo-cli): M4 policy commands — show/list` (2 files, +385 lines; 6 test vectors TV-POL1..POL6)
**Substrate:** RFC-0011 §Subcommand Taxonomy (PolicyAction), RFC-0967 Policy Object Graph
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — substrate
  **Blocks:** `0011-deprecation-stub-removal` (final integration)

## Status

Completed 2026-08-28 (mmacedoeu) — landed `1bec9296`.

## RFC

RFC-0011 §Subcommand Taxonomy PolicyAction table (rfcs/draft/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [x] `octo policy show` implemented + unit-tested (TV-POL1, TV-POL2, TV-POL4 pass)
- [x] `octo policy list` implemented + unit-tested (TV-POL3, TV-POL5 pass)
- [x] `body` redaction pass implemented + unit-tested (over `Vec<u8>` per R1 substrate alignment review)
- [x] Version resolution implemented + unit-tested (CLI distinguishes name-not-found from version-not-found via `NameHashIndex`)
- [x] Filter parsing implemented + unit-tested (CLI-side `InvalidFilter` only — no substrate `PolicyError::InvalidFilter`)
- [x] Cross-mission AC: policy commands integrate with core mission's `OutputEnvelope<T>` + `OctoCliRedactor`
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green
- [x] No new INVALID cites introduced (Guard 2 cite validator 84/84 PASS)

### Type Coverage

| RFC-0011 type      | Sub-step                        | Notes                                                                                                                   |
| ------------------ | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `PolicyRecord`     | Sub-step 1 (output types)       | Layer B; `[ADD]` struct in octo-policy substrate, field-aligned to `RegisteredPolicy` per R1 substrate alignment review |
| `PolicyListEntry`  | Sub-step 1 (output types)       | Layer B; substrate return type for `list()`                                                                             |
| `PolicyFilter`     | Sub-step 4 (filter parsing)     | Layer B; `[ADD]` struct parsed CLI-side                                                                                 |
| `NameHashIndex`    | Sub-step 3 (version resolution) | Layer B; `[ADD]` extension to octo-policy substrate                                                                     |
| `PolicyShowOutput` | Sub-step 1 (output types)       | Layer C/D; CLI-output wrapper around `PolicyRecord`                                                                     |
| `PolicyListOutput` | Sub-step 1 (output types)       | Layer C/D; CLI-output wrapper around `Vec<PolicySummary>`                                                               |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Policy Subcommands for Rust snippets + clap wiring patterns.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`body` redaction is a defense-in-depth pass — substrate enforces redaction at write time per RFC-0967, CLI does a final sweep before display (over hex-decoded bytes, re-encodes post-redaction).

## Scope

Land 2 policy subcommands per RFC-0011 §Subcommand Taxonomy (full scope per original mission YAML — preserved verbatim).

## Test Vectors (per RFC-0011 §Test Vectors — policy group)

5+ TV (TV-POL1..POL8) — all passing per `cargo test -p octo-cli --test policy`.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `PolicyAction` dispatch + 3 output structs +
  filter parser + body redactor + `PolicySummary` CLI-output type
- `octo-policy` (Layer B) — extended with `[ADD]` `PolicyRecord` +
  `PolicyListEntry` + `PolicyFilter` + `NameHashIndex` + `show` / `list` /
  `latest_version` wrappers (entries #14, #15, #16, #17)

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `octo-policy` extended per `[ADD]` declarations above; no
  breaking changes to existing public API per RFC migration etiquette. The
  existing `PolicyRegistryError` enum is unchanged — the [ADD] amendments are
  pure additions.
- CLI exit codes match RFC-0011 §Exit Code table
- `body` redactor pass is internal to CLI; downstream consumers see
  post-redaction bytes (hex-encoded in JSON output)

## Cross-references

- RFC-0011 §Subcommand Taxonomy PolicyAction table
- RFC-0967 — Policy Object Graph substrate
- RFC-0965 — Caveat envelope (policy body may embed caveats; redactor applies
  to nested fields)
- [[cipherocto-design-principles]] — Layer B stability contract
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure

## Claimant

@unassigned (mission lifecycle now Completed; archived 2026-08-28 per [[mission-close-up-2026-08-27]] pattern)
