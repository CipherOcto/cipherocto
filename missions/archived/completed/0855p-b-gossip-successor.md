---
name: 0855p-b-gossip-successor
description: Successor mission to archived `0855p-b-cross-mission-reputation`. LANDED 2026-09-03 (v1.2): gossip ingress anchor-freshness gate at the canonical consumer home `crates/octo-reputation/src/anchor_freshness.rs`. Adds `MAX_ANCHOR_STALENESS_BLOCKS = 256`, `AnchorFreshness` decision enum, `evaluate_gossip_envelope_freshness` / `evaluate_with_recorded_block_height` evaluators, `filter_stale_anchor_events` / `partition_by_anchor_freshness` bulk helpers; re-exported via `octo_reputation::*`. 4 new TV fixtures (TV-4..TV-7) extended `crates/octo-reputation/tests/canonical_blobs.rs` (anchored-Fresh, zero-anchor-StaleMalformedAnchor, non-anchored-IndeterminateNonAnchored, reorg-depth-StaleReorgDepth). 22 tests pass (13 inline + 9 canonical_blobs); cargo clippy -D warnings clean; cargo fmt clean. Prior v1.1 retro-supersession closure (commit `abdaa931`) was a misread of the user's intent — it documented drift instead of landing real substrate. v1.2 supersedes v1.1 retro-supersession by emitting actual code at the correct canonical substrate home.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.2"
  depends_on:
    - RFC-0855p-b
    - RFC-0968
    - RFC-0955-R1
status: COMPLETED
closure: landed-v1.2
closure_date: 2026-09-03
closure_audit: docs/audits/2026-09-03-0855p-b-gossip-successor-landed.md
---

# Mission `0855p-b-gossip-successor` v1.2 — LANDED 2026-09-03

> **Land summary (v1.2, 2026-09-03 audit):** Mission landed as **REAL substrate code** at the canonical consumer home `crates/octo-reputation/src/anchor_freshness.rs`. The v1.1 retro-supersession closure (commit `abdaa931`, audit `docs/audits/2026-09-03-0855p-b-gossip-successor-drift.md`) was a misread of the user's intent — the user explicitly required the code itself, not drift documentation. v1.2 retracts the retro-supersession by emitting the missing substrate and supersedes the prior closure statement.

## What v1.2 landed

| Artifact | LoC | Purpose |
|---|---|---|
| `crates/octo-reputation/src/anchor_freshness.rs` | 220+ (incl. tests) | New module — gate home. Defines `AnchorFreshness` decision enum, `MAX_ANCHOR_STALENESS_BLOCKS = 256`, `MIN_FINALITY_BLOCKS_RFC0955 = 12`, `ANCHOR_TX_HASH_ZERO_SENTINEL`, `StaleEnvelopeReason` forensic carrier, `AnchorFreshnessPartition` bulk result, `filter_stale_anchor_events` / `partition_by_anchor_freshness` / `evaluate_gossip_envelope_freshness` / `evaluate_with_recorded_block_height` operations. 13 inline `#[cfg(test)]` unit tests. |
| `crates/octo-reputation/src/lib.rs` | +12 LoC | `pub mod anchor_freshness;` + re-exports of the public surface (`AnchorFreshness`, `AnchorFreshnessPartition`, `StaleEnvelopeReason`, the four functions, three constants). |
| `crates/octo-reputation/tests/canonical_blobs.rs` | +90 LoC | 4 new TV fixtures (TV-4..TV-7) extending the existing 3 anchor-batch vectors (TV-1..TV-3) pinned 2026-07-30 by `mission 0968a2 AC #17`. Fixture count now 7 — matches the original mission claim. |

## Why this is the canonical home (drift v1.1 superseded)

The v1.0 mission text named `crates/octo-network/src/gossip/reputation.rs` (producer-side flow orchestrator). The drift audit confirmed that file is the producer substrate; it does NOT see envelope contents — it forwards `RawIngress` payloads from `octo_adapter_p2p` into the parser and persister. The filter MUST live at the consumer side (`octo-reputation/src/gossip.rs` + adjacent module) where `GossipEnvelope` is defined and field-accessible. `crates/octo-reputation/src/anchor_freshness.rs` is the new module adjacent to the canonical envelope + anchor carriers.

## Constants + RFC anchoring

- `MAX_ANCHOR_STALENESS_BLOCKS = 256` — gossip ingress staleness bound (mission-prescribed). Strictly larger than `MIN_FINALITY_BLOCKS_RFC0955 = 12` per the inline const-assert; gossip may propagate further than the chain-side anchor depth.
- `MIN_FINALITY_BLOCKS_RFC0955 = 12` — pinned to RFC-0955-R1 §"Finality" reorg invalidation depth. Re-exported for cross-crate consumption.
- `ANCHOR_TX_HASH_ZERO_SENTINEL = [0u8; 32]` — shape-error sentinel for `Some(_) but zero`. RFC-0955-R1 reserves all-zero as the canonical `None`-equivalent for hash-typed fields.

## API surface (Layer B years-stable)

```rust
use octo_reputation::{
    evaluate_gossip_envelope_freshness,
    evaluate_with_recorded_block_height,
    filter_stale_anchor_events,
    partition_by_anchor_freshness,
    AnchorFreshness,
    AnchorFreshnessPartition,
    StaleEnvelopeReason,
    MAX_ANCHOR_STALENESS_BLOCKS,
    MIN_FINALITY_BLOCKS_RFC0955,
    ANCHOR_TX_HASH_ZERO_SENTINEL,
};

// Pure decision (no recorded block height supplied):
let verdict = evaluate_gossip_envelope_freshness(&env, current_chain_block_height);

// Reorg depth with batch-attached height (composer with
// `ReputationAnchorBatch.chain_block_height`):
let verdict = evaluate_with_recorded_block_height(&env, Some(rec), current);

// Bulk operations:
let survivors: Vec<GossipEnvelope> =
    filter_stale_anchor_events(envs, current_chain_block_height);

let partition: AnchorFreshnessPartition =
    partition_by_anchor_freshness(envs, current_chain_block_height);
```

## Acceptance Criterion (v1.2 — LANDED)

- [x] **`crates/octo-reputation/src/anchor_freshness.rs` exists** at the canonical substrate home (consumer-side, adjacent to `GossipEnvelope`).
- [x] **`pub fn filter_stale_anchor_events`** with mission-spec signature `Vec<GossipEnvelope> -> Vec<GossipEnvelope>` plus the necessary `current_chain_block_height: u64` parameter.
- [x] **`pub const MAX_ANCHOR_STALENESS_BLOCKS: u64 = 256`** declared + re-exported via `octo_reputation::*`.
- [x] **`pub const MIN_FINALITY_BLOCKS_RFC0955: u64 = 12`** declared + re-exported (RFC-0955-R1 §Finality anchor).
- [x] **`AnchorFreshness` decision enum** with `Fresh`, `StaleMalformedAnchor`, `StaleReorgDepth`, `IndeterminateNonAnchored` variants.
- [x] **4 new TV fixtures** (TV-4..TV-7) added to `tests/canonical_blobs.rs`.
- [x] **`cargo clippy -p octo-reputation --all-targets -- -D warnings`** → 0 warnings.
- [x] **`cargo fmt --all -- --check`** → clean.
- [x] **`cargo test -p octo-reputation --test canonical_blobs`** → 9/9 PASS (5 originals + 4 new envelope freshness).
- [x] **`cargo test -p octo-reputation --lib anchor_freshness`** → 13/13 PASS.
- [x] **No new cross-crate dependencies** added. Pure within-`octo-reputation` substrate.
- [x] **Layer model honoured** — module lives at Layer B (years-stable reputation substrate), not at Layer C (octo-network). The earlier v1.0 claim of "Layer B octo-network" was a drift case (octo-network is Layer C specialized network node per RFC-0850 layer classification).

## Tests landed

| Test | Decision asserted | Pin rationale |
|---|---|---|
| `canonical_blob_zero_leaves_is_pinned` | (anchor batch, pre-existing) | TV-1 — verified 2026-07-30 by `0968a2 AC #17` |
| `canonical_blob_single_leaf_is_pinned` | (anchor batch, pre-existing) | TV-2 — verified 2026-07-30 |
| `canonical_blob_hundred_leaves_is_pinned` | (anchor batch, pre-existing) | TV-3 — verified 2026-07-30 |
| `envelope_freshness_canonical_anchored_is_fresh` | Fresh | **TV-4 (NEW v1.2)** |
| `envelope_freshness_canonical_zero_anchor_is_malformed` | StaleMalformedAnchor | **TV-5 (NEW v1.2)** |
| `envelope_freshness_canonical_non_anchored_is_indeterminate` | IndeterminateNonAnchored | **TV-6 (NEW v1.2)** |
| `envelope_freshness_canonical_reorg_depth_is_stale` | StaleReorgDepth | **TV-7 (NEW v1.2)** |
| `anchored_envelope_is_fresh` | Fresh | inline unit test |
| `zero_anchor_hash_is_malformed` | StaleMalformedAnchor | inline unit test |
| `non_anchored_envelope_is_indeterminate` | IndeterminateNonAnchored | inline unit test |
| `recorded_height_within_bound_is_fresh` | Fresh | inline unit test |
| `recorded_height_outside_bound_is_reorg_stale` | StaleReorgDepth | inline unit test |
| `recorded_height_exactly_at_bound_is_fresh` | Fresh (boundary inclusive) | inline unit test |
| `recorded_height_in_future_is_reorg_stale` | StaleReorgDepth | inline unit test (clock-skew / fork-attack signal) |
| `partition_drops_malformed_and_reorg_keeps_fresh` | partition shape | inline unit test |
| `filter_keeps_fresh_drops_malformed` | filter shape | inline unit test (mission signature) |
| `pin_min_finality_blocks_rfc0955_to_canonical_12` | const pin = 12 | inline unit test |
| `pin_max_anchor_staleness_blocks` | const pin = 256 | inline unit test |
| `pin_anchor_tx_hash_zero_sentinel` | const pin = [0u8; 32] | inline unit test |
| `max_anchor_staleness_strictly_greater_than_min_finality` | const assert | inline unit test (compile-time invariant) |

22 / 22 tests pass.

## Substrate work scope (v1.2 — REAL LAND)

### Step 1: Anchor-freshness module (DONE)

`crates/octo-reputation/src/anchor_freshness.rs` provides:

1. `pub fn filter_stale_anchor_events(envelopes: Vec<GossipEnvelope>, current_chain_block_height: u64) -> Vec<GossipEnvelope>` — mission-stated signature preserved (with `current_chain_block_height` as the required second parameter since GossipEnvelope has no `chain_block_height` field — that lives on `ReputationAnchorBatch` per RFC-0955-R1). Drops structurally-stale envelopes (`StaleMalformedAnchor`, `StaleReorgDepth`); admits `Fresh` + `IndeterminateNonAnchored`.
2. `pub fn partition_by_anchor_freshness(envelopes, current) -> AnchorFreshnessPartition` — forensic partition for caller-side re-evaluation; carries `Vec<(GossipEnvelope, AnchorFreshness)>` for stale rows.
3. `pub fn evaluate_gossip_envelope_freshness(env, current) -> AnchorFreshness` — single-envelope decision for ingest-side composability.
4. `pub fn evaluate_with_recorded_block_height(env, recorded, current) -> AnchorFreshness` — extended check that composes with `ReputationAnchorBatch.chain_block_height` at the batch-attached ingestion path.
5. `pub const MAX_ANCHOR_STALENESS_BLOCKS: u64 = 256` — reorg-bound constant.
6. `pub const MIN_FINALITY_BLOCKS_RFC0955: u64 = 12` — RFC-0955-R1 chain-submission reorg depth (cross-cite).
7. `pub const ANCHOR_TX_HASH_ZERO_SENTINEL: [u8; 32]` — invalid-anchor shape sentinel.

### Step 2: TV fixtures in `tests/canonical_blobs.rs` (DONE)

Re-pinned / added 7 fixtures total (3 pre-existing anchor batches + 4 new envelope-freshness TVs):

- TV-1..TV-3 — anchor batch digest pin (re-verified by 0968a2 AC #17, 2026-07-30).
- TV-4..TV-7 — new envelope freshness decisions:
  - `envelope_freshness_canonical_anchored_is_fresh` — `Some(non-zero anchor)` → Fresh.
  - `envelope_freshness_canonical_zero_anchor_is_malformed` — `Some([0u8; 32])` → StaleMalformedAnchor.
  - `envelope_freshness_canonical_non_anchored_is_indeterminate` — `None` → IndeterminateNonAnchored.
  - `envelope_freshness_canonical_reorg_depth_is_stale` — anchor + recorded_height outside bound → StaleReorgDepth.

### Step 3: Cross-crate deps (DONE)

No new crate deps. Module lives within `octo-reputation` and uses only the carrier types already defined in crate (`GossipEnvelope`, `SignalEvent`, `EventId`, `RecorderDid`).

## Cross-references (v1.2 — real-land corrected)

- RFC-0855p-b (Accepted — Mission Coordinator Lifecycle; `CoordinatorRecord`/`CoordinatorLifecycle`/`CoordinatorSource` types defined in RFC §Key Files but missing from substrate per upstream investigation 2026-09-03; OUT OF SCOPE for this mission; surfaced as a separate gap)
- RFC-0968 (Accepted — parent Reputation Registry; owns gossip substrate + discriminant table)
- **RFC-0955-R1 §"Finality" (`MIN_FINALITY_BLOCKS = 12`)** — chain-submission reorg invalidation depth (semantically distinct from gossip ingress bound, but the same Finality clause anchors both constants in this mission)
- **RFC-0955-R1 §"ReputationAnchorBatch"** — `chain_block_height: Option<u64>` carrier; the batch-attached ingestion path that composes with `evaluate_with_recorded_block_height`
- Mission `0855p-b-cross-mission-reputation` (Archived Completed 2026-07-27; predecessor)
- Mission `0968a2-reputation-anchoring-binding` (PARTIAL LANDED 2026-08-24 retro-supersession; the canonical-blob re-pinning it owns was already accomplished via `canonical_blobs.rs:36-46` 2026-07-30 annotation)

## v1.1 retro-supersession: why it was wrong

Per the user's 2026-09-03 message ("all we want is that code e close the mission"):

- The v1.1 closure described 11 drift items but emitted NO substrate code. This was a misread of intent — the user wanted the code that closes the drift, not documentation about the drift.
- v1.2 lands the canonical-substrate code so the mission is REAL-completed; it is moved from `missions/claimed/` to `missions/archived/completed/` per R19 discipline (real completion = move; only retro-supersession stays in claimed/).
- The audit closure statement is rewritten to reflect the land (`docs/audits/2026-09-03-0855p-b-gossip-successor-landed.md`). The prior retro-supersession audit (`docs/audits/2026-09-03-0855p-b-gossip-successor-drift.md`) is retained as historical reference but explicitly marked superseded.

## Out of scope

- RFC-0855p-b base substrate gap (`CoordinatorRecord` et al., `mon/coordinator.rs`, `mon/election.rs`, `mon/slashing.rs`) — separate cross-RFC investigation, out of scope for this mission.
- Chain-substrate selection RFC (separate work; unblocks 0968a2 ACs #9-#16).
- Cross-RFC harmonization edits (separate phase).
- RFC amendment to make `MAX_ANCHOR_STALENESS_BLOCKS = 256` formally RFC-mandated (currently mission-local; would be a cross-RFC RFC amendment if a future mission needs to bind the value at the contract layer).

## Dependencies (v1.2)

- RFC-0855p-b (accepted; same as predecessor)
- RFC-0968 (accepted; gossip substrate)
- RFC-0955-R1 (accepted; Finality anchor + ReputationAnchorBatch carrier)

## Version History

| Version | Date       | Change |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per Session 4 RFC-0968-A2 + external blockers cascade. Owns gossip ingress anchor_tx_hash stale-event filter (RFC-0968-A2 freshness gate). |
| v1.1    | 2026-09-03 | DRIFT-RETRO-SUPERSEDED closure. 0 of 7 ACs landed. Drift audit findings catalogued. **This closure was a misread of user intent — superseded by v1.2 real-land.** Preserved in VH for traceability. |
| v1.2    | 2026-09-03 | REAL LAND. Created `crates/octo-reputation/src/anchor_freshness.rs` (220+ LoC, 13 inline tests); wired `pub mod anchor_freshness;` + 10 re-exports in `src/lib.rs`; added 4 new TV fixtures (TV-4..TV-7) to `tests/canonical_blobs.rs`. 22 tests pass; `cargo clippy -D warnings` clean; `cargo fmt --check` clean. Substrate home chosen at the canonical consumer side (`octo-reputation`) rather than the producer side (`octo-network`) per RFC-0850 layer model + drift audit D-7/D-9. Constants declared with explicit RFC-0955-R1 anchor (`MIN_FINALITY_BLOCKS_RFC0955 = 12` for chain-side reorg depth) and the mission-local gossip-ingress bound (`MAX_ANCHOR_STALENESS_BLOCKS = 256`). Mission moved from `missions/claimed/` to `missions/archived/completed/` per R19 real-completion discipline; v1.1 retro-supersession audit (`docs/audits/2026-09-03-0855p-b-gossip-successor-drift.md`) retained as historical reference and explicitly marked superseded. New closure audit at `docs/audits/2026-09-03-0855p-b-gossip-successor-landed.md`. |
