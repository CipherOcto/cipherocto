# RFC-0011-c: `octo agent` Subcommands

## Status

Accepted (2026-08-31)

> **Amendment chain:** This document is Phase 3 of the RFC-0011 amendment
> chain. The parent (RFC-0011) covers identity, capability, and policy
> subcommands. Follow-on amendments cover audit (RFC-0011-a), reputation
> (RFC-0011-b), **agent lifecycle (RFC-0011-c, this document)**, role
> provisioning (RFC-0011-d), vault operations (RFC-0011-e), mesh
> operations (RFC-0011-f), and governance (RFC-0011-g).

> **Legacy numbering note:** RFC-0011-c was originally numbered as
> "RFC-0011-c" in the amendment chain enumeration (per the RFC-0011
> Status blockquote amendment enumeration). No number reassignment
> occurred during the chain — the letter
> suffix is the canonical designation. Prior interim drafts referenced
> this amendment informally as the "Phase 4 agent lifecycle" amendment;
> the canonical amendment name is now `octo agent` Subcommands.

## Authorship Note

Authored by `@cipherocto` and `@mmacedoeu` per the amendment chain enumerated in RFC-0011 Status header (audit, reputation, agent lifecycle, role provisioning, vault operations, mesh operations, governance). The Authorship Note placeholder is filled at promotion to Accepted.

## Summary

This RFC defines the `octo agent` subcommand group for the `octo` CLI:
operator-driven agent lifecycle operations. The group covers five
subcommands — `agent create`, `agent run`, `agent list`, `agent destroy`,
and `agent attach`. Each subcommand is a thin Layer-C/D operator UX
wrapper over the `octo-wallet` substrate (for state, manifest, and
capability registration) and the `octo-runtime` substrate (for spawn,
attach, and lifecycle dispatch). State transitions bind to the agent
state machine defined in RFC-0002 §Agent State Machine, and capability
validation follows RFC-0002 §Capability Validation (the 6-step
verification pipeline). Replay protection reuses RFC-0002 §Replay
Protection (`nonce: [u8;16]` per `AgentMessage` + `prev_message_hash:
Option<[u8;32]>` chain via `chain_store::verify_prev_hash`).

## Dependencies

**Requires:**

- RFC-0011 — `octo` CLI Substrate (parent RFC; provides
  `OutputEnvelope<T>`, `OctoCliError`, `OctoCliRedactor`, clap tree,
  exit-code table, and confirmation-flag matrix)
- RFC-0002 — Agent Manifest, Capability, and Lifecycle Substrate
  (canonical state machine, manifest wire form, 6-step capability
  validation, replay protection)

**Optional:**

- RFC-0957 — Macaroon Substrate (capability witness format the agent
  manifest references in `capability_root`)
- RFC-0009 — Identity Substrate (`active_did()` resolution, the
  authority under which every `agent create` registers a manifest)
- RFC-0011-b — Reputation Substrate (the optional `reputation_score`
  field surfaced by `agent list` and `agent attach` if present)

> **Dependency Validation Rules:**
>
> 1. Dependencies form a DAG (no cycles) — RFC-0011-c depends upward
>    only on substrate RFCs and the parent CLI RFC.
> 2. All "Requires" RFCs are listed as Accepted or Draft (RFC-0002 is
>    Accepted as of the RFC-0011-c authoring date; RFC-0011 is
>    Accepted).
> 3. Optional dependencies are documented separately; failure of any
>    optional dependency does not block the 5-subcommand surface.
> 4. `octo agent attach` is **conditionally dependent** on the
>    `octo-runtime` crate landing in Layer B; the subcommand ships as
>    a stub-with-error until `octo-runtime::attach` is implemented.

## Design Goals

| Goal | Target   | Metric                                                                                                                                                                                  |
| ---- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1   | `<200ms` | `octo agent list` wall-clock latency for an operator with 50 owned agents (cache-hit path)                                                                                              |
| G2   | `<300ms` | `octo agent create <manifest-path>` wall-clock latency (manifest parse + signature verify + substrate `register_agent` call)                                                            |
| G3   | `<150ms` | `octo agent run <agent-id>` warm-path wall-clock latency (state transition REGISTERED → BUSY + `spawn_agent` dispatch); cold path `<2s` over RPC                                        |
| G4   | `100%`   | State transitions reported by `octo agent run` / `octo agent destroy` MUST match RFC-0002 §Agent State Machine transitions byte-for-byte for the same `(agent_id, transition_op)` tuple |
| G5   | `0`      | Private-key material present in CLI process memory; manifest signing happens entirely inside HSM slot; CLI never sees the holder private key                                            |
| G6   | `>95%`   | Cache hit ratio for `agent list` over a 24-hour window for an active operator (5+ agent touches per day)                                                                                |

## Motivation

Operators who run the `octo` CLI today (post-RFC-0011 Phase 1+2) can
mint capabilities and inspect identities, but they **cannot create,
list, run, destroy, or attach to agents** from the operator
workstation. The agent lifecycle surface — manifest registration, state
transitions, runtime spawning, audit-grade destruction — is invisible
from the operator workstation. Substrate crates (`octo-wallet`,
`octo-runtime`) carry the logic; the CLI lacks the UX surface.

This amendment closes the operator-UX gap with five tightly-scoped
subcommands. It also deprecates the legacy `octo agent` stub command
that exists today (per RFC-0011 compatibility window; Status blockquote). The stub
emits `StaleStub` (exit code 65) starting at CLI v1.0 (warn-only)
and becomes a hard error in v1.1 (per §Compatibility below).

## Roles and Authorities

Three roles interact with the `octo agent` subcommands:

| Role              | Authority                                                                                                                                                                                                                    |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Operator          | Owns the workstation where `octo` runs; can `create`, `run`, `list`, `destroy`, `attach` against any agent whose `owner_did == active_did()`; cannot bypass `--confirm` for destroy                                          |
| Capability Holder | Holds the macaroon referenced by `capability_root` registration parameter (per §9.10 `register_agent` signature); receives state-transition notifications via the substrate pub-sub bus; never appears in CLI process memory |
| Auditor           | Read-only role; consumes `octo agent list` JSON output via `--json` for offline audit; cannot mutate state; cannot bypass `--confirm` for destroy                                                                            |

### Role/Authority Coverage Table

| Role                      | Identifier                     | Authority Scope                                                                                                                                                                                                              | Lifecycle                              | Source/Ref                                                     |
| ------------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | -------------------------------------------------------------- |
| Operator                  | `active_did()` resolution      | Owns the workstation where `octo` runs; can `create`, `run`, `list`, `destroy`, `attach` against any agent whose `owner_did == active_did()`; cannot bypass `--confirm` for destroy                                          | Per-CLI-invocation (no stateful actor) | RFC-0009 §Identity Struct                                      |
| Capability Holder         | macaroon holder (HSM slot)     | Holds the macaroon referenced by `capability_root` registration parameter (per §9.10 `register_agent` signature); receives state-transition notifications via the substrate pub-sub bus; never appears in CLI process memory | Long-lived (HSM-bound)                 | RFC-0957 §Algorithms                                           |
| Auditor                   | `octo agent list --json`       | Read-only role; consumes JSON output for offline audit; cannot mutate state; cannot bypass `--confirm` for destroy                                                                                                           | Per-audit-run (read-only)              | RFC-0011 §Output Envelope                                      |
| Substrate (authoritative) | `octo-wallet` / `octo-runtime` | Owns state machine, capability validation, replay protection; rejects unknown transitions / failed validations                                                                                                               | Long-lived (crate-level)               | RFC-0002 §Agent State Machine; RFC-0002 §Capability Validation |
| External (cross-node)     | remote `octo-*` nodes          | May receive state-transition notifications via substrate pub-sub bus                                                                                                                                                         | Long-lived (node-level)                | RFC-0002 §AgentMessage                                         |

## Performance Targets

| Subcommand                          | p95 target | Cold path | Notes                                                                                           |
| ----------------------------------- | ---------- | --------- | ----------------------------------------------------------------------------------------------- |
| `octo agent list`                   | `<200ms`   | `<500ms`  | Cache-hit figure; cold path includes substrate `list_owned_agents` RPC                          |
| `octo agent create <manifest-path>` | `<300ms`   | `<1s`     | Manifest parse + 6-step capability validate + HSM sign + `register_agent`                       |
| `octo agent run <agent-id>`         | `<150ms`   | `<2s`     | Warm-path state transition + `spawn_agent` dispatch; cold path includes runtime container start |
| `octo agent destroy <agent-id>`     | `<200ms`   | `<500ms`  | State transition ACTIVE → TERMINATED + audit log append; no RPC if cache-warm                   |
| `octo agent attach <agent-id>`      | `<100ms`   | `<300ms`  | Read-only handle resolution + attach to existing runtime; no state mutation                     |

## Implicit Assumptions Audit

| Assumption                                                                                                                                                                                                                                                      | Where Relied Upon                                                                               | Blast Radius if False                                                                                 | Mitigation / Status                                                |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| **A1:** RFC-0002 §Agent State Machine defines the transitions `REGISTERED → ACTIVE → BUSY → ACTIVE → SUSPENDED → ACTIVE`; terminal: `REJECTED` (from REGISTERED), `TERMINATED` (from ACTIVE), `RETIRED` (from SUSPENDED); the CLI never invents new transitions | §9.5 Agent State Machine Integration; §9.3 subcommand taxonomy                                  | Substrate rejects unknown transitions; `InvalidStateTransition` (exit 43) surfaces verbatim           | Validated by RFC-0002 §Agent State Machine                         |
| **A2:** RFC-0002 §Capability Validation (6-step) is sufficient to gate `agent create`; no additional checks are required at the CLI layer                                                                                                                       | §9.6 Capability Validation; §9.3.1 subcommand taxonomy                                          | Substrate returns `CapabilityValidationFailed` with the failing step; CLI surfaces verbatim (exit 40) | Substrate-authoritative; CLI never implements a parallel validator |
| **A3:** RFC-0002 §Replay Protection (`nonce: [u8;16]` per `AgentMessage` + `prev_message_hash: Option<[u8;32]>` chain via `chain_store::verify_prev_hash`) is the canonical replay-protection surface; the CLI MUST NOT add a second replay window              | §9.7 Replay Protection (separate from §9.6 Capability Validation 6-step)                        | Substrate enforces both; CLI does not maintain a parallel window                                      | Validated by RFC-0002 §Replay Protection                           |
| **A4:** RFC-0002 §Agent Manifest is the canonical wire form; the CLI does not parse or synthesize manifest bytes                                                                                                                                                | §9.6 step 1 (holder signature); §9.3.1 `manifest-path` clap arg                                 | CLI reads canonical bytes; substrate verifies over those bytes                                        | Validated by RFC-0002 §Agent Manifest                              |
| **A5:** `octo-runtime` substrate crate is provided by companion mission `0011-c-octo-runtime-substrate`; until it lands, `agent run`/`attach` emit `RuntimeSubstrateNotReady` (exit 51)                                                                         | §9.3.2 / §9.3.5 subcommand taxonomy; §9.10 substrate signatures; §Implementation Phases Phase 1 | Substrate not landed → subcommand ships as stub; no operator-facing state change                      | Gated via companion mission                                        |

## Specification

### 9.1 Architecture

```mermaid
graph TD
    Operator[Operator workstation] -->|invokes| OctoCli[octo CLI binary]
    OctoCli -->|Commands::Agent| ClapTree[clap root + Agent enum]
    ClapTree -->|dispatch| Handler[agent command handler]
    Handler -->|create| WalletSubstrate[octo-wallet substrate]
    Handler -->|run| RuntimeSubstrate[octo-runtime substrate]
    Handler -->|list| WalletSubstrate
    Handler -->|destroy| WalletSubstrate
    Handler -->|attach| RuntimeSubstrate
    WalletSubstrate -->|state| AgentStateMachine[RFC-0002 Agent State Machine]
    RuntimeSubstrate -->|state| AgentStateMachine
    WalletSubstrate -->|capability validate| CapValidation[RFC-0002 6-step Capability Validation]
    RuntimeSubstrate -->|replay protection| ReplayProt[RFC-0002 Replay Protection]
    Handler -->|output envelope| Envelope[OutputEnvelope T]
    Envelope -->|TTY| Pretty[Pretty table renderer]
    Envelope -->|not-TTY| Json[JSON renderer]
```

### 9.2 Binary Surface

The clap derive surface for the `octo agent` subcommand group is
defined as follows:

```rust
#[derive(Subcommand)]
pub enum Commands {
    // ... existing variants (Identity, Capability, Policy) ...

    /// Agent lifecycle operations (RFC-0011-c).
    #[command(subcommand)]
    Agent(AgentAction),
}

#[derive(Subcommand)]
pub enum AgentAction {
    /// Register a new agent manifest (RFC-0011-c §9.3.1).
    Create(AgentCreateArgs),

    /// Transition an agent to BUSY and spawn runtime (RFC-0011-c §9.3.2).
    Run(AgentRunArgs),

    /// List agents owned by the active DID (RFC-0011-c §9.3.3).
    List(AgentListArgs),

    /// Transition an agent to TERMINATED (RFC-0011-c §9.3.4).
    Destroy(AgentDestroyArgs),

    /// Attach to a running agent (read-only) (RFC-0011-c §9.3.5).
    Attach(AgentAttachArgs),
}
```

The `AgentAction` enum is **non-exhaustive** via `#[non_exhaustive]`
on the surrounding `Commands` enum (per RFC-0011 §Binary Surface).
Future amendments (if any beyond RFC-0011-a through RFC-0011-g) may add new variants
without breaking downstream parsers.

### 9.3 Subcommand Taxonomy

Five subcommands land in RFC-0011-c. Each is described in detail in
the subsections below.

#### 9.3.1 `octo agent create <manifest-path>`

| Aspect      | Detail                                                                                                                                                                      |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Clap args   | `manifest-path: PathBuf` (positional); `--capability-root <cid>` (optional override); `--label <string>` (optional; human-readable); `--json` (TTY-override)                |
| Output      | `AgentCreateOutput { agent_id: Uuid, state: AgentState, registered_at_unix: u64, manifest_digest: Hex32 }`                                                                  |
| Substrate   | `octo_wallet::register_agent(manifest, capability_root, active_did)`; validates against RFC-0002 §Capability Validation (6-step) and RFC-0002 §Replay Protection            |
| Errors      | `ManifestParseError` (exit 39), `CapabilityValidationFailed(usize)` (exit 40), `AgentAlreadyExists(Uuid)` (exit 41), `HsmUnavailable` (exit 5 per RFC-0011 §Error Handling) |
| Test vector | TV-AGT1, TV-AGT2, TV-AGT3                                                                                                                                                   |

#### 9.3.2 `octo agent run <agent-id>`

| Aspect      | Detail                                                                                                                             |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Clap args   | `agent_id: Uuid` (positional); `--detach` (default: detached); `--json` (TTY-override)                                             |
| Output      | `AgentRunOutput { agent_id: Uuid, state: AgentState, runtime_handle: String, spawned_at_unix: u64 }`                               |
| Substrate   | `octo_wallet::transition_agent(agent_id, Active)` then `octo_runtime::spawn_agent(agent_id, handle)`; spawns the runtime container |
| Errors      | `AgentNotFound(Uuid)` (exit 42), `InvalidStateTransition { from, to }` (exit 43), `RuntimeSpawnFailed { reason }` (exit 44)        |
| Test vector | TV-AGT4, TV-AGT5                                                                                                                   |

#### 9.3.3 `octo agent list`

| Aspect      | Detail                                                                                                                             |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Clap args   | `--state <AgentState>` (filter); `--chain-id <cid>` (filter); `--limit <u32>` (default 100); `--cursor <string>`; `--json`         |
| Output      | `AgentListOutput { agents: Vec<AgentSummary>, next_cursor: Option<String>, resolved_at_unix: u64 }`                                |
| Substrate   | `octo_wallet::list_owned_agents(filter, active_did)`; respects `--limit`/`--cursor` defensively (substrate validates `limit >= 1`) |
| Errors      | `InvalidLimit` (exit 45), `InvalidCursor` (exit 46)                                                                                |
| Test vector | TV-AGT6, TV-AGT7, TV-AGT8                                                                                                          |

#### 9.3.4 `octo agent destroy <agent-id>`

| Aspect      | Detail                                                                                                                                                                           |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Clap args   | `agent_id: Uuid` (positional); `--confirm` (required flag; absent → `ConfirmationRequired` exit 2 per RFC-0011 §Error Handling); `--reason <string>` (audit log entry); `--json` |
| Output      | `AgentDestroyOutput { agent_id: Uuid, state: AgentState, terminated_at_unix: u64, audit_log_entry: Hex32 }`                                                                      |
| Substrate   | `octo_wallet::transition_agent(agent_id, Terminated { reason })`; appends to audit log via RFC-0011-a substrate                                                                  |
| Errors      | `AgentNotFound(Uuid)` (exit 42), `ConfirmationRequired` (exit 2 per RFC-0011 §Error Handling), `InvalidStateTransition { from: Active, to: Terminated }` (exit 43)               |
| Test vector | TV-AGT9, TV-AGT10                                                                                                                                                                |

#### 9.3.5 `octo agent attach <agent-id>`

| Aspect      | Detail                                                                                                              |
| ----------- | ------------------------------------------------------------------------------------------------------------------- |
| Clap args   | `agent_id: Uuid` (positional); `--since <unix-seconds>` (optional; replay from timestamp); `--json`                 |
| Output      | `AgentAttachOutput { agent_id: Uuid, runtime_handle: String, attached_at_unix: u64, event_cursor: Option<String> }` |
| Substrate   | `octo_runtime::attach(handle, since)`; read-only handle; does not mutate state                                      |
| Errors      | `AgentNotFound(Uuid)` (exit 42), `AgentNotRunning(Uuid)` (exit 48), `RuntimeAttachFailed { reason }` (exit 49)      |
| Test vector | TV-AGT11                                                                                                            |

#### 9.3.6 `octo agent revoke-attach --session-id <HEX64>` — Layer C/D primitive per RFC-0011-c §Follow-on §F.3 (mirrors `octo_runtime::revoke_attach_token`)

| Aspect      | Detail                                                                                                                                                                                                                                                                 |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Clap args   | `--session-id <HEX64>` (required; hex-encoded `SessionId` from a previously-issued `AttachHandle`); `--confirm` (global flag per RFC-0011 §Compatibility; suppresses `ConfirmationRequired` exit 2 in `Human`/`Ci` mode; ignored in `Dev` mode)                        |
| Output      | `RevokeAttachOutput { session_id_hex: String, process_scoped: true }`                                                                                                                                                                                                  |
| Substrate   | `octo_runtime::revoke_attach_token(session_id)`; adds to in-memory revocation set keyed by `SessionId` (no token decode, no signature verify)                                                                                                                          |
| Errors      | `InvalidSessionIdHex { reason }` (exit 47; CLI-parse failure on the `--session-id` argument), `AuditorDenied { command }` / `ConfirmationRequired { command }` (exit 2; mode-gate per RFC-0011 §Compatibility), `RevocationError(String)` (exit 58; substrate failure) |
| Test vector | TV-AGT22                                                                                                                                                                                                                                                               |

### 9.4 Output Envelope

All five subcommands return values wrapped in the canonical
`OutputEnvelope<T>` defined in RFC-0011 §Output Envelope:

```rust
pub struct OutputEnvelope<T> {
    pub schema_version: u32,    // pinned to 4 for RFC-0011-c
    pub command: String,        // e.g., "octo agent create"
    pub executed_at_unix: u64,
    pub payload: T,
    pub redacted: bool,
}
```

`schema_version = 4` for all 5 subcommands. The renderer is TTY-aware:
pretty table on TTY, single-line JSON when stdout is not a TTY or
`--json` is set.

#### 9.4.1 Divergence from RFC-0011 §Output Envelope

This envelope is **not** field-identical with the parent's version 2.
The divergences are deliberate and load-bearing for the `agent`
subcommand group:

**Per-amendment `schema_version` slot table** (authoritative values per RFC-0011-e; c adopts -e field renames):

| Amendment      | `schema_version` | Reason                                                                                                                                                 |
| -------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| RFC-0011       | 2                | Parent envelope (baseline; adopted by -a, -b, -d)                                                                                                      |
| RFC-0011-a     | 2                | No envelope divergence (audit-only amendment)                                                                                                          |
| RFC-0011-b     | 2                | No envelope divergence (reputation-only amendment)                                                                                                     |
| RFC-0011-d     | 2                | No envelope divergence (role-provisioning amendment)                                                                                                   |
| **RFC-0011-e** | **3**            | **Renames `data`→`payload`; retypes `generated_at`→`executed_at_unix`; drops `exit_code` + `preview_only`; adds `command` + `redacted`**               |
| **RFC-0011-c** | **4**            | **Inherits all v3 field renames from -e (`payload`, `executed_at_unix`, `redacted`); consumes the same envelope wire format (no new field additions)** |
| RFC-0011-f     | 4..5             | Mesh ops: TBD at promotion                                                                                                                             |
| RFC-0011-g     | 6..7             | Governance: TBD at promotion                                                                                                                           |
| (future)       | 8+               | New amendments start at 8; consecutive slots reserve room for multi-version evolution                                                                  |

| Parent field (v2)        | RFC-0011-c              | Divergence                                                                            |
| ------------------------ | ----------------------- | ------------------------------------------------------------------------------------- |
| `data: T`                | `payload: T`            | **Renamed.** Breaking for consumers that read `data`                                  |
| `generated_at: DateTime` | `executed_at_unix: u64` | **Renamed + retyped** RFC 3339 string → `u64` unix seconds (single-clock determinism) |
| `preview_only: bool`     | `redacted: bool`        | **Renamed.** Renamed to surface `OctoCliRedactor` alteration                          |
| _(absent)_               | `command: String`       | **Added.** Identifies the invoking subcommand for log correlation                     |

Because fields are renamed and retyped, inheriting the parent's
`schema_version = 2` would misrepresent the payload to any consumer
that branches on the version. Version 4 is therefore a **breaking**
bump scoped to RFC-0011-c. Sibling amendments that do NOT diverge
from the parent envelope remain at version 2; a consumer MUST
read `schema_version` before reading any payload field.

### 9.5 Agent State Machine Integration

The CLI binds to the state machine in RFC-0002 §Agent State Machine.
The CLI is **purely an orchestration layer**; it never invents
transitions and never bypasses substrate validation. The mapping is:

| Subcommand      | Substrate call                                                                    | State transition enforced                                                                     |
| --------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `agent create`  | `octo_wallet::register_agent(...)`                                                | `(none) → REGISTERED`                                                                         |
| `agent run`     | `octo_wallet::transition_agent(id, Active)` then `octo_runtime::spawn_agent(...)` | `REGISTERED → ACTIVE` then `ACTIVE → BUSY`                                                    |
| `agent list`    | `octo_wallet::list_owned_agents(...)`                                             | no transition (read-only)                                                                     |
| `agent destroy` | `octo_wallet::transition_agent(id, Terminated)`                                   | `ACTIVE → TERMINATED` (terminal; SUSPENDED → ACTIVE → TERMINATED; BUSY → ACTIVE → TERMINATED) |
| `agent attach`  | `octo_runtime::attach(handle, since)`                                             | no transition (read-only)                                                                     |

The CLI does not maintain its own state cache; the substrate is the
single source of truth.

### 9.6 Capability Validation

Per RFC-0002 §Capability Validation, every `agent create` invocation
runs the 6-step pipeline:

1. **Holder signature** — verify `signature` against `holder_did`
   public_key per RFC-0957 §Algorithms
2. **HMAC chain** — walk caveat chain from capability root; verify
   each attenuation per RFC-0957 §Caveat DSL Extension
3. **Category check** — verify `category_id` matches the mission's
   required capability category (typed UUID)
4. **Operation claim check** — verify `operation_claims[].op_id`
   covers the mission's operations
5. **Reputation gate** — verify `score >= mission.min_score` per
   RFC-0011-b
6. **Mission limits** — verify `claimed_missions < max_missions` per
   RFC-0001 §Mission File Format

Any failure surfaces as `CapabilityValidationFailed(step: usize)`
with the failing step number (exit 40). The CLI does not implement
a parallel validation surface. Step descriptions follow RFC-0002
§Capability Validation substrate text; any divergence is additive
c-specific (e.g., manifest registration context).

### 9.7 Replay Protection

Per RFC-0002 §Replay Protection, every state-transitioning invocation
(`agent create`, `agent run`, `agent destroy`) is bound to a manifest
that carries replay protection primitives (`nonce: [u8;16]` per
`AgentMessage` + `prev_message_hash: Option<[u8;32]>` chain via
`chain_store::verify_prev_hash`). The substrate enforces both;
the CLI does not maintain a parallel window. Replay failures
surface through a dedicated typed-discriminator variant
`ReplayDetected { since_unix: u64, recorded_cursor: u64 }`
(exit 61; new slot per the amendment-chain shared-slot pattern,
distinct from `Internal(reason)` envelope exit 64) wired as the
additive follow-on amendment per [[cipherocto-design-principles]]
§Extension over enumeration. The substrate's `attach_with_token`
validation chain surfaces the variant when step (e) session-registry
detects a `since_unix` cursor at or behind the recorded session
cursor — i.e. the consumption guarantee is violated: the same
token has been bound once and is being replayed, or a different
token on the same session is being bound with a stale cursor.
The variant carries both the replayed `since_unix` cursor and the
recorded cursor it fell behind so the CLI dispatch can render
diagnostic context for operators without re-deriving from
event-stream state; this is the canonical substrate-faithful
mapping per [[substrate-faithfulness-verification]] discipline.

### 9.8 Error Handling

New `OctoCliError` variants are added (all `#[non_exhaustive]`
inheriting from RFC-0011 §Error Handling). **Slot allocation: 39-61**
(post -g's 35-38; 14 base plus 8 follow-on AttachHandle/AttachSession
per §Follow-on §F.4 mirror (8 unique slots, `InvalidSinceCursor`
shares slot 53 with `Expired` per amendment-chain shared-slot
pattern), plus 1 CLI dispatch `TokenMintSkipped` per §F.6.1,
plus 1 boundary-parse `InvalidSessionIdHex` per §9.3.6, plus 1
typed-discriminator `ReplayDetected` per §9.7 — totaling
**25 variants across 22 occupied slots** in the 23-slot range
39-61, 3 shared-slot pairings (43, 51, 53)). Renegotiation
is required if -h/i follow-on amendments claim earlier slots:

| Variant                                                 | Exit code   | Notes                                                                                                                                                                                                                                              |
| ------------------------------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ManifestParseError { path, reason }`                   | 39          | Manifest file unparseable; CLI never invents missing fields                                                                                                                                                                                        |
| `CapabilityValidationFailed(usize)`                     | 40          | step number from RFC-0002 §Capability Validation                                                                                                                                                                                                   |
| `AgentAlreadyExists(Uuid)`                              | 41          | Substrate rejects duplicate `agent_id`                                                                                                                                                                                                             |
| `AgentNotFound(Uuid)`                                   | 42          | Used by `agent run`/`agent list`/`agent destroy`/`agent attach`                                                                                                                                                                                    |
| `InvalidStateTransition { from, to }`                   | 43          | State machine rejects transition                                                                                                                                                                                                                   |
| `AlreadyInTransition(Uuid)`                             | 43 (shared) | RFC-0015-a §6.3 write-path — agent already mid-transition; shared slot with `InvalidStateTransition` per amendment-chain shared-slot pattern (state-machine write-path errors)                                                                     |
| `RuntimeSpawnFailed { reason }`                         | 44          | `agent run` runtime container spawn failure                                                                                                                                                                                                        |
| `InvalidLimit`                                          | 45          | `agent list --limit 0`                                                                                                                                                                                                                             |
| `InvalidCursor`                                         | 46          | `agent list --cursor <bad>`                                                                                                                                                                                                                        |
| `AgentNotRunning(Uuid)`                                 | 48          | `agent attach` against TERMINATED                                                                                                                                                                                                                  |
| `RuntimeAttachFailed { reason }`                        | 49          | `agent attach` runtime refused                                                                                                                                                                                                                     |
| `RuntimeSubstrateNotReady`                              | 51          | `octo-runtime` substrate not yet landed                                                                                                                                                                                                            |
| `GovernanceSubstrateError { reason }`                   | 51 (shared) | RFC-0011-g governance substrate not yet landed; shared slot with `RuntimeSubstrateNotReady` per amendment-chain shared-slot pattern (substrate-not-ready errors)                                                                                   |
| `AuditSubstrateNotReady`                                | 52          | `octo-audit` (RFC-0011-a) substrate not yet landed                                                                                                                                                                                                 |
| `AttachHandleExpired { mint_unix, ttl_unix, now_unix }` | 53          | §Follow-on §F.4 mirror — TTL elapsed at step (c) of `attach_with_token` validation chain                                                                                                                                                           |
| `AttachHandleBadSignature { reason }`                   | 54          | §Follow-on §F.4 mirror — step (a) signature verify failure                                                                                                                                                                                         |
| `AttachSessionMismatch { declared, actual }`            | 55          | §Follow-on §F.4 mirror — step (e) session_id registry mismatch                                                                                                                                                                                     |
| `AttachSessionUnknown { session_id }`                   | 56          | §Follow-on §F.4 mirror — step (e) session_id absent from running registry                                                                                                                                                                          |
| `PersistenceError(String)`                              | 57          | §Follow-on §F.4 passthrough — Stoolap ledger feature-disabled or fault                                                                                                                                                                             |
| `RevocationError(String)`                               | 58          | §Follow-on §F.4 passthrough — step (b) revocation-set check failed                                                                                                                                                                                 |
| `TransportHandlerNotRegistered { kind_label }`          | 59          | §Follow-on §F.4 mirror — step (e) no handler registered for `token.transport.kind`; extension surfaces (UnixSocket, Raw schemes) land via follow-on Layer D crates + registry per [[cipherocto-design-principles]] §Extension over enumeration     |
| `TokenMintSkipped { reason }`                           | 60          | RFC-0011-c §F.6.1 — CLI-dispatch-side precondition failure (idempotent self-transition on `--detach` `--token-file`); distinct from substrate AttachError mirror variants slots 53-59                                                              |
| `InvalidSinceCursor { mint_unix, requested }`           | 53 (shared) | §Follow-on §F.4 mirror — step (d) `since_unix < mint_timestamp_unix`; shared slot with `AttachHandleExpired` per amendment-chain shared-slot pattern                                                                                               |
| `InvalidSessionIdHex { reason }`                        | 47          | CLI-parse failure on `octo agent revoke-attach --session-id` (not 64-char lowercase hex). CLI-boundary, distinct from `AttachHandleBadSignature` (exit 54, substrate signature-verify failure).                                                    |
| `ReplayDetected { since_unix, recorded_cursor }`        | 61          | §9.7 follow-on amendment — step (e) session-registry detects `since_unix` cursor at or behind the recorded session cursor; typed-discriminator additive variant per amendment-chain shared-slot pattern (distinct from `Internal(reason)` exit 64) |

The CLI reuses parent's `HsmUnavailable` (exit 5 per RFC-0011
§Error Handling) instead of inventing `HsmUnreachable`. The CLI
reuses parent's `ConfirmationRequired` (exit 2 per RFC-0011
§Error Handling) for the CLI-level re-check.

Exit codes 39–61 sit in the reserved 17–63 range per RFC-0011
§Exit Codes.

### 9.9 RFC-0008 Execution Class Mapping

Per RFC-0008 §Determinism Requirements, every subcommand
maps to an execution class:

| Subcommand      | Execution Class             | Rationale                                                                                             |
| --------------- | --------------------------- | ----------------------------------------------------------------------------------------------------- |
| `agent create`  | `E3` (off-chain, HSM-gated) | Pure substrate call; no AI inference; HSM signs the manifest                                          |
| `agent run`     | `E2` (off-chain, RPC-gated) | Pure substrate call; spawns the runtime container; no AI inference yet (runtime may run E1 inference) |
| `agent list`    | `E3` (off-chain, HSM-gated) | Pure substrate read; no AI inference                                                                  |
| `agent destroy` | `E3` (off-chain, HSM-gated) | Pure substrate call; audit log append                                                                 |
| `agent attach`  | `E3` (off-chain, HSM-gated) | Pure substrate read; no AI inference                                                                  |

None of the 5 subcommands perform AI inference. The runtime spawned
by `agent run` may execute `E1` inference inside its container; that
is out of scope for this RFC and is governed by the runtime substrate
(RFC-0002 §Implementation Phases).

### 9.10 Substrate `[ADD]` Signatures

This RFC depends on the following substrate additions (Layer B):

```rust
// [ADD] octo-wallet additions (per RFC-0002 §Capability Validation)
pub fn register_agent(
    manifest: &AgentManifest,
    capability_root: &CapabilityId,
    active_did: &Did,
) -> Result<Uuid, WalletError>;  // WalletError variants per RFC-0002 substrate (non-exhaustive enum)

pub fn transition_agent(
    agent_id: Uuid,
    target_state: AgentState,
    reason: Option<&str>,
) -> Result<(), WalletError>;

pub fn list_owned_agents(
    filter: AgentFilter,
) -> Result<Vec<AgentSummary>, WalletError>;

// [ADD] octo-runtime additions (per RFC-0002 §Implementation Phases Phase 3)
pub fn spawn_agent(
    agent_id: Uuid,
    attach_handle: Option<AttachHandle>,
) -> Result<RuntimeHandle, RuntimeError>;

pub fn attach(
    handle: RuntimeHandle,
    since: Option<DateTime<Utc>>,
) -> Result<EventStream, RuntimeError>;
```

The `octo-runtime` crate is a Layer B substrate-authoritative
addition delivered by the companion mission
`0011-c-octo-runtime-substrate`. Until it lands, `agent run` and
`agent attach` ship as stubs that emit `RuntimeSubstrateNotReady`
(exit 51).

## Security Considerations

- **HSM-bound signing** — All 5 subcommands enforce that manifest
  signing happens inside the HSM. The CLI never holds a private key
  in process memory (G5).
- **Confirmation gate** — `agent destroy` requires `--confirm`
  (CLI-level exit 2 per RFC-0011 §Error Handling if absent). The
  CLI does not bypass this gate under any circumstance.
- **Capability validation** — `agent create` runs the 6-step pipeline
  per RFC-0002 §Capability Validation. The CLI does not parallel-
  validate. **Macaroon revocation** is substrate-owned per
  RFC-0957 §Algorithms (revocation lives in the macaroon
  substrate, not the Agent Manifest substrate).
- **Replay protection** — Substrate-enforced per RFC-0002 §Replay
  Protection. The CLI does not maintain a parallel window.
- **Redaction** — `OctoCliRedactor` patterns apply to all output:
  `agent_id` truncation (first 8 hex chars + `...` per RFC-0011
  §Hex32 newtype redaction); `holder_did` redaction unless
  `holder_did == active_did`; `capability_root` always truncated
  (sensitive material). `state`, `registered_at_unix`,
  `runtime_handle`, `spawned_at_unix` are NOT redacted
  (operator-owned info).
- **Cache staleness** — `agent list` cache hit surfaces a warning
  when projection older than cache TTL (`>N seconds old; consider
--no-cache`). Staleness is informational, not an error.
- **Stub deprecation** — The legacy `octo agent` stub command
  emits `StaleStub` (exit 65) starting at CLI v1.0; hard error
  in v1.1. See §Compatibility.

## Adversary Analysis

Five threats considered; all MITIGATED.

| #   | Threat                                                                  | Q1: Can they reach CLI? | Q2: Can they bypass HSM? | Q3: Can they bypass confirmation? | Q4: Can they bypass capability validation? | Q5: Can they replay?             | Status    |
| --- | ----------------------------------------------------------------------- | ----------------------- | ------------------------ | --------------------------------- | ------------------------------------------ | -------------------------------- | --------- |
| 1   | **Local malware** tries to spawn agents owned by another operator's DID | NO (`active_did` check) | NO (HSM-gated)           | n/a                               | n/a                                        | n/a                              | MITIGATED |
| 2   | **Capability thief** tries to register an agent with stolen macaroon    | NO (HSM required)       | NO (HSM-gated)           | n/a                               | NO (substrate rejects revoked macaroon)    | NO (digest tracked)              | MITIGATED |
| 3   | **Operator mistake** destroys the wrong agent                           | n/a                     | n/a                      | YES (`--confirm` required)        | n/a                                        | n/a                              | MITIGATED |
| 4   | **Replay attacker** re-sends yesterday's `agent create` manifest        | NO                      | NO                       | n/a                               | NO (RFC-0002 §Replay Protection)           | NO (RFC-0002 §Replay Protection) | MITIGATED |
| 5   | **Cache poisoning** corrupts `agent list` output                        | NO (substrate TTL)      | n/a                      | n/a                               | n/a                                        | n/a                              | MITIGATED |

### A1 — Local Malware

**Threat:** Malware on the operator workstation invokes
`octo agent create` to register an attacker-controlled manifest.

**Q1: Can they reach CLI?** The CLI requires `active_did()` to resolve;
the malware does not have access to the HSM slot, so `active_did()`
returns `HsmUnavailable` (exit 5 per RFC-0011 §Error Handling).

**Q2: Can they bypass HSM?** No — manifest signing is HSM-gated.

**Q3: Can they bypass confirmation?** n/a — `agent create` does not
require confirmation.

**Q4: Can they bypass capability validation?** n/a — capability
validation runs on the substrate side and rejects revoked macaroons.

**Q5: Can they replay?** No — RFC-0002 §Replay Protection tracks
`nonce: [u8;16]` per `AgentMessage` + `prev_message_hash: Option<[u8;32]>`
chain via `chain_store::verify_prev_hash`; duplicates are rejected.

**Status:** MITIGATED.

### A2 — Capability Thief

**Threat:** Attacker steals a macaroon (e.g., from a leaked
`capability_root` reference in a log file) and uses it to register
an agent.

**Q1–Q4: HSM-gated.** The attacker still needs the holder private key
to sign the manifest; without it, the substrate rejects the manifest
at step 1 of capability validation.

**Q5: Replay protection.** Even if the attacker somehow signs, the
manifest digest is tracked and rejected on re-submission.

**Status:** MITIGATED.

### A3 — Operator Mistake

**Threat:** Operator accidentally runs `octo agent destroy <wrong-id>`
and destroys a healthy agent.

**Q3: Confirmation gate.** `agent destroy` requires `--confirm`;
absent flag → `ConfirmationRequired` (CLI-level exit 2 per
RFC-0011 §Error Handling). The CLI does not provide a `--yes`
or `--force` override.

**Status:** MITIGATED.

### A4 — Replay Attacker

**Threat:** Attacker captures yesterday's `agent create` manifest and
re-submits it.

**Q4: Replay protection (separate from capability validation 6-step).**
Substrate rejects duplicate manifest digests per RFC-0002 §Replay
Protection (`nonce: [u8;16]` per `AgentMessage` + `prev_message_hash:
Option<[u8;32]>` chain).

**Q5: Replay chain.** Substrate rejects mismatched
`chain_store::verify_prev_hash` per RFC-0002 §Replay Protection.

**Status:** MITIGATED.

### A5 — Cache Poisoning

**Threat:** Attacker corrupts the CLI's local cache to inject fake
`agent list` output.

**Q1–Q5: Substrate TTL cache.** Cache is substrate TTL-based per
RFC-0011-e §Design Goals G2/G5 (LRU + TTL pattern); the CLI does
not implement custom cache signing. Stale cache surfaces a warning
to stderr; operator can re-run with `--no-cache`.

**Status:** MITIGATED.

## Test Vectors

Test vectors for the `octo agent` subcommand group. All vectors
must pass before the amendment is promoted.

| #        | Subcommand      | Input                                            | Expected Output                                                                            | Notes                                |
| -------- | --------------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------ | ------------------------------------ |
| TV-AGT1  | `agent create`  | Valid manifest, valid capability root, valid HSM | `AgentCreateOutput { state: REGISTERED, ... }` (exit 0)                                    | Happy path                           |
| TV-AGT2  | `agent create`  | Invalid manifest signature                       | `CapabilityValidationFailed(1)` (exit 40)                                                  | Fails step 1 of 6-step pipeline      |
| TV-AGT3  | `agent create`  | Duplicate `agent_id`                             | `AgentAlreadyExists(uuid)` (exit 41)                                                       | Substrate rejects                    |
| TV-AGT4  | `agent run`     | Registered agent, no runtime                     | `AgentRunOutput { state: BUSY, ... }` (exit 0)                                             | Spawns runtime container             |
| TV-AGT5  | `agent run`     | Terminated agent                                 | `InvalidStateTransition { from: TERMINATED, to: ACTIVE }` (exit 43)                        | State machine rejects                |
| TV-AGT6  | `agent list`    | 50 owned agents, no filter                       | `AgentListOutput { agents: [...50], next_cursor: None }`                                   | All 50 in single page                |
| TV-AGT7  | `agent list`    | `--state ACTIVE --chain-id chain-a`              | Filtered list of ACTIVE agents on chain-a                                                  | Client-side filter                   |
| TV-AGT8  | `agent list`    | `--limit 0`                                      | `InvalidLimit` (exit 45)                                                                   | Defensive validation                 |
| TV-AGT9  | `agent destroy` | Active agent, `--confirm`                        | `AgentDestroyOutput { state: TERMINATED, audit_log_entry: ..., ... }` (exit 0)             | Audit log appended                   |
| TV-AGT10 | `agent destroy` | Active agent, no `--confirm`                     | `ConfirmationRequired` (exit 2)                                                            | Confirmation gate enforced           |
| TV-AGT11 | `agent attach`  | Running agent                                    | `AgentAttachOutput { runtime_handle: ..., attached_at_unix: ..., ... }` (exit 0)           | Read-only attach                     |
| TV-AGT12 | `agent attach`  | Terminated agent                                 | `AgentNotRunning(uuid)` (exit 48)                                                          | Attach to non-running agent rejected |
| TV-AGT13 | `agent create`  | Replay of yesterday's manifest                   | `Internal(reason)` (exit 64) — typed-discriminator variant deferred to follow-on amendment | RFC-0002 §Replay Protection enforced |

## Alternatives Considered

### REST API

**Considered:** Expose the agent lifecycle as a REST API instead of a
CLI subcommand group.

**Rejected because:** Operators want a single binary surface for all
RFC-0011 amendments; REST API would fragment the operator UX across
HTTP and CLI. The CLI is the canonical substrate surface; REST API
would be a thin wrapper anyway (Layer C/D).

### TUI

**Considered:** Replace the pretty-table renderer with a full TUI
(ncurses-style) for `agent list`.

**Rejected because:** TUI requires an interactive terminal; many
operator workflows (cron jobs, scripts, SSH from non-tty contexts)
need non-interactive output. The TTY-aware renderer covers both.

### Separate binary

**Considered:** Land `octo-agent` as a separate binary rather than a
subcommand of `octo`.

**Rejected because:** Single binary surface is a core principle of
RFC-0011 (one `octo` binary for identity/capability/policy + every
amendment). A separate binary would duplicate the clap root, the
output envelope, the redaction layer, and the exit-code table.

## Implementation Phases

### Phase 1 — `octo-runtime` substrate

The `octo-runtime` crate must land first (Layer B substrate) with:

- `spawn_agent(agent_id, handle) -> RuntimeHandle`
- `attach(handle, since) -> EventStream`
- State-machine integration with `octo-wallet` (shared via substrate
  trait `AgentStateDispatcher`)

This is gated on the companion substrate mission
`0011-c-octo-runtime-substrate` landing. Until Phase 1 lands,
`octo agent run` and `octo agent attach` ship as stubs that emit
`RuntimeSubstrateNotReady` (exit 51).

#### Layer Direction

RFC-0011-c is Layer C/D (operator UX). Phase 1 introduces
`octo-runtime` Layer B substrate via companion mission
`0011-c-octo-runtime-substrate`. Per project principle §RFC Reference
Conventions Reaffirmed, Layer C/D amendments CANNOT define Layer B
substrate in the same RFC; the companion mission provides the
Layer B definition. RFC-0011-c consumes that substrate; the layer
direction is preserved.

The 3 octo-wallet `[ADD]` functions in §9.10
(`register_agent`, `transition_agent`, `list_owned_agents`) are
interface contracts only; implementation requires a follow-on
companion mission `0011-c-octowallet-agents-substrate` (Layer B) to
be created and accepted before RFC-0011-c moves to Accepted status.

### Phase 2 — `octo-cli` wiring

The CLI changes (Layer C/D) land in five missions:

1. `0011-c-agent-create-subcommand` — `agent create` subcommand
2. `0011-c-agent-run-subcommand` — `agent run` subcommand
   (gated on `0011-c-octo-runtime-substrate`)
3. `0011-c-agent-list-subcommand` — `agent list` subcommand
4. `0011-c-agent-destroy-subcommand` — `agent destroy` subcommand
5. `0011-c-agent-attach-subcommand` — `agent attach` subcommand
   (gated on `0011-c-octo-runtime-substrate`)

Each mission lands one subcommand end-to-end (clap wiring, output
type, error variant, redaction pattern, test vectors). The five
missions are independent at the CLI layer (no cross-mission
prerequisite beyond the shared `Commands::Agent` enum landing in
the clap root, which is part of mission `0011-c-agent-create-subcommand`).

## Mission Decomposition

Six missions: five subcommand missions (1:1 with the five subcommands) + one substrate mission (`0011-c-octo-runtime-substrate` per §9.10). The mission
decomposition is **flat** (no nested sub-missions) per
[[cipherocto-design-principles]] (no premature coupling).

## Key Files to Modify

### DOC-ONLY (this RFC cycle)

- `rfcs/draft/process/0011-c-agent-lifecycle.md` — this RFC
- `missions/open/0011-c-agent-create-subcommand.md` — companion mission
- `missions/open/0011-c-agent-run-subcommand.md` — companion mission
- `missions/open/0011-c-agent-list-subcommand.md` — companion mission
- `missions/open/0011-c-agent-destroy-subcommand.md` — companion mission
- `missions/open/0011-c-agent-attach-subcommand.md` — companion mission
- `missions/open/0011-c-octo-runtime-substrate.md` — companion substrate mission (NEW; Layer B)

### SUBSTRATE (follow-on missions, NOT this RFC cycle)

- `octo-cli` Cargo manifest — add `octo-runtime = { path = "../octo-runtime" }` (Layer B substrate)
- `octo_cli::commands::agent` module — NEW; `Commands::Agent` clap enum + `AgentAction::{Create, Run, List, Destroy, Attach}` dispatch + 5 payload types (per RFC-0011-c §9.3)
- `octo_cli::error` module — add 16 new variants to `#[non_exhaustive] OctoCliError` (per RFC-0011-c §9.8; slots 39-61)
- `octo_cli::redact` module — add agent-specific redaction patterns (`agent_id`, `holder_did`, `capability_root` per RFC-0011-c §Security)
- `octo_runtime` crate root — NEW (per companion mission `0011-c-octo-runtime-substrate`); `spawn_agent` + `attach` + `RuntimeHandle` + `EventStream`
- `octo_runtime::spawn` module — NEW; `spawn_agent` impl
- `octo_runtime::attach` module — NEW; `attach` impl
- `octo-runtime` Cargo manifest — NEW; deps per companion mission

## Future Work

- Status polling for `octo agent run` / `octo agent attach` (post-v1.1; CLI currently surfaces the substrate handle and exits).
- `octo-runtime` substrate mission lands via `0011-c-octo-runtime-substrate` (NEW; companion mission per L2).
- `octo-wallet` agents substrate mission `0011-c-octowallet-agents-substrate` (Layer B) — implementation of the 3 octo-wallet `[ADD]` functions in §9.10 (TBD follow-on companion mission; required before RFC-0011-c moves to Accepted status per §Layer Direction).
- RFC-0011-a audit substrate dependency for `agent destroy` mission completion — mission ships as `AuditSubstrateNotReady` (exit 52) stub until RFC-0011-a Accepted.
- Future amendment beyond the RFC-0011-a through RFC-0011-g chain (if any) — new subcommands land as additive variants on the `#[non_exhaustive] AgentAction` enum.

## Follow-on

The AttachHandle token pathway (NEW; added per paired RFC amendment with mission `0011-c-octo-runtime-attachhandle-substrate` per [[no-phantom-mission-pointers]] rule) defines how `octo agent attach` binds to a previously-detached `octo agent run --detach` invocation across process boundaries. The pathway covers substrate (`octo-runtime` Layer B), CLI extension (`octo-cli` Layer C/D), in-memory token revocation, and per-agent cursor persistence via Stoolap ledger extension. Five sections follow.

### §F.1 Encoding

Canonical byte encoding for `AttachHandle` tokens at the `octo_runtime::handle::encoding` module:

- `encode_token(token: &AttachHandle) -> Result<Vec<u8>, AttachError>` — length-prefixed; version byte `0x00`; BLAKE3-checked
- `decode_token(bytes: &[u8], holder_pubkey: &[u8; 32]) -> Result<AttachHandle, AttachError>` — symmetric decoder; verifies signature on decode using `verify_attach_handle_payload` (signature is over `session_id || payload || mint_timestamp_unix || ttl_unix` which does NOT bind to holder_pubkey, hence explicit pass-through)
- Encoding version pinned to `0x00`; future versions increment per octo-runtime canonical-bytes invariant (mirrors RFC-0016-a §6.10)
- Encoding must be canonical (no equivalent-but-distinct bytes for same token); reject non-canonical input on decode

### §F.2 Token Substrate

`AttachHandle` type and bind operations at the `octo_runtime::handle` module (Layer B):

- `pub struct AttachHandle { session_id: SessionId, mint_timestamp_unix: u64, ttl_unix: u64, signature: Signature, payload: AttachPayload, transport: Transport }`
- `pub struct Transport { pub kind: TransportKind, pub addr: Option<String> }` — typed-discriminator + Raw escape hatch per RFC-0855 §Typed UUID discriminators; new transports land via `TransportKind::Raw(Uuid)` without central enum edit
- `#[non_exhaustive] pub enum TransportKind { InProcess, UnixSocket, Raw(Uuid) }` — discriminators; `Raw` is the extension surface per [[cipherocto-design-principles]] §Extension over enumeration
- `pub struct AttachPayload { agent_id: Uuid, since_cursor: u64 }`
- `pub type SessionId = [u8; 32]` — random per `spawn_agent` call
- `pub fn mint_attach_handle(holder: &IdentityKey, agent_id: Uuid, session_id: SessionId, since_cursor: u64, ttl_unix: u64, transport: Transport) -> Result<AttachHandle, AttachError>` (calls `sign_attach_handle_payload` per §F.5; composes `octo_wallet::IdentityKey::sign`). DID → `IdentityKey` resolution is a CLI boundary concern (Layer C/D), not substrate.
- `pub async fn attach_with_token(holder_pubkey: &[u8; 32], token: &AttachHandle, since_unix: u64) -> Result<AttachedSession, AttachError>` (defined at the `octo_runtime` crate root, re-exported alongside `attach` + `clamp_since` per §F.2) — validation chain: (a) signature verify via `verify_attach_handle_payload` (per §F.5), (b) revocation-set check via `is_token_revoked`, (c) `now_unix <= token.ttl_unix` (substrate discipline — `ttl_unix == u64::MAX` is the reserved sentinel; any token whose `ttl_unix == u64::MAX` is always rejected as `Expired` per the fail-CLOSED on broken-clock ambiguity rule: the same value `u64::MAX` would saturate `now_unix` from a `duration_since` error so the substrate cannot distinguish a sentinel TTL from a saturated clock and rejects uniformly), (d) `since_unix >= token.mint_timestamp_unix`, (e) **transport-handler dispatch** via `token.transport.kind` against the process-singleton `octo_runtime::handle::transport::HANDLE_TRANSPORT_REGISTRY` (built-in `InProcessHandler` registered at lazy init; extension transports register from follow-on Layer D crates per [[cipherocto-design-principles]] §per-extension crates + registry). Unregistered `TransportKind` surfaces `AttachError::TransportHandlerNotRegistered { kind_label }` (CLI exit 59; per-handler errors surface per-Layer-D). The substrate stays filesystem-free + socket-IO-free per §Layer direction. The leading `holder_pubkey` parameter mirrors the static-helper signature shape of `verify_attach_handle_payload` (Layer B pure helper, no `&self` binding).
- `pub trait octo_runtime::handle::transport::Handler: Send + Sync + std::fmt::Debug { fn bind(&self, token: &AttachHandle, since_unix: u64) -> Result<AttachedSession, AttachError> }` — single-method, segregated per [[cipherocto-design-principles]] §Interface Segregation; new extension-bearing operations land as additive traits, not `Handler` mutations. Per-transport protocol I/O (filesystem resolution, socket connect, …) lives in `Arc<dyn Handler>` impls registered at process startup.
- `pub struct octo_runtime::handle::transport::Registry { handlers: RwLock<HashMap<TransportKind, Arc<dyn Handler>>> }` — process-singleton (via `HANDLE_TRANSPORT_REGISTRY: std::sync::OnceLock<Registry>`), `RwLock` for read-heavy dispatch (handler lookup vastly outnumbers registration), **fail-CLOSED on poison** (mirroring the `InMemoryRevocationStore::inner` discipline per §F.7.5 Phase C — a poisoned lock indicates writer panic; substrate propagates the failure to the caller rather than silently serving stale dispatch). `Handler` debug-required supertrait enables `#[derive(Debug)]` on the registry.
- `pub struct InProcessHandler` — built-in broadcast-channel binding handler (per-handle `tokio::sync::broadcast` from `RuntimeHandle`); registered at `HANDLE_TRANSPORT_REGISTRY` lazy init via `build_in_process_registry()`. Session-registry-wiring (mapping `session_id → Arc<HandleInner>` across the process boundary) is deferred to a follow-on amendment per §F.2 — the handler currently mirrors pre-handler behavior (panic-in-debug + `AttachError::UnknownSession` in release) to catch accidental callsite reliance during the substrate-first rollout.
- `pub static HANDLE_TRANSPORT_REGISTRY: std::sync::OnceLock<Registry>` — lazy-init process singleton (Rust 1.70+, no `once_cell` dep); initial value via `build_in_process_registry()` factory fn. Extension Layer D transport crates (`octo-runtime-transport-unix`, user-extension `Raw(Uuid)` implementations, …) register additional handlers via `Registry::register` at process startup per [[cipherocto-design-principles]] §per-extension crates + registry pattern (extension surface stays open: new transports land via a registry call, no central `match` edit in `attach_with_token`).

The existing `pub fn attach(handle: RuntimeHandle, since: Option<DateTime<Utc>>) -> Result<EventStream, RuntimeError>` at the `octo_runtime::attach` module is UNCHANGED — it consumes the renamed 4-field in-process binding (`RuntimeHandleBinding` per Path B rename: `agent_id`, `handle_id`, `session_id`, `spawned_at_unix`). The 6-field `AttachHandle` token lives alongside the renamed struct at the `octo_runtime::handle` module per [[cipherocto-design-principles]] §No parallel abstractions.

- `pub struct AttachedSession { pub event_cursor: u64, pub broadcast_rx: tokio::sync::broadcast::Receiver<RuntimeEvent> }`

### §F.3 Persistence + Revocation

Stoolap cursor persistence + in-memory revocation set at the `octo_runtime::persistence` module (Layer B):

- `pub fn persist_event_cursor(agent_id: Uuid, cursor: u64) -> Result<(), PersistenceError>` — gated on `cfg(feature = "octo-runtime-persistence")` (the feature flag is being newly added to `octo_runtime`'s manifest per the paired mission); canonical-bytes-on-write pattern is a coding reference per RFC-0016-a §6.10, not a paired-acceptance contract
- `pub fn load_event_cursor(agent_id: Uuid) -> Result<Option<u64>, PersistenceError>` — symmetric
- `pub fn revoke_attach_token(session_id: SessionId) -> Result<(), AttachError>` — dispatches via the installed `RevocationStore` (process-singleton `Arc<dyn RevocationStore>` per §F.7.5 Phase C paired follow-on). The default `InMemoryRevocationStore` carries the original `RwLock<HashSet<SessionId>>` with the existing fork-fail-closed contract: revocation set is not propagated across `fork()`; child processes start with an empty revocation set. Test seam: `InMemoryRevocationStore::with_inner<F, R>(&self, f: F) -> R` per §F.7.5 (preserves the prior `with_revocation_set<F, R>` test injection intent). Cross-process revocation propagation lands via the `StoolapRevocationStore` extension crate per §F.7.5.
- `pub fn is_token_revoked(session_id: &SessionId) -> bool` — fast-path check invoked at step (b) of `attach_with_token()` validation chain per §F.2. **Fail-CLOSED on poisoned `RwLock`**: a prior panic during a revocation operation poisons the underlying `RwLock`; in that state the function returns `true` so the token is treated as revoked (fail-CLOSED preserves the explicit-operator-revocation guarantee at the cost of denying attach operations until the operator restarts the process). The alternative (fail-OPEN) would silently bypass explicit operator revocations.

### §F.4 Errors

`pub enum AttachError` at the `octo_runtime::handle::error` module (Layer B):

- `Expired { session_id: SessionId, mint_unix: u64, expired_at_unix: u64, now_unix: u64 }` (mirror → `OctoCliError::AttachHandleExpired { mint_unix, ttl_unix, now_unix }` exit 53 per RFC-0011-c §9.8 slot allocation extended 39-61; the CLI-boundary `ttl_unix` mirrors the substrate `expired_at_unix` with CLI-friendly shortening, and the CLI envelope intentionally drops `session_id` — typed-discriminator recovery is available via the substrate `Debug` impl on the typed `[u8; 32]`)
- `BadSignature { reason: String }` (mirror → `OctoCliError::AttachHandleBadSignature` exit 54)
- `SessionMismatch { declared: SessionId, actual: SessionId }` (mirror → `OctoCliError::AttachSessionMismatch` exit 55)
- `UnknownSession { session_id: SessionId }` (mirror → `OctoCliError::AttachSessionUnknown` exit 56)
- `InvalidSinceCursor { mint_unix: u64, requested: u64 }` (mirror → `OctoCliError::InvalidSinceCursor { mint_unix, requested }` exit 53 — **shared slot** with `AttachHandleExpired` per amendment-chain shared-slot pattern; operator-unambiguous within the `agent attach` command surface because the render layer distinguishes the two payloads by variant name)
- `PersistenceError(String)` (mirror → `OctoCliError::PersistenceError(String)` exit 57)
- `RevocationError(String)` (mirror → `OctoCliError::RevocationError(String)` exit 58) — folded into `AttachError` per R7 simplification; the standalone `RevocationError` substrate enum from earlier §F.3 is deleted (the canonical variant name is `RevocationError`, not `RevocationFailed`)
- `TransportHandlerNotRegistered { kind_label: String }` (mirror → `OctoCliError::TransportHandlerNotRegistered { kind_label }` exit 59 per RFC-0011-c §9.8 row added in this amendment; surfaces step (e) registry-miss of `octo_runtime::handle::transport::HANDLE_TRANSPORT_REGISTRY`)
- `ReplayDetected { since_unix: u64, recorded_cursor: u64 }` (mirror → `OctoCliError::ReplayDetected { since_unix, recorded_cursor }` exit 61 per RFC-0011-c §9.8 row added in the §9.7 follow-on amendment; surfaces step (e) session-registry detecting replayed `since_unix` cursor at or behind recorded cursor)

CLI error surface mirrors via `OctoCliError` variants appended per RFC-0011-c §9.8 slot allocation.

### §F.5 Signing Surface

Signing wrappers colocated with the `AttachHandle` token type at the `octo_runtime::handle::signing` submodule (Layer B; re-exported at the `octo_runtime` crate root via `pub use handle::signing::{...}`). The wrappers compose substrate helpers on `octo_wallet::IdentityKey` per RFC-0015-a Appendix A (the existing operative signing surface); no new Layer A types introduced. The substrate follows the `verify_successor_proof` / `verify_revocation_proof` static-helper pattern (pure helpers that verify against arbitrary `pubkey: &[u8; 32]` rather than binding to a `&self` receiver):

- `pub fn sign_attach_handle_payload(holder: &IdentityKey, session_id: SessionId, payload: &AttachPayload, mint_timestamp_unix: u64, ttl_unix: u64) -> Result<Signature, AttachError>` — encodes `session_id || payload || mint_timestamp_unix || ttl_unix` via the single canonical helper `canonical_payload_bytes` (per §F.1, colocated with encoding module) and delegates to `octo_wallet::IdentityKey::sign(msg_bytes)`; the resulting `ed25519-dalek` signature is captured into the substrate-visible `Signature` newtype (`pub struct Signature(pub [u8;64])` in `octo_runtime::handle`) directly via the 64-byte raw form — avoids Layer A type leak per [[cipherocto-design-principles]] §Stable Abstractions Principle.
- `pub fn verify_attach_handle_payload(holder_pubkey: &[u8; 32], session_id: &SessionId, payload: &AttachPayload, mint_timestamp_unix: u64, ttl_unix: u64, sig: &Signature) -> Result<(), AttachError>` — static helper mirroring `verify_revocation_proof` shape; consumes the same `canonical_payload_bytes` helper as `sign_attach_handle_payload` so both produce / consume IDENTICAL canonical bytes per RFC-0016-a §6.10 canonical-bytes invariant.

`octo-wallet` is the substrate for `IdentityKey::sign` (Layer B per RFC-0015-a Appendix A); `ed25519-dalek` (Layer A frozen, re-exported via `octo_wallet::ed25519_dalek`) is the underlying cryptographic primitive. No `octo_wallet::crypto` module is created — composition pattern follows [[cipherocto-design-principles]] §Stable Abstractions Principle, primitives in stable substrate, business semantics in composed layer.

### §F.6 CLI Dispatch Wiring

The CLI dispatch bodies that bind the substrate surface (§F.1-§F.5) into the operator workstation live in `crates/octo-cli/src/commands/agent.rs` (Layer C/D). The dispatch is split across two subcommands that compose via the persistent `AttachHandle` token file written by `run` and consumed by `attach`:

**§F.6.1 — `octo agent run --detach --token-file <path>` (emit side)**

- clap args: `--detach` (existing) + `--token-file <path>` (NEW; `requires = "detach"` enforces that the token pathway is only activated on detached spawns, where the in-process `RuntimeHandle` survives beyond the CLI process lifetime via the persisted token)
- dispatch flow (after `transition_agent` + `spawn_agent` succeed):
  1. Resolve caller DID → `IdentityKey` via `mod common::resolve_active_identity_key()` (helper added in this cycle per §F.5; CLI boundary concern)
  2. Extract `session_id: SessionId` (= `[u8; 32]`) from the `RuntimeHandle` returned by `spawn_agent` (the substrate carries the derived session id on the handle per §F.2)
  3. Call `octo_runtime::mint_attach_handle(holder: &IdentityKey, agent_id, session_id, since_cursor, ttl_unix, Transport::IN_PROCESS)` per §F.2
  4. Call `octo_runtime::encode_token(&handle) -> Vec<u8>` per §F.1
  5. Write bytes to `--token-file` via `std::fs::OpenOptions::create + truncate + mode 0o600 + set_permissions(0o600) + fsync` (POSIX mandatory; the token is a credential per RFC-0011-c §Layer direction — re-enforce 0o600 post-open in case the path pre-exists at world-readable perms)
  6. Create parent dirs via `std::fs::create_dir_all(parent)` if absent
  7. Populate `AgentRunOutput::token_written: Option<TokenWrittenReceipt>` with `{ session_id_hex, bytes_written, path_redacted: RedactedIdentifier::new(redacted_path), written_at_unix }`

**§F.6.2 — `octo agent attach --token-file <path> [--since <unix-seconds>]` (consume side)**

- clap args: `--token-file <path>` (NEW; required when the runtime handle must bind via persistent token pathway) + `--since <unix-seconds>` (existing; default = `mint_timestamp_unix` if absent)
- dispatch flow (after `lookup_agent` + `read_agent_state` confirm `Running` per the §9.3.5 state-gate):
  1. Read token bytes from `--token-file` via `std::fs::read` (caller-scoped error if path missing / unreadable)
  2. Resolve caller DID → `[u8; 32]` holder_pubkey via `IdentityKey::public_key_bytes()` (CLI boundary concern per §F.5)
  3. Call `octo_runtime::decode_token(&bytes, holder_pubkey) -> AttachHandle` (verifies signature + BLAKE3 integrity per §F.1)
  4. Call `octo_runtime::attach_with_token(holder_pubkey, &token, since_unix).await` (executes the §F.2 validation chain steps (a)-(e))
  5. Populate `AgentAttachOutput { agent_id, runtime_handle: None, attached_at_unix, event_cursor, session_id_hex: hex::encode(token.session_id) }` (`runtime_handle` is `None` on the attach pathway because `octo_runtime::handle::AttachedSession` only carries `event_cursor` + `broadcast_rx`; `session_id_hex` is the canonical binding identifier populated from `RuntimeHandle.session_id` per §F.6.1 step 2; the post-R5.5 schema-faithful reconciliation prevents older consumers from misinterpreting session_id_hex as a UUID via the `runtime_handle` field)

**§F.6.3 — `OctoCliError` mirror surface**

The 9 substrate `AttachError` variants map to 9 `OctoCliError` variants (slots 53-61; 9 variants / 8 slots per the amendment-chain shared-slot pattern, including the §9.7 follow-on amendment `ReplayDetected` at slot 61) via the `From<octo_runtime::AttachError> for OctoCliError` impl. The CLI adds zero new substrate-mirror variants in this cycle — the mirror surface landed in the substrate cycle. The canonical exit codes per RFC-0011-c §9.8 row:

- `AttachHandleExpired { mint_unix, ttl_unix, now_unix }` → exit 53
- `AttachHandleBadSignature { reason }` → exit 54
- `AttachSessionMismatch { declared, actual }` → exit 55
- `AttachSessionUnknown(String)` → exit 56
- `PersistenceError(String)` → exit 57
- `RevocationError(String)` → exit 58
- `TransportHandlerNotRegistered { kind_label }` → exit 59
- `InvalidSinceCursor { mint_unix, requested }` → exit 53 (shared-slot with `AttachHandleExpired`)
- `ReplayDetected { since_unix, recorded_cursor }` → exit 61 (§9.7 follow-on amendment — typed-discriminator additive variant, not shared)

The CLI surface ALSO adds a dispatch-side variant outside the substrate mirror: `TokenMintSkipped { reason: String }` → exit 60 (CLI dispatch surface per §F.6.1; reserved per the RFC-0011-c §9.8 slot allocation introduced in this amendment cycle). The substrate does not surface `TokenMintSkipped` because the substrate's `spawn_agent` is the silent-self-transition ancestor — only the CLI dispatch sees the idempotent self-transition signal.

**§F.6.4 — Pairing invariant**

Every successful `octo agent run --detach --token-file <path>` write produces token bytes that ONLY `octo agent attach --token-file <path>` (with the matching caller DID) can consume — the substrate enforces this via the §F.2 step (a) signature verify (holder_pubkey must equal the `IdentityKey` used at mint time) + step (b) revocation-set check (token revoked by `octo revoke-attach --session-id` → step (b) returns `RevocationError` exit 58) + step (c) TTL check (`now_unix > ttl_unix` → `Expired` exit 53; `ttl_unix == u64::MAX` always rejected per fail-CLOSED on broken-clock ambiguity rule) + step (d) since-cursor check (`since_unix < mint_timestamp_unix` → `InvalidSinceCursor` exit 53 shared-slot). Replay across `attach` invocations is bounded by step (e) session-registry-wiring (deferred to follow-on amendment per §F.6.5).

**§F.6.5 — Out of scope (deferred to follow-on amendment cycles)**

- **Session-registry-wiring for `InProcessHandler::bind`** — the `InProcessHandler` currently mirrors pre-handler behavior (panic-in-debug + `AttachError::UnknownSession` exit 56 in release) per §F.2 step (e) deferral. Successful cross-process `attach --token-file` requires the session-registry-wiring follow-on amendment (substrate-side: register `session_id → Arc<HandleInner>` in the process-singleton registry on `spawn_agent`; cli-side: unblocks the happy-path attach TV). The CLI dispatch surface wired in §F.6.1-§F.6.2 is substrate-faithful; the missing wiring is purely substrate-side. Follow-on amendment paired mission (companion to this one) lands the registry write in `spawn_agent` + the `InProcessHandler::bind` dispatch lookup.
- **Replay typed-discriminator variant** — added in the §9.7 follow-on amendment (exit 61; typed-discriminator `ReplayDetected { since_unix, recorded_cursor }`). Removed from this out-of-scope list per §9.7 amendment + §9.8 row extension (39-61).
- **Hybrid `node_type` placeholder** — populated by a future mission that consumes the substrate's `Transport::Raw(Uuid)` extension seam. Landed via the `octo-runtime-transport-hybrid` extension crate per §F.7.3 (Phase B follow-on cycle).
- **UnixSocket + Raw extension Layer D crates** — register handlers into `HANDLE_TRANSPORT_REGISTRY` via the per-extension crate + registry pattern per [[cipherocto-design-principles]] §Extension over enumeration. Landed via §F.7.1 (`octo-runtime-transport-unix`) + §F.7.2 (`octo-runtime-transport-raw`) extension crates (Phase B follow-on cycle).

### §F.7 Layer D Extension Crates

The Layer D extension crates implement the `Handler` trait per [[cipherocto-design-principles]] §per-extension crates + registry pattern. Three crates ship as the Phase B follow-on cycle:

- `octo-runtime-transport-unix` — Unix-domain socket transport
- `octo-runtime-transport-raw` — Raw scheme UUID transport (extension seam)
- `octo-runtime-transport-hybrid` — Hybrid multiplexer wrapping two sub-Handlers

Each crate owns its transport-protocol I/O (filesystem resolution, socket connect, scheme resolution). The core `octo-runtime` stays filesystem-free + socket-IO-free per §Layer direction (substrate stays unaware of how a `Handler` impl resolves a `Transport` payload).

Per-extension crate pattern:

1. **Trait in core** — `octo_runtime::handle::transport::Handler` (existing, single-method `bind` per §F.2)
2. **Each extension = own crate** — Layer D crates implement the trait independently
3. **Registry in core** — `HANDLE_TRANSPORT_REGISTRY: OnceLock<Registry>` (existing, §F.2)
4. **Extensions register at startup** — `pub fn register_into(registry: &Registry)` init fn exposed by each crate + consumed by `octo-cli` (or downstream consumer crate) at startup
5. **Core code dispatches via registry lookup** — `attach_with_token` step (e) per §F.2
6. **Core unchanged when new extensions land** — new transports register via a registry call, no central `match` edit in `attach_with_token`

#### §F.7.1 — `octo-runtime-transport-unix`

Layer D extension crate at `crates/octo-runtime-transport-unix/`:

- `Cargo.toml`: `octo-runtime = { path = "../octo-runtime", version = "0.1.0" }` (Layer B sibling dep). Layer model = D. No `octo-cli` or `octo-wallet` deps (Layer D does not reach into Layer C). Phase B owns the client-side connect surface only (no tokio dep — the protocol framing in this crate is hand-rolled 8-byte LE on `std::os::unix::net::UnixStream` to keep the substrate constraint of `Handler::bind` being a sync trait method; server-side event piping + cross-process broadcast → process-local subscribe is the Phase C paired follow-on per §F.7.4).
- `pub struct UnixSocketHandler` (unit struct) — implements `Handler` trait
- `fn bind(&self, token: &AttachHandle, since_unix: u64) -> Result<AttachedSession, AttachError>` — **Phase B scope (current):** resolves `token.transport.addr` (Unix-domain socket path string) for the missing-addr check, then fails-CLOSED with `AttachError::Internal("cross-process event bridge not implemented...")` until the Phase C follow-on amendment wires the server-side event piping. **Phase C scope (paired follow-on):** performs protocol handshake (sends token canonical bytes + `since_unix`; receives `event_cursor` reply); returns `AttachedSession { event_cursor, broadcast_rx }` where `broadcast_rx` is subscribed from a local `tokio::sync::broadcast::Sender<RuntimeEvent>` the server-side process pipes events into. The cross-process event bridging mechanism is a Layer D concern per §F.7.4.
- Failure modes map to the additive `AttachError::Internal(String)` substrate variant (routed via the existing `From<AttachError>` wildcard arm in `crates/octo-cli/src/error.rs`): `Internal("...requires an addr...")` when `token.transport.addr` is `None`; `Internal("...cross-process event bridge not implemented...")` until Phase C lands. No other typed-discriminator variants for protocol-level errors are introduced — typed-discriminator additions remain a future-work follow-on if/when operator observability requires it.
- `pub fn register_into(registry: &Registry)` — convenience init fn that registers `TransportKind::UnixSocket → Arc::new(UnixSocketHandler::default())` into the supplied registry. The shared `Arc<dyn Handler>` is allocated once and cached in a `static OnceLock<Arc<dyn Handler>>` so identity-idempotent re-calls yield pointer-equal `Arc` (per fail-CLOSED + identity discipline for `Registry::register` last-write-wins).
- TV-AGT23 — `agent attach` multi-process via UnixSocket loopback (inverts from RED exit 59 to GREEN happy path). **Phase B (current):** `UnixSocketHandler::bind` returns `AttachError::Internal("...cross-process event bridge not implemented...")` (the substrate-side fail-CLOSED deferral). **Phase C (paired follow-on):** happy path returns `AttachedSession { event_cursor: 0, broadcast_rx: local-subscribe }`. Operator-facing cross-process test lands in the Phase C follow-on amendment paired with cross-process revocation propagation.

#### §F.7.2 — `octo-runtime-transport-raw`

Layer D extension crate at `crates/octo-runtime-transport-raw/`:

- `Cargo.toml`: `octo-runtime = { path = "../octo-runtime", version = "0.1.0" }` (no Layer D I/O deps — scheme resolution is opaque to the substrate; downstream crates wire their own protocol). Layer model = D.
- `pub struct RawHandler { scheme_id: Uuid }` — implements `Handler` trait
- `fn bind(&self, _token: &AttachHandle, _since_unix: u64) -> Result<AttachedSession, AttachError>` — returns `Err(AttachError::Internal(format!("raw scheme `{}` dispatch not configured — downstream crate must register a Handler via octo_runtime_transport_raw::register_into before attach", self.scheme_id)))` per the substrate's fail-CLOSED on unconfigured extension. The Raw escape hatch exists for downstream crates to wire their own protocol — the substrate never invents a default behavior (typed-discriminator + Raw pattern per [[cipherocto-design-principles]] §Extension over enumeration).
- `pub fn register_into(registry: &Registry, scheme_id: Uuid, handler: Arc<dyn Handler>)` — variant registration helper that lets a downstream crate populate the `scheme_id`-specific dispatch under `TransportKind::Raw(scheme_id)`.
- TV-AGT25 — `agent attach` via Raw scheme UUID (substrate-side fail-CLOSED test asserts `AttachError::Internal` returned when no downstream crate registered).

#### §F.7.3 — `octo-runtime-transport-hybrid`

Layer D extension crate at `crates/octo-runtime-transport-hybrid/`:

- `Cargo.toml`: `octo-runtime = { path = "../octo-runtime", version = "0.1.0" }` (no Layer D I/O deps — multiplexer is pure dispatch). Layer model = D.
- `pub struct HybridHandler { primary_kind: TransportKind, fallback_kind: TransportKind, registry: Arc<Registry> }` — implements `Handler` trait. The Hybrid handler owns a reference to the supplied registry (not new handlers) so it can look up the primary + fallback handlers at `bind()` time.
- `fn bind(&self, token: &AttachHandle, since_unix: u64) -> Result<AttachedSession, AttachError>` — primary lookup via `self.registry.lookup(&self.primary_kind)` (`TransportKind` is `Copy`, so `&self.primary_kind` borrows without `.clone()`); on `Some(primary)`, attempt `primary.bind(token, since_unix)`; on primary failure (substrate error), fallback lookup + `fallback.bind(token, since_unix)`; aggregate error wraps both error variants per §F.4 substrate error contract (the wrapping preserves operator observability of both attempts).
- `pub fn register_into(registry: Arc<Registry>, dispatch_kind: TransportKind, primary_kind: TransportKind, fallback_kind: TransportKind)` — convenience init fn that registers `HybridHandler::new(primary_kind, fallback_kind, Arc::clone(&registry))` under the caller-chosen `dispatch_kind` discriminator. The `registry` parameter is `Arc<Registry>` (not `&Registry`) because `Registry` does not implement `Clone` (the inner `RwLock` prevents that) — the hybrid needs an owned `Arc<Registry>` to look up primary + fallback handlers at `bind()` time. The hybrid dispatches against the registry it was constructed with — operators register the primary + fallback handlers FIRST, then the hybrid multiplexer under the `dispatch_kind` of their choice.
- TV-AGT26 — `agent attach` via Hybrid (primary InProcess + fallback UnixSocket) loopback test inverts the multiplexer dispatch from RED to GREEN

#### §F.7.4 — Cross-process event bridging

Cross-process event delivery from the spawn-side runtime to the attach-side `AttachedSession.broadcast_rx` is a Layer D concern, not a substrate concern. The substrate's process-singleton session registry (per §F.2 step (e) `HANDLE_TRANSPORT_REGISTRY` discipline) is process-scoped; cross-process propagation lands via the Phase C paired follow-on amendment that introduces the `RevocationStore` substrate (§F.7.5) backed by the Stoolap fork ledger.

Each Layer D extension crate returns `AttachedSession` (per §F.7.1) where `broadcast_rx` is wired by the Layer D crate itself (the unix handler subscribes to a Stoolap pubsub channel for `agent_id` and pipes events into a local broadcast per §F.7.5 `StoolapRevocationStore` concern). The substrate does not prescribe the wiring mechanism — that's a Layer D concern per §Layer direction. The Phase C amendment **promotes the in-memory revocation set behind a trait-dispatched `RevocationStore`** so Layer D crates can swap in a Stoolap-backed store that survives process boundaries:

- Substrate (`octo-runtime` Layer B) exposes `RevocationStore` trait + default impl `InMemoryRevocationStore` (the existing `revocation_set()` per §F.3 promoted behind the trait).
- New crate `octo-runtime-revocation-store` (Layer D per per-extension crate pattern; trait lives in `octo-runtime` Layer B) provides `StoolapRevocationStore` impl that writes through to the Stoolap fork ledger at `rev = "527e8eb"` (CipherOcto Stoolap fork per [[feedback_stoolap_persistence]]).
- `octo-runtime` defaults to `InMemoryRevocationStore` (zero new deps); operators opt-in to `StoolapRevocationStore` by registering the alternative store via the substrate's `install_revocation_store_default_with` factory façade at startup.
- Cross-process test **TV-AGT27** (distinct from TV-AGT23 §F.7.1 event-bridge happy path) inverts from RED (Phase B `AttachError::Internal`) to GREEN by spawning two processes (spawn-side + attach-side) over UnixSocket loopback, where the spawn-side `revoke_attach_token` writes through to the Stoolap ledger and the attach-side `is_token_revoked` reads from the same ledger directly (not via the unix handler — see §F.7.5 test spec).

#### §F.7.5 — Cross-process revocation substrate

**Architecture decision (paired amendment):** cross-process revocation propagation is implemented via a **trait-dispatched `RevocationStore` substrate** backed by the Stoolap fork ledger. The trait lives in `octo-runtime` Layer B; the Stoolap-backed impl lives in the new `octo-runtime-revocation-store` crate (Layer D per per-extension crate pattern per [[cipherocto-design-principles]] §User extensibility — the "per-extension crate" category in §Layer model covers both transport adapters (the prior Phase B precedent) and persistence extensions sharing the trait-in-core / impl-in-extension / registry-in-core topology).

Substrate additions (`crates/octo-runtime/src/persistence.rs`):

- `pub trait RevocationStore: Send + Sync + std::fmt::Debug`:
  - `fn revoke_attach_token(&self, session_id: SessionId) -> Result<(), AttachError>`
  - `fn is_token_revoked(&self, session_id: &SessionId) -> bool` — ledger-backed existence check.
  - `fn kind(&self) -> &'static str` — per-impl diagnostic identity (returns e.g. `"InMemoryRevocationStore"` or `"StoolapRevocationStore"`); used by the substrate's `tracing` logs on fail-CLOSED paths.
- Two-slot singleton pattern (avoids the L944 reviewer deadlock):
  - `pub static DEFAULT_REVOCATION_STORE: OnceLock<Arc<InMemoryRevocationStore>>` — lazily-initialized default; never written by `set_revocation_store`.
  - `pub static ACTIVE_REVOCATION_STORE: OnceLock<Arc<dyn RevocationStore>>` — set by `set_revocation_store` / `install_revocation_store_default_with`; single-shot (subsequent calls return `Err(AttachError::Internal("revocation store already installed"))`).
  - `fn current_revocation_store() -> Arc<dyn RevocationStore>` (module-private) — reads `ACTIVE_REVOCATION_STORE.get()` first; on miss, calls `DEFAULT_REVOCATION_STORE.get_or_init(...)` to lazily-initialize the default and returns it as `Arc<dyn RevocationStore>` WITHOUT writing to `ACTIVE_REVOCATION_STORE` (the default install does NOT block subsequent `set_revocation_store` overrides).
- `pub fn set_revocation_store(store: Arc<dyn RevocationStore>) -> Result<(), AttachError>` — writes to `ACTIVE_REVOCATION_STORE.set(store)`. On double-install, returns `Err(AttachError::Internal("revocation store already installed"))` (uses `AttachError::Internal(String)` per §F.4 slot 62, NOT `AttachError::RevocationError(String)` per §F.4 slot 56 — `RevocationError` is reserved for runtime revocation failures, not init failures). This is a NEW substrate discipline distinct from `HANDLE_TRANSPORT_REGISTRY` per §F.2 step (e) (which is `OnceLock::get_or_init` lazy-init + `Registry::register` last-write-wins); the difference is intentional because revocation-store init is a one-shot operator decision, not an idempotent extension registration.
- `pub fn install_revocation_store_default_with<F>(factory: F) -> Result<(), AttachError> where F: FnOnce() -> Result<Arc<dyn RevocationStore>, AttachError>` — factory-mediated init fn; routes through `set_revocation_store(store)`. The extension crate provides the factory; the substrate owns the registration. C → B → D wiring via factory closure (see Layer direction diagram below). Layer B has NO compile-time dependency on Layer D; the factory type-erases the D impl via `Arc<dyn RevocationStore>`.
- `pub fn revoke_attach_token(session_id: SessionId) -> Result<(), AttachError>` — REWRITTEN to call `current_revocation_store().revoke_attach_token(session_id)`. Lazy-init of `DEFAULT_REVOCATION_STORE` on first call when `set_revocation_store` was never invoked.
- `pub fn is_token_revoked(session_id: &SessionId) -> bool` — REWRITTEN to call `current_revocation_store().is_token_revoked(session_id)`. Same lazy-init pattern.
- `pub struct InMemoryRevocationStore { inner: RwLock<HashSet<SessionId>> }` — additive substrate type; carries the existing §F.3 fork-fail-closed semantics (poisoned `RwLock` returns `true` for `is_token_revoked`); promoted behind the trait (no behavior change for default consumers). Each `InMemoryRevocationStore` instance owns its own `RwLock`; the `Arc<dyn RevocationStore>` singleton (or the `Arc<InMemoryRevocationStore>` static default) carries the chosen instance for the lifetime of the process. `InMemoryRevocationStore::kind()` returns `"InMemoryRevocationStore"`.
- `pub(crate) fn with_inner<F, R>(&self, f: F) -> R where F: FnOnce(&HashSet<SessionId>) -> R` — test injection hook on `InMemoryRevocationStore`, preserving the prior test injection intent (the `with_revocation_set<F, R>` test seam removed from earlier §F.3 drafts per R1.5 LOW fix); tests obtain a concrete `Arc<InMemoryRevocationStore>` reference via `DEFAULT_REVOCATION_STORE.get_or_init(|| Arc::new(InMemoryRevocationStore::default()))` and invoke `with_inner` on it.
- 8 new tests in `persistence.rs::tests` covering trait dispatch + default impl + idempotent revoke + poisoned-lock fail-CLOSED + `with_inner` test hook + `set_revocation_store` re-init rejection + two-slot singleton isolation (default installs do NOT block override) + `install_revocation_store_default_with` factory closure path.

New crate `crates/octo-runtime-revocation-store/` (Layer D per-extension pattern; trait lives in `octo-runtime` Layer B):

- Cargo.toml: `stoolap = { git = "...", rev = "527e8eb" }` (CipherOcto fork per [[feedback_stoolap_persistence]]) — additive dep with rationale comment per [[cipherocto-design-principles]] §Crate dependency rationale. The Stoolap fork in this position is a **frozen external primitive** (fork-pinned, non-crypto); it does not host cipherocto business schema beyond the single revocation table (HARD RED LINE per [[stoolap-general-purpose-db]]).
- `pub struct StoolapRevocationStore { db: RwLock<stoolap::Database> }` — opens / creates the ledger under `~/.local/share/octo/runtime/revocation.stoolap` (or `$CIPHEROCTO_DATA_DIR/revocation.stoolap`). Send + Sync + Debug bounds derive from `stoolap::Database: Send + Sync + Debug` at the pinned rev (verified at implementation time before merge; if the fork does not satisfy these bounds, the impl is blocked at compile time, not at runtime).
- Ledger schema (single table, plain Stoolap convention — no table-level `STRICT` qualifier; Stoolap fork `STRICT` keyword support is unverified at rev 527e8eb and dropped per substrate-discipline no-unverified-features rule). The `revoked_at_unix` column is an audit timestamp (no current logic consumer — `is_token_revoked` uses the `session_id` PK existence check); reserved for future TTL / audit-read paths:
  ```sql
  CREATE TABLE IF NOT EXISTS revocation (
      session_id BLOB(32) NOT NULL PRIMARY KEY,
      revoked_at_unix INTEGER NOT NULL
  );
  ```
- `impl RevocationStore for StoolapRevocationStore`:
  - `kind` returns `"StoolapRevocationStore"`.
  - `revoke_attach_token` — `INSERT OR IGNORE INTO revocation VALUES (?, ?)` (idempotent — re-revoke is a no-op so concurrent revokers don't conflict). Returns `AttachError::PersistenceError(reason)` per §F.4 slot 57 on ledger write failure.
  - `is_token_revoked` — `SELECT 1 FROM revocation WHERE session_id = ? LIMIT 1` (ledger-backed existence check). Returns `true` on ledger read failure (fail-CLOSED per §Failure semantics below); logs at ERROR level via `tracing::error!` (with `kind()` value) before the fail-CLOSED return so substrate observability is preserved per [[cipherocto-design-principles]] §Push complexity to edges.
- `pub fn install_default() -> Result<Arc<dyn RevocationStore>, AttachError>` — opens the ledger + returns the constructed `StoolapRevocationStore` as `Arc<dyn RevocationStore>`. **Does NOT self-register** — the caller (Layer C via Layer B façade) decides how to register; see CLI wiring below. This shape enables the factory closure pattern (the extension crate exports the constructor; Layer B owns the registration; Layer C wires the dependency injection via the factory).
- 6 tests covering `kind()` return value + INSERT OR IGNORE idempotence + existence-check fast-path + ledger persistence across reopens + schema bootstrap + Send/Sync compile-time bounds.

**CLI wiring (concrete — replaces the prior §F.6 cross-reference that did not name a wiring point):**

```rust
// crates/octo-cli/src/main.rs
fn main() {
    #[cfg(feature = "revocation-store-stoolap")]
    {
        if let Err(e) = octo_runtime::install_revocation_store_default_with(
            octo_runtime_revocation_store::install_default,
        ) {
            tracing::warn!(
                reason = %e,
                "revocation store install failed; falling back to InMemoryRevocationStore (process-local revocation, no cross-process propagation)"
            );
        }
    }
    // ... rest of CLI bootstrap
}
```

The Cargo.toml `revocation-store-stoolap` feature is opt-in per per-extension crate pattern; default CLI builds use the zero-dep `InMemoryRevocationStore` default. Cross-process revocation propagation is opt-in per operator decision.

**Layer direction (per [[cipherocto-design-principles]] §Layer model + §User extensibility):**

```
crates/octo-runtime-revocation-store/   (Layer D, per-extension crate)
  └─> octo-runtime (Layer B) — for `RevocationStore` trait + `install_revocation_store_default_with` factory façade
  └─> stoolap (frozen external primitive — fork-pinned rev "527e8eb", non-crypto per [[stoolap-general-purpose-db]])
crates/octo-runtime/                    (Layer B)
  └─> no reverse deps (no compile-time dep on Layer D; the factory façade accepts `Arc<dyn RevocationStore>` from any caller)
crates/octo-cli/                        (Layer C)
  └─> depends on octo-runtime (Layer B) — always
  └─> depends on octo-runtime-revocation-store (Layer D) — when feature `revocation-store-stoolap` is enabled
  └─> runtime wiring: `octo_runtime::install_revocation_store_default_with(octo_runtime_revocation_store::install_default)`
      Direction at wiring: C → B → D via the factory closure (B owns the registration, D owns the constructor, C wires the dependency injection). The compile-time C → D edge is feature-gated per per-extension crate pattern (the C → B → D runtime direction is the canonical "extension register at startup" topology in §User extensibility).
```

**Cross-process test TV-AGT27 (Phase C paired follow-on):** spawns two `octo` CLI processes connected via UnixSocket loopback; both processes install `StoolapRevocationStore` against the shared ledger path:

1. Spawn-side: `octo agent run --detach --token-file /tmp/tok.bin` (writes the token file; spawns the runtime; spawn-side calls `install_default` on startup → ledger open at `$CIPHEROCTO_DATA_DIR/revocation.stoolap`).
2. Attach-side: `octo agent attach --token-file /tmp/tok.bin` (reads the token file; binds via `UnixSocketHandler::bind` to the spawn-side process; attach-side ALSO calls `install_default` on startup → opens the SAME ledger at the SAME path).
3. Operator-side: `octo agent revoke-attach --session-id <HEX64>` (a SEPARATE CLI process — NOT the spawn-side `octo agent run --detach` from step 1 — invokes `revoke_attach_token` → `StoolapRevocationStore::revoke_attach_token` writes through `INSERT OR IGNORE`).
4. Attach-side: calls the free function `octo_runtime::is_token_revoked(&session_id)` (which dispatches via the attach-side's OWN installed store, routing through `current_revocation_store()`); sees the revocation propagated across the process boundary via the shared Stoolap ledger. The unix handler owns the §F.7.1 event-bridge path (TV-AGT23); it is NOT exercised by this step.

Test harness lives in `crates/octo-runtime-revocation-store/tests/cross_process.rs`; spawns two child processes (one per side) sharing a tempdir ledger path. Cross-process consistency: eventual (the Stoolap fork uses synchronous commits per `stoolap::Database::exec`); the attach-side observes the spawn-side's revoke within milliseconds (subprocess latency bound).

**Substrate discipline preserved (per [[cipherocto-design-principles]] §Attenuation invariants cross boundaries):**

- The substrate's `revoke_attach_token` + `is_token_revoked` semantics are UNCHANGED (same return types, same error variants, same fork-fail-closed contract for the default `InMemoryRevocationStore`).
- The Stoolap fork NEVER hosts cipherocto business schema beyond the single revocation table (HARD RED LINE per [[stoolap-general-purpose-db]]); the ledger is a single-table cross-process primitive, not a general-purpose DB.

**Performance characteristics (non-discipline):**

The trait dispatch adds one vtable call. The vtable call cost is single-digit-ns; the Stoolap impl's ledger query is microsecond-bound. Default `InMemoryRevocationStore` consumers see no hot-path cost change (the trait dispatch degenerates to a single vtable call plus the existing `RwLock` lookup). The `StoolapRevocationStore` is opt-in for cross-process operators; the substrate performance baseline per §F.3 is unchanged for in-memory consumers.

**Failure semantics (fail-CLOSED per [[cipherocto-design-principles]] §Push complexity to edges):**

- `StoolapRevocationStore::is_token_revoked` returns `true` on ledger read failure (e.g., DB corruption, I/O error, schema drift) — fail-CLOSED preserves the explicit-operator-revocation guarantee; logs at ERROR via `tracing::error!` (with `kind()` value) before the fail-CLOSED return.
- `StoolapRevocationStore::revoke_attach_token` returns `AttachError::PersistenceError(reason)` per §F.4 slot 57 on ledger write failure — the operator's `octo agent revoke-attach` propagates the failure visibly rather than silently succeeding.
- `install_revocation_store_default_with` failure fallback: WARN log + retain default `InMemoryRevocationStore` + the dispatch continues with `ACTIVE_REVOCATION_STORE` empty (calls fall back to `DEFAULT_REVOCATION_STORE`). The CLI's main.rs wrapper applies the SAME WARN log + `tracing::warn!` fallback (canonical wording: "revocation store install failed; falling back to InMemoryRevocationStore (process-local revocation, no cross-process propagation)").

### §F.8 Per-Extension Crate Manifest Spec

Per-extension crates follow a uniform manifest template:

```toml
[package]
name = "octo-runtime-transport-{unix,raw,hybrid}"
version = "0.1.0"
edition = "2021"
layer = "D"

[dependencies]
octo-runtime = { path = "../octo-runtime", version = "0.1.0" }
# Layer D owned I/O deps as needed (none for raw, none for hybrid; unix uses std::os::unix::net in Phase B client-side only — server-side piping lives in the Phase C follow-on per §F.7.4)
```

Module structure per crate:

- `src/lib.rs` — `Handler` impl + `register_into(registry: &Registry)` init fn + tests
- `tests/` — integration tests (loopback for unix, fail-CLOSED for raw, multiplexer for hybrid)

Workspace integration: each crate added to workspace `Cargo.toml` `[members]` array (the `["crates/*"]` glob covers new `crates/octo-runtime-transport-*` dirs without explicit manifest edit).

Per-extension crate tests verify:

- `Handler` trait wiring (impl signature matches core trait)
- Registry registration round-trip (`register_into` + `Registry::lookup`)
- Per-handler error mapping to `AttachError` variants
- Init fn is idempotent (re-calling `register_into` overwrites prior registration per `Registry::register` discipline per §F.2)

## Rationale

### Why five subcommands

Five tightly-scoped subcommands mirror the canonical agent lifecycle:
create → run → list → destroy → attach. Each subcommand is a single
operator action; splitting them keeps the clap surface flat
(no omnibus verb). Future subcommands (e.g., pause, resume, migrate)
land as additive variants on `AgentAction` per the no-central-enums
principle.

### Why `schema_version = 4` (post-divergence)

Per the RFC-0011-e Output Envelope divergence pattern (parent `OutputEnvelope` vs amendment-specific envelope), each amendment that diverges from the parent
envelope inherits its slot from the slot table. RFC-0011-f and
RFC-0011-g already occupy slot 4 (additive divergence only). Pinning
RFC-0011-c to slot 4 keeps the slot table monotonic and avoids
fragmenting the version space; field renames (`data`→`payload`,
`generated_at`→`executed_at_unix`, `preview_only`→`redacted`)
are additive to the v4 surface.

### Why exit code slots 39-61

Per the slot allocation table, RFC-0011-c consumes slots 39-61
(post -g's 35-38; 14 base plus 8 follow-on AttachHandle/AttachSession
per §Follow-on §F.4 mirror (8 unique slots), plus 1 CLI dispatch
`TokenMintSkipped` per §F.6.1, plus 1 boundary-parse
`InvalidSessionIdHex` per §9.3.6, plus 1 typed-discriminator
`ReplayDetected` per §9.7 — totaling **25 variants across 22
occupied slots** in the 23-slot range 39-61, 3 shared-slot
pairings (43, 51, 53)). Sibling amendments that do not consume
slots MUST NOT claim earlier slots. Renegotiation is required if
-h/i follow-on amendments claim earlier slots.

### Why state machine aliasing ACTIVE↔BUSY

Per RFC-0002 §Agent State Machine, `ACTIVE` is the registered-and-idle
state; `BUSY` is `ACTIVE` + runtime attached. The CLI collapses
`run` to a single two-transition sequence (REGISTERED → ACTIVE →
BUSY) because the substrate enforces each transition separately
and the CLI is purely an orchestration layer; collapsing would
bypass substrate validation.

## Determinism Requirements

Inherited from RFC-0011 §Determinism Requirements (exit code
stability, JSON field order, RFC 3339 UTC timestamps). Additive:
`command` field at position 2 in `OutputEnvelope` (string, no
semantic dependency on timestamp); `redacted` field at
position 5 (bool, surfaces `OctoCliRedactor` alteration).

## Lifecycle Requirements

No stateful actors in this RFC; the CLI binds to RFC-0002 §Agent
State Machine (substrate-authoritative). CLI surface mapping:
§9.5. State transitions are substrate-rejected if invalid; the
CLI surfaces `InvalidStateTransition` (exit 43) verbatim.

## Compatibility

Per RFC-0011 (parent compatibility window per Status blockquote), the existing `octo agent` stub command
(unchanged from RFC-0011 Phase 1) is deprecated. The timeline is:

| Version | Behavior                                                                                 |
| ------- | ---------------------------------------------------------------------------------------- |
| v1.0    | Stub emits `StaleStub` warning to stderr + exit code 65; subcommand group also available |
| v1.1    | Stub hard errors with `StaleStub` (exit 65); subcommand group is the only surface        |
| v2.0    | Stub binary path removed entirely                                                        |

The CLI follows the parent stub-deprecation compatibility window — no
version shorthand in prose, only RFC numbers + section refs.

### Stale-stub window env-var override (v1.1 hard-error opt-in)

Per the RFC-0011 stub-deprecation pattern (see compatibility timeline), the operator
may set `OCTO_STALE_STUB_WINDOW=1` to opt in to the v1.1 hard-error
behavior ahead of the version bump. Setting this env-var is the
defensive lever for operators who want to validate their migration
script before the v1.1 release.

## Impact

### Breaking Changes

- **None for operators** — the new subcommands are additive. Operators
  who do not invoke `octo agent <subcommand>` are unaffected.
- **Stub deprecation** — the legacy `octo agent` stub (RFC-0011
  §Compatibility) emits `StaleStub` (exit 65) starting at CLI v1.0.
  Operators relying on the stub must migrate before v1.1.
- **New exit codes** — exit codes 39–59 added to the reserved
  17–63 range (RFC-0011 §Exit Codes; base amendment 39–52
  plus follow-on amendment 53–59 per §9.8). Existing exit codes
  unchanged.

### Privacy Considerations

- **Holder DID redaction** — `holder_did` redacted unless it matches
  `active_did()`. Prevents leaking the holder's DID in shared
  terminal scrollback or `--json` output piped to logs.
- **Capability root redaction** — `capability_root` always truncated
  to first 8 hex chars + `...`. Prevents leaking macaroon
  identifiers that could aid capability-thief attacks (Adversary A2).
- **Audit log entry redaction** — the `audit_log_entry` digest is
  not redacted (operator-owned info), but the underlying audit log
  content remains in the substrate, not the CLI.

## Economic Analysis

This amendment does not introduce any new economic surface. The five
subcommands are operator-UX wrappers; no fees, no token minting, no
burn events. The substrate crates (`octo-wallet`, `octo-runtime`)
that these subcommands wrap MAY have their own economic surface
(e.g., per-RPC gas); that surface is out of scope for this RFC and
is governed by the respective substrate RFC.

## Version History

| Version | Date       | Status   | Changes                                                                                                                                                                           |
| ------- | ---------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-31 | Draft    | Initial draft; 5 subcommands; 13 test vectors (TV-AGT1..TV-AGT13); 6 missions (5 subcommand + 1 substrate); stub deprecation timeline                                             |
| v1.0.1  | 2026-08-31 | Draft    | Legacy numbering note appended; per-section anchor for the "Phase 4 agent lifecycle" reference                                                                                    |
| v1.1    | 2026-08-31 | Draft    | Schema divergence + slot allocation + [ADD] contract + IDLE fix                                                                                                                   |
| v1.2    | 2026-08-31 | Accepted | Promotion Draft → Accepted after 6-wave review loop (32+17+8+4+0+0 findings); 53 fixes applied; 6 mission YAMLs preserved in missions/open/; Authorship Note placeholder stripped |

## Related RFCs

- RFC-0011 — `octo` CLI Substrate (parent)
- RFC-0002 — Agent Manifest, Capability, and Lifecycle Substrate
- RFC-0009 — Identity Substrate
- RFC-0957 — Macaroon Substrate (capability witness format; revocation)
- RFC-0011-a — Audit Substrate (consumed by `agent destroy`)
- RFC-0011-b — Reputation Substrate (optional `reputation_score` field)
- RFC-0011-d — Role Provisioning (gates `agent run` if role-gated)
- RFC-0011-e — Vault Operations (sibling amendment)
- RFC-0011-f — Mesh Operations (sibling amendment)
- RFC-0011-g — Governance (sibling amendment)
- RFC-0008 — Deterministic AI Execution Boundary (execution class)
- RFC-0960 — Caveat Catalog (consumed by capability validation)
- RFC-0967 — Policy Object Graph (consumed by capability validation)

## References

- [[cipherocto-design-principles]] — Layer B stability contract;
  no-parallel-abstractions; extension-over-enumeration
- [[no-line-refs-anywhere]] — §section refs only; no file:line in prose
- [[feedback_initiation_user_only]] — user initiates push / commit / PR
- [[git-workflow]] — commits free, push + remote writes need explicit
  user instruction
- [[no-phantom-mission-pointers]] — `depends_on:` cites real missions
  or RFC numbers
- [[deferred-vs-unspecified]] — Deferred ≠ Unspecified
- [[rfc-0011-loop-dry-gate-closure]] — parent chain closure pattern
- Parent stub-deprecation compatibility window — stub-deprecation
  timeline for `octo agent` legacy stub group

---

**Submission Date:** 2026-08-31
**Acceptance Date:** 2026-08-31
**Last Updated:** 2026-08-31
**Changes:**

- 2026-08-31 — Promoted Draft → Accepted per BLUEPRINT.md §RFC Acceptance Process (file moved to `rfcs/accepted/process/`; Status header updated to Accepted; VH row v1.2 appended documenting 6-wave review loop DRY closure; Authorship Note placeholder stripped per BLUEPRINT §RFC Process; cite hygiene sweep PASS). Review cycle satisfied: 6-wave review loop (R1=32 + R2=17 + R3=8 + R4=4 + R5=0 + R6=0 findings → DRY closure); 53 fixes applied; spec cycle R5 + review loop R6 = 2 consecutive zero-finding rounds.
- 2026-08-31 — Initial draft (v1.0 → v1.1, prior versions retained in VH)
