# RFC-0011: `octo` CLI substrate (identity · capability · authorization slice)

## Summary

This PR introduces **RFC-0011** — the formal specification for the `octo`
command-line interface substrate. RFC-0011 closes the gap between the
Layer-B substrate crates (`octo-wallet`, `octo-cap-macaroon`, `octo-policy`)
and the operator-facing CLI that has been a 213-line stub since the
`init/join/role/agent/status` placeholder era.

**Spec-only PR.** No code changes. The 5 implementation missions land as
follow-on PRs after RFC Accept per BLUEPRINT.md.

## Subcommand surface (this RFC)

| Group      | Subcommand                  | Substrate call                                   |
| ---------- | --------------------------- | ------------------------------------------------ |
| Identity   | `octo whoami`               | `octo_wallet::active_identity()`                 |
| Identity   | `octo identity show [DID]`  | `octo_wallet::identity_record()`                 |
| Identity   | `octo identity rotate`      | `octo_wallet::begin_rotation()` (24h hard-coded) |
| Identity   | `octo identity revoke`      | `octo_wallet::revoke()`                          |
| Capability | `octo capability list`      | `octo_cap_macaroon::list_active()`               |
| Capability | `octo capability mint`      | `octo_cap_macaroon::mint()`                      |
| Capability | `octo capability attenuate` | `octo_cap_macaroon::attenuate()`                 |
| Policy     | `octo policy show <name>`   | `octo_policy::show(name, version)`               |
| Policy     | `octo policy list`          | `octo_policy::list(filter)`                      |

## Key design choices

- **`OutputEnvelope<T>` TTY-aware** — pretty-print on TTY, JSON when `--json`
  or non-TTY. RFC 3339 UTC timestamps. `schema_version: u32` for
  forward-compat.
- **`OctoCliError` 20-variant enum** with fixed exit-code table:
  2 (ClapParse / NoActiveIdentity / ConfirmationRequired), 3, 4, 5, 6,
  7, 8, 9, 10, 11 (SigningFailed), 12, 13, 14, 15, 16 (InvalidFilter,
  amendment-reserved), 64 (Internal), 65 (StaleStub), 101 (ConcurrentLock).
- **`OctoCliRedactor` tracing Layer + `redact_string()` helper** — 11
  field names + 9 value-prefix patterns + 128-hex `holder_sig` detector +
  case-insensitive bearer detection with ASCII-whitespace boundary.
- **Two-step confirmation gate** — `--confirm-acknowledge` has clap
  `requires = "confirm"` AND dispatch-time check in `require_confirm()`.
  Mutating commands in human mode without `--confirm` → exit 2 with
  `ConfirmationRequired` error.
- **Stub deprecation timeline** — v1.0 warn / v1.1 hard-error (exit 65,
  `StaleStub`) / v2.0 removal.

## Layer model

`octo-cli` is **Layer C/D** orchestrator. Pulls Layer B substrate via the
10 `[ADD]` API entries; no new Layer A or Layer B types introduced.
Per CLAUDE.md §Architectural Principles.

## Test vectors

54+ TVs across 6 groups: 8 identity, 19 capability (incl. tv_cap_confirm_required
mirroring tv_id3), 5 policy, 5+5+8 output/error/redaction, 5+3 environment,
2 deprecation. Baseline RFC-0011 floor is 30; current count exceeds
the floor by ~80%.

## Cross-cite hygiene

Cite validator PASS:

```
CHECKED=162 VALID=162 PHANTOM=0 INVALID=0 STALE=0
```

No `(Draft)` or `vN.M` suffixes in cross-references per CLAUDE.md
§RFC Reference Conventions. §section refs use name-based only
(`§Subcommand Taxonomy`, `§Output Envelope`, `§Error Handling`, etc).

## Diffstat

```
 rfcs/draft/process/0011-octo-cli-substrate.md                 | ~1200 lines (NEW)
 docs/07-developers/octo-cli-implementation-guide.md            | ~1100 lines (NEW)
 missions/open/0011-core-output-envelope-redaction.md           | ~125 lines  (NEW)
 missions/open/0011-identity-commands.md                        | ~155 lines  (NEW)
 missions/open/0011-capability-commands.md                      | ~175 lines  (NEW)
 missions/open/0011-policy-commands.md                          | ~140 lines  (NEW)
 missions/open/0011-deprecation-stub-removal.md                 | ~131 lines  (NEW)
```

## Review checklist

- [x] Prettier applied to all 7 files
- [x] 5-lens adversarial review loop-until-DRY (R5+R6 = 2 consecutive rounds)
- [x] Layer direction clean (no reverse deps, no new Layer A/B types)
- [x] Mermaid diagrams replace all ASCII art (module tree + subcommand tree)
- [x] Bare RFC numbers in cross-references per CLAUDE.md §RFC Reference Conventions
- [x] §section refs name-based only
- [x] RFC migration etiquette followed (stub removal gated on 1 release cycle)

## Out of scope (deferred to follow-on amendments)

- RFC-0011-a: audit subcommands (RFC-0965 substrate)
- RFC-0011-b: reputation subcommands (RFC-0968 substrate)
- RFC-0011-c: agent lifecycle (octo-runtime)
- RFC-0011-d: role provisioning (RFC-0900+ economic)
- RFC-0011-e: vault ops (RFC-0960 — vault balance projection)
- RFC-0011-f: mesh forwarding (RFC-0871)
- RFC-0011-g: governance (RFC-0855)

## Implementation missions (post-Accept)

5 missions queued at `missions/open/`:

1. `0011-core-output-envelope-redaction` — OutputEnvelope + OctoCliError + OctoCliRedactor + clap root
2. `0011-identity-commands` — whoami + identity {show, rotate, revoke}
3. `0011-capability-commands` — capability {list, mint, attenuate}
4. `0011-policy-commands` — policy {show, list}
5. `0011-deprecation-stub-removal` — drop 5 stub commands (gated on v1.1 hard-error cycle)

Sequencing: mission 1 → 2 → 3 → 4 → 5. Substrate `[ADD]` work parallelisable
alongside CLI substrate work; both gated on RFC-0011 Accept.

## Related RFCs

RFC-0009 (identity lifecycle), RFC-0008 (execution classes), RFC-0003
(deterministic time), RFC-0957 (macaroon substrate), RFC-0958 (ZK
capability subclass), RFC-0960 (vaults), RFC-0964 (caveat envelope),
RFC-0967 (policy object graph), RFC-0850p-a (whatsapp clap pattern),
RFC-0850ab-a (telegram clap pattern), RFC-0917 (mode-gate invariant).

## Acceptance criteria

Per BLUEPRINT.md §RFC Process:

- 7-day community review minimum
- ≥2 maintainer approvals
- No blocking objections on RFC-0011-a..g amendment reservations
- Cite validator PASS (currently 162/162)
