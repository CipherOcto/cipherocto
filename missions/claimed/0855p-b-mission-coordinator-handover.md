---
name: 0855p-b-mission-coordinator-handover
description: Mission Coordinator handover protocol per RFC-0855p-b §Implementation Phase 4. Wire `HandoverRequestEnvelope` + `HandoverResultEnvelope` + `MessagePreservationQueue` for voluntary / forced / emergency handover paths (Active→Handover / Handover→Inactive transitions). Compose with `HandoverReasonTypeId` from `octo-coordinator-types/src/lib.rs` (commit 93f1758c) + RFC-0855p-e handover envelope substrate (commit b0f14119). Land at `crates/octo-network/src/dot/handover.rs` (Layer C, per RFC §Key Files path) — append new handlers, do NOT duplicate Layer B types. RFC-0008 Class A determinism. 3 canonical handover vectors. BLOCKED by `0855p-b-state-machine-types` + `0855p-b-mission-coordinator-liveness`.
metadata:
  node_type: substrate-coordinator
  type: substrate-handover-protocol
  rfc_source: RFC-0855p-b
  rfc_section: Implementation Phases §Phase 4 (L827-833) + §Appendix A State Machine (Active → Handover + Handover → Inactive) + Round 4 adversarial review (L832)
  substrate_home:
    canonical: crates/octo-network/src/dot/handover.rs (append handlers; existing RFC-0855p-e substrate)
    tests: crates/octo-network/tests/canonical_handover_blobs.rs
  execution_class: A
  deterministic: true
  depends_on:
    - RFC-0855p-b
    - RFC-0855p-e
    - mission:0855p-b-state-machine-types
    - mission:0855p-b-mission-coordinator-liveness
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-03
v: "1.0"
created: 2026-09-03
---

# Mission `0855p-b-mission-coordinator-handover` v1.0 — RFC-0855p-b §Phase 4

## Status

Claimed (2026-09-03) by @mmacedoeu — partial land via RFC-0855p-e substrate (`HandoverReasonTypeId` in `octo-coordinator-types/src/lib.rs`, `Handover envelope` in `crates/octo-network/src/dot/handover.rs`). Missing: Phase 4 protocol logic + message preservation queue.

## RFC

RFC-0855p-b §Implementation Phase 4 (L827-833) + §Appendix A State Machine (Active → Handover, Handover → Inactive transitions) + RFC-0855p-e handover envelope substrate (already landed).

## Summary

Wire the §Phase 4 handover protocol on top of the partial RFC-0855p-e substrate + §Phase 1 + §Phase 3 state machine. Three trigger paths: (1) signed `HandoverRequest` (voluntary Active→Handover), (2) liveness-driven Suspect→Handover (from Phase 3), (3) emergency (governance override → Emergency→Handover). Each path produces a `HandoverRequestEnvelope`, propagates the message-pending queue to the successor, then transitions Handover→Inactive after successor Active.

**Layer-model note (intentional divergence from Phases 1/2/3/5):** RFC §Key Files L847-851 assigns the handover file to `crates/octo-network/src/dot/handover.rs` (Layer C, per RFC-0855p-e §Key Files). This Phase-4 mission therefore lives at **Layer C** and APPENDS handlers to the existing `crates/octo-network/src/dot/handover.rs` (not at Layer B). The companion Phase-1 state-machine substrate (`CoordinatorSource::Handover`, `CoordinatorRecord` for successors) IS at Layer B per Phase-1 mission; this Phase-4 mission consumes those types via `pub use` re-export from `crates/octo-network/src/mon/coordinator.rs`. Do NOT duplicate Phase 1 types here.

`MessagePreservationQueue` is the canonical RFC §Phase 4 L828 reference: every message destined for the predecessor-but-not-yet-handled queue MUST move to the successor before transition Handover→Inactive.

## Substrate work scope

### Step 1 — HandoverRequestEnvelope handler

Append to `crates/octo-network/src/dot/handover.rs`:
- `HandoverRequest` envelope (signed by departing coordinator).
- Voluntary path: existing coordinator signs + transitions Active→Handover.
- Forced path: liveness subsystem signals grace-period-exceeded → Handover.
- Emergency path: governance override (signed envelope from `governance_id`).

### Step 2 — Successor coordination + preservation queue

```rust
pub struct MessagePreservationQueue {
    pub predecessor: CoordinatorId,
    pub successor: CoordinatorId,
    pub pending_envelopes: Vec<(u64 /* sequence */, [u8; 32] /* envelope_digest */)>,
}
```

Transfer on HandoverRequest acknowledgment. Flush on successor Active.

### Step 3 — Canonical handover vectors

3 pinned vectors:
- Voluntary Active→Handover→Inactive (signed HandoverRequest + preservation queue ack).
- Forced Suspect→Handover (Phase 3 trigger).
- Emergency Emergency→Handover (governance override).

### Step 4 — Cross-mission coordination

`CoordinatorSource::Handover` (Phase 1 enum) consumes the `HandoverRequest` envelope and emits a fresh `CoordinatorRecord { source: Handover, ... }` for the successor.

## Acceptance Criteria

- [ ] `crates/octo-network/src/dot/handover.rs` extended with §Phase 4 handlers (voluntary + forced + emergency paths).
- [ ] `MessagePreservationQueue` declared; transfer + flush semantics.
- [ ] 3 canonical handover vectors pinned in `crates/octo-network/tests/canonical_handover_blobs.rs`; all PASS.
- [ ] Composition with `HandoverReasonTypeId` (Layer B event type, `octo-coordinator-types/src/lib.rs:171`).
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean.
- [ ] Reuses (does NOT duplicate) Layer B event types from `octo-coordinator-types` per [[cipherocto-design-principles]] §Stable Abstractions.

## Dependencies

**Requires:**
- RFC-0855p-b
- RFC-0855p-e (handover envelope substrate at `crates/octo-network/src/dot/handover.rs`)
- Mission `0855p-b-state-machine-types`
- Mission `0855p-b-mission-coordinator-liveness`

**Optional:**
- Mission `0855p-e-coordinator-types-shared-crate` (commit `93f1758c`; provides HandoverReasonTypeId)

## Version History

| Version | Date | Change |
| ------- | ---- | ------ |
| v1.0    | 2026-09-03 | Initial filing per RFC-0855p-b §Implementation Phase 4 + user audit (2026-09-03). Builds on partial RFC-0855p-e substrate (commits `93f1758c`, `b0f14119`). |
