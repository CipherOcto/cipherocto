# Mission 0968a2: Reputation Anchoring — Chain Binding

## Status

**Open 2026-07-30.** Follow-up to mission 0968a (still in `claimed/`
state at `missions/claimed/0968a-reputation-anchoring.md` with 9
ungrounded ACs documented per Round 3 review). Mission 0968a covered
the in-memory batch envelope + Merkle-root aggregation + ledger table
scaffolding; this mission 0968a2 covers the LIVE chain-side binding

- governance/quorum wiring + reorg-aware resubmission + the IMPL/RFC
  cross-document drift identified in mission 0968a's Round 4 review N8
- N9.

**Note**: 0968a is NOT closed via Path B. Earlier drafts of this
mission referenced `missions/archived/0968a-...`; that path never
existed. 0968a remains in `claimed/` pending user-initiated Path B
closure or active work on the 9 ungrounded ACs.

## Why split

Mission 0968a's Round 4 review (REV-3 commit `b5cb0d1f`) identified
the 9 ungrounded ACs and 2 cross-RFC drifts (N8 `StakeBelowMinimum`,
N9 `ReputationAnchorBatch` governance fields). This mission 0968a2
owns the cross-RFC drift reconciliation + the live chain-side
adapter + the remaining 6 ungrounded ACs. The 10 ungrounded ACs
chunk naturally into 9 Scope items:

1. `StakeBelowMinimum` IMPL/RFC discriminant delta
2. `ReputationAnchorBatch` governance fields
3. Live `ChainAnchorSubmitter` impl for the on-chain ledger
4. Reorg-aware resubmission
5. Governance quorum signature verification
6. Per-deployment config plumbing
7. Idempotency behavior test
8. Failure isolation test
9. Gossip cross-reference

## RFC

- RFC-0955-R1: Reputation Anchoring Amendment (canonical binding
  contract; `ReputationAnchorBatch` struct + governance binding)
- RFC-0968: Reputation Registry (the persisted source-of-truth;
  §"Compute Assets" + §13 error enum table)
- RFC-0927: RouterConfig Extension (consumer-relevant for the per-deployment
  config plumbing in Scope item 6; RFC-0927 is about `RouterConfig`,
  not reputation — the actual reputation config block is a separate
  consideration)

## Dependencies

**Hard**: Mission 0968a (must land for the live chain binding to
have a substrate to bind to — items 1, 2, 7, 8 build on 0968a's
envelope + Merkle-root aggregation + ledger migration).

**Hard**: A chain-substrate selection RFC. The Scope item 3 ("live
`ChainAnchorSubmitter` impl") needs a target chain (CipherOcto
app-chain vs reused Cosmos/Substrate/etc.). The chain substrate is
NOT specified in RFC-0955 or RFC-0955-R1 — only the binding target
is (`ComputeOffer.reputation: ReputationDigest`). Resolve before
implementing Scope item 3.

**Soft**: Mission 0855p-b (now archived 2026-07-27, see
`missions/archived/0855p-b-cross-mission-reputation.md`). Scope
item 9 modifies `crates/octo-network/src/gossip/reputation.rs`,
which is owned by 0855p-b. The 0855p-b mission is closed but the
file is the canonical substrate for slash gossip. Coordinate with
the 0855p-b successor (or create a new mission 0968a3-gossip-anchor-provenance
if the cross-reference requires dedicated scope).

## Why not RFC-0968-A2 amendment

The drift is real but filing a new RFC amendment is heavier than the
fix scope. RFC-0968-A2 has been referenced in past mission files
(`missions/archived/0968-b-marketplace-integration.md` lines 33, 115)
but never materialized as a draft file. The cleanest path is: fix in
impl (move `StakeBelowMinimum` to 0x17 per RFC-0968; extend
`ReputationAnchorBatch` per RFC-0955-R1) + document the field-level
reconciliation in the PR description. A2 amendment remains optional
if governance prefers spec-first.

## Scope

- [ ] **`StakeBelowMinimum` IMPL/RFC delta** — change
      `crates/octo-reputation/src/error.rs:190` from `0x2D` to `0x17` to
      match RFC-0968 §13. Update `error.rs:359-364` test mapping. Decide
      payload field harmonization: either IMPL `{ component: StakeComponent }`
      (line 190) → RFC `{ provided: u64 }` (RFC-0968 line 2057), OR
      update RFC-0968 §13 to match the IMPL payload. Pick one and document
      the decision in the PR description. Consumers using `serde` over
      the error enum will silently misdeserialize during the swap; a
      coordinated release is required.
- [ ] **`ReputationAnchorBatch` governance fields** — the IMPL
      `crates/octo-reputation/src/anchor.rs:121-137` has 5 fields
      (`controller_id`, `window`, `chain_block_height`,
      `rotation_receipt_id`, `leaves`). RFC-0955-R1 §"ReputationAnchorBatch"
      defines 14 fields. The drift is two-part: (a) the per-tuple fields
      (`did`, `signal_kind`, `layer`, `last_event_id`, `score_ewma_raw`,
      `last_event_unix`, `samples`, `severity_total`, `batch_size`) moved
      into `AnchorLeaf` per RFC-0955-R1 amendment 48 (per-controller
      refactor — deliberate, NOT a drift to fix); (b) the 3 governance
      fields (`governance_snapshot`, `governance_proof`,
      `governance_set_hash`) are MISSING from the IMPL `ReputationAnchorBatch`
      struct. Extend the IMPL struct with the 3 governance fields. Update
      `ReputationAnchorBatch::digest` (`anchor.rs:139-167`) to fold the
      governance fields into the domain-separated envelope hash. Add unit
      tests for construction + digest stability with the new fields. Note:
      existing 5 canonical test vectors in `tests/canonical_blobs.rs` will
      need re-pinning once the digest covers governance fields.
- [ ] **Live `ChainAnchorSubmitter` impl for the on-chain ledger** —
      implement a real `ChainAnchorSubmitter` (alongside the existing
      `StubChainAnchorSubmitter` at `anchor_job.rs:139`) that takes a
      `ReputationAnchorBatch` and submits the on-chain merkle-root
      transaction. Wire it into `anchor_job.rs:run_once_strict` (line 338)
      as the default submitter for production deployments. The stub
      remains the test/CI default. The target chain substrate is pending
      a separate chain-substrate selection RFC (see ## Dependencies).
- [ ] **Reorg-aware resubmission** — wire `plan_batches`
      (`anchor_job.rs:172`) to consult `is_finality_reached`
      (`anchor.rs:206-208`) for reorg detection. On reorg > `MIN_FINALITY_BLOCKS`
      (`constants.rs:42`), re-submit the affected `(controller_id, anchor_root)`
      pair. Add tests for the reorg-handler path. Per RFC-0955-R1 §"Finality"
      (lines 227-248), this also covers DID-rotation finality: if a
      `consume_rotation_receipt` for the anchor's `did` is finalized in
      the chain BEFORE the anchor's `MIN_FINALITY_BLOCKS`, re-submit the
      anchor for `new_did` with the post-decay `score_ewma` (the `0.9`
      decay factor per RFC-0968 §2.1 step 3).
- [ ] **Governance quorum signature verification** — verify that
      every anchor tx carries `GOVERNANCE_QUORUM = 3` 3-of-3 signatures
      over `governance_set_hash` (per RFC-0955-R1 §"Governance Snapshot
      Binding" lines 250-266). Add the verification as a pre-flight in
      `anchor_job.rs::run_once_strict` (line 338). Reject on signatures
      count != `GOVERNANCE_QUORUM` or any signature failing verification.
- [ ] **Per-deployment config plumbing** — surface
      `DEFAULT_ANCHOR_INTERVAL_SECS` (`constants.rs:53`) +
      `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL` (`constants.rs:57`)
      via the per-deployment config layer. RFC-0927 is about
      `RouterConfig` (LiteLLM compatibility), not reputation; the
      reputation config block is a separate concern. The exact config
      crate path is TBD (resolution tied to the chain-substrate
      selection RFC). Add the spec for `interval_secs`, `controller_id`,
      `chain_endpoint` once the config crate is identified.
- [ ] **Idempotency behavior test** — add a test that submits the
      same `EventId` twice and asserts the second submission is a no-op
      (relies on the `UNIQUE` constraint on `reputation_anchors(event_id)`
      and the composite-PK scope on `reputation_events(recorder_did, event_id)`
      at `stoolap.rs:1714-1717`). The `anchor_tx_hash` claim in the
      earlier draft was wrong — idempotency is on `event_id`, not
      `anchor_tx_hash`.
- [ ] **Failure isolation test** — add a test that injects a
      `ChainAnchorSubmitter` failure mid-batch and asserts that
      `reputation_events` rows are not mutated (the `anchor_tx_hash`
      column should remain `NULL` for un-anchored events).
- [ ] **Gossip cross-reference** — wire `crates/octo-network/src/gossip/reputation.rs`
      to read `SignalEvent::anchor_tx_hash` and reject gossiped events
      whose `anchor_tx_hash` is `None` AND the event is older than
      `DEFAULT_ANCHOR_INTERVAL_SECS` (i.e., the event should have been
      anchored by now). The gossip file is owned by mission 0855p-b
      (now archived). Coordinate with the 0855p-b successor; if no
      successor, file a new mission 0968a3-gossip-anchor-provenance.

## Out of scope

- Anything already covered by mission 0968a (batch envelope, Merkle-root
  aggregation, v010 ledger table, schema migration). See
  `missions/claimed/0968a-reputation-anchoring.md` for the canonical
  record.
- RFC-0968-A2 amendment process. If governance prefers spec-first
  amendment, the impl change for `StakeBelowMinimum` (item 1) needs
  to wait for the A2 filing.
- Chain-substrate selection RFC. Required as a precondition for
  Scope item 3; not owned by this mission.

## Acceptance Criteria

- [ ] `StakeBelowMinimum` discriminant byte equals 0x17 (impl + RFC agreed)
- [ ] `ReputationAnchorBatch` has the 3 governance fields with `digest()` covering them
- [ ] Live `ChainAnchorSubmitter` impl exists; `run_once_strict` uses it for production
- [ ] Reorg handler re-submits batches whose `(submitted, finalized)` height delta exceeds `MIN_FINALITY_BLOCKS`
- [ ] DID-rotation finality handler re-submits anchors when `consume_rotation_receipt` is finalized before `MIN_FINALITY_BLOCKS`
- [ ] Governance signature verification rejects batches with != `GOVERNANCE_QUORUM` (= 3) signatures
- [ ] Per-deployment config layer exposes `interval_secs` + `controller_id` + `chain_endpoint`
- [ ] Idempotency test (2 duplicate submits on `event_id`) passes
- [ ] Failure isolation test (submitter mid-batch fail) passes
- [ ] Gossip consumer rejects stale `anchor_tx_hash: None` events (requires 0855p-b successor)

## Complexity

High. Live chain integration + governance wiring + IMPL/RFC
reconciliation + cross-mission coordination with 0855p-b gossip

- chain-substrate selection dependency. Comparable to 0968a scope
  but with the additional complication of IMPL/RFC byte-level
  reconciliation + cross-mission ownership of the gossip file.

## Claimant

(unassigned)

## Pull Request

# (TBD — pending chain-substrate selection RFC + 0855p-b successor)
