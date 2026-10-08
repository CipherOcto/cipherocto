---
name: 0013-governance-tally-tests
description: 10 canonical tally test vectors for `voting_weight` + `tally_quorum` + `GovernancePolicy::default_dao` per RFC-0013 §Test Vectors
metadata:
  node_type: substrate-tests
  type: tally-property-tests
  originSessionId: RFC-0013 author session
  created: 2026-09-10
  v: "1.0"
  depends_on:
    - RFC-0013
    - mission 0013-governance-substrate-extraction
    - mission 0013-governance-network-migration
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0013-governance-tally-tests — 10 canonical tally test vectors

**Status:** Claimed — substrate-level property tests for tally helpers
**Substrate:** RFC-0013 §Test Vectors (10 canonical vectors)
**Parent:** RFC-0013

## Scope

Per RFC-0013 §Test Vectors, 10 substrate-level property tests for the pure tally helpers (`voting_weight`, `tally_quorum`) + `GovernancePolicy::default_dao()` + cross-replica tally equivalence via `BTreeMap` ordering.

### Deliverables

1. **`crates/octo-governance-core/tests/tally.rs` (NEW)** — 10 canonical tally test vectors per RFC-0013 §Test Vectors
2. **`crates/octo-governance-core/tests/cross_replica.rs` (NEW)** — cross-replica tally equivalence test (asserts `BTreeMap` iteration is identical across 2 deterministic shuffles)
3. **`crates/octo-governance-core/tests/reputation_bound.rs` (NEW)** — `voting_weight` reputation bound invariant test (u64 reputation cannot overflow `voting_weight` output)

### Acceptance criteria

- [ ] AC-1: `cargo test -p octo-governance-core --test tally` passes all 10 canonical vectors
- [ ] AC-2: `cargo test -p octo-governance-core --test cross_replica` passes (BTreeMap ordering invariance)
- [ ] AC-3: `cargo test -p octo-governance-core --test reputation_bound` passes (reputation u64 bound)
- [ ] AC-4: `voting_weight(dao_policy, reputation=0) == 0` (zero-reputation has zero weight)
- [ ] AC-5: `voting_weight(dao_policy, reputation=u64::MAX) <= u64::MAX` (no overflow)
- [ ] AC-6: `tally_quorum(dao_policy, total_weight, approvals) == true` when `approvals * 3 >= total_weight * 2` (canonical 2/3 supermajority)
- [ ] AC-7: `GovernancePolicy::default_dao()` returns `(Dao, 2, 3, 10, Coordinator)` per RFC-0013 TV-`policy-default-dao`
- [ ] AC-8: Workspace `cargo test --workspace` green

### Dependencies

- `RFC-0013` — canonical substrate spec
- `mission 0013-governance-substrate-extraction` — substrate crates must exist
- `mission 0013-governance-network-migration` — network-domain consumer migrated

### Risk

- **MEDIUM** — Cross-replica tally equivalence drift if `HashMap` accidentally replaces `BTreeMap`. Mitigation: AC-2 explicit test + RFC-0013 §Cross-Replica Tally Equivalence row.
- **LOW** — Reputation bound overflow. Mitigation: AC-5 + RFC-0855 §11.1 reputation bound constraint.
- **LOW** — `default_dao` field drift. Mitigation: AC-7 explicit field-value assertion (model=Dao, num=2, den=3, deadline=10, authority=Coordinator).

### Cross-RFC invariants preserved

- `BTreeMap` iteration deterministic per RFC-0855 §11.3 (cross-replica tally equivalence)
- Reputation u64 bound per RFC-0855 §11.1 (reputation cannot overflow `voting_weight`)
- `repr(u16)` discriminants byte-identical (SQL persistence constraint)

### Test vectors (10 canonical, RFC-0013 §Test Vectors)

| ID | Scenario | Expected |
|----|----------|----------|
| `policy-default-dao` | `GovernancePolicy::default_dao()` | model=Dao, num=2, den=3, deadline=10, authority=Coordinator |
| `voting-weight-zero-reputation` | `voting_weight(dao, 0)` | `0` |
| `voting-weight-max-reputation` | `voting_weight(dao, u64::MAX)` | ≤ u64::MAX (no overflow) |
| `tally-quorum-supermajority-met` | `tally_quorum(dao, total=100, approvals=67)` | `true` (67/100 ≥ 2/3) |
| `tally-quorum-supermajority-fail` | `tally_quorum(dao, total=100, approvals=66)` | `false` (66/100 < 2/3) |
| `tally-quorum-exact-boundary` | `tally_quorum(dao, total=99, approvals=66)` | `true` (66/99 = 2/3 exact) |
| `governance-model-discriminant-invariant` | `GovernanceModel::Dao as u16` | byte-identical to RFC-0855 §11.1 (Dao=1) |
| `proposal-state-discriminant-invariant` | `ProposalState::Voting as u16` | byte-identical to RFC-0855 §11.3 (Voting=1) |
| `decision-type-discriminant-invariant` | `DecisionType::EmergencyRekey as u16` | byte-identical to RFC-0855 §11.3 (EmergencyRekey=5) |
| `cross-replica-btreemap-equivalence` | Vote tally `BTreeMap` iteration across 2 deterministic shuffles | identical sequence (deterministic cross-replica) |
