# Mission 0968a: Reputation On-Chain Anchoring

## Status

**Claimed 2026-07-27.** RFC-0955-R1 accepted 2026-07-27 (sibling of
RFC-0955, both promoted to Accepted this session). RFC-0968 accepted
2026-07-25. Both blockers cleared. **Status refreshed 2026-07-30**
(Round 3 review, after R13/R14/R15 commits `1b528ef3`,
`1e042356`, `e42c67d5`): per the per-AC audit, **10 of 19 AC items
(Scope + Acceptance Criteria combined) are landed in code** — see
the `[x]` items below for file:line citations.

**Status refreshed 2026-07-30 (round 16)**: mission **0968a2**
(`missions/claimed/0968a2-reputation-anchoring-binding.md`) landed
in commit `72bf19d7`. 0968a2 closed the N9 governance fields drift
(RFC-0955-R1 §"ReputationAnchorBatch") by extending
`ReputationAnchorBatch` with `governance_snapshot`,
`governance_proof`, `governance_set_hash`, `batch_size: u32` fields,
changing `chain_block_height: u64` → `Option<u64>` (None at
submission, Some(h) post-finality per RFC-0955-R1 line 170), and
fixing the `AnchorLeaf::digest` field order to put
`score_ewma_raw` at position 5 per RFC-0955-R1 lines 420-422.
v012 migration added 3 governance BLOB columns to
`reputation_anchors`. 3 canonical test vectors re-pinned in
`tests/canonical_blobs.rs`. AC L146
("Governance-set hash + 3 distinct signatures in every anchor tx")
MOSTLY grounds at the struct level; the runtime verification gate
(`governance_proof.meets_quorum()`) lands with 0968a2 but the
in-anchor-job pre-flight check (calling `meets_quorum()` before
`ChainAnchorSubmitter::submit`) is ungrounded — still awaits
chain-substrate selection.

The 9 ungrounded `[ ]` AC items remain:

1. Live background job submission path (chain-side adapter not wired)
2. Idempotency AC (no behavior test against the live job)
3. Failure isolation AC (no failure-mode tests referencing `reputation_events` mutation)
4. Per-deployment config plumbing (constants declared, config layer ungrounded)
5. `rotation_receipt_id` chain encoding (in-memory struct + digest
   folding landed via 0968a2; chain encoding ungrounded — needs
   live `ChainAnchorSubmitter` wiring, gated on AC #1)
6. DID-rotation finality handler integration (`MIN_FINALITY_BLOCKS` +
   `is_finality_reached` declared, not integrated into `plan_batches`)
7. Governance quorum signatures in anchor tx (struct fields +
   `meets_quorum()` landed via 0968a2; runtime pre-flight check
   in `anchor_job.rs::run_once_strict` ungrounded — needs AC #1)
8. Reorg re-submission on reorg > `MIN_FINALITY_BLOCKS` (no handler)
9. Gossip cross-reference (mission 0855p-b does not verify
   `anchor_tx_hash` provenance)

Of the 9, **5 (#1, #4, #5, #6, #7, #8) are gated on the
chain-substrate selection RFC** (a separate deliverable not owned
by this mission or 0968a2). **#9 is gated on a successor to
archived mission 0855p-b** (the gossip file
`crates/octo-network/src/gossip/reputation.rs` is owned by that
mission family; if no successor is filed, a new mission
0968a3-gossip-anchor-provenance would be required). Only **#2 and
#3 (the idempotency + failure isolation behavior tests)** are
achievable here without external blockers — they were deferred
because they need a live `ChainAnchorSubmitter` to test against.

**Path forward**: user-initiated Path B closure recommended (per
BLUEPRINT §1152-1158). The 9 ungrounded ACs split cleanly into
3 categories with explicit external ownership, all of which are
out-of-scope for 0968a's commit boundary. The mission is
substantively complete; the residual work is a separate
chain-substrate + gossip coordination effort.

The `anchor_job.rs` scheduler scaffold, the `v010__reputation_anchors.sql`
migration, the `v011__reputation_events_anchor.sql` migration, and
the new `v012__reputation_anchors_governance.sql` migration are
all committed (v012 added 2026-07-30 in commit `72bf19d7`).
Migration summary:

- `v010` — creates `reputation_anchors` table (the per-controller
  Merkle-root ledger). Dormant until a future amendment wires
  `set_event_anchor_tx_hash` to INSERT into this ledger.
- `v011` — adds `anchor_tx_hash` column to
  `reputation_events` (the column the anchor job operates on; the
  AC-required `WHERE anchor_tx_hash IS NULL` sweep depends on this).
  Schema: `ALTER TABLE reputation_events ADD COLUMN anchor_tx_hash BLOB;`
  - `idx_reputation_events_controller_anchor` on `(controller_id, anchor_tx_hash)`.
- `v012` — adds the 3 governance BLOB columns to
  `reputation_anchors` (`governance_snapshot`, `governance_proof`,
  `governance_set_hash`) per RFC-0955-R1 lines 177-200. Plus
  `idx_reputation_anchors_governance_set_hash` for the
  cross-replica consistency check.

Original blocker (now cleared): RFC-0955-R1 acceptance — sibling Accepted
RFC at `rfcs/accepted/economics/0955-r1-reputation-anchoring.md` (promoted
2026-07-27 from the in-file amendment previously at RFC-0955 lines
912-1023). RFC-0968 acceptance (satisfied 2026-07-25) was a transitive
prerequisite but not the binding blocker.

The canonical on-chain binding is `ComputeOffer.reputation: ReputationDigest`
(RFC-0955-R1 §"ReputationDigest") + `ReputationAnchorBatch.score_ewma_raw:
[u8; 24]` (RFC-0955-R1 §"ReputationAnchorBatch"). RFC-0955-R1 does NOT define
a standalone `reputation:blake3_digest` field; `ReputationDigest` carries the
envelope-domain-separated 32-byte BLAKE3 digest over the canonical 24-byte
Dfp BLOB plus the full tuple identity (`did || signal_kind || layer ||
last_event_id || last_event_unix || samples || severity_total`). Earlier
draft wording referencing `reputation:blake3_digest` is RETIRED.

**Discriminant note (Round 3 review B2, resolved round 16)**: `ReputationError::AnchorTupleFanoutExceeded`
is **0x2A** in the impl (`crates/octo-reputation/src/error.rs:174`) and
**0x2A** in RFC-0968 §13 reserved band (`0x2A..=0xFF`); the previous
RFC-0955-R1 references to 0x2D were stale (0x2D is already used by
`StakeBelowMinimum` in `error.rs:190`). RFC-0955-R1 corrected to 0x2A
in this revision.

**Cross-RFC drift notes (Round 4 review N8, N9 — N9 resolved round 16)**:

1. **`StakeBelowMinimum` discriminant (N8)**: IMPL assigns `0x2D`
   (`error.rs:190`, payload `{ component: StakeComponent }`) but
   RFC-0968 §13 (line 2057 + 2621 table) assigns `0x17` (payload
   `{ provided: u64 }`). The IMPL value is in the reserved band
   `0x2A..=0xFF` per RFC-0968 §13 line 2641. **Resolved in commit
   `013a5676`** (round 2 of 0968a2 review): RFC-0968 §10 line 2057 +
   §13 line 2621 + §3 line 616 updated to declare
   `StakeBelowMinimum = 0x2D` with `{ component: StakeComponent }`
   payload, matching the IMPL. The IMPL stays as-is per the
   `error.rs:22-28` guardrail ("Do NOT change discriminants until
   RFC-0968-A2 lands"). The 0x17 slot is occupied by
   `GovernanceSlashFieldMismatch` at `error.rs:133`.

2. **`ReputationAnchorBatch` governance fields (N9)**: **Resolved
   by 0968a2 in commit `72bf19d7`** (round 16). `ReputationAnchorBatch`
   now carries `governance_snapshot: AnchorGovernanceSnapshot`,
   `governance_proof: AnchorGovernanceProof`, `governance_set_hash:
[u8; 32]`, plus `batch_size: u32`, plus
   `chain_block_height: Option<u64>` per RFC-0955-R1 lines 170,
   173, 177-200. The anchor-specific verifier types
   (`AnchorGovernanceSnapshot` / `AnchorGovernanceSigner` /
   `AnchorGovernanceProof`) live at `crates/octo-reputation/src/auth.rs`
   per path (a) reconciliation. The `AnchorLeaf::digest` field
   order was also fixed (`score_ewma_raw` at position 5 per
   RFC-0955-R1 lines 420-422 — the previous last-position was a
   cross-implementation interoperability bug). 3 canonical test
   vectors re-pinned.

## RFC

- RFC-0955-R1: Reputation Anchoring Amendment (sibling Draft RFC, canonical
  authority for the binding contract; promoted 2026-07-27 from the
  previously in-file RFC-0955 amendment)
- RFC-0955: Model Liquidity Layer (parent RFC; cross-references RFC-0955-R1
  from §"Compute Assets" + §"Performance Targets" + §"Implementation Phases"
  Phase 5)
- RFC-0968: Reputation Registry (the persisted source-of-truth whose events
  will be anchored; canonical home of `ReputationError::AnchorTupleFanoutExceeded
(0x2A, reserved band 0x2A..=0xFF per §13)`)

## Summary

Anchor `SignalEvent` records from RFC-0968's `reputation_events` table to the
CipherOcto on-chain ledger via RFC-0955-R1's `ReputationAnchorBatch` struct.
The anchor envelope binds a tuple `(did, signal_kind, layer, last_event_id)`
to the canonical 24-byte Dfp encoding `score_ewma_raw: [u8; 24]` plus
provenance counters; the on-chain `ReputationDigest` is computed over the
full domain-separated envelope. The persisted source aggregate has exactly
nine canonical fields (`did`, `kind`, `layer`, `score_ewma`, `samples`,
`severity_total`, `last_event_id`, `last_event_unix`, `updated_at_unix`).
This mission extends `SignalEvent` with `anchor_tx_hash: Option<[u8; 32]>` and
adds a background job that submits a Merkle-root batch transaction per
`(controller_id, ANCHOR_INTERVAL_SECS)` window (RFC-0968-A1 amendment 48 (beyond A2 scope — future amendment round TBD)).

This mission is **extracted** from RFC-0968's Phase 5 per Round 1 finding H11
(the original RFC conflated gossip federation + on-chain anchoring under one
mission; RFC-0968 now owns gossip federation (mission 0855p-b substrate) and
this mission owns on-chain anchoring).

## Why deferred? (historical)

- ~~RFC-0955-R1 is not yet final. On-chain anchoring requires the binding
  contract to be live.~~ **Cleared 2026-07-27** (RFC-0955-R1 Accepted).
- Anchoring is a separate cost model (gas, batch frequency) from gossip
  federation (storage, durability). This cost-model distinction remains
  a non-blocker rationale for keeping the missions separate.
- Mission 0968a unblocks independently of RFC-0968 acceptance (RFC-0968
  was promoted to Accepted 2026-07-25; the binding blocker was RFC-0955-R1).

## Scope (when unblocked)

> **Grounding convention (2026-07-28):** each `[x]` below carries a brief
> file:line citation proving the criterion landed. `[ ]` items have no
> grounded evidence in this repo or only partial coverage.

- [x] Extend `SignalEvent` with `anchor_tx_hash: Option<[u8; 32]>`
      — _ground_: `crates/octo-reputation/src/types.rs:289` `pub anchor_tx_hash: Option<[u8; 32]>` field declared on `SignalEvent` with doc-comment "Optional. Anchor tx hash (32-byte BLAKE3) populated by `ReputationStore::anchor_pending` once the event is committed to the anchoring chain (RFC-0955-R1). `None` until the anchor job runs and writes back via `set_event_anchor_tx_hash`." `SignalEvent::canonical_bytes` (`types.rs:311`) **EXCLUDES** `anchor_tx_hash` by deliberate design (Round 1 F3) — `anchor_tx_hash` is a post-event sidecar; folding it into the digest would break federation/replay stability. The Round 4 review verified this via the test `canonical_bytes_length_is_127_for_unanchored_event` at `types.rs:539-548` (pinned length 127 with anchor_tx_hash excluded).
- [x] Add `ReputationStore::anchor_pending(batch_size: u32)` API
      — _ground_: `crates/octo-reputation/src/store/mod.rs:192` `async fn anchor_pending(&self, batch_size: u32) -> StoreResult<Vec<(EventId, [u8; 32])>>` + `:198` `async fn set_event_anchor_tx_hash(&self, event_id: EventId, anchor_tx_hash: [u8; 32]) -> StoreResult<()>`. Implemented in `crates/octo-reputation/src/store/memory.rs:518,540` (linear scan + placeholder hash) and `crates/octo-reputation/src/store/stoolap.rs:1548,1606` for function definitions; the `WHERE anchor_tx_hash IS NULL ORDER BY recorded_at_unix LIMIT ?` sweep is at `stoolap.rs:1561` and the `UPDATE reputation_events SET anchor_tx_hash = ?` writes-back path is at `stoolap.rs:1714-1717` (composite-PK scope `(recorder_did, event_id)`, not the simple `WHERE event_id = ?` originally cited). Stub variant at `stoolap.rs:1826` (`mod stub`), dispatch at `:1863`, and `anchor_pending` stub fn at `:1976` returning `Err(ReputationError::ChainRefInvalid("stoolap_backend_unimplemented:anchor_pending"))` — the stub is NOT a placeholder-with-default; it preserves `cargo build` for feature-consumers that don't link the SQL backend, making any accidental call site immediately observable via the `ChainRefInvalid` error. `ReputationStoreCompat` forwarders in `crates/octo-reputation/src/compat/mod.rs` route both methods to the inner store.
- [x] Add `reputation_anchors` table (migration `v010__reputation_anchors.sql`)
      — _ground_: `crates/octo-reputation/migrations/v010__reputation_anchors.sql` is registered in `BUILTIN_MIGRATIONS` at `crates/octo-reputation/src/migrations.rs:42-44` (the `("v010__reputation_anchors", include_str!(...))` tuple entry). `crates/octo-reputation/src/store/stoolap.rs:1561` queries `WHERE anchor_tx_hash IS NULL` against `reputation_events` (the AC-required scan; ingestion path via `anchor_pending` returns `(EventId, [0u8; 32])` placeholder hashes that the caller overwrites with the real chain hash after on-chain submission). Schema columns (8 total, per `v010__reputation_anchors.sql`): `id INTEGER PRIMARY KEY AUTOINCREMENT`, `event_id BLOB NOT NULL UNIQUE`, `anchor_tx_hash BLOB NOT NULL`, `anchored_at_unix INTEGER NOT NULL`, `controller_id BLOB NOT NULL`, `anchor_root BLOB NOT NULL`, `leaf_count INTEGER NOT NULL`, `rotation_receipt_id BLOB` (nullable by default — no explicit `NOT NULL`). NO explicit `PRIMARY KEY (event_id)` constraint; the implicit PK is `id INTEGER` (stoolap-fork rowid alias), and uniqueness on `event_id` is enforced by the `UNIQUE` constraint. `event_id` is the chain-side idempotency key per RFC-0955-R1 §"Chain-Level Idempotency". Four indexes are created: `idx_reputation_anchors_controller` on `(controller_id)` (read-side lookup), `idx_reputation_anchors_controller_time` on `(controller_id, anchored_at_unix)` (daily fanout count), `idx_reputation_anchors_controller_root` (UNIQUE) on `(controller_id, anchor_root)` (chain-side Merkle-root uniqueness), and `idx_reputation_anchors_rotation_receipt` on `(rotation_receipt_id)` (plain, for post-rotation resubmission lookups per RFC-0955-R1 amendment 51). Also: `crates/octo-reputation/migrations/v011__reputation_events_anchor.sql` (registered in `BUILTIN_MIGRATIONS` at `:46-48`) adds `anchor_tx_hash BLOB` to `reputation_events` itself + the `idx_reputation_events_controller_anchor` index. The `anchor_tx_hash` column added by v011 is what `anchor_pending` scans and `set_event_anchor_tx_hash` writes back; the v010 ledger table is the chain-side mirror (not directly written by the impl yet — see the dormant-table note in `v010__reputation_anchors.sql` header).
- [ ] Background job: scan `reputation_events` where `anchor_tx_hash IS NULL`, submit Merkle-root batch transaction, persist
      — _ungrounded_: `crates/octo-reputation/src/anchor_job.rs` is committed (last touched by `32ea46e7`) and contains the in-process scheduler scaffold. The trait is `ChainAnchorSubmitter` with a `submit(&self, batch, fee) -> Result<[u8; 32], AnchorJobError>` method; the only impl in the file is `StubChainAnchorSubmitter` (`anchor_job.rs:139` struct decl + `:141` impl method) which writes a deterministic placeholder `batch.digest()` (32-byte BLAKE3 of the batch envelope, distinct per batch — Round 2 review F5 closed the previous concern that distinct batches would collide). No real chain-side adapter is wired into the job. The file is the scheduler scaffold, NOT the live RFC-0955-R1 binding-submission job. The actual on-chain submission path is ungrounded.
- [x] `DEFAULT_ANCHOR_INTERVAL_SECS` config + `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL=1` + `MAX_TUPLES_PER_ROOT=100`
      — _ground, constants declared_: `crates/octo-reputation/src/constants.rs:57` `pub const MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL: u64 = 1;`; `:61` `pub const MAX_TUPLES_PER_ROOT: u64 = 100;`; `anchor.rs:20-24` doc references. `DEFAULT_ANCHOR_INTERVAL_SECS = 300` const-tested at `crates/octo-reputation/src/anchor.rs:236` (`const { assert!(DEFAULT_ANCHOR_INTERVAL_SECS == 300) };`). Per-deployment config layer (separate from the in-process scheduler) is ungrounded — `[x]` for the constants being declared and consumed by the scheduler logic, not for the config plumbing.
- [ ] Cross-reference: mission 0855p-b gossip uses `anchor_tx_hash` to verify gossiped events have on-chain provenance
      — _ungrounded_: no consumer of `SignalEvent::anchor_tx_hash` exists in `crates/octo-network/src/gossip/`.

## Out of scope (this mission)

- Persisted reputation storage (RFC-0968 / mission 0968).
- Gossip federation (mission 0855p-b).
- Reputation tokenization (debated, no RFC).

## Acceptance Criteria

- [ ] Anchoring job is idempotent
      — _ungrounded_: no live anchoring job exists. The `anchor_job.rs` scheduler runs the in-process window-collision check but does not submit chain transactions.
- [ ] Anchoring failure does not corrupt `reputation_events`
      — _ungrounded_: no failure-mode tests in `anchor_job.rs` reference `reputation_events` mutation paths.
- [x] `reputation_anchors` is queryable by `did` (joined via `reputation_events`)
      — _ground_: `crates/octo-reputation/src/store/mod.rs:223` `async fn query_anchors_by_controller_id(&self, controller_id: ControllerId) -> StoreResult<Vec<AnchorRecord>>` on `ReputationStore` trait. Implemented in `crates/octo-reputation/src/store/memory.rs:593` (linear scan over `inner.events`, filter by `controller_id` + `anchor_tx_hash.is_some()`, sort `(recorded_at_unix ASC, event_id ASC)` tie-break — added in Round 4 review F2) and `crates/octo-reputation/src/store/stoolap.rs` real impl (`SELECT event_id, anchor_tx_hash, recorded_at_unix FROM reputation_events WHERE controller_id = $1 AND anchor_tx_hash IS NOT NULL ORDER BY recorded_at_unix ASC`) + stub variant (`stoolap_backend_unimplemented:query_anchors_by_controller_id`). Compat forwarder in `crates/octo-reputation/src/compat/mod.rs:256`. Test: `query_anchors_by_controller_id_filters_and_orders` (memory).
- [x] `reputation_anchors` stores only `EventId` values (not `AttestationId`)
      — _ground_: `crates/octo-reputation/src/store/mod.rs:234` `pub struct AnchorRecord { event_id: EventId, anchor_tx_hash: [u8; 32], anchored_at_unix: u64 }` — only `EventId`, never `AttestationId`. Schema (`migrations/v010__reputation_anchors.sql`) stores `event_id BLOB NOT NULL UNIQUE` — uniqueness (not PK) on `event_id` is the AC-required constraint. Schema + Rust type both reflect the AC constraint.
- [ ] Anchor batch interval is configurable per deployment; default = 300s
      — _partial / ungrounded for per-deployment config plumbing_: `crates/octo-reputation/src/anchor.rs:236` const-test asserts `DEFAULT_ANCHOR_INTERVAL_SECS == 300`. The constant is pinned; the per-deployment config plumbing is ungrounded. Job is in-process only — no deployment-side config layer yet.
- [x] Round 8 snapshot rule (`snapshot.finalized_at_unix + MAX_GOVERNANCE_SNAPSHOT_AGE_SECS < now_unix` ⇒ `GovernanceSnapshotStale`)
      — _ground_: `crates/octo-reputation/src/constants.rs:38` `pub const MAX_GOVERNANCE_SNAPSHOT_AGE_SECS: u64 = 600;`. `crates/octo-reputation/src/auth.rs:28` `pub fn age_secs(&self, now_unix: u64) -> u64` + `auth.rs:32` `pub fn is_fresh(&self, now_unix: u64) -> bool { self.age_secs(now_unix) <= MAX_GOVERNANCE_SNAPSHOT_AGE_SECS }`. `ReputationError::GovernanceSnapshotStale { age_secs, max } = 0x10` declared at `crates/octo-reputation/src/error.rs:106` and emitted by `crates/octo-reputation/src/retirement.rs:88` when the snapshot is stale. Snapshot validation flow is grounded in retirement path; the anchor submission path does NOT yet invoke the freshness check — partial `[x]` (constant + variant + retirement caller grounded; anchor-submission caller ungrounded).
- [ ] **`ReputationAnchorBatch::rotation_receipt_id: Option<[u8; 32]>` (RFC-0955-R1 §"ReputationAnchorBatch")**
      — _ground_: `crates/octo-reputation/src/anchor.rs:121` `pub struct ReputationAnchorBatch { ... }` (the in-memory struct declaration) and `anchor.rs:134` `pub rotation_receipt_id: Option<[u8; 32]>` field on the struct. NOT `[x]` for the **chain-side wiring** of the same field (in-memory struct is grounded; chain encoding is ungrounded).
- [ ] DID-rotation finality interaction (`MIN_FINALITY_BLOCKS = 12` threshold)
      — _ungrounded (constant + helper only)_: `crates/octo-reputation/src/constants.rs:42` `pub const MIN_FINALITY_BLOCKS: u64 = 12;` declared. Helper `is_finality_reached(submitted, finalized)` implemented at `crates/octo-reputation/src/anchor.rs:206-208`. No DID-rotation finality handler grounded — `anchor_job.rs:139` `StubChainAnchorSubmitter` is a stub; `plan_batches` (`anchor_job.rs:172` function decl) does not consult `is_finality_reached` for reorg-aware finality checks. Re-flagged to `[ ]` per strict grounded check.
- [x] `AnchorTupleFanoutExceeded` (`MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY = 100`)
      — _ground_: `crates/octo-reputation/src/constants.rs:66` `pub const MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY: u64 = 100;`. Variant `ReputationError::AnchorTupleFanoutExceeded(u64) = 0x2A` declared at `crates/octo-reputation/src/error.rs:174` (the canonical 0x2A in RFC-0968 §13 reserved band `0x2A..=0xFF`; RFC-0955-R1 corrected to 0x2A in this revision). **Canonical emission path** at `crates/octo-reputation/src/anchor_job.rs`: `pub struct AnchorTupleFanout {` (line 112) with `pub fn to_reputation_error(&self) -> crate::error::ReputationError` (line 121) → `AnchorTupleFanoutExceeded(count)`; `pub fn check_daily_fanout(...)` (function decl line 307; offending-branch `Some(AnchorTupleFanout { ... })` at lines 313-317) preflight; `pub async fn run_once_strict<S: ChainAnchorSubmitter>(submitter: Arc<S>, ...)` (lines 338-339) returns `Result<AnchorJobOutcome, ReputationError>` so the canonical `0x2A` variant is the live error from the wrapping API. Test module at `anchor_job.rs` includes `check_daily_fanout_returns_some_at_cap` (line 538), `check_daily_fanout_returns_none_below_cap` (line 555), `run_once_strict_emits_anchor_tuple_fanout_exceeded` (line 562); the module also contains 6 additional tests (`run_once_strict_emits_anchor_submitter_rejected`, `run_once_strict_emits_already_anchored_in_window`, `run_once_submits_via_stub_with_expected_fee`, etc.) covering the surrounding surface.
- [x] Per-leaf anchor fee (`MIN_FEE_PER_LEAF = 50`, `ANCHOR_FEE_PER_ROOT = 5_000`)
      — _ground_: `crates/octo-reputation/src/constants.rs:70` `pub const ANCHOR_FEE_PER_ROOT: u64 = 5_000;` + `:74` `pub const MIN_FEE_PER_LEAF: u64 = 50;`. `crates/octo-reputation/src/anchor.rs:172` `pub fn fee(&self) -> u128 { (ANCHOR_FEE_PER_ROOT as u128) + (MIN_FEE_PER_LEAF as u128) * (self.leaves.len() as u128) }` and `:177` `pub fn within_leaf_cap(&self) -> bool` enforce `leaves.len() <= MAX_TUPLES_PER_ROOT = 100`. Compiles const-tests at `anchor.rs:239-240` (`const { assert!(ANCHOR_FEE_PER_ROOT == 5_000) }; const { assert!(MIN_FEE_PER_LEAF == 50) };`). Per-deployment OCTO balance check still ungrounded.
- [ ] Governance-set hash + 3 distinct signatures in every anchor tx
      — _ground, constants only_: `GOVERNANCE_QUORUM = 3` at `crates/octo-reputation/src/constants.rs`. The verification flow inside the anchor submission path is ungrounded.
- [ ] Reorg re-submission on reorg > `MIN_FINALITY_BLOCKS`
      — _ungrounded (constant + helper only)_: `MIN_FINALITY_BLOCKS = 12` declared at `crates/octo-reputation/src/constants.rs:42`; `is_finality_reached(submitted, finalized) -> bool` implemented at `crates/octo-reputation/src/anchor.rs:206-208`. The reorg-aware resubmission handler is NOT grounded — `plan_batches` (`anchor_job.rs:172`) does not consult `is_finality_reached` to re-submit batches whose submitted-but-not-finalized roots have been reorged. Re-flagged to `[ ]` per strict grounded check.
- [x] Test vectors at `crates/octo-reputation/tests/anchoring/canonical_blobs.rs::CANONICAL_ANCHOR_BLOB`
      — _ground_: `crates/octo-reputation/tests/canonical_blobs.rs` (note: Cargo's test auto-discovery requires the file at `tests/*.rs`; the `anchoring/` subdir was merged into `tests/` so the file lives at `crates/octo-reputation/tests/canonical_blobs.rs`). Three pinned vectors: `CANONICAL_ANCHOR_BLOB_0_LEAVES` (controller `[0;32]`, window 0), `CANONICAL_ANCHOR_BLOB_1_LEAF` (controller `[1;32]`, window 3_333 = 1_000_000 / 300), `CANONICAL_ANCHOR_BLOB_100_LEAVES` (controller `[0xAB;32]`, window 5_666_666 = 1_700_000_000 / 300). Domain separator `BLAKE3_REPUTATION_ANCHOR_DOMAIN = b"cipherocto/reputation/anchor/v1"` pinned at `crates/octo-reputation/src/constants.rs:181` and asserted by `canonical_blob_digest_domain_separator_is_stable`. 5 tests pass: zero/single/hundred-leaf pinned + domain-separator stability + BLAKE3 determinism (`canonical_blob_two_independent_computations_are_byte_identical`).

## Location

Migration slots `v010__reputation_anchors.sql` and
`v011__reputation_events_anchor.sql` allocated in RFC-0968 §28 catalog
(after v008 = `recorder_registration` and v009 = `kind_weights`),
gated on RFC-0955-R1 acceptance (now satisfied 2026-07-27). The
migrations are registered in `BUILTIN_MIGRATIONS` at
`crates/octo-reputation/src/migrations.rs:42-48`. Both files are
committed (created 2026-07-28, last touched by `32ea46e7`).

## Complexity

Medium. RFC-0955-R1 binding contract interaction + Merkle-root batch job +
storage extension + governance-proof verification + reorg re-submission.

## Claimant

@cipherocto (mission-level; sub-tasks claimed individually per RFC-0955-R1 §"Implementation Phases")

## Pull Request

None open. Per the Round 3 review (2026-07-30), 9 ungrounded ACs remain
(see Status header). Submission blocked on addressing the ungrounded
items in §"Acceptance Criteria" below OR on user-initiated Path B
closure with explicit acknowledgement of the ungrounded ACs in the
Path B audit banner.
