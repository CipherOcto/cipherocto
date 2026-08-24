---
name: 0855p-b-gossip-successor
description: Successor mission to archived `0855p-b-cross-mission-reputation` per RFC-0968-A2 v0.8.1 Status + 0968a2-reputation-anchoring-binding AC #17 ("Gossip consumer rejects stale `anchor_tx_hash: None` events at ingress handler only — 7 test fixtures remain unchanged; requires 0855p-b successor"). This mission owns the gossip ingress handler anchor_tx_hash stale-event filter (RFC-0968-A2 freshness gate). Unblocks 0968a2 AC #17 + 3 canonical test vector re-pinning coordination per 0968a2 AC #18. Filed 2026-08-24 per claim-and-implement scope.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0855p-b
    - RFC-0968
    - RFC-0968-A2
status: OPEN
---

# Mission `0855p-b-gossip-successor` v1.0 — OPEN 2026-08-24

## Context

Archived mission `0855p-b-cross-mission-reputation` (PR submission pending, 12/12 ACs landed) covered gossip substrate + recorder-DID keying + canonical envelopes. RFC-0968-A2 v0.8.1 introduces anchor-event freshness gate (`anchor_tx_hash: Option<Hash>` field on gossip envelope). Mission `0968a2-reputation-anchoring-binding` AC #17 explicitly requires successor mission to land the ingress handler filter for stale `anchor_tx_hash: None` events.

## Substrate work scope (NEW — owned by this mission)

### Step 1: Stale-event filter at gossip ingress

In `crates/octo-network/src/gossip/reputation.rs`, add `pub fn filter_stale_anchor_events(events: Vec<GossipEnvelope>) -> Vec<GossipEnvelope>` that:

1. Drops any `GossipEnvelope` with `anchor_tx_hash: None` AND a recorded `chain_block_height` older than `MAX_ANCHOR_STALENESS_BLOCKS` (= 256, per RFC-0968-A2 freshness gate).
2. Preserves all `GossipEnvelope` with `anchor_tx_hash: Some(_)` (chain-submitted anchors always have a hash).
3. Returns the filtered vec for downstream propagation.

### Step 2: 7 test fixtures in `tests/canonical_blobs.rs`

Re-pin the 7 test fixtures (lines 34, 41, 48, 55, 62, 69, 76) to demonstrate:

- TV-1: `anchor_tx_hash: Some(hash)` accepted (fresh, chain-submitted)
- TV-2: `anchor_tx_hash: None` + recent height accepted (within staleness window)
- TV-3: `anchor_tx_hash: None` + stale height REJECTED (filter)
- TV-4..TV-7: edge cases (chain reorg at boundary, double-submit detection, recorder-DID rotation lineage, gossip fan-out)

### Step 3: Cross-crate deps

No new crate deps. Modifies `octo-network` (Layer B) only.

## Acceptance Criterion

- `filter_stale_anchor_events` function landed in `crates/octo-network/src/gossip/reputation.rs`
- 7 TV fixtures PASS in `tests/canonical_blobs.rs` lines 34-76
- `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- `cargo fmt --all -- --check` clean
- AC gate: `rg 'fn filter_stale_anchor_events' crates/octo-network/src/gossip/reputation.rs` → 1 hit
- AC gate: `cargo test -p octo-network --test canonical_blobs 2>&1 | tail -3` → "test result: ok. 7 passed; 0 failed"
- 0968a2-reputation-anchoring-binding AC #17 + AC #18 can flip to GREEN after this mission closes

## Files / Artifacts

- New: `crates/octo-network/src/gossip/reputation.rs` `filter_stale_anchor_events` (~15 LoC)
- Modified: `crates/octo-network/tests/canonical_blobs.rs` (7 TV fixtures re-pinned)

## Cross-references

- RFC-0855p-b (archived — predecessor mission)
- RFC-0968 (parent reputation RFC)
- RFC-0968-A2 v0.8.1 (anchor-event freshness gate)
- Mission `0855p-b-cross-mission-reputation` (archived)
- Mission `0968a2-reputation-anchoring-binding` (claimed — depends on this mission for AC #17)
- Mission `0968a-reputation-anchoring` (claimed — sibling)

## Out of scope

- Inline retro-supersession of archived `0855p-b-cross-mission-reputation` (per historical-mission-preservation)
- Chain-substrate selection RFC (separate work; unblocks 0968a2 ACs #9-#16)
- Cross-RFC harmonization edits (separate phase)

## Dependencies

- RFC-0855p-b (archived canonical)
- RFC-0968 (parent reputation RFC)
- RFC-0968-A2 v0.8.1 (anchor freshness gate mandate)
- Mission `0968a2-reputation-anchoring-binding` (consumer — depends on this mission)

## Version History

| Version | Date       | Change                                                                                                                                                                  |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per Session 4 RFC-0968-A2 + external blockers cascade. Owns gossip ingress anchor_tx_hash stale-event filter (RFC-0968-A2 freshness gate). |
