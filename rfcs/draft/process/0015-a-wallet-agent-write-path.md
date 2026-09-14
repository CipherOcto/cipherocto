# RFC-0015-a: `octo-wallet` Agent Write-Path Amendment

## Status

Draft (2026-09-11)

## Authors

- Authored by `@cipherocto` per RFC-0015 amendment chain + RFC-0002 §Agent State Machine substrate authority.

## Maintainers

- Maintainer: `@cipherocto` per RFC-0015 amendment chain.

## Summary

This RFC is a **sibling amendment** to RFC-0015 carrying the **write-path surface** that RFC-0015 deliberately DEFERRED:

1. **`pub fn transition_agent(caller_did: &Did, uuid: Uuid, target: AgentState, reason: Option<&str>) -> Result<TransitionReceipt, WalletError>`** — NEW ADDITIVE write function per §6.1. Paired with `AuditEventKind::AgentTransition` (RFC-0012 substrate amendment). Substrate-faithful caller-attestation + `std::sync::Mutex` lock on GLOBAL `AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>>` (TOCTOU mitigation; mutex poison → `WalletError::Config(String)`) + audit append + rollback contract.
2. **`WalletError::AlreadyInTransition(Uuid)`** — NEW ADDITIVE enum variant per §6.3. Raised when the substrate's in-flight transition guard detects a second concurrent `transition_agent` call against the same `agent_id` (substrate-side state, not lock-contention).
3. **`WalletError::InvalidStateTransition { from, to }`** — NEW ADDITIVE enum variant per §6.3. Raised on illegal transition (e.g., `Registered → Terminated`). Substrate keeps SEPARATE variant per `crates/octo-wallet/src/error.rs` §InvalidStateTransition; CLI mirrors with separate `OctoCliError::InvalidStateTransition { from, to }` per `crates/octo-cli/src/error.rs` §InvalidStateTransition.
4. **`WalletError::AuditUnavailable(String)`** — NEW ADDITIVE enum variant per §6.3. Raised when `AppendOnlyAuditSink` is unreachable OR fails closed (per the rollback contract in §6.1 (5)). The String payload carries the substrate-level `Debug`-formatted `AuditError` (per §transition_agent audit append branch: `.map_err(|e| WalletError::AuditUnavailable(format!("{e:?}")))`).

The three new error variants + the `transition_agent` function + the audit append of `AgentTransition` rows land at RFC-0015-a acceptance paired with RFC-0012 acceptance (Layer A `AuditEventKind::AgentTransition` variant addition to `octo-audit-core`). No new Cargo.toml dep lands (substrate uses `std::sync::Mutex` already in §AgentRecord struct).

**Pairing invariant:** Acceptance of RFC-0015-a REQUIRES paired acceptance of RFC-0012 (Layer A `AuditEventKind::AgentTransition` variant in `octo-audit-core`) per CLAUDE.md §Extension over enumeration + §Layer B depends on A (stable substrate). Without RFC-0012 acceptance, `transition_agent` cannot persist state-machine transitions to the canonical `AppendOnlyAuditSink`.

## Dependencies

**Requires:**

- RFC-0015 — `octo-wallet` Agent Operations Substrate (sibling KEEP-only RFC; this is the write-path amendment)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers; defines write operator UX)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` + `AgentManifest` authority per §Agent State Machine; substrate-faithful drift per RFC-0015 §Summary note)
- RFC-0010 — Canonical DID Codec (DID parsing for `holder_did: Did` filter field)
- RFC-0009 — Identity Management (informational; lifecycle state-machine substrate precedent)
- RFC-0011 — `octo` CLI Substrate (parent RFC; provides `OutputEnvelope<T>`, `OctoCliError`, `OctoCliRedactor`, clap root, exit code table)

**Substrate amendment dependencies (REQUIRED for RFC-0015-a acceptance):**

- **RFC-0012** — `AuditEventKind::AgentTransition { agent_id, from, to, reason }` variant addition to `octo-audit-core` (Layer A frozen; parent `AuditEvent::at_millis_unix` carries the timestamp — variant payload does NOT duplicate the parent timestamp). **(REQUIRED substrate amendment; without this, `transition_agent` cannot persist state-machine transitions to the canonical `AppendOnlyAuditSink`.)**

**RFC-0015 paired consumption:**

- The NEW ADDITIVE `WalletError::{AlreadyInTransition(Uuid), InvalidStateTransition { from, to }, AuditUnavailable(String)}` variants land in `crates/octo-wallet/src/error.rs` alongside the KEEP variants added by RFC-0015. No re-promotion needed across phases.

## Design Goals

1. **Substrate-faithful** — function names, parameter shapes, return types match the canonical CLI consumer contracts in RFC-0011-c §9.3 (no parallel abstractions per [[cipherocto-design-principles]]).
2. **State-machine substrate authority** — `transition_agent` is the substrate-level guard for `AgentState` transitions; CLI cannot bypass. Replay protection + idempotency live in the substrate, not the CLI.
3. **TOCTOU mitigation via GLOBAL std Mutex** — `std::sync::Mutex` lock on `AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>>` (canonical substrate pattern per §AGENT_REGISTRY); mutex poison → `WalletError::Config(String)` per `WalletError::Config` mapping (same as other call sites in the module).
4. **Audit append + rollback contract** — every successful transition appends an `AuditEventKind::AgentTransition` row to `AppendOnlyAuditSink` per RFC-0012. On fail-closed audit-append failure the in-memory state is rolled back. `AlreadyExists` (idempotent re-append) is recognized as audit-sink-internal idempotent-retry at the `octo-audit-core` layer per the canonical `AuditError::AlreadyExists` variant; the wallet substrate sees the resulting `Ok(existing_chain_hash)` without any special-case handling (the audit-core sink transparently normalizes AlreadyExists to Ok before the wallet observes it).
5. **Caller-attestation discipline** — `caller_did: &Did` is borrowed from an HSM-bound `IdentityHandle` at the Layer B façade boundary; substrate enforces `caller_did == agent.holder_did` (HIGH sec fix; same discipline as RFC-0015 read path).
6. **Reason length + control-char filter reuse** — `transition_agent` calls `validate_reason` (RFC-0015 KEEP primitive) upstream; the primitive's `WalletError::{ReasonContainsControlChars(String), ReasonTooLong(usize)}` errors propagate through unchanged.

## Motivation

RFC-0015 KEEP accepts the read path (`list_owned_agents` + `lookup_agent` + `validate_reason`) + 4 KEEP `WalletError` variants. The CLI missions `0011-c-agent-{run,destroy}-subcommand` reference substrate functions on `octo-wallet` for state-machine writes that do not exist in the substrate as of 2026-09-11. Hard-checked via `grep -rE "pub (fn|async fn) " crates/octo-wallet/src/`:

- `octo_wallet::transition_agent` — **MISSING**; CLI consumers `0011-c-agent-{run,destroy}-subcommand` Sub-step 3 / Sub-step 4 cannot dispatch state transitions.

RFC-0015-a closes this gap by adding the write function substrate-faithfully + paired `WalletError` variants + paired audit append. **The substrate addition is the unblock**; the missions then proceed with their existing plans.

## Roles and Authorities

| Role                | Authority                                                                               | Audit trail                              |
| ------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------- |
| Operator (human/CI) | `transition_agent` (write; requires CLI confirmation gate)                              | CLI log + audit append per RFC-0012      |
| Wallet substrate    | Source of truth for `AgentState`; rejects invalid transitions; surfaces reason in error | Internal state machine log               |
| Audit substrate     | Appends `AgentTransition` event to `AppendOnlyAuditSink` per RFC-0012                   | Append-only audit log (BLAKE3-256 chain) |
| CLI (octo-cli)      | Operator UX over substrate functions; never bypasses substrate state-machine            | Same as operator                         |

## Specification

### §6.1 `transition_agent`

```rust
/// State-machine transition for an agent owned by the caller-attested DID.
///
/// Paired with RFC-0012 acceptance (`AuditEventKind::AgentTransition` variant
/// lands in `octo-audit-core` per RFC-0012 paired amendment).
///
/// SECURITY (HIGH — caller-attestation pattern per RFC-0015 §6.2.1):
/// the caller MUST pass the active DID as `caller_did` (NOT derived from
/// process state). The substrate re-validates that `agent.holder_did ==
/// caller_did` and returns `WalletError::ForbiddenHolderMismatch` on
/// mismatch (multi-DID enumeration prevention).
///
/// TOCTOU mitigation: the substrate acquires the GLOBAL
/// `AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>>`
/// `std::sync::Mutex::lock()` (canonical pattern per
/// §AGENT_REGISTRY). Mutex poison maps to
/// `WalletError::Config(String)` (same mapping as other call sites in
/// the module). `current_state` is re-read INSIDE the lock.
///
/// Idempotency on same-state (no carve-out): `transition_agent(
/// caller_did, uuid, current_state, _)` on any state
/// (`Registered → Registered`, `Running → Running`,
/// `Terminated → Terminated`) returns `Ok(TransitionReceipt {
/// previous_state == current_state, audit_log_entry: [0u8; 32], .. })`
/// without audit append (per §transition_agent_self_transition_is_idempotent test fn
/// `transition_agent_self_transition_is_idempotent`). Terminal-state
/// replay is guarded by the state-machine authority, NOT by a terminal
/// carve-out (substrate-faithful).
///
/// Audit append + rollback contract (paired with RFC-0012
/// `AuditEventKind::AgentTransition`): every successful non-idempotent
/// transition appends an `AuditEventKind::AgentTransition { agent_id,
/// from, to, reason }` row to the canonical `AppendOnlyAuditSink` per
/// RFC-0012 (parent `AuditEvent::at_millis_unix` carries the timestamp
/// — variant payload does NOT carry `at_unix`). The audit append failure
/// path maps `AuditError` variants to `WalletError::AuditUnavailable(String)`
/// (String payload carries the `Debug`-formatted `AuditError` per
/// §transition_agent audit append branch). On audit append failure
/// the in-memory state mutation is rolled back (fail-closed).
pub fn transition_agent(
    caller_did: &Did,
    uuid: Uuid,
    target: AgentState,
    reason: Option<&str>,
) -> Result<TransitionReceipt, WalletError>;
```

- **(1) TOCTOU mitigation** — `std::sync::Mutex::lock()` on GLOBAL `AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>>` (canonical substrate pattern per §AGENT_REGISTRY). Mutex poison → `WalletError::Config(String)` (canonical poison-mapping per §Config). The lock is held across reason filter + state-machine work + audit append and released after the audit append completes (or rolls back). `current_state` is re-read INSIDE the lock. **Rationale for std Mutex:** the canonical substrate pattern uses std `Mutex`; mutex poison is mapped to `WalletError::Config` (same as other call sites in `agent` module per §Config variant mapping). The single-writer invariant is enforced via the GLOBAL lock — concurrent callers serialize at lock acquisition. The per-agent rejection path is the state-machine guard (§6.1 (3) below), not a per-agent `try_lock`.
- **(2) Caller-attestation** — `caller_did: &Did` is REQUIRED; substrate rejects when `caller_did != agent.holder_did` (returns `WalletError::ForbiddenHolderMismatch` per RFC-0015 §6.2.4). Closes the same multi-DID enumeration attack surface that `list_owned_agents` + `lookup_agent` defend against per RFC-0015 §6.2.1 + §6.2.5 (lookup_agent); the substrate treats read + write paths with the same caller-attestation discipline. **Provenance invariant:** `caller_did` MUST be sourced from the active `IdentityHandle` (HSM-bound per RFC-0009 §Identity Struct + §HsmAdapter Integration) at the Layer B façade boundary. The substrate does NOT authenticate `caller_did` itself — it only enforces `caller_did == agent.holder_did`. A fabricated `caller_did` (compromised CLI passing an arbitrary DID string) would pass the substrate check trivially. The CLI / programmatic caller is the trust boundary for DID provenance.
- **(3) State-machine authority** — substrate rejects invalid transitions with `WalletError::InvalidStateTransition { from: AgentState, to: AgentState }` (canonical payload shape per §InvalidStateTransition). The `#[non_exhaustive]` enum attribute permits future expansion. Canonical transitions per §state_machine_matches_arm (substrate-canonical `matches!` arm): `Registered → Running` + `Running → Terminated`. All other transitions are rejected with `InvalidStateTransition`. Substrate keeps SEPARATE variant from `AlreadyInTransition(Uuid)` (per §AlreadyInTransition); CLI mirrors with separate `OctoCliError::InvalidStateTransition { from, to }` per §InvalidStateTransition (both share exit 43 per §exit_codes write-path slot).
- **(4) Idempotency on same-state** — `transition_agent(caller_did, uuid, current_state, _)` is **idempotent on all states** (no terminal carve-out): the substrate returns `Ok(TransitionReceipt { previous_state == current_state, audit_log_entry: [0u8; 32], transitioned_at_unix: <wall-clock>, agent_id: <uuid>, .. })` without audit append per substrate test `transition_agent_self_transition_is_idempotent` (per §transition_agent_self_transition_is_idempotent). Replay protection on terminal state lives in the state-machine guard (§6.1 (3)): `Running → Terminated` succeeds; subsequent `Terminated → Terminated` is the idempotent no-op above (no audit event; safe-replay). `Registered → Terminated` is rejected with `InvalidStateTransition` (substrate-canonical).
- **(5) Audit append + rollback contract** — every successful non-idempotent transition appends an `AuditEventKind::AgentTransition { agent_id, from, to, reason }` row to the canonical `AppendOnlyAuditSink` per RFC-0012 (parent `AuditEvent::at_millis_unix` carries the timestamp — variant payload does NOT carry `at_unix`). Substrate-faithful symmetry: parent struct is the canonical timestamp carrier; variants carry transition-specific payload only. On audit append failure (`AuditError` variant returned from `append_audit_event`):
  - The substrate maps the error to `WalletError::AuditUnavailable(String)` (canonical String payload). The String is cfg-gated per the `octo-audit-internal` feature flag: when the feature is enabled, the substrate uses `format!("{e:?}")` (Debug-formatted `AuditError` per §transition_agent audit append branch); when the feature is disabled, the substrate uses a static placeholder string `"octo-audit-internal feature not enabled (RFC-0015-a §6.4 paired-acceptance bridge)"` per the feature-off branch. The wallet surface contract is the same in both branches: `WalletError::AuditUnavailable(String)`.
  - The in-memory state mutation is rolled back to `from` (per §transition_agent_rollback).
  - The function returns `Err(WalletError::AuditUnavailable(_))` (fail-closed).
  - Defense-in-depth: the audit append is the source of truth for the transition log; in-memory state is the source of truth for current `AgentState`. On fail-closed audit-append failure, the state mutation MUST be rolled back so the registry remains consistent.
- **(6) Reason length + control-char filter reuse** — `transition_agent` calls `validate_reason(reason)` (RFC-0015 KEEP primitive per §6.2.5 validate_reason) upstream; the primitive's `WalletError::{ReasonContainsControlChars(String), ReasonTooLong(usize)}` errors propagate through unchanged. Empty string allowed; ≤256 chars enforced (longer → `WalletError::ReasonTooLong(usize)`); control characters (`U+0000`-`U+001F`, `U+007F`) are REJECTED upfront before any state-machine work. Non-UTF-8 input rejected at CLI (substrate trust). **C1 range gap (carry-over from RFC-0015 §6.2.5 validate_reason):** the filter scope is `U+0000`-`U+001F` + `U+007F`; the C1 range (`U+0080`-`U+009F`) passes the filter (per TV-WLT-AGT-13c). The gap is documented as accepted residual in §Adversary Analysis + RFC-0015 §Security Considerations row 1.
- **(7) Single-callsite per agent** — the GLOBAL `AGENT_REGISTRY` std Mutex serializes concurrent `transition_agent` calls (canonical substrate pattern per §6.1 (1)). Per-agent rejection is the state-machine guard (§6.1 (3)) + the `AlreadyInTransition(Uuid)` substrate variant. The `AlreadyInTransition(Uuid)` variant is declared in `crates/octo-wallet/src/error.rs` §AlreadyInTransition; the substrate does NOT currently construct it (no code path in `transition_agent` raises it — the variant is reserved for a future paired-acceptance substrate amendment that adds per-agent `parking_lot::Mutex::try_lock` detection; until then, concurrent callers serialize via the GLOBAL std Mutex). The CLI surfaces `AlreadyInTransition` per `OctoCliError::AlreadyInTransition(uuid)` at `crates/octo-cli/src/error.rs` §AlreadyInTransition, exit 43.

### §6.2 `TransitionReceipt` projection

```rust
/// Returned by `transition_agent` on successful transition (including
/// idempotent same-state no-ops). Carries the audit-event coordinate
/// so CLI consumers can cross-reference the audit log via
/// `audit_log_entry` (the BLAKE3-256 chain-hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionReceipt {
    /// Canonical UUID of the transitioned agent (substrate-canonical per
    /// §TransitionReceipt::agent_id).
    pub agent_id: Uuid,
    /// State before the transition (snapshot taken INSIDE the lock per
    /// §6.1 (1)).
    pub previous_state: AgentState,
    /// State after the successful transition (or same-state for
    /// idempotent no-op per §6.1 (4)).
    pub current_state: AgentState,
    /// Unix-seconds timestamp at which the substrate applied the
    /// transition (best-effort `SystemTime::now()` per
    /// §SystemTime::now).
    pub transitioned_at_unix: u64,
    /// BLAKE3-256 chain-hash of the audit event that committed this
    /// transition (Hex32 wire form per RFC-0015 Appendix A); set to
    /// `[0u8; 32]` for idempotent same-state no-ops (no audit event
    /// emitted per §6.1 (4)).
    pub audit_log_entry: [u8; 32],
}
```

**Substrate-faithful note:** the 5 fields mirror the substrate struct verbatim (per §TransitionReceipt struct): `agent_id` + `previous_state` + `current_state` + `transitioned_at_unix` + `audit_log_entry`. The CLI surfaces this via `OutputEnvelope<TransitionOutput>` with the canonical RFC-0010 DID form. The `TransitionReceipt` projection is additive to `crates/octo-wallet/src/agent.rs`; no parallel abstraction to `AuditEvent` (the projection is the wallet-side re-projection of the audit-event coordinate). The `audit_log_entry: [u8; 32]` field IS the canonical chain-hash (Hex32 wire form); CLI surfaces the chain-hash as `audit_log_entry` (NOT as a separate `chain_hash` field — there is no parallel abstraction).

### §6.3 Error envelope

Canonical substrate-variant → CLI-variant → exit-code cross-reference lives in RFC-0015 §Appendix B. The 3 NEW additive `WalletError` variants (`AlreadyInTransition(Uuid)`, `InvalidStateTransition { from, to }`, `AuditUnavailable(String)`) and their corresponding `OctoCliError` mirror variants are listed there; no parallel abstraction at this section.

CLI mapping follows RFC-0011 §7.4 Substrate `[ADD]` error-envelope pattern (substrate variant → CLI variant → exit code) byte-for-byte.

### §6.4 Substrate-faithful state-machine table

The table below documents the substrate-canonical state machine surface (see `AgentState` enum per §AgentState enum). RFC-0002 §Agent State Machine's spec diagram declares the five-state `ACTIVE/BUSY` working substates; the substrate collapses these to `Running`. RFC-0015-a documents both forms; CLI consumers operate on the substrate three-state form.

| Substrate variant        | Spec diagram equivalent                     | CLI rendering          | Allowed transitions TO (substrate-canonical per §state_machine_matches_arm) |
| ------------------------ | ------------------------------------------- | ---------------------- | --------------------------------------------------------------------------- |
| `AgentState::Registered` | `REGISTERED`                                | `state = "registered"` | `Running`                                                                   |
| `AgentState::Running`    | `ACTIVE` or `BUSY` (per substrate collapse) | `state = "running"`    | `Terminated`                                                                |
| `AgentState::Terminated` | `TERMINATED` (terminal)                     | `state = "terminated"` | (idempotent same-state no-op only per §6.1 (4))                             |

> See RFC-0015 §Summary "Substrate-faithful note" — collapsed ACTIVE/BUSY is unrecoverable from the audit log alone.

### §6.5 Layer A Paired-Acceptance Bridge

The `AuditEventKind::AgentTransition` variant lives in `octo-audit-core` Layer A (frozen). Per RFC-0012-v2 paired-amendment, the variant is gated `#[cfg(feature = "octo-audit-internal")]` — visible only when the wallet substrate feature is enabled. The cfg-gate is the Layer A frozen preservation mechanism: the variant exists in source but is invisible to default builds, preserving the frozen-contract invariant (no additive variants visible to consumers without paired amendment acceptance).

The bridge resolves when RFC-0012-v2 acceptance lands: the cfg-gate is removed, the variant becomes permanent. Until then, `transition_agent` audit append depends on the feature being enabled at compile-time; this is the substrate-faithful paired-acceptance contract per §6.4 paired-invariance.

### §6.6 CLI integration contract

CLI missions consuming this substrate (write path):

| Mission                              | Substrate call                                                                                                                                                                             | Sub-step                                                         |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| `0011-c-agent-run-subcommand.md`     | `transition_agent(caller_did, uuid, Running, None)` then `octo_runtime::spawn_agent`                                                                                                       | Sub-step 3 (NEW, **paired-DEFERRED with RFC-0015-a acceptance**) |
| `0011-c-agent-destroy-subcommand.md` | `transition_agent(caller_did, uuid, Terminated, reason)` then audit append (audit logging is internal to `transition_agent`; CLI surfaces `TransitionReceipt::audit_log_entry` chain-hash) | Sub-step 3 (NEW, **paired-DEFERRED with RFC-0015-a acceptance**) |

Each mission's accepted-state precondition is checked locally against the `AgentState` returned by `octo_wallet::register_agent` + `list_owned_agents` + `lookup_agent` calls (RFC-0015 KEEP); the substrate is source of truth, not the CLI. RFC-0015-a acceptance unblocks the write-path missions (`run`, `destroy`). The CLI surfaces the canonical `TransitionReceipt` projection per §6.2 (5 fields: `agent_id`, `previous_state`, `current_state`, `transitioned_at_unix`, `audit_log_entry`).

### §6.7 Determinism requirements

- **Transition determinism** — `transition_agent` is idempotent on same-state (all states per §6.1 (4); no terminal carve-out); deterministic on first-transition success (state machine is fully ordered per §state_machine_matches_arm: `Registered → Running → Terminated`).
- **Audit log integrity** — every successful non-idempotent `transition_agent` appends a BLAKE3-256 entry to `AppendOnlyAuditSink` per RFC-0012; the chain is `verify_chain`-able end-to-end via `octo_audit_core::verify_chain` (re-exported from `octo-audit`).
- **Exit codes stable** — substrate error variants map to stable CLI exit codes (see §6.3) per parent RFC-0011 §Error Handling.

### §6.8 RFC-0008 Execution Class Mapping

| Operation          | Execution class | Rationale                                                                                                     |
| ------------------ | --------------- | ------------------------------------------------------------------------------------------------------------- |
| `transition_agent` | Class B (write) | State mutation gated by substrate state machine; CLI confirmation gate per RFC-0011 §Confirmation Flag Matrix |

CLI surfaces `Class B` operations via `--confirm` per RFC-0011 §Confirmation Flag Matrix.

### §6.9 Forward Pointer — read path lives in RFC-0015

The read-path surface (`list_owned_agents` + `lookup_agent` + `validate_reason` + paired `WalletError::{AgentNotFound(Uuid), ForbiddenHolderMismatch, ReasonContainsControlChars(String), ReasonTooLong(usize)}`) lives in RFC-0015 KEEP. See `rfcs/draft/process/0015-wallet-agent-operations.md`.

Acceptance of RFC-0015-a does NOT authorize the read path independently; the missions in RFC-0011-c §9.3 list / show / create / attach require RFC-0015 acceptance first (parallel sibling RFC). The missions in RFC-0011-c §9.3 run / destroy require RFC-0015-a acceptance.

## Performance Targets

- `transition_agent` happy path: p95 < 10ms (in-process state-machine work + audit append; substrate does not touch disk for the in-memory state mutation).
- `transition_agent` audit append (RFC-0012 chain): p95 < 2ms in-process.
- `transition_agent` GLOBAL registry lock acquisition (`std::sync::Mutex::lock` on `AGENT_REGISTRY`): p95 < 1µs (uncontended).

## Implicit Assumptions Audit

1. **Single-writer per `agent_id`** — enforced via the GLOBAL `AGENT_REGISTRY` `std::sync::Mutex` lock per §6.1 (1) + §6.1 (7). Concurrent calls observe `WalletError::AlreadyInTransition` at the in-flight transition guard level (not lock contention; the std Mutex serializes rather than rejects-on-contention).
2. **Substrate is canonical for `AgentState`** — CLI never pattern-matches on `AgentState` string representation; serde-derived lowercase string is for display only per `#[serde(rename_all = "lowercase")]` on the enum (per §Display for AgentState `as_str` impl).
3. **Reason string is UTF-8, ≤256 chars, no control chars** — `transition_agent` calls `validate_reason` (RFC-0015 KEEP primitive per §6.2.5 validate_reason) upstream; the primitive's length cap + control-char filter applies.
4. **`register_agent` precedes any transition** — substrate assumes the `agent_id` returned by `register_agent` exists in the in-memory registry before any `transition_agent` call can succeed; CLI precondition check (CLI missions `0011-c-agent-{run,destroy}-subcommand` Sub-step 2 enforces via `lookup_agent`).
5. **Audit substrate is available** — `transition_agent` requires `AppendOnlyAuditSink` per RFC-0012; audit substrate unavailability → `WalletError::AuditUnavailable(String)` per §6.1 (5) rollback contract (String payload carries substrate `AuditError` Debug-formatted per §transition_agent audit append branch).
6. **Operator config dir writable** — agent registry persists to `$OCTO_HOME/wallet/agents`; substrate raises `WalletError::Config(String)` on registry-corruption / unwritable paths OR on mutex poison (per §Config variant mapping (std::sync::Mutex poison) mapping). CLI surfaces `OctoCliError::NoOctoHome` upstream (exit 27 per `crates/octo-cli/src/error.rs` §NoOctoHome).
7. **`caller_did` provenance from active `IdentityHandle` (HSM-bound)** — the substrate does NOT authenticate `caller_did`; it only enforces `caller_did == agent.holder_did` (per §6.1 (2)). A fabricated `caller_did` (compromised CLI passing an arbitrary DID string) would pass the substrate check trivially. The CLI / programmatic caller MUST source `caller_did` from the active `IdentityHandle` (HSM-bound per RFC-0009 §Identity Struct + §HsmAdapter Integration) at the Layer B façade boundary. The substrate-faithful pattern: `caller_did: &Did` is borrowed from an HSM-bound `IdentityHandle` retrieved from process session state. The substrate does not look up the HSM itself. Per-process trust boundary assumed at the façade boundary.

## Security Considerations

1. **State-machine integrity** — substrate rejects invalid transitions with `WalletError::InvalidStateTransition { from, to }` per §6.1 (3). The CLI cannot bypass; the substrate is source of truth.
2. **Audit immutability** — `AppendOnlyAuditSink` is type-level append-only per RFC-0012; tampering breaks the BLAKE3 chain verification on next `verify_chain` call. CLI does not provide a delete primitive.
3. **Reason field is operator-controlled** — `transition_agent` calls `validate_reason` (RFC-0015 KEEP primitive) upstream; the primitive's length cap + control-char filter (`U+0000`-`U+001F`, `U+007F`) applies (MEDIUM sec fix per RFC-0015 §Security Considerations row 1). Defense-in-depth: CLI parses length/UTF-8 at parse time; substrate rejects control chars at the primitive boundary so any future caller inherits the same filter. **C1 range accepted residual** (carried over from RFC-0015): the filter scope is `U+0000`-`U+001F` + `U+007F`; the C1 range (`U+0080`-`U+009F`) passes the filter. Modern UTF-8 terminals render C1 safely; some legacy / non-UTF-8 terminals may interpret them as control sequences.
4. **Audit append rollback contract** (paired with RFC-0012 `AuditEventKind::AgentTransition`) — covers `octo_audit::AuditError` variants per §6.1 (5):
   - `SinkSpecific` and `SequenceGap` trigger fail-closed rollback of the in-memory state mutation; propagate as `WalletError::AuditUnavailable(String)` (String payload carries substrate `AuditError` Debug-formatted).
   - `AlreadyExists` is the audit-sink-internal idempotent-retry signal at the `octo-audit-core` layer per the canonical `AuditError::AlreadyExists` variant. The audit-core sink transparently normalizes AlreadyExists to `Ok(existing_chain_hash)` before the wallet substrate observes it; the wallet sees an ordinary `Ok(_)` and proceeds without rollback. The wallet substrate does NOT have special-case handling for AlreadyExists — if the audit-core propagates AlreadyExists as `Err`, the wallet's fail-closed rollback contract applies.
5. **`transition_agent` caller-attestation** (HIGH sec fix per §6.1 (2)) — `caller_did` is a required parameter; substrate rejects any transition whose agent's `holder_did` differs from `caller_did` (`WalletError::ForbiddenHolderMismatch`). Closes the multi-DID enumeration attack surface for the write path.

## Adversarial Review

### Threat: replay-attack via cloned `transition_agent` call

**Adversary:** Operator retries a successful `transition_agent` call (e.g., re-runs the CLI on transient network failure).

**Mitigation:** State machine is idempotent on same-state (no-op success) for ALL states (no terminal carve-out per §6.1 (4) substrate-canonical); transition to the same state cannot replay because the audit log already contains the prior transition row (idempotent path emits no audit event). New transition differs in `from` (the second call's `target == current_state`); substrate records the transition AFTER the first call's audit append completes. Replay protection on terminal state lives in the state-machine guard (§6.1 (3)): `Running → Terminated` succeeds; subsequent `Terminated → Terminated` is the idempotent no-op (no audit event emitted; safe-replay). `Registered → Terminated` is rejected with `InvalidStateTransition { from: Registered, to: Terminated }` (substrate-canonical per §state_machine_matches_arm + §transition_agent_rejects_registered_to_terminated test `transition_agent_rejects_registered_to_terminated`).

### Threat: invalid transition bypass

**Adversary:** Compromised CLI binary attempts to invoke `transition_agent(uuid, Terminated)` on a `Registered` agent directly.

**Mitigation:** Substrate rejects per state machine; CLI cannot bypass the registry. RFC-0011-c §Security Confirmation Gate surfaces `ConfirmationRequired` for terminal-state transitions; runtime enforces per-process serialization.

### Threat: audit-log trim

**Adversary:** Operator attempts to delete audit log entries to hide a destructive transition.

**Mitigation:** `AppendOnlyAuditSink` is type-level append-only per RFC-0012; tampering breaks the BLAKE3 chain verification on next `verify_chain` call. CLI does not provide a delete primitive.

### Threat: reason-field XSS / control-char injection

**Adversary:** Compromised CLI / operator-supplied `--reason` payload containing ANSI escape sequences (`\x1b[...`), terminal control bytes (`\x07` bell, `\x08` backspace), or terminal emulator OSC sequences (`\x1b]...`) attempts to manipulate downstream tooling that renders the audit log (terminal pagers, log viewers, audit dashboards).

**Mitigation:** `transition_agent` calls `validate_reason` (RFC-0015 KEEP primitive per §6.2.5 validate_reason) upstream; the primitive rejects reason strings containing any control character (`U+0000`-`U+001F`, `U+007F`) BEFORE any state-machine work. The filter is applied at the substrate boundary, not at the CLI parser, so any future caller inherits the same defense. The reason string is stored verbatim as UTF-8 in the audit row's metadata; downstream redaction (RFC-0011 §Redaction) treats it as opaque text and never re-interprets bytes as escape sequences. ASCII printable + non-control Unicode (`U+0020+`) is allowed; control chars `U+0000`-`U+001F` and `U+007F` are rejected per RFC-0015 §6.2.5 (validate_reason); the filter does not block legitimate Unicode (e.g., non-ASCII names, emoji in destroy reasons).

## Adversary Analysis (5-Question Test)

| Threat                                     | Q1: Who                    | Q2: What?                                                                                            | Q3: Why?                               | Q4: How mitigated?                                                                                                                                                                                                                                                                                                                    | Q5: Residual risk?                                                                                                                                                                                                                                                                                           |
| ------------------------------------------ | -------------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Replay of transition call                  | Operator                   | Replay `Terminated` tx                                                                               | Cover destructive intent               | Idempotent on same-state (ALL states per §6.1 (4); no terminal carve-out) + audit row reorder + state-machine guard rejects illegal transitions (e.g., `Registered → Terminated`)                                                                                                                                                     | CLI retry surface; auto-retry disabled                                                                                                                                                                                                                                                                       |
| Invalid transition bypass                  | Compromised CLI            | Direct state write                                                                                   | Skip CLI confirmation gate             | Substrate state-machine guard is mandatory (per §state_machine_matches_arm `matches!` arm)                                                                                                                                                                                                                                            | Substrate bug = total compromise (low)                                                                                                                                                                                                                                                                       |
| Audit-log trim                             | Operator                   | Delete audit row                                                                                     | Hide destructive intent                | Append-only sink + BLAKE3 chain (paired with RFC-0012)                                                                                                                                                                                                                                                                                | Substrate storage failure (mitigated)                                                                                                                                                                                                                                                                        |
| Reason-field XSS                           | Compromised CLI            | Inject ANSI/OSC escape                                                                               | Manipulate downstream renderer         | `validate_reason` (RFC-0015 KEEP primitive) control-char filter `U+0000`-`U+001F`, `U+007F` rejected (MEDIUM sec fix per RFC-0015 §6.2.5 (validate_reason))                                                                                                                                                                           | LOW (C1 range gap per RFC-0015 TV-WLT-AGT-13c — U+0085 NEL, U+009B 8-bit CSI, U+009D 8-bit OSC pass the filter; modern UTF-8 terminals render C1 safely, but some legacy / non-UTF-8 terminals may interpret them as control sequences). Substrate trust + documented C1 gap accepted as RFC-0015 follow-on. |
| Concurrent-call lock contention            | Operator / compromised CLI | Second `transition_agent(agent_id)` while first holds the lock                                       | Bypass TOCTOU check via race window    | GLOBAL `AGENT_REGISTRY` `std::sync::Mutex::lock()` per §6.1 (1) + §6.1 (7); second caller serializes at lock acquisition; in-flight transition guard detects concurrent call against same `agent_id` and returns `WalletError::AlreadyInTransition(uuid)` per substrate canonical form                                                | Lock acquire < 1µs (uncontended); contended path returns substrate error (transient, retry-safe)                                                                                                                                                                                                             |
| Audit-append failure → state inconsistency | Operator / substrate bug   | Audit sink transient failure (`SinkSpecific` / `SequenceGap` / `AlreadyExists` if propagated as Err) | State advances without audit log entry | ROLLBACK contract per §6.1 (5): any `Err` from `append_audit_event` → in-memory state rolled back; `WalletError::AuditUnavailable(String)` propagated (fail-closed). `AlreadyExists` is normalized to `Ok(existing_chain_hash)` at the audit-core layer; the wallet substrate sees an ordinary `Ok(_)` and proceeds without rollback. | Persisted rollback window; concurrent reader may observe stale state during rollback; substrate-faithful best-effort rollback                                                                                                                                                                                |

## Economic Analysis

DEFER — agent write operations have no direct token cost; cite RFC-0900+ (Role Economics) for any cost implications.

## Compatibility

1. **No breaking changes.** Five additive items (1 function + 1 projection struct + 3 error variants) on `octo-wallet` (Layer B years-stable); no existing public API modified. RFC-0015 KEEP variants are unaffected.
2. **No `schema_version` bump.** The `OutputEnvelope<T>` envelope carries no new fields; CLI mission output schemas unchanged.
3. **No new exit codes for the new errors.** The three DEFERRED error variants map to existing RFC-0011-c §9.8 reserved slots: `AlreadyInTransition` (43), `InvalidStateTransition` (43, SEPARATE substrate variant with `{ from, to }` payload mirrored at CLI), `AuditUnavailable` (52).
4. **No new clap variants.** Existing `AgentAction` enum (RFC-0011-c §9.3 dispatch) absorbs the new substrate call; the missions land their variant-per-subcommand as planned.

## Test Vectors

Substrate-level test vectors (`crates/octo-wallet/src/agent.rs` test module). RFC-0015 KEEP vectors (TV-WLT-AGT-1, 2, 12, 13, 13b-13h, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23) are unaffected. RFC-0015-a KEEP vectors land at RFC-0015-a acceptance (paired with RFC-0012 acceptance).

| #              | Substrate call                                                              | Input                                                                                                   | Expected Output                                                                                                                                                                                                                                                             | Notes                                                                                                                                                                      |
| -------------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TV-WLT-AGT-3   | `transition_agent(caller_did, uuid, Running, None)`                         | Registered agent                                                                                        | `Ok(<TransitionReceipt { agent_id: <uuid>, previous_state: Registered, current_state: Running, transitioned_at_unix: <wall-clock>, audit_log_entry: <chain_hash> }>)` + audit append                                                                                        | Happy path register→running                                                                                                                                                |
| TV-WLT-AGT-4   | `transition_agent(caller_did, uuid, Running, _)`                            | Running agent                                                                                           | `Ok(<TransitionReceipt { agent_id: <uuid>, previous_state: Running, current_state: Running, transitioned_at_unix: <wall-clock>, audit_log_entry: [0u8; 32] }>)` + NO audit append (idempotent no-op on same-state)                                                          | Idempotent same-state (no terminal carve-out; per §6.1 (4) + §transition_agent_self_transition_is_idempotent)                                                              |
| TV-WLT-AGT-5   | `transition_agent(caller_did, uuid, Terminated, _)`                         | Terminated agent                                                                                        | `Ok(<TransitionReceipt { agent_id: <uuid>, previous_state: Terminated, current_state: Terminated, transitioned_at_unix: <wall-clock>, audit_log_entry: [0u8; 32] }>)` + NO audit append                                                                                     | Terminal state guard (IS idempotent per §6.1 (4) substrate-canonical; not the inverse of older draft)                                                                      |
| TV-WLT-AGT-6   | `transition_agent(caller_did, missing_uuid, _, _)`                          | Unknown UUID                                                                                            | `Err(WalletError::AgentNotFound(uuid))`                                                                                                                                                                                                                                     | Existence check                                                                                                                                                            |
| TV-WLT-AGT-7   | `transition_agent(caller_did, uuid, _, Some(long_str))`                     | Reason > 256 chars OR contains control char                                                             | `Err(WalletError::ReasonTooLong(257))` OR `Err(WalletError::ReasonContainsControlChars("<U+XXXX>".to_string()))` (propagated from RFC-0015 KEEP `validate_reason` primitive per §6.1 (6))                                                                                   | Length cap + control-char filter reuse                                                                                                                                     |
| TV-WLT-AGT-8   | `transition_agent(caller_did, uuid, Terminated, _)`                         | Running agent                                                                                           | `Ok(<TransitionReceipt { agent_id: <uuid>, previous_state: Running, current_state: Terminated, transitioned_at_unix: <wall-clock>, audit_log_entry: <chain_hash> }>)` + audit append                                                                                        | Destroy path                                                                                                                                                               |
| TV-WLT-AGT-9   | `transition_agent(caller_did, uuid, Registered, _)`                         | Running agent                                                                                           | `Err(WalletError::InvalidStateTransition { from: Running, to: Registered })` (substrate-canonical per §state_machine_matches_arm)                                                                                                                                           | `Running → Registered` is NOT a valid edge in substrate                                                                                                                    |
| TV-WLT-AGT-10  | `transition_agent(caller_did, uuid, _, _)`                                  | Concurrent call against same `agent_id`                                                                 | `Err(WalletError::AlreadyInTransition(uuid))`                                                                                                                                                                                                                               | Concurrent-call guard (GLOBAL `AGENT_REGISTRY` std Mutex per §6.1 (1) + §6.1 (7))                                                                                          |
| TV-WLT-AGT-11  | `transition_agent(caller_did, uuid, _, _)` with audit sink unavailable      | `AppendOnlyAuditSink` returns `AuditError::SinkSpecific(_)`                                             | `Err(WalletError::AuditUnavailable(<Debug-formatted AuditError string>))` + in-memory state rolled back (per §6.1 (5) rollback contract)                                                                                                                                    | Rollback contract (SinkSpecific trigger)                                                                                                                                   |
| TV-WLT-AGT-11b | `transition_agent(caller_did, uuid, _, _)` with audit sink gap              | `append_audit_event` returns `AuditError::SequenceGap { event_id: 43, prev: 42 }`                       | `Err(WalletError::AuditUnavailable(<Debug-formatted AuditError string>))` + in-memory state rolled back (per §6.1 (5) rollback contract)                                                                                                                                    | Rollback contract (SequenceGap fail-closed trigger per §6.1 (5))                                                                                                           |
| TV-WLT-AGT-11c | `transition_agent(caller_did, uuid, _, _)` with audit sink idempotent retry | `append_audit_event` returns `AuditError::AlreadyExists(42)` (idempotent re-append at audit-core layer) | If audit-core normalizes to `Ok(existing_chain_hash)`: `Ok(<TransitionReceipt with existing audit_log_entry chain-hash>)` + state NOT rolled back. If audit-core propagates `Err(AlreadyExists)`: `Err(WalletError::AuditUnavailable(_))` + state rolled back per §6.1 (5). | Substrate-canonical audit-core layer behavior; wallet substrate has no special-case for `AlreadyExists` — fail-closed rollback contract applies whenever `Err` is observed |

CLI-level test vectors live in RFC-0011-c §Test Vectors TV-AGT1..AGT-12 (UNCHANGED — RFC-0015-a substrate alignment does not modify CLI TV).

## Alternatives Considered

- **Pure-CLI state-machine** — substrate delegates transition validation to CLI; rejected: violates substrate-faithful principle; CLI bypass becomes possible.
- **Async transition callbacks** — `transition_agent` returns a future and signals completion via a channel; rejected: adds runtime complexity for no operator-visible benefit; substrate sync semantics match RFC-0002 §Agent State Machine intent.
- **Composite state variants** — keep ACTIVE + BUSY in the substrate enum per RFC-0002 spec; rejected: substrate-faithful principle (the three-state form is canonical in the substrate); a future RFC-0002 amendment may restore the split.
- **Per-(holder_did, agent_id) `parking_lot::Mutex::try_lock`** — use a non-blocking parking_lot try_lock per (holder_did, agent_id) tuple for concurrent-call rejection; rejected: substrate uses GLOBAL `AGENT_REGISTRY` `std::sync::Mutex::lock()` (canonical pattern per §AGENT_REGISTRY in `agent` module); mutex poison maps to `WalletError::Config(String)` per the existing module-wide mapping.

## Implementation Phases

- **Phase 1 (RFC-0015 acceptance)** — read surface (`list_owned_agents` + `lookup_agent` + `validate_reason`) + 4 KEEP `WalletError` variants land on `octo-wallet` Layer B; no `transition_agent`; no write-path variants.
- **Phase 2 (RFC-0011-c read-only missions)** — CLI missions `0011-c-agent-{create,list,show,attach}-subcommand` consume the read surface.
- **Phase 2.5 (RFC-0015-a acceptance, paired with RFC-0012 acceptance)** — `transition_agent` write function lands on `octo-wallet` Layer B; 3 NEW ADDITIVE `WalletError` variants (`AlreadyInTransition(Uuid)`, `InvalidStateTransition { from, to }`, `AuditUnavailable(String)`) land on `octo-wallet/src/error.rs`; `AuditEventKind::AgentTransition { agent_id, from, to, reason }` variant lands in `octo-audit-core` (Layer A frozen; parent `AuditEvent::at_millis_unix` carries the timestamp); NO new Cargo.toml dep (substrate uses `std::sync::Mutex` already in §AgentRecord struct).
- **Phase 3 (RFC-0011-c write missions)** — CLI missions `0011-c-agent-{run,destroy}-subcommand` consume the write surface (post-RFC-0015-a acceptance); mutation traces per RFC-0011-c §Test Vectors.

## Key Files to Modify

- `crates/octo-wallet/Cargo.toml` — **NO new dep** at RFC-0015-a acceptance (substrate uses `std::sync::Mutex` already in §AgentRecord struct per `use std::sync::{Mutex, OnceLock};`).
- `crates/octo-wallet/src/agent.rs` — append `transition_agent` (~80 LoC incl. tests) + `TransitionReceipt` projection struct. `list_owned_agents` + `lookup_agent` + `validate_reason` are RFC-0015 KEEP (already landed). The lock mode is `std::sync::Mutex::lock()` on GLOBAL `AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>>` per §6.1 (1).
- `crates/octo-wallet/src/error.rs` — append 3 NEW ADDITIVE variants: `WalletError::AlreadyInTransition(Uuid)` + `WalletError::InvalidStateTransition { from, to }` + `WalletError::AuditUnavailable(String)` per §6.3. The 4 KEEP variants from RFC-0015 (`AgentNotFound(Uuid)` + `ForbiddenHolderMismatch` + `ReasonContainsControlChars(String)` + `ReasonTooLong(usize)`) are unaffected.
- `crates/octo-wallet/src/lib.rs` — re-export the new function + projection struct (no breaking change to existing public surface).
- `crates/octo-audit-core/src/event.rs` — **RFC-0012 substrate amendment** (paired): append `AuditEventKind::AgentTransition { agent_id, from, to, reason }` variant. Parent `AuditEvent::at_millis_unix` carries the timestamp (variant payload does NOT duplicate per R21 M-1 fix). NO changes to this file in RFC-0015-a itself; the change is in RFC-0012 (the paired substrate amendment).

**Layer placement table (M-4 amendment — explicit layer discipline per CLAUDE.md §Rust crate-level stability):**

| Crate             | Layer                       | Substrate anchor (§symbol ref)                                                                    | Role at RFC-0015-a acceptance                                                                                             |
| ----------------- | --------------------------- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `octo-wallet`     | Layer B façade (RFC-0011-c) | `crates/octo-wallet/src/agent.rs` §AgentManifest + `crates/octo-wallet/src/error.rs` §WalletError | Façade; RFC-0015-a additive items (`transition_agent` + `TransitionReceipt` + 3 error variants) land here                 |
| `octo-audit-core` | Layer A frozen (RFC-0012)   | `AuditEventKind` enum                                                                             | Canonical substrate for paired RFC-0012 `AuditEventKind::AgentTransition` variant (lands with RFC-0012 paired acceptance) |

Layer direction: `octo-wallet` (Layer B) → `octo-audit-core` (Layer A frozen) for the audit append (paired with RFC-0012). Layer B → Layer A is the canonical façade-to-substrate hop per CLAUDE.md §Architectural Principles. The `IdentityHandle` for the caller-attestation provenance (HSM-bound) lives in `crates/octo-wallet/src/identity.rs` per the RFC-0015 substrate (NOT in a separate `octo-wallet-core` crate — that crate is not a workspace member); the substrate-faithful pattern is `caller_did: &Did` borrowed from an HSM-bound `IdentityHandle` retrieved from process session state at the Layer B façade boundary.

No changes to Layer A crates from RFC-0015-a alone (the `AuditEventKind::AgentTransition` amendment is in the paired RFC-0012); no CLI binary changes; no envelope / redactor / exit-code table changes.

## Future Work

- RFC-0015-a paired RFC-0012 — `AuditEventKind::AgentTransition` substrate amendment (sibling; required for RFC-0015-a acceptance).
- `transition_agent` batch API — multi-agent transition in one substrate call (Phase 4 companion).
- RFC-0002 amendment — restore the five-state ACTIVE/BUSY split if operator demand surfaces (out of scope here).

## Rationale

- **Substrate-faithful** — substrate is canonical per RFC-0012/0013/0014 acceptance pattern; the three-state `AgentState` enum is canonical even when it differs from the RFC-0002 spec diagram.
- **Additive only** — CLAUDE.md §Layer A stability: Layer B additive changes do not break consumers; the new function + 3 DEFERRED error variants + `TransitionReceipt` projection struct are additive; the `AuditEventKind::AgentTransition` variant is in the paired RFC-0012 Layer A amendment (paired acceptance unblocks both RFCs together).
- **No parallel abstractions** — function names + parameter shapes mirror CLI mission call sites exactly (per [[cipherocto-design-principles]] §No parallel abstractions).
- **Substrate-owned invariants** — state-machine validation lives in the substrate; CLI cannot bypass. Per [[cipherocto-design-principles]] §Discipline at first call site pays off.
- **Pairing discipline** — RFC-0015 + RFC-0015-a + RFC-0012 form an acceptance triplet per [[cipherocto-design-principles]] §Extension over enumeration; no central enum edit at Layer A is performed in RFC-0015-a alone.

## Version History

- 2026-09-11 — Initial draft. Sibling amendment to RFC-0015 carrying the write-path surface (`transition_agent` + paired `WalletError` variants + `TransitionReceipt` projection + std Mutex lock-mode + audit append + rollback contract). Paired with RFC-0012 acceptance.

## Related RFCs

- RFC-0015 — `octo-wallet` Agent Operations Substrate (sibling; KEEP-only read surface)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers; defines operator UX surface)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` + `AgentManifest` authority)
- RFC-0009 — Identity Management (lifecycle state-machine substrate precedent)
- RFC-0012 — Audit Substrate (sibling; required for RFC-0015-a acceptance; adds `AuditEventKind::AgentTransition` variant)
- RFC-0011 — `octo` CLI Substrate (parent RFC; provides envelope + error + exit-code substrate)
- RFC-0010 — Canonical DID Codec (DID parsing for `holder_did` filter field)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping)
- [[cipherocto-design-principles]] — Layer model + substrate-faithful principle; RFC-0015 / RFC-0015-a / RFC-0012 triplet follows the extension-over-enumeration pattern (no central enum edit at Layer A)

## Related Use Cases

- `docs/use-cases/agent-marketplace.md` — agent registration + verification flow context.
- `docs/use-cases/hybrid-ai-blockchain-runtime.md` — runtime attach / run context.

## Appendices

### Appendix A. Substrate function signatures (full Rust surface)

```rust
// crates/octo-wallet/src/agent.rs (append to existing module)

// Paired with RFC-0012 acceptance (AuditEventKind::AgentTransition lands):

pub fn transition_agent(
    caller_did: &Did,
    uuid: Uuid,
    target: AgentState,
    reason: Option<&str>,
) -> Result<TransitionReceipt, WalletError> {
    // 1. Validate reason: if `reason.is_some()`, call
    //    `validate_reason(reason)` (RFC-0015 KEEP primitive); propagate
    //    `WalletError::ReasonContainsControlChars(String)` /
    //    `WalletError::ReasonTooLong(usize)` unchanged.
    // 2. Lock GLOBAL AGENT_REGISTRY std::sync::Mutex
    //    (`static AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>>`
    //    per §AGENT_REGISTRY). Poison → `WalletError::Config(String)`.
    // 3. Look up agent by `uuid` to resolve `holder_did`.
    //    On miss → `WalletError::AgentNotFound(uuid)`.
    // 4. Caller-attestation: enforce `caller_did == agent.holder_did`
    //    (else `WalletError::ForbiddenHolderMismatch`).
    // 5. In-flight transition guard: if a concurrent call against same
    //    `agent_id` is in flight, return `WalletError::AlreadyInTransition(uuid)`.
    // 6. RE-READ `current_state` INSIDE the lock; if `current_state == target`
    //    (any state, no terminal carve-out per §6.1 (4)), early return
    //    `Ok(TransitionReceipt { previous_state: current_state, current_state: target,
    //    audit_log_entry: [0u8; 32], transitioned_at_unix: <wall-clock>, agent_id: uuid })`
    //    with NO audit append.
    // 7. State-machine guard: reject invalid transitions (e.g.,
    //    `Registered → Terminated`, `Running → Registered`, `Terminated → Running`)
    //    with `WalletError::InvalidStateTransition { from, to }` per `agent.rs`
    //    §state_machine_matches_arm `matches!` arm (only `Registered → Running` + `Running → Terminated`
    //    are valid).
    // 8. Mutate in-memory state map; `transitioned_at_unix = now_unix_secs()`.
    // 9. Append `AuditEventKind::AgentTransition { agent_id, from, to, reason }`
    //    (parent `AuditEvent::at_millis_unix` carries the timestamp)
    //    via RFC-0012 `append_audit_event` write path.
    // 10. If append fails (`AuditError::SinkSpecific(_)` /
    //     `AuditError::SequenceGap { .. }`) — ROLLBACK in-memory state
    //     to `from`; return `WalletError::AuditUnavailable(format!("{e:?}"))`
    //     (String payload per `agent.rs` §transition_agent audit append branch; fail-closed).
    //     If append returns `AuditError::AlreadyExists(event_id)` — this is
    //     an IDEMPOTENT RE-APPEND (prior call succeeded); do NOT rollback;
    //     recognize existing chain-hash and return success.
    // 11. Release lock; return `TransitionReceipt { agent_id, previous_state, current_state,
    //     transitioned_at_unix, audit_log_entry: chain_hash }`.
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionReceipt {
    pub agent_id: Uuid,
    pub previous_state: AgentState,
    pub current_state: AgentState,
    pub transitioned_at_unix: u64,
    pub audit_log_entry: [u8; 32],
}
```

### Appendix B. Error envelope cross-reference table

The 3 NEW ADDITIVE rows for the write-path (`AlreadyInTransition` / `InvalidStateTransition` / `AuditUnavailable`) are documented in the canonical RFC-0015 §Appendix B (rows 5-7). No re-listing here — single source of truth per substrate-faithful principle. See RFC-0015 §Appendix B for the canonical substrate-variant to CLI-variant to exit-code cross-reference; the §Substrate-faithful symmetry note (CLI variant payload mirrors substrate variant payload; `from`/`to` fields serialize as `String` at CLI boundary per §Display for AgentState) lives in the same location.

### Appendix C. Mermaid diagram — CLI → substrate → audit flow (RFC-0015-a write path)

```mermaid
sequenceDiagram
    participant Op as Operator
    participant CLI as octo-cli (Layer C/D)
    participant Wal as octo-wallet (Layer B)
    participant Lock as std::sync::Mutex (GLOBAL AGENT_REGISTRY)
    participant Audit as octo-audit (Layer B façade)
    participant AuditCore as octo-audit-core (Layer A frozen)

    Op->>CLI: octo agent destroy <uuid> --reason "<reason>" --confirm
    CLI->>Wal: transition_agent(caller_did, uuid, Terminated, Some("<reason>"))
    Wal->>Wal: validate_reason("<reason>") (RFC-0015 KEEP primitive per §6.1 (6))
    Wal->>Lock: std::sync::Mutex::lock() on GLOBAL AGENT_REGISTRY
    Lock-->>Wal: Ok(mutex_guard) (poison → WalletError::Config(String))
    Wal->>Wal: lookup_agent(caller_did, uuid) → resolve holder_did
    Wal->>Wal: validate caller_did == holder_did (else ForbiddenHolderMismatch per §6.1 (2))
    Wal->>Wal: re-read current_state INSIDE lock
    alt same-state (all states; idempotent per §6.1 (4))
        Wal->>Wal: same-state early return: Ok(TransitionReceipt { audit_log_entry: [0u8;32], .. }) + NO audit append
        Wal-->>CLI: Ok(TransitionReceipt { previous_state == current_state, audit_log_entry: [0u8; 32], .. })
    else invalid transition (per §6.1 (3))
        Wal->>Wal: reject with WalletError::InvalidStateTransition { from, to }
        Wal-->>CLI: Err(WalletError::InvalidStateTransition { from, to })
    else valid edge (Registered → Running OR Running → Terminated per §6.1 (3))
        Wal->>Wal: mutate in-memory state map (no disk persistence in current substrate)
        Wal->>Audit: append_agent_transition_event(AuditEvent { event_kind: AgentTransition, agent_id, from, to, reason, .. })
        Audit->>AuditCore: AppendOnlyAuditSink::append(event)
        alt SinkSpecific / SequenceGap (fail-closed)
            AuditCore-->>Audit: Err(AuditError::SinkSpecific(_) | SequenceGap { .. })
            Audit-->>Wal: Err(AuditError::..)
            Wal->>Wal: ROLLBACK in-memory state to `from` per §6.1 (5)
            Wal-->>CLI: Err(WalletError::AuditUnavailable(<Debug-formatted string>))
        else AlreadyExists (idempotent-retry)
            AuditCore-->>Audit: Err(AuditError::AlreadyExists(event_id))
            Audit-->>Wal: Err(AuditError::AlreadyExists(event_id))
            Wal->>Wal: do NOT rollback; recognize existing chain-hash per §6.1 (5)
            Wal-->>CLI: Ok(TransitionReceipt { audit_log_entry: existing_chain_hash, .. })
        else Ok (success)
            AuditCore-->>Audit: Ok(ChainHash)
            Audit-->>Wal: Ok(ChainHash)
            Wal-->>CLI: Ok(TransitionReceipt { agent_id, previous_state, current_state, transitioned_at_unix, audit_log_entry: chain_hash })
        end
    end
    Lock-->>Wal: release lock on drop
    CLI-->>Op: OutputEnvelope<TransitionOutput> exit 0 or 43 or 52
```

> **RFC-0015-a scope note:** the sequence diagram illustrates the `transition_agent` write path through `octo-audit-core` paired with RFC-0012 (`AuditEventKind::AgentTransition` variant in `octo-audit-core`). Lock mode is `std::sync::Mutex` on GLOBAL `AGENT_REGISTRY` per §AGENT_REGISTRY; rollback contract per §6.1 (5): `SinkSpecific` / `SequenceGap` fail-closed (with state rollback to `from`); `AlreadyExists` is idempotent-retry (no rollback). Idempotency on same-state (§6.1 (4)) returns `Ok(TransitionReceipt { audit_log_entry: [0u8; 32], .. })` with NO audit append (no terminal carve-out per substrate-canonical test `transition_agent_self_transition_is_idempotent` at §transition_agent_self_transition_is_idempotent).
