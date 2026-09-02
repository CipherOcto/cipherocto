---
name: 0855p-d3-subgroup-routing-aggregation-teardown
description: Create sub-group routing + aggregation + teardown substrate per RFC-0855p-d3 (P2SR/S2PA/SGTP + MemberAttestation + SignersBitmap + distinct-signer + hodn_quorum + TEARDOWN_GRACE_EPOCHS) in `crates/octo-network/src/dot/subgroup_routing.rs` + `subgroup_teardown.rs`.
metadata:
  node_type: substrate-network
  type: substrate-creation
  originSessionId: RFC-0855p-d3 author session
  created: 2026-09-02
  v: "1.0"
  depends_on:
    - RFC-0855p-d3
    - RFC-0855p-d
    - RFC-0855p-d1
    - RFC-0855p-d2
    - mission 0855p-d1-subgroup-creation-state
    - mission 0855p-d2-subgroup-delegation-lifecycle
    - RFC-0853
    - RFC-0009
    - RFC-0850p-c
    - RFC-0126
    - RFC-0855p-b
    - RFC-0855p-c
status: Open
---

# 0855p-d3-subgroup-routing-aggregation-teardown — Routing + Aggregation + Teardown Substrate per RFC-0855p-d3

**Status:** Open
**Substrate:** RFC-0855p-d3 (per-concern RFC, sibling of RFC-0855p-d INDEX)
**Parent:** RFC-0855p-d (slim INDEX; cross-cutting chain)
**Depends on:** RFC-0855p-d3 Accepted (2026-09-02 at commit `0e915618`); RFC-0855p-d1 + d2 Accepted; missions `0855p-d1-subgroup-creation-state` + `0855p-d2-subgroup-delegation-lifecycle` (subgroup + state + `SubDCDelegationPolicy` re-exports)

## Status

Open (2026-09-02) per RFC-0855p-d3 promotion to Accepted at commit `0e915618`. Hard sequencing: this mission lands LAST in the d chain (after d1 + d2). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] + [[implementation-workflow-hook]].

## Substrate (RFC-0855p-d3)

Per RFC-0855p-d3 §Data Structure + §Specification + §Security Considerations + §Layer-C Substrate Surface:

**`crates/octo-network/src/dot/subgroup_routing.rs`:**

- `ParentToSubRouteEnvelope` (`P2SR` subtype) — parent-to-sub-group route envelope (broadcast scoped to child sub-domain)
- `SubToParentAggregateEnvelope` (`S2PA` subtype) — sub-to-parent aggregate envelope (cross-sub-group witness collection rolled up to parent)
- `MemberAttestation` — individual attestation (Layer C; not embedded in Layer-B wire)
- `SignersBitmap` — `from_indices` rejects duplicate indices BEFORE bitmap construction
- `aggregate_id` derivation
- `hodn_quorum(witness_set_size)` policy lookup
- `mesh_aggregated_signature` verification
- Distinct-signer enforcement
- Bitmap-vs-quorum coverage check ordering (BLS verify FIRST, then count_ones == attestations.len(), then count_ones >= hodn_quorum(witness_set_size))

**`crates/octo-network/src/dot/subgroup_teardown.rs`:**

- `TeardownProofEnvelope` (`SGTP` subtype) — final attestation that releases resources + cascades dissolution
- Grace-elapsed check: `local_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS = 50` (recipient-local, NOT envelope-supplied)
- State transition `Dissolving → Dissolved`

**Constants:**

- `MAX_AGGREGATE_ATTESTATIONS = 1024` (witness collection cap)
- `TEARDOWN_GRACE_EPOCHS = 50` (dissolution bound)
- `SUBGROUP_ROUTE_CONTEXT = "DOT/1/CGROUP_SUB/route"`
- `SUBGROUP_AGGREGATE_CONTEXT = "DOT/1/CGROUP_SUB/aggregate"`
- `SUBGROUP_TEARDOWN_CONTEXT = "DOT/1/CGROUP_SUB/teardown"`
- `pub use` re-exports of `MAX_BIND_AWAIT_EPOCHS`, `MAX_BIND_RETRY_COUNT`, `MAX_FSKEW_EPOCHS`, `RACE_EPOCHS` from d1 (canonical home)

## Parent

RFC-0855p-d (slim INDEX; cross-cutting chain). Per RFC-0855p-d §Layer placement table, RFC-0855p-d3 owns the P2SR/S2PA/SGTP + bitmap + quorum + teardown substrate surface.

## Depends on

See YAML frontmatter `depends_on` block. Hard sequencing:

1. `mission 0855p-d1-subgroup-creation-state` (subgroup + state + `MAX_BIND_AWAIT_EPOCHS` + `MAX_BIND_RETRY_COUNT` + `MAX_FSKEW_EPOCHS` + `RACE_EPOCHS` re-exports)
2. `mission 0855p-d2-subgroup-delegation-lifecycle` (`SubDCDelegationPolicy` re-export)
3. This mission lands LAST

## Acceptance Criteria

- [ ] `crates/octo-network/src/dot/subgroup_routing.rs` created
- [ ] `crates/octo-network/src/dot/subgroup_teardown.rs` created
- [ ] `ParentToSubRouteEnvelope` (P2SR) + `SubToParentAggregateEnvelope` (S2PA) + `MemberAttestation` + `SignersBitmap` types defined
- [ ] `TeardownProofEnvelope` (SGTP) type defined
- [ ] `SignersBitmap::from_indices` rejects duplicate indices BEFORE bitmap construction
- [ ] `hodn_quorum(witness_set_size: usize) -> usize` implemented (returns threshold count for S2PA quorum per RFC-0855p-d3 §Specification)
- [ ] `mesh_aggregated_signature` verification (BLS12-381 G1 48-byte compressed per RFC-0855p-b §Witness Set Aggregation)
- [ ] Bitmap-vs-quorum coverage check ordering enforced per RFC-0855p-d3 Appendix B: BLS first, then `signers_bitmap.count_ones() == attestations.len()` (distinct-signer), then `signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)` (quorum)
- [ ] `MAX_AGGREGATE_ATTESTATIONS = 1024` + `TEARDOWN_GRACE_EPOCHS = 50` enforcement
- [ ] Teardown grace check uses `local_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS` (recipient-local; envelope `teardown_epoch` is informational/audit-only)
- [ ] State transition `Dissolving → Dissolved` enforced only on SGTP accept
- [ ] `pub use` re-exports of d1 constants per RFC-0855p-d3 §Layer placement
- [ ] Test vectors TV-SG-8, TV-SG-9, TV-SG-9a, TV-SG-9b, TV-SG-9c pass per RFC-0855p-d3 §Test Vectors
- [ ] `cargo test -p octo-network subgroup_routing subgroup_teardown` zero failures
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean

## Sub-steps

1. **VERIFY GATE** — RFC-0855p-d3 + d1 + d2 Accepted; missions `0855p-d1-subgroup-creation-state` + `0855p-d2-subgroup-delegation-lifecycle` substrate landed
2. Create `crates/octo-network/src/dot/subgroup_routing.rs` skeleton
3. Define core types (`P2SR`, `S2PA`, `MemberAttestation`, `SignersBitmap`, `aggregate_id`)
4. Implement `hodn_quorum(witness_set_size: usize) -> usize` policy lookup per RFC-0855p-d3 §Layer placement L38 (canonical name per plateau closure `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`)
5. Implement `mesh_aggregated_signature` BLS verification
6. Implement bitmap-vs-quorum coverage check ordering (Appendix B)
7. Create `crates/octo-network/src/dot/subgroup_teardown.rs` skeleton
8. Define `SGTP` type + grace-elapsed check (recipient-local)
9. Implement state transition `Dissolving → Dissolved`
10. Add test vectors TV-SG-8..9c (including 9a distinct-signer, 9b bitmap-quorum, 9c ordering)
11. Verify cargo test + clippy + fmt

## Test Vectors (per RFC-0855p-d3 §Test Vectors)

- TV-SG-8: valid P2SR + S2PA acceptance; aggregate verification passes; `aggregate_id` derivation matches spec
- TV-SG-9: valid SGTP acceptance; grace-elapsed check passes; state `Dissolving → Dissolved` transitions
- TV-SG-9a: distinct-signer enforcement — duplicate bitmap indices → reject
- TV-SG-9b: bitmap-vs-quorum coverage — `count_ones < hodn_quorum` → reject
- TV-SG-9c: ordering enforcement — `mesh_aggregated_signature` verify failure → reject before bitmap-vs-quorum check

## Layer direction (per [[cipherocto-design-principles]])

- `subgroup_routing.rs` (Layer C) — routing + aggregation logic
- `subgroup_teardown.rs` (Layer C) — teardown + state transition logic
- `P2SR/S2PA/SGTP` envelopes (Layer B) — wire format + canonical encoding
- `MemberAttestation` (Layer C) — per-member attestation record (not embedded in Layer-B wire; consumed by substrate per RFC-0855p-d3 §Layer placement L37)
- `SignersBitmap` (Layer B) — embedded in `S2PA` envelope wire format (RFC-0855p-d3 line 247 field `signers_bitmap: SignersBitmap`; canonical encoding per RFC-0126)
- `aggregate_id` (Layer B) — wire field of `S2PA` envelope (RFC-0855p-d3 line 248 field `aggregate_id: [u8; 32]`); the `derive_aggregate_id` function (line 281) is Layer A (pure BLAKE3 derivation)
- `hodn_quorum` policy (Layer C) — looks up witness-set-size threshold per RFC-0855p-d3 §Layer placement L38

No upward dependency. Substrate recipients reading unknown envelope subtypes fail closed.

## Backward compat

Substrate-creation; no existing code. Additive to module tree. Re-exports d1 constants (cross-RFC canonical home pattern per plateau closure).

## Risk

- **Bitmap-vs-quorum check ordering**: implementing `count_ones` check before BLS verify wastes cycles on forged bitmaps; implementing it after allows forged-bitmap DoS. Mitigation: enforce strict ordering per RFC-0855p-d3 Appendix B; explicit TV-SG-9c test.
- **Quorum forgery via plain bitmap**: without `mesh_aggregated_signature` covering `signers_bitmap`, attacker can submit any bitmap with valid aggregate. Mitigation: explicit bitmap coverage test (per plateau closure `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`).
- **Distinct-signer bypass via repeated indices**: `SignersBitmap::from_indices` accepting duplicate indices allows quorum forgery. Mitigation: reject duplicates BEFORE bitmap construction (per RFC-0855p-d3 §Data Structure).
- **Teardown grace recipient-local vs envelope-supplied**: using `teardown_epoch` (envelope-supplied) instead of `local_epoch` (recipient-local) allows late delivery. Mitigation: enforce recipient-local check per RFC-0855p-d3 Appendix A.

## Notes

- Per RFC-0855p-d3 §Implementation Notes, this mission is the substrate anchor for the d3 RFC.
- Re-exports d1 constants for cross-RFC canonical home pattern (per `docs/audits/2026-09-02-rfc-0855p-de-review-dry.md`).
- `hodn_quorum` parameter naming: `witness_set_size` (NOT `successor_member_set_size`); canonical name per plateau closure `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`.

## Cross-references

- RFC-0855p-d (slim INDEX; cross-cutting chain)
- RFC-0855p-d1 (canonical home for `MAX_BIND_AWAIT_EPOCHS`, `MAX_BIND_RETRY_COUNT`, `MAX_FSKEW_EPOCHS`, `RACE_EPOCHS`)
- RFC-0855p-d2 (`SubDCDelegationPolicy` re-export)
- RFC-0855p-d3 (this mission's canonical spec; §Data Structure, §Specification, §Security, §Test Vectors, §Appendix A, §Appendix B)
- RFC-0855p-b (witness set aggregation; BLS12-381 G1 48-byte compressed)
- RFC-0855p-c (DomainCoordinator Role)
- RFC-0853 (BLAKE3-256)
- RFC-0009 (Identity substrate)
- RFC-0126 (DCS canonical BE bytes)
- Mission `0855p-d1-subgroup-creation-state` (prerequisite; substrate must land first)
- Mission `0855p-d2-subgroup-delegation-lifecycle` (prerequisite; substrate must land second)

## Claimant

(none — Open mission)
