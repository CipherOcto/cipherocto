# Mission 0968a2: Reputation Anchoring — Chain Binding

## Status

**Open 2026-07-30.** Follow-up to mission 0968a (closed via Path B,
2026-07-30, see `missions/archived/0968a-reputation-anchoring.md`).
Mission 0968a covered the in-memory batch envelope + Merkle-root
aggregation + ledger table scaffolding; this mission 0968a2 covers the
LIVE chain-side binding + governance/quorum wiring + reorg-aware
resubmission + the IMPL/RFC cross-document drift identified in
mission 0968a's Round 4 review N8 + N9.

## Why split

Mission 0968a closed via Path B with 9 ungrounded ACs (see audit
banner in archived mission). Of those 9, 3 are blocked by cross-RFC
drifts that are out of scope for a per-mission fix:

1. `StakeBelowMinimum` discriminant: IMPL = 0x2D
   (`crates/octo-reputation/src/error.rs:190`), RFC-0968 §13 = 0x17
   (RFC-0968 lines 2057 + 2621 table). Two definitions are NOT
   binary-compatible.
2. `ReputationAnchorBatch` governance fields: RFC-0955-R1 §"ReputationAnchorBatch"
   (lines 148-198) defines `governance_snapshot`, `governance_proof`,
   `governance_set_hash`; IMPL `anchor.rs:121-137` has split into
   `AnchorLeaf` + `ReputationAnchorBatch` and the governance binding
   is not wired.
3. Chain-side adapter: only `StubChainAnchorSubmitter` exists
   (`anchor_job.rs:139`). No real `ChainAnchorSubmitter` impl for
   the on-chain ledger.

The IMPL/RFC drifts are also relevant to mission 0968b (now archived
as Path B closure), which left a "pending RFC-0968-A2 realignment"
note for the error-enum byte codes.

## Why not RFC-0968-A2 amendment

The drift is real but filing a new RFC amendment is heavier than the
fix scope. RFC-0968-A2 has been referenced in past mission files (`missions/archived/0968-b-marketplace-integration.md`
lines 33, 115) but never materialized as a draft file. The cleanest
path is: fix in impl (move `StakeBelowMinimum` to 0x17 per RFC-0968;
extend `ReputationAnchorBatch` per RFC-0955-R1) + add a compat
shim for in-flight `0x2D` discriminants with a deprecation window.
A2 amendment remains optional if governance prefers spec-first.

## Scope

- [ ] **`StakeBelowMinimum` IMPL/RFC delta** — change
  `crates/octo-reputation/src/error.rs:190` from `0x2D` to `0x17` to
  match RFC-0968 §13. Add compat shim for any in-flight `0x2D`
  discriminants (deprecation window: 2 minor versions, per RFC-0968
  cross-cutting rule). Update `error.rs:362-363` test mapping + the
  payload field from `{ component: StakeComponent }` to `{ provided: u64 }`
  (or update RFC-0968 §13 to match impl — pick one and document).
- [ ] **`ReputationAnchorBatch` governance fields** — extend
  `crates/octo-reputation/src/anchor.rs:121-137` with
  `governance_snapshot: GovernanceSnapshot`,
  `governance_proof: GovernanceProof`,
  `governance_set_hash: [u8; 32]` per RFC-0955-R1 §"ReputationAnchorBatch"
  (lines 148-198). Add unit tests for construction + digest stability
  with the new fields. Update `ReputationAnchorBatch::digest` to fold
  the governance fields into the domain-separated envelope hash.
- [ ] **Live `ChainAnchorSubmitter` impl for the on-chain ledger** —
  implement a real `ChainAnchorSubmitter` (alongside the existing
  `StubChainAnchorSubmitter` at `anchor_job.rs:139`) that takes a
  `ReputationAnchorBatch` and submits the on-chain merkle-root
  transaction. Wire it into `anchor_job.rs:run_once_strict` (line 338)
  as the default submitter for production deployments. The stub
  remains the test/CI default.
- [ ] **Reorg-aware resubmission** — wire `plan_batches`
  (`anchor_job.rs:172`) to consult `is_finality_reached`
  (`anchor.rs:206-208`) for reorg detection. On reorg > `MIN_FINALITY_BLOCKS`
  (constants.rs:42), re-submit the affected `(controller_id, anchor_root)`
  pair. Add tests for the reorg-handler path.
- [ ] **Governance quorum signature verification** — verify that
  every anchor tx carries `GOVERNANCE_QUORUM = 3` distinct signatures
  over `governance_set_hash`. Add the verification to the
  `ChainAnchorSubmitter` signature check (or as a separate pre-flight
  in `anchor_job.rs::run_once_strict`).
- [ ] **Per-deployment config plumbing** — surface `DEFAULT_ANCHOR_INTERVAL_SECS`
  (constants.rs:30 area) + `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL`
  (constants.rs:57) via the per-deployment config layer (RFC-0927
  RouterConfig extension). Add a `peers_reputation::anchor` config
  block with `interval_secs`, `controller_id`, `chain_endpoint`, etc.
- [ ] **Idempotency behavior test** — add a test that submits the
  same `(EventId, anchor_tx_hash)` pair twice and asserts the second
  submission is a no-op (relies on the `UNIQUE` constraint on
  `reputation_anchors(anchor_tx_hash)` and the composite-PK scope
  in `stoolap.rs:1714-1717`).
- [ ] **Failure isolation test** — add a test that injects a
  `ChainAnchorSubmitter` failure mid-batch and asserts that
  `reputation_events` rows are not mutated (the `anchor_tx_hash`
  column should remain `NULL` for un-anchored events).
- [ ] **Gossip cross-reference** — wire `crates/octo-network/src/gossip/reputation.rs`
  to read `SignalEvent::anchor_tx_hash` and reject gossiped events
  whose `anchor_tx_hash` is `None` AND the event is older than
  `ANCHOR_INTERVAL_SECS` (i.e., the event should have been anchored
  by now). Coordinate with mission 0855p-b gossip consumer.

## Out of scope

- Anything already covered by mission 0968a (batch envelope, Merkle-root
  aggregation, v010 ledger table, schema migration). See
  `missions/archived/0968a-reputation-anchoring.md` for the full
  pre-archive record.
- RFC-0968-A2 amendment process. If governance prefers spec-first
  amendment, the impl change for `StakeBelowMinimum` (item 1) needs
  to wait for the A2 filing.

## Acceptance Criteria

- [ ] `StakeBelowMinimum` discriminant byte equals 0x17 (impl + RFC agreed)
- [ ] `ReputationAnchorBatch` has the 3 governance fields with `digest()` covering them
- [ ] Live `ChainAnchorSubmitter` impl exists; `run_once_strict` uses it for production
- [ ] Reorg handler re-submits batches whose `(submitted, finalized)` height delta exceeds `MIN_FINALITY_BLOCKS`
- [ ] Governance signature verification rejects batches with < `GOVERNANCE_QUORUM` signatures
- [ ] Per-deployment config layer exposes `interval_secs` + `controller_id` + `chain_endpoint`
- [ ] Idempotency test (2 duplicate submits) passes
- [ ] Failure isolation test (submitter mid-batch fail) passes
- [ ] Gossip consumer rejects stale `anchor_tx_hash: None` events
- [ ] parent mission 0968a Path B banner referenced in PR description

## Complexity

High. Live chain integration + governance wiring + IMPL/RFC reconciliation
+ cross-mission coordination with 0855p-b gossip.

## Claimant

(unassigned)

## Pull Request

# (TBD)
