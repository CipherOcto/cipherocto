---
name: 0855p-b-gossip-successor
description: Successor mission to archived `0855p-b-cross-mission-reputation`. Originally filed to bridge 0968a2-reputation-anchoring-binding AC #17 + AC #18 per an imagined RFC-0968-A2 v0.8.1 "freshness gate" specification. DRIFT CLOSED 2026-09-03 (v1.1): the cited RFC-0968-A2 v0.8.1 is the **Discriminant Stability Sub-amendment** (`rfcs/accepted/economics/0968-a2-discriminant-stability.md`) and does NOT define a freshness gate; the `MAX_ANCHOR_STALENESS_BLOCKS = 256` constant is not in any RFC; the substrate paths claimed (octo-network-side gossip filter + octo-network/tests/canonical_blobs.rs) do not match actual topology (canonical_blobs.rs lives in `crates/octo-reputation/tests/`; the 3 TV vectors were re-pinned 2026-07-30 by `0968a2 AC #17` per its own file:line annotation). Predecessor `0855p-b-cross-mission-reputation` Archived Completed 2026-07-27; downstream `0968a2-reputation-anchoring-binding` PARTIAL LANDED 2026-08-24 retro-supersession; no external mission references this file. Retro-supersession closure per [[cipherocto-design-principles]] §Stable Abstractions (no substrate code invented to match unbacked spec). File retained in `missions/claimed/` per R19 historical-mission-preservation discipline.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0855p-b
    - RFC-0968
    - RFC-0968-A2
status: CLOSED
closure: retro-superseded-via-drift
closure_date: 2026-09-03
closure_audit: docs/audits/2026-09-03-0855p-b-gossip-successor-drift.md
---

# Mission `0855p-b-gossip-successor` v1.1 — CLOSED 2026-09-03 (DRIFT-RETRO-SUPERSEDED)

> **Drift closure (2026-09-03 audit):** Mission status = **CLOSED (DRIFT-RETRO-SUPERSEDED)** — **0 of 7 ACs landed** because the cited substrate paths + RFC citations were drift cases. CLOSURE rationale:
>
> **RFC citation drift (CRITICAL):**
> - `RFC-0968-A2 v0.8.1` referenced here as the "freshness gate" specification is the **Discriminant Stability Sub-amendment** (`rfcs/accepted/economics/0968-a2-discriminant-stability.md`; status Accepted 2026-08-23). Its actual content: codepoint reservation `ControllerIdMissing = 0x34` + `controller_id = blake3(governance_pubkey)` derivation. **It does NOT define a freshness gate, anchor staleness, or gossip ingress filter.**
> - `MAX_ANCHOR_STALENESS_BLOCKS = 256` constant referenced here is not declared in any RFC. The closest analogue is RFC-0955-R1 §"Finality" `MIN_FINALITY_BLOCKS = 12` (`rfcs/accepted/economics/0955-r1-reputation-anchoring.md` L285), a reorg-invalidation depth — semantically distinct from gossip-anchor staleness.
>
> **Substrate path drift (HIGH):**
> - `crates/octo-network/src/gossip/reputation.rs` was the producer (sender-side) gossip substrate, landed by predecessor `0855p-b-cross-mission-reputation` (`388fd327`). The consumer (ingress) substrate lives at `crates/octo-reputation/src/gossip.rs` (`GossipEnvelope` defined at L42). A producer-side filter cannot see consumer-side envelope freshness signals without a cross-crate borrow.
> - `tests/canonical_blobs.rs` was claimed at `crates/octo-network/tests/canonical_blobs.rs`; **the canonical_blobs.rs file actually lives at `crates/octo-reputation/tests/canonical_blobs.rs`** and contains 3 TV vectors (not 7), re-pinned 2026-07-30 by `mission 0968a2 AC #17` per the in-file annotation (L36-46).
>
> **Field path drift (HIGH):**
> - `GossipEnvelope` does NOT have a top-level `anchor_tx_hash` field. The `anchor_tx_hash: Option<[u8; 32]>` lives on the inner `SignalEvent` (`crates/octo-reputation/src/types.rs:289`), reachable as `envelope.event.anchor_tx_hash`.
> - `GossipEnvelope` does NOT have a `chain_block_height` field. The `chain_block_height: Option<u64>` lives on `ReputationAnchorBatch` (`crates/octo-reputation/src/anchor.rs:184`). Envelopes and anchor batches are distinct carrier types in distinct missions.
>
> **Layer drift (MEDIUM):**
> - Step 3 claims `octo-network` is "Layer B" per [[cipherocto-design-principles]]. Per RFC-0850 layer classification, `octo-network` is **Layer C specialized network node**; Layer B is the envelope-wire / identity substrate (RFC-0957, RFC-0850 envelope).
>
> **Predecessor chain (DELIVERED):**
> - `0855p-b-cross-mission-reputation` (Archived Completed 2026-07-27; 12/12 ACs landed across `f0c8d6ad` / `388fd327` / `87ffe153` / `f16132c0`) delivered the gossip substrate + recorder-DID keying model + gossip consumer integration test (`crates/octo-network/tests/cross_mission_federation.rs`).
> - `0968a2-reputation-anchoring-binding` (PARTIAL LANDED 2026-08-24 retro-supersession; 8/27 ACs GREEN) delivered `ReputationAnchorBatch` governance fields + `chain_block_height: Option<u64>` + 3 canonical TV re-pinning under the `0968a2 AC #17` annotation in `canonical_blobs.rs:36-46`.
> - The "gossip ingress anchor_tx_hash stale-event filter" function imagined by this mission has no RFC anchor and no downstream consumer (verified: 0 references in `missions/`+`rfcs/`+`docs/` outside this file).
>
> **No external dependency blocks on this mission.** `rg 0855p-b-gossip-successor missions/ rfcs/ docs/` returns only this file. Closing this mission does NOT unblock or break any other artifact. Closure preserves the file in `missions/claimed/` per historical-mission-preservation discipline (R19 scope discipline from `0968a2` retro-supersession template). Audit closure statement: `docs/audits/2026-09-03-0855p-b-gossip-successor-drift.md`.

## Context (v1.0 — drift-annotated)

Archived mission `0855p-b-cross-mission-reputation` (PR submission pending, 12/12 ACs landed) covered gossip substrate + recorder-DID keying + canonical envelopes. The original v1.0 description named RFC-0968-A2 v0.8.1 as introducing "anchor-event freshness gate" — **that characterization is drift**; RFC-0968-A2 v0.8.1 is the Discriminant Stability Sub-amendment and does not introduce a freshness gate. Mission `0968a2-reputation-anchoring-binding` AC #17 (canonical TV re-pinning) was GREEN at the 2026-07-30 re-pin (per `canonical_blobs.rs:36-46` annotation) and is the actual predecessor for the anchor-event freshness work.

## Substrate work scope (v1.0 — DRIFT-RETRO-SUPERSEDED)

> **DRIFT v1.1 (2026-09-03):** All substrate work below is **superseded** by the Drift Closure Note at the top of this mission. The substrate paths (`crates/octo-network/src/gossip/reputation.rs`, `crates/octo-network/tests/canonical_blobs.rs`), RFC citation (RFC-0968-A2 v0.8.1 freshness gate), constant (`MAX_ANCHOR_STALENESS_BLOCKS = 256`), and layer label (octo-network = Layer B) were all drift cases documented in the closure rationale. No substrate code in this section was claimed, written, merged, or referenced by any downstream mission.

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

## Acceptance Criterion (v1.0 — DRIFT-RETRO-SUPERSEDED)

> **DRIFT v1.1 (2026-09-03):** All 7 ACs in this section are DRIFT — substrate paths, RFC citations, and field locations all mismatch actual topology per the Drift Closure Note. AC gates cannot be satisfied; no implementation was attempted.

- `filter_stale_anchor_events` function landed in `crates/octo-network/src/gossip/reputation.rs`
- 7 TV fixtures PASS in `tests/canonical_blobs.rs` lines 34-76
- `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- `cargo fmt --all -- --check` clean
- AC gate: `rg 'fn filter_stale_anchor_events' crates/octo-network/src/gossip/reputation.rs` → 1 hit
- AC gate: `cargo test -p octo-network --test canonical_blobs 2>&1 | tail -3` → "test result: ok. 7 passed; 0 failed"
- 0968a2-reputation-anchoring-binding AC #17 + AC #18 can flip to GREEN after this mission closes

## Files / Artifacts (v1.0 — DRIFT-RETRO-SUPERSEDED)

> **DRIFT v1.1 (2026-09-03):** No new files were created. The function `filter_stale_anchor_events` was never implemented; `crates/octo-network/tests/canonical_blobs.rs` never existed (actual canonical_blobs.rs lives at `crates/octo-reputation/tests/canonical_blobs.rs` and contains 3 TV vectors, not 7).

- New: `crates/octo-network/src/gossip/reputation.rs` `filter_stale_anchor_events` (~15 LoC)
- Modified: `crates/octo-network/tests/canonical_blobs.rs` (7 TV fixtures re-pinned)

## Cross-references (v1.1 — drift-corrected)

- RFC-0855p-b (Accepted 2026-06-15 — Mission Coordinator Lifecycle; `CoordinatorRecord`/`CoordinatorLifecycle`/`CoordinatorSource` types defined in RFC §Key Files L847-851 but missing from substrate per upstream investigation 2026-09-03; out of scope for this closure)
- RFC-0968 (Accepted — parent Reputation Registry; authoritative for gossip substrate + discriminant table)
- **RFC-0968-A2 v0.8.1 (Accepted 2026-08-23) — Discriminant Stability Sub-amendment** (`rfcs/accepted/economics/0968-a2-discriminant-stability.md`); does NOT define a freshness gate, anchor staleness, or gossip ingress filter (v1.0 misattribution corrected in v1.1 drift closure)
- **RFC-0955-R1 §"Finality" (`MIN_FINALITY_BLOCKS = 12`, L285) — the closest RFC-anchored constant to the imagined `MAX_ANCHOR_STALENESS_BLOCKS`; reorg-invalidation depth, not gossip-anchor staleness** (v1.1 cross-ref added; semantically distinct from the imagined filter)
- Mission `0855p-b-cross-mission-reputation` (Archived Completed 2026-07-27; predecessor of this filed successor)
- Mission `0968a2-reputation-anchoring-binding` (PARTIAL LANDED 2026-08-24 retro-supersession; delivered the canonical-blob re-pinning that v1.0 mistakenly attributed to this mission)
- Mission `0968a-reputation-anchoring` (claimed — sibling mission; 10/19 ACs landed)

## Out of scope

- Inline retro-supersession of archived `0855p-b-cross-mission-reputation` (per historical-mission-preservation)
- Chain-substrate selection RFC (separate work; unblocks 0968a2 ACs #9-#16)
- Cross-RFC harmonization edits (separate phase)

## Dependencies (v1.1 — drift-corrected)

- RFC-0855p-b (accepted canonical substrate for Mission Coordinator types)
- RFC-0968 (parent reputation RFC; owns gossip substrate)
- Mission `0968a2-reputation-anchoring-binding` (claimed — partially landed; the canonical-blob re-pinning it owns was already accomplished via `canonical_blobs.rs:36-46` 2026-07-30 annotation; this mission's v1.0 imagined consumer-dependency never materialized)

## Version History

| Version | Date       | Change                                                                                                                                                                  |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per Session 4 RFC-0968-A2 + external blockers cascade. Owns gossip ingress anchor_tx_hash stale-event filter (RFC-0968-A2 freshness gate). |
| v1.1    | 2026-09-03 | DRIFT-RETRO-SUPERSEDED closure. 0 of 7 ACs landed. Drift audit findings: (a) RFC-0968-A2 v0.8.1 is the Discriminant Stability Sub-amendment, NOT a freshness gate spec; (b) `MAX_ANCHOR_STALENESS_BLOCKS = 256` undefined in any RFC (closest analogue RFC-0955-R1 §Finality `MIN_FINALITY_BLOCKS = 12` is reorg-invalidation depth, semantically distinct); (c) substrate path drift — claimed `crates/octo-network/src/gossip/reputation.rs` is producer-side; canonical ingress is `crates/octo-reputation/src/gossip.rs`; `crates/octo-network/tests/canonical_blobs.rs` does not exist (actual `crates/octo-reputation/tests/canonical_blobs.rs`); (d) field path drift — `anchor_tx_hash` lives on `SignalEvent` (`types.rs:289`), `chain_block_height` lives on `ReputationAnchorBatch` (`anchor.rs:184`), neither on `GossipEnvelope`; (e) layer label drift — `octo-network` is Layer C specialized network node, not Layer B per RFC-0850 + design principles. Predecessor chain (`0855p-b-cross-mission-reputation` Archived Completed 2026-07-27 + `0968a2-reputation-anchoring-binding` PARTIAL LANDED 2026-08-24 retro-supersession) delivered the anchor-event freshness work collectively; no external mission references this file. Closure preserved in `missions/claimed/` per R19 historical-mission-preservation discipline. Audit closure statement at `docs/audits/2026-09-03-0855p-b-gossip-successor-drift.md` (scratchpad, gitignored per [[docs-audits-scratchpad]]). |
