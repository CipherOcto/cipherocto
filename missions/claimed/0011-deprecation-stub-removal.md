---
name: 0011-deprecation-stub-removal
description: Drop stub commands (init, join, role, agent, status) per RFC-0011 stub deprecation timeline
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "1.0"
  depends_on:
    - RFC-0011
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
  release_gate:
    require: "1 release cycle elapsed after v1.1 hard-error (StaleStub, exit 65)"
    released_version: TBD
status: Open
---

# 0011-deprecation-stub-removal — Drop stub commands (init, join, role, agent, status)

**Status:** Open — release-gated on the v1.1 hard-error (`StaleStub`, exit 65) cycle elapsed before v2.0 stub removal. Per SPEC-10, this mission's status was reverted from an earlier Claimed marker because the substrate amendment that lands the hard-error behaviour has not shipped yet. Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] once the gate clears per RFC-0011 §Compatibility
**Substrate:** RFC-0011 §Compatibility (stub deprecation timeline)
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-identity-commands` (or equivalent identity substrate landed)
- Mission `0011-capability-commands`
- Mission `0011-policy-commands`
- 1 release cycle elapsed since RFC-0011 (initial) + 1 release cycle elapsed
  since the next minor (per RFC migration etiquette: 1 release deprecation
  window + 1 release hard-error window before removal — applies to the
  initial deprecation cycle of this RFC, with forward amendments each
  following the same etiquette per Status header amendment chain)
  **Blocks:** none

## Status

Open — release-gated on the v1.1 hard-error (`StaleStub`, exit 65) cycle elapsed before v2.0 stub removal. SPEC-10 reverts this mission's status from the earlier Claimed marker because the substrate amendment that lands the hard-error behaviour has not shipped yet; no implementation work is permitted until the gate clears per RFC-0011 §Compatibility

## RFC

RFC-0011 (see rfcs/draft/process/0011-octo-cli-substrate.md §Compatibility)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [ ] Pre-removal gate check verified (v1.1 hard-error cycle elapsed)
- [ ] `commands/stub.rs` deleted (or stripped to empty)
- [ ] `Commands::Init/Join/Role/Agent/Status` variants removed from clap derive struct
- [ ] Tests referencing stub commands deleted
- [ ] Deprecation banner section deleted from RFC-0011
- [ ] CHANGELOG entry added: "Stub commands removed in v2.0"
- [ ] Cross-mission AC: final integration — `octo` now exposes only RFC-0011 subcommands + amendments
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test --workspace --lib green
- [ ] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011 type                                               | Sub-step                      | Notes                                                                 |
| ----------------------------------------------------------- | ----------------------------- | --------------------------------------------------------------------- |
| 5 stub commands (`init`, `join`, `role`, `agent`, `status`) | Sub-step 2 (code removal)     | Layer C/D; pure deletion from clap derive struct                      |
| `StaleStub` exit 65 path                                    | Sub-step 1 (pre-removal gate) | Layer C/D; verifies v1.1 hard-error cycle elapsed before v2.0 removal |

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

2 new TV covering the deprecation-warning + hard-error transitions:

- `tv_dep1_warning_text` — `octo init` in the v1.0 window emits
  `DEPRECATED: \`octo init\` is a stub...` to stderr and exits 0 (deprecation
  banner; command still works) — NEW
- `tv_dep2_exit_65` — `octo init` in the v1.1 window emits the deprecation
  banner PLUS exits 65 (`StaleStub` hard error per RFC-0011 §Exit Code table) — NEW

Pre-removal verification (unchanged):

- `grep -r "Commands::Init\|Commands::Join\|Commands::Role\|Commands::Agent\|Commands::Status" crates/octo-cli/src/` → 0 hits
- `cargo test -p octo-cli --all-features` → 0 reference to `init`/`join`/`role`/`agent`/`status` subcommands
- `octo init` → "error: unrecognized subcommand 'init'" (clap default after removal)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — pure deletion; no new types
- NO substrate crate changes

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-cli --all-features
# Hard error gate check:
octo init 2>&1; echo $?  # expect 65 (StaleStub hard error) OR "unrecognized subcommand" if v1.1 already shipped
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
