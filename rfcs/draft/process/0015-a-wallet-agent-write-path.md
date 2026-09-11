# RFC-0015-a: `octo-wallet` Agent Write-Path Amendment

## Status

Draft (2026-09-11)

## Authors

- Authored by `@cipherocto` per RFC-0015 amendment chain + RFC-0002 §Agent State Machine substrate authority.

## Maintainers

- Maintainer: `@cipherocto` per RFC-0015 amendment chain.

## Summary

This RFC is a **sibling amendment** to RFC-0015 carrying the **write-path surface** that RFC-0015 v2 deliberately DEFERRED:

1. **`pub fn transition_agent(caller_did: &Did, uuid: Uuid, target: AgentState, reason: Option<&str>) -> Result<AgentSummary, WalletError>`** — NEW ADDITIVE write function per §6.1. Paired with `AuditEventKind::AgentTransition` (RFC-0012-v2 substrate amendment). Substrate-faithful caller-attestation + per-`(holder_did, agent_id)` `parking_lot::Mutex::try_lock()` lock-mode (TOCTOU mitigation) + audit append + rollback contract.
2. **`WalletError::AlreadyInTransition(Uuid)`** — NEW ADDITIVE enum variant per §6.3. Raised on concurrent `transition_agent` call against the same `(holder_did, agent_id)`.
3. **`WalletError::InvalidStateTransition { from, to }`** — NEW ADDITIVE enum variant per §6.3. Raised on illegal transition (e.g., `Terminated → Terminated` per TV-WLT-AGT-5).
4. **`WalletError::AuditUnavailable`** — NEW ADDITIVE enum variant per §6.3. Raised when `AppendOnlyAuditSink` is unreachable OR fails closed (per the rollback contract in §6.1 (5)).

The three new error variants + the `transition_agent` function + the audit append of `AgentTransition` rows + the `parking_lot` Cargo.toml dep all land at RFC-0015-a acceptance paired with RFC-0012-v2 acceptance (Layer A `AuditEventKind::AgentTransition` variant addition to `octo-audit-core`).

**Pairing invariant:** Acceptance of RFC-0015-a REQUIRES paired acceptance of RFC-0012-v2 (Layer A `AuditEventKind::AgentTransition` variant in `octo-audit-core`) per CLAUDE.md §Extension over enumeration + §Layer B depends on A (stable substrate). Without RFC-0012-v2 acceptance, `transition_agent` cannot persist state-machine transitions to the canonical `AppendOnlyAuditSink` per RFC-0012.

## Dependencies

**Requires:**

- RFC-0015 — `octo-wallet` Agent Operations Substrate (sibling KEEP-only RFC; this is the write-path amendment)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers; defines write operator UX)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` + `AgentManifest` authority per §Agent State Machine; substrate-faithful drift per RFC-0015 v2 §Summary note)
- RFC-0010 — Canonical DID Codec (DID parsing for `holder_did: Did` filter field)
- RFC-0009 — Identity Management (informational; lifecycle state-machine substrate precedent)
- RFC-0011 — `octo` CLI Substrate (parent RFC; provides `OutputEnvelope<T>`, `OctoCliError`, `OctoCliRedactor`, clap root, exit code table)

**Substrate amendment dependencies (REQUIRED for RFC-0015-a acceptance):**

- **RFC-0012-v2** — `AuditEventKind::AgentTransition { agent_id, from, to, reason }` variant addition to `octo-audit-core` (Layer A frozen; parent `AuditEvent::at_millis_unix` carries the timestamp — variant payload does NOT duplicate the parent timestamp). **(DRAFT — required substrate amendment; without this, `transition_agent` cannot persist state-machine transitions to the canonical `AppendOnlyAuditSink` per RFC-0012.)**

**RFC-0015 paired consumption:**

- The NEW ADDITIVE `WalletError::{AlreadyInTransition(Uuid), InvalidStateTransition { from, to }, AuditUnavailable}` variants land in `crates/octo-wallet/src/error.rs` alongside the KEEP variants added by RFC-0015 v2. No re-promotion needed across phases.

## Design Goals

1. **Substrate-faithful** — function names, parameter shapes, return types match the canonical CLI consumer contracts in RFC-0011-c §9.3 (no parallel abstractions per [[cipherocto-design-principles]]).
2. **State-machine substrate authority** — `transition_agent` is the substrate-level guard for `AgentState` transitions; CLI cannot bypass. Replay protection + idempotency live in the substrate, not the CLI.
3. **TOCTOU mitigation via explicit lock-mode** — per-`(holder_did, agent_id)` `parking_lot::Mutex::try_lock()` AFTER holder_did resolution; second caller observes `WalletError::AlreadyInTransition` immediately (non-blocking; no poisoned-lock panics).
4. **Audit append + rollback contract** — every successful transition appends an `AuditEventKind::AgentTransition` row to `AppendOnlyAuditSink` per RFC-0012-v2. On fail-closed audit-append failure (`SinkSpecific` OR `SequenceGap`) the in-memory state is rolled back; on `AlreadyExists` (idempotent re-append) the prior success is recognized and the existing chain-hash is returned.
5. **Caller-attestation discipline** — `caller_did: &Did` is borrowed from an HSM-bound `IdentityHandle` at the Layer B façade boundary; substrate enforces `caller_did == agent.holder_did` (HIGH sec fix; same discipline as RFC-0015 v2 read path).
6. **Reason length + control-char filter reuse** — `transition_agent` calls `validate_reason` (RFC-0015 v2 KEEP primitive) upstream; the primitive's `WalletError::{ReasonContainsControlChars(String), ReasonTooLong(usize)}` errors propagate through unchanged.

## Motivation

RFC-0015 v2 KEEP accepts the read path (`list_owned_agents` + `lookup_agent` + `validate_reason`) + 4 KEEP `WalletError` variants. The CLI missions `0011-c-agent-{run,destroy}-subcommand` reference substrate functions on `octo-wallet` for state-machine writes that do not exist in the substrate as of 2026-09-11. Hard-checked via `grep -rE "pub (fn|async fn) " crates/octo-wallet/src/`:

- `octo_wallet::transition_agent` — **MISSING**; CLI consumers `0011-c-agent-{run,destroy}-subcommand` Sub-step 3 / Sub-step 4 cannot dispatch state transitions.

RFC-0015-a closes this gap by adding the write function substrate-faithfully + paired `WalletError` variants + paired audit append. **The substrate addition is the unblock**; the missions then proceed with their existing plans.

## Roles and Authorities

| Role                | Authority                                                                               | Audit trail                              |
| ------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------- |
| Operator (human/CI) | `transition_agent` (write; requires CLI confirmation gate)                              | CLI log + audit append per RFC-0012-v2   |
| Wallet substrate    | Source of truth for `AgentState`; rejects invalid transitions; surfaces reason in error | Internal state machine log               |
| Audit substrate     | Appends `AgentTransition` event to `AppendOnlyAuditSink` per RFC-0012-v2                | Append-only audit log (BLAKE3-256 chain) |
| CLI (octo-cli)      | Operator UX over substrate functions; never bypasses substrate state-machine            | Same as operator                         |

## Specification

### §6.1 `transition_agent`

```rust
/// State-machine transition for an agent owned by the caller-attested DID.
///
/// DEFERRED to RFC-0015-a acceptance (paired with RFC-0012-v2 acceptance).
///
/// SECURITY (HIGH — caller-attestation pattern per RFC-0015 v2 §6.2.1):
/// the caller MUST pass the active DID as `caller_did` (NOT derived from
/// process state). The substrate re-validates that `agent.holder_did ==
/// caller_did` and returns `WalletError::ForbiddenHolderMismatch` on
/// mismatch (multi-DID enumeration prevention).
///
/// TOCTOU mitigation: the substrate acquires a per-`(holder_did,
/// agent_id)` `parking_lot::Mutex::try_lock()` AFTER looking up
/// `holder_did` by `uuid`; on contention the second caller observes
/// `WalletError::AlreadyInTransition(Uuid)` immediately (non-blocking;
/// §6.1 (7) single-callsite-per-agent). The first caller's lock is held
/// across reason filter + state-machine work + audit append and released
/// after the audit append completes (or rolls back). `current_state` is
/// re-read INSIDE the lock.
///
/// Idempotency carve-out (NOT idempotent on terminal): `transition_agent(
/// caller_did, uuid, current_state, _)` on a `Terminated` state returns
/// `WalletError::InvalidStateTransition { from: Terminated, to:
/// current_state }` per TV-WLT-AGT-5 (replay-attack guard). Same-state
/// transitions on non-terminal states (`Registered → Registered`,
/// `Running → Running`) remain idempotent no-ops (no audit append).
///
/// Audit append + rollback contract (paired with RFC-0012-v2
/// `AuditEventKind::AgentTransition`): every successful non-idempotent
/// transition appends an `AuditEventKind::AgentTransition { agent_id,
/// from, to, reason }` row to the canonical `AppendOnlyAuditSink` per
/// RFC-0012 (parent `AuditEvent::at_millis_unix` carries the timestamp
/// — variant payload does NOT carry `at_unix`). On ANY
/// `octo_audit::error::AuditError` variant returned from `append_audit_event`:
/// - `AuditError::SinkSpecific(_)` — transient sink failure, ROLLBACK
///   in-memory state + propagate as `WalletError::AuditUnavailable`.
/// - `AuditError::SequenceGap { event_id, prev }` — preceding audit
///   rows missing, ROLLBACK + propagate as `WalletError::AuditUnavailable`
///   (fail-closed).
/// - `AuditError::AlreadyExists(event_id)` — IDEMPOTENT RE-APPEND (prior
///   call succeeded); do NOT rollback; recognize previous success, return
///   existing chain-hash as success. Do NOT propagate as failure.
pub fn transition_agent(
    caller_did: &Did,
    uuid: Uuid,
    target: AgentState,
    reason: Option<&str>,
) -> Result<AgentSummary, WalletError>;
```

- **(1) TOCTOU mitigation** — `parking_lot::Mutex::try_lock()` AFTER holder_did resolution; non-blocking; second caller observes `AlreadyInTransition` immediately. The lock is held across reason filter + state-machine work + audit append and released after the audit append completes (or rolls back). `current_state` is re-read INSIDE the lock. The lock mode is `parking_lot::Mutex::try_lock()` (non-blocking; immediate `AlreadyInTransition` return on contention) — NOT `lock()` (blocking) which would risk substrate-internal deadlock under load. **Rationale for `parking_lot` over std:** `parking_lot::Mutex` provides `try_lock()` without poisoning — Rust std `Mutex::try_lock()` poisons on holder-panic; parking_lot returns lock error without poisoning the lock state, allowing graceful Phase 2.5 deferred-write-path fallback to canonical substrate `WalletError::AlreadyInTransition` rather than poisoned-lock panics.
- **(2) Caller-attestation** — `caller_did: &Did` is REQUIRED; substrate rejects when `caller_did != agent.holder_did` (returns `WalletError::ForbiddenHolderMismatch` per RFC-0015 v2 §6.2.4). Closes the same multi-DID enumeration attack surface that `list_owned_agents` + `lookup_agent` defend against per RFC-0015 v2 §6.2.1 + §6.2.5 (lookup_agent); the substrate treats read + write paths with the same caller-attestation discipline. **Provenance invariant:** `caller_did` MUST be sourced from the active `IdentityHandle` (HSM-bound per RFC-0009 §Identity Struct + §HsmAdapter Integration) at the Layer B façade boundary. The substrate does NOT authenticate `caller_did` itself — it only enforces `caller_did == agent.holder_did`. A fabricated `caller_did` (compromised CLI passing an arbitrary DID string) would pass the substrate check trivially. The CLI / programmatic caller is the trust boundary for DID provenance.
- **(3) State-machine authority** — substrate rejects invalid transitions with `WalletError::InvalidStateTransition { from: AgentState, to: AgentState }`. The `#[non_exhaustive]` enum attribute permits future expansion (e.g., `Paused`, `Draining` per RFC-0002-v2 amendment). Canonical transitions per RFC-0002 §Agent State Machine substrate diagram: `Registered → Running`, `Running → Terminated`, `Running → Registered`; `Terminated` is terminal.
- **(4) Idempotency carve-out** — `transition_agent(caller_did, uuid, current_state, _)` on a **terminal** state (`Terminated`) is **NOT** idempotent (would mask replay attacks); it returns `WalletError::InvalidStateTransition { from: Terminated, to: current_state }` per TV-WLT-AGT-5. Same-state transitions on non-terminal states (`Registered → Registered`, `Running → Running`) remain idempotent no-ops.
- **(5) Audit append + rollback contract** — every successful non-idempotent transition appends an `AuditEventKind::AgentTransition { agent_id, from, to, reason }` row to the canonical `AppendOnlyAuditSink` per RFC-0012-v2 (parent `AuditEvent::at_millis_unix` carries the timestamp — variant payload does NOT carry `at_unix`). Substrate-faithful symmetry: parent struct is the canonical timestamp carrier; variants carry transition-specific payload only. On ANY `octo_audit::error::AuditError` variant returned from `append_audit_event`:
  - `AuditError::SinkSpecific(_)` — transient sink failure, ROLLBACK + propagate as `WalletError::AuditUnavailable`.
  - `AuditError::SequenceGap { event_id, prev }` — preceding audit rows missing, ROLLBACK + propagate as `WalletError::AuditUnavailable` (fail-closed).
  - `AuditError::AlreadyExists(event_id)` — IDEMPOTENT RE-APPEND: the `event_id` already persisted (prior call succeeded). Do NOT rollback; recognize previous success, return existing chain-hash as success. Do NOT propagate as failure.
  - Defense-in-depth: any FAIL-CLOSED audit-append failure (`SinkSpecific` / `SequenceGap`) MUST trigger rollback. The audit append is the source of truth for the transition log; in-memory state is the source of truth for current `AgentState`. The `AlreadyExists` variant is NOT a failure — it is the substrate-canonical idempotency signal.
- **(6) Reason length + control-char filter reuse** — `transition_agent` calls `validate_reason(reason)` (RFC-0015 v2 KEEP primitive per §6.2.5 validate_reason) upstream; the primitive's `WalletError::{ReasonContainsControlChars(String), ReasonTooLong(usize)}` errors propagate through unchanged. Empty string allowed; ≤256 chars enforced (longer → `WalletError::ReasonTooLong(usize)`); control characters (`U+0000`-`U+001F`, `U+007F`) are REJECTED upfront before any state-machine work. Non-UTF-8 input rejected at CLI (substrate trust). **C1 range gap (carry-over from RFC-0015 v2 §6.2.5 validate_reason):** the filter scope is `U+0000`-`U+001F` + `U+007F`; the C1 range (`U+0080`-`U+009F`) passes the filter (per TV-WLT-AGT-13f). The gap is documented as accepted residual in §Adversary Analysis + RFC-0015 v2 §Security Considerations row 1.
- **(7) Single-callsite per agent** — concurrent `transition_agent` against the same `(holder_did, agent_id)` returns `WalletError::AlreadyInTransition(Uuid)` (transient, retry-safe; CLI surfaces as substrate reason). The `WalletError::AlreadyInTransition(Uuid)` variant is NEW ADDITIVE (lands with the `transition_agent` function per §6.3); the `try_lock` semantics are the §6.1 (1) lock-mode contract. The substrate-faithful implementation uses `parking_lot::Mutex::try_lock()` (non-blocking; per §6.1 (1)) — the second caller's `try_lock` returns immediately with `WouldBlock`, which the substrate maps to `WalletError::AlreadyInTransition(uuid)`. Single-callsite-per-agent invariant is the §6.1 (1) lock-mode contract; the lock is held across the FIRST caller's reason filter + state-machine work + audit append per §6.1 (1) + §6.1 (5).

### §6.2 `TransitionReceipt` projection

```rust
/// Returned by `transition_agent` on successful non-idempotent transition.
/// Carries the audit-event coordinate so CLI consumers can cross-reference
/// the audit log (e.g., `octo audit show <audit_event_id>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionReceipt {
    /// Previous state (snapshot taken INSIDE the lock per §6.1 (1)).
    pub previous_state: AgentState,
    /// Current state after the successful transition.
    pub current_state: AgentState,
    /// Audit-event `event_id` (RFC-0012-v2 `AuditEvent::event_id`).
    pub audit_event_id: u64,
    /// Audit-event `chain_hash` (RFC-0012-v2 `AuditEvent::chain_hash`,
    /// 32-byte BLAKE3-256 digest).
    pub chain_hash: [u8; 32],
}
```

**Substrate-faithful note:** the `audit_event_id` + `chain_hash` fields carry the canonical audit-event coordinate per RFC-0012-v2 `AuditEvent`. The CLI surfaces this via `OutputEnvelope<TransitionOutput>` with the canonical RFC-0010 DID form. The `TransitionReceipt` projection is additive to `crates/octo-wallet/src/agent.rs`; no parallel abstraction to `AuditEvent` (the projection is the wallet-side re-projection of the audit-event coordinate).

### §6.3 Error envelope

`WalletError` variants (additive; `#[non_exhaustive]` is already in scope). The KEEP variants from RFC-0015 v2 are unaffected.

| Variant                                                             | Source                                                                                                                                                                                                                                                   | CLI exit                      | RFC-0011-c reference    |
| ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------- | ----------------------- |
| `AlreadyInTransition(Uuid)` (NEW, DEFERRED to RFC-0015-a)           | `transition_agent` (write-only; concurrent call against same `(holder_did, agent_id)`; paired with §6.1 (7) lock-mode contract)                                                                                                                          | `AlreadyInTransition` (43)    | §9.8 slot 43            |
| `InvalidStateTransition { from, to }` (NEW, DEFERRED to RFC-0015-a) | `transition_agent` (write-only; illegal transition per §6.1 (3) state-machine authority; NOT idempotent on terminal state per §6.1 (4) idempotency carve-out)                                                                                            | `AlreadyInTransition` (43)    | §9.8 slot 43            |
| `AuditUnavailable` (NEW, DEFERRED to RFC-0015-a)                    | `transition_agent` (write-only; `AppendOnlyAuditSink` unreachable OR fail-closed per §6.1 (5) audit append + rollback contract — `SinkSpecific` OR `SequenceGap` trigger `WalletError::AuditUnavailable`; `AlreadyExists` is NOT a failure per §6.1 (5)) | `AuditSubstrateNotReady` (52) | RFC-0011-c §9.8 slot 52 |

**R21 M-6 variant unification note:** the CLI variant for `AlreadyInTransition` + `InvalidStateTransition` is `OctoCliError::AlreadyInTransition(Uuid)` (mirrors substrate `Uuid` payload; eliminates asymmetric payload shape between substrate `Uuid` and CLI `{ from, to }` structs). The substrate-faithful principle: CLI variant payload mirrors substrate variant payload (no parallel abstractions). The slot 43 reservation per RFC-0011-c §9.8 is preserved; the variant name changes only. The substrate `{ from, to }` payload for `InvalidStateTransition` is DROPPED at the CLI boundary per the R21 M-6 variant unification; the CLI may surface the cause via the `OctoCliError::AlreadyInTransition(uuid)` Display string (includes `from: <state>` + `to: <state>` concatenated per the `AgentState` `Display` impl).

CLI mapping follows RFC-0011-a §7.4 Substrate `[ADD]` error-envelope pattern (substrate variant → CLI variant → exit code) byte-for-byte.

### §6.4 Substrate-faithful state-machine table

The table below documents the substrate-canonical state machine surface (see `AgentState` enum per `crates/octo-wallet/src/agent.rs`). RFC-0002 §Agent State Machine's spec diagram declares the five-state `ACTIVE/BUSY` working substates; the substrate collapses these to `Running`. RFC-0015-a documents both forms; CLI consumers operate on the substrate three-state form.

| Substrate variant        | Spec diagram equivalent                     | CLI rendering          | Allowed transitions FROM (substrate) |
| ------------------------ | ------------------------------------------- | ---------------------- | ------------------------------------ |
| `AgentState::Registered` | `REGISTERED`                                | `state = "registered"` | `Running`                            |
| `AgentState::Running`    | `ACTIVE` or `BUSY` (per substrate collapse) | `state = "running"`    | `Terminated`, `Registered`           |
| `AgentState::Terminated` | `TERMINATED` (terminal)                     | `state = "terminated"` | NONE (terminal)                      |

> **Substrate-faithful drift (carried over from RFC-0015 v2 §Summary "Substrate-faithful note"):** the working substate pair `ACTIVE ↔ BUSY` is collapsed into the single observable `Running` variant. Operators querying agent state see `Running` for both currently-idle (was ACTIVE) and currently-executing (was BUSY) agents. The audit row records the canonical `AgentState` transition (Running, etc.) without working/non-working substate distinction; consumers requiring the substate split must consult the registry snapshot at the audit-event timestamp. The drift is: substrate collapses for both the registry AND the audit log; the working/non-working split is unrecoverable from the audit log alone.

### §6.5 CLI integration contract

CLI missions consuming this substrate (write path):

| Mission                              | Substrate call                                                                                                       | Sub-step                                                         |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `0011-c-agent-run-subcommand.md`     | `transition_agent(caller_did, uuid, Running, None)` then `octo_runtime::spawn_agent`                                 | Sub-step 3 (NEW, **paired-DEFERRED with RFC-0015-a acceptance**) |
| `0011-c-agent-destroy-subcommand.md` | `transition_agent(caller_did, uuid, Terminated, reason)` then audit append (via `TransitionReceipt::audit_event_id`) | Sub-step 3 (NEW, **paired-DEFERRED with RFC-0015-a acceptance**) |

Each mission's accepted-state precondition is checked locally against the `AgentState` returned by `octo_wallet::register_agent` + `list_owned_agents` + `lookup_agent` calls (RFC-0015 v2 KEEP); the substrate is source of truth, not the CLI. RFC-0015-a acceptance unblocks the write-path missions (`run`, `destroy`).

### §6.6 Determinism requirements

- **Transition determinism** — `transition_agent` is idempotent on same-state (non-terminal); deterministic on first-transition success (state machine is fully ordered: `Registered ↔ Running → Terminated`).
- **Audit log integrity** — every `transition_agent` success appends a BLAKE3-256 entry to `AppendOnlyAuditSink` (RFC-0012-v2); the chain is `verify_chain`-able end-to-end via `octo_audit_core::verify_chain` (re-exported from `octo-audit`).
- **Exit codes stable** — substrate error variants map to stable CLI exit codes (see §6.3) per parent RFC-0011 §Error Handling.

### §6.7 RFC-0008 Execution Class Mapping

| Operation          | Execution class | Rationale                                                                                                     |
| ------------------ | --------------- | ------------------------------------------------------------------------------------------------------------- |
| `transition_agent` | Class B (write) | State mutation gated by substrate state machine; CLI confirmation gate per RFC-0011 §Confirmation Flag Matrix |

CLI surfaces `Class B` operations via `--confirm` per RFC-0011 §Confirmation Flag Matrix.

### §6.8 Forward Pointer — read path lives in RFC-0015 v2

The read-path surface (`list_owned_agents` + `lookup_agent` + `validate_reason` + paired `WalletError::{AgentNotFound(Uuid), ForbiddenHolderMismatch, ReasonContainsControlChars(String), ReasonTooLong(usize)}`) lives in RFC-0015 v2 KEEP. See `rfcs/draft/process/0015-wallet-agent-operations.md`.

Acceptance of RFC-0015-a does NOT authorize the read path independently; the missions in RFC-0011-c §9.3 list / show / create / attach require RFC-0015 v2 acceptance first (parallel sibling RFC). The missions in RFC-0011-c §9.3 run / destroy require RFC-0015-a acceptance.

## Performance Targets

- `transition_agent` happy path: p95 < 10ms (in-process state-machine work + audit append; substrate does not touch disk for the in-memory state mutation).
- `transition_agent` audit append (RFC-0012 chain): p95 < 2ms in-process.
- `transition_agent` lock acquisition (`parking_lot::Mutex::try_lock`): p95 < 1µs (uncontended).

## Implicit Assumptions Audit

1. **Single-writer per `(holder_did, agent_id)`** — enforced via `parking_lot::Mutex::try_lock()` per §6.1 (1) + §6.1 (7). Concurrent calls observe `WalletError::AlreadyInTransition` immediately.
2. **Substrate is canonical for `AgentState`** — CLI never pattern-matches on `AgentState` string representation; serde-derived lowercase string is for display only per `#[serde(rename_all = "lowercase")]` on the enum.
3. **Reason string is UTF-8, ≤256 chars, no control chars** — `transition_agent` calls `validate_reason` (RFC-0015 v2 KEEP primitive per §6.2.5 validate_reason) upstream; the primitive's length cap + control-char filter applies.
4. **`register_agent` precedes any transition** — substrate assumes the `agent_id` returned by `register_agent` exists in the in-memory registry before any `transition_agent` call can succeed; CLI precondition check (CLI missions `0011-c-agent-{run,destroy}-subcommand` Sub-step 2 enforces via `lookup_agent`).
5. **Audit substrate is available** — `transition_agent` requires `AppendOnlyAuditSink` per RFC-0012-v2; audit substrate unavailability → `WalletError::AuditUnavailable` per §6.1 (5) rollback contract.
6. **Operator config dir writable** — agent registry persists to `$OCTO_HOME/wallet/agents`; substrate raises `WalletError::Config(String)` on registry-corruption / unwritable paths. CLI surfaces `OctoCliError::NoOctoHome` upstream (exit 27 per `crates/octo-cli/src/error.rs` §`NoOctoHome`).
7. **`caller_did` provenance from active `IdentityHandle` (HSM-bound)** — the substrate does NOT authenticate `caller_did`; it only enforces `caller_did == agent.holder_did` (per §6.1 (2)). A fabricated `caller_did` (compromised CLI passing an arbitrary DID string) would pass the substrate check trivially. The CLI / programmatic caller MUST source `caller_did` from the active `IdentityHandle` (HSM-bound per RFC-0009 §Identity Struct + §HsmAdapter Integration) at the Layer B façade boundary. The substrate-faithful pattern: `caller_did: &Did` is borrowed from an HSM-bound `IdentityHandle` retrieved from process session state. The substrate does not look up the HSM itself. Per-process trust boundary assumed at the façade boundary.

## Security Considerations

1. **State-machine integrity** — substrate rejects invalid transitions with `WalletError::InvalidStateTransition { from, to }` per §6.1 (3). The CLI cannot bypass; the substrate is source of truth.
2. **Audit immutability** — `AppendOnlyAuditSink` is type-level append-only per RFC-0012; tampering breaks the BLAKE3 chain verification on next `verify_chain` call. CLI does not provide a delete primitive.
3. **Reason field is operator-controlled** — `transition_agent` calls `validate_reason` (RFC-0015 v2 KEEP primitive) upstream; the primitive's length cap + control-char filter (`U+0000`-`U+001F`, `U+007F`) applies (MEDIUM sec fix per RFC-0015 v2 §Security Considerations row 1). Defense-in-depth: CLI parses length/UTF-8 at parse time; substrate rejects control chars at the primitive boundary so any future caller inherits the same filter. **C1 range accepted residual** (carried over from RFC-0015 v2): the filter scope is `U+0000`-`U+001F` + `U+007F`; the C1 range (`U+0080`-`U+009F`) passes the filter. Modern UTF-8 terminals render C1 safely; some legacy / non-UTF-8 terminals may interpret them as control sequences.
4. **Audit append rollback contract** (paired with RFC-0012-v2 `AuditEventKind::AgentTransition`) — covers `octo_audit::AuditError` variants per §6.1 (5):
   - `SinkSpecific` and `SequenceGap` trigger fail-closed rollback of the in-memory state mutation; propagate as `WalletError::AuditUnavailable`.
   - `AlreadyExists` is the substrate-canonical idempotent-retry signal (NOT a failure; no rollback, return existing chain-hash as success) per `crates/octo-audit-core/src/error.rs`.
5. **`transition_agent` caller-attestation** (HIGH sec fix per §6.1 (2)) — `caller_did` is a required parameter; substrate rejects any transition whose agent's `holder_did` differs from `caller_did` (`WalletError::ForbiddenHolderMismatch`). Closes the multi-DID enumeration attack surface for the write path.

## Adversarial Review

### Threat: replay-attack via cloned `transition_agent` call

**Adversary:** Operator retries a successful `transition_agent` call (e.g., re-runs the CLI on transient network failure).

**Mitigation:** State machine is idempotent on same-state (no-op success) for non-terminal states; transition to the same state cannot replay because the audit log already contains the prior transition row. New transition differs in `from` (the second call's `target == current_state`); substrate surface records transition AFTER first call. Terminal-state carve-out (NOT idempotent on `Terminated`): `transition_agent(caller_did, uuid, Terminated, _)` on a `Terminated` agent returns `WalletError::InvalidStateTransition { from: Terminated, to: Terminated }` per TV-WLT-AGT-5 (replay-attack guard on terminal-state destructive transitions).

### Threat: invalid transition bypass

**Adversary:** Compromised CLI binary attempts to invoke `transition_agent(uuid, Terminated)` on a `Registered` agent directly.

**Mitigation:** Substrate rejects per state machine; CLI cannot bypass the registry. RFC-0011-c §Security Confirmation Gate surfaces `ConfirmationRequired` for terminal-state transitions; runtime enforces per-process serialization.

### Threat: audit-log trim

**Adversary:** Operator attempts to delete audit log entries to hide a destructive transition.

**Mitigation:** `AppendOnlyAuditSink` is type-level append-only per RFC-0012-v2; tampering breaks the BLAKE3 chain verification on next `verify_chain` call. CLI does not provide a delete primitive.

### Threat: reason-field XSS / control-char injection

**Adversary:** Compromised CLI / operator-supplied `--reason` payload containing ANSI escape sequences (`\x1b[...`), terminal control bytes (`\x07` bell, `\x08` backspace), or terminal emulator OSC sequences (`\x1b]...`) attempts to manipulate downstream tooling that renders the audit log (terminal pagers, log viewers, audit dashboards).

**Mitigation:** `transition_agent` calls `validate_reason` (RFC-0015 v2 KEEP primitive per §6.2.5 validate_reason) upstream; the primitive rejects reason strings containing any control character (`U+0000`-`U+001F`, `U+007F`) BEFORE any state-machine work. The filter is applied at the substrate boundary, not at the CLI parser, so any future caller inherits the same defense. The reason string is stored verbatim as UTF-8 in the audit row's metadata; downstream redaction (RFC-0011-a §Redaction) treats it as opaque text and never re-interprets bytes as escape sequences. ASCII printable + non-control Unicode (`U+0020+`) is allowed; control chars `U+0000`-`U+001F` and `U+007F` are rejected per RFC-0015 v2 §6.2.5 (validate_reason); the filter does not block legitimate Unicode (e.g., non-ASCII names, emoji in destroy reasons).

## Adversary Analysis (5-Question Test)

| Threat                                     | Q1: Who                    | Q2: What?                                                      | Q3: Why?                               | Q4: How mitigated?                                                                                                                                                                                                                   | Q5: Residual risk?                                                                                                                                                                                                                                                                                                 |
| ------------------------------------------ | -------------------------- | -------------------------------------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Replay of transition call                  | Operator                   | Replay `Terminated` tx                                         | Cover destructive intent               | Idempotent on same-state (non-terminal) + audit row reorder + terminal-state carve-out (NOT idempotent; TV-WLT-AGT-5)                                                                                                                | CLI retry surface; auto-retry disabled                                                                                                                                                                                                                                                                             |
| Invalid transition bypass                  | Compromised CLI            | Direct state write                                             | Skip CLI confirmation gate             | Substrate state-machine guard is mandatory                                                                                                                                                                                           | Substrate bug = total compromise (low)                                                                                                                                                                                                                                                                             |
| Audit-log trim                             | Operator                   | Delete audit row                                               | Hide destructive intent                | Append-only sink + BLAKE3 chain (paired with RFC-0012-v2)                                                                                                                                                                            | Substrate storage failure (mitigated)                                                                                                                                                                                                                                                                              |
| Reason-field XSS                           | Compromised CLI            | Inject ANSI/OSC escape                                         | Manipulate downstream renderer         | `validate_reason` (RFC-0015 v2 KEEP primitive) control-char filter `U+0000`-`U+001F`, `U+007F` rejected (MEDIUM sec fix per RFC-0015 v2 §6.2.5 (validate_reason))                                                                    | LOW (C1 range gap per RFC-0015 v2 TV-WLT-AGT-13f — U+0085 NEL, U+009B 8-bit CSI, U+009D 8-bit OSC pass the filter; modern UTF-8 terminals render C1 safely, but some legacy / non-UTF-8 terminals may interpret them as control sequences). Substrate trust + documented C1 gap accepted as RFC-0015-v2 follow-on. |
| Concurrent-call lock contention            | Operator / compromised CLI | Second `transition_agent(agent_id)` while first holds the lock | Bypass TOCTOU check via race window    | `parking_lot::Mutex::try_lock()` per §6.1 (1) + §6.1 (7); second caller observes `WalletError::AlreadyInTransition` immediately (non-blocking)                                                                                       | Lock acquire < 1µs (uncontended); contended path returns substrate error (transient, retry-safe)                                                                                                                                                                                                                   |
| Audit-append failure → state inconsistency | Operator / substrate bug   | Audit sink transient failure (`SinkSpecific` / `SequenceGap`)  | State advances without audit log entry | ROLLBACK contract per §6.1 (5): `SinkSpecific` / `SequenceGap` → in-memory state rolled back + persisted; `WalletError::AuditUnavailable` propagated (fail-closed); `AlreadyExists` is NOT a failure (idempotent-retry per §6.1 (5)) | Persisted rollback window; concurrent reader may observe stale state during rollback; substrate-faithful best-effort rollback                                                                                                                                                                                      |

## Economic Analysis

DEFER — agent write operations have no direct token cost; cite RFC-0900+ (Role Economics) for any cost implications.

## Compatibility

1. **No breaking changes.** Four additive items (1 function + 3 error variants) on `octo-wallet` (Layer B years-stable); no existing public API modified. RFC-0015 v2 KEEP variants are unaffected.
2. **No `schema_version` bump.** The `OutputEnvelope<T>` envelope carries no new fields; CLI mission output schemas unchanged.
3. **No new exit codes for the new errors.** The three DEFERRED error variants map to existing RFC-0011-c §9.8 reserved slots: `AlreadyInTransition` (43), `InvalidStateTransition` (43, unified with `AlreadyInTransition` per R21 M-6 variant unification), `AuditUnavailable` (52).
4. **No new clap variants.** Existing `AgentAction` enum (RFC-0011-c §9.3 dispatch) absorbs the new substrate call; the missions land their variant-per-subcommand as planned.

## Test Vectors

Substrate-level test vectors (`crates/octo-wallet/src/agent.rs` test module). RFC-0015 v2 KEEP vectors (TV-WLT-AGT-1, 2, 12, 13, 13b-13h, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23) are unaffected. RFC-0015-a KEEP vectors land at RFC-0015-a acceptance (paired with RFC-0012-v2 acceptance).

| #              | Substrate call                                                              | Input                                                                               | Expected Output                                                                                                                                                                              | Notes                                                                            |
| -------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| TV-WLT-AGT-3   | `transition_agent(caller_did, uuid, Running, None)`                         | Registered agent                                                                    | `Ok(<TransitionReceipt { previous_state: Registered, current_state: Running, audit_event_id, chain_hash }>)` + audit append                                                                  | Happy path register→running                                                      |
| TV-WLT-AGT-4   | `transition_agent(caller_did, uuid, Running, _)`                            | Running agent                                                                       | `Ok(<unchanged TransitionReceipt>)` + NO audit append (idempotent no-op on same-state non-terminal)                                                                                          | Idempotent same-state                                                            |
| TV-WLT-AGT-5   | `transition_agent(caller_did, uuid, Terminated, _)`                         | Terminated agent                                                                    | `Err(WalletError::InvalidStateTransition { from: Terminated, to: Terminated })`                                                                                                              | Terminal state guard (NOT idempotent)                                            |
| TV-WLT-AGT-6   | `transition_agent(caller_did, missing_uuid, _, _)`                          | Unknown UUID                                                                        | `Err(WalletError::AgentNotFound(uuid))`                                                                                                                                                      | Existence check                                                                  |
| TV-WLT-AGT-7   | `transition_agent(caller_did, uuid, _, Some(long_str))`                     | Reason > 256 chars OR contains control char                                         | `Err(WalletError::ReasonTooLong(257))` OR `Err(WalletError::ReasonContainsControlChars("<U+XXXX>".to_string()))` (propagated from RFC-0015 v2 KEEP `validate_reason` primitive per §6.1 (6)) | Length cap + control-char filter reuse                                           |
| TV-WLT-AGT-8   | `transition_agent(caller_did, uuid, Terminated, _)`                         | Running agent                                                                       | `Ok(<TransitionReceipt { previous_state: Running, current_state: Terminated, audit_event_id, chain_hash }>)` + audit append                                                                  | Destroy path                                                                     |
| TV-WLT-AGT-9   | `transition_agent(caller_did, uuid, Registered, _)`                         | Running agent                                                                       | `Ok(<TransitionReceipt { previous_state: Running, current_state: Registered, audit_event_id, chain_hash }>)` + audit append                                                                  | Restart-from-running                                                             |
| TV-WLT-AGT-10  | `transition_agent(caller_did, uuid, _, _)`                                  | Concurrent call against same `(holder_did, agent_id)`                               | `Err(WalletError::AlreadyInTransition(uuid))`                                                                                                                                                | Concurrent-call guard (`parking_lot::Mutex::try_lock()` per §6.1 (1) + §6.1 (7)) |
| TV-WLT-AGT-11  | `transition_agent(caller_did, uuid, _, _)` with audit sink unavailable      | `AppendOnlyAuditSink` returns `AuditError::SinkSpecific(_)`                         | `Err(WalletError::AuditUnavailable)` + in-memory state rolled back (per §6.1 (5) rollback contract)                                                                                          | Rollback contract (SinkSpecific trigger)                                         |
| TV-WLT-AGT-11b | `transition_agent(caller_did, uuid, _, _)` with audit sink gap              | `append_audit_event` returns `AuditError::SequenceGap { event_id: 43, prev: 42 }`   | `Err(WalletError::AuditUnavailable)` + in-memory state rolled back (per §6.1 (5) rollback contract)                                                                                          | Rollback contract (SequenceGap fail-closed trigger per §6.1 (5))                 |
| TV-WLT-AGT-11c | `transition_agent(caller_did, uuid, _, _)` with audit sink idempotent retry | `append_audit_event` returns `AuditError::AlreadyExists(42)` (idempotent re-append) | `Ok(<existing TransitionReceipt with existing_chain_hash>)` + state NOT rolled back (idempotent-success per §6.1 (5))                                                                        | Idempotent retry (AlreadyExists NOT a failure per §6.1 (5))                      |

CLI-level test vectors live in RFC-0011-c §Test Vectors TV-AGT1..AGT-12 (UNCHANGED — RFC-0015-a substrate alignment does not modify CLI TV).

## Alternatives Considered

- **Pure-CLI state-machine** — substrate delegates transition validation to CLI; rejected: violates substrate-faithful principle; CLI bypass becomes possible.
- **Async transition callbacks** — `transition_agent` returns a future and signals completion via a channel; rejected: adds runtime complexity for no operator-visible benefit; substrate sync semantics match RFC-0002 §Agent State Machine intent.
- **Composite state variants** — keep ACTIVE + BUSY in the substrate enum per RFC-0002 spec; rejected: substrate-faithful principle (the three-state form is canonical in the substrate); RFC-0002-v2 amendment may restore the split later.
- **Std `Mutex::try_lock`** — use Rust std `Mutex::try_lock` instead of `parking_lot::Mutex::try_lock`; rejected: std `Mutex::try_lock()` poisons on holder-panic; parking_lot returns lock error without poisoning the lock state, allowing graceful fallback to `WalletError::AlreadyInTransition` rather than poisoned-lock panics.

## Implementation Phases

- **Phase 1 (RFC-0015 v2 acceptance)** — read surface (`list_owned_agents` + `lookup_agent` + `validate_reason`) + 4 KEEP `WalletError` variants land on `octo-wallet` Layer B; no `transition_agent`; no write-path variants; no `parking_lot` dep.
- **Phase 2 (RFC-0011-c read-only missions)** — CLI missions `0011-c-agent-{create,list,show,attach}-subcommand` consume the read surface.
- **Phase 2.5 (RFC-0015-a acceptance, paired with RFC-0012-v2 acceptance)** — `transition_agent` write function lands on `octo-wallet` Layer B; 3 NEW ADDITIVE `WalletError` variants (`AlreadyInTransition(Uuid)`, `InvalidStateTransition { from, to }`, `AuditUnavailable`) land on `octo-wallet/src/error.rs`; `AuditEventKind::AgentTransition { agent_id, from, to, reason }` variant lands in `octo-audit-core` (Layer A frozen; parent `AuditEvent::at_millis_unix` carries the timestamp); `parking_lot` Cargo.toml entry lands on `octo-wallet/Cargo.toml`.
- **Phase 3 (RFC-0011-c write missions)** — CLI missions `0011-c-agent-{run,destroy}-subcommand` consume the write surface (post-RFC-0015-a acceptance); mutation traces per RFC-0011-c §Test Vectors.

## Key Files to Modify

- `crates/octo-wallet/Cargo.toml` — **add `parking_lot` dep** at RFC-0015-a acceptance (paired with write-path Phase 2.5 lock-mode contract):
  ```toml
  # Write-path lock-mode substrate (RFC-0015-a; non-blocking try_lock without poisoning)
  parking_lot = { version = "0.12", features = ["deadlock_detection"] }
  ```
  Rationale: `parking_lot::Mutex` provides `try_lock()` without poisoning — Rust std `Mutex::try_lock()` poisons on holder-panic; parking_lot returns lock error without poisoning the lock state, allowing graceful fallback to `WalletError::AlreadyInTransition` rather than poisoned-lock panics.
- `crates/octo-wallet/src/agent.rs` — append `transition_agent` (~80 LoC incl. tests) + `TransitionReceipt` projection struct. `list_owned_agents` + `lookup_agent` + `validate_reason` are RFC-0015 v2 KEEP (already landed). The lock mode is `parking_lot::Mutex::try_lock()` per §6.1 (1).
- `crates/octo-wallet/src/error.rs` — append 3 NEW ADDITIVE variants: `WalletError::AlreadyInTransition(Uuid)` + `WalletError::InvalidStateTransition { from, to }` + `WalletError::AuditUnavailable` per §6.3. The 4 KEEP variants from RFC-0015 v2 (`AgentNotFound(Uuid)` + `ForbiddenHolderMismatch` + `ReasonContainsControlChars(String)` + `ReasonTooLong(usize)`) are unaffected.
- `crates/octo-wallet/src/lib.rs` — re-export the new function + projection struct (no breaking change to existing public surface).
- `crates/octo-audit-core/src/event.rs` — **RFC-0012-v2 substrate amendment** (paired): append `AuditEventKind::AgentTransition { agent_id, from, to, reason }` variant. Parent `AuditEvent::at_millis_unix` carries the timestamp (variant payload does NOT duplicate per R21 M-1 fix). NO changes to this file in RFC-0015-a itself; the change is in RFC-0012-v2 (the paired substrate amendment).

**Layer placement table (M-4 amendment — explicit layer discipline per CLAUDE.md §Rust crate-level stability):**

| Crate              | Layer                       | Substrate anchor (§symbol ref)                                                                        | Role at RFC-0015-a acceptance                                                                                                   |
| ------------------ | --------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `octo-wallet-core` | Layer A frozen (RFC-0009)   | `IdentityHandle` struct per `crates/octo-wallet-core/src/identity.rs` §`IdentityHandle`               | Canonical identity substrate; HSM-bound `caller_did` provenance source for §6.1 (2) caller-attestation pattern                  |
| `octo-wallet`      | Layer B façade (RFC-0011-c) | `crates/octo-wallet/src/agent.rs` §`AgentManifest` + `crates/octo-wallet/src/error.rs` §`WalletError` | Façade; RFC-0015-a additive items (`transition_agent` + `TransitionReceipt` + 3 error variants + `parking_lot` dep) land here   |
| `octo-audit-core`  | Layer A frozen (RFC-0012)   | `AuditEventKind` enum                                                                                 | Canonical substrate for paired RFC-0012-v2 `AuditEventKind::AgentTransition` variant (lands with RFC-0012-v2 paired acceptance) |

Layer direction: `octo-wallet` (Layer B) → `octo-wallet-core` (Layer A frozen) for HSM-bound identity; `octo-wallet` (Layer B) → `octo-audit-core` (Layer A frozen) for the audit append (paired with RFC-0012-v2). Layer B → Layer A is the canonical façade-to-substrate hop per CLAUDE.md §Architectural Principles.

No changes to Layer A crates from RFC-0015-a alone (the `AuditEventKind::AgentTransition` amendment is in the paired RFC-0012-v2); no CLI binary changes; no envelope / redactor / exit-code table changes.

## Future Work

- RFC-0015-a paired RFC-0012-v2 — `AuditEventKind::AgentTransition` substrate amendment (sibling; required for RFC-0015-a acceptance).
- `transition_agent` batch API — multi-agent transition in one substrate call (Phase 4 RFC-0002-v2 companion).
- RFC-0002-v2 amendment — restore the five-state ACTIVE/BUSY split if operator demand surfaces (out of scope here).

## Rationale

- **Substrate-faithful** — substrate is canonical per RFC-0012/0013/0014 acceptance pattern; the three-state `AgentState` enum is canonical even when it differs from the RFC-0002 spec diagram.
- **Additive only** — CLAUDE.md §Layer A stability: Layer B additive changes do not break consumers; the new function + 3 DEFERRED error variants are additive; the `AuditEventKind::AgentTransition` variant is in the paired RFC-0012-v2 Layer A amendment (paired acceptance unblocks both RFCs together).
- **No parallel abstractions** — function names + parameter shapes mirror CLI mission call sites exactly (per [[cipherocto-design-principles]] §No parallel abstractions).
- **Substrate-owned invariants** — state-machine validation lives in the substrate; CLI cannot bypass. Per [[cipherocto-design-principles]] §Discipline at first call site pays off.
- **Pairing discipline** — RFC-0015 + RFC-0015-a + RFC-0012-v2 form an acceptance triplet per [[cipherocto-design-principles]] §Extension over enumeration; no central enum edit at Layer A is performed in RFC-0015-a alone.

## Version History

- v1.0 (2026-09-11) Initial draft. Sibling amendment to RFC-0015 carrying the write-path surface (`transition_agent` + paired `WalletError` variants + `parking_lot` lock-mode + audit append + rollback contract). Paired with RFC-0012-v2 acceptance.

## Related RFCs

- RFC-0015 — `octo-wallet` Agent Operations Substrate (sibling; KEEP-only read surface)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers; defines operator UX surface)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` + `AgentManifest` authority)
- RFC-0009 — Identity Management (lifecycle state-machine substrate precedent)
- RFC-0012-v2 — Audit Substrate Amendment (sibling; required for RFC-0015-a acceptance; adds `AuditEventKind::AgentTransition` variant)
- RFC-0011 — `octo` CLI Substrate (parent RFC; provides envelope + error + exit-code substrate)
- RFC-0010 — Canonical DID Codec (DID parsing for `holder_did` filter field)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping)
- [[cipherocto-design-principles]] — Layer model + substrate-faithful principle; RFC-0015 / RFC-0015-a / RFC-0012-v2 triplet follows the extension-over-enumeration pattern (no central enum edit at Layer A)

## Related Use Cases

- `docs/use-cases/agent-marketplace.md` — agent registration + verification flow context.
- `docs/use-cases/hybrid-ai-blockchain-runtime.md` — runtime attach / run context.

## Appendices

### Appendix A. Substrate function signatures (full Rust surface)

```rust
// crates/octo-wallet/src/agent.rs (append to existing module)

// DEFERRED to RFC-0015-a acceptance (paired with RFC-0012-v2 acceptance):

pub fn transition_agent(
    caller_did: &Did,
    uuid: Uuid,
    target: AgentState,
    reason: Option<&str>,
) -> Result<TransitionReceipt, WalletError> {
    // 1. Validate reason: if `reason.is_some()`, call
    //    `validate_reason(reason)` (RFC-0015 v2 KEEP primitive); propagate
    //    `WalletError::ReasonContainsControlChars(String)` /
    //    `WalletError::ReasonTooLong(usize)` unchanged.
    // 2. Look up agent by `uuid` to resolve `holder_did`.
    //    On miss → `WalletError::AgentNotFound(uuid)`.
    // 3. Caller-attestation: enforce `caller_did == agent.holder_did`
    //    (else `WalletError::ForbiddenHolderMismatch`).
    // 4. Acquire per-(holder_did, agent_id) `parking_lot::Mutex::try_lock()`
    //    AFTER holder_did resolved (TOCTOU mitigation per §6.1 (1)).
    //    On `WouldBlock` → `WalletError::AlreadyInTransition(uuid)`.
    // 5. RE-READ `current_state` INSIDE the lock; reject invalid transitions
    //    (e.g., `Terminated → Terminated`) with
    //    `WalletError::InvalidStateTransition { from, to }`.
    // 6. Idempotent on same-state (non-terminal) — early return
    //    `Ok(<unchanged TransitionReceipt>)`; NO audit append.
    // 7. Terminal-state same-state call → `InvalidStateTransition`
    //    (NOT idempotent per §6.1 (4) carve-out).
    // 8. Mutate in-memory state map; persist to disk atomically.
    // 9. Append `AuditEventKind::AgentTransition { agent_id, from, to, reason }`
    //    (parent `AuditEvent::at_millis_unix` carries the timestamp)
    //    via RFC-0012-v2 + RFC-0016 `append_audit_event` write path.
    // 10. If append returns `AuditError::SinkSpecific(_)` OR
    //     `AuditError::SequenceGap { .. }` — ROLLBACK in-memory state +
    //     persist; return `WalletError::AuditUnavailable` (fail-closed).
    //     If append returns `AuditError::AlreadyExists(event_id)` — this is
    //     an IDEMPOTENT RE-APPEND (prior call succeeded); do NOT rollback;
    //     recognize existing chain-hash and return success.
    // 11. Release lock; return updated `TransitionReceipt`.
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionReceipt {
    pub previous_state: AgentState,
    pub current_state: AgentState,
    pub audit_event_id: u64,
    pub chain_hash: [u8; 32],
}
```

### Appendix B. Error envelope cross-reference table

| Substrate variant                                  | CLI variant                               | CLI exit | RFC-0011-c slot | Status                                                                                                                                                                                         |
| -------------------------------------------------- | ----------------------------------------- | -------- | --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WalletError::AlreadyInTransition(uuid)`           | `OctoCliError::AlreadyInTransition(uuid)` | 43       | §9.8 slot 43    | NEW ADDITIVE (lands with RFC-0015-a per §6.3 + §Appendix B; substrate amendment to `octo-wallet/src/error.rs`)                                                                                 |
| `WalletError::InvalidStateTransition { from, to }` | `OctoCliError::AlreadyInTransition(uuid)` | 43       | §9.8 slot 43    | NEW ADDITIVE (lands with RFC-0015-a per §6.3 + §Appendix B; substrate amendment to `octo-wallet/src/error.rs`; CLI payload unified with `AlreadyInTransition` per R21 M-6 variant unification) |
| `WalletError::AuditUnavailable`                    | `OctoCliError::AuditSubstrateNotReady`    | 52       | RFC-0011-c §9.8 | NEW ADDITIVE (lands with RFC-0015-a per §6.3 + §Appendix B; substrate amendment to `octo-wallet/src/error.rs`)                                                                                 |

**R21 M-6 amendment:** the CLI variant for `AlreadyInTransition` + `InvalidStateTransition` is unified to `OctoCliError::AlreadyInTransition(Uuid)` (mirrors substrate `Uuid` payload; eliminates asymmetric payload shape between substrate `Uuid` and CLI `{ from, to }` structs). The substrate-faithful principle: CLI variant payload mirrors substrate variant payload (no parallel abstractions). The slot 43 reservation per RFC-0011-c §9.8 is preserved; the variant name changes only.

### Appendix C. Mermaid diagram — CLI → substrate → audit flow (RFC-0015-a write path)

```mermaid
sequenceDiagram
    participant Op as Operator
    participant CLI as octo-cli (Layer C/D)
    participant Wal as octo-wallet (Layer B)
    participant Lock as parking_lot::Mutex (per-(holder_did, agent_id))
    participant Audit as octo-audit (Layer B façade)
    participant AuditCore as octo-audit-core (Layer A frozen)

    Op->>CLI: octo agent destroy <uuid> --reason "<reason>" --confirm
    CLI->>Wal: transition_agent(caller_did, uuid, Terminated, Some("<reason>"))
    Wal->>Wal: validate_reason("<reason>") (RFC-0015 v2 KEEP primitive per §6.1 (6))
    Wal->>Wal: lookup_agent(caller_did, uuid) → resolve holder_did
    Wal->>Wal: validate caller_did == holder_did (else ForbiddenHolderMismatch per §6.1 (2))
    Wal->>Lock: parking_lot::Mutex::try_lock((holder_did, agent_id))
    alt lock acquired (first caller)
        Lock-->>Wal: Ok(mutex_guard)
        Wal->>Wal: re-read current_state INSIDE lock
        Wal->>Wal: reject invalid transition (e.g., Terminated → Terminated) per §6.1 (3)
        Wal->>Wal: idempotent same-state (non-terminal) → Ok(unchanged) per §6.1 (4)
        Wal->>Wal: mutate in-memory state map; persist to disk atomically
        Wal->>Audit: append_audit_event(AuditEvent { event_kind: AgentTransition, agent_id, from, to, reason, ... })
        Audit->>AuditCore: AppendOnlyAuditSink::append(event)
        alt SinkSpecific / SequenceGap (fail-closed)
            AuditCore-->>Audit: Err(AuditError::SinkSpecific(_) | SequenceGap { .. })
            Audit-->>Wal: Err(AuditError::..)
            Wal->>Wal: ROLLBACK in-memory state + persist per §6.1 (5)
            Wal-->>CLI: Err(WalletError::AuditUnavailable)
        else AlreadyExists (idempotent-retry)
            AuditCore-->>Audit: Err(AuditError::AlreadyExists(event_id))
            Audit-->>Wal: Err(AuditError::AlreadyExists(event_id))
            Wal->>Wal: do NOT rollback; recognize existing chain-hash per §6.1 (5)
            Wal-->>CLI: Ok(TransitionReceipt { audit_event_id, chain_hash: existing_hash, .. })
        else Ok (success)
            AuditCore-->>Audit: Ok(ChainHash)
            Audit-->>Wal: Ok(ChainHash)
            Wal-->>CLI: Ok(TransitionReceipt { previous_state, current_state, audit_event_id, chain_hash })
        end
    else lock contended (second caller)
        Lock-->>Wal: Err(WouldBlock)
        Wal-->>CLI: Err(WalletError::AlreadyInTransition(uuid))
    end
    Lock-->>Wal: release lock on drop
    CLI-->>Op: OutputEnvelope<TransitionOutput> exit 0 or 43 or 52
```

> **RFC-0015-a scope note:** the previous RFC-0015 v1.0 sequence diagram illustrated the `transition_agent` write path through `octo-audit-core`. RFC-0015-a formalizes it with the explicit `parking_lot::Mutex::try_lock()` lock-mode + the rollback contract (`SinkSpecific` / `SequenceGap` fail-closed; `AlreadyExists` idempotent-retry). Paired with RFC-0012-v2 acceptance (`AuditEventKind::AgentTransition` variant in `octo-audit-core`).
