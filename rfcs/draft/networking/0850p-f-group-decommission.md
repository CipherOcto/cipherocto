# RFC-0850p-f (Networking): Transport Group Decommission

## Status

Draft (2026-06-17) — early stage; main scenarios to be elaborated in next iteration

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Specifies how a DomainCoordinator (DC), MissionController, MissionCreator, or governance vote can **decommission a transport group** (UNBIND_ALL), with platform-side leave / dissolve, post-decommission audit log, and platform-side leave race handling. The basic `DOT/1/UNBIND_ALL` envelope is defined in RFC-0850p-d §F; this RFC elaborates the lifecycle, audit trail, platform-side choreography, and edge cases (DC rotation, partial decommission, leave race with new members). Closes scenario family **S-G7** (group decommission) from `docs/research/networking-rfc-cross-reference-analysis.md`.

## Dependencies

- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `GroupBinding`, `GroupState`
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — `UNBIND_ALL` envelope (defined here, refined here)
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — DC rotation
- RFC-0126 (Numeric): DCS

## Design Goals (preliminary)

1. **Deterministic decommission.** All nodes MUST agree on the decommission of a group via ≥ 1 witness ACK.
2. **Platform-side leave race handling.** If a new member joins the group during the decommission window, the DC MUST re-include them in the UNBIND_ALL.
3. **Audit trail.** A signed decommission receipt is stored per node for forensic analysis.
4. **DC rotation support.** A DC can resign mid-decommission; the new DC inherits the decommission state.

## Motivation

RFC-0850p-d §F defines the basic `UNBIND_ALL` flow but does not address:
- DC rotation during decommission (the DC may resign or be slashed)
- Platform-side leave race (a new member joins while UNBIND_ALL is in flight)
- Audit trail (who initiated, when, why, witness count)
- Re-decommission (a group that was decommissioned but is somehow still bound on some nodes)

This RFC closes the gap.

## Status (Detailed)

This RFC is in early-stage draft. The basic UNBIND_ALL envelope is defined in RFC-0850p-d §F. This RFC will elaborate:

- Detailed state machine for `UnboundAllPending` and `UnboundAllDone` states
- DC rotation semantics during UNBIND_ALL
- Platform-side leave race handling
- Audit trail structure
- Re-decommission (a node that missed the original UNBIND_ALL)
- Quorum semantics: 1 witness ACK vs. N-of-M witness quorum

### v0.2 elaboration (2026-07-30)

F-1 through F-6 elaborated below in §"Specification (v0.2)".

- **F-1 (Quorum)**: chosen policy = **1 witness ACK** (`UNBIND_ALL_MIN_ACKS = 1`). Rationale: trade-off favors availability over absolute safety for decommission (which is a defensive action, not a state-changing authority transfer). See §"Quorum Semantics".
- **F-2 (DC rotation)**: a DC resigning mid-decommission hands over to the successor via the existing `HandoverRequestEnvelope` (RFC-0855p-e §"Handover"); the `SlashTally` carries the in-flight `UnboundAllPending` state. See §"DC Rotation During UNBIND_ALL".
- **F-3 (Leave race)**: new members joining during the `UnboundAllPending` window MUST be re-included in the broadcast; the DC tracks `pending_members: BTreeSet<[u8; 32]>` and rebroadcasts if the set grows mid-window. See §"Leave Race Window".
- **F-4 (Audit trail)**: `UnbindAllAuditEnvelope` carries `initiator_id`, `reason`, `reason_text`, `initiated_at_epoch`, `completed_at_epoch`, `witness_count`, `unbind_hash`, `nonce`. Local `AuditLog` is FIFO-evicted at 1024 entries. See §"Audit Trail".
- **F-5 (Re-decommission)**: a node that missed the original UNBIND_ALL may receive a *new* `UnbindAllEnvelope` carrying `original_nonce: Option<[u8; 32]>` set to the original `nonce`. The receiver verifies the nonce matches the locally-stored `pending_nonce` (kept until quorum reached or quarantine window expires). See §"Re-decommission".
- **F-6 (Platform permissions)**: if the DC lacks admin rights on the platform, the group can only be **left** by members (not dissolved). The DC emits `UnbindAllEnvelope::reason = CoordinatorResign` and members individually `leave_group`. See §"Platform-side Dissolve Permissions".

## Use Case Link

- `docs/use-cases/social-platform-transport-layer.md` — "Group Lifecycle" section
- `docs/use-cases/mission-coordinator-lifecycle.md` — "DC Resignation" section

## Specification (preliminary)

### Envelope Types Added

| Envelope Type | Subtype tag | Direction | Description |
|---------------|-------------|-----------|-------------|
| `DOT/1/UNBIND_ALL` | `b"UALL"` | Authority → mesh (broadcast) | (Defined in RFC-0850p-d §F) |
| `DOT/1/UNBIND_ALL_ACK` | `b"UAAC"` | Witness → Authority | (Defined in RFC-0850p-d §F) |
| `DOT/1/UNBIND_ALL_DONE` | `b"UADN"` | Authority → mesh (broadcast) | Final confirmation; all members have left |
| `DOT/1/UNBIND_ALL_AUDIT` | `b"UAAU"` | Authority → audit log (out-of-band) | Signed audit entry |

All envelopes use the canonical 10-byte header per RFC-0850p-c §A: `envelope_type = b"DOT1"`, the per-envelope subtype tag from the table above, `version = u16 // 0x0001`. (R16 R1-C1 fix: migrated from the 1-byte subtype + 1-byte version stub in the v0.1 draft; the canonical format is the 4-byte ASCII + `u16` form per RFC-0850p-c.)

> **Note on `b"UADN"` and `b"UAAU"`:** these subtype tags are NEW (allocated by this RFC). The full struct definitions (`UnbindAllDoneEnvelope`, `UnbindAllAuditEnvelope`) will be added in the next iteration of this RFC (current Version 0.1 is an early-stage stub). For now, this RFC claims the subtype tags so future iterations do not conflict.

### State Machine (preliminary)

| State | Value | Description |
|-------|-------|-------------|
| `UnboundAllPending` | 0x06 | UNBIND_ALL broadcast; awaiting ACK from all members |
| `UnboundAllDone` | 0x07 | All members have left the platform; group is fully decommissioned |

### State Machine transitions (v0.2)

```
                                  ┌─────────────────────┐
                                  │  GroupState::Bound  │
                                  └──────────┬──────────┘
                                             │
                                  transition_to_unbound_all_pending
                                             │
                                             ▼
                       ┌─────────────────────────────────────┐
                       │ GroupState::UnboundAllPending (0x06) │
                       └──────────────┬──────────────────────┘
                                      │
              transition_to_unbound_all_done        transition_to_unbound_quarantined
              (all witnesses ACK)                  (timeout / witness rejection / DC unreachable)
                          │                                    │
                          ▼                                    ▼
        ┌──────────────────────────────────┐   ┌──────────────────────────────────┐
        │ GroupState::UnboundAllDone (0x07)│   │ GroupState::UnboundQuarantined   │
        │ (terminal — binding removed)     │   │ (0x03; recoverable via REJOIN)    │
        └──────────────────────────────────┘   └──────────────────────────────────┘
```

Transitions are explicit methods on `GroupRegistry` (`crates/octo-network/src/dot/group_registry.rs`):

- `transition_to_unbound_all_pending` (line 543): source = `Bound | Inviting | ReBinding | UnboundAllPending` (idempotent re-broadcast). Target = `UnboundAllPending`.
- `transition_to_unbound_all_done` (line 573): source = `UnboundAllPending`. Removes the binding + `domain_index` entry. Returns `UnbindEnvelope` for ceremony.
- `transition_to_unbound_quarantined` (line 237, RFC-0850p-e §"unbound_quarantine"): the failure path. Moves binding into the quarantine map with `REJOIN_GRANT_TIMEOUT = 50` epochs recovery window.

### Quorum Semantics (v0.2 — F-1)

**Policy**: `UNBIND_ALL_MIN_ACKS = 1`. A single distinct witness ACK suffices to advance from `UnboundAllPending` to `UnboundAllDone`.

**Rationale** (1 vs N-of-M trade-off):

| Option | Latency | Safety | When |
|---|---|---|---|
| 1 ACK | fastest (1 RTT) | relies on at least 1 honest witness | chosen |
| N-of-M | slower (waits for N) | safer under witness collusion | not chosen |

**Why 1 is sufficient for decommission**: UNBIND_ALL is a *defensive* action (the group is unsafe; §SafetyShutdown, §MassKick, etc.). False-negatives (no decommission when needed) are more harmful than false-positives (decommission a healthy group — which is recoverable via REBIND). The audit trail (F-4) provides post-hoc accountability for any misuse.

**Implementation** (`crates/octo-network/src/dot/dc.rs`):

```rust
pub const UNBIND_ALL_MIN_ACKS: u32 = 1;
```

`UnbindAllAckCollector` (decommission.rs, added in v0.2) tracks unique `witness_id`s. `is_quorum_reached() -> bool` returns `true` when `count(distinct witness_id) >= UNBIND_ALL_MIN_ACKS`.

### DC Rotation During UNBIND_ALL (v0.2 — F-2)

If the DC resigns (or is slashed) mid-decommission:

1. The current DC emits a `HandoverRequestEnvelope` (RFC-0855p-e §"Handover") with `HandoverReason::CoordinatorTermLimit` or `HandoverReason::CoordinatorSlashed`.
2. The `SlashTally` carried in the handover envelope (handover.rs:218) MUST include the in-flight `UnboundAllPending` state (the original `unbind_hash`, the `pending_members` set, the `initiated_at_epoch`).
3. The successor DC takes over the in-flight `UnbindAllEnvelope` and continues collecting ACKs against the *original* `unbind_hash` (no rebroadcast required).
4. The successor's `coordinator_term_id` is included in the new `UnbindAllAckEnvelope.witness_epoch` so witnesses can validate the term transition.

**Edge case**: if the successor DC is unreachable, the in-flight state stays `UnboundAllPending` until `UNBIND_ALL_TIMEOUT_EPOCHS = 100` (configurable) elapses, at which point the binding transitions to `UnboundQuarantined`.

### Leave Race Window (v0.2 — F-3)

Race: a new member joins the group (via `InviteEnvelope` ACK) while `UnboundAllPending`.

**Policy**: the new member MUST be re-included in the active UNBIND_ALL.

**Mechanism**:

1. The DC maintains `pending_members: BTreeSet<[u8; 32]>` per in-flight decommission.
2. On a new `InviteAckEnvelope`, the DC checks if `(platform, group_jid)` is in `pending_members` for any in-flight `UnboundAllPending`. If yes, the new member is added to the pending set AND a fresh `UnbindAllEnvelope` is rebroadcast with the same `unbind_hash` (NOT a new nonce — the nonce is stable per decommission ceremony).
3. Witnesses who already ACK'd the original broadcast re-ACK with the same `unbind_hash`; duplicate ACKs are deduplicated by `witness_id` in `UnbindAllAckCollector`.

**Race window**: `pending_members` is updated atomically with each `InviteAck`. The window is bounded by the `UNBIND_ALL_TIMEOUT_EPOCHS = 100` constant.

### Audit Trail (v0.2 — F-4)

`UnbindAllAuditEnvelope` (subtype `b"UAAU"`, `decommission.rs:121`) carries:

| Field | Type | Description |
|---|---|---|
| `domain_id` | `[u8; 32]` | Domain identifier (the `(mission_id, domain_id)` triple) |
| `group_jid` | `String` | Platform-specific group identifier |
| `platform` | `String` | Platform string (`whatsapp`, `signal`, etc.) |
| `initiator_id` | `[u8; 32]` | Public key of the DC that initiated the UNBIND_ALL |
| `reason` | `UnbindReason` | One of `Scheduled`, `MassKick`, `MissionTerminated`, `CoordinatorResign`, `SafetyShutdown` |
| `reason_text` | `String` | Free-form reason (UTF-8) |
| `initiated_at_epoch` | `u64` | Epoch when UNBIND_ALL was issued |
| `completed_at_epoch` | `u64` | Epoch when decommission completed (or aborted) |
| `witness_count` | `u32` | Number of witness ACKs collected |
| `unbind_hash` | `[u8; 32]` | Correlation: the `unbind_hash` of the original `UnbindAllEnvelope` |
| `nonce` | `[u8; 32]` | 32-byte random nonce (R17 R1-HIGH-1 fix: replay protection) |
| `audit_hash` | `[u8; 32]` | `BLAKE3-256(header \|\| body)` |
| `signature` | `[u8; 64]` | Ed25519 over `audit_hash` |

**Local audit log** (`AuditLog`, `decommission.rs:215`): in-memory `BTreeMap<u64, AuditEntry>` bounded at 1024 entries by FIFO eviction. Persistent rotation (disk-backed) is deferred to a future iteration (see "Deferred to v0.3" below).

**Lookup helpers**: `iter()` (chronological), `find_by_domain(domain_id)` (filtered by `domain_id`), `get(seq)`.

### Re-decommission (v0.2 — F-5)

A node that missed the original `UnbindAllEnvelope` (offline, network partition) may still be in `GroupState::Bound` while the rest of the mesh has transitioned to `UnboundAllDone`.

**Flow**:

1. The node reconnects and receives the `UnbindAllAuditEnvelope` (subtype `b"UAAU"`).
2. The audit envelope's `unbind_hash` references an `UnbindAllEnvelope` the node never saw.
3. The node emits a *follow-up* `UnbindAllEnvelope` with `nonce` = the original `nonce` (carried in the audit envelope's `nonce` field) so witnesses can correlate.
4. Witnesses who already saw the original replay their original ACK against the follow-up; `UnbindAllAckCollector` dedups by `witness_id`.
5. Once quorum (1 ACK) reached, the late node transitions `Bound → UnboundAllPending → UnboundAllDone` with no additional ceremony.

**Implementation** (`crates/octo-network/src/dot/dc.rs:281` — `build_unbind_all`):

```rust
pub fn build_unbind_all(
    &mut self,
    domain_id: [u8; 32],
    group_jid: String,
    platform: String,
    binding_hash: [u8; 32],
    reason: UnbindReason,
    current_epoch: u64,
    coordinator_term_id: u64,
    original_nonce: Option<[u8; 32]>,  // F-5: re-decommission carry-forward
) -> UnbindAllEnvelope { ... }
```

`original_nonce = None` for a fresh decommission; `Some(orig)` for re-decommission. The envelope's `nonce` field is set to `original_nonce.unwrap_or_else(|| self.fresh_nonce())`.

### Platform-side Dissolve Permissions (v0.2 — F-6)

If the DC lacks admin rights on the platform (e.g., the WhatsApp group was created by a member who is not the DC):

- The DC cannot **dissolve** the group. It can only request all members to **leave**.
- The `UnbindAllEnvelope` is emitted with `reason = CoordinatorResign` and the `platform_dissolve: false` flag (reserved for future use).
- Each member individually calls `leave_group` on the platform; their local transition to `UnboundAllDone` waits for the platform-side leave receipt.
- The audit entry records `dissolve_succeeded: false` (reserved field) so operators can distinguish "DC dissolved" from "members left individually".

### Deferred to v0.3

- **Disk-backed audit log rotation**: time-based + size-based rotation; NDJSON per file.
- **Quorum policy switch**: `UNBIND_ALL_MIN_ACKS` is currently a constant; making it governance-configurable requires RFC-0850p-f v0.3 + cross-RFC review.

## Future Work (specific)

- **F-1: Quorum semantics.** Define whether UNBIND_ALL needs 1 witness ACK (current proposal) or N-of-M witness quorum. Trade-off: 1 ACK is faster but less safe; N-of-M is safer but slower.
- **F-2: DC rotation during UNBIND_ALL.** If the DC resigns mid-decommission, the new DC must inherit the UNBIND_ALL state. Specify the handover protocol.
- **F-3: Platform-side leave race.** If a new member joins while UNBIND_ALL is in flight, the new member is added to the UNBIND_ALL recipient list. Specify the race window.
- **F-4: Audit trail.** Define the structure of the signed audit entry (who, when, why, witness count, group_jid).
- **F-5: Re-decommission.** A node that missed the original UNBIND_ALL may still have a `Bound` state. Specify the re-decommission flow.
- **F-6: Platform-side dissolve permissions.** If the DC does not have admin rights on the platform, the group cannot be dissolved (only left). Specify the policy.

## Rationale (preliminary)

This RFC is in early-stage draft. The basic UNBIND_ALL flow is captured in RFC-0850p-d §F. This RFC will elaborate the lifecycle, audit trail, and edge cases once the base mission (`0850p-c-base.md`) and DC-initiated group creation mission (`0850p-d-dc-initiated-group-creation.md`) are complete.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-06-17 | Initial stub; main spec to be elaborated |
| 0.2 | 2026-06-17 | R16 R1 fix: (C1) migrated Envelope Types Added table to use 4-byte ASCII subtype tags (`b"UALL"`, `b"UAAC"`, `b"UADN"`, `b"UAAU"`) per RFC-0850p-c §A canonical 10-byte header; the new subtype tags `b"UADN"` and `b"UAAU"` are claimed by this RFC to prevent future conflicts. |
| 0.3 | 2026-07-30 | F-1..F-6 elaborated: quorum = 1 ACK; DC rotation handover; leave race window + `pending_members`; audit envelope fields + local log; re-decommission `original_nonce` carry-forward; platform-side dissolve permissions. Disk-backed audit log rotation deferred to v0.4. |

## Related RFCs

- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite (sister RFC, co-implemented)
- RFC-0850p-e (Networking): Kick & Platform Membership Change Detection (sister RFC, co-implemented)
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle

## Related Use Cases

- `docs/use-cases/social-platform-transport-layer.md`
- `docs/use-cases/mission-coordinator-lifecycle.md`
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-G7

---

**Version:** 0.3
**Submission Date:** 2026-06-17
**Last Updated:** 2026-07-30
