---
name: 0011-deprecation-stub-removal
description: Drop stub commands (init, join, status) per RFC-0011 stub deprecation timeline
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
    require: "v1.1 cycle elapsed on next per RFC-0011 §Changelog (banner-only default; operator opt-in window via OCTO_STALE_STUB_WINDOW=1)"
    released_version: "2.0"
status: Claimed
---

# 0011-deprecation-stub-removal — Drop stub commands (init, join, status)

**Status:** Claimed — see §Status below.
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
- v1.1 deprecation cycle elapsed on `next` per RFC-0011 §Changelog
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

See RFC-0011 §Compatibility for the post-cut operator impact (operators calling these hit clap `unrecognized subcommand`, exit 2). Accepted risk: v1.1 cycle was observed in the substrate; no operator scripts are expected to depend on `octo join` / `octo status` in production (those surfaces have been banner-only since v1.0).

## Acceptance Criteria

- [x] Pre-removal gate check verified (v1.1 cycle elapsed on `next`)
- [x] `commands/stub.rs` deleted
- [x] `Commands::Init/Join/Status` variants removed from clap derive struct
- [x] `Commands::Role/Agent` were already first-class subcommands in the post-cut substrate (RFC-0011-d Phase 1 + RFC-0011-c); no stub surface to remove
- [x] Tests referencing stub commands deleted (`commands/stub.rs` 4 unit tests + `tests/stub.rs` 4 integration tests)
- [x] `§Stub command compatibility` section rewritten in RFC-0011: timeline table preserved; banner-emission prose removed
- [x] `§Changelog` section ADDED with row entries covering the banner-only, hard-error, and removal phases
- [x] Cross-mission AC: final integration — `octo` exposes only RFC-0011 subcommands + amendments (structural validity via `clap_surface_is_valid` `debug_assert`; explicit surface enumeration deferred to a follow-on per-crate list-extension test)
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy --workspace --all-targets -- -D warnings clean (octo-cli)
- [x] Cargo test -p octo-cli --lib green (323 passed post-cut)
- [x] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011 type                                                          | Sub-step                  | Notes                                                                                                                       |
| ---------------------------------------------------------------------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| 3 stub commands actually removed (`init`, `join`, `status`)            | Sub-step 2 (code removal) | Layer C; pure deletion from clap derive struct in `crates/octo-cli/src/lib.rs`; `role` and `agent` were already first-class |
| `StaleStub` exit 65 path (variant retained; `replaced_by` field added) | Sub-step 1 (substrate)    | Layer C; soft sentinel per [[cipherocto-design-principles]] §Extension over enumeration                                     |

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

Remove the five stub commands that `crates/octo-cli/src/lib.rs` exposes today
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

| Version           | Stub behavior                                      | Status             |
| ----------------- | -------------------------------------------------- | ------------------ |
| v1.0              | Hidden from `--help`; deprecation warning on use   | shipped on `next`  |
| v1.1 (next minor) | Emit hard error (`StaleStub`, exit code 65) on use | shipped on `next`  |
| v2.0 (next major) | Remove entirely                                    | landed in 2c28cbb2 |

This mission has landed the v2.0 removal step. Pre-requisite (v1.1 hard-error
cycle elapsed on `next`) was verified before cut per the timeline above.

### Sub-steps (post-cut record)

1. **Gate verified** — v1.1 hard-error cycle (`StaleStub`, exit 65) elapsed on
   `next` per the §Changelog timeline before the cut landed.

2. **Code removal** — `crates/octo-cli/src/commands/stub.rs` and
   `crates/octo-cli/tests/stub.rs` DELETED. Update
   `crates/octo-cli/src/lib.rs` to remove the `Commands::Init`,
   `Commands::Join`, `Commands::Status` variants from the `Commands` enum.
   `Commands::Role` and `Commands::Agent` were already first-class
   subcommands before this cut (RFC-0011-d Phase 1 + RFC-0011-c) and were
   NOT removed.

3. **Tests** — DELETED tests that exercised stub commands. Post-cut
   `cargo test -p octo-cli --lib` runs clean with no references to the
   deleted stubs.

4. **Docs** — DELETED the deprecation banner section from RFC-0011. ADDED a
   `## Changelog` section with v1.0 / v1.1 / v2.0 rows. Cite removal commit
   `2c28cbb2`.

5. **Mission state** — Mission `0011-core-output-envelope-redaction`'s stub
   banner code (added in v1.0) was deleted in this mission.

### Cargo deps

None added or removed. Pure deletion.

## Test Vectors

`tv_dep1_warning_text` + `tv_dep2_exit_65` DELETED with `commands/stub.rs` (4 unit tests) + `tests/stub.rs` (4 integration tests) per RFC-0011 §Changelog. Two new tests pin the v2.0 `StaleStub` variant in `crates/octo-cli/src/error.rs`:
`tv_stalestub_v2_replaced_by_display_format` asserts the Display format carries both `name` and `replaced_by` substrings plus the `octo --help` operator pointer, and `tv_stalestub_v2_replaced_by_user_message` pins the `user_message()` envelope.

Post-cut verification:

- `grep -r "Commands::Init\|Commands::Join\|Commands::Status" crates/octo-cli/src/` → 0 hits (variants removed from the enum entirely per §Scope, not hidden)
- `cargo test -p octo-cli --lib` → 0 references to deleted stub commands
- `octo init` → "error: unrecognized subcommand 'init'" (clap default after v2.0 removal; exit 2)
- `octo join` → "error: unrecognized subcommand 'join'" (exit 2; `octo network bootstrap` DEFERRED to future amendment)
- `octo status` → "error: unrecognized subcommand 'status'" (exit 2; `octo network status` DEFERRED)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C) — pure deletion; no new types
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
  `octo join`, `octo status`, etc. Per RFC migration etiquette, this is
  acceptable after 1 release cycle deprecation + 1 release cycle hard-error.
- `octo` now exposes the post-cut surface per `crates/octo-cli/src/lib.rs`
  `Commands` enum: `whoami`, `identity {show,rotate,revoke}`,
  `capability {list,mint,attenuate}`, `policy {show,list}`,
  `role {select,...}` (RFC-0011-d), `reputation {list,show}` (RFC-0011-b),
  `mesh {peers,connect,status,...}` (RFC-0011-f), `vault {list,balance,...}` (RFC-0011-e),
  `agent {create,run,list,destroy,attach}` (RFC-0011-c),
  `governance {snapshot,attest,vote,...}` (RFC-0011-g), `audit {list,show,redact,...}`
  (RFC-0011-a).
- `OctoCliError::StaleStub` retained with the `replaced_by: &'static str`
  field — library-API soft sentinel for any downstream consumer of
  `OctoCliError` that matches on the variant. CLI operators never
  observe `StaleStub` post-cut because clap intercepts `octo init` /
  `octo join` / `octo status` with `unrecognized subcommand` (exit 2)
  before the variant can be constructed; the field is preserved for
  library consumers under the `#[non_exhaustive]` additive contract
  per [[cipherocto-design-principles]] §Extension over enumeration.

## Cross-references

- RFC-0011 §Compatibility — stub deprecation timeline
- RFC-0011 §Status header amendment chain — role provisioning (lands role
  select), agent lifecycle (lands agent lifecycle), mesh operations (lands
  network bootstrap + status)
- [[cipherocto-design-principles]] — Layer C per-RFC evolution

## Claimant

@unassigned
