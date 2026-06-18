# R15 R4 Adversarial Review

**Date:** 2026-06-17
**Reviewer:** Self (autonomous)
**Scope:** Uncovered areas: dc/discipline, dc/reputation, dc/sub_admin,
dc/admin_attest, mon/slash_aggregation, mon/rebind, mon/nostr_bootstrap,
mon/mission_id, mon/vdf, mon/error

**Prior rounds:** R1 (8), R2 (5), R3 (6). All fixed.

## Issues Found

### R4-1 (HIGH): DisciplineAction::Slash with ForcedUnbind is
  semantically wrong
**File:** `crates/octo-network/src/dc/discipline.rs`
**Problem:** For a 3rd-strike in a small group, the function
returned `DisciplineAction::Slash { cooldown_epochs: 8,
state: SuspectState::ForcedUnbind }`. The action was `Slash` but
its `state` flagged "forced UNBIND", with a misleading cool-down
of 8 that no caller should use. The dead `SuspectState::ForcedUnbind`
variant enabled the confusion.

**Fix:** Return `DisciplineAction::Unbind` directly. Removed
`SuspectState::ForcedUnbind` variant. Updated test
`small_group_third_offender_forced_unbind`; added
`small_group_fourth_offender_also_unbind`.

### R4-2 (MEDIUM): Quorum::is_met(0, 0) returns true
**File:** `crates/octo-network/src/dc/consensus.rs`
**Problem:** `Quorum::Unilateral::is_met(0, 0)` returns true
(0 == 0 is trivially met). N=0 callers can therefore claim
"quorum met" with 0 votes. The `record_vote` and `check_deadline`
callers do guard against N=0, but defending in depth in
`is_met` prevents accidental misuse if a new caller is added.

**Fix:** Added `if n == 0 { return false; }` at the top of
`is_met`. Test `n0_is_never_met`.

### R4-3 (MEDIUM): SlashAggregator::aggregate() does not guard N=0
**File:** `crates/octo-network/src/mon/slash_aggregation.rs`
**Problem:** `is_finalized` guards against `total_witnesses == 0`
but `aggregate` does not. With total=0, the condition
`yes*3 >= total*2` is `0 >= 0 = true`, so `aggregate()` would
return `FinalizedYes { yes: 0, no: 0, total: 0 }` — a slash
finalized with zero witnesses and zero votes.

**Fix:** Added N=0 guard to `aggregate()`. Test `n0_aggregator_rejects`.

### R4-4 (MEDIUM): RebindCoordinator with empty participants
**File:** `crates/octo-network/src/mon/rebind.rs`
**Problem:** `record_vote` with an empty `participants` list
would, on first vote, satisfy `responses.len() == participants.len()`
(both 0) and `all()` on empty returns true, transitioning to
`Committing` without any 2PC coordination. The `check_deadline`
function already guards against this case; `record_vote` did not.

**Fix:** Added `if self.participants.is_empty() { return
self.abort(RebindAbortReason::VoteAbort); }` to `record_vote`.
Test `zero_participants_aborts`.

### R4-5 (MEDIUM): VdfEvaluation::simulate with iterations=0 is
  internally inconsistent
**File:** `crates/octo-network/src/mon/vdf.rs`
**Problem:** With iterations=0, the loops don't execute, so
output = proof = H(seed). `verify` checks `H(proof) == output`
which is `H(H(seed)) == H(seed)` — false. The simulation is
inconsistent for the degenerate case.

**Fix:** Added an explicit early return for iterations=0:
output = H(seed), proof = seed (so H(proof) = H(seed) = output).
Test `vdf_simulate_zero_iterations`.

### R4-6 (MEDIUM): now_epoch() is a misleading name
**File:** `crates/octo-network/src/dc/admin_attest.rs`,
`crates/octo-network/src/dc/rejoin.rs`, `crates/octo-network/src/mon/vdf.rs`
**Problem:** `now_epoch()` returns Unix seconds (not a network
consensus epoch). The name is misleading and could cause
unit-mismatch bugs if a caller uses it for `signed_at_epoch`
in a freshness check that expects a consensus-epoch value.

**Fix:** Renamed to `now_unix_seconds()`. Kept `now_epoch` as a
`#[deprecated]` alias to avoid breaking callers.

### R4-7 (MEDIUM): Nip05Identifier::parse accepts path-traversal
  in user/domain
**File:** `crates/octo-network/src/mon/nostr_bootstrap.rs`
**Problem:** The parser accepted any non-empty `user@domain`,
including `user@../etc/passwd` (path-traversal in domain) and
`../etc@domain` (path-traversal in user). The `resolution_url()`
would produce a URL with a path-traversed domain, which a
logging layer could log or a careless HTTP client could request.

**Fix:** Added explicit checks for `/`, `\`, whitespace, NUL,
tab, newline in both parts. Also reject user > 64 chars.
Tests `nip05_identifier_rejects_path_traversal` and
`nip05_identifier_rejects_oversize_user`.

### R4-8 (MEDIUM): MissionId::from_canonical_bytes returns a
  placeholder error
**File:** `crates/octo-network/src/mon/mission_id.rs`,
`crates/octo-network/src/mon/error.rs`
**Problem:** On size mismatch, the function returned
`InvalidMissionId { mission_hash: [0u8; 32] }` with a
placeholder. The error message said "Invalid mission id" with
all-zero bytes — unhelpful for debugging.

**Fix:** Added new variant `InvalidMissionIdBytes { expected,
actual }` and use it for the size-mismatch case. Test updated
to assert the new variant.

### R4-9 (LOW): SlashAggregator::add_vote accepts empty witness
**File:** `crates/octo-network/src/mon/slash_aggregation.rs`
**Problem:** `add_vote` accepted a vote with `witness = ""`,
which (combined with the dedup-by-witness behavior) would let
an attacker contribute one anonymous vote per slash event.

**Fix:** Reject empty witness in `add_vote`. Test
`empty_witness_vote_rejected`.

## Test Counts

| Crate                                | R3   | After R4 |
|--------------------------------------|------|----------|
| octo-network                         | 1059 | 1067     |
| octo-whatsapp-onboard-core           | 41   | 41       |
| octo-whatsapp-onboard                | 21   | 21       |

R4 added 8 tests: 2 in dc::discipline, 1 in dc::consensus, 2 in
mon::slash_aggregation, 1 in mon::rebind, 2 in mon::nostr_bootstrap,
1 in mon::vdf. (MissionId test was an update, not a new test.)

## Summary

9 issues found, 9 fixed. 1 HIGH (discipline dead variant /
semantic mismatch), 6 MEDIUM, 2 LOW. No new Critical issues.
