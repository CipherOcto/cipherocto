---
name: 0015-b-substrate-defect-impl
description: Implement the 7 substrate defects per RFC-0015-b X.1 through X.7 paired amendment
metadata:
  type: substrate-implementation
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-14
  v: "1.0"
  depends_on:
    - RFC-0015-b
    - RFC-0015
    - RFC-0015-a
    - RFC-0016
    - mission 0015-b-substrate-defect-amendment
release_gate: RFC-0015-b Accepted
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-14
---

# 0015-b-substrate-defect-impl — RFC-0015-b paired substrate implementation

**Status:** Open
**Substrate:** RFC-0015-b §X.1 + §X.2 + §X.3 + §X.4 + §X.5 + §X.6 + §X.7
**Parent:** RFC-0015-b (substrate-defect paired amendment; see `missions/open/0015-b-substrate-defect-amendment.md`)
**Depends on:** RFC-0015-b Accepted; RFC-0015 Accepted; RFC-0015-a Accepted; RFC-0016 Accepted

## Scope

Implement the 7 substrate defects documented in RFC-0015-b §X.1-§X.7.
This is the **paired substrate implementation** mission that lands
post-RFC-0015-b acceptance. Per RFC-0015-a §6.4 paired-invariance
rule, the amendment RFC and this implementation mission form a unit.

## Why this exists

Per `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`,
the R12 + R13 DRY plateau surfaced 7 substrate defects that were
out of scope for the RFC DRY loop. RFC-0015-b drafts the amendment;
this mission implements it. After both land, the 7 defects are
closed and the substrate matches the RFC contract.

## Mission sub-steps (one per RFC-0015-b §X.x)

### Sub-step 1 — `AlreadyInTransition` activation (§X.1)

**File:** `crates/octo-wallet/src/agent.rs` + `crates/octo-wallet/src/error.rs`

- Replace `std::sync::Mutex::lock()` with `parking_lot::Mutex::try_lock()`
  at `agent.rs:488`.
- On `try_lock` failure → `Err(WalletError::AlreadyInTransition(uuid))`.
- Add `parking_lot` dep to `crates/octo-wallet/Cargo.toml` with rationale
  comment (Layer B substrate mutation per parent RFC-0015-a §6.4).
- Doc-comment at `error.rs:184-190` aligns with new implementation
  (closes defect 2 simultaneously).

### Sub-step 2 — `lookup_agent` normalization (§X.3)

**File:** `crates/octo-wallet/src/agent.rs`

- `lookup_agent(caller_did, uuid)` normalizes BOTH unknown + not-owned
  cases to `WalletError::AgentNotFound`.
- `ForbiddenHolderMismatch` variant remains reserved for other substrate
  paths (used by `transition_agent` caller-attestation; not by
  `lookup_agent`).
- Test vectors: TV-LAUP1 (unknown Uuid), TV-LAUP2 (not-owned Uuid),
  TV-LAUP3 (caller is holder — happy path).

### Sub-step 3 — `transition_agent` TOCTOU closure (§X.4)

**File:** `crates/octo-wallet/src/agent.rs`

- Acquire `registry().lock()` FIRST.
- Run `validate_reason` INSIDE the lock (not before).
- Lock-then-validate ordering closes the TOCTOU window.
- Test vectors: TV-TTO1 (concurrent callers; one wins, one gets
  `AlreadyInTransition`), TV-TTO2 (rapid sequential callers;
  second rejected via state machine).

### Sub-step 4 — phantom-event detection (§X.5)

**File:** `crates/octo-wallet/src/agent.rs` + new `crates/octo-wallet/src/state_version.rs`

- Add `state_version: u64` field on `AgentRecord` (default 0;
  bumped before each successful transition).
- Add `AuditChainInconsistent { observed: u64, expected: u64 }`
  variant to `WalletError`.
- Post-rollback verification pass asserts
  `audit_chain.tip.state_version == current_state_version - 1`.
- Mismatch → `WalletError::AuditChainInconsistent { observed, expected }`.

### Sub-step 5 — state-machine test vectors (§X.6)

**File:** `crates/octo-wallet/src/agent.rs` (test module)

| TV           | Edge                        | Expected                                                                        |
| ------------ | --------------------------- | ------------------------------------------------------------------------------- |
| TV-AGT-EDGE1 | `Registered → Running`      | `Ok(TransitionReceipt { current_state: Running, state_version: 1 })`            |
| TV-AGT-EDGE2 | `Running → Terminated`      | `Ok(TransitionReceipt { current_state: Terminated, state_version: 2 })`         |
| TV-AGT-EDGE3 | `Terminated → Running`      | `Err(WalletError::InvalidStateTransition { from: Terminated, to: Running })`    |
| TV-AGT-EDGE4 | `Registered → Terminated`   | `Err(WalletError::InvalidStateTransition { from: Registered, to: Terminated })` |
| TV-AGT-EDGE5 | `Running → Running`         | `Ok(TransitionReceipt)` idempotent (no audit append; state_version unchanged)   |
| TV-AGT-EDGE6 | post-§X.4 TOCTOU regression | Race test: 2 concurrent callers; one wins, one gets `AlreadyInTransition`       |

### Sub-step 6 — RFC-0015 §Pre-existing Substrate refresh (§X.7)

**File:** `rfcs/accepted/process/0015-wallet-agent-operations.md`

- Add 3 rows to §Pre-existing Substrate table:
  - `transition_agent(caller_did, uuid, target, reason)`
  - `TransitionReceipt` projection
  - `AgentRecord.state: AgentState` (default `Registered`)
- Add "Phase 2 unblock landing (commit `next e09f3e3a` 2026-09-13)"
  to timestamp column for the new rows.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-wallet` (Layer B) — substrate mutations: lock discipline,
  state-version tracking, error variant additions, test coverage.
- `octo-audit` (Layer B) — phantom-event detection uses
  `audit_chain.tip.state_version` accessor (additive per RFC-0016-b
  if not present; otherwise already-landed).
- `octo-cli` (Layer C/D) — **NO** code changes. CLI error exit
  mapping already declared at parent RFC-0015-a acceptance; substrate
  changes flow through transparently.

NO new Layer A types introduced.

## Acceptance Criteria

- [ ] Sub-step 1 landed (`try_lock` + `AlreadyInTransition` activation)
- [ ] Sub-step 2 landed (`lookup_agent` normalization + 3 test vectors)
- [ ] Sub-step 3 landed (TOCTOU closure + 2 test vectors)
- [ ] Sub-step 4 landed (state_version + `AuditChainInconsistent` + verification pass)
- [ ] Sub-step 5 landed (6 state-machine edge test vectors)
- [ ] Sub-step 6 landed (RFC-0015 §Pre-existing Substrate refresh)
- [ ] All 7 defects closed per RFC-0015-b §X.1-§X.7
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --features full --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-wallet --lib agent` green (all 11 new TVs pass)
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)
- [ ] Multi-round DRY review loop on implementation (per parent RFC pattern)

## DRY review pattern (paired-acceptance mirror)

R1 spawn 5-len reviewers on substrate diff + RFC text refresh.
Aggregate → R1.5 fix → R2 (DRY verification round 1) → R2.5 fix →
R3 (DRY verification round 2 = DRY CLOSED) → closure artifacts
(audit doc + memory card).

5 reviewers per round:

1. correctness (does each sub-step match the RFC-0015-b §X.x requirement?)
2. layer-model (Layer B mutation; no CLI changes; no reverse deps)
3. simplification (canonical surfaces; no parallel abstractions)
4. hygiene (file:line vs §symbol refs; prettier compliance)
5. substrate-faithfulness (does implementation match the RFC-0015-b
   contract verbatim?)

## Cross-references

- RFC-0015-b §X.1 + §X.2 + §X.3 + §X.4 + §X.5 + §X.6 + §X.7
- `missions/open/0015-b-substrate-defect-amendment.md` (paired amendment RFC)
- `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` §Deferred substrate defects
- `crates/octo-wallet/src/agent.rs` (substrate target)
- `crates/octo-wallet/src/error.rs` (error variant additions)
- `rfcs/accepted/process/0015-wallet-agent-operations.md` (RFC text refresh target for §X.7)
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- [[cipherocto-design-principles]] — Layer B stability contract

## Why gate

Release-gated on RFC-0015-b Accepted (parent amendment RFC).
Per RFC-0015-a §6.4 paired-invariance rule, the amendment RFC
lands Accepted FIRST, then this implementation mission lands
(which references the Accepted amendment as substrate contract).
This sequencing closes the [[no-phantom-mission-pointer]]
violation pattern: this mission's `depends_on:` cites a real
accepted RFC, not a phantom pointer.
