# Use Case: DOT Mission Coordinator Lifecycle

**Date:** 2026-06-16
**Status:** Draft

---

## Problem

CipherOcto's DOT (Deterministic Overlay Transport) network supports **mission overlay networks** (RFC-0855): groups of nodes that coordinate on a shared goal (e.g., a relay network, a content distribution cluster, a tokenized agent economy). Each mission has one or more **coordinators** that drive the mission's lifecycle (formation, election, active operation, demotion).

Currently, the coordinator role is only described at a high level in RFC-0855 §3 "Mission Lifecycle" and §11 "Governance Models". The actual state machine (Designated → Elected → Active → Suspect → Handover → Demoting → Resigned → Inactive), per-governance-model election algorithms, slash reason codes, handover semantics, and liveness checks are not specified. Without these, no implementation is possible.

**Two specialized coordinator roles exist:**

1. **Mission Coordinator** (RFC-0855p-b) — generic coordinator for any mission type. Defines the state machine, election, handover, slashing, and liveness check.

2. **DomainCoordinator** (RFC-0855p-c) — specialization of Mission Coordinator for physical broadcast domains (WhatsApp groups, Telegram supergroups, Matrix rooms). The DomainCoordinator's authority comes from being the platform's group admin (per RFC-0850p-c §5 "Multi-Platform Binding Rule").

## Stakeholders

- **Primary:** CipherOcto node operators who run mission coordinators
- **Secondary:** Mission participants (members, witnesses) who vote and slash
- **Affected:** Platform admins (e.g., WhatsApp group admins) whose authority is bridged to DOT via the DomainCoordinator

## Motivation

### Why Coordinator Lifecycle Matters

A mission's coordinator is the **root of authority** for that mission. The coordinator:

- Signs envelopes that drive mission state
- Decides who is admitted (BIND)
- Decides who is removed (UNBIND)
- Proposes slash votes (subject to witness approval)
- Hands over to a successor (Handover)

If the coordinator is misbehaving, slashes are possible. If the coordinator goes offline, liveness checks and handover are needed. If the coordinator's key is compromised, key rotation is needed.

Without a clear spec, each implementation would invent its own state machine, leading to interop failures.

### Why a Specialized DomainCoordinator

A mission on a physical platform (WhatsApp, Telegram) needs a coordinator that:

1. **Holds the platform's admin authority.** A non-admin cannot drive a WhatsApp group.
2. **Inherits the platform's admin lifecycle.** If the platform admin is demoted on WhatsApp, the DomainCoordinator loses authority automatically.
3. **Bridges to the DOT protocol.** The DomainCoordinator signs DOT envelopes (BIND, REBIND, UNBIND) on behalf of the mission.

The DomainCoordinator is a specialization of the generic Mission Coordinator. It reuses the state machine, election, handover, and slashing — but adds platform-specific states and admin authority checks.

## Success Metrics

| Metric | Target | Measurement |
| ------ | ------ | ----------- |
| Mission formation time | < 5 minutes | From genesis BIND to mission `Active` state |
| Coordinator election time | < 2 minutes | From election trigger to `Elected` state |
| Handover time | < 60 seconds | From Handover trigger to successor `Active` |
| Slash finalization time | < 5 minutes | From slash vote to coordinator `Demoting` |
| Liveness false-positive rate | < 1% | Heartbeat checks that incorrectly trigger `Suspect` |
| Slash false-positive rate | < 0.1% | Slash votes that are rejected on appeal |

## Constraints

- **Must not:** Allow a coordinator to act without being in the `Active` state.
- **Must not:** Allow slash without 2/3 witness majority.
- **Limited to:** The 5 governance models defined in RFC-0855 §11 (Centralized, DAO, Federated, AI-Assisted, Autonomous).
- **Limited to:** The slash reason codes defined in RFC-0855p-b §B "Slash Offense Codes" (0x0001-0x0009 slash-only, 0x000A-0x000B transport-level, 0x000C-0xFFFF reserved for future).

## Non-Goals

- **Not in scope:** A new governance model beyond the 5 in RFC-0855 §11. (Adding a model is a new RFC.)
- **Not in scope:** New slash reason codes beyond the reserved range (0x000C-0xFFFF). (Adding a code is an extension of §B.)
- **Not in scope:** Cross-chain coordination. (Mission coordinators operate within a single DOT mesh.)
- **Not in scope:** Human-readable coordinator identification (display names). Coordinators are identified by `peer_id` (libp2p).

## Impact

If this use case is implemented:

1. **Mission formation is deterministic.** Every mission has a well-defined coordinator from genesis.
2. **Coordinator failure is recoverable.** Handover, slash, and demotion are all specified.
3. **Cross-platform missions are supported.** A DomainCoordinator can be elected for a multi-platform mission (per RFC-0850p-c §5 "Multi-Platform Binding Rule").
4. **Byzantine coordinators can be slashed.** 2/3 witness majority ensures collusion is required.

## Related RFCs

- RFC-0855: Mission Overlay Networks — primary; §3 "Mission Lifecycle", §11 "Governance Models", §16.3 "Coordinator State" (forward-reference; filled by 0855p-b)
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — `CoordinatorLifecycle` state machine, election, handover, slashing
- RFC-0855p-c (Networking): DomainCoordinator Role — specialization for physical platforms
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `domain_id` and BIND/REBIND/UNBIND envelope types
- RFC-0850: Deterministic Overlay Transport — base protocol

## Related Use Cases

- [Social Platform Transport Layer](social-platform-transport-layer.md) — DomainCoordinator is the bridge between DOT and physical platforms
- [Decentralized Mission Execution](decentralized-mission-execution.md) — Missions are the unit of coordination in CipherOcto

## Pipeline Position

```
Use Case (Mission Coordinator Lifecycle — this document)
   │
   ▼
RFC-0855: Mission Overlay Networks
   │
   ▼
RFC-0855p-b (Networking): Mission Coordinator Lifecycle (general state machine)
   │
   ▼
RFC-0855p-c (Networking): DomainCoordinator Role (platform specialization)
   │
   ▼
Missions: 0855p-b-{cross-mission-reputation, vdf-election, ...}
Missions: 0855p-c-{cross-platform-consensus, admin-attestation, cross-domain-slash, ...}
```

## Related Missions

Under RFC-0855p-b:
- `missions/open/0855p-b-cross-mission-reputation.md` — Cross-mission coordinator reputation
- `missions/open/0855p-b-vdf-election.md` — VDF-based election (random beacon)
- `missions/open/0855p-b-stake-weighted-quadratic.md` — Anti-plutocracy voting weight
- `missions/open/0855p-b-governance-rfc.md` — Governance key rotation RFC (0855p-d)

Under RFC-0855p-c:
- `missions/open/0855p-c-cross-platform-consensus.md` — 2-phase commit for cross-platform REBIND
- `missions/open/0855p-c-admin-attestation.md` — Platform admin verification via API attestations
- `missions/open/0855p-c-cross-domain-slash.md` — Cross-domain slash via mission-level coordinator
- `missions/open/0855p-c-slash-small-groups.md` — Slash instead of UNBIND for small groups
- `missions/open/0855p-c-sub-admins.md` — Sub-admin designations for redundancy
- `missions/open/0855p-c-reputation.md` — DC reputation (slash history across domains)
- `missions/open/0855p-c-auto-rejoin.md` — Auto-rejoin for accidentally kicked members

---

**Category:** Networking
**Priority:** Critical (mission-level)
**RFCs:** RFC-0855, RFC-0855p-b, RFC-0855p-c
**Status:** Defined → Mission phase
