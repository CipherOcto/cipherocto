# RFC-0855p-e (Networking): HandoverRequest Envelope & Coordinator Term Handover

## Status

Draft (2026-06-17) — early stage; main spec to be elaborated in next iteration

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Specifies the `DOT/1/HANDOVER_REQUEST` envelope type and the coordinator term handover ceremony. A coordinator (MissionCoordinator, DomainCoordinator, or WitnessCoordinator) may initiate a handover to a successor at any time via this signed envelope. The handover includes the current term, slash tally, group bindings, and pending envelopes. Closes scenario family **S-C4** (coordinator term handover with full state) from `docs/research/networking-rfc-cross-reference-analysis.md`. Builds on RFC-0855p-b §5a (slash tally) and RFC-0855p-c (DC authority).

## Dependencies

- RFC-0850 (Networking): Deterministic Overlay Transport
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — slash tally
- RFC-0855p-c (Networking): DomainCoordinator Role — DC authority
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `GroupBinding`
- RFC-0126 (Numeric): DCS

## Design Goals (preliminary)

1. **Atomic handover.** The handover MUST be atomic across the mesh: either all nodes see the new term, or all nodes see the old term.
2. **Slash tally inclusion.** The HANDOVER_REQUEST MUST include the current slash tally (per RFC-0855p-b §5a) so the successor inherits the slashes.
3. **Group binding transfer.** All `GroupBinding`s for the current term MUST be transferred to the successor.
4. **Witness-validated.** A HANDOVER_REQUEST requires ≥ 1 witness ACK to be valid.
5. **Slash protection.** A coordinator that is `Suspect` or `Demoting` CANNOT initiate a HANDOVER_REQUEST (the slash is enforced first).

## Motivation

RFC-0855p-b §3 defines the coordinator lifecycle (Designated → Elected → Active → Suspect → Handover → Demoting → Resigned → Inactive) but does not specify the envelope type or the state transfer for the `Active → Handover` transition. This RFC defines the envelope and ceremony.

## Status (Detailed)

This RFC is in early-stage draft. The basic HANDOVER_REQUEST envelope is sketched below. A future iteration will elaborate:
- Detailed state machine for `Handover` and `HandoverComplete` states
- Slash tally serialization (per RFC-0855p-b §5a)
- Group binding transfer (per RFC-0850p-c)
- Witness ACK aggregation
- Handover race handling (two coordinators handing over simultaneously)
- Handover revocation (the new coordinator is slashed; the old coordinator is restored)
- Quorum semantics

## Use Case Link

- `docs/use-cases/mission-coordinator-lifecycle.md` — "Coordinator Handover" section
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-C4

## Specification (preliminary)

### Envelope Type Added

| Envelope Type | Subtype tag | Direction | Description |
|---------------|-------------|-----------|-------------|
| `DOT/1/HANDOVER_REQUEST` | `b"HORQ"` | Coordinator → mesh (broadcast) | Coordinator initiates handover to a successor |
| `DOT/1/HANDOVER_ACK` | `b"HOAK"` | Witness → mesh (broadcast) | Witness confirms the HANDOVER_REQUEST |
| `DOT/1/HANDOVER_DONE` | `b"HODN"` | New Coordinator → mesh (broadcast) | New coordinator confirms receipt and acceptance |

All envelopes use the canonical 10-byte header per RFC-0850p-c §A: `envelope_type = b"DOT1"`, the per-envelope subtype tag from the table above, `version = u16 // 0x0001`.

### Data Structure (preliminary)

```rust
/// Coordinator handover request (DOT/1/HANDOVER_REQUEST).
/// (R16 R1-C1 fix: migrated from 1-byte subtype + 1-byte version stub to the
///  canonical 10-byte header per RFC-0850p-c §A.)
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct HandoverRequestEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"HORQ" (HANDOVER_REQUEST)
    pub version: u16,                   // 0x0001
    pub coordinator_id: [u8; 32],       // current coordinator's peer_id
    pub successor_id: [u8; 32],         // proposed successor's peer_id
    pub coordinator_role: CoordinatorRole,    // inlined below (R16 R1-L3 fix)
    pub current_term_id: [u8; 32],
    pub new_term_id: [u8; 32],          // proposed new term
    pub slash_tally: SlashTally,        // see "SlashTally struct" below (R16 R1-H5 fix)
    pub group_bindings: Vec<GroupBinding>,    // per RFC-0850p-c
    pub pending_envelopes_hash: [u8; 32],     // BLAKE3 hash of pending envelopes to be transferred
    pub reason: HandoverReason,         // Voluntary, Scheduled, Suspect, Demoting, etc.
    pub nonce: [u8; 16],
    pub current_epoch: u64,
    pub signature: [u8; 64],
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub enum HandoverReason {
    Voluntary         = 0x00,
    Scheduled         = 0x01,  // e.g., term limit reached
    Suspect           = 0x02,  // coordinator failed heartbeat
    Demoting          = 0x03,  // coordinator was slashed; forced handover
    MissionTerminated = 0x04,
}

/// Coordinator role. (R16 R1-L3 fix: this enum was previously referenced as
///  "CoordinatorRole" but not defined. It is inlined here.)
/// Roles are: Mission Coordinator (RFC-0855p-b), Domain Coordinator (RFC-0855p-c),
/// Witness Coordinator (RFC-0855p-b §4).
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CoordinatorRole {
    MissionCoordinator  = 0x00,
    DomainCoordinator   = 0x01,
    WitnessCoordinator  = 0x02,
}

/// Slash tally, inlined here (R16 R1-H5 fix: the previous version referenced
///  "RFC-0855p-b §5a" and "RFC-0855p-b.1 amendment" — but RFC-0855p-b.1 does
///  NOT exist. The SlashTally struct is inlined here. Future versions may
///  consolidate this with RFC-0855p-b.)
///
/// The slash tally is per-coordinator (a coordinator's tally aggregates all
/// slash events it has witnessed/been a party to). On handover, the tally is
/// transferred to the successor so the new coordinator continues enforcement.
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SlashTally {
    pub slash_events: Vec<SlashEvent>,
    pub last_updated_epoch: u64,
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SlashEvent {
    pub slash_reason_code: u16,        // per RFC-0855p-b §B code space 0x0001-0xFFFF
    pub slashed_peer_id: [u8; 32],
    pub witness_count: u16,            // number of witness signatures collected
    pub slash_evidence_hash: [u8; 32], // BLAKE3 of evidence envelope chain
    pub epoch: u64,                    // epoch when slash was applied
    pub signature: [u8; 64],           // coordinator's signature over this event
}
```

## Future Work (specific)

- **F-1: Slash tally serialization.** Specify the DCS-canonical serialization of `SlashTally` (per RFC-0855p-b §5a).
- **F-2: Group binding transfer.** Specify the protocol for the new coordinator to take ownership of all `GroupBinding`s from the old coordinator.
- **F-3: Handover race handling.** If two coordinators HANDOVER_REQUEST simultaneously, the lexicographic comparison on `coordinator_id` is the tiebreak.
- **F-4: Handover revocation.** Specify the protocol for a new coordinator to be slashed; the old coordinator is restored.
- **F-5: Witness ACK aggregation.** Define the quorum for HANDOVER_REQUEST (≥ 1 witness ACK vs. N-of-M).
- **F-6: Pending envelope transfer.** Specify how pending envelopes in the old coordinator's queue are transferred to the new coordinator (or replayed on the mesh).
- **F-7: Slash tally amendment.** Slash tally is inlined into this RFC's "SlashTally struct" definition above (R16 R1-H5 fix: the previous version referenced a non-existent "RFC-0855p-b.1 amendment"). Future versions may deprecate the inline definition in favor of a consolidated SlashTally in RFC-0855p-b.

## Rationale (preliminary)

This RFC is in early-stage draft. The basic HANDOVER_REQUEST envelope is sketched. A future iteration will elaborate the lifecycle, witness aggregation, and handover race handling.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-06-17 | Initial stub; main spec to be elaborated |
| 0.2 | 2026-06-17 | R16 R1 fix: (C1) migrated `HandoverRequestEnvelope` struct from 1-byte subtype + 1-byte version stub to the canonical 10-byte header per RFC-0850p-c §A (subtype tag `b"HORQ"`, `version: u16 // 0x0001`); (H5) inlined the `SlashTally` and `SlashEvent` structs (the previous version referenced non-existent RFC-0855p-b.1); (L3) inlined the `CoordinatorRole` enum (was referenced but not defined); removed reference to non-existent RFC-0855p-b.1 in Related RFCs. |

## Related RFCs

- RFC-0850 (Networking): Deterministic Overlay Transport
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle (slash tally source)
- RFC-0855p-c (Networking): DomainCoordinator Role
- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0008 (Cross-cutting): Number Registry (for slash reason code space — used in inline SlashEvent struct)

> **Note (R16 R1-H5 fix):** an earlier version of this RFC listed "RFC-0855p-b.1 (Networking): Coordinator Term + Slash Tally Amendment (planned, related)" in this section. RFC-0855p-b.1 does NOT exist. Slash tally is inlined in this RFC instead.

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — "Coordinator Handover"
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-C4

---

**Version:** 0.1
**Submission Date:** 2026-06-17
**Last Updated:** 2026-06-17
