---
name: 0011-c-agent-run-subcommand
description: Land the `octo agent run` subcommand per RFC-0011-c
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
    - mission 0011-c-agent-create-subcommand
    - mission 0011-c-octo-runtime-substrate
release_gate: 0011-c-octo-runtime-substrate mission landing (per RFC-0011-c §Implementation Phases Phase 1)
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
completed_at: 2026-09-15
completed_by: mmacedoeu
implementation_commit: 80fa0d66
substrate_commit: e09f3e3a
review_rounds: 5
dry_closure_audit: docs/audits/2026-09-15-0011-c-agent-run-subcommand-closure.md
---

# 0011-c-agent-run-subcommand — `octo agent run` subcommand

**Status:** Closed (2026-09-15)
**Substrate:** RFC-0011-c §9.3.2 (`octo agent run <agent-id>`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-create-subcommand` — `Commands::Agent` clap root variant
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Closed 2026-09-15 (RFC-0011-c §Phase 2 CLI wiring, subcommand 2 of 5).
DRY CLOSED gate achieved after 5 review rounds + R5.5 fix-up.

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.2 `octo agent run <agent-id>` and §9.8 Error Handling (`AgentNotFound`, `InvalidStateTransition`, `RuntimeSpawnFailed`).

### Substrate additions landed (commit `next e09f3e3a`, 2026-09-13)

The following substrate surface is now in place so this mission can be implemented in a follow-on session without further substrate work:

- `octo_wallet::transition_agent(caller_did, uuid, target: AgentState, reason)` (Layer B) — full state-machine guard per RFC-0015-a Appendix A. The `Registered → Running` edge is exercised by this mission.
- `octo_wallet::TransitionReceipt` projection struct (Layer B) re-exported from `crates/octo-wallet/src/lib.rs`.
- `WalletError::AlreadyInTransition(Uuid)` / `InvalidStateTransition { from, to }` / `AuditUnavailable(String)` variants.
- `OctoCliError::AlreadyInTransition(Uuid)` → exit 43, `InvalidStateTransition { from, to }` → exit 43 (mirrors), `RuntimeSpawnFailed { reason }` → exit 44, `AgentNotFound(Uuid)` → exit 42.
- `AuditEventKind::AgentTransition` variant cfg-gated behind `octo-audit-internal` feature (Layer A frozen contract preserved); permanent once RFC-0012-v2 lands Accepted.

### Still pending at substrate level

- The `octo-runtime` substrate crate provides `spawn_agent(agent_id, handle) -> RuntimeHandle` (Layer B).

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` → `0011-c-agent-run-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission).

## Acceptance Criteria

- [x] `octo agent run <agent-id>` implemented + unit-tested (TV-AGT4, TV-AGT5 pass per RFC-0011-c §Test Vectors)
- [x] `AgentRunOutput` payload type implemented + unit-tested (`agent_id`, `state`, `runtime_handle`, `spawned_at_unix`)
- [x] State transition Registered → Running verified end-to-end against RFC-0002 §Agent State Machine
- [x] `OctoCliRedactor` patterns applied (same set as `agent create` per RFC-0011-c §Security)
- [x] `AgentNotFound(Uuid)` (exit 42), `InvalidStateTransition { from, to }` (exit 43), `RuntimeSpawnFailed { reason }` (exit 44) wired (per RFC-0011-c §9.8 slot allocation 39-52)
- [x] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [x] `--detach` flag implemented (default: in-process per clap `default_value_t = false`; per RFC-0011-c §9.3.2 — substrate-faithful default aligned with substrate behavior)
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green (286 lib tests pass on `next`)
- [x] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-c type                       | Sub-step            | Notes                                                                                                                   |
| ------------------------------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `AgentRunArgs`                        | Sub-step 1 (clap)   | Layer C/D; clap derive struct (`agent_id: Uuid`, `--detach`, `--json`)                                                  |
| `AgentRunOutput`                      | Sub-step 2 (output) | Layer C/D; CLI-output wrapper (`agent_id: Uuid`, `state: AgentState`, `runtime_handle: Option<RedactedIdentifier>`, `spawned_at_unix: u64`) |
| `AgentNotFound(Uuid)`                 | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 42 per RFC-0011-c §9.8 (reserved 17–63 range)                               |
| `InvalidStateTransition { from, to }` | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 43                                                                          |
| `RuntimeSpawnFailed { reason }`       | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 44                                                                          |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::Run` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent run` performs the canonical `Registered → Running` edge per RFC-0011-c §9.5 Agent State Machine Integration. The substrate enforces the single transition per RFC-0002 §Agent State Machine (the `AgentState` enum is `Registered | Running | Terminated` per `octo_wallet::AgentState`).

The `octo-runtime` substrate crate provides `spawn_agent`.

## Risk

- **HIGH** — state machine transitions must match RFC-0002 §Agent State Machine byte-for-byte. Mitigation: substrate returns `InvalidStateTransition` with the failing transition; CLI surfaces verbatim.
- **MEDIUM** — runtime container spawn may fail under load. Mitigation: `RuntimeSpawnFailed { reason }` (exit 44) surfaces the substrate reason; operator can retry.
- **LOW** — concurrent `agent run` invocations against the same `agent_id`. Substrate rejects with `InvalidStateTransition`; CLI surfaces.

## Scope

`octo agent run` subcommand per RFC-0011-c §9.3.2. The four sibling subcommands (`create`, `list`, `destroy`, `attach`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-list-subcommand`, `0011-c-agent-destroy-subcommand`, `0011-c-agent-attach-subcommand`.

## Sub-steps

1. **`AgentAction::Run` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.2). Add `Run(AgentRunArgs)` variant to existing `AgentAction` enum from mission `0011-c-agent-create-subcommand`.

2. **`AgentRunOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.

3. **CLI handler** — same file. `agent run` calls `octo_wallet::transition_agent(caller_did, agent_id, AgentState::Running, reason)` (Layer B; substrate enforces caller-attestation against holder_did per RFC-0011 §Lifecycle Requirements, then the state-machine guard) then `octo_runtime::spawn_agent(agent_id, handle)`; surfaces `runtime_handle` in output alongside `TransitionReceipt::audit_log_entry` (BLAKE3-256 chain-hash `[u8; 32]`; the CLI hex-encodes it via `OctoCliRedactor` for the wire form). Respects `--detach` (default: in-process; spawn does not block). Respects `--json` (TTY-override).

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
| TV-AGT4 | `agent run` | Registered agent, no runtime | `AgentRunOutput { state: running, ... }` (exit 0)                    | Spawns runtime container; warm path                                       |
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
cargo test -p octo-cli --lib --tests  # green (286 lib tests)
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

## Cross-references

- RFC-0011-c §9.3.2 `octo agent run` subcommand specification
- RFC-0011-c §9.5 Agent State Machine Integration (Registered → Running mapping)
- RFC-0011-c §9.8 Error Handling (3 new variants: `AgentNotFound`, `InvalidStateTransition`, `RuntimeSpawnFailed`)
- RFC-0011-c §9.1 Architecture (octo-runtime substrate dependency)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- RFC-0002 §Replay Protection (replay substrate)
- RFC-0015-a Appendix A (operative `transition_agent` semantics)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain
- [[memory-is-never-status-ground-truth]] — provenance rule

## Why gate

Release-gated on companion substrate mission `0011-c-octo-runtime-substrate` landing (per RFC-0011-c §Implementation Phases Phase 1). The `octo-runtime` substrate crate provides `spawn_agent`.

## Substrate Gap Closure (2026-09-13)

Substrate state verified after commit `next e09f3e3a`:

- `octo_runtime::spawn_agent` EXISTS at `crates/octo-runtime/src/spawn.rs`
- `octo_runtime::error::RuntimeError::RuntimeSpawnFailed` EXISTS at
  `crates/octo-runtime/src/error.rs` with exit 44 wired
- `octo_runtime::handle::RuntimeHandle` EXISTS
- `octo_wallet::transition_agent(caller_did: &Did, uuid: Uuid, target: AgentState, reason: Option<&str>) -> Result<TransitionReceipt, WalletError>`
  EXISTS at `crates/octo-wallet/src/agent.rs` (Layer B; state-machine
  guard accepts `Registered → Running` and `Running → Terminated` per
  RFC-0015-a Appendix A). `TransitionReceipt` projection
  (`agent_id, previous_state, current_state, transitioned_at_unix,
  audit_log_entry: [u8; 32]`) is re-exported via
  `crates/octo-wallet/src/lib.rs`.

## RFC-0015-b substrate-defect dependency

7 substrate defects documented for RFC-0015-b paired amendment per
`docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`.
RFC-0015-b is the formal amendment surface; this mission interacts
with 2 of the 7 defects:

- **Defect 1** (`WalletError::AlreadyInTransition(Uuid)` dead surface) — once RFC-0015-b activates the variant for concurrent-call detection, this mission's CLI surface must include the mirror `OctoCliError::AlreadyInTransition(Uuid)` exit-code path (already declared at RFC-0015-a acceptance; re-verified at amendment landing).
- **Defect 4** (`transition_agent` TOCTOU window — `validate_reason` runs before lock acquisition) — this mission calls `transition_agent(Registered → Running)`. Substrate amendment moves `validate_reason` INSIDE the lock. CLI behavior unchanged (signature preserved) but test vectors must add TOCTOU regression coverage.

Remaining 5 defects (2 doc-comment drift, 3 lookup_agent existence-leak, 5 phantom-event window, 6 missing state-machine tests, 7 RFC parity gap) do NOT affect this mission's CLI surface.

**Hard sequencing:** RFC-0015-b acceptance (Draft → Accepted) is required BEFORE this mission's CLI implementation lands (per [[no-phantom-mission-pointer]] rule; the dependency is on the amendment RFC, not on the paired substrate-implementation mission which lands post-acceptance).

## DRY CLOSURE chain

| Round | Status | Notable |
|---|---|---|
| R1 | 30+ findings | 2 HIGH + 9 MED + 4 LOW per R4.5 commit description |
| R1.5 | fix landed | `80fa0d66` — 13 substantive fixes (HIGH-1 `render_with_redaction` semantic inversion + HIGH-2 stale stub removal + 11 MED/LOW) |
| R2 | findings | MED aggregation → R2.5 fix |
| R2.5 | fix landed | MED fixes (task #975) |
| R3 | zero | first zero round |
| R4 | zero | second zero round |
| R5 | 1 MED (layer-model facade reach-in in list handler) | sibling-mission scope from `0011-c-agent-list` which closed DRY CLEAN 2026-09-13 |
| R5.5 | fix landed | `f1c1d44b` — 1-line façade import fix |
| R5 (effective post-R5.5) | zero | DRY CLOSED gate |

## Closure artifacts

- `docs/audits/2026-09-15-0011-c-agent-run-subcommand-closure.md` — full audit doc
- `~/.claude/projects/.../memory/0011c-agent-run-closure-2026-09-15.md` — memory card
- `MEMORY.md` index — pointer to memory card

## Claimant

@unassigned
