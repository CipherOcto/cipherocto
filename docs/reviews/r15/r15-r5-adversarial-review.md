# R15 R5 Adversarial Review

## Scope
Re-review of `crates/octo-network/src/{mon,dc,gossip,common}/` after R1-R4 fixes.
Focus on: unchecked invariants, integer overflow, allocation bounds, panic
in hot paths, and semantic mismatches between count-based and weight-based
quorum/majority logic.

## Findings & Fixes

### R5-1 (LOW) — `mon/reputation.rs::gossip_topic` accepts empty coordinator
**Issue**: `format!("/dot/reputation/mon/{coordinator}")` would produce the
malformed topic `"/dot/reputation/mon/"` if `coordinator` is empty.
**Fix**: `assert!(!coordinator.is_empty(), "coordinator must not be empty")`.
Added `gossip_topic_rejects_empty` test.

### R5-2 (LOW) — `mon/governance.rs::cast_vote` accepts weight=0
**Issue**: Weight-zero votes are spam — they count toward the BTreeMap size
without affecting outcome.
**Fix**: Reject `weight=0` in `cast_vote` (and `cast_vote_weighted`). Added
`cast_vote_rejects_zero_weight` test.

### R5-3 (MEDIUM) — `mon/execution.rs::SwarmCoordinator::assign_task` silently overwrites
**Issue**: When an agent already has `current_task`, the previous task was
silently overwritten in `agent.current_task` but remained in
`self.assignments` (orphan entry). `complete_task` would never find the
agent's `current_task` matching the orphan.
**Fix**: Return `false` if the agent is busy. Caller must `complete_task` or
`fail_task` first. Added `test_swarm_assign_busy_agent`.

### R5-4 (HIGH) — DAO uses count-based quorum but weight-based majority
**Issue**: `Proposal::resolve()` for `Dao` checks count-based quorum
(`eligible_voters.count()`) but computes majority from weights. This
creates a semantic mismatch: a proposal with 4/10 voters each holding
60 weight (240/600 = 40%) passes the count-quorum (4/10 >= 2/3 would
fail, but 5/10 case passes) but the weight-based majority is correct.
The fix is to add weight-based quorum that matches the weight-based
majority for Dao (and let Federated/AiAssisted use the existing count).
**Fix**: Added `GovernancePolicy::is_weighted_quorum_met(weight_voted,
total_eligible_weight)` and `Proposal::resolve_weighted(policy,
total_eligible_weight)`. Existing `resolve()` now documents count-based
behavior for Federated/AiAssisted. 5 new tests.

### R5-5 (MEDIUM) — `mon/membership.rs::is_valid_role_combination` accepts unknown bits
**Issue**: Function only checks `count_ones() <= MAX_ROLES_PER_NODE`. A
node could submit `role_flags = 0x8000_0000_0000_0000` (one unknown bit
set) and pass the count check.
**Fix**: Added `KNOWN_ROLE_MASK` (OR of all 8 ROLE_* flags). Reject any
bit outside the mask. New test `test_unknown_role_bits_rejected`.

### R5-6 (LOW) — `dc/reputation.rs::gossip_topic` accepts empty dc_pubkey
**Issue**: Same as R5-1 but for DC reputation. Produces `"/dot/reputation/dc/"`.
**Fix**: assert + test `gossip_topic_rejects_empty`.

### R5-7 (MEDIUM) — `dc/sub_admin.rs::elect_active_sub_admin` integer overflow
**Issue**:
1. `*agg.entry(sa.clone()).or_insert(0) += w;` can overflow with hostile
   weight spam.
2. `distinct.len() * 3 < total_sub_admins * 2` can overflow with
   adversarial `total_sub_admins`.
**Fix**:
1. `saturating_add` for weight accumulation.
2. `saturating_mul` for the 2/3 check.

## Test Results
- octo-network: 1076 passed (up from 1067 at R4; +9 tests)
- Pre-existing dead-code warnings in test mocks unchanged

## Files Changed
- `crates/octo-network/src/mon/reputation.rs`
- `crates/octo-network/src/mon/governance.rs`
- `crates/octo-network/src/mon/execution.rs`
- `crates/octo-network/src/mon/membership.rs`
- `crates/octo-network/src/dc/reputation.rs`
- `crates/octo-network/src/dc/sub_admin.rs`
