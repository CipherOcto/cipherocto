---
name: 0855p-b-mission-coordinator-slashing
description: Mission Coordinator slashing integration per RFC-0855p-b §Implementation Phase 5. Wire `SlashProof` type (RFC §Data Structures L218-230) + `verify_slash_proof` (checks adjudicator signature against governance multi-sig + offense code ∈ RFC-0855p-b §Appendix B canonical set 0x0001-0x0012 [excluding reserved 0x000C-0x000D] + 0x0013-0x0016 + Extension range 0x0100-0xFFFF + penalty ≤ OCTO-O stake locked) + Active→Demoting transition + Demoting→Inactive (penalty applied) + cool-down tracking. COMPOSE with `SlashTallyUpdate` per `octo-coordinator-types/src/lib.rs:45` (commit 93f1758c) AND **EXTEND** `SlashReasonCode` substrate (currently only knows 0x0001/0x0013-0x0016/Extension per `lib.rs:91`+`:154`) to cover the full RFC §Appendix B canonical set. Land verification logic at `crates/octo-coordinator-types/src/slashing.rs` with re-export shim at `crates/octo-network/src/mon/slashing.rs`. RFC-0008 Class A determinism. 4 canonical slash vectors. BLOCKED by `0855p-b-state-machine-types`.
metadata:
  node_type: substrate-coordinator
  type: substrate-slashing
  rfc_source: RFC-0855p-b
  rfc_section: Implementation Phases §Phase 5 (L834-841) + §Data Structures SlashProof (L218-230) + §Appendix B Slash Offense Codes (L945+, extending RFC-0855 §17) + Round 5 adversarial review (L840)
  substrate_home:
    canonical: crates/octo-coordinator-types/src/slashing.rs
    re_export: crates/octo-network/src/mon/slashing.rs
    tests: crates/octo-coordinator-types/tests/canonical_slashing_blobs.rs
  execution_class: A
  deterministic: true
  depends_on:
    - RFC-0855p-b
    - RFC-0855
    - RFC-0008
    - mission:0855p-b-state-machine-types
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-03
v: "1.0"
created: 2026-09-03
---

# Mission `0855p-b-mission-coordinator-slashing` v1.0 — RFC-0855p-b §Phase 5

## Status

Claimed (2026-09-03) by @mmacedoeu — partial land via RFC-0855p-b / RFC-0855p-e substrate (`SlashTallyUpdate` + `SlashReasonCode` in `octo-coordinator-types/src/lib.rs`). Missing: `SlashProof` type + verification fn + Demoting transition.

## RFC

RFC-0855p-b §Implementation Phase 5 (L834-841) + §Data Structures SlashProof (L218-230) + §Appendix B Slash Offense Codes (L945+, extending RFC-0855 §17 "Token Economics Integration") + RFC-0855 §11 "Governance Models".

## Summary

Wire the §Phase 5 slashing integration on top of partial `octo-coordinator-types` substrate + §Phase 1 state machine. `SlashProof` is the canonical wire format for an adjudicated slash decision. Verification fn: (a) adjudicator signature checks against `governance_id` multi-sig (RFC-0855 §11); (b) `offense: u16` must be in §Appendix B canonical set **OR** `SlashReasonCode::Extension(0x0100..=0xFFFF)` user-extension range per `octo-coordinator-types/src/lib.rs:154`; (c) `penalty: u64` ≤ `octo_o_stake_locked`; (d) `coordinator_term_id` matches the target's current term. Successful verification triggers Active→Demoting transition + penalty release from `octo_o_stake_locked`; after penalty applied, Demoting→Inactive.

`SlashTallyUpdate` (Layer B event carrier per `octo-coordinator-types/src/lib.rs:45`) is emitted on successful slash via `SlashTallyUpdate::new(reason_code, target, slash_count, penalty)` (existing constructor per `lib.rs:56`).

**Substrate gap (must address in this mission):** existing `SlashReasonCode` enum (`octo-coordinator-types/src/lib.rs:91`) only knows reason codes `0x0001 (Generic) + 0x0013..0x0016 (FalseAttestation / QuorumTimeout / TallyTamper / LateDelivery)` + `Extension(0x0100..=0xFFFF)`. RFC §Appendix B defines `0x0002..=0x0012` as **substrate-RESERVED** (`lib.rs:140` returns `SlashReasonCodeError`). Phase-5 mission MUST **extend** `SlashReasonCode` substrate (NOT just consume it) to cover the RFC §Appendix B canonical set:
- 0x0001 DoubleSign, 0x0002 LivenessFailure, 0x0003 FounderSquat, 0x0004 Censorship, 0x0005 CoordinatorMisbehavior, 0x0006 KeyCompromise, 0x0007 BanningLegitimateMember, 0x0008 VoteBuying, 0x0009 GenesisCompromise (v1.1-R1-CL-1), 0x000A PlatformMigration (RFC-0850p-c §6a), 0x000B IsReconnectLie (RFC-0850p-c §8), 0x000E CreateGroupFailed, 0x000F CgGroupSpam, 0x0010 FalseWitness, 0x0011 SelfKicked, 0x0012 CrossPlatformWitnessCollusion (RFC-0855p-c §9b).
- 0x000C-0x000D remain RESERVED (NOT slash reasons per RFC-0855p-d §Sub-DC delegation protocol); do NOT add variants.

## Substrate work scope

### Step 1 — `SlashProof` type

Already specified in RFC §Data Structures L218-230. Add definition here (canonical home at Layer B per §Phase 1 substrate extraction pattern).

### Step 2 — Slash verification fn

```rust
pub fn verify_slash_proof(
    proof: &SlashProof,
    record: &CoordinatorRecord,
    governance_set: &[(CoordinatorId, [u8; 32])], // multi-sig signers
    slash_offense_codes: &[u16], // RFC §Appendix B canonical set
) -> Result<(), CoordinatorError>;
```

Returns `Ok(())` on valid slash; `Err(CoordinatorError::*)` on:
- `InvalidAdjudicatorSignature` (signature verification against governance multi-sig failed).
- `UnknownOffenseCode` (offense: u16 not in §Appendix B).
- `PenaltyExceedsStake` (`penalty > octo_o_stake_locked`).
- `TermIdMismatch` (`coordinator_term_id` ≠ target's current term).

### Step 3 — Active→Demoting + Demoting→Inactive transitions

Use `validate_transition` from Phase 1 (per RFC §Appendix A). On `Ok`: build + emit `SlashTallyUpdate::new(reason_code, target_coordinator, slash_count_incremented, penalty)` per existing Layer B constructor (`octo-coordinator-types/src/lib.rs:56`).

### Step 4 — Canonical slash vectors

4 pinned vectors in `tests/canonical_slashing_blobs.rs`:
- Valid slash (Active → Demoting via governance vote).
- Demoting → Inactive (penalty fully applied).
- Invalid signature → `InvalidAdjudicatorSignature`.
- Unknown offense → `UnknownOffenseCode`.

### Step 5 — Cool-down tracking

`pub struct CoolDownTracker { /* coordinator → cool_down_end_epoch */ }` — gates re-election eligibility per RFC §Data Structures `slash_count` field + RFC §Appendix A mermaid `Resigned → Inactive` (cool-down expires before `[*]`).

## Acceptance Criteria

- [ ] `crates/octo-coordinator-types/src/slashing.rs` exists; `SlashProof` type (mirrors RFC L218-230) + `verify_slash_proof` + cool-down tracker.
- [ ] `crates/octo-network/src/mon/slashing.rs` re-export shim (honors RFC §Key Files L849).
- [ ] 4 canonical slash vectors pinned; all PASS.
- [ ] Slash verification fn covers all 4 error variants.
- [ ] Active→Demoting + Demoting→Inactive transitions wired through Phase 1 `validate_transition`.
- [ ] **`SlashReasonCode` substrate EXTENDED** in `octo-coordinator-types/src/lib.rs` (NOT just consumed): add named variants for all RFC §Appendix B canonical codes 0x0001-0x0012 (skipping reserved 0x000C-0x000D); `from_reason_id` no longer returns `Err` for these codes.
- [ ] `SlashTallyUpdate` emitted on successful slash via existing `SlashTallyUpdate::new(...)` constructor (`octo-coordinator-types/src/lib.rs:56`); reason_code: u16 ∈ RFC §Appendix B canonical set 0x0001-0x0012 (excluding 0x000C-0x000D) + 0x0013-0x0016 + user-extension range 0x0100-0xFFFF (per `lib.rs:154`).
- [ ] `cargo clippy -p octo-coordinator-types --all-targets -- -D warnings` clean.

## Dependencies

**Requires:**
- RFC-0855p-b
- RFC-0855 §17 slash codes (extended by RFC-0855p-b §Appendix B 0x0001-0x0012) + §11 governance model
- RFC-0008 Class A determinism
- Mission `0855p-b-state-machine-types` (Phase 1 — SlashProof, CoordinatorRecord, CoordinatorError)

**Optional:**
- Mission `0855p-b-mission-coordinator-liveness` (cool-down expiry interacts with Phase 3 heartbeat)
- `octo_coordinator_types::SlashTallyUpdate::new(...)` (existing constructor, `octo-coordinator-types/src/lib.rs:56`)

## Cross-references

- RFC-0855p-b §Implementation Phase 5 (L834-841)
- RFC-0855p-b §Data Structures SlashProof (L218-230)
- RFC-0855p-b §Appendix B Slash Offense Codes (L945+, extends RFC-0855 §17)
- RFC-0855 §17 Slash Reason Codes (extended by RFC-0855p-b §Appendix B)
- RFC-0855 §11 Governance Models
- Mission `0855p-e-coordinator-types-shared-crate` (commit `93f1758c`; provides `SlashTallyUpdate` + `SlashReasonCode`)

## Version History

| Version | Date | Change |
| ------- | ---- | ------ |
| v1.0    | 2026-09-03 | Initial filing per RFC-0855p-b §Implementation Phase 5 + user audit (2026-09-03). Builds on partial RFC-0855p-b / RFC-0855p-e Layer B substrate (commits `93f1758c`, `b0f14119`). |
