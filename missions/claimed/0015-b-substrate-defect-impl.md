---
name: 0015-b-substrate-defect-impl
description: Implement the 4 surviving substrate defects per RFC-0015-b X.1 through X.4 paired amendment (defect 5 demoted to RFC-0012-v4 paired Layer A cycle)
metadata:
  type: substrate-implementation
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-14
  v: "1.2"
  depends_on:
    - RFC-0015-b
    - RFC-0015
    - RFC-0015-a
    - mission 0015-b-substrate-defect-amendment
release_gate: RFC-0015-b Accepted
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-14
---

# 0015-b-substrate-defect-impl — RFC-0015-b paired substrate implementation

**Status:** Open
**Substrate:** RFC-0015-b §X.1 + §X.2 + §X.3 + §X.4 (defect 5 demoted to RFC-0012-v4 paired Layer A cycle; out of scope for this mission)
**Parent:** RFC-0015-b (substrate-defect paired amendment; see `missions/claimed/0015-b-substrate-defect-amendment.md`)
**Depends on:** RFC-0015-b Accepted; RFC-0015 Accepted; RFC-0015-a Accepted

## Scope

Implement the **4 surviving canonical-substrate defects** documented in RFC-0015-b §X.1, §X.2, §X.3, §X.4. This is the **paired substrate implementation** mission that lands post-RFC-0015-b acceptance. Per RFC-0015-a §6.5 Layer A Paired-Acceptance Bridge, the amendment RFC and this implementation mission form a unit.

Defect 5 (phantom-event window on audit-append + rollback) is **DEFERRED** to a separate RFC-0012-v4 paired Layer A amendment cycle (requires `state_version: u64` field on `AuditEvent` + `audit_chain.tip.state_version: u64` accessor on `AppendOnlyAuditSink` in `octo-audit-core` Layer A frozen per CLAUDE.md §Layer A stability). Out of scope for this mission.

## Why this exists

Per `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md`, the R12 + R13 DRY plateau surfaced 7 substrate defects that were out of scope for the RFC DRY loop. RFC-0015-b §Motivation table (post-R2.5 fix) reclassifies the defects against the canonical substrate patterns established at parent RFC-0015-a acceptance: **4 defects survive canonicalization, 2 defects are rejected (defect 4 TOCTOU flawed premise; defect 5 demoted), and 1 defect (defect 5 phantom-event) is DEFERRED to the separate RFC-0012-v4 Layer A amendment cycle.** This mission implements the 4 surviving canonical-substrate defects.

## Mission sub-steps (one per RFC-0015-b §X.x)

### Sub-step 1 — `AlreadyInTransition` activation via in-flight flag (§X.1)

**File:** `crates/octo-wallet/src/agent.rs` + `crates/octo-wallet/src/error.rs`

- Add `transitioning: bool` field to `AgentRecord` struct (additive; default `false`; serde-defaulted per `#[serde(default)]` attribute to preserve pre-amendment deserialization round-trip per parent RFC-0015-a additive-field precedent).
- `transition_agent` reads `transitioning` flag INSIDE canonical GLOBAL `std::sync::Mutex` lock (per parent RFC-0015-a §6.1 (1); NO `parking_lot::Mutex::try_lock` — parent §Alternatives Considered EXPLICITLY REJECT of `parking_lot::Mutex::try_lock`).
- If `transitioning == true` on entry to critical section → return `Err(WalletError::AlreadyInTransition(uuid))`; NO state mutation; NO audit append.
- If `transitioning == false` on entry → set `true`; proceed with state-machine work + audit append + rollback-or-success per parent §6.1 (3) + §6.1 (5); reset `false` on exit (RAII guard preferred for exception safety).
- Lock acquisition order preserved per parent §6.1 (1) + §6.1 (6): validate_reason (pure function) → `std::sync::Mutex::lock()` → re-read `current_state` INSIDE lock → in-flight flag check → state machine.
- Doc-comment hygiene at `crates/octo-wallet/src/error.rs` §AlreadyInTransition: update to reference "in-flight `transitioning` flag inside canonical GLOBAL `std::sync::Mutex`" (replaces legacy `parking_lot::Mutex::try_lock` reference).
- NO new Cargo.toml dep added.

**Test vectors:**

- TV-WLT-AGT-29: in-flight flag activation. Caller A enters lock + sets flag; caller B serializes at lock acquisition + observes flag=true → returns `Err(WalletError::AlreadyInTransition(uuid))`. Sequential test (canonical GLOBAL std Mutex ordering; deterministic per parent §6.1 (1)).

### Sub-step 2 — `lookup_agent` existence-leak closure (§X.2)

**File:** `crates/octo-wallet/src/agent.rs`

- `lookup_agent(caller_did, uuid)` normalizes BOTH unknown Uuid + caller-DID mismatch cases to `WalletError::AgentNotFound`.
- `ForbiddenHolderMismatch` variant REMAINS in `WalletError` enum (used by `transition_agent` caller-attestation per parent §6.1 (2); distinction between unknown-vs-not-owned contributes to `transition_agent`'s caller-attestation error reporting integrity).
- No changes to `transition_agent` substrate path (the existence-leak closure is scoped to `lookup_agent` only per RFC-0015-b §X.2 acceptance criteria).

**Test vectors:**

- TV-WLT-AGT-25: not-owned Uuid (caller_did != holder_did) → `Err(WalletError::AgentNotFound(uuid))` (genuinely new test; closes the existence-leak side-channel).
- Happy-path + unknown-UUID scenarios are PRESERVED per parent RFC-0015 TV-WLT-AGT-22 + TV-WLT-AGT-23 (no new TVs needed for those cases; §X.2 closure does not modify happy-path or unknown-UUID substrate behavior).

### Sub-step 3 — state-machine test coverage (§X.3)

**File:** `crates/octo-wallet/src/agent.rs` (test module)

- 1 genuinely new state-machine edge TV:

| TV            | Edge                   | Expected                                                                                                                                                |
| ------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TV-WLT-AGT-27 | `Terminated → Running` | `Err(WalletError::InvalidStateTransition { from: Terminated, to: Running })` — terminal-state reactivation rejected per parent §6.4 state-machine table |

- Concurrent in-flight rejection coverage is canonical home at Sub-step 1 §X.1 test `TV-WLT-AGT-29` (NOT duplicated here per R2.5 MED finding #1).
- Existing parent RFC-0015-a TVs (TV-WLT-AGT-3 through 11c) cover the canonical happy-path + idempotent + invalid-edge surface; no duplication.

### Sub-step 4 — RFC-0015-a §Amendment Surface parity refresh (§X.4)

**File:** `rfcs/accepted/process/0015-a-wallet-agent-write-path.md`

- ADD a new `#### §Amendment Surface` heading to RFC-0015-a (mirrors RFC-0015 §Amendment Surface per substrate-faithful single-source-of-truth principle; canonical home for additive write-path items post-RFC-0015-a acceptance per [[deferred-vs-unspecified]]).
- 1 additive row in the new §Amendment Surface table:

  | Substrate item                                                                 | Layer | Source                                       | Notes                                                                   |
  | ------------------------------------------------------------------------------ | ----- | -------------------------------------------- | ----------------------------------------------------------------------- |
  | `AgentRecord.transitioning: bool` (additive; default `false`; serde-defaulted) | B     | RFC-0015-b §X.1 acceptance; Sub-step 1 above | In-flight flag for `AlreadyInTransition` activation per RFC-0015-b §X.1 |

- 1 forward-pointer row:

  | Substrate item                                                                       | Layer | Source                                                 | Notes                   |
  | ------------------------------------------------------------------------------------ | ----- | ------------------------------------------------------ | ----------------------- |
  | Defect 5 phantom-event detection (`state_version` + `audit_chain.tip.state_version`) | A     | DEFERRED to RFC-0012-v4 paired Layer A amendment cycle | Out of RFC-0015-b scope |

- Add timestamp entry: "Substrate-defect amendment landing (RFC-0015-b Acceptance) 2026-09-14"

## Acceptance Criteria

- [ ] Sub-step 1 landed: `transitioning: bool` field on `AgentRecord` + `transition_agent` in-flight flag check + RAII guard + `AlreadyInTransition` activation; doc-comment hygiene on `WalletError::AlreadyInTransition`; TV-WLT-AGT-29 passes
- [ ] Sub-step 2 landed: `lookup_agent` normalization for both unknown + not-owned cases; `ForbiddenHolderMismatch` retained for `transition_agent`; TV-WLT-AGT-25 passes
- [ ] Sub-step 3 landed: TV-WLT-AGT-27 (`Terminated → Running` rejection) passes; no TV duplication with parent RFC-0015-a TVs
- [ ] Sub-step 4 landed: RFC-0015-a `#### §Amendment Surface` heading added; 1 additive row for `transitioning: bool`; 1 forward-pointer row for defect 5 (DEFERRED to RFC-0012-v4); timestamp entry
- [ ] NO new Cargo.toml dep added (`crates/octo-wallet/Cargo.toml` unchanged)
- [ ] NO new WalletError variants beyond `AlreadyInTransition` (pre-existing; activated not added)
- [ ] NO `parking_lot` dep introduced (canonical GLOBAL `std::sync::Mutex` per parent §6.1 (1) preserved)
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --features full --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-wallet --lib agent` green (all new TVs pass)
- [ ] Pre-amendment deserialization round-trip test for `AgentRecord` (verifies `#[serde(default)]` on `transitioning` reads pre-amendment state without the field per parent RFC-0015-a additive-field precedent)
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)
- [ ] Multi-round DRY review loop on implementation (per parent RFC pattern)

## DRY review pattern (paired-acceptance mirror)

R1 spawn 5-len reviewers on substrate diff + RFC text refresh.
Aggregate → R1.5 fix → R2 (DRY verification round 1) → R2.5 fix →
R3 (DRY verification round 2 = DRY CLOSED) → closure artifacts
(audit doc + memory card).

5 reviewers per round:

1. correctness (does each sub-step match the RFC-0015-b §X.x requirement?)
2. layer-model (Layer B mutation only; no Layer A changes; no CLI changes; no reverse deps)
3. simplification (canonical surfaces; no parallel abstractions; no `parking_lot`; no extra Cargo.toml dep)
4. hygiene (file:line vs §symbol refs; prettier compliance)
5. substrate-faithfulness (does implementation match the RFC-0015-b contract verbatim + parent RFC-0015-a canonical substrate patterns?)

## Cross-references

- RFC-0015-b §X.1 + §X.2 + §X.3 + §X.4 (defect 5 DEFERRED to RFC-0012-v4 paired Layer A cycle)
- `missions/claimed/0015-b-substrate-defect-amendment.md` (paired amendment RFC)
- `docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` §Deferred substrate defects
- `crates/octo-wallet/src/agent.rs` (substrate target)
- `crates/octo-wallet/src/error.rs` (doc-comment hygiene target)
- `rfcs/accepted/process/0015-a-wallet-agent-write-path.md` (RFC text refresh target for §X.4; new `#### §Amendment Surface` heading added)
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- RFC-0015-a §6.1 (canonical lock primitive + acquisition order + audit append + rollback contract)
- RFC-0015-a §6.5 Layer A Paired-Acceptance Bridge (paired-acceptance discipline)
- [[cipherocto-design-principles]] — Layer B stability contract
- [[deferred-vs-unspecified]] — defect 5 deferral rationale

## Why gate

Release-gated on RFC-0015-b Accepted (parent amendment RFC).
Per RFC-0015-a §6.5 paired-acceptance bridge, the amendment RFC
lands Accepted FIRST, then this implementation mission lands
(which references the Accepted amendment as substrate contract).
This sequencing closes the [[no-phantom-mission-pointer]]
violation pattern: this mission's `depends_on:` cites a real
accepted RFC, not a phantom pointer.

## Version History

| Version | Date       | Changes                                                        |
| ------- | ---------- | -------------------------------------------------------------- |
| 1.0     | 2026-09-14 | Initial paired impl mission; pre-R1.5 RFC draft                |
| 1.1     | 2026-09-14 | R2.5: defect 5 demoted; defect 4 rejected; Sub-step 4 retarget |
| 1.2     | 2026-09-14 | R3.5: §X.4 renumber + Sub-step cleanup                         |     |
