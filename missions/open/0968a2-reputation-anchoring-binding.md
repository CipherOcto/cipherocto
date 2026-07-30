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

Mission 0968a's Round 4 review (REV-4 commit `b5cb0d1f`) identified
the 9 ungrounded ACs and 2 cross-RFC drifts (N8 `StakeBelowMinimum`,
N9 `ReputationAnchorBatch` governance fields). The N8 discriminant
drift was already resolved in commit `013a5676` (Round 2 of 0968a2
review). This mission 0968a2 now owns the remaining 9 ungrounded ACs
from 0968a (the 8 ungrounded ACs + the N9 governance fields drift
consolidated as one of the 9 inherited ACs) + 8 new fix items
discovered during rounds 1-5 of 0968a2 review (governance types,
`batch_size`, `chain_block_height` type, `AnchorLeaf::digest` field
order, rotation_receipt_id chain encoding, live adapter, v012
migration, test re-pinning). The 9 Scope items (reorg + DID-rotation
finality combined into 1 Scope item covering both) map cleanly to
17 ACs (9 inherited from 0968a + 8 new for 0968a2).

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

- RFC-0955: Model Liquidity Layer (parent RFC; `ComputeOffer.reputation:
  ReputationDigest` defined in §"Compute Assets" — the binding target
  for the anchoring)
- RFC-0955-R1: Reputation Anchoring Amendment (canonical binding
  contract; `ReputationAnchorBatch` struct + governance binding)
- RFC-0968: Reputation Registry (the persisted source-of-truth; §13
  error enum table)
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

The RFC-0968 §13 discriminant drift was already resolved in commit
`013a5676` (Round 2): RFC-0968 line 2057 + 2621 were updated to declare
`StakeBelowMinimum = 0x2D` with `{ component: StakeComponent }` payload,
matching the IMPL. The remaining `error.rs:8-49` guardrail
("Do NOT change discriminants until RFC-0968-A2 lands") still applies
to OTHER discriminant reshuffles — the comment cites 3 categories of
breakage (cross-replica error propagation, test fixtures, wire-format
stability) and a v011 migration + chained Rust type rename. RFC-0968-A2
remains a future-resolved amendment for any further discriminant
alignment but is NOT a precondition for this mission (the align-0x2D
work landed directly in RFC-0968 §13). A2 amendment remains optional
if governance prefers spec-first for any remaining cross-RFC drift.

## Scope

- [ ] **`StakeBelowMinimum` discriminant verification** — the discriminant
      drift that motivated this Scope item was already resolved in
      commit `013a5676` (Round 2): RFC-0968 §13 line 2057 + 2621 table
      now declare `StakeBelowMinimum = 0x2D` with payload `{ component: StakeComponent }`,
      matching the IMPL `crates/octo-reputation/src/error.rs:190`. The
      `0x17` slot is occupied by `GovernanceSlashFieldMismatch` at
      `error.rs:133` (which is correct per RFC-0968). This Scope item
      is now a **verification step** rather than a fix: confirm both
      `git diff` (RFC-0968 line 2057 + 2621 + 616) and `error.rs:190`
      agree on `0x2D` + `{ component: StakeComponent }` payload before
      claiming the work. The IMPL stays as-is per the `error.rs:22-28`
      guardrail ("Do NOT change discriminants until RFC-0968-A2 lands").
      Document the verification in the PR description.
- [ ] **`ReputationAnchorBatch` governance fields** — the IMPL
      `crates/octo-reputation/src/anchor.rs:121-137` has 5 fields
      (`controller_id`, `window`, `chain_block_height`,
      `rotation_receipt_id`, `leaves`). RFC-0955-R1 §"ReputationAnchorBatch"
      defines 14 fields. The drift is two-part: (a) the per-tuple fields
      (`did`, `signal_kind`, `layer`, `last_event_id`, `score_ewma_raw`,
      `last_event_unix`, `samples`, `severity_total`) moved into
      `AnchorLeaf` per RFC-0955-R1 amendment 48 (per-controller
      refactor — deliberate, NOT a drift to fix). The remaining
      RFC-mandated fields on `ReputationAnchorBatch` are also missing
      from the IMPL: (b) the 3 governance fields (`governance_snapshot`,
      `governance_proof`, `governance_set_hash`); (c) `batch_size: u32`
      (RFC-0955-R1 line 173); (d) `chain_block_height: Option<u64>`
      (IMPL currently has `chain_block_height: u64`; RFC-0955-R1 line
      170 declares `Option<u64>` because the field is `None` at
      submission and `Some(_)` only after the anchor reaches
      `MIN_FINALITY_BLOCKS` depth). The fix scope is items (b)+(c)+(d).
      Extend the IMPL struct with the 3 governance fields + `batch_size` + change `chain_block_height` to `Option<u64>`. Update
      `ReputationAnchorBatch::digest` (`anchor.rs:139-167`) to fold the
      governance fields into the domain-separated envelope hash. Add a new migration
      `v012__reputation_anchors_governance.sql` extending `reputation_anchors`
      with `governance_snapshot BLOB`, `governance_proof BLOB`,
      `governance_set_hash BLOB` columns (all nullable). Add unit tests for
      construction + digest stability with the new fields. Note: existing
      3 canonical test vectors in `tests/canonical_blobs.rs` (the
      `CANONICAL_ANCHOR_BLOB_{0,1,100}_LEAVES` pinned bytes at lines
      34, 41, 48) will need re-pinning once the digest covers
      governance fields. **Reconcile the verifier types** — RFC-0955-R1
§"ReputationAnchorBatch" (lines 177-200) defines three
      Rust types (`GovernanceSnapshot` with `block_height`, `epoch`,
      `finalized_at_unix`; `GovernanceSigner` with `pubkey: [u8; 32]`,
      `signature: [u8; 64]`; `GovernanceProof` with `signers: Vec<GovernanceSigner>`).
      The IMPL `crates/octo-reputation/src/auth.rs` ALREADY has
      `GovernanceSnapshot` (lines 21-25: `finalized_at_unix`,
      `governance_set_hash`, `members: Vec<[u8; 32]>`) and
      `GovernanceProof` (line 113+) but with INCOMPATIBLE shapes vs
      RFC-0955-R1. `GovernanceSigner` is absent. Following the prior
      mission text literally (create new types in a new auth.rs
      module) would collide with the existing names. Two reconciliation
      paths. **Only path (a) is viable** — path (b) in-place evolution
      is not viable because the existing `GovernanceProof` (line 113+) is
      a semantically distinct **slash/suspension authorization envelope**
      carrying `governance_pubkey`, `recorder_id`, `reason_hash`,
      slash destination/amount/asset fields required by RFC-0968
      authorization flows. Replacing it would remove data required by
      current RFC-0968 authorization flows; the existing
      `GovernanceSnapshot` (lines 21-25) is similarly tied to
      governance-membership semantics.

      **(a)** Add RFC-0955-R1 types under new names in the same module
      (e.g., `AnchorGovernanceSnapshot`, `AnchorGovernanceSigner`,
      `AnchorGovernanceProof`) — preserves existing auth.rs callers
      (RFC-0968 §3 retirement, SuspensionAuth, SlashDestination) and
      keeps the anchoring wire schema distinct. Wire them into the
      new `ReputationAnchorBatch` BLOB fields via serde
      deserialization. (Mandated path.)

                      **Also: fix `AnchorLeaf::digest` field-order bug in IMPL** (per
                      `crates/octo-reputation/src/anchor.rs:80-100`) — the IMPL hashes
                      `last_event_unix`, `samples`, `severity_total`, then
                      `score_ewma_raw`. RFC-0955-R1 line 420-422 requires the canonical
                      order `(did, signal_kind, layer, last_event_id, score_ewma_raw,

last_event_unix, samples, severity_total)`— i.e.,`score_ewma_raw`      at position 5 (after`last_event_id`, before the counters). The
      current IMPL puts `score_ewma_raw`last. This breaks
      cross-implementation digest interoperability (per RFC-0955-R1
      line 422: "An independent Python implementation using the
     `hashlib.blake3`library MUST reproduce the same expected bytes").
      The 3 pinned test vectors in`tests/canonical_blobs.rs` would
NOT match any RFC-compliant independent reimplementation; the
bug fix requires re-pinning the 3 vectors to the correct order.

- [ ] **Live `ChainAnchorSubmitter` impl for the on-chain ledger** —
      implement a real `ChainAnchorSubmitter` (alongside the existing
      `StubChainAnchorSubmitter` at `anchor_job.rs:139`) that takes a
      `ReputationAnchorBatch` and submits the on-chain merkle-root
      transaction. **Explicitly covers 0968a AC #5 (chain-side encoding
      of `rotation_receipt_id`):** the live submitter must write the
      `ReputationAnchorBatch.rotation_receipt_id` field through to the
      v010 ledger's `rotation_receipt_id` column (per
      `v010__reputation_anchors.sql` line 62). Wire it into
      `anchor_job.rs:run_once_strict` (line 338) as the default submitter
      for production deployments. The stub remains the test/CI default.
      The target chain substrate is pending a separate chain-substrate
      selection RFC (see ## Dependencies).
- [ ] **Reorg-aware resubmission** — `plan_batches`
      (`anchor_job.rs:172`) is a pure function (no chain-state
      arguments). Wire a reorg-aware wrapper around it: either extend
      `run_once_strict` (line 338) to call `is_finality_reached`
      (`anchor.rs:206-208`) BEFORE calling `plan_batches`, OR introduce
      a new `plan_batches_with_reorg_check` that takes chain-state
      parameters. On reorg > `MIN_FINALITY_BLOCKS` (`constants.rs:42`),
      re-submit the affected `(controller_id, anchor_root)` pair. Add
      tests for the reorg-handler path. Per RFC-0955-R1 §"Finality"
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
      same `(did, signal_kind, layer, last_event_id)` 4-tuple twice (which
      uniquely determines `event_id` per RFC-0955-R1 §"Chain-Level
      Idempotency") and asserts the second submission is a no-op. Relies
      on the `UNIQUE` constraint on `reputation_anchors(event_id)` (defined
      in `v010__reputation_anchors.sql` line 24) and the composite-PK
      scope on `reputation_events(recorder_did, event_id)` enforced at
      `stoolap.rs:1718-1719` (WHERE clause with composite-PK scope
      `(recorder_did, event_id)`). The `anchor_tx_hash` claim in the earlier
      draft was wrong — idempotency is on `event_id`, not
      `anchor_tx_hash`.
      draft was wrong — idempotency is on `event_id`, not `anchor_tx_hash`.
- [ ] **Failure isolation test** — add a test that injects a
      `ChainAnchorSubmitter` failure mid-batch and asserts that
      `reputation_events` rows are not mutated (the `anchor_tx_hash`
      column should remain `NULL` for un-anchored events).
- [ ] **Gossip cross-reference** — wire `crates/octo-network/src/gossip/reputation.rs`
      to read `SignalEvent::anchor_tx_hash` and reject gossiped events
      whose `anchor_tx_hash` is `None` AND the event is older than
      `DEFAULT_ANCHOR_INTERVAL_SECS` (i.e., the event should have been
      anchored by now). **Target the ingress handler only** (the
      `handle_one` / `validate_envelope` call site); the 7 test
      fixtures at lines 813, 1056, 1206, 1288, 1336, 1389, 1526
      intentionally use `anchor_tx_hash: None` as test default and
      should remain unchanged. The gossip file is owned by mission
      0855p-b (now archived). Coordinate with the 0855p-b successor;
      if no successor, file a new mission 0968a3-gossip-anchor-provenance.

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

- [ ] `StakeBelowMinimum` discriminant verification: confirmation that RFC-0968 §13 line 2057 + 2621 + 616 + IMPL `error.rs:190` all agree on `0x2D` + `{ component: StakeComponent }` payload
- [ ] `ReputationAnchorBatch` has the 3 governance fields with `digest()` covering them
- [ ] `ReputationAnchorBatch` has `batch_size: u32` field per RFC-0955-R1 line 173
- [ ] `ReputationAnchorBatch.chain_block_height` typed `Option<u64>` per RFC-0955-R1 line 170 (currently IMPL `u64`)
- [ ] `AnchorLeaf::digest` field order matches RFC-0955-R1 line 420-422 (score_ewma_raw at position 5, not last)
- [ ] v012 migration adds `governance_snapshot`, `governance_proof`, `governance_set_hash` columns to `reputation_anchors`
- [ ] Live `ChainAnchorSubmitter` impl exists; `run_once_strict` uses it for production (gated on chain-substrate selection RFC)
- [ ] Live `ChainAnchorSubmitter` writes `rotation_receipt_id` through to v010 ledger (covers 0968a AC #5 chain-side encoding)
- [ ] Reorg handler re-submits batches whose `(submitted, finalized)` height delta exceeds `MIN_FINALITY_BLOCKS`
- [ ] DID-rotation finality handler re-submits anchors when `consume_rotation_receipt` is finalized before `MIN_FINALITY_BLOCKS`
- [ ] Governance signature verification rejects batches with != `GOVERNANCE_QUORUM` (= 3) signatures
- [ ] Anchor-specific verifier types (`AnchorGovernanceSnapshot` / `AnchorGovernanceSigner` / `AnchorGovernanceProof`) defined per RFC-0955-R1 lines 177-200 — mandated path (a), with existing `GovernanceSnapshot` / `GovernanceProof` (RFC-0968 authorization envelopes) preserved unchanged at `auth.rs:21-25` and `auth.rs:113+`
- [ ] Per-deployment config layer exposes `interval_secs` + `controller_id` + `chain_endpoint`
- [ ] Idempotency test (2 duplicate submits on `(did, signal_kind, layer, last_event_id)` 4-tuple) passes
- [ ] Failure isolation test (submitter mid-batch fail) passes
- [ ] Gossip consumer rejects stale `anchor_tx_hash: None` events at ingress handler only (7 test fixtures remain unchanged; requires 0855p-b successor)
- [ ] 3 canonical test vectors in `tests/canonical_blobs.rs` (lines 34, 41, 48) re-pinned to new digest

## AC → Scope mapping

| AC                                                                                    | Scope item(s) |
| ------------------------------------------------------------------------------------- | ------------- |
| `StakeBelowMinimum` 0x2D verification (impl + RFC agreed)                             | 1             |
| `ReputationAnchorBatch` governance fields                                             | 2             |
| `ReputationAnchorBatch` `batch_size: u32` field                                       | 2             |
| `ReputationAnchorBatch.chain_block_height: Option<u64>` type                          | 2             |
| `AnchorLeaf::digest` field order per RFC-0955-R1 line 420-422                         | 2             |
| `AnchorGovernanceSnapshot` / `AnchorGovernanceSigner` / `AnchorGovernanceProof` types | 2             |
| v012 migration                                                                        | 2             |
| Live `ChainAnchorSubmitter` impl                                                      | 3             |
| Live `ChainAnchorSubmitter` writes `rotation_receipt_id`                              | 3             |
| Reorg handler                                                                         | 4             |
| DID-rotation finality handler                                                         | 4             |
| Governance signature verification                                                     | 5             |
| Per-deployment config plumbing                                                        | 6             |
| Idempotency test                                                                      | 7             |
| Failure isolation test                                                                | 8             |
| Gossip cross-reference                                                                | 9             |
| Test vector re-pinning                                                                | 2             |

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
