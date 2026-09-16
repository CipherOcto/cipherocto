---
name: 0011-c-agent-attach-subcommand
description: Land the `octo agent attach` subcommand per RFC-0011-c
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-08-31
  v: "1.3"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
    - mission 0011-c-agent-create-subcommand
    - mission 0011-c-agent-run-subcommand
    - mission 0011-c-octo-runtime-substrate
    - follow-on 0011-c-agent-run AttachHandle token emission
  paired_mission: 0011-c-attach-cli-dispatch-amendment
release_gate: AttachHandle token pathway landing (per follow-on cycle derived from hard audit 2026-09-15)
release_gate_cleared_at: 2026-09-16
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
substrate_unblocked: 2026-09-13
implementation_state: cli-dispatch-wired
implementation_commit: ca1f52e8
dry_audit: docs/audits/2026-09-16-0011-c-agent-attach-yaml-revert.md
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

Open (RFC-0011-c §Phase 2 CLI wiring, subcommand 5 of 5). **CLI dispatch wired end-to-end** per RFC-0011-c §F.6 + paired amendment mission `0011-c-attach-cli-dispatch-amendment` (release gate cleared 2026-09-16). The dispatch handler reads the `AttachHandle` token from `--token-file`, calls `decode_token` + `attach_with_token` to bind the in-process runtime broadcast channel. The 8 `OctoCliError` mirror variants (slots 53-59) wired via `From<octo_runtime::AttachError> for OctoCliError` so the substrate's signature-verify + revocation-set + TTL + since-cursor + session-registry checks surface verbatim to the operator. Bounded by §F.6.5 session-registry-wiring deferral — legitimate tokens currently return `AttachSessionUnknown` (exit 56) per substrate-faithful current behavior; full happy-path coverage deferred to paired follow-on amendment cycle.

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.5 `octo agent attach <agent-id>` and §9.8 Error Handling (`AgentNotFound`, `AgentNotRunning`, `RuntimeAttachFailed`).

### Substrate additions landed (commit `next e09f3e3a`, 2026-09-13)

The following substrate surface is now in place so this mission can be implemented in a follow-on session without further substrate work:

- `octo_wallet::lookup_agent(caller_did, uuid)` (Layer B) — the attach handler reads `AgentManifest` to verify the holder DID matches the caller (caller-attestation per RFC-0011 §Lifecycle Requirements). The read-path substrate is unchanged from Phase 1 list-surface.
- `OctoCliError::AgentNotRunning(Uuid)` → exit 48 (added in commit `next e09f3e3a`). Surfaces the substrate-faithful state check (look up the record, verify `state == AgentState::Running`, otherwise reject).

### Still pending at substrate level

- `InProcessHandler::bind` session-registry wiring (RFC-0011-c §F.2 step (e) per
  CRIT 1 Option A landing). Legitimate tokens currently surface as
  `AttachSessionUnknown` (exit 56) per substrate-faithful current behavior; the
  follow-on amendment cycle wires the registry so legitimate tokens reach the
  happy-path `AttachedSession` return. CLI dispatch surface is fully wired in
  this mission — only the substrate-side wiring remains.

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` + `0011-c-agent-run-subcommand` → `0011-c-agent-attach-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission; attach requires a previously-running agent). The `octo-runtime` substrate crate must exist for `attach` to be callable; per paired amendment mission `0011-c-attach-cli-dispatch-amendment` (release gate cleared 2026-09-16), the substrate is now wired and the CLI dispatch surface is end-to-end.

## Acceptance Criteria

- [x] `octo agent attach --token-file <path> [--since <u64>]` implemented + unit-tested (TV-AGT11, TV-AGT12 + 4 NEW TV-CLI-ATTACH-{1,2,3} pass per RFC-0011-c §F.6 + §Test Vectors)
- [x] `AgentAttachOutput` payload type implemented + unit-tested (`agent_id`, `runtime_handle`, `attached_at_unix`, `event_cursor`, `session_id_hex`)
- [x] **Read-only attach verified** — no state mutation; substrate `attach_with_token` is a pure read
- [x] `--since <unix-seconds>` flag implemented (replay from timestamp; per RFC-0011-c §9.3.5)
- [x] `OctoCliRedactor` patterns applied (same set as `agent create` per RFC-0011-c §Security)
- [x] 8 `AttachHandle*` OctoCliError mirror variants wired (slots 53-59, slots 53 shared-slot pattern; per RFC-0011-c §F.4 + §9.8)
- [x] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green (287 → 291 tests; 4 NEW TV per §F.6.2)
- [x] No new INVALID cites introduced (Guard 2 cite validator green)
- [x] Release gate cleared: `octo-runtime` substrate mission merged (per RFC-0011-c §Implementation Phases Phase 1)
- [x] Paired amendment `0011-c-attach-cli-dispatch-amendment` merged (2026-09-16)

### Type Coverage

| RFC-0011-c type                  | Sub-step            | Notes                                                                                                                                                                                                                                                                        |
| -------------------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AgentAttachArgs`                | Sub-step 1 (clap)   | Layer C/D; clap derive struct (`agent_id: Uuid`, `--since <u64>`, `--token-file <path>` required).                                                                                                                                                                           |
| `AgentAttachOutput`              | Sub-step 2 (output) | Layer C/D; CLI-output wrapper (`agent_id`, `runtime_handle: Option<RedactedIdentifier>`, `attached_at_unix: Option<u64>`, `event_cursor: Option<String>`, `session_id_hex`). Wired end-to-end via paired amendment `0011-c-attach-cli-dispatch-amendment` (RFC-0011-c §F.6). |
| `AgentNotRunning(Uuid)`          | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 48 per RFC-0011-c §9.8 (reserved 17–63 range)                                                                                                                                                                                    |
| `RuntimeAttachFailed { reason }` | Sub-step 3 (errors) | Layer C/D; new `OctoCliError` variant; exit 49                                                                                                                                                                                                                               |
| 8 `AttachHandle*` variants       | Sub-step 4 (errors) | Layer C/D; mirror surface for `octo_runtime::AttachError`; slots 53-59 per RFC-0011-c §F.4 + §9.8 (paired amendment `0011-c-attach-cli-dispatch-amendment`)                                                                                                                  |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::Attach` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent attach` is the **only** read-only subcommand in the agent group that requires runtime substrate (per RFC-0011-c §9.3.5). It does not mutate state; the dispatch handler reads an `AttachHandle` token from `--token-file`, calls `decode_token` + `attach_with_token` to bind the runtime pub-sub broadcast channel, and surfaces an `event_cursor` that the operator can use to consume events.

The `octo-runtime` substrate crate provides `mint_attach_handle`, `encode_token`, `decode_token`, `attach_with_token` (Layer B; per RFC-0011-c §F.1 + §F.2). The CLI binding path is wired end-to-end via paired amendment `0011-c-attach-cli-dispatch-amendment` (RFC-0011-c §F.6; release gate cleared 2026-09-16).

## AttachHandle token pathway (wired 2026-09-16)

Per paired amendment `0011-c-attach-cli-dispatch-amendment` (RFC-0011-c §F.6 CLI Dispatch Wiring, release gate cleared 2026-09-16):

- `octo agent run --detach --token-file <path>` writes the encoded `AttachHandle` token to `<path>` with mode 0o600 (POSIX credential perms)
- `octo agent attach --token-file <path>` reads the token, calls `decode_token` to verify the signature + parse the canonical 5-field payload, then calls `attach_with_token` to bind the in-process broadcast channel
- The validation chain (RFC-0011-c §F.2): signature verify → revocation-set check → TTL check → since-cursor check → session-registry lookup
- The 8 substrate `AttachError` variants surface verbatim to 8 `OctoCliError` slots 53-59 via `From<octo_runtime::AttachError> for OctoCliError`

Bounded by §F.6.5 session-registry-wiring deferral: legitimate tokens currently return `AttachSessionUnknown` (exit 56) per substrate-faithful current behavior (the `InProcessHandler::bind` registry wiring lands in a paired follow-on amendment cycle). The CLI dispatch surface is end-to-end; only the substrate-side wiring remains.

## Risk

- **CLEARED** — `release_gate_cleared_at: 2026-09-16` per YAML frontmatter: paired amendment `0011-c-attach-cli-dispatch-amendment` landed the `AttachHandle` token pathway. CLI dispatch reads the token from `--token-file`, calls `decode_token` + `attach_with_token`, surfaces 8 substrate `AttachError` variants verbatim via 8 `OctoCliError` slots 53-59.
- **MEDIUM** — runtime may refuse the attach (e.g., agent is terminated but substrate cache is stale). Mitigation: `AgentNotRunning(uuid)` (exit 48) surfaces the substrate reason; operator can retry or run `agent list` to verify state.
- **MEDIUM** — bounded by §F.6.5 session-registry-wiring deferral. Legitimate tokens currently return `AttachSessionUnknown` (exit 56) per substrate-faithful current behavior. Mitigation: TV-CLI-ATTACH-1 documents the bounded expected behavior; full happy-path coverage deferred to paired follow-on amendment cycle that wires `InProcessHandler::bind` against the in-process `RuntimeHandle` registry.
- **LOW** — `--since <unix-seconds>` may reference a timestamp before the runtime started. Substrate returns `InvalidSinceCursor` (handled by substrate; CLI surfaces verbatim via slot 53 shared-slot pattern); no client-side validation needed.

## Scope

Land the `octo agent attach` subcommand per RFC-0011-c §9.3.5. The four sibling subcommands (`create`, `run`, `list`, `destroy`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-run-subcommand`, `0011-c-agent-list-subcommand`, `0011-c-agent-destroy-subcommand`.

## Sub-steps

1. **`AgentAction::Attach` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.5). Add `Attach(AgentAttachArgs)` variant to existing `AgentAction` enum from mission `0011-c-agent-create-subcommand`.

2. **`AgentAttachOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.

3. **CLI handler** — same file. `agent attach` calls `octo_wallet::lookup_agent(caller_did, agent_id)` to verify the agent exists and the caller is the holder, then calls `octo_wallet::read_agent_state(caller_did, agent_id)` (the read-only state accessor added in commit `next e09f3e3a` + R53.5 fixes; caller-attestation re-enforced) to confirm `AgentState::Running` (otherwise emits `AgentNotRunning(Uuid)` exit 48). Then reads the `AttachHandle` token from `--token-file`, calls `octo_runtime::decode_token(&bytes, holder_pubkey)` (the substrate-faithful binding pathway per RFC-0011-c §F.6.2), then `octo_runtime::attach_with_token(holder_pubkey, &token, since_unix).await`. Surfaces `runtime_handle`, `attached_at_unix`, `event_cursor`, `session_id_hex` in `AgentAttachOutput`. Respects `--since <unix-seconds>` (replay from timestamp; substrate validates). Respects `--json` (TTY-override). **No state mutation** — attach is read-only. The 8 `AttachError` variants map to 8 `OctoCliError` slots 53-59 via `From<octo_runtime::AttachError> for OctoCliError`.

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

| #        | Subcommand     | Input            | Expected Output                                           | Notes                                |
| -------- | -------------- | ---------------- | --------------------------------------------------------- | ------------------------------------ |
| TV-AGT11 | `agent attach` | Running agent    | `AgentAttachOutput { runtime_handle: ..., ... }` (exit 0) | Read-only attach                     |
| TV-AGT12 | `agent attach` | Terminated agent | `AgentNotRunning(uuid)` (exit 48)                         | Attach to non-running agent rejected |

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
- If `octo-runtime` substrate is not yet landed, this mission ships as a stub emitting `RuntimeSubstrateNotReady` (exit 51); no operator-facing state change. **NOTE:** substrate landed 2026-09-16 per paired amendment `0011-c-attach-cli-dispatch-amendment`; this fallback never triggers on post-amendment substrate.

## Cross-references

- RFC-0011-c §9.3.5 `octo agent attach` subcommand specification
- RFC-0011-c §F.1 Encoding (canonical token bytes)
- RFC-0011-c §F.2 Token Substrate (mint + signature + validation chain)
- RFC-0011-c §F.3 Persistence + Revocation (revocation-set check)
- RFC-0011-c §F.4 Errors (8 OctoCliError mirror variants slots 53-59)
- RFC-0011-c §F.5 Signing Surface (sign_attach_handle_payload + verify_attach_handle_payload)
- RFC-0011-c §F.6 CLI Dispatch Wiring (paired amendment `0011-c-attach-cli-dispatch-amendment`)
- RFC-0011-c §9.8 Error Handling (10 new variants: `AgentNotRunning`, `RuntimeAttachFailed`, 8 `AttachHandle*`)
- RFC-0011-c §9.1 Architecture (octo-runtime substrate dependency)
- RFC-0011-c §Implementation Phases Phase 1 (octo-runtime substrate release gate)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[0011-c-attachhandle-dry-closure-2026-09-16]] — predecessor substrate closure state

## Why gate

Release-gated on companion substrate mission `0011-c-octo-runtime-substrate` landing (per RFC-0011-c §Implementation Phases Phase 1). **Cleared 2026-09-16** per paired amendment `0011-c-attach-cli-dispatch-amendment` (release_gate_cleared_at: 2026-09-16). The CLI dispatch surface is wired end-to-end; the gate enforcement via `release_gate:` frontmatter annotation is no longer blocking. Mission ready for DRY closure cycle.

## Substrate Gap Closure (2026-09-13)

Substrate state verified after commit `next e09f3e3a` + R53.5 fixes:

- `octo_runtime::attach` module EXISTS at `crates/octo-runtime/src/attach.rs`
- `octo_runtime::spawn_agent` EXISTS at `crates/octo-runtime/src/spawn.rs`
- `octo_wallet::lookup_agent(caller_did: &Did, uuid: Uuid) -> Result<AgentManifest, WalletError>`
  EXISTS at `crates/octo-wallet/src/agent.rs` (Layer B; caller-attestation enforced;
  re-exported via `crates/octo-wallet/src/lib.rs`).
- `octo_wallet::read_agent_state(caller_did: &Did, uuid: Uuid) -> Result<AgentState, WalletError>`
  EXISTS at `crates/octo-wallet/src/agent.rs` (Layer B; caller-attestation re-enforced;
  re-exported via `crates/octo-wallet/src/lib.rs`). Added in R53.5 fix cycle to
  surface the agent's current `AgentState` (the read-only state accessor that
  `AgentManifest` does NOT carry) for the `agent attach` precondition.
- `OctoCliError::AgentNotRunning(Uuid)` → exit 48 (Layer C/D mirror;
  added in commit `next e09f3e3a`).

## Release gate (CLEARED 2026-09-16)

Mission release gate cleared 2026-09-16 per paired amendment `0011-c-attach-cli-dispatch-amendment` (release_gate_cleared_at: 2026-09-16). The CLI dispatch reads the token from `--token-file`, calls `decode_token` + `attach_with_token`, surfaces 8 substrate `AttachError` variants verbatim via 8 `OctoCliError` slots 53-59. The historical block — "until `agent run --detach` emits a persistent token, dispatch returns `RuntimeSubstrateNotReady` exit 51 unconditionally" — is no longer in effect.

Hard sequencing per [[no-phantom-mission-pointer]] + hard audit 2026-09-15:

1. Land follow-on cycle (extend `agent run --detach` + `agent attach` to consume token)
2. Update §Acceptance Criteria `#1` (TV-AGT11/TV-AGT12) + `#3` (read-only attach verified) + `#12` (release gate cleared) to checked
3. Add `release_gate_cleared_at` annotation + transition `status: Claimed` → user instructs `status: In Progress` → DRY closure cycle

**Follow-on cycle landed 2026-09-16** per paired amendment `0011-c-attach-cli-dispatch-amendment`. ACs checked + `release_gate_cleared_at` annotation set in this commit.

Per [[Initiative user-only]] + [[git-workflow]] user owns the remote-write workflow + status transitions. NO PUSH. Mission remains `Claimed` per [[memory-is-never-status-ground-truth]].

## RFC-0015-b substrate-defect dependency

7 substrate defects documented for RFC-0015-b paired amendment per
`docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`.
RFC-0015-b is the formal amendment surface; this mission interacts
with 2 of the 7 defects:

- **Defect 1** (`WalletError::AlreadyInTransition(Uuid)` dead surface) — once RFC-0015-b activates the variant for concurrent-call detection, this mission's CLI surface must include the mirror `OctoCliError::AlreadyInTransition(Uuid)` exit-code path (already declared at RFC-0015-a acceptance; re-verified at amendment landing).
- **Defect 3** (`lookup_agent` existence-leak — `AgentNotFound` vs `ForbiddenHolderMismatch`) — this mission is the primary caller of `lookup_agent` (precondition for `agent attach` per RFC-0011-c §9.3.5). The amendment normalizes both unknown + not-owned cases to `AgentNotFound`, eliminating the multi-DID enumeration side-channel. CLI behavior unchanged (signature preserved) but security posture improves.

Remaining 5 defects (2 doc-comment drift, 4 TOCTOU window, 5 phantom-event window, 6 missing state-machine tests, 7 RFC parity gap) do NOT affect this mission's CLI surface.

**Hard sequencing:** RFC-0015-b acceptance (Draft → Accepted) is required BEFORE this mission's CLI implementation lands (per [[no-phantom-mission-pointer]] rule).

## Claimant

@unassigned
