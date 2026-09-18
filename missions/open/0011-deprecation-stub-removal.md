---
name: 0011-deprecation-stub-removal
description: Drop stub commands (init, join, role, agent, status) per RFC-0011 stub deprecation timeline
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "2.0"
  depends_on:
    - RFC-0011
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
    - mission 0011-c-agent-create-subcommand
    - mission 0011-c-agent-list-subcommand
    - mission 0011-c-agent-run-subcommand
    - mission 0011-c-agent-destroy-subcommand
    - mission 0011-c-agent-attach-subcommand
    - mission 0011-d-role-subcommands-phase1
  release_gate:
    require: "v1.1 cycle elapsed on next per RFC-0011 §Changelog v1.1 row (banner-only default; operator opt-in window via OCTO_STALE_STUB_WINDOW=1)"
    released_version: "2.0"
status: Claimed
---

# 0011-deprecation-stub-removal — Drop stub commands (init, join, role, agent, status)

**Status:** Claimed — v2.0 stub removal cut landed in commit 2c28cbb2 on `next` 2026-09-17. Stub commands `init`, `join`, `status` removed from clap surface; `role` and `agent` were already migrated to first-class subcommands in prior RFC-0011-d / RFC-0011-c missions. v1.1 deprecation cycle elapsed on `next` per RFC-0011 §Changelog v1.1 row.
**Substrate:** RFC-0011 §Compatibility (stub deprecation timeline)
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-identity-commands` (or equivalent identity substrate landed)
- Mission `0011-capability-commands`
- Mission `0011-policy-commands`
- Mission `0011-c-agent-create-subcommand` (lands first-class `octo agent create`)
- Mission `0011-c-agent-list-subcommand` (lands first-class `octo agent list`)
- Mission `0011-c-agent-run-subcommand` (lands first-class `octo agent run`)
- Mission `0011-c-agent-destroy-subcommand` (lands first-class `octo agent destroy`)
- Mission `0011-c-agent-attach-subcommand` (lands first-class `octo agent attach`)
- Mission `0011-d-role-subcommands-phase1` (lands first-class `octo role select`)
- v1.1 deprecation cycle elapsed on `next` per RFC migration etiquette
  **Blocks:** none

## Status

Claimed — v2.0 stub removal cut landed in commit 2c28cbb2 on `next` 2026-09-17. Implementation complete; awaiting DRY closure gate per [[feedback_initiation_user_only]] + [[git-workflow]].

## RFC

RFC-0011 §Compatibility (rfcs/accepted/process/0011-octo-cli-substrate.md)
RFC-0011 §Changelog (rfcs/accepted/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Out of Scope (User Decision 2026-09-17)

- `octo network bootstrap` (replacement for `octo join`) — DEFERRED to future amendment
- `octo network status` (replacement for `octo status`) — DEFERRED to future amendment

Operators calling these post-cut hit clap `unrecognized subcommand` (exit 2) until that amendment lands. Accepted risk: v1.1 cycle was observed in the substrate; no operator scripts are expected to depend on `octo join` / `octo status` in production (those surfaces have been banner-only since v1.0).

## StaleStub retention rationale

`OctoCliError::StaleStub` retained (not deleted) with new `replaced_by: &'static str` field. Preserves operator switch tables that map exit code 65 to "stub removed; see X". Substrate-faithful to [[cipherocto-design-principles]] §Extension over enumeration: `#[non_exhaustive]` library surface must not lose variants. Soft sentinel — non-stale code paths (e.g. clap `unrecognized subcommand`) supersede this path per RFC-0011 §Changelog.

## Acceptance Criteria

- [x] Pre-removal gate check verified (v1.1 cycle elapsed on `next`)
- [x] `commands/stub.rs` deleted
- [x] `Commands::Init/Join/Status` variants removed from clap derive struct
- [x] `Commands::Role/Agent` were already first-class subcommands (RFC-0011-d Phase 1 + RFC-0011-c); not stubs at v2.0 cut
- [x] Tests referencing stub commands deleted (`commands/stub.rs` 4 unit tests + `tests/stub.rs` 4 integration tests)
- [x] `§Stub command compatibility` section rewritten in RFC-0011: timeline table preserved; banner-emission prose removed
- [x] `§Changelog` section ADDED with v1.0 / v1.1 / v2.0 rows
- [x] Cross-mission AC: final integration — `octo` exposes only RFC-0011 subcommands + amendments (structural validity via `clap_surface_is_valid` `debug_assert`; explicit surface enumeration deferred to a follow-on per-crate list-extension test)
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy --workspace --all-targets -- -D warnings clean (octo-cli)
- [x] Cargo test -p octo-cli --lib green (322 passed post-cut)
- [x] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011 type                                                          | Sub-step                  | Notes                                                                                         |
| ---------------------------------------------------------------------- | ------------------------- | --------------------------------------------------------------------------------------------- |
| 3 stub commands actually removed (`init`, `join`, `status`)            | Sub-step 2 (code removal) | Layer C/D; pure deletion from clap derive struct; `role` and `agent` were already first-class |
| `StaleStub` exit 65 path (variant retained; `replaced_by` field added) | Sub-step 1 (substrate)    | Layer C/D; soft sentinel per [[cipherocto-design-principles]] §Extension over enumeration     |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Stub Deprecation for Rust snippets + clap wiring patterns.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Risk

- Removing stubs before forward amendment lands breaks operator workflow (`octo init`, `octo role`, `octo agent`, `octo join`, `octo status`). Mitigation: gate on RFC-0011 acceptance + 1 release cycle hard-error cycle (per RFC-0011 §Compatibility — Stub command compatibility).
- Forward amendments (audit/reputation/agent-lifecycle/role-provisioning/vault-operations/mesh-operations/governance) may not land in the same release cycle as stub removal. Mitigation: each stub's replacement surface is independent; operators can keep using deprecated stubs until replacement amendment ships.

## Notes

Forward-references future amendment landing order per RFC-0011 §Implementation Phases Phase 2..8 (audit/reputation/agent-lifecycle/role-provisioning/vault-operations/mesh-operations/governance amendments per Status header amendment chain).

## Scope

Remove the five stub commands that `crates/octo-cli/src/main.rs` exposes today
and RFC-0011 preserved as deprecated wrappers:

1. **`octo init`** — prints init banner. Replace landing is `octo-wallet init`
   (lands in RFC-0011 wallet substrate amendment; out of scope here).
2. **`octo join`** — prints join banner. Replace landing is `octo network
bootstrap` (per Status header amendment chain).
3. **`octo role {builder,provider,storage,bandwidth,orchestrator}`** — prints
   role banner. Replace landing is `octo role select` (per Status header
   amendment chain).
4. **`octo agent {create,run,list}`** — prints agent banner. Replace landing
   is `octo agent lifecycle` (per Status header amendment chain).
5. **`octo status`** — prints status banner. Replace landing is `octo network
status` (per Status header amendment chain).

Per RFC-0011 §Compatibility timeline:

| Version           | Stub behavior                                      |
| ----------------- | -------------------------------------------------- |
| v1.0 (RFC-0011)   | Hidden from `--help`; deprecation warning on use   |
| v1.1 (next minor) | Emit hard error (`StaleStub`, exit code 65) on use |
| v2.0 (next major) | Remove entirely                                    |

This mission lands the v2.0 removal step. Pre-requisite: v1.1 has shipped (1
release cycle with hard-error behavior). Until that gate, this mission is
**ON HOLD**.

### Sub-steps

1. **Pre-removal gate check** — verify the 5 stub commands have emitted
   hard-error (`StaleStub`, exit code 65) for at least 1 release cycle. Cite the released
   version commit + CHANGELOG entry.

2. **Code removal** — `crates/octo-cli/src/commands/stub.rs` DELETE (or strip
   to empty file). Update `crates/octo-cli/src/main.rs` to remove the
   `Commands::Init`, `Commands::Join`, `Commands::Role`, `Commands::Agent`,
   `Commands::Status` variants from the `Commands` enum. Update clap derive
   struct accordingly.

3. **Tests** — DELETE tests that exercised stub commands. Search tests:
   `grep -r "octo_init\|octo_join\|octo_role\|octo_agent\|octo_status"
crates/octo-cli/tests/` → DELETE.

4. **Docs** — DELETE the deprecation banner section from RFC-0011. Add a
   "Stub commands removed in v2.0" changelog entry. Cite removal commit.

5. **Mission state** — Mission `0011-core-output-envelope-redaction`'s stub
   banner code (added in v1.0) gets deleted in this mission.

### Cargo deps

None added or removed. Pure deletion.

## Test Vectors

`tv_dep1_warning_text` + `tv_dep2_exit_65` DELETED with `commands/stub.rs` (4 unit tests) + `tests/stub.rs` (4 integration tests) per RFC-0011 §Changelog. The new `tv_stalestub_v2_replaced_by_display_format` (in `crates/octo-cli/src/error.rs`) is the only post-cut test pinning the `StaleStub` variant; it asserts the Display format carries both `name` and `replaced_by` substrings plus the `octo --help` operator pointer.

Post-cut verification:

- `grep -r "Commands::Init\|Commands::Join\|Commands::Status" crates/octo-cli/src/` → 0 hits (variants removed from the enum entirely per §Scope, not hidden)
- `cargo test -p octo-cli --lib` → 0 references to deleted stub commands
- `octo init` → "error: unrecognized subcommand 'init'" (clap default after v2.0 removal; exit 2)
- `octo join` → "error: unrecognized subcommand 'join'" (exit 2; `octo network bootstrap` DEFERRED to future amendment)
- `octo status` → "error: unrecognized subcommand 'status'" (exit 2; `octo network status` DEFERRED)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — pure deletion; no new types
- NO substrate crate changes

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets -- -D warnings
cargo test -p octo-cli --lib
# Post-cut smoke (clap default exit 2):
octo init 2>&1; echo $?  # expect 2 (unrecognized subcommand)
octo join 2>&1; echo $?  # expect 2 (unrecognized subcommand)
octo status 2>&1; echo $?  # expect 2 (unrecognized subcommand)
```

## Backward compat

- **Breaking change** for any operator script that still calls `octo init`,
  `octo join`, etc. Per RFC migration etiquette, this is acceptable after
  1 release cycle deprecation + 1 release cycle hard-error.
- `octo` now only exposes: `whoami`, `identity {show,rotate,revoke}`,
  `capability {list,mint,attenuate}`, `policy {show,list}` (and the
  follow-on amendments per Status header amendment chain as they land).

## Cross-references

- RFC-0011 §Compatibility — stub deprecation timeline
- RFC-0011 §Status header amendment chain — role provisioning (lands role
  select), agent lifecycle (lands agent lifecycle), mesh operations (lands
  network bootstrap + status)
- [[cipherocto-design-principles]] — Layer C/D per-RFC evolution

## Why 1 release cycle gate

Per CLAUDE.md + RFC migration etiquette:

1. v1.0 lands → deprecation warnings
2. v1.1 ships → hard-error (`StaleStub`, exit 65) — operators see clear signal
3. v2.0 ships → removal

Per RFC migration etiquette, v1.0 → v1.1 → v2.0 is a hard sequencing; no
skip is permitted. The v1.1 hard-error cycle MUST elapse (`StaleStub`, exit 65) before this mission lands. RFC §Compatibility is the authoritative
timeline; this mission implements §Compatibility's v1.1 → v2.0 progression.

## Claimant

@unassigned
