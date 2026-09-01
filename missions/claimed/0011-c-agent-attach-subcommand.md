---
name: 0011-c-agent-attach-subcommand
description: Land the `octo agent attach` subcommand per RFC-0011-c
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
    - mission 0011-c-agent-create-subcommand
    - mission 0011-c-agent-run-subcommand
    - mission 0011-c-octo-runtime-substrate
release_gate: octo-runtime substrate landing (per RFC-0011-c §Implementation Phases Phase 1)
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-c-agent-attach-subcommand — `octo agent attach` subcommand

**Status:** Open
**Substrate:** RFC-0011-c §9.3.5 (`octo agent attach <agent-id>`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-create-subcommand` — `Commands::Agent` clap root variant
- Mission `0011-c-agent-run-subcommand` — agent lifecycle context (attach requires a previously-running agent)
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Open (RFC-0011-c §Phase 2 CLI wiring, subcommand 5 of 5). **Release-gated** on `octo-runtime` substrate landing per RFC-0011-c §Implementation Phases Phase 1.

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.5 `octo agent attach <agent-id>` and §9.8 Error Handling (`AgentNotFound`, `AgentNotRunning`, `RuntimeAttachFailed`).

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` + `0011-c-agent-run-subcommand` → `0011-c-agent-attach-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission; attach requires a previously-running agent). The `octo-runtime` substrate crate must exist for `attach` to be callable; until it lands, `agent attach` ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51).

## Acceptance Criteria

- [ ] `octo agent attach <agent-id>` implemented + unit-tested (TV-AGT11, TV-AGT12 pass per RFC-0011-c §Test Vectors)
- [ ] `AgentAttachOutput` payload type implemented + unit-tested (`agent_id`, `runtime_handle`, `attached_at_unix`, `event_cursor`)
- [ ] **Read-only attach verified** — no state mutation; substrate `attach` is a pure read
- [ ] `--since <unix-seconds>` flag implemented (replay from timestamp; per RFC-0011-c §9.3.5)
- [ ] `OctoCliRedactor` patterns applied (same set as `agent create` per RFC-0011-c §Security)
- [ ] `AgentNotFound(Uuid)` (exit 42), `AgentNotRunning(Uuid)` (exit 48), `RuntimeAttachFailed { reason }` (exit 49) wired (per RFC-0011-c §9.8 slot allocation 39-52)
- [ ] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)
- [ ] Release gate cleared: `octo-runtime` substrate mission merged (per RFC-0011-c §Implementation Phases Phase 1)

### Type Coverage

| RFC-0011-c type                       | Sub-step                | Notes                                                                                                                                                  |
| ------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `AgentAttachArgs`                     | Sub-step 1 (clap)       | Layer C/D; clap derive struct (`agent_id: Uuid`, `--since <u64>`, `--json`)                                                                            |
| `AgentAttachOutput`                   | Sub-step 2 (output)     | Layer C/D; CLI-output wrapper (`agent_id: Uuid`, `runtime_handle: String`, `attached_at_unix: u64`, `event_cursor: Option<String>`)                    |
| `AgentNotRunning(Uuid)`               | Sub-step 3 (errors)     | Layer C/D; new `OctoCliError` variant; exit 48 per RFC-0011-c §9.8 (reserved 17–63 range)                                                              |
| `RuntimeAttachFailed { reason }`      | Sub-step 3 (errors)     | Layer C/D; new `OctoCliError` variant; exit 49                                                                                                         |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::Attach` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent attach` is the **only** read-only subcommand in the agent group that requires runtime substrate (per RFC-0011-c §9.3.5). It does not mutate state; it returns an `event_cursor` that the operator can use to consume events from the runtime substrate pub-sub bus.

The `octo-runtime` substrate crate is required for `attach`. Until it lands, this subcommand ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51).

## Risk

- **MEDIUM** — runtime may refuse the attach (e.g., agent is terminated but substrate cache is stale). Mitigation: `AgentNotRunning(uuid)` (exit 48) surfaces the substrate reason; operator can retry or run `agent list` to verify state.
- **LOW** — `--since <unix-seconds>` may reference a timestamp before the runtime started. Substrate returns `InvalidSince` (handled by substrate; CLI surfaces verbatim); no client-side validation needed.

## Scope

Land the `octo agent attach` subcommand per RFC-0011-c §9.3.5. The four sibling subcommands (`create`, `run`, `list`, `destroy`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-run-subcommand`, `0011-c-agent-list-subcommand`, `0011-c-agent-destroy-subcommand`.

## Sub-steps

1. **`AgentAction::Attach` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.5). Add `Attach(AgentAttachArgs)` variant to existing `AgentAction` enum from mission `0011-c-agent-create-subcommand`.

2. **`AgentAttachOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.

3. **CLI handler** — same file. `agent attach` calls `octo_runtime::attach(handle, since)`; surfaces `runtime_handle`, `attached_at_unix`, `event_cursor` in output. Respects `--since <unix-seconds>` (replay from timestamp; substrate validates). Respects `--json` (TTY-override). **No state mutation** — attach is read-only.

4. **`AgentNotRunning`, `RuntimeAttachFailed` error variants + exit 48/49 mapping** — `crates/octo-cli/src/error.rs` (Layer C/D). Add two variants to the `#[non_exhaustive] OctoCliError` enum; map to exits 48/49 per RFC-0011-c §9.8 (slot allocation 39-52).

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Implementation Phases
octo-wallet = { path = "../octo-wallet" }     # Layer B substrate (RFC-0011-c §Key Files to Modify)
octo-runtime = { path = "../octo-runtime" }   # Layer B substrate (RFC-0011-c §9.1 Architecture)
```

No new external crates required; all substrate types are defined in `octo-wallet` and `octo-runtime` and re-used by the CLI.

## Test Vectors (per RFC-0011-c §Test Vectors — `agent attach` group)

2 TV (TV-AGT11..TV-AGT12) covering `agent attach`:

| #       | Subcommand       | Input                       | Expected Output                                                  | Notes                              |
| ------- | ---------------- | --------------------------- | ---------------------------------------------------------------- | ---------------------------------- |
| TV-AGT11| `agent attach`   | Running agent               | `AgentAttachOutput { runtime_handle: ..., ... }` (exit 0)        | Read-only attach                   |
| TV-AGT12| `agent attach`   | Terminated agent            | `AgentNotRunning(uuid)` (exit 48)                                | Attach to non-running agent rejected |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `AgentAction::Attach` dispatch + `AgentAttachOutput` payload type + 2 error variants.
- `octo-wallet` (Layer B) — substrate `AgentState` (existing types; used to surface `AgentNotRunning`).
- `octo-runtime` (Layer B) — substrate `attach`, `RuntimeHandle` (new crate; per RFC-0011-c §9.1 Architecture).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `AgentAction::Attach` variant added; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-c §9.8 Error Handling (2 new variants: exit 48/49; slot allocation 39-52).
- `OutputEnvelope<T>::schema_version = 4` pinned (RFC-0011-c §9.4 / §9.4.1 Divergence slot table). Field renames from parent v2:
  - `data: T` → `payload: T`
  - `generated_at: DateTime` → `executed_at_unix: u64`
  - `preview_only: bool` → `redacted: bool`
  - `command: String` (ADDED)
  Old CLI ignores unknown fields.
- If `octo-runtime` substrate is not yet landed, this mission ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51); no operator-facing state change.

## Cross-references

- RFC-0011-c §9.3.5 `octo agent attach` subcommand specification
- RFC-0011-c §9.8 Error Handling (2 new variants: `AgentNotRunning`, `RuntimeAttachFailed`)
- RFC-0011-c §9.1 Architecture (octo-runtime substrate dependency)
- RFC-0011-c §Implementation Phases Phase 1 (octo-runtime substrate release gate)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why gate

Release-gated on companion substrate mission `0011-c-octo-runtime-substrate` landing (per RFC-0011-c §Implementation Phases Phase 1). Until `0011-c-octo-runtime-substrate` lands, the subcommand ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51). The gate is enforced in CI via the `release_gate:` frontmatter annotation; the mission cannot be marked Completed without the substrate mission in the dependency graph being Closed first.

## Claimant

@unassigned