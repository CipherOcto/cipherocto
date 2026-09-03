---
name: 0855p-b-state-machine-types
description: Mission Coordinator state machine substrate per RFC-0855p-b §Implementation Phase 1. Land the 6 RFC-defined base types (CoordinatorLifecycle enum, CoordinatorSource enum, CoordinatorId alias, CoordinatorRecord struct, CoordinatorError enum — per RFC Phase 1 checklist L802-810) + v1.1-added `GenesisState` 3-state machine (per RFC §"Genesis State Machine" L286-313) at the Layer B canonical home `crates/octo-coordinator-types/src/state.rs` with re-export shim `crates/octo-network/src/mon/coordinator.rs`. Layer A determinism contract per RFC-0855p-b §RFC-0008 Execution Class Mapping (Class A) — `Borsh` + canonical-bytes derivation + 6 canonical-blob test vectors (TV-1..TV-6 per §Test Vectors L671-787) re-pinned at `crates/octo-coordinator-types/tests/canonical_coordinator_blobs.rs`. State transition validity table mirrors RFC §Appendix A state diagram (12 transitions: Designated→Elected; Elected→Active; Active→Active/Suspect/Handover/Demoting/Resigned; Suspect→Active/Handover; Handover→Inactive; Demoting→Inactive; Resigned→Inactive). BLOCKING prerequisite for `0855p-b-mission-coordinator-election` (Phase 2), `0855p-b-mission-coordinator-liveness` (Phase 3), `0855p-b-mission-coordinator-handover` (Phase 4), `0855p-b-mission-coordinator-slashing` (Phase 5); also blocks `octo-network/src/dot/dc.rs:59` `CoordinatorLifecycle::Handover` doc-comment reference resolution + RFC-0855p-b §Future Work §F1 `DomainCoordinator` specialization substrate (which cites RFC-0855p-c).
metadata:
  node_type: substrate-coordinator
  type: substrate-state-machine
  rfc_source: RFC-0855p-b
  rfc_section: Implementation Phases §Phase 1 (L801-810) + Data Structures (L147-256) + Key Files (L847) + Test Vectors (L671-787) + Appendix A (L925-944) + RFC-0008 Execution Class Mapping (L483-496)
  substrate_home:
    canonical: crates/octo-coordinator-types/src/state.rs
    re_export: crates/octo-network/src/mon/coordinator.rs
    tests: crates/octo-coordinator-types/tests/canonical_coordinator_blobs.rs
  execution_class: A
  deterministic: true
  depends_on:
    - RFC-0855p-b
    - RFC-0008
  blocks:
    - 0855p-b-mission-coordinator-election
    - 0855p-b-mission-coordinator-liveness
    - 0855p-b-mission-coordinator-handover
    - 0855p-b-mission-coordinator-slashing
    - "RFC-0855p-b §Future Work §F1 DomainCoordinator specialization" (reuses `CoordinatorRecord` per RFC §Future Work F1 + cites `RFC-0855p-c`)
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-03
v: "1.0"
created: 2026-09-03
---

# Mission `0855p-b-state-machine-types` v1.0 — RFC-0855p-b §Phase 1

## Status

Claimed (2026-09-03) by @mmacedoeu — substrate unowned. RFC-0855p-b §Implementation Phase 1 promises 8 base types + `GenesisState` 3-state machine (v1.1 addition) + state transition validity + 6 canonical-blob test vectors; **0/9 types defined** in `crates/`. §Key Files L847-851 promises `crates/octo-network/src/mon/coordinator.rs` (MISSING) + `mon/election.rs` (MISSING) + `mon/slashing.rs` (MISSING).

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §Implementation Phases §Phase 1 (L801-810) + §Data Structures (L147-256) + §Key Files (L847) + §Test Vectors (L671-787) + §Appendix A (L925-944) + §RFC-0008 Execution Class Mapping (L483-496).

## Summary

Land the 8 RFC-defined Mission Coordinator state-machine types at the Layer B canonical home `crates/octo-coordinator-types/src/state.rs`. Adopt the pattern already proven by `commit 93f1758c` (which extracted `SlashTallyUpdate` + `SlashReasonCode` + `HandoverReasonTypeId` from `octo-network/src/dot/handover.rs` to the shared Layer B crate); mirror via `pub use` re-export shim at `crates/octo-network/src/mon/coordinator.rs` so Layer-C consumers retain ergonomic call paths. Per RFC-0008 Class A determinism, every type derives `Borsh` + canonical-bytes serialization + unit tests pinning 6 canonical-blob vectors from RFC §Test Vectors.

The state machine transition table mirrors RFC §Appendix A mermaid diagram (Designated→Elected→Active→Suspect→Handover→Demoting→Resigned→Inactive). The `CoordinatorError` enum captures every invalid transition + structural violation.

## Substrate work scope

### Step 1 — Pure types in Layer B crate

**New file `crates/octo-coordinator-types/src/state.rs`** with 9 types:

1. `pub enum CoordinatorLifecycle` (8 variants, discriminants `0x00..=0x07`) per RFC §Data Structures L153-167 (Designated / Elected / Active / Suspect / Handover / Demoting / Resigned / Inactive).
2. `pub enum CoordinatorSource` (4 variants, `0x00..=0x03`) per RFC L175-182 (GenesisDesignation / Election / Handover / Emergency).
3. `pub type CoordinatorId = [u8; 32]` (alias for `PeerId` in mission namespace per RFC L74 + L120).
4. `pub struct ElectionTally` (7 fields) per RFC L188-206 — election_id, election_epoch, closed_epoch, governance_model, ballots: Vec<ElectionBallot>, winner: CoordinatorId, votes_received, votes_total.
5. `pub struct ElectionBallot` (4 fields) per RFC L208-216 — voter_peer_id, candidate_peer_id, ballot_epoch, signature.
6. `pub struct SlashProof` (7 fields) per RFC L218-230 — slash_id, coordinator, coordinator_term_id, offense: u16 (RFC-0855 §17 slash codes), evidence, penalty, adjudicator + adjudicator_signature.
7. `pub struct CoordinatorRecord` (10 fields) per RFC L234-254 — coordinator_peer_id, state, term_start_epoch, term_end_epoch, source, coordinator_term_id (BLAKE3 of peer_id||start||source), slash_count, octo_o_stake_locked, last_heartbeat_epoch, heartbeat_interval.
8. `pub enum CoordinatorError` per RFC §Error Handling L497-535 — InvalidTransition { from, to }, InvalidProvenance, TermEndEpochExceedsMax, HeartbeatOutOfRange, SlashCountOverflow, StakeUnderflow, etc.
9. `pub enum GenesisState` (3 variants v1.1-added per RFC L286-313, L334-345) — GenesisDesignated / GenesisSelfAttest / GenesisActive (discriminants `0x00..=0x02`). Mission also imports `GenesisState::canonical_bytes()` for the v1.1 `GenesisState Bootstrap` transition table (5 transitions including `GenesisSelfAttest → GenesisActive` failure path; `GenesisActive → Inactive` for creator-key-compromise case with `SlashReasonCode::GenesisCompromise(0x0009)`).

All types derive `BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq, Eq`.

### Step 2 — State transition validity

`pub fn transition_valid(from: CoordinatorLifecycle, to: CoordinatorLifecycle) -> bool` + `pub fn validate_transition(record: &CoordinatorRecord, next: CoordinatorLifecycle) -> Result<(), CoordinatorError>` — encode the transition table:

| From | To | Trigger |
|------|----|---------|
| Designated | Elected | election tally meets quorum |
| Elected | Active | activation envelope + 1/3 ack |
| Active | Active | heartbeat OK |
| Active | Suspect | 2× heartbeat miss |
| Suspect | Active | heartbeat recovered |
| Suspect | Handover | grace period exceeded |
| Active | Handover | signed HandoverRequest |
| Active | Demoting | slash proof + governance vote |
| Active | Resigned | signed ResignationRequest |
| Handover | Inactive | successor Active |
| Demoting | Inactive | penalty applied |
| Resigned | Inactive | cool-down expires |

### Step 3 — Layer C re-export shim

**New file `crates/octo-network/src/mon/coordinator.rs`** with:

```rust
pub use octo_coordinator_types::state::{
    CoordinatorLifecycle, CoordinatorSource, CoordinatorId, CoordinatorRecord,
    ElectionTally, ElectionBallot, SlashProof, CoordinatorError,
    transition_valid, validate_transition,
};
```

Honors RFC §Key Files L847 promise at Layer C while keeping the canonical home at Layer B per [[cipherocto-design-principles]] §Stable Abstractions.

### Step 4 — Canonical-blob test vectors

**New file `crates/octo-coordinator-types/tests/canonical_coordinator_blobs.rs`** with 6 pinned vectors per RFC §Test Vectors L671-787:

- TV-1: Genesis Designation — `coordinator_peer_id: [0xBB; 32], state: Designated, source: GenesisDesignation, ...`
- TV-2: Election Win (DAO) — ballots sorted by `(voter_peer_id, ballot_epoch)` determinism; winner = lowest lex `CoordinatorId`.
- TV-3: Heartbeat Miss → Suspect — Active → Suspect (2× heartbeat interval elapsed).
- TV-4: Slash Proof → Demoting — `SlashProof { offense: 0x0001, adjudicator: [0xAA; 32], ... }`; Active → Demoting.
- TV-5: Cool-down After Resignation — Resigned → Inactive transition + cool-down envelope.
- TV-6: Genesis State Bootstrap (v1.1) — initial `CoordinatorRecord` at mission genesis.

Each TV computes `Borsh-serialized bytes for (CoordinatorRecord || SlashProof || ElectionTally)` under domain separator `BLAKE3_REPUTATION_COORDINATOR_DOMAIN = b"cipherocto/coordinator/state/v1"` and asserts equality to a pinned 32-byte BLAKE3 digest.

### Step 5 — Unit-test matrix

Inline `#[cfg(test)]` in `state.rs`:

- All 12 valid transitions (above table) — `transition_valid` returns `true`.
- All other pairs return `false` (with a small set of explicit anti-transitions as guards: Inactive → *, Designated → Active (skipping Elected), Handover → Resigned, etc.).
- `CoordinatorRecord::canonical_bytes()` is deterministic.
- Pin discriminants = `#[repr(u8)]` discriminants `0x00..=0x07` for forward-compat.

## Acceptance Criteria

- [ ] `crates/octo-coordinator-types/src/state.rs` exists; **9 types** (8 base + `GenesisState` v1.1) defined with `BorshSerialize + BorshDeserialize + Serialize + Deserialize + Clone + Debug + PartialEq + Eq` derives.
- [ ] `pub type CoordinatorId = [u8; 32]` alias declared per RFC L74 + L120.
- [ ] `transition_valid` + `validate_transition` encode the §Appendix A state diagram (12 valid transitions; all other pairs = Invalid).
- [ ] `crates/octo-network/src/mon/coordinator.rs` re-exports the public surface (including `GenesisState`) via `pub use octo_coordinator_types::state::*`.
- [ ] `crates/octo-coordinator-types/tests/canonical_coordinator_blobs.rs` pins 6 canonical vectors (TV-1..TV-6 per RFC §Test Vectors L671-787); each vector carries pinned BLAKE3 digest + builds the `CoordinatorRecord` (and dependents) and asserts byte-equality.
- [ ] `CoordinatorError` enum per RFC §Error Handling L497-535 with at minimum 6 variants.
- [ ] No new cross-crate dependencies (Borsh already in `octo-coordinator-types/Cargo.toml` per commit `93f1758c`).
- [ ] Layer A determinism contract honoured: no `f64`, no `SystemTime::now()`, no HashMap iteration without `BTreeMap` sort.
- [ ] `cargo clippy -p octo-coordinator-types --all-targets -- -D warnings` → 0 warnings.
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` → 0 warnings (re-export surface).
- [ ] `cargo fmt --all -- --check` clean.
- [ ] `cargo test -p octo-coordinator-types --test canonical_coordinator_blobs` → 6/6 PASS.
- [ ] `cargo test -p octo-coordinator-types --lib state` → all unit tests PASS.
- [ ] `octo-network/src/dot/dc.rs:59` doc-comment referencing `CoordinatorLifecycle::Handover` now compiles cleanly against the re-exported type (importable via `use crate::mon::coordinator::CoordinatorLifecycle;`).
- [ ] `octo-network/src/mon/quadratic.rs:8,36` `CoordinatorRecord` doc-comment references now importable via `use crate::mon::coordinator::CoordinatorRecord;`.

## Layer model

- **Layer B (canonical)** — `octo-coordinator-types/src/state.rs` (pure types + canonical encoding + transition validity + Class A determinism). Years-stable; consensus-critical.
- **Layer C (re-export shim)** — `octo-network/src/mon/coordinator.rs` (`pub use octo_coordinator_types::state::*;`). Ergonomic consumer-side call paths; honors RFC §Key Files L847 verbatim.

Per [[cipherocto-design-principles]] §Stable Abstractions: state machine survives 10-year migrations; specialization (DomainCoordinator per RFC-0855p-c §F1, mission-bound overrides per future RFC amendments) layers on top via composition, not duplication.

## Out of scope

- Election algorithm wiring (Phase 2 — separate mission `0855p-b-mission-coordinator-election`).
- Heartbeat envelope (Phase 3 — `0855p-b-mission-coordinator-liveness`).
- Handover envelope (Phase 4 — `0855p-b-mission-coordinator-handover`).
- Slash verification function (Phase 5 — `0855p-b-mission-coordinator-slashing`).
- Per-governance-model election logic (DAO / Centralized / Federated / Autonomous — Phase 2).
- `DomainCoordinatorRecord` specialization (RFC-0855p-c §F1 — cross-RFC).

## Cross-references

- RFC-0855p-b §Implementation Phase 1 (L801-810)
- RFC-0855p-b §Data Structures (L147-256)
- RFC-0855p-b §Key Files (L847)
- RFC-0855p-b §Test Vectors (L671-787)
- RFC-0855p-b §Appendix A State Machine Reference (L925-944)
- RFC-0855p-b §RFC-0008 Execution Class Mapping (L483-496) — Class A determinism
- RFC-0855p-b §Error Handling (L497-535)
- RFC-0008: Deterministic AI Execution Boundary (Execution Class A contract)
- Mission `0855p-b-cross-mission-reputation` (archived/completed; predecessor — covers F2 cross-mission reputation surface, NOT Phase 1 state machine)
- Mission `0855p-e-coordinator-types-shared-crate` (commit `93f1758c`; precedent for Layer B extraction pattern)
- Mission `0855p-b-gossip-successor` v1.2 (archived completed 2026-09-03; orthogonal — gossip ingress anchor-freshness gate at `octo-reputation/src/anchor_freshness.rs`)

## Dependencies

**Requires:**
- RFC-0855p-b (Accepted; canonical spec)
- RFC-0008 (Accepted; Execution Class A determinism contract)

**Optional:**
- `octo_coordinator_types` crate (precedent set by commit `93f1758c`; provides `Borsh` + derive macros)

## Version History

| Version | Date | Change |
| ------- | ---- | ------ |
| v1.0    | 2026-09-03 | Initial filing per RFC-0855p-b §Implementation Phase 1 + user audit (2026-09-03). Mission opens with 0/8 RFC-defined types in `crates/`; unblocks Phase 2-5 + cross-RFC RFC-0855p-c §F1. |
