---
name: 0855p-b-mission-coordinator-liveness
description: Mission Coordinator liveness check + heartbeat envelope per RFC-0855p-b §Implementation Phase 3. Land `CoordinatorHeartbeat` envelope (canonical wire form per RFC §Phase 3 L819-826) + per-coordinator epoch counter + Active→Suspect detection (`current_epoch - last_heartbeat_epoch > 2 × heartbeat_interval`) + Suspect→Active recovery + Suspect→Handover (grace period exceeded). Layer B canonical home `crates/octo-coordinator-types/src/liveness.rs` + re-export shim `crates/octo-network/src/mon/liveness.rs`. RFC-0008 Class A determinism (epoch-driven, monotonic-clock pinned). 3 canonical heartbeat vectors in `tests/canonical_liveness_blobs.rs`. BLOCKED by `0855p-b-state-machine-types` (Phase 1 CoordinatorLifecycle transitions).
metadata:
  node_type: substrate-coordinator
  type: substrate-liveness
  rfc_source: RFC-0855p-b
  rfc_section: Implementation Phases §Phase 3 (L819-826) + Heartbeat semantics (RFC §Appendix A mermaid Active→Suspect) + Round 3 adversarial review (L826)
  substrate_home:
    canonical: crates/octo-coordinator-types/src/liveness.rs
    re_export: crates/octo-network/src/mon/liveness.rs
    tests: crates/octo-coordinator-types/tests/canonical_liveness_blobs.rs
  execution_class: A
  deterministic: true
  depends_on:
    - RFC-0855p-b
    - RFC-0008
    - mission:0855p-b-state-machine-types
  blocks:
    - 0855p-b-mission-coordinator-handover
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-03
v: "1.0"
created: 2026-09-03
---

# Mission `0855p-b-mission-coordinator-liveness` v1.0 — RFC-0855p-b §Phase 3

## Status

Claimed (2026-09-03) by @mmacedoeu — blocked on `0855p-b-state-machine-types`. RFC-0855p-b §Implementation Phase 3 promises heartbeat envelope + Active→Suspect detection; **0 heartbeat substrate in `crates/`**.

## RFC

RFC-0855p-b §Implementation Phase 3 (L819-826) + §Appendix A State Machine (Active → Suspect / Suspect → Active / Suspect → Handover).

## Summary

Land heartbeat envelope + per-coordinator liveness tracker. Heartbeat is monotonic-clock-pinned; every `HEARTBEAT_INTERVAL` epochs, an active coordinator signs a `CoordinatorHeartbeat` envelope. Missed 2× heartbeat → transition `Active → Suspect`. Recovery on receipt → `Suspect → Active`. Grace period after Suspect → `Handover`. Tracker is per-coordinator keyed by `CoordinatorId`.

## Substrate work scope

### Step 1 — `CoordinatorHeartbeat` envelope

```rust
pub struct CoordinatorHeartbeat {
    pub coordinator_peer_id: CoordinatorId,
    pub coordinator_term_id: [u8; 32],
    pub current_epoch: u64,
    pub heartbeat_interval: u64,
    pub last_event_digest: [u8; 32], // BLAKE3 of canonical-bytes of last signed envelope
    pub signature: [u8; 64],
}
```

Borsh + Serialize. Domain separator `BLAKE3_REPUTATION_COORDINATOR_HEARTBEAT_DOMAIN = b"cipherocto/coordinator/heartbeat/v1"`.

### Step 2 — Liveness tracker

```rust
pub struct LivenessTracker { /* coordinator_peer_id → last_heartbeat_epoch + heartbeat_interval + grace_period */ }

pub fn evaluate_liveness(
    record: &CoordinatorRecord,
    current_epoch: u64,
    has_heartbeat: bool,
) -> CoordinatorLifecycle;
```

- `Active` + `(current_epoch - last_heartbeat_epoch <= heartbeat_interval)` → `Active`.
- `Active` + `(current_epoch - last_heartbeat_epoch > 2 × heartbeat_interval)` → `Suspect`.
- `Suspect` + heartbeat received → `Active`.
- `Suspect` + `(current_epoch - last_heartbeat_epoch > GRACE_PERIOD)` → `Handover`.

GRACE_PERIOD canonical constant = 3 × heartbeat_interval (per RFC §Appendix A mermaid implicit).

### Step 3 — Canonical vectors

3 pinned vectors in `tests/canonical_liveness_blobs.rs`:
- Active_OK (heartbeat within interval).
- Active_to_Suspect (2× interval miss).
- Suspect_to_Handover (grace exceeded).

## Acceptance Criteria

- [ ] `crates/octo-coordinator-types/src/liveness.rs` exists; `CoordinatorHeartbeat` + `LivenessTracker` + `evaluate_liveness`.
- [ ] All 3 §Phase 3 transitions correctly emit (Active→Suspect, Suspect→Active, Suspect→Handover).
- [ ] `pub const HEARTBEAT_INTERVAL_DEFAULT: u64` + `pub const HEARTBEAT_GRACE_MULTIPLIER: u64 = 3`.
- [ ] 3 canonical heartbeat vectors pinned; all PASS.
- [ ] `cargo clippy -p octo-coordinator-types --all-targets -- -D warnings` clean.
- [ ] Re-export shim at `crates/octo-network/src/mon/liveness.rs`.

## Dependencies

**Requires:**
- RFC-0855p-b
- RFC-0008
- Mission `0855p-b-state-machine-types` (Phase 1 — CoordinatorRecord + CoordinatorLifecycle)

## Cross-references

- RFC-0855p-b §Implementation Phase 3 (L819-826)
- RFC-0855p-b §Appendix A (L925-944)

## Version History

| Version | Date | Change |
| ------- | ---- | ------ |
| v1.0    | 2026-09-03 | Initial filing per RFC-0855p-b §Implementation Phase 3 + user audit (2026-09-03). |
