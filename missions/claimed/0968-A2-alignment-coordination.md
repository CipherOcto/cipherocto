---
name: 0968-A2-alignment-coordination
description: Coordination summary for RFC-0968-A2 v0.8.1 mission alignment per audit 2026-08-24. Documents 1 inline retrofix category surfaced by RFC-0968-A2 spec audit (mission status partial LANDED — 8 of 27 ACs GREEN + 19 DEFERRED to chain-substrate selection RFC external blocker not owned by this mission) + dependency on chain-substrate selection RFC. NO scope of its own — pure cross-RFC alignment documentation; existing 0968a2 mission preserved untouched per historical-mission-preservation discipline except for inline retrofix documented below per R19 scope discipline.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0968a2-reputation-anchoring-binding
    - 0968a-reputation-anchoring
    - RFC-0968-A2
    - RFC-0968
status: OPEN
---

# Mission `0968-A2-alignment-coordination` v1.0 — OPEN 2026-08-24

## Context

RFC-0968-A2 v0.8.1 (canonical Accepted per `rfcs/accepted/economics/0968-a2-discriminant-stability.md` YAML `version: 0.8.1` + `status: Accepted`) extends RFC-0968 (parent reputation registry) with §1 Controller-level Codepoint Amendments + §2 Governance Quorum Carryover + §3 Reserved Codepoint Range + §4 Sub-Amendment Procedure Note. Mission audit 2026-08-24 surfaced 1 retrofix category for existing `0968a2-reputation-anchoring-binding` mission (status partial LANDED + chain-substrate selection RFC external blocker) — no new sibling mission needed because the DEFERRED ACs require substrate + config crate from external chain-substrate selection RFC, not substrate work in scope of RFC-0968-A2.

This mission captures the audit findings + documents the external dependency. **This mission is documentation-only** — it does not edit any existing 0968-* mission file beyond inline retrofix documented below per historical-mission-preservation discipline (existing OPEN/CLAIMED/LANDED mission state represents committed work at its filing time and is preserved where possible; only stale placeholders and clear contradictions receive inline retrofixes per R19 scope discipline).

## Inline retrofix applied (2026-08-24 audit)

### Retrofix: `0968a2-reputation-anchoring-binding` status partial LANDED + external blocker

**Defect:** Mission Status block + §Acceptance Criteria header + §AC → Scope mapping table show mission `claimed` with 27 ACs (8 `[x]` GREEN + 19 `[ ]` DEFERRED). The 19 DEFERRED ACs all cite chain-substrate selection RFC as external blocker — the substrate for those ACs does not yet exist on disk (Live `ChainAnchorSubmitter` impl, reorg handler runtime, DID-rotation finality handler runtime, per-deployment config plumbing for reputation subsystem, gossip ingress handler for anchor events).

**Evidence:**

1. `git log --oneline | grep 0968a2` → 5 commits: `6104a57f` (R7 mission-text refresh) + `48cf9978` (R2 fixes) + `b0660c39` (R1 fixes) + `72bf19d7` (IMPL landed) + `f8ac3a82` (0968a status refresh) — confirms IMPL landed R0-R2 + R7 mission refresh.
2. `crates/octo-reputation/src/anchor.rs:174-208` + `:143-161` + `auth.rs:399-603` — substrate for 8 GREEN ACs present.
3. `crates/octo-reputation/migrations/v012__reputation_anchors_governance.sql` — v012 migration LANDED (governance_snapshot + governance_proof + governance_set_hash columns).
4. `crates/octo-reputation/src/anchor_job.rs:139` — stub only; deterministic placeholder, does NOT exercise `reputation_anchors(event_id)` UNIQUE path. Live submitter fixture required for DEFERRED ACs (idempotency + failure isolation).

**Fix:** Inline retro-supersession note added to Status block quote (combined partial LANDED + external blocker into single quote for readability). Mission §Acceptance Criteria `[x]` vs `[ ]` checkboxes preserved per historical-mission-preservation + R19 scope discipline. DEFERRED ACs unchanged (the blocker is external; closure path documented in this coordination mission).

## Gaps surfaced by RFC-0968-A2 audit

### Gap 1: Live `ChainAnchorSubmitter` impl PENDING (chain-substrate selection RFC external blocker)

RFC-0968-A2 spec declares Live `ChainAnchorSubmitter` impl required for production (gated on chain-substrate selection RFC per `## Dependencies` of `0968a2` mission). 19 of 27 ACs `[ ]` DEFERRED to this external blocker.

**Coverage gap:** `rg 'ChainAnchorSubmitter' crates/` returns 0 hits for live impl. Stub at `crates/octo-reputation/src/anchor_job.rs:139` returns deterministic placeholder; does NOT exercise `reputation_anchors(event_id)` UNIQUE path required for idempotency test (AC #14) + failure isolation test (AC #15).

**Owned by:** External — chain-substrate selection RFC (RFC-0927-adjacent; not yet filed; mission external to RFC-0968-A2 scope per R19). DEFERRED ACs documented in `0968a2` §Acceptance Criteria as DEFERRED — closure path = chain-substrate selection RFC filing + landed + 19 ACs re-evaluated.

### Gap 2: Per-deployment config plumbing for reputation subsystem PENDING

RFC-0968-A2 governance quorum verification requires `interval_secs` + `controller_id` + `chain_endpoint` per-deployment config plumbing. Config crate TBD per `## Dependencies`.

**Coverage gap:** `rg 'interval_secs|chain_endpoint' crates/octo-reputation/src/` returns 0 hits. RFC-0927 is about `RouterConfig`, not reputation subsystem config.

**Owned by:** External — chain-substrate selection RFC (config crate TBD).

### Gap 3: Gossip ingress handler for `anchor_tx_hash: None` events PENDING

RFC-0968-A2 spec declares gossip cross-reference required for DEFERRED AC #16 (gossip consumer rejects stale `anchor_tx_hash: None` events at ingress handler only). 7 test fixtures remain unchanged.

**Coverage gap:** Gossip file owned by archived `0855p-b` mission. RFC-0855p-b successor not yet filed.

**Owned by:** External — `0855p-b` successor mission (TBD).

## Sibling mission cross-references

- `0968a2-reputation-anchoring-binding` (claimed) — substrate ownership for 8 GREEN ACs (LANDED via commits `72bf19d7` + `b0660c39` + `48cf9978` + `6104a57f` + `013a5676`)
- `0968a-reputation-anchoring` (claimed) — predecessor mission; 9 ungrounded ACs (most ground as 0968a2 implementation lands)

## Acceptance Criterion

- 1 inline retrofix applied to `0968a2` per audit findings (status partial LANDED + external blocker)
- AC gate: `rg 'Retro-supersession \(2026-08-24 audit\)' missions/claimed/0968a2-reputation-anchoring-binding.md` → 1 hit (retro-supersession note)
- AC gate: `rg '8 of 27 ACs GREEN' missions/claimed/0968a2-reputation-anchoring-binding.md` → 1 hit (AC count summary)
- AC gate: `rg 'PARTIAL LANDED' missions/claimed/0968a2-reputation-anchoring-binding.md` → 1 hit (status marker)
- Cross-RFC cite validation: Guard 2 PASS for all 1 retrofixed + 1 new coordination mission files
- Prettier clean
- No new INVALID cites introduced

## Files / Artifacts

- Edit: `missions/claimed/0968a2-reputation-anchoring-binding.md` (Status block retro-supersession note)
- New: `missions/claimed/0968-A2-alignment-coordination.md` (this file)

## Cross-references

- RFC-0968-A2 v0.8.1 (canonical Accepted — discriminant stability sub-amendment)
- RFC-0968 (parent reputation registry RFC)
- RFC-0008 §RFC-0008 Execution Class Mapping (every RFC MUST carry this table — RFC-0968-A2 conforms)
- Mission `0968a2-reputation-anchoring-binding` (claimed — retrofix target)
- Mission `0968a-reputation-anchoring` (claimed — predecessor)
- Mission `0968-reputation-persistence` (claimed — `crates/octo-reputation/` substrate owner)
- External: chain-substrate selection RFC (RFC-0927-adjacent; not yet filed)
- External: `0855p-b` successor mission (gossip ingress handler)
- Sibling coordination: `0959-alignment-coordination` + `0960-alignment-coordination` + `0967-A1-alignment-coordination` + `0010-alignment-coordination` (cross-RFC harmonization pattern)

## Out of scope

- Retroactive supersession of older 0968-* missions beyond the 1 inline retrofix (per R19 scope discipline)
- Chain-substrate selection RFC substrate work (external; not owned by RFC-0968-A2 scope)
- Per-deployment config crate for reputation subsystem (external; config crate TBD)
- Gossip ingress handler for `anchor_tx_hash: None` events (external; 0855p-b successor TBD)
- Live `ChainAnchorSubmitter` impl (external blocker; closure path documented in §Acceptance Criteria DEFERRED markers)
- 3 canonical test vector re-pinning (DEFERRED — requires live `ChainAnchorSubmitter` fixture for round-trip byte-exact assertion)
- Cargo command text rewrites in `0968a2-reputation-anchoring-binding` (e.g., `cargo test -p cipherocto-policy` → `cargo test -p octo-policy`) — historical mission text preserved verbatim; only retro-supersession note added per R19
- Cross-RFC harmonization edits (research doc + companion RFC cross-refs) per `vault-monetary-research-consequence` Phase 5 (separate phase)

## Dependencies

- `0968a2-reputation-anchoring-binding` (claimed — retrofix target + substrate ownership for 8 GREEN ACs)
- `0968a-reputation-anchoring` (claimed — predecessor with 9 ungrounded ACs)
- RFC-0968-A2 v0.8.1 (canonical Accepted state)
- External: chain-substrate selection RFC (RFC-0927-adjacent; not yet filed) — blocker for 19 DEFERRED ACs
- External: `0855p-b` successor mission (gossip ingress handler) — blocker for 1 DEFERRED AC

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                           |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per RFC-0968-A2 v0.8.1 mission audit 2026-08-24. 1 inline retrofix category (status partial LANDED + external blocker) + 0 new sibling missions (19 DEFERRED ACs gated on chain-substrate selection RFC external; no in-scope substrate work). Pure coordination. |
