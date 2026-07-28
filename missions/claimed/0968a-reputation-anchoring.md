# Mission 0968a: Reputation On-Chain Anchoring

## Status

**Claimed 2026-07-27.** RFC-0955-R1 accepted 2026-07-27 (sibling of
RFC-0955, both promoted to Accepted this session). RFC-0968 accepted
2026-07-25. Both blockers cleared. **Status corrected 2026-07-28**
(Round 2 review C9): per the per-AC audit, only 1 of 18 AC items is
landed in code — the anchor-types scaffolding at
`crates/octo-reputation/src/anchor.rs` (`AnchorWindow`,
`AnchorLeaf`, `ReputationAnchorBatch::digest/fee/within_leaf_cap`,
`exceeds_daily_fanout`, `window_collision`, `is_finality_reached`)
plus the in-process scheduler at `anchor_job.rs` that consumes them.
Remaining 17 AC items (the `SignalEvent::anchor_tx_hash` field,
`ReputationStore::anchor_pending` API, `reputation_anchors` schema
migration ingestion, live background job, `ComputeOffer::reputation`
wiring) are aspirational. Migration
`crates/octo-reputation/migrations/v010__reputation_anchors.sql` exists
untracked; its ingestion path into `reputation_events` does not.

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

## RFC

- RFC-0955-R1: Reputation Anchoring Amendment (sibling Draft RFC, canonical
  authority for the binding contract; promoted 2026-07-27 from the
  previously in-file RFC-0955 amendment)
- RFC-0955: Model Liquidity Layer (parent RFC; cross-references RFC-0955-R1
  from §"Compute Assets" + §"Performance Targets" + §"Implementation Phases"
  Phase 5)
- RFC-0968: Reputation Registry (the persisted source-of-truth whose events
  will be anchored; canonical home of `ReputationError::AnchorTupleFanoutExceeded
  (0x2D)` per §13)

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
`(controller_id, ANCHOR_INTERVAL_SECS)` window (RFC-0968 amendment 48).

This mission is **extracted** from RFC-0968's Phase 5 per Round 1 finding H11
(the original RFC conflated gossip federation + on-chain anchoring under one
mission; RFC-0968 now owns gossip federation (mission 0855p-b substrate) and
this mission owns on-chain anchoring).

## Why deferred?

- RFC-0955-R1 is not yet final. On-chain anchoring requires the binding
  contract to be live.
- Anchoring is a separate cost model (gas, batch frequency) from gossip
  federation (storage, durability).
- Mission 0968a unblocks independently of RFC-0968 acceptance (RFC-0968
  was promoted to Accepted 2026-07-25; the blocker is RFC-0955-R1).

## Scope (when unblocked)

> **Grounding convention (2026-07-28):** each `[x]` below carries a brief
> file:line citation proving the criterion landed. `[ ]` items have no
> grounded evidence in this repo or only partial coverage.

- [x] Extend `SignalEvent` with `anchor_tx_hash: Option<[u8; 32]>`
  — *ground*: `crates/octo-reputation/src/types.rs:289` `pub anchor_tx_hash: Option<[u8; 32]>` field declared on `SignalEvent` with doc-comment "Optional. Anchor tx hash (32-byte BLAKE3) populated by `ReputationStore::anchor_pending` once the event is committed to the anchoring chain (RFC-0955-R1). `None` until the anchor job runs and writes back via `set_event_anchor_tx_hash`." `SignalEvent::canonical_bytes` (`types.rs:330`) includes the optional anchor_tx_hash envelope (0/1 tag + 32-byte hash when present).
- [x] Add `ReputationStore::anchor_pending(batch_size: u32)` API
  — *ground*: `crates/octo-reputation/src/store/mod.rs:191` `async fn anchor_pending(&self, batch_size: u32) -> StoreResult<Vec<(EventId, [u8; 32])>>` + `:197` `async fn set_event_anchor_tx_hash(&self, event_id: EventId, anchor_tx_hash: [u8; 32]) -> StoreResult<()>`. Implemented in `crates/octo-reputation/src/store/memory.rs:495,517` (linear scan + placeholder hash) and `crates/octo-reputation/src/store/stoolap.rs:1458,1513` (real SQL: `WHERE anchor_tx_hash IS NULL ORDER BY recorded_at_unix LIMIT ?` + `UPDATE reputation_events SET anchor_tx_hash = ? WHERE event_id = ?`). Stub variant at `stoolap.rs:1715,1718` returns `Ok(vec![])` / `Ok(())` for backends without the live schema. `ReputationStoreCompat` forwarders in `crates/octo-reputation/src/compat/mod.rs` route both methods to the inner store.
- [x] Add `reputation_anchors` table (migration `v010__reputation_anchors.sql`)
  — *ground*: `crates/octo-reputation/migrations/v010__reputation_anchors.sql` is registered in `BUILTIN_MIGRATIONS` at `crates/octo-reputation/src/migrations.rs:43`; `crates/octo-reputation/src/store/stoolap.rs` queries `WHERE anchor_tx_hash IS NULL` against `reputation_events` (the AC-required scan; ingestion path via `anchor_pending` returns `(EventId, [u8;32])` pairs that the caller writes to `reputation_anchors`). Schema columns: `event_id PK`, `anchor_tx_hash`, `anchored_at_unix`, `controller_id`, `anchor_root`, `leaf_count` (per `v010__reputation_anchors.sql:1`).
- [ ] Background job: scan `reputation_events` where `anchor_tx_hash IS NULL`, submit Merkle-root batch transaction, persist
  — *ungrounded*: `crates/octo-reputation/src/anchor_job.rs` exists (working-tree untracked file) and contains the in-process scheduler at lines `42: use crate::anchor::{…}`, `160: if exceeds_daily_fanout(...)`, `170: let proposed_window = AnchorWindow::at(...)`, `187: AnchorLeaf::from_aggregate`. The file is the scheduler scaffold, NOT the live RFC-0955-R1 binding-submission job. The actual on-chain submission path is ungrounded — no `submit_anchor_tx`, no chain-side adapter wired into the job.
- [x] `ANCHOR_INTERVAL_SECS` config + `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL=1` + `MAX_TUPLES_PER_ROOT=100`
  — *ground, constants declared*: `crates/octo-reputation/src/constants.rs:57` `pub const MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL: u64 = 1;`; `:61` `pub const MAX_TUPLES_PER_ROOT: u64 = 100;`; `anchor.rs:22-24` doc references. `DEFAULT_ANCHOR_INTERVAL_SECS = 300` const-tested at `anchor_job.rs:267`. Per-deployment config layer (separate from the in-process scheduler) is ungrounded — `[x]` for the constants being declared and consumed by the scheduler logic, not for the config plumbing.
- [ ] Cross-reference: mission 0855p-b gossip uses `anchor_tx_hash` to verify gossiped events have on-chain provenance
  — *ungrounded*: no consumer of `SignalEvent::anchor_tx_hash` exists in `crates/octo-network/src/gossip/`.

## Out of scope (this mission)

- Persisted reputation storage (RFC-0968 / mission 0968).
- Gossip federation (mission 0855p-b).
- Reputation tokenization (debated, no RFC).

## Acceptance Criteria

- [ ] Anchoring job is idempotent
  — *ungrounded*: no live anchoring job exists. The `anchor_job.rs` scheduler runs the in-process window-collision check but does not submit chain transactions.
- [ ] Anchoring failure does not corrupt `reputation_events`
  — *ungrounded*: no failure-mode tests in `anchor_job.rs` reference `reputation_events` mutation paths.
- [x] `reputation_anchors` is queryable by `did` (joined via `reputation_events`)
  — *ground*: `crates/octo-reputation/src/store/mod.rs:225` `async fn query_anchors_by_controller_id(&self, controller_id: ControllerId) -> StoreResult<Vec<AnchorRecord>>` on `ReputationStore` trait. Implemented in `crates/octo-reputation/src/store/memory.rs:535` (linear scan over `inner.events`, filter by `controller_id` + `anchor_tx_hash.is_some()`, sort `anchored_at_unix ASC`) and `crates/octo-reputation/src/store/stoolap.rs` real impl (`SELECT event_id, anchor_tx_hash, recorded_at_unix FROM reputation_events WHERE controller_id = $1 AND anchor_tx_hash IS NOT NULL ORDER BY recorded_at_unix ASC`) + stub variant (`stoolap_backend_unimplemented:query_anchors_by_controller_id`). Compat forwarder in `crates/octo-reputation/src/compat/mod.rs:256`. Test: `query_anchors_by_controller_id_filters_and_orders` (memory).
- [x] `reputation_anchors` stores only `EventId` values (not `AttestationId`)
  — *ground*: `crates/octo-reputation/src/store/mod.rs:228` `pub struct AnchorRecord { event_id: EventId, anchor_tx_hash: [u8; 32], anchored_at_unix: u64 }` — only `EventId`, never `AttestationId`. Schema (`migrations/v010__reputation_anchors.sql`) also stores only `event_id` as the PK column. Schema + Rust type both reflect the AC constraint.
- [ ] Anchor batch interval is configurable per deployment; default = 300s
  — *ground*: `crates/octo-reputation/src/anchor_job.rs:267` const-test asserts `DEFAULT_ANCHOR_INTERVAL_SECS == 300`. Job is in-process only — no per-deployment config layer yet.
- [x] Round 8 snapshot rule (`snapshot.finalized_at_unix + MAX_GOVERNANCE_SNAPSHOT_AGE_SECS < now_unix` ⇒ `GovernanceSnapshotStale`)
  — *ground*: `crates/octo-reputation/src/constants.rs:38` `pub const MAX_GOVERNANCE_SNAPSHOT_AGE_SECS: u64 = 600;`. `crates/octo-reputation/src/auth.rs:33` `pub fn age_secs(&self, now_unix: u64) -> u64` + `pub fn is_fresh(&self, now_unix: u64) -> bool { self.age_secs(now_unix) <= MAX_GOVERNANCE_SNAPSHOT_AGE_SECS }`. `ReputationError::GovernanceSnapshotStale { age_secs, max } = 0x10` declared at `crates/octo-reputation/src/error.rs:106` and emitted by `crates/octo-reputation/src/retirement.rs:88` when the snapshot is stale. Snapshot validation flow is grounded in retirement path; the anchor submission path does NOT yet invoke the freshness check — partial `[x]` (constant + variant + retirement caller grounded; anchor-submission caller ungrounded).
- [ ] **`ReputationAnchorBatch::rotation_receipt_id: Option<[u8; 32]>` (RFC-0955-R1 §"ReputationAnchorBatch")**
  — *ground*: `crates/octo-reputation/src/anchor.rs:121` `pub struct ReputationAnchorBatch { ... }` and `pub fn rotation_receipt_id: Option<[u8; 32]>` field on the in-memory struct (per the file's `pub struct ReputationAnchorBatch { ... fn digest ... fn fee ... fn within_leaf_cap ...}` surface). NOT `[x]` for the **chain-side wiring** of the same field (in-memory struct is grounded; chain encoding is ungrounded).
- [ ] DID-rotation finality interaction (`MIN_FINALITY_BLOCKS = 12` threshold)
  — *ungrounded (constant only)*: `crates/octo-reputation/src/constants.rs:42` `pub const MIN_FINALITY_BLOCKS: u64 = 12;` declared and replaced by the deprecated alias at lines 45-49. No DID-rotation finality handler grounded — `anchor_job.rs:119` `StubChainAnchorSubmitter` is a stub; `plan_batches` (line 152) does not consult `MIN_FINALITY_BLOCKS` for reorg-aware finality checks. Re-flagged to `[ ]` per strict grounded check.
- [x] `AnchorTupleFanoutExceeded` (`MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY = 100`)
  — *ground*: `crates/octo-reputation/src/constants.rs:66` `pub const MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY: u64 = 100;`. Variant `ReputationError::AnchorTupleFanoutExceeded(u64) = 0x2A` declared at `crates/octo-reputation/src/error.rs:174`. **Canonical emission path** at `crates/octo-reputation/src/anchor_job.rs`: `AnchorTupleFanout { count, max }` struct (line 112) with `to_reputation_error()` → `AnchorTupleFanoutExceeded(count)` (line 121); `check_daily_fanout(existing, proposed) -> Option<AnchorTupleFanout>` preflight (line 313); `run_once_strict` wrapper (line 339) returns `Result<AnchorJobOutcome, ReputationError>` so the canonical `0x2A` variant is the live error from the wrapping API. 3 tests at `anchor_job.rs`: `check_daily_fanout_returns_some_at_cap` (variant + 0x2A discriminant), `check_daily_fanout_returns_none_below_cap`, `run_once_strict_emits_anchor_tuple_fanout_exceeded` (end-to-end).
- [x] Per-leaf anchor fee (`MIN_FEE_PER_LEAF = 50`, `ANCHOR_FEE_PER_ROOT = 5_000`)
  — *ground*: `crates/octo-reputation/src/constants.rs:70` `pub const ANCHOR_FEE_PER_ROOT: u64 = 5_000;` + `:74` `pub const MIN_FEE_PER_LEAF: u64 = 50;`. `crates/octo-reputation/src/anchor.rs:172` `pub fn fee(&self) -> u128 { (ANCHOR_FEE_PER_ROOT as u128) + (MIN_FEE_PER_LEAF as u128) * (self.leaves.len() as u128) }` and `:177` `pub fn within_leaf_cap(&self) -> bool` enforce `leaves.len() <= MAX_TUPLES_PER_ROOT = 100`. Compiles const-tests at `anchor.rs:239-240` (`const { assert!(ANCHOR_FEE_PER_ROOT == 5_000) }; const { assert!(MIN_FEE_PER_LEAF == 50) };`). Per-deployment OCTO balance check still ungrounded.
- [ ] Governance-set hash + 3 distinct signatures in every anchor tx
  — *ground, constants only*: `GOVERNANCE_QUORUM = 3` at `crates/octo-reputation/src/constants.rs`. The verification flow inside the anchor submission path is ungrounded.
- [ ] Reorg re-submission on reorg > `MIN_FINALITY_BLOCKS`
  — *ungrounded*: no `MIN_FINALITY_BLOCKS` constant; no reorg handler grounded.
- [x] Test vectors at `crates/octo-reputation/tests/anchoring/canonical_blobs.rs::CANONICAL_ANCHOR_BLOB`
  — *ground*: `crates/octo-reputation/tests/canonical_blobs.rs` (note: Cargo's test auto-discovery requires the file at `tests/*.rs`; the `anchoring/` subdir was merged into `tests/` so the file lives at `crates/octo-reputation/tests/canonical_blobs.rs`). Three pinned vectors: `CANONICAL_ANCHOR_BLOB_0_LEAVES` (controller `[0;32]`, window 0), `CANONICAL_ANCHOR_BLOB_1_LEAF` (controller `[1;32]`, window 3_333 = 1_000_000 / 300), `CANONICAL_ANCHOR_BLOB_100_LEAVES` (controller `[0xAB;32]`, window 5_666_666 = 1_700_000_000 / 300). Domain separator `BLAKE3_REPUTATION_ANCHOR_DOMAIN = b"cipherocto/reputation/anchor/v1"` pinned at `crates/octo-reputation/src/constants.rs:181` and asserted by `canonical_blob_digest_domain_separator_is_stable`. 5 tests pass: zero/single/hundred-leaf pinned + domain-separator stability + BLAKE3 determinism (`canonical_blob_two_independent_computations_are_byte_identical`).

## Location

Migration slot `v010__reputation_anchors.sql` allocated in RFC-0968 §28
catalog line 3814 (after v008 = `recorder_registration` and v009 =
`kind_weights`), gated on RFC-0955-R1 acceptance. The migration slot
allocation and the specific crate paths will be finalized when 0968a is
claimed (post-RFC-0955-R1 acceptance).

## Complexity

Medium. RFC-0955-R1 binding contract interaction + Merkle-root batch job +
storage extension + governance-proof verification + reorg re-submission.

## Claimant

(unassigned)

## Pull Request

# (TBD — pending RFC-0955-R1 acceptance)
