# RFC-0015-b: `octo-wallet` Substrate-Defect Paired Amendment

## Status

Draft (2026-09-14)

> **Substrate-defect paired amendment per RFC-0015 + RFC-0015-a + RFC-0016 plateau declaration 2026-09-14.** This amendment restates surviving substrate defects (post-canonicalization against RFC-0015-a §6.1 canonical `std::sync::Mutex` GLOBAL `AGENT_REGISTRY` pattern + RFC-0012 Layer A frozen contract) as additive §X.1-§X.6 requirements. Paired with `missions/claimed/0015-b-substrate-defect-impl.md` per RFC-0015-a §6.5 paired-acceptance bridge.

## Authors

- Authored by `@cipherocto` per RFC-0015 amendment chain + `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`.

## Maintainers

- Maintainer: `@cipherocto` per RFC-0015 amendment chain.

## Summary

RFC-0015 + RFC-0015-a + RFC-0016 reached a stylistic plateau in the R12 + R13 DRY review loop. The plateau declaration (`docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` §Deferred substrate defects) surfaced 7 substrate defects. After canonicalization against the parent RFC-0015-a substrate (canonical `std::sync::Mutex` GLOBAL `AGENT_REGISTRY` per RFC-0015-a §6.1 (1) + §Alternatives Considered explicit REJECT of `parking_lot::Mutex::try_lock`), **5 substrate defects survive** with substantive substrate impact:

- **§X.1** `AlreadyInTransition` activation (in-flight flag inside canonical GLOBAL std Mutex; no new lock primitive)
- **§X.2** `lookup_agent` existence-leak closure (multi-DID enumeration attack surface)
- **§X.3** state-machine test coverage (2 genuinely new edges: `Terminated → Running` cross-state rejection + concurrent in-flight rejection)
- **§X.4** phantom-event detection forward-pointer (paired RFC-0012-v2 amendment for `state_version` + chain-tip accessor — Layer A frozen substrate)
- **§X.5** RFC-0015-a §Amendment Surface parity refresh (1 additive `AgentRecord` field + forward-pointer row)

The amendment is **strictly additive** — no breaking changes to existing public API per RFC migration etiquette. CLI behavior unchanged for all §X.x (error exit mappings pre-existing at parent RFC-0015-a acceptance); CLI security posture improves for §X.2 (no information leak on probe).

**Pairing invariant:** Acceptance of RFC-0015-b REQUIRES paired closure of mission `0015-b-substrate-defect-impl` per RFC-0015-a §6.5 Layer A Paired-Acceptance Bridge (the substrate code lands before RFC-0015-b can be marked Accepted in the RFC VH). RFC-0015-b Acceptance is the formal authorization step.

## Dependencies

**Requires:**

- RFC-0015 — `octo-wallet` Agent Operations Substrate (parent KEEP RFC)
- RFC-0015-a — `octo-wallet` Agent Write-Path Amendment (parent write-path RFC; provides `transition_agent` + `TransitionReceipt` substrate + canonical `std::sync::Mutex` lock pattern per §6.1 (1))
- RFC-0016 — `octo-audit` Receipt Read-Path API (parent audit read-path RFC)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers; benefits from §X.2 security posture improvement)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` + state-machine substrate authority per §Agent State Machine)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping per §RFC-0008 Execution Class Mapping)

**Substrate amendment dependencies (REQUIRED for §X.4 acceptance):**

- **RFC-0012-v2** — `AuditEventKind` additive extension + `audit_chain.tip.state_version: u64` accessor on `AppendOnlyAuditSink` in `octo-audit-core` (Layer A frozen per CLAUDE.md §Layer A stability). The §X.4 phantom-event detection contract reads the chain-tip accessor for post-rollback verification. RFC-0012-v2 is the canonical home for this Layer A addition; RFC-0015-b §X.4 is a forward-pointer (NOT a direct Layer A spec — Layer A additions require semver-major per CLAUDE.md §Layer A stability).

**Paired implementation mission dependencies:**

- `missions/claimed/0015-b-substrate-defect-impl.md` — the paired substrate implementation mission. Acceptance of RFC-0015-b REQUIRES this mission landing per RFC-0015-a §6.5 paired-acceptance bridge.

## Design Goals

1. **Substrate-defect closure** — each surviving canonical-substrate defect has a numbered §X.x requirement with explicit acceptance criteria + layer-model annotation + dependency declaration.
2. **`AlreadyInTransition` activation via canonical pattern** — in-flight `transitioning: bool` flag on `AgentRecord`, checked INSIDE the canonical GLOBAL `std::sync::Mutex` (parent RFC-0015-a §6.1 (1)). NO new lock primitive. NO new Cargo.toml dep.
3. **Existence-leak closure** — `lookup_agent` normalizes unknown + not-owned cases to `WalletError::AgentNotFound`, closing the multi-DID enumeration attack surface per parent RFC-0015 §Design Goals G6.
4. **State-machine test coverage** — 2 new canonical edges (`Terminated → Running` cross-state rejection + concurrent in-flight rejection).
5. **Phantom-event detection via paired amendment** — `state_version` + chain-tip accessor spec'd in RFC-0012-v2 (Layer A canonical home); RFC-0015-b §X.4 forward-points.
6. **RFC parity refresh** — RFC-0015-a §Amendment Surface updated to reflect `transitioning` field + RFC-0012-v2 forward-pointer row.

## Motivation

RFC-0015 + RFC-0015-a + RFC-0016 reached R12 (30 findings) + R13 (50 findings) — non-converging stylistic plateau. The plateau declaration recommended Option B (accept plateau + paired-acceptance amendment backlog) per R48 precedent.

The 7 substrate defects from the plateau declaration were re-examined against the canonical substrate patterns established at parent RFC-0015-a acceptance:

| #   | Defect                                          | Canonical-substrate verdict                                                                                                                                                              |
| --- | ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `AlreadyInTransition(Uuid)` dead surface        | REAL — activates via in-flight flag (no new primitive needed)                                                                                                                            |
| 2   | `AlreadyInTransition` doc-comment drift         | FOLDED into §X.1 impl mission (doc-comment update is sub-step hygiene)                                                                                                                   |
| 3   | `lookup_agent` existence-leak                   | REAL — closes enumeration side-channel                                                                                                                                                   |
| 4   | `transition_agent` TOCTOU window                | PHANTOM — `validate_reason` is pure function; canonical validate-first → lock-after → re-read inside lock pattern already closes the actual TOCTOU window per parent §6.1 (1) + §6.1 (6) |
| 5   | Phantom-event window on audit-append + rollback | REAL but split — `state_version` field is Layer B; chain-tip accessor is Layer A frozen (RFC-0012-v2 paired amendment)                                                                   |
| 6   | Missing state-machine tests                     | REAL — 2 new edges (rest duplicate parent RFC-0015-a TVs)                                                                                                                                |
| 7   | RFC-0015 §Pre-existing Substrate parity gap     | REAL but small — 1 row + 1 forward-pointer; targets RFC-0015-a §Amendment Surface (NOT RFC-0015 §Pre-existing Substrate which is the read-path KEEP RFC)                                 |

**5 defects survive canonicalization** as substantive §X.x requirements. Defect 2 is folded into the §X.1 impl sub-step (doc-comment hygiene is impl-time). Defect 4 is rejected as a substrate contract change (the canonical validate-first pattern per parent §6.1 (1) + §6.1 (6) is correct; the TOCTOU motivation was based on a flawed premise that `validate_reason` reads the registry, which it does not per parent §6.1 (6) — `validate_reason` is a pure function over the reason string).

## Roles and Authorities

| Role                | Authority                                                                               | Audit trail                              |
| ------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------- |
| Operator (human/CI) | Triggers `transition_agent` via CLI (per RFC-0011-c §9.3.2 + §9.3.4)                    | CLI log + audit append per RFC-0012      |
| Wallet substrate    | Source of truth for `AgentState`; rejects invalid transitions; activates in-flight flag | Internal state machine log               |
| Audit substrate     | Appends `AgentTransition` events to `AppendOnlyAuditSink` per RFC-0012                  | Append-only audit log (BLAKE3-256 chain) |
| CLI (octo-cli)      | Operator UX over substrate; never bypasses substrate state-machine                      | Same as operator                         |

## Specification

### §X.1 `AlreadyInTransition` activation via in-flight flag

**Defect:** `WalletError::AlreadyInTransition(Uuid)` declared at `crates/octo-wallet/src/error.rs` §WalletError but never constructed by `transition_agent` (parent RFC-0015-a §6.1 (7) reserves the variant for a future paired-acceptance substrate amendment per CLAUDE.md §Discipline at first call site).

**Requirement:** `transition_agent` MUST acquire the canonical GLOBAL `AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>>` `std::sync::Mutex` per parent RFC-0015-a §6.1 (1). Inside the lock, check an in-flight `transitioning: bool` flag on `AgentRecord` (additive field; default `false`). If `true` on entry → return `Err(WalletError::AlreadyInTransition(uuid))` WITHOUT state mutation and WITHOUT audit append. If `false` on entry → set `true`, proceed with state-machine work + audit append + rollback-or-success per parent §6.1 (3) + §6.1 (5), then set `false` on exit (RAII guard preferred for exception safety).

**Critical constraints (canonical substrate invariants preserved):**

- NO new lock primitive. The canonical `std::sync::Mutex::lock()` on GLOBAL `AGENT_REGISTRY` per parent §6.1 (1) is retained. Per parent §Alternatives Considered, the per-(holder_did, agent_id) `parking_lot::Mutex::try_lock` path is EXPLICITLY REJECTED — the canonical pattern is the GLOBAL std Mutex. The in-flight flag activation is INSIDE the GLOBAL std Mutex (not a parallel abstraction).
- NO new Cargo.toml dep.
- Lock acquisition order preserved (validate-first → lock-after → re-read current_state INSIDE lock per parent §6.1 (1) + §6.1 (6)). The in-flight flag check happens INSIDE the lock, AFTER the validate_reason step and AFTER the re-read of current_state (so the idempotent same-state early-return per parent §6.1 (4) runs before the in-flight flag is touched).
- Function signature preserved per parent §6.1: `transition_agent(caller_did: &Did, uuid: Uuid, target: AgentState, reason: Option<&str>) -> Result<TransitionReceipt, WalletError>`.

**Layer model:** Layer B substrate mutation. CLI behavior unchanged (`OctoCliError::AlreadyInTransition(Uuid)` exit 43 mapping pre-existing per parent RFC-0015-a §Appendix B).

**Doc-comment hygiene (sub-step folded from old §X.2):** the doc-comment on `WalletError::AlreadyInTransition` in `crates/octo-wallet/src/error.rs` MUST reference "in-flight `transitioning` flag inside canonical GLOBAL `std::sync::Mutex` (per RFC-0015-b §X.1)" rather than the legacy `parking_lot::Mutex::try_lock` reference. This is a one-line doc-comment update; no separate §X requirement.

**Acceptance Criteria:**

- [ ] `crates/octo-wallet/src/agent.rs` §AgentRecord carries `transitioning: bool` field (additive; default `false`; serde-renamed to `transitioning`)
- [ ] `transition_agent` reads `transitioning` flag INSIDE canonical GLOBAL `std::sync::Mutex` (parent RFC-0015-a §6.1 (1))
- [ ] If `transitioning == true` → return `Err(WalletError::AlreadyInTransition(uuid))`; NO state mutation; NO audit append
- [ ] If `transitioning == false` → set `true` (RAII guard on scope exit sets back to `false`); proceed with parent §6.1 (3) state-machine + §6.1 (5) audit append + rollback contract
- [ ] Lock acquisition order preserved per parent §6.1 (1) + §6.1 (6): validate-first → lock → re-read current_state INSIDE lock → in-flight flag check → state machine
- [ ] NO new Cargo.toml dep
- [ ] Doc-comment on `WalletError::AlreadyInTransition` updated to reference in-flight flag (per sub-step above)
- [ ] Test vector TV-WLT-AGT-29 verifies in-flight rejection via `AlreadyInTransition` (sequential: caller A enters lock + sets flag; caller B observes flag=true after A holds lock)

### §X.2 `lookup_agent` existence-leak closure

**Defect:** `lookup_agent(caller_did, uuid)` returns `WalletError::AgentNotFound` for unknown Uuid but `WalletError::ForbiddenHolderMismatch` for caller-DID mismatch. Existence leak enables multi-DID enumeration of agent ownership.

**Requirement:** `lookup_agent(caller_did, uuid)` MUST normalize BOTH unknown Uuid + caller-DID mismatch cases to `WalletError::AgentNotFound`. The `ForbiddenHolderMismatch` variant remains in the `WalletError` enum (used by `transition_agent` caller-attestation per parent RFC-0015-a §6.1 (2) where the distinction is meaningful and contributes to substrate-faithful caller-attestation error reporting).

**Layer model:** Layer B substrate mutation. CLI behavior unchanged (`OctoCliError::AgentNotFound(Uuid)` exit 42 mapping pre-existing). Security posture improves (no information leak on probe; closes enumeration attack surface per RFC-0015 §Design Goals G6).

**Acceptance Criteria:**

- [ ] `crates/octo-wallet/src/agent.rs` §lookup_agent normalizes both unknown + not-owned cases to `WalletError::AgentNotFound`
- [ ] `ForbiddenHolderMismatch` variant remains in `WalletError` enum (used by `transition_agent` per parent §6.1 (2))
- [ ] Test vector TV-WLT-AGT-24 verifies unknown Uuid → `Err(AgentNotFound(uuid))`
- [ ] Test vector TV-WLT-AGT-25 verifies not-owned Uuid (caller_did != holder_did) → `Err(AgentNotFound(uuid))` (existence-leak closure)
- [ ] Test vector TV-WLT-AGT-26 verifies caller-is-holder happy path → `Ok(AgentManifest)`

### §X.3 State-machine test coverage (2 new canonical edges)

**Defect:** `crates/octo-wallet/src/agent.rs` test module lacks coverage for 2 genuinely new state-machine edges. Existing RFC-0015-a TVs (TV-WLT-AGT-3, 4, 5, 6, 7, 8, 9, 10, 11, 11b, 11c) cover the canonical happy-path + idempotent + invalid-edge surface; 2 edges remain under-tested.

**Requirement:** substrate addition MUST include test coverage for the 2 genuinely new state-machine edges:

| Edge                                 | Test expected outcome                                                                                                                                                                                                                |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `Terminated → Running` (cross-state) | `Err(WalletError::InvalidStateTransition { from: Terminated, to: Running })` — terminal-state reactivation rejected per parent RFC-0015-a §6.4 state-machine table                                                                   |
| Concurrent in-flight rejection       | Caller A enters lock + sets `transitioning` flag per §X.1; caller B serializes at lock acquisition; caller B observes `transitioning == true` and returns `Err(WalletError::AlreadyInTransition(uuid))` (per §X.1 + parent §6.1 (7)) |

**Test approach:** use the canonical GLOBAL `std::sync::Mutex` ordering for deterministic test sequencing (NOT `std::thread::spawn` race — the canonical substrate test pattern uses sequential lock-ordered execution per parent RFC-0015-a §6.1 (1); concurrent-call substrate behavior is observable through this sequential pattern).

**Layer model:** test addition; no substrate behavior change beyond §X.1.

**Acceptance Criteria:**

- [ ] TV-WLT-AGT-27: `Terminated → Running` → `Err(WalletError::InvalidStateTransition { from: Terminated, to: Running })` per parent §6.4 state-machine table
- [ ] TV-WLT-AGT-28: concurrent in-flight rejection (sequential std Mutex ordering test) — caller B receives `Err(WalletError::AlreadyInTransition(uuid))` per §X.1
- [ ] NO duplication of parent RFC-0015-a TVs (TV-WLT-AGT-3, 4, 5, 6, 7, 8, 9, 10, 11, 11b, 11c)
- [ ] Use canonical TV numbering per parent RFC-0015-a §Test Vectors (TV-WLT-AGT-NN scheme)

### §X.4 Phantom-event detection — RFC-0012-v2 paired amendment forward-pointer

**Defect:** on audit-append failure + rollback per parent RFC-0015-a §6.1 (5), the in-memory registry state is reverted but the audit chain may have been partially appended. Phantom-event window: registry state appears reverted but audit chain contains a stale "transition committed" event.

**Requirement:** RFC-0015-b §X.4 forward-points to **RFC-0012-v2** paired amendment for the Layer A substrate changes required to detect the phantom-event window:

- **Layer A (RFC-0012-v2):** `state_version: u64` field appended to `AuditEvent` struct (alongside parent `at_millis_unix`) + `audit_chain.tip.state_version: u64` accessor on `AppendOnlyAuditSink` trait in `octo-audit-core` (additive; Layer A frozen requires semver-major per CLAUDE.md §Layer A stability).
- **Layer B (RFC-0015-b §X.4.1):** `state_version: u64` field on `AgentRecord` (default 0; bumped before each successful non-idempotent `transition_agent` call).
- **Layer B (RFC-0015-b §X.4.2):** post-rollback verification contract — after audit append failure + rollback per parent §6.1 (5), `transition_agent` reads `audit_chain.tip.state_version()` and asserts equality with the pre-rollback `state_version - 1`. Mismatch → `Err(WalletError::AuditChainInconsistent { observed: u64, expected: u64 })` (new additive variant per paired impl mission).

**Critical constraint:** RFC-0015-b does NOT spec the Layer A accessor directly. The accessor lives in RFC-0012-v2 (Layer A canonical home per CLAUDE.md §Layer A stability). RFC-0015-b §X.4 is a forward-pointer; the contract is conditional on RFC-0012-v2 acceptance.

**Layer model:** Cross-RFC amendment chain. RFC-0015-b (Layer B) forward-points to RFC-0012-v2 (Layer A amendment per CLAUDE.md §Layer A stability).

**Acceptance Criteria:**

- [ ] RFC-0015-b §X.4 forward-pointer declared (this section)
- [ ] RFC-0012-v2 paired amendment CREATED in `rfcs/draft/process/` (separate mission; not in RFC-0015-b scope but listed in §Future Work)
- [ ] Conditional §X.4 substrate impl: lands ONLY if RFC-0012-v2 acceptance precedes or is paired with RFC-0015-b acceptance; else deferred to a future amendment cycle
- [ ] Tests for §X.4.1 + §X.4.2 land ONLY with the conditional impl

### §X.5 RFC-0015-a §Amendment Surface parity refresh

**Defect:** RFC-0015-a §Amendment Surface lists substrate items at RFC-0015-a acceptance. Post-RFC-0015-b acceptance, additive items (§X.1 `transitioning` flag + §X.4.1 `state_version` field + RFC-0012-v2 forward-pointer) require a parity refresh.

**Requirement:** RFC-0015-a §Amendment Surface table refresh post-RFC-0015-b acceptance. Add rows for:

1. `AgentRecord.transitioning: bool` (additive field per §X.1; default `false`)
2. `AgentRecord.state_version: u64` (additive field per §X.4.1; default `0`; conditional on RFC-0012-v2 acceptance)
3. RFC-0012-v2 paired amendment forward-pointer row (Layer A accessor; conditional)

Add timestamp entries:

- "Substrate-defect amendment landing (RFC-0015-b Acceptance) 2026-09-14"

**Note:** this is a DOC-ONLY refresh + 1-2 additive `AgentRecord` field rows. The substrate code lives in the paired impl mission. The RFC text refresh lives in a paired RFC-0015-a VH entry, NOT in this RFC body (per substrate-faithful single-source-of-truth principle — RFC-0015-a §Amendment Surface is the canonical home for additive RFC-0015-a items).

**Layer model:** RFC text refresh (RFC-0015-a VH entry); no new RFC body content in RFC-0015-b.

**Acceptance Criteria:**

- [ ] 1 row added to RFC-0015-a §Amendment Surface (`transitioning: bool` per §X.1)
- [ ] 1 conditional row added (`state_version: u64` per §X.4.1; conditional on RFC-0012-v2)
- [ ] 1 forward-pointer row added (RFC-0012-v2 paired amendment)
- [ ] Timestamp column updated with RFC-0015-b Acceptance date
- [ ] Cross-reference to RFC-0015-b §X.1 + §X.4 added to RFC-0015-a §Amendment Surface VH entry (NOT RFC body — RFC body unchanged)

### §X.6 Cross-cutting — RFC-0008 Execution Class

| Operation                                    | Class | Rationale                                                          |
| -------------------------------------------- | ----- | ------------------------------------------------------------------ |
| `transition_agent` (post-§X.1 + §X.2 + §X.4) | B     | Substrate-level state-machine guard; deterministic per parent §6.1 |
| `lookup_agent` (post-§X.2 normalization)     | A     | Read; deterministic result per RFC-0015 §6.7 + parent §6.7         |

Per RFC-0008 §Execution Class Mapping, the substrate operation is the unit of classification; internal sub-steps inherit the parent's class. Sub-step elaboration is omitted (single source of truth at parent RFC-0015-a §6.8 + RFC-0015 §6.7).

**Layer model:** Cross-cutting; no substrate change beyond §X.1, §X.2, §X.4.

**Acceptance Criteria:**

- [ ] Row count remains 2 (no inflation; parent RFC-0015-a §6.8 is one row + RFC-0015 §6.7 is one row; §X.6 is the cross-cutting summary)
- [ ] No new Class A or Class C operations introduced

## Implicit Assumptions Audit

| Assumption                                                     | Where Relied Upon            | Blast Radius if False                                                  | Mitigation / Status                                                                                     |
| -------------------------------------------------------------- | ---------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Canonical GLOBAL `std::sync::Mutex` lock per parent §6.1 (1)   | §X.1 + §X.3                  | In-flight flag activation fails; concurrent callers cannot be rejected | Parent RFC-0015-a Acceptance substrate contract; no change required                                     |
| `transitioning` flag is additive (RFC-0015 KEEP compatibility) | §X.1                         | Pre-amendment builds cannot serialize/deserialize the field            | `#[serde(default)]` attribute; additive field with safe default                                         |
| `validate_reason` is pure (parent §6.1 (6))                    | Defect 4 rejection rationale | Defect 4 motivation resurrects; substrate contract change required     | Substrate-faithful verification: `validate_reason` impl per parent §6.1 (6) reads only the input string |
| RFC-0012-v2 paired amendment acceptance precedes §X.4          | §X.4                         | Chain-tip accessor absent; post-rollback verification cannot read      | RFC-0012-v2 mission scope (not in RFC-0015-b); conditional acceptance gates                             |
| `AgentRecord` is the canonical agent state holder              | §X.1 + §X.4.1                | In-flight flag + state_version placed in wrong struct                  | Substrate-faithful principle: RFC-0002 §Agent Manifest spec is canonical                                |
| Caller-attestation pattern (`caller_did == holder_did`) holds  | §X.2 + parent §6.1 (2)       | Existence-leak re-opens; multi-DID enumeration possible                | Test vectors TV-WLT-AGT-24 + 25 + 26 cover all three cases                                              |

## Security Considerations

### §X.S.1 Existence-leak closure (§X.2)

The pre-amendment substrate returned `AgentNotFound` for unknown Uuid but `ForbiddenHolderMismatch` for caller-DID mismatch. An attacker could probe for Uuids belonging to other DIDs by submitting a guessed Uuid and observing the error code. Post-§X.2, both cases return `AgentNotFound`, closing the enumeration side-channel.

**Severity:** HIGH (closes enumeration attack surface per RFC-0015 §Design Goals G6).

### §X.S.2 In-flight detection (§X.1)

The pre-amendment `WalletError::AlreadyInTransition(Uuid)` variant was dead surface (parent RFC-0015-a §6.1 (7) reserves for future paired-acceptance amendment). Post-§X.1, the in-flight `transitioning` flag activates the variant. Concurrent callers serialize at the canonical GLOBAL `std::sync::Mutex` lock per parent §6.1 (1); caller B observes caller A's in-flight flag and receives `Err(AlreadyInTransition(uuid))`. CLI exits 43 on contention; operator can retry.

**Severity:** MED (operational UX + concurrent-call substrate correctness; non-security-critical per parent §Adversary Analysis Concurrent-call lock contention row).

### §X.S.3 Phantom-event detection (§X.4 — conditional on RFC-0012-v2)

The pre-amendment substrate could leave the audit chain in an inconsistent state on partial append + rollback. Post-§X.4 (conditional on RFC-0012-v2 paired amendment acceptance), the post-rollback verification pass asserts `audit_chain.tip.state_version == current_state_version - 1`; mismatch surfaces `WalletError::AuditChainInconsistent { observed, expected }`.

**Severity:** MED (audit chain integrity; non-consensus-critical per parent §Adversary Analysis Audit-append failure → state inconsistency row). **Status:** §X.4 substrate impl is CONDITIONAL on RFC-0012-v2 acceptance. If RFC-0012-v2 is not accepted alongside RFC-0015-b, §X.4 is deferred and the residual risk is the pre-amendment phantom-event window.

## Adversary Analysis

| Decision                                            | Q1 Beneficiary                      | Q2 Cost to Attacker                       | Q3 Gain if Successful                           | Q4 Defense (cost to legit op)                                 | Q5 Residual Risk                                                    |
| --------------------------------------------------- | ----------------------------------- | ----------------------------------------- | ----------------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------- |
| `lookup_agent` normalize to `AgentNotFound` (§X.2)  | External attacker probing for Uuids | High (must guess valid Uuid + holder DID) | Multi-DID enumeration of agent ownership        | Normalization (zero legit cost)                               | LOW (residual: timing-attack on lock acquisition)                   |
| In-flight flag activation (§X.1)                    | Concurrent caller                   | Low (contention is observable)            | None (variant just surfaces existing condition) | Variant activation (zero legit cost; CLI exit 43)             | LOW (residual: false positives under high contention)               |
| `state_version` + post-rollback verification (§X.4) | Operator with audit-chain access    | Very high (must corrupt append-only sink) | Phantom-event observation                       | Monotonic counter + verification pass (negligible legit cost) | MED (residual: requires RFC-0012-v2 amendment for chain-tip access) |

### Severity Classification

| Severity | Definition                                                              | Action                                                                       |
| -------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| HIGH     | Existence-leak closure (§X.2)                                           | MUST mitigate before Accept; §X.2 closes this                                |
| MED      | In-flight detection (§X.1) + phantom-event detection (§X.4 conditional) | SHOULD mitigate before Accept; §X.1 closes + §X.4 conditional on RFC-0012-v2 |
| LOW      | State-machine tests (§X.3) + parity refresh (§X.5)                      | SHOULD mitigate; both closed                                                 |

## Economic Analysis

No economic implications. RFC-0015-b is a substrate-defect amendment; no token economics, dual-stake requirements, or market dynamics are affected.

## Compatibility

- **Backward:** additive only. All existing public API preserved (RFC-0015 + RFC-0015-a KEEP surface); `AgentRecord` gains `transitioning: bool` (default `false`; serde-defaulted) + conditional `state_version: u64` (default `0`; serde-defaulted; conditional on RFC-0012-v2). CLI exit code mappings pre-existing at parent RFC-0015-a acceptance for `AlreadyInTransition` (43) + `AgentNotFound` (42) + `AuditUnavailable` (52). New `AuditChainInconsistent` variant (additive; payload struct `{ observed, expected }`) maps to exit 52 per paired impl mission.
- **Forward:** additive fields with `#[serde(default)]` survive forward-compat (pre-amendment builds read post-amendment state with defaults applied). Layer A frozen contract preserved (no RFC-0012 changes in RFC-0015-b alone; §X.4 forward-points to RFC-0012-v2).
- **Wire form:** no wire form changes (audit chain hash already carries the version internally via the `AuditEventKind::AgentTransition` variant payload per parent RFC-0015-a §6.2 — the `state_version` field is a Layer B-side counter; the audit chain-tip accessor in RFC-0012-v2 reads the count from the `AuditEvent` chain itself).

## Test Vectors

Substrate-level test vectors (canonical numbering per parent RFC-0015-a §Test Vectors scheme: `TV-WLT-AGT-NN`). Parent RFC-0015-a TVs (TV-WLT-AGT-3..11c) are unaffected. RFC-0015-b owns TV-WLT-AGT-24..29 (post-parent block 23).

| TV            | Section | Description                                                                                                                              |
| ------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| TV-WLT-AGT-24 | §X.2    | Unknown Uuid → `Err(WalletError::AgentNotFound(uuid))` (existence-leak closure)                                                          |
| TV-WLT-AGT-25 | §X.2    | Not-owned Uuid (caller_did != holder_did) → `Err(WalletError::AgentNotFound(uuid))` (existence-leak closure)                             |
| TV-WLT-AGT-26 | §X.2    | Caller-is-holder happy path → `Ok(AgentManifest)`                                                                                        |
| TV-WLT-AGT-27 | §X.3    | `Terminated → Running` → `Err(WalletError::InvalidStateTransition { from: Terminated, to: Running })`                                    |
| TV-WLT-AGT-28 | §X.3    | Concurrent in-flight rejection (sequential std Mutex ordering): caller B receives `Err(WalletError::AlreadyInTransition(uuid))` per §X.1 |
| TV-WLT-AGT-29 | §X.1    | In-flight flag activation: caller A holds lock + flag=true; caller B observes + returns `Err(AlreadyInTransition(uuid))`                 |

Plus §X.4 (conditional on RFC-0012-v2): TV-WLT-AGT-30 (state_version matches chain-tip after successful transition) + TV-WLT-AGT-31 (phantom-event detection on rollback).

**Total:** 6 unconditional test vectors + 2 conditional test vectors (land with §X.4 if RFC-0012-v2 acceptance).

## Alternatives Considered

| Approach                                                                              | Pros                                               | Cons                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Option A: Continue RFC DRY loop on RFC-0015 + RFC-0015-a + RFC-0016 (NOT RECOMMENDED) | Defects addressed inline                           | Plateau evidence shows non-converging trend; ~5+ rounds for ~5 more substantive fixes                                                                                                                                                     |
| Option B: Accept plateau + paired-acceptance amendment backlog (CHOSEN)               | R48 precedent; closure-ready with deferred defects | Requires paired impl mission; 2 RFC cycles instead of 1                                                                                                                                                                                   |
| Option C: Re-scope amendment with defects as requirements upfront (rejected)          | Closes substrate-faithfulness gap upfront          | Requires 5-10 additional RFC cycles; unnecessary work per plateau declaration                                                                                                                                                             |
| Option D: Substitute `parking_lot::Mutex::try_lock` for in-flight flag (rejected)     | (none — rejected on canonical substrate grounds)   | Contradicts parent RFC-0015-a §Alternatives Considered explicit REJECT of parking_lot; introduces parallel abstraction; new Cargo.toml dep                                                                                                |
| Option E: Lock-then-validate ordering (defect 4 contract change; rejected)            | (none — rejected on canonical substrate grounds)   | Contradicts parent RFC-0015-a §6.1 (1) + §6.1 (6) canonical validate-first → lock-after → re-read inside lock; `validate_reason` is pure function (does not read registry); the actual TOCTOU window is already closed by parent §6.1 (6) |

## Implementation Phases

### Phase 1: RFC-0015-b Acceptance (this RFC)

- [ ] DRY CLOSED (R3 = zero-finding round 2 per RFC process)
- [ ] Promoted Draft → Accepted via `git mv rfcs/draft/process/0015-b-substrate-defects.md rfcs/accepted/process/0015-b-substrate-defects.md`
- [ ] Status header bump `Draft` → `Accepted`
- [ ] Version History entry added

### Phase 2: Substrate Implementation (paired mission)

- [ ] `missions/claimed/0015-b-substrate-defect-impl.md` lands per paired acceptance bridge (parent RFC-0015-a §6.5)
- [ ] Sub-step 1: §X.1 in-flight flag activation (AgentRecord.transitioning + transition_agent in-flight check + RAII guard + doc-comment hygiene)
- [ ] Sub-step 2: §X.2 lookup_agent existence-leak closure
- [ ] Sub-step 3: §X.3 state-machine test coverage (TV-WLT-AGT-27 + TV-WLT-AGT-28)
- [ ] Sub-step 4: §X.1 test (TV-WLT-AGT-29)
- [ ] Sub-step 5: §X.2 tests (TV-WLT-AGT-24 + TV-WLT-AGT-25 + TV-WLT-AGT-26)
- [ ] Sub-step 6 (CONDITIONAL on RFC-0012-v2 acceptance): §X.4 state_version field + post-rollback verification + new WalletError variant (TV-WLT-AGT-30 + TV-WLT-AGT-31)
- [ ] Sub-step 7: §X.5 RFC-0015-a §Amendment Surface parity refresh (DOC-ONLY in paired RFC-0015-a VH entry; conditional rows gated on RFC-0012-v2)
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --features full --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-wallet --lib agent` green
- [ ] DRY CLOSED on impl
- [ ] RFC-0015-b Acceptance COMPLETE

### Phase 3: RFC-0016-a Promotion (downstream)

- [ ] Substrate-faithfulness verified post-RFC-0015-b landing
- [ ] `missions/claimed/0016-a-audit-write-path-promotion.md` DRY CLOSED
- [ ] RFC-0016-a promoted Draft → Accepted

### Phase 4: RFC-0012-v2 Paired Amendment (parallel track)

- [ ] `state_version: u64` field appended to `AuditEvent` struct in `octo-audit-core`
- [ ] `audit_chain.tip.state_version: u64` accessor on `AppendOnlyAuditSink` trait
- [ ] `state_version` bumping integrated with `AuditEventKind::AgentTransition` variant append path
- [ ] RFC-0012-v2 acceptance unblocks §X.4 substrate impl in RFC-0015-b Phase 2 sub-step 6

## Key Files to Modify

| File                                                      | Change                                                                                                                                                                                              |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/octo-wallet/src/agent.rs`                         | `transitioning: bool` field on `AgentRecord` (§X.1) + `lookup_agent` normalization (§X.2) + state-machine edge tests (§X.3) + in-flight test (§X.1) + `state_version: u64` conditional field (§X.4) |
| `crates/octo-wallet/src/error.rs`                         | Doc-comment alignment for `AlreadyInTransition` (§X.1 sub-step hygiene) + conditional `AuditChainInconsistent { observed, expected }` variant (§X.4)                                                |
| `crates/octo-wallet/src/lib.rs`                           | Re-export unchanged (no new public surface; existing exports cover all additions)                                                                                                                   |
| `crates/octo-wallet/Cargo.toml`                           | NO CHANGE (canonical `std::sync::Mutex` per parent §6.1 (1); no new dep)                                                                                                                            |
| `rfcs/accepted/process/0015-a-wallet-agent-write-path.md` | §Amendment Surface table refresh (§X.5; 1 additive row + 1 conditional row + 1 forward-pointer row; DOC-ONLY in VH entry)                                                                           |
| `rfcs/accepted/process/0015-b-substrate-defects.md`       | NEW (this RFC, post-promotion)                                                                                                                                                                      |

## Future Work

- **F1:** RFC-0012-v2 paired amendment — `AuditEvent.state_version: u64` field + `audit_chain.tip.state_version: u64` accessor on `AppendOnlyAuditSink` trait in `octo-audit-core` (Layer A frozen; semver-major per CLAUDE.md §Layer A stability). Required for §X.4 substrate impl landing. Out of RFC-0015-b scope (separate RFC cycle per Layer A stability contract).
- **F2:** RFC-0002 companion amendment — split `AgentState` 3-state (Registered/Running/Terminated) to 5-state (REGISTERED/ACTIVE/BUSY/ACTIVE/TERMINATED) per RFC-0015 §Summary substrate-faithful drift note. Out of RFC-0015-b critical path; deferred to RFC-0002 companion cycle.
- **F3:** RFC-0015-c — full write-path formal authorization amendment. RFC-0015-b is a focused 5-defect closure; RFC-0015-c would consolidate the write-path surface into a single authoritative amendment. Out of RFC-0015-b scope.

## Rationale

The paired-acceptance pattern (RFC-0015-b amendment + `0015-b-substrate-defect-impl` mission) follows the R48 precedent (RFC-0012-v2 + RFC-0014-v2). The alternative (Option A, continue DRY loop) was projected to produce 40-60 findings per round with diminishing returns. Option C (re-scope with defects as requirements upfront) was rejected as unnecessary work given the substrate is the source of truth.

The 7 plateau defects were re-examined against canonical substrate patterns established at parent RFC-0015-a acceptance. **5 defects survive canonicalization** as substantive §X.x requirements (defects 1, 3, 5, 6, 7); defect 2 is folded into the §X.1 impl sub-step as doc-comment hygiene; defect 4 is rejected as a substrate contract change based on a flawed premise (that `validate_reason` reads the registry — it does not per parent §6.1 (6)). The canonical validate-first → lock-after → re-read inside lock pattern per parent §6.1 (1) + §6.1 (6) already closes the actual TOCTOU window.

The 5 surviving defects have explicit acceptance criteria per §X.1-§X.5. The paired-acceptance pattern ensures RFC-0015-b text lands FIRST (formal authorization), then substrate code lands per parent RFC-0015-a §6.5 Layer A Paired-Acceptance Bridge.

## Version History

| Version | Date       | Changes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.0     | 2026-09-14 | Initial substrate-defect paired amendment per plateau declaration                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 1.1     | 2026-09-14 | R1.5 fix: rewrote against canonical `std::sync::Mutex` substrate (Option A); collapsed 7 → 5 defects; rejected defect 4 TOCTOU contract change; folded defect 2 into §X.1 impl sub-step; renumbered §X sections; fixed phantom §6.4 → §6.5 paired-acceptance bridge cite; removed `parking_lot::Mutex::try_lock` Option D (rejected on canonical substrate grounds); fixed all file:line refs → §symbol refs per [[no-line-refs-anywhere]]; collapsed §X.8/§X.9/§X.10 cross-cutting sections (forward-pointers to parent RFC-0015-a + RFC-0012-v2 paired amendment) |

## Related RFCs

- RFC-0015 — `octo-wallet` Agent Operations Substrate (parent KEEP RFC; read path)
- RFC-0015-a — `octo-wallet` Agent Write-Path Amendment (parent write-path RFC; provides `transition_agent` + canonical lock pattern per §6.1)
- RFC-0016 — `octo-audit` Receipt Read-Path API (parent audit read-path RFC)
- RFC-0016-a — `octo-audit` Receipt Write-Path API (sibling write-path RFC; promoted after RFC-0015-b lands)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` authority)
- RFC-0012 — `octo-audit-core` Layer A substrate (paired amendment for §X.4: RFC-0012-v2)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping)

## Related Use Cases

- `docs/use-cases/agent-lifecycle.md` (canonical lifecycle; future)

## Appendices

### A. Substrate-faithfulness verification notes

§X.1 in-flight flag activation preserves all canonical substrate invariants from parent RFC-0015-a:

- **Lock primitive:** canonical GLOBAL `std::sync::Mutex::lock()` per parent §6.1 (1). NO substitution to `parking_lot::Mutex::try_lock` (explicit REJECT in parent §Alternatives Considered).
- **Lock acquisition order:** validate-first → lock-after → re-read `current_state` inside lock per parent §6.1 (1) + §6.1 (6). In-flight flag check happens INSIDE the lock AFTER the re-read (so idempotent same-state early-return per parent §6.1 (4) runs before the flag is touched).
- **Audit append + rollback:** unchanged per parent §6.1 (5). In-flight flag does not affect the append-or-rollback contract; flag is reset on lock release (RAII guard).
- **Idempotency on same-state:** unchanged per parent §6.1 (4). Idempotent no-op early-return runs BEFORE the in-flight flag check.

§X.2 lookup_agent normalization preserves caller-attestation for `transition_agent`:

- `ForbiddenHolderMismatch` variant retained for `transition_agent` per parent §6.1 (2) (where the distinction between "unknown" vs "not-owned" is NOT leaked — the substrate reports `ForbiddenHolderMismatch` regardless of which case applies, for caller-attestation integrity).
- `lookup_agent` is the ONLY substrate path that normalizes; `transition_agent` retains the distinction for caller-attestation error reporting (substrate-faithful principle: each substrate path surfaces the error envelope that contributes to its caller-attestation discipline).

### B. Defect origin traceability

Each §X.x maps back to the plateau declaration audit doc table:

| §X.x | Defect # | Status                                                                                               |
| ---- | -------- | ---------------------------------------------------------------------------------------------------- |
| §X.1 | 1        | Real (in-flight flag activation; preserves canonical lock primitive)                                 |
| §X.2 | 3        | Real (lookup_agent existence-leak closure)                                                           |
| §X.3 | 6        | Real (2 new state-machine edge TVs; rest duplicate parent RFC-0015-a)                                |
| §X.4 | 5        | Real but split: Layer B field (this RFC) + Layer A accessor (RFC-0012-v2)                            |
| §X.5 | 7        | Real (1 additive row + 1 conditional row + 1 forward-pointer; targets RFC-0015-a §Amendment Surface) |

Defect 2 (doc-comment): folded into §X.1 impl sub-step as doc-comment hygiene. Defect 4 (TOCTOU contract): rejected as substrate contract change based on flawed premise (`validate_reason` is pure per parent §6.1 (6); canonical validate-first → lock-after pattern already closes the actual TOCTOU window).

---

**Version:** 1.1
**Submission Date:** 2026-09-14
**Last Updated:** 2026-09-14
