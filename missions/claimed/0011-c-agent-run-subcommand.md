---
name: 0011-c-agent-run-subcommand
description: Land the `octo agent run` subcommand per RFC-0011-c
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
    - mission 0011-c-octo-runtime-substrate
release_gate: 0011-c-octo-runtime-substrate mission landing (per RFC-0011-c §Implementation Phases Phase 1)
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
substrate_unblocked: 2026-09-13
---

# 0011-c-agent-run-subcommand — `octo agent run` subcommand

**Status:** Open
**Substrate:** RFC-0011-c §9.3.2 (`octo agent run <agent-id>`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-create-subcommand` — `Commands::Agent` clap root variant
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Open (RFC-0011-c §Phase 2 CLI wiring, subcommand 2 of 5). Conditionally depends on `octo-runtime` substrate per RFC-0011-c §9.1 Architecture.

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.2 `octo agent run <agent-id>` and §9.8 Error Handling (`AgentNotFound`, `InvalidStateTransition`, `RuntimeSpawnFailed`).

### Substrate additions landed (commit `next e09f3e3a`, 2026-09-13)

The following substrate surface is now in place so this mission can be implemented in a follow-on session without further substrate work:

- `octo_wallet::transition_agent(caller_did, uuid, target: AgentState, reason)` (Layer B) — full state-machine guard per RFC-0015-a Appendix A. The `Registered → Running` edge is exercised by this mission.
- `octo_wallet::TransitionReceipt` projection struct (Layer B) re-exported from `crates/octo-wallet/src/lib.rs`.
- `WalletError::AlreadyInTransition(Uuid)` / `InvalidStateTransition { from, to }` / `AuditUnavailable(String)` variants.
- `OctoCliError::AlreadyInTransition(Uuid)` → exit 43, `InvalidStateTransition { from, to }` → exit 43 (mirrors), `RuntimeSpawnFailed { reason }` → exit 44 (still to add — part of this mission), `AgentNotFound(Uuid)` → exit 42.
- `AuditEventKind::AgentTransition` variant cfg-gated behind `octo-audit-internal` feature (Layer A frozen contract preserved); permanent once RFC-0012-v2 lands Accepted.

### Still pending at substrate level

- The `octo-runtime` substrate crate provides `spawn_agent(agent_id, handle) -> RuntimeHandle` (Layer B). If not landed, this mission ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51).

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` → `0011-c-agent-run-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission). The `octo-runtime` substrate crate must exist for `spawn_agent` to be callable; until it lands, `agent run` ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51).

## Acceptance Criteria

- [ ] `octo agent run <agent-id>` implemented + unit-tested (TV-AGT4, TV-AGT5 pass per RFC-0011-c §Test Vectors)
- [ ] `AgentRunOutput` payload type implemented + unit-tested (`agent_id`, `state`, `runtime_handle`, `spawned_at_unix`)
- [ ] State transition REGISTERED → ACTIVE → BUSY verified end-to-end against RFC-0002 §Agent State Machine
- [ ] `OctoCliRedactor` patterns applied (same set as `agent create` per RFC-0011-c §Security)
- [ ] `AgentNotFound(Uuid)` (exit 42), `InvalidStateTransition { from, to }` (exit 43), `RuntimeSpawnFailed { reason }` (exit 44) wired (per RFC-0011-c §9.8 slot allocation 39-52)
- [ ] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [ ] `--detach` flag implemented (default: detached; per RFC-0011-c §9.3.2)
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-c type                       | Sub-step            | Notes                                                                                                                   |
| ------------------------------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `AgentRunArgs`                        | Sub-step 1 (clap)   | Layer C/D; clap derive struct (`agent_id: Uuid`, `--detach`, `--json`)                                                  |
| `AgentRunOutput`                      | Sub-step 2 (output) | Layer C/D; CLI-output wrapper (`agent_id: Uuid`, `state: AgentState`, `runtime_handle: String`, `spawned_at_unix: u64`) |
| `AgentNotFound(Uuid)`                 | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 42 per RFC-0011-c §9.8 (reserved 17–63 range)                               |
| `InvalidStateTransition { from, to }` | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 43                                                                          |
| `RuntimeSpawnFailed { reason }`       | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 44                                                                          |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::Run` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent run` performs **two** state transitions (REGISTERED → ACTIVE → BUSY) per RFC-0011-c §9.5 Agent State Machine Integration. The CLI does not collapse these into a single transition; the substrate enforces each transition separately per RFC-0002 §Agent State Machine.

The `octo-runtime` substrate crate is required for `spawn_agent`. Until it lands, this subcommand ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51).

## Risk

- **HIGH** — state machine transitions must match RFC-0002 §Agent State Machine byte-for-byte. Mitigation: substrate returns `InvalidStateTransition` with the failing transition; CLI surfaces verbatim.
- **MEDIUM** — runtime container spawn may fail under load. Mitigation: `RuntimeSpawnFailed { reason }` (exit 44) surfaces the substrate reason; operator can retry.
- **LOW** — concurrent `agent run` invocations against the same `agent_id`. Substrate rejects with `InvalidStateTransition`; CLI surfaces.

## Scope

Land the `octo agent run` subcommand per RFC-0011-c §9.3.2. The four sibling subcommands (`create`, `list`, `destroy`, `attach`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-list-subcommand`, `0011-c-agent-destroy-subcommand`, `0011-c-agent-attach-subcommand`.

## Sub-steps

1. **`AgentAction::Run` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.2). Add `Run(AgentRunArgs)` variant to existing `AgentAction` enum from mission `0011-c-agent-create-subcommand`.

2. **`AgentRunOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.

3. **CLI handler** — same file. `agent run` calls `octo_wallet::transition_agent(caller_did, agent_id, AgentState::Running, reason)` (Layer B; substrate enforces caller-attestation against holder_did per RFC-0011 §Lifecycle Requirements, then the state-machine guard) then `octo_runtime::spawn_agent(agent_id, handle)`; surfaces `runtime_handle` in output alongside `TransitionReceipt::audit_log_entry` (Hex32). Respects `--detach` (default: detached; spawn does not block). Respects `--json` (TTY-override).

4. **`AgentNotFound`, `InvalidStateTransition`, `RuntimeSpawnFailed` error variants + exit 42/43/44 mapping** — `crates/octo-cli/src/error.rs` (Layer C/D). Add three variants to the `#[non_exhaustive] OctoCliError` enum; map to exits 42/43/44 per RFC-0011-c §9.8 (slot allocation 39-52).

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Implementation Phases
octo-wallet = { path = "../octo-wallet" }     # Layer B substrate (RFC-0011-c §Key Files to Modify)
octo-runtime = { path = "../octo-runtime" }   # Layer B substrate (RFC-0011-c §9.1 Architecture)
```

No new external crates required; all substrate types are defined in `octo-wallet` and `octo-runtime` and re-used by the CLI.

## Test Vectors (per RFC-0011-c §Test Vectors — `agent run` group)

2 TV (TV-AGT4..TV-AGT5) covering `agent run`:

| #       | Subcommand  | Input                        | Expected Output                                                      | Notes                                                                     |
| ------- | ----------- | ---------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| TV-AGT4 | `agent run` | Registered agent, no runtime | `AgentRunOutput { state: BUSY, ... }` (exit 0)                       | Spawns runtime container; warm path                                       |
| TV-AGT5 | `agent run` | Terminated agent             | `InvalidStateTransition { from: terminated, to: running }` (exit 43) | State machine rejects (canonical lowercase `AgentState::as_str()` labels) |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `AgentAction::Run` dispatch + `AgentRunOutput` payload type + 3 error variants.
- `octo-wallet` (Layer B) — substrate `transition_agent`, `AgentState` (existing types).
- `octo-runtime` (Layer B) — substrate `spawn_agent` (new crate; per RFC-0011-c §9.1 Architecture).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `AgentAction::Run` variant added; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-c §9.8 Error Handling (3 new variants: exit 42/43/44; slot allocation 39-52).
- `OutputEnvelope<T>::schema_version = 4` pinned (RFC-0011-c §9.4 / §9.4.1 Divergence slot table). Field renames from parent v2:
  - `data: T` → `payload: T`
  - `generated_at: DateTime` → `executed_at_unix: u64`
  - `preview_only: bool` → `redacted: bool`
  - `command: String` (ADDED)
    Old CLI ignores unknown fields.
- If `octo-runtime` substrate is not yet landed, this mission ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51); no operator-facing state change.

## Cross-references

- RFC-0011-c §9.3.2 `octo agent run` subcommand specification
- RFC-0011-c §9.5 Agent State Machine Integration (REGISTERED → ACTIVE → BUSY mapping)
- RFC-0011-c §9.8 Error Handling (3 new variants: `AgentNotFound`, `InvalidStateTransition`, `RuntimeSpawnFailed`)
- RFC-0011-c §9.1 Architecture (octo-runtime substrate dependency)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- RFC-0002 §Replay Protection (replay substrate)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why gate

Release-gated on companion substrate mission `0011-c-octo-runtime-substrate` landing (per RFC-0011-c §Implementation Phases Phase 1). Until `0011-c-octo-runtime-substrate` lands, the subcommand ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51). The gate is enforced in CI via the `release_gate:` frontmatter annotation; the mission cannot be marked Completed without the substrate mission in the dependency graph being Closed first.

## Substrate Gap (hard-checked 2026-09-11)

Substrate verification confirms:

- `octo_runtime::spawn_agent` EXISTS at `crates/octo-runtime/src/spawn.rs`
- `octo_runtime::error::RuntimeError::RuntimeSpawnFailed` EXISTS at
  `crates/octo-runtime/src/error.rs` with exit 44 wired
- `octo_runtime::handle::RuntimeHandle` EXISTS

Substrate gap blocking implementation:

- `octo_wallet::transition_agent` referenced by Sub-step 3 (CLI handler
  calls `octo_wallet::transition_agent(agent_id, Active)`) DOES NOT
  exist in `crates/octo-wallet/src/` (verified via
  `grep -rE "pub (fn|async fn) " crates/octo-wallet/src/`).

**Unblock path:** add `pub fn transition_agent(uuid: Uuid, target: AgentState, reason: Option<&str>) -> Result<AgentSummary, WalletError>`
to `crates/octo-wallet/src/agent.rs` (small additive; ~30 LoC + state
machine guard tests). Until that lands, this mission ships as a stub
emitting `RuntimeSubstrateNotReady` (exit 51) per §Backward compat.
The release_gate on `0011-c-octo-runtime-substrate` is partially
satisfied (`octo-runtime` crate exists); the wallet transition surface
is the residual blocker per RFC-0002 §Agent State Machine substrate.

**Implementation cannot proceed** until the substrate addition lands.
Mission remains `Claimed` per [[memory-is-never-status-ground-truth]].

## Claimant

@unassigned
