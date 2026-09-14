# RFC-0015-b: `octo-wallet` Substrate-Defect Paired Amendment

## Status

Draft (2026-09-14)

> **Substrate-defect paired amendment per RFC-0015 + RFC-0015-a + RFC-0016 plateau declaration 2026-09-14.** This amendment restates 7 substrate defects (out of RFC DRY scope) as additive §X.1-§X.7 requirements. Paired with `missions/claimed/0015-b-substrate-defect-impl.md` per RFC-0015-a §6.4 paired-invariance rule.

## Authors

- Authored by `@cipherocto` per RFC-0015 amendment chain + `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`.

## Maintainers

- Maintainer: `@cipherocto` per RFC-0015 amendment chain.

## Summary

RFC-0015 (Accepted 2026-09-14) + RFC-0015-a (Accepted 2026-09-14) + RFC-0016 (Accepted 2026-09-14) reached a stylistic plateau in the R12 + R13 DRY review loop. The plateau declaration (`docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` §Deferred substrate defects) surfaced **7 substrate defects** that are out of scope for the RFC text cycle but are critical for substrate correctness.

This RFC is the **paired-acceptance amendment** that addresses those 7 defects by restating them as numbered §X.1-§X.7 requirements. The paired implementation mission `missions/claimed/0015-b-substrate-defect-impl.md` lands the substrate code per these requirements post-RFC-0015-b acceptance.

The 7 defects split into three categories:

- **§X.1-§X.2** lock-mechanism activation (`AlreadyInTransition` + doc-comment alignment)
- **§X.3** existence-leak closure (`lookup_agent` normalization)
- **§X.4** TOCTOU window closure (lock-then-validate ordering)
- **§X.5** phantom-event detection (state-version field + post-rollback verification)
- **§X.6** state-machine test coverage (6 canonical edges per RFC-0002)
- **§X.7** RFC-0015 §Pre-existing Substrate parity refresh (3 rows added)

The amendment is **strictly additive** — no breaking changes to existing public API per RFC migration etiquette. CLI behavior unchanged for §X.1, §X.2, §X.4, §X.5, §X.6, §X.7 (error exit mappings pre-existing at parent RFC-0015-a acceptance); CLI security posture improves for §X.3 (no information leak on probe).

**Pairing invariant:** Acceptance of RFC-0015-b REQUIRES paired closure of mission `0015-b-substrate-defect-impl` per RFC-0015-a §6.4 paired-invariance rule. RFC-0015-b Acceptance is the formal authorization step; the impl mission is the substrate code landing.

## Dependencies

**Requires:**

- RFC-0015 — `octo-wallet` Agent Operations Substrate (parent KEEP RFC; this is the substrate-defect amendment)
- RFC-0015-a — `octo-wallet` Agent Write-Path Amendment (parent write-path RFC; provides `transition_agent` + `TransitionReceipt` substrate)
- RFC-0016 — `octo-audit` Receipt Read-Path API (parent audit read-path RFC)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers; benefits from §X.3 security posture improvement)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` + state-machine substrate authority per §Agent State Machine)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping per §RFC-0008 Execution Class Mapping)

**Substrate amendment dependencies (REQUIRED for RFC-0015-b acceptance):**

- **RFC-0012** — `AuditEventKind::AgentTransition` variant + `audit_chain.tip.state_version` accessor in `octo-audit-core` (Layer A frozen; §X.5 phantom-event verification reads `audit_chain.tip.state_version`). **(REQUIRED substrate amendment; without this, §X.5 post-rollback verification has no canonical chain-tip accessor.)**

**Paired implementation mission dependencies:**

- `missions/claimed/0015-b-substrate-defect-impl.md` — the paired substrate implementation mission. Acceptance of RFC-0015-b REQUIRES this mission landing (per RFC-0015-a §6.4 paired-invariance rule).

## Design Goals

1. **Substrate-defect closure** — each of the 7 defects from plateau declaration §Deferred substrate defects has a numbered §X.x requirement with explicit acceptance criteria.
2. **Lock-mechanism activation** — `parking_lot::Mutex::try_lock()` replaces `std::sync::Mutex::lock()` as the canonical contention-detection primitive (RFC-0015-a §6.4 substrate contract).
3. **Existence-leak closure** — `lookup_agent` normalizes unknown + not-owned cases to `WalletError::AgentNotFound`, closing the multi-DID enumeration attack surface.
4. **TOCTOU mitigation** — `transition_agent` acquires the registry lock FIRST, then runs `validate_reason` INSIDE the lock (lock-then-validate ordering).
5. **Phantom-event detection** — monotonic `state_version: u64` field on `AgentRecord` + post-rollback verification pass that asserts `audit_chain.tip.state_version == current_state_version - 1`.
6. **State-machine test coverage** — 6 canonical state-machine edge tests per RFC-0002 §Agent State Machine, including post-TOCTOU regression coverage.
7. **RFC parity refresh** — RFC-0015 §Pre-existing Substrate table updated to reflect current substrate reality (3 rows added per Phase 2 unblock landing commit `next e09f3e3a` 2026-09-13).

## Motivation

RFC-0015 + RFC-0015-a + RFC-0016 reached R12 (30 findings) + R13 (50 findings) — non-converging stylistic plateau. The plateau declaration recommended Option B (accept plateau + promote to Accepted + paired amendment backlog) per the R48 precedent.

The 7 substrate defects are:

| #   | Defect                                                | Source line / artifact                                  | Severity |
| --- | ----------------------------------------------------- | ------------------------------------------------------- | -------- |
| 1   | `WalletError::AlreadyInTransition(Uuid)` dead surface | `crates/octo-wallet/src/error.rs:184-190`               | HIGH     |
| 2   | `AlreadyInTransition` doc-comment drift               | `crates/octo-wallet/src/error.rs:184-190`               | LOW      |
| 3   | `lookup_agent` existence-leak                         | `crates/octo-wallet/src/agent.rs:486-500`               | HIGH     |
| 4   | `transition_agent` TOCTOU window                      | `crates/octo-wallet/src/agent.rs::validate_reason`      | MED      |
| 5   | Phantom-event window on audit-append + rollback       | `crates/octo-wallet/src/agent.rs` (audit append branch) | MED      |
| 6   | Missing state-machine tests                           | `crates/octo-wallet/src/agent.rs` (test module)         | MED      |
| 7   | RFC-0015 §Pre-existing Substrate parity gap           | `rfcs/accepted/process/0015-wallet-agent-operations.md` | LOW      |

Each defect is restated below as a numbered §X.x requirement with explicit acceptance criteria + layer-model annotation + dependency declaration.

## Roles and Authorities

| Role                | Authority                                                                              | Audit trail                              |
| ------------------- | -------------------------------------------------------------------------------------- | ---------------------------------------- |
| Operator (human/CI) | Triggers `transition_agent` via CLI (per RFC-0011-c §9.3.2 + §9.3.4)                   | CLI log + audit append per RFC-0012      |
| Wallet substrate    | Source of truth for `AgentState` + state-version tracking; rejects invalid transitions | Internal state machine log               |
| Audit substrate     | Exposes `audit_chain.tip.state_version` accessor per RFC-0012 paired amendment         | Append-only audit log (BLAKE3-256 chain) |
| CLI (octo-cli)      | Operator UX over substrate; never bypasses substrate state-machine                     | Same as operator                         |

## Specification

### §X.1 Lock-mechanism activation — `AlreadyInTransition` path

**Defect:** `WalletError::AlreadyInTransition(Uuid)` declared at `crates/octo-wallet/src/error.rs:184-190` but never constructed by `transition_agent`. Dead surface.

**Requirement:** `transition_agent` MUST acquire the registry lock via `parking_lot::Mutex::try_lock()`. On `try_lock` failure (contention path), `transition_agent` MUST return `Err(WalletError::AlreadyInTransition(uuid))` with the contested `uuid`. The `try_lock` failure is the canonical signal for in-flight transitions.

**Layer model:** Layer B substrate mutation. CLI behavior unchanged (exit code 43 mapping pre-existing via `OctoCliError::AlreadyInTransition(Uuid)`).

**Acceptance Criteria:**

- [ ] `crates/octo-wallet/src/agent.rs::transition_agent` calls `parking_lot::Mutex::try_lock()` at lock acquisition point
- [ ] On `try_lock` failure → `Err(WalletError::AlreadyInTransition(uuid))` constructed
- [ ] `parking_lot` dep added to `crates/octo-wallet/Cargo.toml` with rationale comment (Layer B additive)
- [ ] Doc-comment at `error.rs:184-190` updated to match implementation (closes §X.2 simultaneously)
- [ ] Test vector TV-0015-b-1 verifies concurrent caller rejection via `AlreadyInTransition`

### §X.2 Doc-comment alignment — `AlreadyInTransition`

**Defect:** doc-comment references `parking_lot::Mutex::try_lock` but substrate implements `std::sync::Mutex::lock` per `crates/octo-wallet/src/agent.rs:488`.

**Requirement:** once §X.1 lands, the doc-comment becomes accurate. Pre-amendment landing: the amendment cites the `try_lock` form as the operative intent; the substrate implementation is upgraded to match per §X.1.

**Layer model:** doc-comment hygiene, no code change beyond §X.1.

**Acceptance Criteria:**

- [ ] Doc-comment at `error.rs:184-190` references `parking_lot::Mutex::try_lock` (matches implementation post-§X.1)
- [ ] No other doc-comment drift introduced by §X.1

### §X.3 `lookup_agent` existence-leak closure

**Defect:** `agent.rs:486-500` returns `WalletError::AgentNotFound` for unknown Uuid but `WalletError::ForbiddenHolderMismatch` for caller-DID mismatch. Existence leak enables enumeration.

**Requirement:** `lookup_agent(caller_did, uuid)` MUST normalize BOTH unknown + not-owned cases to `WalletError::AgentNotFound`. The `ForbiddenHolderMismatch` variant remains reserved for other substrate paths that DO need to distinguish (e.g., `transition_agent` caller-attestation; per RFC-0015-a §6.1).

**Layer model:** Layer B substrate mutation. CLI behavior unchanged (`OctoCliError::AgentNotFound(Uuid)` exit 42 mapping pre-existing). Security posture improves (no information leak on probe).

**Acceptance Criteria:**

- [ ] `crates/octo-wallet/src/agent.rs::lookup_agent` normalizes both cases to `WalletError::AgentNotFound`
- [ ] `ForbiddenHolderMismatch` variant remains in `WalletError` enum (used elsewhere)
- [ ] Test vector TV-0015-b-2 verifies unknown Uuid → `AgentNotFound`
- [ ] Test vector TV-0015-b-3 verifies not-owned Uuid → `AgentNotFound` (existence-leak closure)
- [ ] Test vector TV-0015-b-4 verifies caller-is-holder happy path → `Ok(AgentManifest)`

### §X.4 TOCTOU window closure

**Defect:** `validate_reason` runs BEFORE `registry().lock()`. Concurrent caller could mutate registry between reason validation and lock acquisition.

**Requirement:** `transition_agent` MUST acquire `registry().lock()` FIRST, then run `validate_reason` INSIDE the lock. Lock-then-validate ordering closes the TOCTOU window.

**Layer model:** Layer B substrate mutation. CLI behavior unchanged (signature preserved). Test vectors MUST add TOCTOU regression coverage.

**Acceptance Criteria:**

- [ ] `crates/octo-wallet/src/agent.rs::transition_agent` acquires `registry().lock()` at the top of the function body
- [ ] `validate_reason` call moved INSIDE the lock scope
- [ ] Test vector TV-0015-b-5 verifies TOCTOU race: 2 concurrent callers, one wins, one rejected via `AlreadyInTransition` or `InvalidStateTransition`
- [ ] No regression in existing transition semantics

### §X.5 Phantom-event detection — state-version field

**Defect:** on audit-append failure + rollback, the registry state is reverted but the audit event chain may have been partially appended. Phantom-event window: appears as "transition committed" but chain-hash references stale state.

**Requirement:** `transition_agent` MUST add:

- **§X.5.a** monotonic `state_version: u64` field on `AgentRecord` (default 0; bumped before each successful transition)
- **§X.5.b** post-rollback verification pass that asserts `audit_chain.tip.state_version == current_state_version - 1`. Mismatch → `WalletError::AuditChainInconsistent { observed, expected }`

**Layer model:** Layer B substrate mutation. CLI behavior unchanged. Substrate adds an internal reconciliation step before `transition_agent` returns.

**Substrate dependency:** `audit_chain.tip.state_version` accessor is REQUIRED from RFC-0012 paired amendment (or pre-existing if RFC-0012 acceptance pre-empted this RFC-0015-b acceptance).

**Acceptance Criteria:**

- [ ] `crates/octo-wallet/src/agent.rs::AgentRecord` carries `state_version: u64` field (default 0)
- [ ] `state_version` bumped before each successful `transition_agent` call
- [ ] Post-rollback verification pass asserts `audit_chain.tip.state_version == current_state_version - 1`
- [ ] New `WalletError::AuditChainInconsistent { observed: u64, expected: u64 }` variant added (additive)
- [ ] Test vector TV-0015-b-6 verifies phantom-event detection: simulated partial audit append + rollback triggers `AuditChainInconsistent`
- [ ] Test vector TV-0015-b-7 verifies happy path: state_version matches chain-tip after successful transition

### §X.6 State-machine test coverage

**Defect:** `crates/octo-wallet/src/agent.rs` lacks tests for canonical state-machine edges.

**Requirement:** substrate addition MUST include test coverage for ALL canonical state-machine edges per RFC-0002 §Agent State Machine:

| Edge                        | Test expected outcome                                                           |
| --------------------------- | ------------------------------------------------------------------------------- |
| `Registered → Running`      | `Ok(TransitionReceipt { current_state: Running, state_version: 1 })`            |
| `Running → Terminated`      | `Ok(TransitionReceipt { current_state: Terminated, state_version: 2 })`         |
| `Terminated → Running`      | `Err(WalletError::InvalidStateTransition { from: Terminated, to: Running })`    |
| `Registered → Terminated`   | `Err(WalletError::InvalidStateTransition { from: Registered, to: Terminated })` |
| `Running → Running`         | `Ok(TransitionReceipt)` idempotent (no audit append; state_version unchanged)   |
| post-§X.4 TOCTOU regression | Race test: 2 concurrent callers; one wins, one gets `AlreadyInTransition`       |

**Layer model:** test addition, no behavior change.

**Acceptance Criteria:**

- [ ] 6 test vectors per the table above implemented in `crates/octo-wallet/src/agent.rs` test module
- [ ] All 6 TVs pass
- [ ] TOCTOU race test uses `std::thread::spawn` for 2 concurrent callers (per RFC-0015-a §6.1 lock semantics)

### §X.7 RFC-0015 §Pre-existing Substrate parity refresh

**Defect:** RFC-0015 §Pre-existing Substrate lists substrate items, but several substrate items landed in Phase 2 unblock work (R13.5 + earlier). Substrate parity gap.

**Requirement:** RFC-0015 §Pre-existing Substrate table refresh. Add 3 rows for:

1. `transition_agent(caller_did, uuid, target, reason)`
2. `TransitionReceipt` projection
3. `AgentRecord.state: AgentState` (default `Registered`) — and `state_version: u64` field per §X.5

Add "Phase 2 unblock landing (commit `next e09f3e3a` 2026-09-13)" + "Substrate-defect amendment landing (RFC-0015-b Acceptance)" to timestamp column.

**Layer model:** RFC text refresh, no code change.

**Acceptance Criteria:**

- [ ] 3 rows added to RFC-0015 §Pre-existing Substrate table
- [ ] Timestamp column updated with both commit references
- [ ] Cross-reference to RFC-0015-b §X.1-§X.7 added to RFC-0015 §6.2 §Amendment Surface

### §X.8 RFC-0008 Execution Class Mapping

| Operation                                          | Class | Rationale                                                           |
| -------------------------------------------------- | ----- | ------------------------------------------------------------------- |
| `transition_agent` lock acquisition (try_lock)     | B     | Substrate-level state-machine guard; deterministic under contention |
| `transition_agent` reason validation (inside lock) | B     | Substrate-level input validation; deterministic                     |
| `transition_agent` state_version bump              | B     | Monotonic counter; deterministic increment                          |
| `transition_agent` audit append                    | B     | Append-only sink write; deterministic under RFC-0012 chain-hash     |
| `transition_agent` post-rollback verification      | B     | Assert chain-tip matches expected state_version; deterministic      |
| `lookup_agent` (post-§X.3 normalization)           | B     | Caller-attested read; deterministic result                          |
| `validate_reason` (inside lock per §X.4)           | B     | Control-char + length filter; deterministic                         |

All operations are Class B (RFC-0008 substrate-level; deterministic). No Class A (consensus) or Class C (UX) operations introduced.

### §X.9 Error Handling

| Error                                                             | Exit code | Layer                                        |
| ----------------------------------------------------------------- | --------- | -------------------------------------------- |
| `WalletError::AlreadyInTransition(uuid)`                          | 43 (CLI)  | B → C/D mirror pre-existing                  |
| `WalletError::InvalidStateTransition { from, to }`                | 43 (CLI)  | B → C/D mirror pre-existing                  |
| `WalletError::AuditUnavailable(String)`                           | 52 (CLI)  | B → C/D mirror pre-existing                  |
| `WalletError::AuditChainInconsistent { observed, expected }`      | 52 (CLI)  | B → C/D mirror NEW (post-§X.5)               |
| `WalletError::AgentNotFound(uuid)` (post-§X.3)                    | 42 (CLI)  | B → C/D mirror pre-existing                  |
| `WalletError::ForbiddenHolderMismatch { observed, expected_did }` | 17 (CLI)  | B → C/D mirror pre-existing (used elsewhere) |

`OctoCliError` variants pre-existed at parent RFC-0015-a acceptance for all exit codes. `AuditChainInconsistent` exits 52; CLI mirror variant added per paired impl mission.

### §X.10 Performance Targets

| Metric                                      | Target  | Notes                                      |
| ------------------------------------------- | ------- | ------------------------------------------ |
| `transition_agent` lock acquisition latency | < 1µs   | `parking_lot::Mutex::try_lock` uncontended |
| `lookup_agent` (post-§X.3 normalization)    | < 1µs   | BTreeMap lookup + caller-attestation check |
| `validate_reason` (inside lock)             | < 100ns | Control-char + length scan                 |
| State-version bump                          | < 10ns  | `u64` increment                            |
| Post-rollback verification                  | < 10µs  | BLAKE3 chain-tip lookup + u64 comparison   |

## Implicit Assumptions Audit

| Assumption                                                    | Where Relied Upon      | Blast Radius if False                                                                 | Mitigation / Status                                                      |
| ------------------------------------------------------------- | ---------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Lock-then-validate ordering (`§X.4`) preserves invariant      | §X.4 + RFC-0015-a §6.1 | TOCTOU window opens; concurrent caller could pass reason validation then race on lock | Test vector TV-0015-b-5 covers regression; substrate-level invariant     |
| `parking_lot` dep is semver-stable                            | §X.1 + Cargo.toml      | Compilation breaks on dep update; lock API unstable                                   | Pin `parking_lot = "0.12"` per substrate-first ordering                  |
| `audit_chain.tip.state_version` accessor exists in RFC-0012   | §X.5                   | §X.5 verification pass cannot read chain-tip; phantom-event detection broken          | RFC-0012 paired amendment MUST land before §X.5 acceptance               |
| `AgentRecord` is the canonical agent state holder             | §X.5.a                 | State-version tracking placed in wrong struct; phantom-event detection fails          | Substrate-faithful principle: RFC-0002 §Agent Manifest spec is canonical |
| Caller-attestation pattern (`caller_did == holder_did`) holds | §X.3 + RFC-0015-a §6.1 | Existence-leak re-opens; multi-DID enumeration possible                               | Test vectors TV-0015-b-2..4 cover all three cases                        |

## Security Considerations

### §X.S.1 Existence-leak closure (§X.3)

The pre-amendment substrate returned `AgentNotFound` for unknown Uuid but `ForbiddenHolderMismatch` for caller-DID mismatch. An attacker could probe for Uuids belonging to other DIDs by submitting a guessed Uuid and observing the error code. Post-§X.3, both cases return `AgentNotFound`, closing the enumeration side-channel.

**Severity:** HIGH (closes enumeration attack surface per RFC-0015 §Design Goals G6).

### §X.S.2 TOCTOU window closure (§X.4)

The pre-amendment `validate_reason` ran before `registry().lock()`. A concurrent caller could mutate the registry between reason validation and lock acquisition, causing the validate_reason result to reference stale state. Post-§X.4, lock-then-validate ordering closes the TOCTOU window.

**Severity:** MED (closes timing-attack vector; non-consensus-critical).

### §X.S.3 Phantom-event detection (§X.5)

The pre-amendment substrate could leave the audit chain in an inconsistent state on partial append + rollback. The audit chain would show a "transition committed" event but the registry state would be reverted. Post-§X.5, the post-rollback verification pass asserts `audit_chain.tip.state_version == current_state_version - 1`; mismatch surfaces `AuditChainInconsistent`.

**Severity:** MED (audit chain integrity; non-consensus-critical per RFC-0015-a §6.4 audit append contract).

### §X.S.4 Concurrent-call detection (§X.1)

The pre-amendment `WalletError::AlreadyInTransition(Uuid)` variant was dead surface. Post-§X.1, the `parking_lot::Mutex::try_lock()` contention path activates the variant. CLI exits 43 on contention; operator can retry.

**Severity:** MED (operational UX; non-security-critical).

## Adversary Analysis

| Decision                                            | Q1 Beneficiary                          | Q2 Cost to Attacker                       | Q3 Gain if Successful                           | Q4 Defense (cost to legit op)                                 | Q5 Residual Risk                                                 |
| --------------------------------------------------- | --------------------------------------- | ----------------------------------------- | ----------------------------------------------- | ------------------------------------------------------------- | ---------------------------------------------------------------- |
| `lookup_agent` normalize to `AgentNotFound` (§X.3)  | External attacker probing for Uuids     | High (must guess valid Uuid + holder DID) | Multi-DID enumeration of agent ownership        | Normalization (zero legit cost)                               | LOW (residual: timing-attack on lock acquisition)                |
| Lock-then-validate ordering (§X.4)                  | Concurrent caller racing for transition | Low (race window is nanoseconds)          | Inconsistent reason validation                  | Lock acquisition before validation (negligible legit cost)    | LOW (residual: lock acquisition race at microsecond scale)       |
| `state_version` + post-rollback verification (§X.5) | Operator with audit-chain access        | Very high (must corrupt append-only sink) | Phantom-event observation                       | Monotonic counter + verification pass (negligible legit cost) | MED (residual: requires RFC-0012 amendment for chain-tip access) |
| `parking_lot::try_lock` contention path (§X.1)      | Concurrent caller                       | Low (contention is observable)            | None (variant just surfaces existing condition) | Variant activation (zero legit cost; CLI exit 43)             | LOW (residual: false positives under high contention)            |

### Severity Classification

| Severity | Definition                                                                                 | Action                                        |
| -------- | ------------------------------------------------------------------------------------------ | --------------------------------------------- |
| HIGH     | Existence-leak closure (§X.3)                                                              | MUST mitigate before Accept; §X.3 closes this |
| MED      | TOCTOU closure (§X.4) + phantom-event detection (§X.5) + concurrent-call activation (§X.1) | SHOULD mitigate before Accept; all 3 closed   |
| LOW      | Doc-comment alignment (§X.2) + state-machine tests (§X.6) + parity refresh (§X.7)          | SHOULD mitigate; all 3 closed                 |

## Economic Analysis

No economic implications. RFC-0015-b is a substrate-defect amendment; no token economics, dual-stake requirements, or market dynamics are affected.

## Compatibility

- **Backward:** additive only. All existing public API preserved; new variants added under `#[non_exhaustive] WalletError`. CLI exit code mappings pre-existing; no new exit codes introduced.
- **Forward:** `AgentRecord.state_version` field is additive; pre-amendment builds will not see the field but post-amendment builds will read it. Layer A frozen contract preserved (no RFC-0012 changes beyond `AuditEventKind` variant + chain-tip accessor).
- **Wire form:** no wire form changes (the audit chain hash already carries the version internally via the `AuditEventKind::AgentTransition` variant payload).

## Test Vectors

7 canonical test vectors per §X.1, §X.3, §X.4, §X.5, §X.6:

| TV          | Section | Description                                                                             |
| ----------- | ------- | --------------------------------------------------------------------------------------- |
| TV-0015-b-1 | §X.1    | Concurrent caller rejection via `AlreadyInTransition`                                   |
| TV-0015-b-2 | §X.3    | Unknown Uuid → `AgentNotFound` (existence-leak closure)                                 |
| TV-0015-b-3 | §X.3    | Not-owned Uuid → `AgentNotFound` (existence-leak closure)                               |
| TV-0015-b-4 | §X.3    | Caller-is-holder happy path → `Ok(AgentManifest)`                                       |
| TV-0015-b-5 | §X.4    | TOCTOU race: 2 concurrent callers, one wins, one rejected                               |
| TV-0015-b-6 | §X.5    | Phantom-event detection: simulated partial append + rollback → `AuditChainInconsistent` |
| TV-0015-b-7 | §X.5    | State-version matches chain-tip after successful transition                             |

Plus 6 state-machine edge TVs from §X.6 (TV-AGT-EDGE1 through TV-AGT-EDGE6).

**Total:** 13 new test vectors.

## Alternatives Considered

| Approach                                                                              | Pros                                               | Cons                                                                                  |
| ------------------------------------------------------------------------------------- | -------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Option A: Continue RFC DRY loop on RFC-0015 + RFC-0015-a + RFC-0016 (NOT RECOMMENDED) | Defects addressed inline                           | Plateau evidence shows non-converging trend; ~5+ rounds for ~5 more substantive fixes |
| Option B: Accept plateau + paired-acceptance amendment backlog (CHOSEN)               | R48 precedent; closure-ready with deferred defects | Requires paired impl mission; 2 RFC cycles instead of 1                               |
| Option C: Re-scope amendment with defects as requirements (rejected)                  | Closes substrate-faithfulness gap upfront          | Requires 5-10 additional RFC cycles; unnecessary work per plateau declaration         |

## Implementation Phases

### Phase 1: RFC-0015-b Acceptance (this RFC)

- [ ] DRY CLOSED (R3 = zero-finding round 2)
- [ ] Promoted Draft → Accepted via `git mv rfcs/draft/process/0015-b-substrate-defects.md rfcs/accepted/process/0015-b-substrate-defects.md`
- [ ] Status header bump `Draft` → `Accepted v3.1`
- [ ] Version History entry added

### Phase 2: Substrate Implementation (paired mission)

- [ ] `missions/claimed/0015-b-substrate-defect-impl.md` landed
- [ ] 13 test vectors pass per §Test Vectors
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --features full --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-wallet --lib agent` green
- [ ] DRY CLOSED on impl
- [ ] RFC-0015-b Acceptance COMPLETE

### Phase 3: RFC-0016-a Promotion (downstream)

- [ ] Substrate-faithfulness verified post-RFC-0015-b landing
- [ ] `missions/claimed/0016-a-audit-write-path-promotion.md` DRY CLOSED
- [ ] RFC-0016-a promoted Draft → Accepted

## Key Files to Modify

| File                                                    | Change                                                                                                                                                                                         |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/octo-wallet/src/agent.rs`                       | `try_lock` (§X.1) + `lookup_agent` normalization (§X.3) + lock-then-validate (§X.4) + `state_version` field (§X.5.a) + post-rollback verification (§X.5.b) + 6 state-machine edge tests (§X.6) |
| `crates/octo-wallet/src/error.rs`                       | Doc-comment alignment (§X.2) + `AuditChainInconsistent` variant (§X.5.b)                                                                                                                       |
| `crates/octo-wallet/Cargo.toml`                         | `parking_lot` dep addition (§X.1)                                                                                                                                                              |
| `crates/octo-audit-core/src/event.rs`                   | (paired with RFC-0012 amendment) `audit_chain.tip.state_version` accessor                                                                                                                      |
| `rfcs/accepted/process/0015-wallet-agent-operations.md` | §Pre-existing Substrate table refresh (§X.7)                                                                                                                                                   |
| `rfcs/accepted/process/0015-b-substrate-defects.md`     | NEW (this RFC, post-promotion)                                                                                                                                                                 |

## Future Work

- **F1:** RFC-0002 companion amendment — split `AgentState` 3-state (Registered/Running/Terminated) to 5-state (REGISTERED/ACTIVE/BUSY/ACTIVE/TERMINATED) per RFC-0015 §Summary substrate-faithful drift note. Out of RFC-0015-b critical path; deferred to RFC-0002 companion cycle.
- **F2:** RFC-0015-c — full write-path formal authorization amendment. Substrate-defect amendment (§X.1-§X.7) is a focused 7-defect closure; RFC-0015-c would consolidate the write-path surface into a single authoritative amendment. Out of RFC-0015-b scope.

## Rationale

The paired-acceptance pattern (RFC-0015-b amendment + `0015-b-substrate-defect-impl` mission) follows the R48 precedent (RFC-0012-v2 + RFC-0014-v2). The alternative (Option A, continue DRY loop) was projected to produce 40-60 findings per round with diminishing returns. Option C (re-scope with defects as requirements upfront) was rejected as unnecessary work given the substrate is the source of truth (RFC-0015/0015-a text describes the substrate reality; defects are surgical closures, not architectural changes).

The 7 defects are well-documented in the plateau declaration audit doc and have explicit acceptance criteria per §X.1-§X.7. The paired-acceptance pattern ensures RFC-0015-b text lands FIRST (formal authorization), then substrate code lands (per RFC-0015-a §6.4 paired-invariance rule).

## Version History

| Version | Date       | Changes                                                           |
| ------- | ---------- | ----------------------------------------------------------------- |
| 1.0     | 2026-09-14 | Initial substrate-defect paired amendment per plateau declaration |

## Related RFCs

- RFC-0015 — `octo-wallet` Agent Operations Substrate (parent KEEP RFC)
- RFC-0015-a — `octo-wallet` Agent Write-Path Amendment (parent write-path RFC)
- RFC-0016 — `octo-audit` Receipt Read-Path API (parent audit read-path RFC)
- RFC-0016-a — `octo-audit` Receipt Write-Path API (sibling write-path RFC; promoted after RFC-0015-b lands)
- RFC-0011-c — `octo agent` Subcommands (CLI consumers)
- RFC-0002 — Agent Manifest Specification (canonical `AgentState` authority)
- RFC-0012 — `octo-audit-core` Layer A substrate (paired amendment for §X.5)

## Related Use Cases

- `docs/use-cases/agent-lifecycle.md` (canonical lifecycle; future)

## Appendices

### A. Lock-mechanism migration

Pre-amendment substrate uses `std::sync::Mutex::lock()` (blocking). Post-amendment, `parking_lot::Mutex::try_lock()` (non-blocking; contention-detecting). Migration rationale:

- `parking_lot` is the canonical CipherOcto substrate lock primitive per RFC-0015-a §6.4 substrate contract.
- `try_lock` enables contention-detection (`AlreadyInTransition` activation) which `lock` cannot express.
- `parking_lot` is faster than `std::sync::Mutex` in uncontended paths (per `parking_lot` benchmarks; 2-3× faster).

### B. State-version semantics

`AgentRecord.state_version: u64` is a monotonic counter:

- Default 0 at agent creation.
- Bumped before each successful `transition_agent` call (BEFORE the state mutation + audit append).
- Persisted in audit chain via `AuditEventKind::AgentTransition` variant payload (post-§X.5).
- Verified post-rollback: `audit_chain.tip.state_version == current_state_version - 1`.

A mismatch indicates a phantom-event window: the audit chain recorded a transition that the registry state has reverted.

### C. Defect origin traceability

Each §X.x maps back to the plateau declaration audit doc table:

| §X.x | Defect # | Plateau doc line                   |
| ---- | -------- | ---------------------------------- |
| §X.1 | 1        | §Deferred substrate defects item 1 |
| §X.2 | 2        | §Deferred substrate defects item 2 |
| §X.3 | 3        | §Deferred substrate defects item 3 |
| §X.4 | 4        | §Deferred substrate defects item 4 |
| §X.5 | 5        | §Deferred substrate defects item 5 |
| §X.6 | 6        | §Deferred substrate defects item 6 |
| §X.7 | 7        | §Deferred substrate defects item 7 |

---

**Version:** 1.0
**Submission Date:** 2026-09-14
**Last Updated:** 2026-09-14
