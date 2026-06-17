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

| Envelope Type | Subtype | Direction | Description |
|---------------|---------|-----------|-------------|
| `DOT/1/HANDOVER_REQUEST` | 0x30 | Coordinator → mesh (broadcast) | Coordinator initiates handover to a successor |
| `DOT/1/HANDOVER_ACK` | 0x31 | Witness → mesh (broadcast) | Witness confirms the HANDOVER_REQUEST |
| `DOT/1/HANDOVER_DONE` | 0x32 | New Coordinator → mesh (broadcast) | New coordinator confirms receipt and acceptance |

### Data Structure (preliminary)

```rust
/// Coordinator handover request (DOT/1/HANDOVER_REQUEST).
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct HandoverRequestEnvelope {
    pub envelope_subtype: u8,        // 0x30
    pub version: u8,                 // 0x01
    pub coordinator_id: [u8; 32],     // current coordinator's peer_id
    pub successor_id: [u8; 32],      // proposed successor's peer_id
    pub coordinator_role: CoordinatorRole,  // MissionCoordinator, DomainCoordinator, WitnessCoordinator
    pub current_term_id: [u8; 32],
    pub new_term_id: [u8; 32],       // proposed new term
    pub slash_tally: SlashTally,     // per RFC-0855p-b §5a
    pub group_bindings: Vec<GroupBinding>,  // per RFC-0850p-c
    pub pending_envelopes_hash: [u8; 32],   // BLAKE3 hash of pending envelopes to be transferred
    pub reason: HandoverReason,      // Voluntary, Scheduled, Suspect, Demoting, etc.
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
```

## Future Work (specific)

- **F-1: Slash tally serialization.** Specify the DCS-canonical serialization of `SlashTally` (per RFC-0855p-b §5a).
- **F-2: Group binding transfer.** Specify the protocol for the new coordinator to take ownership of all `GroupBinding`s from the old coordinator.
- **F-3: Handover race handling.** If two coordinators HANDOVER_REQUEST simultaneously, the lexicographic comparison on `coordinator_id` is the tiebreak.
- **F-4: Handover revocation.** Specify the protocol for a new coordinator to be slashed; the old coordinator is restored.
- **F-5: Witness ACK aggregation.** Define the quorum for HANDOVER_REQUEST (≥ 1 witness ACK vs. N-of-M).
- **F-6: Pending envelope transfer.** Specify how pending envelopes in the old coordinator's queue are transferred to the new coordinator (or replayed on the mesh).
- **F-7: Slash tally amendment.** RFC-0855p-b.1 amendment: add a slash tally section to HANDOVER_REQUEST to prevent tally loss on handover.

## Rationale (preliminary)

This RFC is in early-stage draft. The basic HANDOVER_REQUEST envelope is sketched. A future iteration will elaborate the lifecycle, witness aggregation, and handover race handling.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-06-17 | Initial stub; main spec to be elaborated |

## Related RFCs

- RFC-0850 (Networking): Deterministic Overlay Transport
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle
- RFC-0855p-b.1 (Networking): Coordinator Term + Slash Tally Amendment (planned, related)
- RFC-0855p-c (Networking): DomainCoordinator Role
- RFC-0850p-c (Networking): Transport Group Binding Ceremony

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — "Coordinator Handover"
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-C4

---

**Version:** 0.1
**Submission Date:** 2026-06-17
**Last Updated:** 2026-06-17
