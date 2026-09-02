---
name: 0855p-d2-subgroup-delegation-lifecycle
description: Create sub-DC delegation lifecycle substrate per RFC-0855p-d2 (SDCD/SDRV/SDRT + SubDCDelegationPolicy + chain-depth counter + root-delegation table) in `crates/octo-network/src/dot/subgroup_delegation.rs`.
metadata:
  node_type: substrate-network
  type: substrate-creation
  originSessionId: RFC-0855p-d2 author session
  created: 2026-09-02
  v: "1.0"
  depends_on:
    - RFC-0855p-d2
    - RFC-0855p-d
    - RFC-0855p-d1
    - mission 0855p-d1-subgroup-creation-state
    - RFC-0853
    - RFC-0009
    - RFC-0850p-c
    - RFC-0126
    - RFC-0855p-b
    - RFC-0855p-c
status: Claimed
---

# 0855p-d2-subgroup-delegation-lifecycle — Sub-DC Delegation Lifecycle Substrate per RFC-0855p-d2

**Status:** Open
**Substrate:** RFC-0855p-d2 (per-concern RFC, sibling of RFC-0855p-d INDEX)
**Parent:** RFC-0855p-d (slim INDEX; cross-cutting chain)
**Depends on:** RFC-0855p-d2 Accepted (2026-09-02 at commit `0e915618`); RFC-0855p-d1 Accepted; mission `0855p-d1-subgroup-creation-state` (subgroup must exist + parent must be `Bound` before delegation has any effect)

## Status

Open (2026-09-02) per RFC-0855p-d2 promotion to Accepted at commit `0e915618`. Hard sequencing: this mission lands AFTER `0855p-d1-subgroup-creation-state` (subgroup + label + record + DelegationId must exist). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] + [[implementation-workflow-hook]].

## Substrate (RFC-0855p-d2)

Per RFC-0855p-d2 §Data Structure + §Sub-DC Delegation Protocol + §Layer-C Substrate Surface:

- `SubDCDelegationEnvelope` (`SDCD` subtype) — parent-DC-signed delegation proof (parent_dc_id + sub_domain_id + sub_dc_id + term_id + current_epoch + nonce)
- `SubDCRevocationEnvelope` (`SDRV` subtype) — revocation proof terminating the delegation; carries `RevocationReasonCode`
- `SubDCRotationEnvelope` (`SDRT` subtype) — rotation proof requiring BOTH parent + sub-DC joint signature (anti-collusion + anti-forgery)
- `SubDCDelegationPolicy::check(proof, subgroup_state_bound=true)` — pure-function scope (Layer A)
- `RevocationReasonCode` — `#[non_exhaustive]` enum (TermExpired / CoordinatorRotation / SubDCMisconduct / SubDCKeyCompromise / SubDCVoluntaryResignation / GroupDecommission)
- Chain-depth counter (≤ `MAX_DELEGATION_CHAIN_PER_TERM = 256`)
- Root-delegation table (≤ `MAX_ROOT_DELEGATION = 1` per parent)
- Replay-key indexes per envelope-type-specific tuple form (canonical BE bytes per RFC-0126)
- Term-window enforcement

## Parent

RFC-0855p-d (slim INDEX; cross-cutting chain INDEX only). Per RFC-0855p-d §Layer placement table, RFC-0855p-d2 owns the SDCD/SDRV/SDRT + delegation policy + chain-depth + root-delegation substrate surface.

## Depends on

See YAML frontmatter `depends_on` block. Hard sequencing:

1. `mission 0855p-d1-subgroup-creation-state` lands FIRST (subgroup + `SubGroupLabel` + `DelegationId` + `MAX_ROOT_DEPTH` re-exports)
2. This mission lands SECOND
3. `mission 0855p-d3-subgroup-routing-aggregation-teardown` lands THIRD (depends on `SubDCDelegationPolicy` from this mission)

## Acceptance Criteria

- [ ] `crates/octo-network/src/dot/subgroup_delegation.rs` created
- [ ] `SubDCDelegationEnvelope` (SDCD) + `SubDCRevocationEnvelope` (SDRV) + `SubDCRotationEnvelope` (SDRT) types defined
- [ ] `SubDCDelegationPolicy::check(proof, subgroup_state_bound=true)` implemented as pure function (Layer A)
- [ ] `RevocationReasonCode` `#[non_exhaustive]` enum defined per RFC-0855p-d2 Appendix B
- [ ] `MAX_DELEGATION_CHAIN_PER_TERM = 256` + `MAX_ROOT_DELEGATION = 1` enforcement
- [ ] Replay-key tuple forms per RFC-0855p-d2 Appendix A: `(SDCD, parent_dc_id, sub_domain_id, sub_dc_id, term_id, current_epoch, nonce)` etc.
- [ ] Joint signature requirement on SDRT: BOTH parent-DC AND retiring-sub-DC must sign (anti-collusion + anti-forgery)
- [ ] `pub use` re-export of `CoordinatorTermId` (defined in this mission) at `subgroup_delegation::CoordinatorTermId` for downstream crate consumers (d3 imports via this path; non-self-reexport)
- [ ] `pub use` re-export of `SubDCDelegationPolicy` (defined in this mission) at `subgroup_delegation::SubDCDelegationPolicy` for downstream crate consumers (d3 imports via this path; non-self-reexport)
- [ ] Test vectors TV-SG-6, TV-SG-7 pass per RFC-0855p-d2 §Test Vectors
- [ ] Subgroup-not-`Bound` → delegation rejected (subgroup_state_bound invariant)
- [ ] Revocation cascade test: SDRV triggers downstream teardown (verified via d3 substrate state transition)
- [ ] `cargo test -p octo-network subgroup_delegation` zero failures
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean

## Sub-steps

1. **VERIFY GATE** — RFC-0855p-d2 + d1 Accepted; mission `0855p-d1-subgroup-creation-state` substrate landed
2. Create `crates/octo-network/src/dot/subgroup_delegation.rs` skeleton
3. Define core types (`SubDCDelegationEnvelope`, `SubDCRevocationEnvelope`, `SubDCRotationEnvelope`, `RevocationReasonCode`, `CoordinatorTermId`)
4. Implement `SubDCDelegationPolicy::check(proof, subgroup_state_bound=true)` (pure function)
5. Implement chain-depth counter + root-delegation table
6. Implement replay-key tuple forms (RFC-0126 canonical BE bytes)
7. Add test vectors TV-SG-6..7
8. Add joint-signature verification test for SDRT
9. Add revocation cascade test (verifies d3 substrate teardown trigger)
10. Verify cargo test + clippy + fmt

## Test Vectors (per RFC-0855p-d2 §Test Vectors)

- TV-SG-6: valid SDCD acceptance; delegation policy check passes; replay-key rejected on duplicate
- TV-SG-7: invalid SDRV → reject; `RevocationReasonCode` exhaustiveness preserved; subgroup not `Bound` → reject

## Layer direction (per [[cipherocto-design-principles]])

- `subgroup_delegation.rs` (Layer C) — delegation lifecycle logic
- `SDCD/SDRV/SDRT` envelopes (Layer B) — wire format + canonical encoding
- `SubDCDelegationPolicy::check` (Layer C) — pure function (deterministic; no I/O; domain-rule logic, NOT crypto primitive)
- `RevocationReasonCode` (Layer B) — `#[non_exhaustive]` enum
- `CoordinatorTermId` (Layer B) — typed term identifier (defined here; consumed by d3 substrate via `pub use` re-export)
- `MAX_DELEGATION_CHAIN_PER_TERM = 256` + `MAX_ROOT_DELEGATION = 1` (Layer C) — delegation-specific limits; canonical home in `subgroup_delegation.rs`; NOT shared with other RFCs

No upward dependency. Substrate recipients reading unknown revocation reason codes fail closed.

## Backward compat

Substrate-creation; no existing code. Additive to module tree. Re-exports `SubGroupLabel`, `DelegationId`, `MAX_ROOT_DEPTH` from d1 (cross-RFC canonical home pattern per plateau closure).

## Risk

- **Joint signature bypass on SDRT**: implementing single-signer SDRT allows unilateral rotation (collusion risk OR forgery risk). Mitigation: explicit joint-signature test vector.
- **Chain-depth overflow**: unbounded delegation chain allows unbounded growth. Mitigation: `MAX_DELEGATION_CHAIN_PER_TERM = 256` enforcement at `SubDCDelegationPolicy::check`.
- **Root-delegation drift**: allowing >1 root delegation breaks `MAX_ROOT_DELEGATION = 1` invariant. Mitigation: explicit root-delegation table check before SDCD acceptance.

## Notes

- Per RFC-0855p-d2 §Layer placement, this mission's substrate is Layer C; envelopes are Layer B; `SubDCDelegationPolicy::check` is Layer A.
- SDRT requires BOTH parent-DC AND retiring-sub-DC signatures (joint signing is the trustless path; SDRV + fresh SDCD is the unreachable-retiring-sub-DC path).
- Revocation cascade: SDRV triggers downstream teardown via d3 substrate state transition (`Bound → Dissolving`); this mission owns the revocation surface, d3 owns the teardown surface.

## Cross-references

- RFC-0855p-d (slim INDEX; cross-cutting chain)
- RFC-0855p-d1 (prerequisite; subgroup must exist + be `Bound` + `DelegationId` re-exported)
- RFC-0855p-d2 (this mission's canonical spec)
- RFC-0855p-d3 (downstream consumer; re-exports `CoordinatorTermId` + `SubDCDelegationPolicy`)
- RFC-0855p-b (slash tally for `SubDCMisconduct` revocation; Mission Coordinator Lifecycle)
- RFC-0855p-c (DomainCoordinator Role + parent DC authority scope)
- RFC-0853 (BLAKE3-256)
- RFC-0009 (Identity substrate)
- RFC-0126 (DCS canonical BE bytes for replay-key tuple forms)
- Mission `0855p-d1-subgroup-creation-state` (prerequisite; substrate must land first)
- Mission `0855p-d3-subgroup-routing-aggregation-teardown` (downstream; depends on this mission)

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-02 after W4.5 DRY closure at commit `647b2cf4`; lands SECOND per substrate-first pattern d1→d2→d3)
