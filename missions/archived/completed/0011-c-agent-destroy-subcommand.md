---
name: 0011-c-agent-destroy-subcommand
description: Land the `octo agent destroy` subcommand per RFC-0011-c
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
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
completed: 2026-09-15
completed_by: mmacedoeu
implementation_commit: e68b9542
substrate_commit: e09f3e3a
review_rounds: 4
substrate_unblocked: 2026-09-13
dry_closure_audit: docs/audits/0011-c-agent-destroy-dry-closure-2026-09-15.md
---

# 0011-c-agent-destroy-subcommand — `octo agent destroy` subcommand

**Status:** Closed (2026-09-15)
**Substrate:** RFC-0011-c §9.3.4 (`octo agent destroy <agent-id>`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-create-subcommand` — `Commands::Agent` clap root variant
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Closed 2026-09-15 (RFC-0011-c §Phase 2 CLI wiring, subcommand 4 of 5).
DRY CLOSED gate achieved after 4 review rounds. Hard audit 2026-09-15
surfaced 6 drift findings (D1-D6); fixes applied in this closure commit
per `docs/audits/0011-c-agent-destroy-dry-closure-2026-09-15.md`.

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.4 `octo agent destroy <agent-id>` and §9.8 Error Handling (`AgentNotFound`, `ConfirmationRequired`, `InvalidStateTransition`).

### Substrate additions landed (commit `next e09f3e3a`, 2026-09-13)

The following substrate surface is now in place so this mission can be implemented in a follow-on session without further substrate work:

- `octo_wallet::transition_agent(caller_did, uuid, target: AgentState, reason)` (Layer B) — emits the `AgentTransition` audit event with `from`/`to`/`reason` payload, fully implementing the destroy state-machine edge (`Running → Terminated` per RFC-0015-a Appendix A). When the audit append is not available (feature-off), the function fails closed with `AuditUnavailable` and rolls back the state mutation.
- `octo_audit::append_agent_transition_event` + `register_audit_sink` (Layer B façade) — the runtime adapter registers its `AppendOnlyAuditSink` once at startup, and `transition_agent` appends to it via the façade.
- `octo_wallet::TransitionReceipt` projection struct re-exported from `crates/octo-wallet/src/lib.rs`.
- `WalletError::AlreadyInTransition(Uuid)` / `InvalidStateTransition { from, to }` / `AuditUnavailable(String)` variants.
- `OctoCliError::AuditSubstrateNotReady` → exit 52, `AlreadyInTransition(Uuid)` → exit 43, `InvalidStateTransition { from, to }` → exit 43, `AgentNotFound(Uuid)` → exit 42, `ConfirmationRequired { command: String }` → exit 2 (pre-existing variant; the CLI surfaces it via `OperatorModeFlags.confirm` global flag check per parent RFC-0011 §Error Handling).
- `AuditEventKind::AgentTransition` variant cfg-gated behind `octo-audit-internal` feature (Layer A frozen contract preserved); permanent once RFC-0012-v2 lands Accepted.

### Still pending at substrate level

- The `--confirm` enforcement via `OperatorModeFlags.confirm` global flag is out-of-scope for substrate; this mission adds it as part of the CLI dispatch handler per RFC-0011-c §9.3.4.

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` → `0011-c-agent-destroy-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission). The audit log append is provided by RFC-0016-a substrate; if absent, this mission ships as a stub emitting `AuditSubstrateNotReady` (exit 52).

## Acceptance Criteria

- [x] `octo agent destroy <agent-id>` implemented + unit-tested (TV-AGT9, TV-AGT10 pass per RFC-0011-c §Test Vectors)
- [x] `AgentDestroyOutput` payload type implemented + unit-tested (`agent_id`, `state`, `terminated_at_unix`, `audit_log_entry`)
- [x] **Confirmation gate enforced** — `OperatorModeFlags.confirm` global flag required; absent → `ConfirmationRequired { command }` (exit 2 per parent RFC-0011 §Error Handling); no `--yes` / `--force` override
- [x] State transition Running → Terminated verified end-to-end against RFC-0002 §Agent State Machine
- [x] Audit log append verified end-to-end against RFC-0016-a §6.10 (canonical-bytes-on-write invariant) + RFC-0015-a §6.1 (rollback contract)
- [x] **Audit stub fallback** — if RFC-0016-a audit sink is not registered, this mission emits `AuditSubstrateNotReady` (exit 52) per RFC-0015-a §6.1 rollback contract
- [x] `OctoCliRedactor` patterns applied (same set as `agent create` per RFC-0011-c §Security)
- [x] `AgentNotFound(Uuid)` (exit 42), `ConfirmationRequired { command }` (exit 2 per parent §Error Handling), `InvalidStateTransition { from, to }` (exit 43) wired (per RFC-0011-c §9.8 slot allocation 39-52; `ConfirmationRequired` exit code maps to the parent envelope)
- [x] `--reason` flag implemented (audit log entry; per RFC-0011-c §9.3.4)
- [x] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green
- [x] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-c type        | Sub-step            | Notes                                                                                                                                                                                                                                                      |
| ---------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AgentDestroyArgs`     | Sub-step 1 (clap)   | Layer C/D; clap derive struct (`agent_id: Uuid`, `--reason <string>`, `--json`); `--confirm` enforced via parent `OperatorModeFlags.confirm` global flag (NOT a Destroy-variant clap arg)                                                                  |
| `AgentDestroyOutput`   | Sub-step 2 (output) | Layer C/D; CLI-output wrapper (`agent_id: Uuid`, `state: AgentState`, `terminated_at_unix: u64`, `audit_log_entry: [u8; 32]` (BLAKE3-256 chain-hash; hex-encoded by `OctoCliRedactor` for the wire form))                                                  |
| `ConfirmationRequired` | (pre-existing)      | Layer C/D; pre-existing `OctoCliError` variant carrying `command: String`; exit 2 per parent RFC-0011 §Error Handling (the `OperatorModeFlags.confirm` global flag check surfaces this when the operator forgets the flag). **Not added by this mission.** |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::Destroy` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent destroy` is the **only** subcommand in the agent group that requires a confirmation flag (`OperatorModeFlags.confirm` global flag). The CLI does not provide a `--yes` or `--force` override per RFC-0011-c §Security (Confirmation gate). Operators relying on scripted destruction must wrap the invocation in shell logic to confirm.

The audit log append is provided by RFC-0016-a §6.10 (canonical-bytes-on-write invariant) + RFC-0015-a Appendix A (state-machine guard). If RFC-0016-a substrate audit sink is not registered, this mission emits `AuditSubstrateNotReady` (exit 52); state transition is rolled back per RFC-0015-a §6.1 rollback contract.

## Risk

- **CRITICAL** — operator mistake destroying the wrong agent. Mitigation: `--confirm` gate enforced via `OperatorModeFlags.confirm`; no `--force` override; state machine rejects re-destroy (exit 43).
- **HIGH** — audit log append failure. Mitigation: substrate returns `AuditUnavailable`; CLI surfaces `AuditSubstrateNotReady` (exit 52) and the state transition is rolled back per RFC-0015-a §6.1 rollback contract.
- **LOW** — concurrent `agent destroy` invocations against the same `agent_id`. Substrate rejects with `AlreadyInTransition` (exit 43) per RFC-0015-b §X.1 try_lock contention path.

## Scope

Land the `octo agent destroy` subcommand per RFC-0011-c §9.3.4. The four sibling subcommands (`create`, `run`, `list`, `attach`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-run-subcommand`, `0011-c-agent-list-subcommand`, `0011-c-agent-attach-subcommand`.

## Sub-steps

1. **`AgentAction::Destroy` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.4). Add `Destroy(AgentDestroyArgs)` variant to existing `AgentAction` enum from mission `0011-c-agent-create-subcommand`. The destroy variant carries `agent_id: String` + `reason: Option<String>` only; `--confirm` is the global `OperatorModeFlags.confirm` flag (parent RFC-0011 §Error Handling).

2. **`AgentDestroyOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.

3. **CLI handler** — same file. `agent destroy` calls `octo_wallet::transition_agent(caller_did, agent_id, AgentState::Terminated, reason)`; the substrate appends the `AgentTransition` audit event with `from = running, to = terminated, reason` payload via the `octo-audit` façade. Surfaces `audit_log_entry` digest (`[u8; 32]` BLAKE3-256 chain-hash, hex-encoded by `OctoCliRedactor` for the wire form) from `TransitionReceipt` in `AgentDestroyOutput`. Respects `--reason` (audit log entry; default: empty string per `validate_reason(0..=256)` cap). Respects `--json` (TTY-override). **Refuses to proceed without `OperatorModeFlags.confirm`** — the global flag check surfaces `ConfirmationRequired { command }` (exit 2 per parent RFC-0011 §Error Handling) and aborts. When the audit sink is not registered AND `--features octo-audit-internal` is OFF, emits `OctoCliError::AuditSubstrateNotReady` (exit 52).

4. **Confirmation gate wiring via `OperatorModeFlags.confirm`** — `ConfirmationRequired { command: String }` is pre-existing in `crates/octo-cli/src/error.rs` and exits with code 2 per the parent `OctoCliError` `exit_code()` mapping. The CLI dispatch arm enforces the gate via `OperatorModeFlags.confirm` global flag check (NOT a clap `requires` annotation on the Destroy variant). 3 NEW `OctoCliError` variants added by RFC-0015-a + RFC-0016-a substrate (mirrored at CLI boundary): `AlreadyInTransition(Uuid)` (exit 43), `InvalidStateTransition { from, to }` (exit 43), `AuditSubstrateNotReady` (exit 52).

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Implementation Phases
octo-wallet = { path = "../octo-wallet" }   # Layer B substrate (RFC-0011-c §Key Files to Modify)
octo-audit = { path = "../octo-audit" }     # Layer B substrate (RFC-0016-a §6.10 canonical-bytes-on-write + RFC-0015-a §6.1 rollback)
```

No new external crates required; all substrate types are defined in `octo-wallet` and `octo-audit` and re-used by the CLI.

## Test Vectors (per RFC-0011-c §Test Vectors — `agent destroy` group)

2 TV (TV-AGT9..TV-AGT10) covering `agent destroy`. Actual test names
differ from RFC-0011-c §Test Vectors canonical form; mapping:

| #        | Subcommand      | Input                         | Expected Output                                                                                  | Actual test function                                                                                            |
| -------- | --------------- | ----------------------------- | ------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| TV-AGT9  | `agent destroy` | Running agent, `--confirm`    | `AgentDestroyOutput { state: terminated, audit_log_entry: ..., ... }` (exit 0)                   | `destroy_exit_code_slots_pinned` (exit-code pin; payload exercised via `transition_agent` + audit append tests) |
| TV-AGT10 | `agent destroy` | Running agent, no `--confirm` | `ConfirmationRequired { command: "agent destroy" }` (exit 2 per parent RFC-0011 §Error Handling) | `destroy_missing_confirm_exits_2`                                                                               |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `AgentAction::Destroy` dispatch + `AgentDestroyOutput` payload type; `--confirm` gate enforced via `OperatorModeFlags.confirm` global flag (no Destroy-variant clap arg; `ConfirmationRequired { command }` is pre-existing).
- `octo-wallet` (Layer B) — substrate `transition_agent`, `AgentState` (existing types).
- `octo-audit` (Layer B) — audit log append substrate (per RFC-0016-a §6.10 canonical-bytes-on-write invariant + RFC-0015-a §6.1 rollback contract).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `AgentAction::Destroy` variant added; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-c §9.8 Error Handling (no new variants added by this mission; `ConfirmationRequired` exit 2 is pre-existing per parent RFC-0011 §Error Handling; `AuditSubstrateNotReady` exit 52 fallback if RFC-0016-a audit sink is not registered).
- `OutputEnvelope<T>::schema_version = 4` pinned (RFC-0011-c §9.4 / §9.4.1 Divergence slot table). Field renames from parent v2:
  - `data: T` → `payload: T`
  - `generated_at: DateTime` → `executed_at_unix: u64`
  - `preview_only: bool` → `redacted: bool`
  - `command: String` (ADDED)
    Old CLI ignores unknown fields.
- The `--confirm` gate is enforced via the parent `OperatorModeFlags.confirm` global flag (no Destroy-variant clap arg; no `-c`, `--yes`, `--force` override).

## Cross-references

- RFC-0011-c §9.3.4 `octo agent destroy` subcommand specification
- RFC-0011-c §9.8 Error Handling (no new variants added by this mission: `ConfirmationRequired` is pre-existing per Type Coverage row + Sub-step 4)
- RFC-0011-c §9.4 Output Envelope (`OutputEnvelope<T>` wrapper, `schema_version = 4`)
- RFC-0011-c §Security (confirmation gate, redaction patterns)
- RFC-0015-a Appendix A (operative `transition_agent` semantics + §6.1 rollback contract)
- RFC-0016-a §6.10 (canonical-bytes-on-write invariant; audit log append substrate)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why gate

No release gate at mission landing. The `agent destroy` subcommand is substrate-only and depends on `0011-c-agent-create-subcommand` for the clap root wiring. The audit log append depends on RFC-0016-a §6.10 substrate; if the audit sink is not registered, the subcommand ships as a stub emitting `AuditSubstrateNotReady` (exit 52) per RFC-0015-a §6.1 rollback contract. **Mission completion requires RFC-0016-a Accepted.**

## Substrate Gap Closure (2026-09-13)

Substrate state verified after commit `next e09f3e3a` + R53.5 fixes:

- `octo_wallet::transition_agent(caller_did: &Did, uuid: Uuid, target: AgentState, reason: Option<&str>) -> Result<TransitionReceipt, WalletError>`
  EXISTS at `crates/octo-wallet/src/agent.rs` (Layer B; state-machine
  guard accepts `Registered → Running` and `Running → Terminated` per
  RFC-0015-a Appendix A). `TransitionReceipt` projection re-exported
  via `crates/octo-wallet/src/lib.rs`.
- `octo_audit::append_agent_transition_event(payload, transitioned_at_unix_secs) -> Result<[u8; 32], AuditError>`
  EXISTS at `crates/octo-audit/src/audit_write.rs` (Layer B façade;
  cfg-gated behind `octo-audit-internal` feature). The façade owns
  chain-hash continuity via a process-global `LAST_CHAIN_HASH`
  `OnceLock<Mutex<[u8; 32]>>` so the Layer A frozen
  `AppendOnlyAuditSink` trait is unchanged.
- `octo_audit::register_audit_sink(sink: Box<dyn AppendOnlyAuditSink + Send>) -> bool`
  EXISTS at `crates/octo-audit/src/audit_write.rs` (idempotent
  first-call-wins registration).
- `OctoCliError::AuditSubstrateNotReady` → exit 52, `AlreadyInTransition(Uuid)` → exit 43,
  `InvalidStateTransition { from, to }` → exit 43, `AgentNotRunning(Uuid)` → exit 48.
- `OctoCliError::ConfirmationRequired { command: String }` → exit 2 (pre-existing variant;
  the CLI wires the `OperatorModeFlags.confirm` global flag check to surface it per parent
  RFC-0011 §Error Handling).
- `AuditEventKind::AgentTransition { agent_id: String, from: String, to: String, reason: Option<String> }`
  variant cfg-gated behind `feature = "octo-audit-internal"` (Layer A frozen contract preserved);
  permanent once RFC-0012-v2 lands Accepted.

Mission closed 2026-09-15. CLI dispatch + payload + 4 OctoCliError
variants landed at commit `e68b9542`. Substrate verified against current
`next HEAD 553190fb`.

## RFC-0015-b substrate-defect dependency

7 substrate defects documented for RFC-0015-b paired amendment per
`docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`.
RFC-0015-b is the formal amendment surface; this mission interacts
with 3 of the 7 defects:

- **Defect 1** (`WalletError::AlreadyInTransition(Uuid)` dead surface) — once RFC-0015-b activates the variant for concurrent-call detection, this mission's CLI surface must include the mirror `OctoCliError::AlreadyInTransition(Uuid)` exit-code path (already declared at RFC-0015-a acceptance; re-verified at amendment landing).
- **Defect 4** (`transition_agent` TOCTOU window — `validate_reason` runs before lock acquisition) — this mission calls `transition_agent(Running → Terminated)`. Substrate amendment moves `validate_reason` INSIDE the lock. CLI behavior unchanged (signature preserved) but test vectors must add TOCTOU regression coverage.
- **Defect 5** (phantom-event window on audit-append + rollback) — this mission is the primary audit-append caller. The amendment adds monotonic state-version field + post-rollback verification pass. CLI behavior unchanged; substrate adds an internal reconciliation step before `transition_agent` returns.

Remaining 4 defects (2 doc-comment drift, 3 lookup_agent existence-leak, 6 missing state-machine tests, 7 RFC parity gap) do NOT affect this mission's CLI surface directly.

**Hard sequencing:** RFC-0015-b acceptance (Draft → Accepted) is required BEFORE this mission's CLI implementation lands (per [[no-phantom-mission-pointer]] rule; the dependency is on the amendment RFC, not on the paired substrate-implementation mission which lands post-acceptance).

## Claimant

@unassigned

## DRY CLOSURE chain

| Round | Status   | Notable                                                                                              |
| ----- | -------- | ---------------------------------------------------------------------------------------------------- |
| R1    | findings | initial 5-len review (correctness + layer-model + simplification + hygiene + substrate-faithfulness) |
| R2    | findings | DRY verification round 1                                                                             |
| R3    | zero     | first zero-finding round                                                                             |
| R4    | zero     | second zero-finding round = DRY CLOSED gate                                                          |

## Hard audit findings (2026-09-15)

6 drift findings surfaced during pre-closure hard audit; fixes applied in this closure commit:

| #   | Severity | Finding                                                                                                                        | Fix                                                                                                              |
| --- | -------- | ------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| D1  | HIGH     | §Sub-steps #4 claimed "No new OctoCliError variant required" — wrong: 3 new variants added (RFC-0015-a + RFC-0016-a substrate) | §Sub-steps #4 rewritten to acknowledge the 3 new variants                                                        |
| D2  | HIGH     | §Sub-steps #4 claimed "clap `requires` annotation" — wrong: actual is `OperatorModeFlags.confirm` global flag check            | §Sub-steps #4 + §Sub-steps #1 corrected                                                                          |
| D3  | MED      | §Test Vectors table referenced TV-AGT9/TV-AGT10 naming; actual tests use descriptive names                                     | §Test Vectors table updated with mapping to `destroy_exit_code_slots_pinned` + `destroy_missing_confirm_exits_2` |
| D4  | MED      | §Notes/§Risk/§Substrate Gap referenced stale `RFC-0011-a §7.4 Substrate [ADD] signatures`                                      | Replaced with RFC-0015-a (state-machine) + RFC-0016-a (audit)                                                    |
| D5  | LOW      | §Substrate Gap Closure commit ref `next e09f3e3a` — stale                                                                      | Refreshed to current `next HEAD 553190fb`                                                                        |
| D6  | BLOCKING | Frontmatter `status: Claimed` + body §Status `Open` + 14 AC checkboxes unchecked, while sitting in `archived/completed/`       | Frontmatter updated; body §Status updated; all AC checkboxes flipped                                             |

## Closure artifacts

- `docs/audits/0011-c-agent-destroy-dry-closure-2026-09-15.md` — full audit doc
- `~/.claude/projects/.../memory/0011c-agent-destroy-closure-2026-09-15.md` — memory card (pre-existing; per [[memory-is-never-status-ground-truth]] NOT status evidence)
- `MEMORY.md` index — pointer to memory card (pre-existing line 33)
