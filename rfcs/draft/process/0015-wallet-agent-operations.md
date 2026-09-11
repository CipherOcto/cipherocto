# RFC-0015: Agent Operations Substrate (`octo-wallet` agent list + transition)

## Status

Draft (2026-09-11)

## Authors

- Authored by `@cipherocto` per RFC-0011-c agent lifecycle amendment + RFC-0002 §Agent State Machine substrate authority.

## Maintainers

- Maintainer: `@cipherocto` per RFC-0011-c amendment chain.

## Summary

This RFC defines the canonical agent operations substrate as **two additive public functions + two error variants** on the existing `octo-wallet` (Layer B years-stable per CLAUDE.md §Rust crate-level stability):

1. **`pub fn list_owned_agents(caller_did: &Did, filter: &AgentFilter) -> Result<Vec<AgentSummary>, WalletError>`** — server-side filter on `holder_did`, `state`, `limit`, `cursor`; returns `Vec<AgentSummary>` per-call (cursor is forward-compat opaque token).
2. **`pub fn validate_reason(reason: &str) -> Result<(), WalletError>`** — substrate-faithful control-char filter primitive (per §6.2.5); string-level filter with no substrate amendment required.
3. **`WalletError::AgentNotFound(Uuid)`** — new error variant; mirrors the existing `WalletError::AgentAlreadyExists(Uuid)` shape.
4. **`WalletError::ForbiddenHolderMismatch`** — new error variant (HIGH sec fix per §6.2.1; multi-DID enumeration prevention).

The write-path surface (`transition_agent` + paired `WalletError` variants) is documented in §6.2.2 / §6.3 / §6.8 but DEFERRED per Scope-cut summary pending RFC-0012-v2 acceptance.

The substrate is intentionally **read-only on RFC-0015 acceptance** — `register_agent` already exists at `cli_fns.rs`. No new persistence, no new envelopes. Domain consumers are CLI missions `0011-c-agent-{list,run,destroy,attach}-subcommand`. RFC-0002 §Agent State Machine is the canonical state authority.

**Scope-cut summary (R2 review outcome):** RFC-0015 R1 surface proposed `list_owned_agents` (read) + `transition_agent` (write) + `WalletError::AgentNotFound`. The R2 scope-cut KEEPS the substrate-faithful read surface + error variant DEFERs the write surface. Concretely:

- **KEEP** — `list_owned_agents(caller_did: &Did, filter: &AgentFilter) -> Result<Vec<AgentSummary>, WalletError>` (substrate-faithful read; per RFC-0015 §6.2.1).
- **KEEP** — `validate_reason(reason: &str) -> Result<(), WalletError>` (substrate-faithful primitive string-level filter; per RFC-0015 §6.2.5).
- **KEEP** — `WalletError::AgentNotFound(Uuid)` (substrate-faithful error variant; mirrors existing `WalletError::AgentAlreadyExists(Uuid)` shape).
- **KEEP** — `WalletError::ForbiddenHolderMismatch` (NEW per R2 — HIGH sec fix per §6.2.1; multi-DID enumeration prevention).
- **DEFER** — `transition_agent(...)` write function (RFC-0015 §6.2.2 DEFERRED — depends on RFC-0012-v2 amendment that adds `AuditEventKind::AgentTransition` to Layer A frozen `octo-audit-core`; this variant does not exist in the substrate as of 2026-09-11).
- **DEFER** — `WalletError::AuditUnavailable` (RFC-0015 §6.3 DEFERRED — paired with `transition_agent` write path).
- **DEFER** — `WalletError::AlreadyInTransition(Uuid)` (write-only; see §6.8 DEFERRED SURFACE).
- **DEFER** — `WalletError::InvalidStateTransition { from, to }` (write-only; see §6.8 DEFERRED SURFACE).
- **DEFER** — `WalletError::ReasonTooLong(usize)` (write-only; see §6.8 DEFERRED SURFACE).
- **DEFER** — `WalletError::ReasonContainsControlChars` (write-only; paired with `ReasonTooLong` length cap + control-char filter per §6.2.2 (Reason length + control-char filter); see §6.8 DEFERRED SURFACE).
- **DEFER** — Audit append of `AgentTransition` row (paired with RFC-0016-v2 `append_audit_event` write path per RFC-0016 §6.2.3 / §6.9 DEFERRED SURFACE).

**Substrate-faithful note (mandatory):** RFC-0002 §Agent State Machine spec diagram declares a five-state model (`REGISTERED → ACTIVE → BUSY → ACTIVE → TERMINATED`). The substrate enum (§AgentState enum in `octo-wallet`) currently implements a **three-state model** (`Registered`, `Running`, `Terminated`). Per the substrate-faithful principle (per RFC-0012/0013/0014 acceptance pattern), this RFC treats the substrate as canonical — the `Running` variant collapses RFC-0002's `ACTIVE` and `BUSY` working state into a single observable runtime state. A future amendment (RFC-0002-v2) may split the substrate enum back to match the spec diagram; until then, CLI surfaces `Running` as both ACTIVE and BUSY (per RFC-0011-c `AgentState` rendering rule).

## Dependencies

**Requires:**

- RFC-0011-c — `octo agent` Subcommands (consumers; defines read + transition operator UX)
- RFC-0002 — Agent Manifest Specification (canonical `AgentManifest` + `AgentState` authority per §Agent State Machine; substrate-faithful drift per §Summary note above)
- RFC-0010 — Canonical DID Codec (DID parsing for `holder_did: Did` filter field)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping per §RFC-0008 Execution Class Mapping)
- RFC-0009 — Identity Management (informational; lifecycle state-machine substrate precedent)
- RFC-0011 — `octo` CLI Substrate (parent RFC; provides `OutputEnvelope<T>`, `OctoCliError`, `OctoCliRedactor`, clap root, exit code table)

**Substrate amendment dependencies (DEFERRED surfaces — R2 scope-cut):**

- **RFC-0012-v2** — `AuditEventKind::AgentTransition { agent_id, from, to, reason, at_unix }` variant addition to `octo-audit-core` (Layer A frozen). **(DRAFT — required substrate amendment for the `transition_agent` write path DEFERRED in §6.2.2 / §6.3 / §6.8 DEFERRED SURFACE.)** Without this amendment, `transition_agent` cannot persist state-machine transitions to the canonical `AppendOnlyAuditSink` per RFC-0012.
- **RFC-0002-v2** — `AgentState` 5-state ACTIVE/BUSY split (companion amendment per §Summary "Substrate-faithful note"). **(DRAFT — informational; not on the RFC-0015 critical path; substrate currently carries the 3-state form.)**

## Design Goals

1. **Additive only** — new functions append to the existing `octo-wallet` (Layer B) public surface; no breaking changes to existing public API per RFC migration etiquette.
2. **State-machine substrate authority** — `transition_agent` is the substrate-level guard for `AgentState` transitions; CLI cannot bypass. Replay protection + idempotency live in the substrate, not the CLI.
3. **No new persistence** — `list_owned_agents` + `transition_agent` operate on the existing `AgentManifest` / `AgentSummary` registry stored in `octo-wallet`; no new IO surface, no new envelopes.
4. **Layer B stability** — public functions are additive within the years-stable identity substrate; no PQC-migration impact per CLAUDE.md §Architectural Principles.
5. **Substrate-faithful** — function names, parameter shapes, return types match the canonical CLI consumer contracts in RFC-0011-c §9.3.2 / §9.3.3 / §9.3.4 / §9.3.5 (no parallel abstractions per [[cipherocto-design-principles]]).
6. **Read is single-writer, transition is single-callsite per agent** — substrate enforces `(single-writer, multi-reader)`; concurrent `transition_agent` against the same `agent_id` returns `WalletError::AlreadyInTransition`.
7. **Read returns ordered results** — `list_owned_agents` returns summaries sorted by `registered_at_unix DESC` for deterministic CLI + scripting output; `cursor` token is forward-compat for Phase 2 multi-page iteration.

## Motivation

The 6 RFC-0011-c CLI missions (`0011-c-agent-create`, `...-run`, `...-list`, `...-destroy`, `...-attach` per `missions/claimed/`) reference substrate functions on `octo-wallet` that do not exist in the substrate as of 2026-09-11. Hard-checked via `grep -rE "pub (fn|async fn) " crates/octo-wallet/src/`:

- `octo_wallet::list_owned_agents` — **MISSING**; CLI consumer `0011-c-agent-list-subcommand.md` Sub-step 3 cannot dispatch.
- `octo_wallet::transition_agent` — **MISSING**; CLI consumers `0011-c-agent-{run,destroy,attach}-subcommand.md` Sub-step 3 / Sub-step 4 cannot dispatch state transitions.

The substrate's existing surface is **types-only**: `AgentManifest`, `AgentState`, `AgentSummary`, `AgentFilter`, `CapabilityId`, plus the `cli_fns::register_agent` function (which materializes an `AgentManifest` from a parsed RFC-0002 toml document). Reads + transitions are missing. This gap is what RFC-0015 closes.

The mission YAMLs reference the missing surface as if it already exists, but the substrate reality is different. RFC-0015 closes the gap by adding the functions substrate-faithfully — same names + shapes the missions already target. **The substrate addition is the unblock**; the missions then proceed with their existing plans.

## Roles and Authorities

| Role                | Authority                                                                                  | Audit trail                           |
| ------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------- |
| Operator (human/CI) | `list_owned_agents` (read); `transition_agent` (write; requires CLI confirmation gate)     | CLI log + audit append per RFC-0011-a |
| Wallet substrate    | Source of truth for `AgentState`; rejects invalid transitions; surfaces reason in error    | Internal state machine log            |
| Audit substrate     | Appends `transition` event to `AppendOnlyAuditSink` (RFC-0012) per `transition_agent` call | Append-only audit log                 |
| CLI (octo-cli)      | Operator UX over substrate functions; never bypasses substrate state-machine               | Same as operator                      |

## Specification

### §6.1 System architecture (mermaid)

```mermaid
graph LR
  CLI[octo agent {list,run,destroy,attach}]
  Wallet[octo-wallet Layer B]
  Audit[octo-audit-core Layer A]
  State[(octo-wallet AgentState registry<br>Layer B)]

  CLI -- "list_owned_agents(caller_did, filter)" --> Wallet
  CLI -. "transition_agent(uuid, target) (DEFERRED)" .-> Wallet
  Wallet -. "verify state machine (DEFERRED)" .-> State
  Wallet -. "transition event (DEFERRED)" .-> Audit
  Wallet -. "update summary (DEFERRED)" .-> State
```

**R4.5 scope-cut note:** the KEEP-only flow is the solid `CLI -- list_owned_agents --> Wallet` edge; the write-path edges (transition_agent + verify state machine + transition event + update summary) are dotted and DEFERRED to RFC-0012-v2 + RFC-0015 re-implementation. At R4.5 acceptance, `Wallet` exposes `list_owned_agents(caller_did, filter)` only.

Layer direction: CLI (Layer C/D) → `octo-wallet` (Layer B) → `octo-audit-core` (Layer A). No reverse deps. No business logic in CLI; substrate owns invariants.

### §6.2 Public surface additions (`crates/octo-wallet/src/agent.rs`)

Four additive items = 2 functions + 2 error variants, layered atop the existing `AgentManifest` / `AgentState` / `AgentSummary` / `AgentFilter` / `CapabilityId` types already present in the same file:

1. **`list_owned_agents`** — read function (KEEP per R5.5)
2. **`validate_reason`** — primitive string-level filter (KEEP per R20.5; substrate-faithful per §6.2.5)
3. **`WalletError::AgentNotFound(Uuid)`** — error variant (KEEP per R5.5)
4. **`WalletError::ForbiddenHolderMismatch`** — error variant (KEEP per R5.5; NEW per R2)

The write function `transition_agent` is DEFERRED per §6.8 DEFERRED SURFACE (RFC-0012-v2 acceptance required) and is documented in its own §6.2.2 DEFERRED section below — it is **NOT** counted among the four additive items above.

#### §6.2.1 `list_owned_agents`

```rust
/// List all agents owned by the caller-attested DID, filtered server-side.
///
/// SECURITY (HIGH — caller-attestation pattern per RFC-0011 §Lifecycle Requirements):
/// the caller MUST pass the active DID as `caller_did` (NOT derived from
/// process state). The substrate re-validates `caller_did` against the
/// `AgentFilter::holder_did` field and rejects any filter whose
/// `holder_did` differs from `caller_did` (multi-DID enumeration
/// prevention). The caller is the CLI mission or upstream façade; the
/// substrate is the source of truth for authorization.
///
/// Sorting: `registered_at_unix DESC` (deterministic across calls).
/// Limit: substrate applies a hard ceiling of 1024 (per `AgentFilter::limit`
/// docs); values exceeding 1024 clamp to 1024 with no error.
/// Cursor: opaque forward-compat token (none today; reserved for Phase 2).
/// Read-only: no state mutation.
pub fn list_owned_agents(
    caller_did: &Did,
    filter: &AgentFilter,
) -> Result<Vec<AgentSummary>, WalletError>;
```

- **Filter semantics** — server-side `holder_did == filter.holder_did.unwrap_or(caller_did.clone())`; substrate enforces `filter.holder_did.is_none() || filter.holder_did.as_deref() == Some(caller_did.as_str())` (mismatch → `WalletError::ForbiddenHolderMismatch`); `state == filter.state` (exact match, no wildcards); `limit` clamp; `cursor` reserved (ignored on Phase 1 reads).
- **Return semantics** — empty `Vec` when zero matches (NOT an error per RFC-0011-c §9.3.3 TV-AGT6); summaries sorted by `registered_at_unix DESC`, with secondary sort by `agent_id` (canonical UUID v7) ASC as deterministic tiebreaker for entries sharing the same `registered_at_unix` (substrate-faithful ordering per §6.6 determinism requirement).
- **Error semantics** — `WalletError::Config` on registry corruption (unrecoverable); `WalletError::Io` on disk read failure; `WalletError::ForbiddenHolderMismatch` (NEW) on caller/filter DID mismatch.

#### §6.2.2 `transition_agent` — DEFERRED DESIGN

**Status: DEFERRED** pending RFC-0012-v2 substrate amendment (adds `AuditEventKind::AgentTransition` to `octo-audit-core` Layer A frozen). Signature below is **forward-looking only**; NOT implementable at R2 acceptance. The TOCTOU mitigation and caller-attestation parameters documented in §6.8 row 1 represent the substrate-amendment-conditional contract.

```rust
// FORWARD-LOOKING ONLY — see §6.8 DEFERRED SURFACE row 1
pub fn transition_agent(
    caller_did: &Did,
    uuid: Uuid,
    target: AgentState,
    reason: Option<&str>,
) -> Result<AgentSummary, WalletError>;
```

- **(1) TOCTOU mitigation** (forward-looking) — substrate acquires a per-`(holder_did, agent_id)` lock AFTER looking up holder_did by `uuid`; `current_state` is re-read INSIDE the lock. The lookup-by-uuid reveals `holder_did`; the lock is then keyed on `(holder_did, agent_id)`. The lock is released after the audit append completes (or rolls back). The order matters: substrate first resolves the agent to learn its holder_did, THEN takes the per-`(holder_did, agent_id)` lock; this avoids a parallel lookup-then-relookup pattern that would race on agent reassignment.
- **(2) Caller-attestation** (forward-looking) — `caller_did` parameter is REQUIRED; substrate rejects when `caller_did != agent.holder_did` (returns `WalletError::ForbiddenHolderMismatch` per §6.2.4). Closes the same multi-DID enumeration attack surface that `list_owned_agents` defends against per §6.2.1; the substrate treats read + write paths with the same caller-attestation discipline. **Provenance invariant (R20.5 finding H-5):** `caller_did` MUST be sourced from the active `IdentityHandle` (HSM-bound per RFC-0009 §Identity Struct + §HsmAdapter Integration) at the Layer B façade boundary. The substrate does NOT authenticate `caller_did` itself — it only enforces `caller_did == agent.holder_did`. A fabricated `caller_did` (compromised CLI passing an arbitrary DID string) would pass the substrate check trivially. The CLI / programmatic caller is the trust boundary for DID provenance. The substrate-faithful pattern: `caller_did: &Did` is borrowed from an HSM-bound `IdentityHandle` retrieved from process session state (per RFC-0009 §Identity Struct); the substrate does not look up the HSM itself. Per-process trust boundary assumed at the façade boundary per §Implicit Assumptions Audit row 7.
- **(3) State-machine authority** (forward-looking) — substrate rejects invalid transitions with `WalletError::InvalidStateTransition { from: AgentState, to: AgentState }`. The `#[non_exhaustive]` enum attribute permits future expansion (e.g., `Paused`, `Draining` per RFC-0002-v2 amendment). Canonical transitions per RFC-0002 §Agent State Machine substrate diagram: `Registered → Running`, `Running → Terminated`, `Running → Registered`; `Terminated` is terminal.
- **(4) Idempotency carve-out** (forward-looking) — `transition_agent(caller_did, uuid, current_state, _)` on a **terminal** state (`Terminated`) is **NOT** idempotent (would mask replay attacks); it returns `WalletError::InvalidStateTransition { from: Terminated, to: Terminated }` per TV-WLT-AGT-5. Same-state transitions on non-terminal states (`Registered → Registered`, `Running → Running`) remain idempotent no-ops.
- **(5) Audit append + rollback contract** (forward-looking) — every successful transition appends an `AuditEventKind::AgentTransition { agent_id, from, to, reason, at_unix }` row to the canonical `AppendOnlyAuditSink` per RFC-0012 (post-RFC-0012-v2). The substrate-faithful rollback contract is enumerated below — see RFC-0016 §6.4 (canonical `octo_audit::AuditError` re-export shadows the substrate `octo_audit_core::AuditError`; façade-type name canonical in `octo_audit` scope; substrate reachable via fully-qualified `octo_audit_core::AuditError` path); `SinkSpecific`/`AlreadyExists`/`SequenceGap` live in `octo_audit_core::AuditError` per `crates/octo-audit-core/src/error.rs`. On ANY `octo_audit::AuditError` variant returned from `append_audit_event` (post-RFC-0012-v2 acceptance):
  - `AuditError::SinkSpecific(_)` — transient sink failure, ROLLBACK + propagate.
  - `AuditError::SequenceGap { event_id, prev }` — preceding audit rows missing, ROLLBACK + propagate (fail-closed).
  - `AuditError::AlreadyExists(event_id)` — IDEMPOTENT RE-APPEND: the event_id already persisted (prior call succeeded). Do NOT rollback; recognize previous success, return existing chain-hash as success. Do NOT propagate as failure.
  - Defense-in-depth: any FAIL-CLOSED audit-append failure (SinkSpecific / SequenceGap) MUST trigger rollback. The audit append is the source of truth for the transition log; in-memory state is the source of truth for current `AgentState`. The AlreadyExists variant is NOT a failure — it is the substrate-canonical idempotency signal. Note that `AuditError::AuditAppendFailed` does **not** exist as a substrate variant (per `crates/octo-audit-core/src/error.rs` §`AuditError` enum, hard-checked 2026-09-11).
- **(6) Reason length + control-char filter** (forward-looking) — empty string allowed; ≤256 chars enforced (longer → `WalletError::ReasonTooLong(usize)`); **control characters (`U+0000`-`U+001F`, `U+007F`) are REJECTED upfront** before any state-machine work (MEDIUM sec fix) — the audit log treats the reason as a UTF-8 string and downstream redaction / display tooling can mishandle embedded control bytes. ASCII printable + non-control Unicode (U+0020+) is allowed; control chars U+0000-U+001F and U+007F are rejected per §6.2.2 (Reason length + control-char filter). Non-UTF-8 input rejected at CLI (substrate trust). **C1 range gap follow-on note (R20.5 finding M-2):** the filter scope is `U+0000`-`U+001F` + `U+007F`; the C1 range (`U+0080`-`U+009F`, including `U+0085` NEL, `U+009B` 8-bit CSI, `U+009D` 8-bit OSC) passes the filter (per TV-WLT-AGT-13f). The gap is documented as accepted residual in §Adversary Analysis + §6.2.5 "C1 range gap (R20.5 finding M-2)" note; widening the filter to include the C1 range would require RFC-0015-v2 substrate amendment. Substrate-faithful principle: substrate today does not have a `validate_reason_c1_strict` primitive; the accepted gap is the substrate-faithful cost of accepting before the C1 filter ships.
- **(7) Single-callsite per agent** (forward-looking) — concurrent `transition_agent` against the same `(holder_did, agent_id)` returns `WalletError::AlreadyInTransition` (transient, retry-safe; CLI surfaces as substrate reason).

#### §6.2.3 `WalletError::AgentNotFound(Uuid)`

```rust
/// Agent UUID not found in the active DID's registry.
#[error("agent not found: {0}")]
AgentNotFound(Uuid),
```

- Mirrors the existing `WalletError::AgentAlreadyExists(Uuid)` shape (see §`WalletError` enum) for symmetry.
- CLI maps to `OctoCliError::AgentNotFound(Uuid)` (exit 42 per RFC-0011-c §9.8 slot allocation).
- Substrate-faithful mapping: substrate carries the UUID; CLI surfaces the canonical hyphenated form to scripting consumers.
- **Symmetry-only note (R20.5 finding M-12):** at R4.5 KEEP, `WalletError::AgentNotFound(Uuid)` is **dead code** in the substrate — no KEEP source raises this variant (per §6.3 R2 reconciliation note table row "AgentNotFound"). The variant is enumerated as KEEP because it mirrors the existing `WalletError::AgentAlreadyExists(Uuid)` shape (substrate-faithful symmetry pattern per [[cipherocto-design-principles]] §Composition over inheritance) and will be raised by the DEFERRED `transition_agent` write path per §6.2.2 (3) state-machine authority + TV-WLT-AGT-6 existence check. At RFC-0015 R4.5 acceptance, the variant is present in the `WalletError` enum (additive) but unreachable from any KEEP caller; this is the substrate-faithful cost of accepting the variant before the write path lands (no phantom source per `no-phantom-mission-pointers` rule applied to type space). The dead-code status is documented here to prevent accidental removal during refactors (the variant IS load-bearing for the post-RFC-0012-v2 write surface).

#### §6.2.4 `WalletError::ForbiddenHolderMismatch`

```rust
/// Caller-attested DID does not match filter's `holder_did` field.
/// SECURITY (HIGH — multi-DID enumeration prevention per §6.2.1).
#[error("forbidden: holder DID mismatch")]
ForbiddenHolderMismatch,
```

- **Where raised:** `list_owned_agents(caller_did, filter)` when `filter.holder_did.is_some()` and `filter.holder_did != caller_did` (per §6.2.1 caller-attestation enforcement).
- **CLI mapping:** Exit code 13 → `OctoCliError::PermissionDenied` (per RFC-0011 §Exit Codes).
- **Substrate-faithful note:** Additive variant mirroring `AgentNotFound(Uuid)` shape; no canonical substrate variant exists at R2 acceptance.

#### §6.2.5 `validate_reason` — primitive string-level filter (KEEP per R20.5)

**Status:** KEEP at RFC-0015 R4.5 acceptance — substrate-faithful primitive; pure string-level filter with no substrate amendment required. The primitive is exposed at the substrate boundary for both the DEFERRED `transition_agent` write path (per §6.2.2 (6)) AND for unit-test coverage on its own (per Test Vectors TV-WLT-AGT-13..13f). Listing as a KEEP item closes the implicit TV-WLT-AGT-13 anchor gap (R20.5 finding H-3: `validate_reason` was implied by TV-WLT-AGT-13 but had no §6.2 KEEP anchor).

```rust
/// Substrate-faithful control-character filter primitive.
///
/// Substrate-faithful to the control-char filter contract documented
/// in §6.2.2 (6) "Reason length + control-char filter" for the DEFERRED
/// `transition_agent` write path; the filter is exposed as a primitive at
/// the substrate boundary so future callers (CLI, wallet-on-host daemon,
/// programmatic API) inherit the same defense-in-depth filter.
///
/// Returns:
/// - `Ok(())` if `reason` is empty OR contains only ASCII printable +
///   non-control Unicode (`U+0020+`); per §6.2.2 (6) empty reason is
///   allowed and the C1 range (`U+0080`-`U+009F`) is NOT rejected by
///   this primitive (documented gap per §Adversary Analysis + R20.5
///   finding M-2 follow-on).
/// - `Err(WalletError::ReasonContainsControlChars)` if `reason` contains
///   any control character in `U+0000`-`U+001F` or `U+007F`. NOTE: this
///   variant is DEFERRED per §6.8 row 5; the primitive's return type
///   is the DEFERRED variant so any caller of the primitive inherits
///   the substrate-faithful error contract. The primitive lands at
///   R4.5 with the KEEP surface; the variant lands with `transition_agent`
///   re-implementation at Phase 2.5.
pub fn validate_reason(reason: &str) -> Result<(), WalletError>;
```

- **Where raised:** called from `transition_agent` (DEFERRED per §6.2.2 (6)) as the upstream control-char filter before any state-machine work. Exposed as a substrate-faithful primitive at R4.5 KEEP for unit-test coverage (per TV-WLT-AGT-13..13f) and for any future caller (CLI parser, programmatic API, wallet-on-host daemon) that wants the same defense-in-depth filter without depending on the write path.
- **Length semantics:** the primitive is filter-only (no length enforcement); the 256-char cap is the responsibility of `transition_agent` (DEFERRED per §6.2.2 (6)). CLI parser caps length at parse time per RFC-0011-c §Security considerations.
- **UTF-8 semantics:** the primitive operates on `&str`; non-UTF-8 input is rejected at the CLI parser boundary (substrate trust; per §6.2.2 (6) "Non-UTF-8 input rejected at CLI").
- **CLI mapping:** the primitive is NOT directly surfaced by CLI at R4.5 KEEP (CLI does not invoke substrate primitives directly; substrate primitives are invoked by `transition_agent` per §6.2.2 (6)). CLI surface remains the read-only mission set per §6.5.
- **Substrate-faithful note:** the primitive lands at R4.5 KEEP because it requires NO substrate amendment (string-level filter on UTF-8 input; pure function). The DEFERRED error variant it returns lands with `transition_agent` re-implementation per §6.8 row 5; the primitive itself is callable today as `validate_reason(reason: &str) -> Result<(), WalletError>` once `WalletError::ReasonContainsControlChars` lands (paired with `transition_agent` re-implementation). Until then, the primitive is exercisable only in `octo-wallet` crate-level unit tests (the TV-WLT-AGT-13 series) using the substrate's internal error contract.
- **C1 range gap (R20.5 finding M-2):** the primitive rejects only `U+0000`-`U+001F` and `U+007F` (per §6.2.2 (6)); the C1 range (`U+0080`-`U+009F`, including `U+0085` NEL, `U+009B` 8-bit CSI, `U+009D` 8-bit OSC) passes the filter (per TV-WLT-AGT-13f). The gap is documented in §Adversary Analysis and is the accepted residual pending RFC-0015-v2 substrate amendment that may widen the filter to include the C1 range. Modern UTF-8 terminals render C1 range safely; some legacy / non-UTF-8 terminals may interpret them as control sequences. Substrate-faithful note: substrate today does not have a separate `validate_reason_c1_strict` primitive; widening the filter would require a future amendment.

### §6.3 Error envelope

`WalletError` variants (additive; `#[non_exhaustive]` is already in scope). The write-path variants are DEFERRED pending RFC-0012-v2 acceptance — see §6.8 DEFERRED SURFACE.

| Variant                                                   | Source                                                                                                                     | CLI exit                      | RFC-0011-c reference        |
| --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ----------------------------- | --------------------------- |
| `AgentNotFound(Uuid)` (NEW, KEEP)                         | DEFERRED `transition_agent` (no KEEP source — variant mirrors `AgentAlreadyExists` shape per substrate-faithful principle) | `AgentNotFound` (42)          | §9.8 slot 42                |
| `ForbiddenHolderMismatch` (NEW, KEEP)                     | `list_owned_agents` (caller/filter DID mismatch per §6.2.1 HIGH sec fix)                                                   | `PermissionDenied` (13)       | parent RFC-0011 §Exit Codes |
| `AlreadyInTransition(Uuid)` (NEW, **DEFERRED**)           | `transition_agent` (write-only; concurrent call)                                                                           | `InvalidStateTransition` (43) | §9.8 slot 43                |
| `InvalidStateTransition { from, to }` (NEW, **DEFERRED**) | `transition_agent` (write-only; illegal transition)                                                                        | `InvalidStateTransition` (43) | §9.8 slot 43                |
| `ReasonTooLong(usize)` (NEW, **DEFERRED**)                | `transition_agent` (write-only; reason > 256 chars)                                                                        | `InvalidFilter` (16)          | parent reserved             |
| `ReasonContainsControlChars` (NEW, **DEFERRED**)          | `transition_agent` (write-only; control-char filter per §6.2.2 (Reason length + control-char filter))                      | `InvalidFilter` (16)          | parent reserved             |
| `AuditUnavailable` (NEW, **DEFERRED**)                    | `transition_agent` (write-only; `AppendOnlyAuditSink` unreachable per RFC-0012)                                            | `AuditSubstrateNotReady` (52) | RFC-0011-c §9.8 slot 52     |
| `AgentAlreadyExists(Uuid)` (EXISTING)                     | `register_agent`                                                                                                           | `AgentAlreadyExists` (41)     | §9.8 slot 41                |

**R2 reconciliation note — `AlreadyInTransition` source:** R1 v1.0 had `AlreadyInTransition` reachable from `list_owned_agents` reads during registry writes. R2 commits to **write-only** per §6.2.2 (the registry read is single-writer internally; reads during writes are serialized via the substrate-internal lock and never surface this variant). The variant is reachable exclusively from concurrent `transition_agent` calls against the same `(holder_did, agent_id)`.

CLI mapping follows RFC-0011-a §7.4 Substrate `[ADD]` error-envelope pattern (substrate variant → CLI variant → exit code) byte-for-byte.

### §6.4 Substrate-faithful state-machine table

The table below documents the substrate-canonical state machine surface (see §AgentState enum). RFC-0002 §Agent State Machine's spec diagram declares the five-state `ACTIVE/BUSY` working substates; the substrate collapses these to `Running`. RFC-0015 documents both forms; CLI consumers operate on the substrate three-state form.

> **§6.4 status note:** The state-machine table below is **informational** on RFC-0015 R2 acceptance — no substrate state-machine write surface is added at R2 (DEFERRED to RFC-0012-v2 + RFC-0015 substrate re-implementation per §6.8). The table documents the substrate-canonical three-state form for CLI consumer reference.

| Substrate variant        | Spec diagram equivalent                     | CLI rendering          | Allowed transitions FROM (substrate) |
| ------------------------ | ------------------------------------------- | ---------------------- | ------------------------------------ |
| `AgentState::Registered` | `REGISTERED`                                | `state = "registered"` | `Running`                            |
| `AgentState::Running`    | `ACTIVE` or `BUSY` (per substrate collapse) | `state = "running"`    | `Terminated`, `Registered`           |
| `AgentState::Terminated` | `TERMINATED` (terminal)                     | `state = "terminated"` | NONE (terminal)                      |

> **Substrate-faithful drift (replaces RFC-0002 §Agent State Machine spec diagram until RFC-0002-v2 lands):** the working substate pair `ACTIVE ↔ BUSY` is collapsed into the single observable `Running` variant. Operators querying agent state see `Running` for both currently-idle (was ACTIVE) and currently-executing (was BUSY) agents. The `AuditEventKind::AgentTransition` log row carries the prior `BUSY/ACTIVE` distinction internally (substrate collapses for the registry, preserves for audit) so audit log analysis retains the working/non-working split.

### §6.5 CLI integration contract

CLI missions consuming this substrate:

| Mission                              | Substrate call                                                                                               | Sub-step                   |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------ | -------------------------- |
| `0011-c-agent-create-subcommand.md`  | `octo_wallet::cli_fns::register_agent` (EXISTING)                                                            | Sub-step 3 (existing)      |
| `0011-c-agent-list-subcommand.md`    | `octo_wallet::list_owned_agents(caller_did, &filter)` (NEW per §6.2.1)                                       | Sub-step 3 (NEW, **KEEP**) |
| `0011-c-agent-run-subcommand.md`     | `transition_agent(caller_did, uuid, Running, None)` then `octo_runtime::spawn_agent` — **DEFERRED** per §6.8 | Sub-step 3 (**DEFERRED**)  |
| `0011-c-agent-destroy-subcommand.md` | `transition_agent(caller_did, uuid, Terminated, reason)` then audit append — **DEFERRED** per §6.8           | Sub-step 3 (**DEFERRED**)  |
| `0011-c-agent-attach-subcommand.md`  | (read-only; `transition_agent` is N/A; uses `octo_runtime::attach`) — substrate is read-only at R2           | N/A                        |

Each mission's accepted-state precondition is checked locally against the `AgentState` returned by `octo_wallet::register_agent` + `list_owned_agents` calls; the substrate is source of truth, not the CLI. RFC-0015 R2 acceptance unblocks the **read-only** missions (`create`, `list`, `attach`); write-path missions (`run`, `destroy`) remain gated on RFC-0012-v2 acceptance per §6.8 DEFERRED SURFACE.

### §6.6 Determinism requirements

- **Read determinism** — `list_owned_agents` returns the same result for the same input across runs (substrate is single-writer; the registry is in-memory + persisted atomically per write); sorting is deterministic (`registered_at_unix DESC`, with secondary sort by `agent_id` canonical UUID v7 ASC as deterministic tiebreaker per §6.2.1).
- **Transition determinism** — `transition_agent` is idempotent on same-state; deterministic on first-transition success (state machine is fully ordered: `Registered ↔ Running → Terminated`).
- **Audit log integrity** — every `transition_agent` success appends a BLAKE3-256 entry to `AppendOnlyAuditSink` (RFC-0012); the chain is `verify_chain`-able end-to-end.
- **Exit codes stable** — substrate error variants map to stable CLI exit codes (see §6.3) per parent RFC-0011 §Error Handling.

### §6.7 RFC-0008 Execution Class Mapping

| Operation                              | Execution class | Rationale                                                                                      |
| -------------------------------------- | --------------- | ---------------------------------------------------------------------------------------------- |
| `list_owned_agents`                    | Class A (read)  | No state mutation; observable in any environment                                               |
| `transition_agent`                     | Class B (write) | State mutation gated by substrate state machine; CLI confirmation gate — **DEFERRED** per §6.8 |
| `WalletError::AgentNotFound`           | Class A         | Pure error mapping                                                                             |
| `WalletError::ForbiddenHolderMismatch` | Class A         | Pure error mapping; no state mutation; HIGH sec fix per §6.2.1                                 |

CLI surfaces `Class A` operations unconditionally (no `--allow-write` gate per parent §Confirmation Flag Matrix); `Class B` operations require `--confirm` per RFC-0011 §Confirmation Flag Matrix.

### §6.8 DEFERRED SURFACE (cross-substrate features awaiting RFC-0012-v2 acceptance)

The R2 scope-cut DEFERs the following cross-substrate features. Each entry lists the substrate amendment required, the current substrate reality (hard-checked 2026-09-11), and the unblock condition. Acceptance of RFC-0015 at R2 does **not** authorize these features; they require a future amendment cycle.

| Deferred feature                                           | Substrate amendment required                                                                                | Current substrate reality (hard-checked)                                                                                                                       | Unblock condition                                                                                                                                                                                                                                         |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `transition_agent(caller_did, uuid, target, reason)` write | RFC-0012-v2: `AuditEventKind::AgentTransition { agent_id, from, to, reason, at_unix }` in `octo-audit-core` | `AuditEventKind` has 3 variants only (`Insert`, `Revoke`, `Sync`); NO `AgentTransition` variant. `crates/octo-audit-core/src/event.rs` §`AuditEventKind` enum. | RFC-0012-v2 ACCEPTED + `transition_agent` re-implemented per §6.2.2 (with `caller_did` parameter; holder_did resolved by uuid-lookup; rollback per §6.2.2 (covers SinkSpecific fail-closed + SequenceGap fail-closed + AlreadyExists idempotent-success)) |
| `WalletError::AlreadyInTransition(Uuid)`                   | Paired with `transition_agent` (write-only per §6.3 R2 reconciliation)                                      | Not in `WalletError` enum. See `crates/octo-wallet/src/error.rs` §`WalletError` enum.                                                                          | Same as `transition_agent` (unblocked together)                                                                                                                                                                                                           |
| `WalletError::InvalidStateTransition { from, to }`         | Paired with `transition_agent`                                                                              | Not in `WalletError` enum                                                                                                                                      | Same as `transition_agent`                                                                                                                                                                                                                                |
| `WalletError::ReasonTooLong(usize)`                        | Paired with `transition_agent` (reason length per §6.2.2 (Reason length + control-char filter))             | Not in `WalletError` enum                                                                                                                                      | Same as `transition_agent`                                                                                                                                                                                                                                |
| `WalletError::ReasonContainsControlChars`                  | Paired with `transition_agent` (control-char filter per §6.2.2 (Reason length + control-char filter))       | Not in `WalletError` enum                                                                                                                                      | Same as `transition_agent`                                                                                                                                                                                                                                |
| `WalletError::AuditUnavailable`                            | Paired with `transition_agent` (audit substrate unavailable per §6.2.2 rollback contract)                   | Not in `WalletError` enum                                                                                                                                      | Same as `transition_agent`                                                                                                                                                                                                                                |
| Audit append of `AgentTransition` row                      | RFC-0012-v2 (same as above) + RFC-0016-v2 (audit receipt API `append_audit_event` write path)               | `AppendOnlyAuditSink::append` exists in `octo-audit-core` but no `AuditEventKind::AgentTransition` to append.                                                  | RFC-0012-v2 + RFC-0016-v2 ACCEPTED                                                                                                                                                                                                                        |

**Reconciliation note (R1 → R2):** R1 v1.0 proposed the full write surface; R2 commits to read-only surface + documents the write surface as DEFERRED with explicit unblock conditions. This avoids substrate amendments at the RFC-0015 acceptance boundary (Layer A frozen per CLAUDE.md §Rust crate-level stability).

## Performance Targets

- `list_owned_agents` with 1000-agent registry: p95 < 5ms (in-memory registry walk; substrate does not touch disk for the read).
- `transition_agent` happy path: p95 < 10ms — **DEFERRED** per §6.8; no substrate write path at R2.
- `transition_agent` audit append (RFC-0012 chain): p95 < 2ms in-process — **DEFERRED** per §6.8.

## Implicit Assumptions Audit

1. **Single-writer per `(holder_did, agent_id)`** — DEFERRED per §6.8. The substrate does NOT add per-`(holder_did, agent_id)` write serialization at R2 (no write path). The read path is single-writer internally per `AgentFilter` walk.
2. **Substrate is canonical for `AgentState`** — CLI never pattern-matches on `AgentState` string representation; serde-derived lowercase string is for display only per `#[serde(rename_all = "lowercase")]` on the enum.
3. **Reason string is UTF-8, ≤256 chars, no control chars** — DEFERRED per §6.8 for the write path; CLI parser caps length at parse (exit 16 per `OctoCliError::InvalidFilter`) per RFC-0011-c §Security considerations; substrate enforces control-char filter (`U+0000`-`U+001F`, `U+007F`) at the substrate boundary per §6.2.2 — defense-in-depth (CLI parses, substrate filters). Substrate trust assumption (when write path unblocks) per §6.2.2 (Reason length + control-char filter).
4. **`register_agent` precedes any read** — substrate assumes the agent_id returned by `register_agent` exists in the in-memory registry before any `list_owned_agents` call returns a non-empty `Vec`; CLI precondition check.
5. **Audit substrate is available** — DEFERRED per §6.8 (paired with `transition_agent` write path).
6. **Operator config dir writable** — agent registry persists to `$OCTO_HOME/wallet/agents`; CLI surfaces `OctoCliError::NoOctoHome` upstream (exit 27 per `crates/octo-cli/src/error.rs` §`NoOctoHome`).
7. **`caller_did` provenance from active `IdentityHandle` (HSM-bound)** — R20.5 finding H-5 substrate-faithful note: the substrate does NOT authenticate `caller_did`; it only enforces `caller_did == agent.holder_did` (per §6.2.1 + §6.2.2 (2)). A fabricated `caller_did` (compromised CLI passing an arbitrary DID string) would pass the substrate check trivially. The CLI / programmatic caller MUST source `caller_did` from the active `IdentityHandle` (HSM-bound per RFC-0009 §Identity Struct + §HsmAdapter Integration) at the Layer B façade boundary. The substrate-faithful pattern: `caller_did: &Did` is borrowed from an HSM-bound `IdentityHandle` retrieved from process session state. The substrate does not look up the HSM itself. Per-process trust boundary assumed at the façade boundary; the substrate has no visibility into HSM provenance.

## Security Considerations

1. **State-machine integrity** — DEFERRED per §6.8. R2 acceptance does NOT add a substrate state-machine write surface; integrity guarantees land with RFC-0012-v2 acceptance.
2. **Audit immutability** — DEFERRED per §6.8. R2 acceptance does NOT add the `AgentTransition` audit-append path; immutability guarantees land with RFC-0012-v2 + RFC-0016-v2 acceptance.
3. **Reason field is operator-controlled** — DEFERRED per §6.8 for the write path. When write path unblocks: CLI parser caps at 256 chars + UTF-8 (exit 16 on parse failure per `OctoCliError::InvalidFilter`); substrate enforces control-char filter (`U+0000`-`U+001F`, `U+007F`) at the substrate boundary per §6.2.2 (Reason length + control-char filter) (MEDIUM sec fix). Defense-in-depth: CLI parses length/UTF-8 at parse time; substrate rejects control chars at write time so any future caller (CLI, wallet-on-host daemon, programmatic API) inherits the same control-char filter. Substrate does not interpret the string as code; no shell metachar escape surface. ASCII printable + non-control Unicode (U+0020+) is allowed; control chars U+0000-U+001F and U+007F are rejected per §6.2.2 (Reason length + control-char filter).
4. **`AgentNotFound` does not leak existence** — substrate returns the variant for unknown UUIDs; enumeration attacks via timing differences are mitigated by **substrate-internal** registry lookups (substrate-faithful `BTreeMap::get` indexed by UUID — substrate does NOT enforce constant-time per R20.5 finding M-5; the timing oracle is documented as accepted residual risk in §Adversary Analysis row "Constant-time registry lookup (claim)"). The substrate-faithful registry uses BTreeMap indexing which is NOT constant-time (lookup time varies based on tree depth and distribution); size-based padding not yet implemented. Per-process trust boundary assumed at the façade boundary per §Implicit Assumptions Audit row 7.
5. **Reason string redacted at audit** — DEFERRED per §6.8 for the write path. When write path unblocks: `OctoCliRedactor` value-pattern sweep applied per RFC-0011-a §Redaction; no plaintext secrets leak into the audit log via the reason field. The reason field's 256-char cap + control-char filter (§6.2.2 (Reason length + control-char filter)) is the upstream defense-in-depth.
6. **`list_owned_agents` caller-attestation** (HIGH sec fix per §6.2.1) — `caller_did` is a required parameter; substrate rejects any filter whose `holder_did` differs from `caller_did` (`WalletError::ForbiddenHolderMismatch`). Closes the multi-DID enumeration attack surface. Audit append rollback (per §6.2.2 (Audit append + rollback contract)) covers `octo_audit::AuditError` variants — `SinkSpecific` and `SequenceGap` trigger fail-closed rollback of the in-memory state mutation; `AlreadyExists` is the substrate-canonical idempotent-retry signal (NOT a failure; no rollback, return existing chain-hash as success) per `crates/octo-audit-core/src/error.rs`.

## Adversarial Review

### Threat: replay-attack via cloned `transition_agent` call

**Adversary:** Operator retries a successful `transition_agent` call (e.g., re-runs the CLI on transient network failure).

**Mitigation:** State machine is idempotent on same-state (no-op success); transition to the same state cannot replay because the audit log already contains the prior transition row. New transition differs in `from` (the second call's `target == current_state`); substrate surface records transition AFTER first call.

### Threat: invalid transition bypass

**Adversary:** Compromised CLI binary attempts to invoke `transition_agent(uuid, Terminated)` on a `Registered` agent directly.

**Mitigation:** Substrate rejects per state machine; CLI cannot bypass the registry. RFC-0011-c §Security Confirmation Gate surfaces `ConfirmationRequired` for terminal-state transitions; runtime enforces per-process serialization.

### Threat: audit-log trim

**Adversary:** Operator attempts to delete audit log entries to hide a destructive transition.

**Mitigation:** `AppendOnlyAuditSink` is type-level append-only per RFC-0012; tampering breaks the BLAKE3 chain verification on next `verify_chain` call. CLI does not provide a delete primitive.

### Threat: reason-field XSS / control-char injection

**Adversary:** Compromised CLI / operator-supplied `--reason` payload containing ANSI escape sequences (`\x1b[...`), terminal control bytes (`\x07` bell, `\x08` backspace), or terminal emulator OSC sequences (`\x1b]...`) attempts to manipulate downstream tooling that renders the audit log (terminal pagers, log viewers, audit dashboards).

**Mitigation (DEFERRED per §6.8, paired with `transition_agent` write path):** substrate rejects reason strings containing any control character (`U+0000`-`U+001F`, `U+007F`) BEFORE any state-machine work per §6.2.2 (Reason length + control-char filter) (MEDIUM sec fix). The filter is applied at the substrate boundary, not at the CLI parser, so any future caller (CLI, wallet-on-host daemon, programmatic API) inherits the same defense. The reason string is stored verbatim as UTF-8 in the audit row's metadata; downstream redaction (RFC-0011-a §Redaction) treats it as opaque text and never re-interprets bytes as escape sequences. ASCII printable + non-control Unicode (U+0020+) is allowed; control chars U+0000-U+001F and U+007F are rejected per §6.2.2 (Reason length + control-char filter); the filter does not block legitimate Unicode (e.g., non-ASCII names, emoji in destroy reasons).

## Adversary Analysis (5-Question Test)

| Threat                                                                | Q1: Who?                  | Q2: What?                                 | Q3: Why?                                        | Q4: How mitigated?                                                                                                                                                                                                                                                                                                                                       | Q5: Residual risk?                                                                                                                                                                                                                                                                                     |
| --------------------------------------------------------------------- | ------------------------- | ----------------------------------------- | ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Replay of transition call                                             | Operator                  | Replay `Terminated` tx                    | Cover destructive intent                        | Idempotent on same-state (non-terminal) + audit row reorder + terminal-state carve-out (TV-WLT-AGT-5)                                                                                                                                                                                                                                                    | CLI retry surface; auto-retry disabled                                                                                                                                                                                                                                                                 |
| Invalid transition bypass                                             | Compromised CLI           | Direct state write                        | Skip CLI confirmation gate                      | Substrate state-machine guard is mandatory                                                                                                                                                                                                                                                                                                               | Substrate bug = total compromise (low)                                                                                                                                                                                                                                                                 |
| Audit-log trim                                                        | Operator                  | Delete audit row                          | Hide destructive intent                         | Append-only sink + BLAKE3 chain (DEFERRED per §6.8)                                                                                                                                                                                                                                                                                                      | Substrate storage failure (mitigated)                                                                                                                                                                                                                                                                  |
| Reason-field XSS                                                      | Compromised CLI           | Inject ANSI/OSC escape                    | Manipulate downstream renderer                  | Substrate control-char filter `U+0000`-`U+001F`, `U+007F` rejected (DEFERRED per §6.8 / §6.2.2 (Reason length + control-char filter))                                                                                                                                                                                                                    | LOW (C1 range gap per TV-WLT-AGT-13f — U+0085 NEL, U+009B 8-bit CSI, U+009D 8-bit OSC pass the filter; modern UTF-8 terminals render C1 safely, but some legacy / non-UTF-8 terminals may interpret them as control sequences). Substrate trust + documented C1 gap accepted as RFC-0015-v2 follow-on. |
| Multi-DID enumeration                                                 | Compromised CLI           | Cross-DID `holder_did`                    | Enumerate agents across DIDs                    | `caller_did` + `filter.holder_did` mismatch → `ForbiddenHolderMismatch` (HIGH sec fix per §6.2.1)                                                                                                                                                                                                                                                        | Caller-DID provenance gap (caller must source DID from session state)                                                                                                                                                                                                                                  |
| UUID echo in `AgentNotFound`                                          | Compromised CLI           | Echo unknown UUID in error payload        | Confirm UUID existence in target DID's registry | NONE — substrate-faithful `BTreeMap::get` is NOT constant-time per R20.5 finding M-5; the registry lookup timing reveals whether the UUID exists in the DID's registry (accepted residual risk).                                                                                                                                                         | Accepted low-severity leak (UUID is operator-supplied; echo confirms DID/UUID pairing exists, not existence; mitigation is per-process trust boundary)                                                                                                                                                 |
| Caller-DID provenance gap                                             | Compromised CLI           | Fabricate `caller_did` from process state | Enumerate agents owned by other DIDs            | HIGH sec fix `caller_did` parameter per §6.2.1; substrate enforces `filter.holder_did == caller_did`                                                                                                                                                                                                                                                     | Trust placed in CLI session-state derivation (per-process trust boundary)                                                                                                                                                                                                                              |
| Constant-time registry lookup (claim) — REMOVED per R20.5 finding M-5 | Compensated timing oracle | Measure lookup latency to infer existence | Existence side-channel on unknown UUIDs         | NONE — substrate-faithful `BTreeMap::get` is NOT constant-time (per R20.5 finding M-5); lookup time varies based on tree depth and distribution. Accepted residual risk; documented in §Security #4 + this row. Size-based padding not yet implemented; per-process trust boundary assumed at the façade boundary per §Implicit Assumptions Audit row 7. | Accepted low-severity residual risk; constant-time claim REMOVED from §Security #4 (the prior "BTreeMap::get padded with dummy-access" claim was a soft claim — the substrate does NOT perform dummy-access padding today)                                                                             |

## Economic Analysis

DEFER — agent operations have no direct token cost; cite RFC-0900+ (Role Economics) for any cost implications.

## Compatibility

1. **No breaking changes.** Three additive items (1 function + 2 error variants) on `octo-wallet` (Layer B years-stable); no existing public API modified.
2. **No `schema_version` bump.** The `OutputEnvelope<T>` envelope carries no new fields; CLI mission output schemas unchanged.
3. **No new exit codes for the new errors.** The two KEEP error variants `AgentNotFound (42)` and `ForbiddenHolderMismatch (13)` map to RFC-0011-c §9.8 reserved slots already documented. The slot-43 mappings (`AlreadyInTransition`, `InvalidStateTransition`) and slot-16 (`ReasonTooLong`, `ReasonContainsControlChars`) and slot-52 (`AuditUnavailable`) remain RESERVED for the post-RFC-0012-v2 acceptance write-path variants per §6.3 / §6.8 DEFERRED SURFACE — they are NOT consumed at RFC-0015 R4.5 KEEP.
4. **No new clap variants.** Existing `AgentAction` enum (RFC-0011-c §9.3 dispatch) absorbs the new substrate calls; the missions land their variant-per-subcommand as planned.

## Test Vectors

Substrate-level test vectors (`crates/octo-wallet/src/agent.rs` test module). Write-path vectors (TV-WLT-AGT-3 through TV-WLT-AGT-11) are **DEFERRED** per §6.8 — they exercise `transition_agent` which requires RFC-0012-v2 acceptance. TV-WLT-AGT-1, 2, 12, 13, 13b, 13c, 13d, 13e, 13f KEEP at R5.5; 3-11 DEFERRED per §6.8; the `validate_reason` primitive tests (TV-WLT-AGT-13 series) exercise a substrate function (`transition_agent`) that is itself DEFERRED but the primitive has standalone test coverage that lands with the post-RFC-0012-v2 substrate.

| #              | Substrate call                                                              | Input                                                                               | Expected Output                                                                                                                                    | Notes                                                                                      |
| -------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| TV-WLT-AGT-1   | `list_owned_agents(caller_did, &AgentFilter::default())`                    | Empty registry                                                                      | `Ok(vec![])`                                                                                                                                       | TV-AGT6 substrate echo                                                                     |
| TV-WLT-AGT-2   | `list_owned_agents(caller_did, &filter { limit: Some(100) })`               | 1000-agent registry                                                                 | `Ok(vec_of_100_summaries)` truncated                                                                                                               | Limit clamp                                                                                |
| TV-WLT-AGT-3   | `transition_agent(caller_did, uuid, Running, None)`                         | Registered agent                                                                    | `Ok(<summary with state=Running>)` + audit append                                                                                                  | **DEFERRED** per §6.8 — Happy path register→running                                        |
| TV-WLT-AGT-4   | `transition_agent(caller_did, uuid, Running, _)`                            | Running agent                                                                       | `Ok(<unchanged summary>)` + NO audit append (idempotent)                                                                                           | **DEFERRED** per §6.8 — Idempotent same-state                                              |
| TV-WLT-AGT-5   | `transition_agent(caller_did, uuid, Terminated, _)`                         | Terminated agent                                                                    | `Err(WalletError::InvalidStateTransition { from: Terminated, to: Terminated })`                                                                    | **DEFERRED** per §6.8 — Terminal state guard (NOT idempotent)                              |
| TV-WLT-AGT-6   | `transition_agent(caller_did, missing_uuid, _, _)`                          | Unknown UUID                                                                        | `Err(WalletError::AgentNotFound(uuid))`                                                                                                            | **DEFERRED** per §6.8 — Existence check                                                    |
| TV-WLT-AGT-7   | `transition_agent(caller_did, uuid, _, Some(long_str))`                     | Reason > 256 chars OR contains control char                                         | `Err(WalletError::ReasonTooLong(257))`                                                                                                             | **DEFERRED** per §6.8 — Length cap + control-char filter                                   |
| TV-WLT-AGT-8   | `transition_agent(caller_did, uuid, Terminated, _)`                         | Running agent                                                                       | `Ok(<summary with state=Terminated>)` + audit append                                                                                               | **DEFERRED** per §6.8 — Destroy path                                                       |
| TV-WLT-AGT-9   | `transition_agent(caller_did, uuid, Registered, _)`                         | Running agent                                                                       | `Ok(<summary with state=Registered>)` + audit append                                                                                               | **DEFERRED** per §6.8 — Restart-from-running                                               |
| TV-WLT-AGT-10  | `transition_agent(caller_did, uuid, _, _)`                                  | Concurrent call against same `(holder_did, agent_id)`                               | `Err(WalletError::AlreadyInTransition(uuid))`                                                                                                      | **DEFERRED** per §6.8 — Concurrent-call guard                                              |
| TV-WLT-AGT-11  | `transition_agent(caller_did, uuid, _, _)` with audit sink unavailable      | `AppendOnlyAuditSink` returns `AuditError::SinkSpecific(_)`                         | `Err(WalletError::AuditUnavailable)` + in-memory state rolled back                                                                                 | **DEFERRED** per §6.8 — Rollback contract (SinkSpecific trigger)                           |
| TV-WLT-AGT-11b | `transition_agent(caller_did, uuid, _, _)` with audit sink gap              | `append_audit_event` returns `AuditError::SequenceGap { event_id: 43, prev: 42 }`   | `Err(WalletError::AuditUnavailable)` + in-memory state rolled back                                                                                 | **DEFERRED** per §6.8 — Rollback contract (SequenceGap fail-closed trigger per §6.2.2 (5)) |
| TV-WLT-AGT-11c | `transition_agent(caller_did, uuid, _, _)` with audit sink idempotent retry | `append_audit_event` returns `AuditError::AlreadyExists(42)` (idempotent re-append) | `Ok(<existing_chain_hash>)` + state NOT rolled back (idempotent-success per §6.2.2 (5))                                                            | **DEFERRED** per §6.8 — Idempotent retry (AlreadyExists NOT a failure per §6.2.2 (5))      |
| TV-WLT-AGT-12  | `list_owned_agents(caller_did, &filter { holder_did: Some(other_did) })`    | Caller DID ≠ filter `holder_did`                                                    | `Err(WalletError::ForbiddenHolderMismatch)`                                                                                                        | HIGH sec fix (caller-attestation per §6.2.1)                                               |
| TV-WLT-AGT-13  | `validate_reason("ok")` (unit test on control-char filter primitive)        | ASCII printable reason                                                              | `Ok(())`                                                                                                                                           | Verifies filter accepts ASCII printable                                                    |
| TV-WLT-AGT-13b | `validate_reason("\x1b[31mred")` (unit test)                                | ANSI escape sequence reason                                                         | `Err(WalletError::ReasonContainsControlChars)` (DEFERRED variant, exposed for filter-primitive unit testing only)                                  | Verifies filter rejects `U+001B` ESC                                                       |
| TV-WLT-AGT-13c | `validate_reason("emoji-ok-🎉")` (unit test)                                | Non-control Unicode reason                                                          | `Ok(())`                                                                                                                                           | Verifies filter does NOT reject legitimate Unicode                                         |
| TV-WLT-AGT-13d | `validate_reason("\x00null")` (unit test)                                   | NUL byte reason                                                                     | `Err(WalletError::ReasonContainsControlChars)` (DEFERRED variant)                                                                                  | Verifies filter rejects `U+0000` NUL                                                       |
| TV-WLT-AGT-13e | `validate_reason("\r\n[ADMIN] approved")` (unit test)                       | CRLF-injection reason                                                               | `Err(WalletError::ReasonContainsControlChars)` (DEFERRED variant)                                                                                  | Verifies filter rejects `U+000A` LF / `U+000D` CR                                          |
| TV-WLT-AGT-13f | `validate_reason("\x85")` (unit test)                                       | C1 control (U+0085 NEL)                                                             | `Ok(())` (filter is U+007F scope per §6.2.2 (Reason length + control-char filter); C1 range gap documented — future RFC-0015-v2 may extend filter) | Verifies filter does NOT reject C1 range; documented gap                                   |

CLI-level test vectors live in RFC-0011-c §Test Vectors TV-AGT1..AGT-12 (UNCHANGED — RFC-0015 substrate alignment does not modify CLI TV).

## Alternatives Considered

- **Pure-CLI state-machine** — substrate delegates transition validation to CLI; rejected: violates substrate-faithful principle; CLI bypass becomes possible.
- **Async transition callbacks** — `transition_agent` returns a future and signals completion via a channel; rejected: adds runtime complexity for no operator-visible benefit; substrate sync semantics match RFC-0002 §Agent State Machine intent.
- **Composite state variants** — keep ACTIVE + BUSY in the substrate enum per RFC-0002 spec; rejected: substrate-faithful principle (the three-state form is canonical in the substrate); RFC-0002-v2 amendment may restore the split later.

## Implementation Phases

- **Phase 1 (this RFC, R4.5 KEEP)** — substrate function additions only; 3 KEEP items = 1 function (`list_owned_agents`) + 2 error variants (`WalletError::AgentNotFound(Uuid)` + `WalletError::ForbiddenHolderMismatch`) land on `octo-wallet` Layer B; no CLI changes; no `transition_agent`.
- **Phase 2 (RFC-0011-c read-only missions)** — CLI missions `0011-c-agent-{create,list,attach}-subcommand` consume the read surface; mutation traces per RFC-0011-c §Test Vectors.
- **Phase 2.5 (RFC-0012-v2 acceptance)** — `AuditEventKind::AgentTransition { agent_id, from, to, reason, at_unix }` variant lands in `octo-audit-core` (Layer A frozen). This unblocks §6.8 DEFERRED SURFACE: 7 DEFER items total = 6 RFC-0015 §6.8 items + row 7 audit append (paired with RFC-0016-v2 acceptance). The 6 §6.8 items are `transition_agent` + `WalletError::AlreadyInTransition` + `InvalidStateTransition` + `ReasonTooLong` + `ReasonContainsControlChars` + `AuditUnavailable` land at this phase; row 7 is the audit append of the `AgentTransition` event row (paired with RFC-0016-v2 `append_audit_event` write path).
- **Phase 3 (RFC-0011-c write missions)** — CLI missions `0011-c-agent-{run,destroy}-subcommand` consume the write surface (post-RFC-0012-v2 acceptance); mutation traces per RFC-0011-c §Test Vectors.
- **Phase 4 (RFC-0016 audit-receipt additions)** — companion amendment adds the `AgentTransition` audit-event filter to RFC-0011-a, enabling `octo audit list --model agent-transition` query shape.

## Key Files to Modify

- `crates/octo-wallet/src/agent.rs` — append `list_owned_agents` ONLY (existing types; ~40 LoC incl. tests). `transition_agent` is DEFERRED per §6.8 — its file edit lands with RFC-0012-v2 + RFC-0015 re-implementation, NOT at R4.5 KEEP acceptance.
- `crates/octo-wallet/src/error.rs` — append `WalletError::AgentNotFound(Uuid)` + `WalletError::ForbiddenHolderMismatch` (2 KEEP variants per §6.3). The DEFERRED variants `AlreadyInTransition(Uuid)` + `InvalidStateTransition { from, to }` + `ReasonTooLong(usize)` + `ReasonContainsControlChars` + `AuditUnavailable` are NOT added at R4.5 — they land with the RFC-0012-v2 + RFC-0015 re-implementation.
- `crates/octo-wallet/src/lib.rs` — re-export the new function (no breaking change to existing public surface).

No changes to Layer A crates (`octo-audit-core`, etc.); no CLI binary changes; no envelope / redactor / exit-code table changes.

## Future Work

- `transition_agent` batch API — multi-agent transition in one substrate call (Phase 4 RFC-0002-v2 companion).
- `list_owned_agents` cursor support — opaque cursor token for >1024-agent registries (Phase 4).
- RFC-0002-v2 amendment — restore the five-state ACTIVE/BUSY split if operator demand surfaces (out of scope here).

## Rationale

- **Substrate-faithful** — substrate is canonical per RFC-0012/0013/0014 acceptance pattern; the three-state `AgentState` enum is canonical even when it differs from the RFC-0002 spec diagram.
- **Additive only** — CLAUDE.md §Layer A stability: Layer B additive changes do not break consumers; the new function + 2 KEEP error variants are additive; the 5 DEFERRED variants land with RFC-0012-v2 per §6.8.
- **No parallel abstractions** — function names + parameter shapes mirror CLI mission call sites exactly (per [[cipherocto-design-principles]] §No parallel abstractions).
- **Substrate-owned invariants** — state-machine validation lives in the substrate; CLI cannot bypass. Per [[cipherocto-design-principles]] §Discipline at first call site pays off.

## Version History

- v1.0 (2026-09-11) Initial draft. Substrate-faithful `octo-wallet` read surface (RFC-0002 + RFC-0011-c).

## Related RFCs

- RFC-0011-c — `octo agent` Subcommands (CLI consumers; defines operator UX surface)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` + `AgentManifest` authority)
- RFC-0009 — Identity Management (lifecycle state-machine substrate precedent)
- RFC-0012 — Audit Substrate (`AppendOnlyAuditSink` for transition log appends); **RFC-0012-v2 required for §6.8 DEFERRED SURFACE**
- RFC-0016 — Audit Receipt API Substrate (companion amendment; adds `AgentTransition` audit-event filter); **RFC-0016-v2 required for §6.8 DEFERRED SURFACE**
- RFC-0011 — `octo` CLI Substrate (parent RFC; provides envelope + error + exit-code substrate)
- RFC-0010 — Canonical DID Codec (DID parsing for `holder_did` filter field)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping)
- [[cipherocto-design-principles]] — Layer model + substrate-faithful principle; **§6.8 DEFERRED SURFACE follows the extension-over-enumeration pattern (no central enum edit at Layer A)**

## Related Use Cases

- `docs/use-cases/agent-marketplace.md` — agent registration + verification flow context.
- `docs/use-cases/hybrid-ai-blockchain-runtime.md` — runtime attach / run context.

## Appendices

### Appendix A. Substrate function signatures (full Rust surface)

```rust
// crates/octo-wallet/src/agent.rs (append to existing module)

// KEEP at RFC-0015 R2 acceptance:
pub fn list_owned_agents(
    caller_did: &Did,
    filter: &AgentFilter,
) -> Result<Vec<AgentSummary>, WalletError> {
    // 1. Validate caller/filter DID consistency: substrate enforces
    //    `filter.holder_did.is_none() || filter.holder_did.as_deref()
    //    == Some(caller_did.as_str())`; mismatch → ForbiddenHolderMismatch.
    // 2. Walk the in-process registry; apply `filter.holder_did` (default active),
    //    `filter.state` (exact match), `filter.limit` (clamp 1024).
    // 3. Sort `Vec<AgentSummary>` by `registered_at_unix DESC`.
    // 4. Return.
}

// DEFERRED per §6.8 / §6.2.2 DEFERRED DESIGN (FORWARD-LOOKING ONLY;
// not implementable at R2 acceptance; requires RFC-0012-v2 acceptance):
#[cfg(feature = "deferred-rfc-0012-v2")]
pub fn transition_agent(
    caller_did: &Did,
    uuid: Uuid,
    target: AgentState,
    reason: Option<&str>,
) -> Result<AgentSummary, WalletError> {
    // 1. Validate reason (≤256 chars; UTF-8; reject control chars U+0000-U+001F, U+007F).
    // 2. Look up agent by `uuid` to resolve `holder_did`.
    // 3. Caller-attestation: enforce `caller_did == agent.holder_did`
    //    (else WalletError::ForbiddenHolderMismatch per §6.2.4).
    // 4. Acquire per-(holder_did, agent_id) lock AFTER holder_did resolved
    //    (TOCTOU mitigation per §6.2.2).
    // 5. RE-READ `current_state` INSIDE the lock; reject invalid transitions.
    // 6. Idempotent on same-state (non-terminal) — early return unchanged summary;
    //    NO audit row.
    // 7. Terminal-state same-state call → InvalidStateTransition (NOT idempotent).
    // 8. Mutate in-memory state map; persist to disk atomically.
    // 9. Append `AuditEventKind::AgentTransition { agent_id, from, to, reason, at_unix }`
    //    via RFC-0012-v2 + RFC-0016-v2 audit substrate.
    // 10. If append returns AuditError::SinkSpecific(_) OR AuditError::SequenceGap { .. } —
    //     ROLLBACK in-memory state + persist; return WalletError::AuditUnavailable (fail-closed).
    //     If append returns AuditError::AlreadyExists(event_id) — this is an IDEMPOTENT
    //     RE-APPEND (prior call succeeded); do NOT rollback; recognize existing chain-hash
    //     and return success. (Note: AuditError has SequenceGap / AlreadyExists / SinkSpecific
    //     per §6.2.2 rollback contract; AuditAppendFailed does not exist in substrate.)
    // 11. Release lock; return updated summary.
}
```

### Appendix B. Error envelope cross-reference table

| Substrate variant                                  | CLI variant                                         | CLI exit | RFC-0011-c slot | Status     |
| -------------------------------------------------- | --------------------------------------------------- | -------- | --------------- | ---------- |
| `WalletError::AgentNotFound(uuid)`                 | `OctoCliError::AgentNotFound(uuid)`                 | 42       | §9.8 slot 42    | KEEP       |
| `WalletError::ForbiddenHolderMismatch`             | `OctoCliError::PermissionDenied`                    | 13       | parent reserved | KEEP (NEW) |
| `WalletError::AlreadyInTransition(uuid)`           | `OctoCliError::InvalidStateTransition { from, to }` | 43       | §9.8 slot 43    | DEFERRED   |
| `WalletError::InvalidStateTransition { from, to }` | `OctoCliError::InvalidStateTransition { from, to }` | 43       | §9.8 slot 43    | DEFERRED   |
| `WalletError::ReasonTooLong(len)`                  | `OctoCliError::InvalidFilter(reason)`               | 16       | parent reserved | DEFERRED   |
| `WalletError::ReasonContainsControlChars`          | `OctoCliError::InvalidFilter(reason)`               | 16       | parent reserved | DEFERRED   |
| `WalletError::AuditUnavailable`                    | `OctoCliError::AuditSubstrateNotReady`              | 52       | RFC-0011-c §9.8 | DEFERRED   |

### Appendix C. Mermaid diagram — CLI → substrate → audit flow (R4.5 KEEP read-only)

```mermaid
sequenceDiagram
    participant Op as Operator
    participant CLI as octo-cli (Layer C/D)
    participant Wal as octo-wallet (Layer B)
    participant Settle as octo-agent-registry (in-process)

    Op->>CLI: octo agent list --state Running --limit 100
    CLI->>Wal: list_owned_agents(caller_did, &AgentFilter { state: Some(Running), limit: Some(100) })
    Wal->>Wal: validate caller_did == filter.holder_did (else ForbiddenHolderMismatch per §6.2.1)
    Wal->>Settle: walk in-process registry; filter + sort (registered_at_unix DESC)
    Settle-->>Wal: Vec<AgentSummary>
    Wal-->>CLI: Ok(Vec<AgentSummary>)
    CLI-->>Op: OutputEnvelope<AgentListOutput> exit 0
```

> **R4.5 scope-cut note:** the previous v1.0 sequence diagram illustrated the `transition_agent` write path through `octo-audit-core`. R4.5 replaces it with the read-only `list_owned_agents` flow matching the RFC-0016 §Appendix C R2 read-only stance. The write-path sequence (`octo agent destroy --reason`) is **DEFERRED** per §6.8 — it lands with RFC-0012-v2 + RFC-0015 re-implementation.
