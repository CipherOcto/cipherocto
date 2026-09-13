---
name: 0011-c-agent-destroy-subcommand
description: Land the `octo agent destroy` subcommand per RFC-0011-c
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
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
substrate_unblocked: 2026-09-13
---

# 0011-c-agent-destroy-subcommand — `octo agent destroy` subcommand

**Status:** Open
**Substrate:** RFC-0011-c §9.3.4 (`octo agent destroy <agent-id>`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-create-subcommand` — `Commands::Agent` clap root variant
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Open (RFC-0011-c §Phase 2 CLI wiring, subcommand 4 of 5).

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.4 `octo agent destroy <agent-id>` and §9.8 Error Handling (`AgentNotFound`, `ConfirmationRequired`, `InvalidStateTransition`).

### Substrate additions landed (commit `next e09f3e3a`, 2026-09-13)

The following substrate surface is now in place so this mission can be implemented in a follow-on session without further substrate work:

- `octo_wallet::transition_agent(caller_did, uuid, target: AgentState, reason)` (Layer B) — emits the `AgentTransition` audit event with `from`/`to`/`reason` payload, fully implementing the destroy state-machine edge (`Running → Terminated` per RFC-0015-a Appendix A). When the audit append is not available (feature-off), the function fails closed with `AuditUnavailable` and rolls back the state mutation.
- `octo_audit::append_agent_transition_event` + `register_audit_sink` (Layer B façade) — the runtime adapter registers its `AppendOnlyAuditSink` once at startup, and `transition_agent` appends to it via the façade.
- `octo_wallet::TransitionReceipt` projection struct re-exported from `crates/octo-wallet/src/lib.rs`.
- `WalletError::AlreadyInTransition(Uuid)` / `InvalidStateTransition { from, to }` / `AuditUnavailable(String)` variants.
- `OctoCliError::AuditSubstrateNotReady` → exit 52, `AlreadyInTransition(Uuid)` → exit 43, `InvalidStateTransition { from, to }` → exit 43, `AgentNotFound(Uuid)` → exit 42, `ConfirmationRequired` (still to add — part of this mission) → exit 47.
- `AuditEventKind::AgentTransition` variant cfg-gated behind `octo-audit-internal` feature (Layer A frozen contract preserved); permanent once RFC-0012-v2 lands Accepted.

### Still pending at substrate level

- The `ConfirmationRequired` error variant (exit 47) is reserved by RFC-0011-c §9.8 but the substrate enum shape is out-of-scope for unblock — this mission adds it.

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` → `0011-c-agent-destroy-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission). The audit log append is provided by RFC-0011-a substrate; if absent, this mission ships as a stub emitting `AuditSubstrateNotReady` (exit 52).

## Acceptance Criteria

- [ ] `octo agent destroy <agent-id>` implemented + unit-tested (TV-AGT9, TV-AGT10 pass per RFC-0011-c §Test Vectors)
- [ ] `AgentDestroyOutput` payload type implemented + unit-tested (`agent_id`, `state`, `terminated_at_unix`, `audit_log_entry`)
- [ ] **Confirmation gate enforced** — `--confirm` required flag; absent → `ConfirmationRequired` (CLI exit 2 / substrate exit 47); no `--yes` / `--force` override
- [ ] State transition ACTIVE → TERMINATED verified end-to-end against RFC-0002 §Agent State Machine
- [ ] Audit log append verified end-to-end against RFC-0011-a §7.4 Substrate [ADD] signatures (when landed)
- [ ] **Audit stub fallback** — if RFC-0011-a audit substrate is not yet Accepted, this mission ships as an audit-append-failed stub emitting `AuditSubstrateNotReady` (exit 52). Mission completion requires RFC-0011-a Accepted.
- [ ] `OctoCliRedactor` patterns applied (same set as `agent create` per RFC-0011-c §Security)
- [ ] `AgentNotFound(Uuid)` (exit 42), `ConfirmationRequired` (exit 47), `InvalidStateTransition { from, to }` (exit 43) wired (per RFC-0011-c §9.8 slot allocation 39-52)
- [ ] `--reason` flag implemented (audit log entry; per RFC-0011-c §9.3.4)
- [ ] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-c type        | Sub-step            | Notes                                                                                                                                       |
| ---------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `AgentDestroyArgs`     | Sub-step 1 (clap)   | Layer C/D; clap derive struct (`agent_id: Uuid`, `--confirm` (required), `--reason <string>`, `--json`)                                     |
| `AgentDestroyOutput`   | Sub-step 2 (output) | Layer C/D; CLI-output wrapper (`agent_id: Uuid`, `state: AgentState`, `terminated_at_unix: u64`, `audit_log_entry: Hex32`)                  |
| `ConfirmationRequired` | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 47 per RFC-0011-c §9.8 (reserved 17–63 range; substrate exit; CLI re-checks with parent exit 2) |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::Destroy` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent destroy` is the **only** subcommand in the agent group that requires a confirmation flag (`--confirm`). The CLI does not provide a `--yes` or `--force` override per RFC-0011-c §Security (Confirmation gate). Operators relying on scripted destruction must wrap the invocation in shell logic to confirm.

The audit log append is provided by RFC-0011-a §7.4 Substrate [ADD] signatures. If RFC-0011-a substrate is not yet landed, this mission ships as a stub emitting `AuditSubstrateNotReady` (exit 52). Mission completion requires RFC-0011-a Accepted.

## Risk

- **CRITICAL** — operator mistake destroying the wrong agent. Mitigation: `--confirm` gate enforced; no `--force` override; state machine rejects re-destroy (exit 43).
- **HIGH** — audit log append failure. Mitigation: substrate returns `AuditAppendFailed`; CLI surfaces and the state transition is rolled back per RFC-0011-a §7.4 Substrate [ADD] signatures.
- **LOW** — concurrent `agent destroy` invocations against the same `agent_id`. Substrate rejects with `InvalidStateTransition`; CLI surfaces.

## Scope

Land the `octo agent destroy` subcommand per RFC-0011-c §9.3.4. The four sibling subcommands (`create`, `run`, `list`, `attach`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-run-subcommand`, `0011-c-agent-list-subcommand`, `0011-c-agent-attach-subcommand`.

## Sub-steps

1. **`AgentAction::Destroy` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.4). Add `Destroy(AgentDestroyArgs)` variant to existing `AgentAction` enum from mission `0011-c-agent-create-subcommand`. **`--confirm`** is a required flag (clap `requires` annotation).

2. **`AgentDestroyOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.

3. **CLI handler** — same file. `agent destroy` calls `octo_wallet::transition_agent(caller_did, agent_id, AgentState::Terminated, reason)`; the substrate appends the `AgentTransition` audit event with `from = running, to = terminated, reason` payload via the `octo-audit` façade. Surfaces `audit_log_entry` digest (Hex32) from `TransitionReceipt` in `AgentDestroyOutput`. Respects `--reason` (audit log entry; default: empty string per `validate_reason(0..=256)` cap). Respects `--json` (TTY-override). **Refuses to proceed without `--confirm`** — emits `ConfirmationRequired` (CLI exit 2 / substrate exit 47) and aborts. When the audit sink is not registered AND `--features octo-audit-internal` is OFF, emits `OctoCliError::AuditSubstrateNotReady` (exit 52).

4. **`ConfirmationRequired` error variant + exit 47 mapping** — `crates/octo-cli/src/error.rs` (Layer C/D). Add `ConfirmationRequired` variant to the `#[non_exhaustive] OctoCliError` enum; map to exit 47 per RFC-0011-c §9.8 (slot allocation 39-52). The CLI also performs its own `--confirm` re-check surfacing parent RFC-0011 §Error Handling exit 2.

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Implementation Phases
octo-wallet = { path = "../octo-wallet" }   # Layer B substrate (RFC-0011-c §Key Files to Modify)
octo-audit = { path = "../octo-audit" }     # Layer B substrate (RFC-0011-a §7.4 Substrate [ADD] signatures)
```

No new external crates required; all substrate types are defined in `octo-wallet` and `octo-audit` and re-used by the CLI.

## Test Vectors (per RFC-0011-c §Test Vectors — `agent destroy` group)

2 TV (TV-AGT9..TV-AGT10) covering `agent destroy`:

| #        | Subcommand      | Input                        | Expected Output                                                                | Notes                      |
| -------- | --------------- | ---------------------------- | ------------------------------------------------------------------------------ | -------------------------- |
| TV-AGT9  | `agent destroy` | Active agent, `--confirm`    | `AgentDestroyOutput { state: TERMINATED, audit_log_entry: ..., ... }` (exit 0) | Audit log appended         |
| TV-AGT10 | `agent destroy` | Active agent, no `--confirm` | `ConfirmationRequired` (exit 47 substrate / exit 2 CLI)                        | Confirmation gate enforced |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `AgentAction::Destroy` dispatch + `AgentDestroyOutput` payload type + `ConfirmationRequired` error variant.
- `octo-wallet` (Layer B) — substrate `transition_agent`, `AgentState` (existing types).
- `octo-audit` (Layer B) — audit log append substrate (per RFC-0011-a §7.4 Substrate [ADD] signatures).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `AgentAction::Destroy` variant added; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-c §9.8 Error Handling (1 new variant: exit 47 substrate / exit 2 CLI; slot allocation 39-52; `AuditSubstrateNotReady` exit 52 if RFC-0011-a substrate not landed).
- `OutputEnvelope<T>::schema_version = 4` pinned (RFC-0011-c §9.4 / §9.4.1 Divergence slot table). Field renames from parent v2:
  - `data: T` → `payload: T`
  - `generated_at: DateTime` → `executed_at_unix: u64`
  - `preview_only: bool` → `redacted: bool`
  - `command: String` (ADDED)
    Old CLI ignores unknown fields.
- The `--confirm` flag is a clap-level requirement; no CLI flag short-form (`-c`, `--yes`, `--force`) is provided to bypass.

## Cross-references

- RFC-0011-c §9.3.4 `octo agent destroy` subcommand specification
- RFC-0011-c §9.8 Error Handling (1 new variant: `ConfirmationRequired` (substrate exit 47 / parent CLI exit 2))
- RFC-0011-c §9.4 Output Envelope (`OutputEnvelope<T>` wrapper, `schema_version = 4`)
- RFC-0011-c §Security (confirmation gate, redaction patterns)
- RFC-0011-a §7.4 Substrate [ADD] signatures (audit log append substrate)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why gate

No release gate at mission landing. The `agent destroy` subcommand is substrate-only and depends on `0011-c-agent-create-subcommand` for the clap root wiring. The audit log append depends on RFC-0011-a substrate; if RFC-0011-a is not yet Accepted, the subcommand ships as a stub emitting `AuditSubstrateNotReady` (exit 52). **Mission completion requires RFC-0011-a Accepted.**

## Substrate Gap (hard-checked 2026-09-11)

Two-blocker substrate verification:

1. **State machine blocker** — `octo_wallet::transition_agent` with
   `Terminated` target DOES NOT EXIST in `crates/octo-wallet/src/`
   (verified via `grep -rE "pub (fn|async fn) " crates/octo-wallet/src/`).
   Sub-step 3 cannot dispatch the destroy transition.

2. **Audit append blocker** — `octo-audit` and `octo-audit-core`
   expose `AppendOnlyAuditSink` trait + `AuditEvent`/`AuditEventKind`
   enums (Layer A frozen per RFC-0012) but DO NOT expose a public
   `append_event` / `append_audit` function callable from CLI. The
   `OctoCliAuditAppend` facade layer required by RFC-0011-a §7.4
   Substrate [ADD] signatures is unbuilt.

**Unblock path:**

- Substrate #1: add `pub fn transition_agent` (small additive, ~30 LoC)
  to `crates/octo-wallet/src/agent.rs` (shared landing with 0011-c
  run + attach missions).
- Substrate #2: file RFC-0011-a-substrate (settlement receipt append
  surface) — separate substrate RFC cycle, NOT a closure exercise.
  Mission YAML frontmatter `v: "1.0"` should bump to `v: "1.1"`
  once substrate RFC is authored.

**Mission completion requires RFC-0011-a Accepted** per YAML frontmatter
`release_gate`. RFC-0011-a status MUST be `Accepted` not `Draft` for
this mission to land. The mission ships as a stub emitting
`AuditSubstrateNotReady` (exit 52) per §Backward compat until both
substrate blockers resolve.

**Implementation cannot proceed** until both substrate blockers land.
Mission remains `Claimed` per [[memory-is-never-status-ground-truth]].

## Claimant

@unassigned
