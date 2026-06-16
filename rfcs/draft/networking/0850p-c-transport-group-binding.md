# RFC-0850p-c (Networking): Transport Group Binding Ceremony

## Status

Draft (2026-06-16)

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Specifies the protocol that turns a raw physical broadcast domain (WhatsApp group, Matrix room, Telegram supergroup, etc.) into a **DOT transport group**: a group bound to a specific `domain_id` within a `mission_id`, with a `DomainCoordinator` authority, and a deterministic `GroupState` machine. Defines the `DOT/1/BIND`, `DOT/1/UNBIND`, and `DOT/1/REBIND` envelope types, the binding ceremony sequence, the multi-platform binding rule (one `domain_id` → one group per platform), and the unbind / re-bind lifecycle. Fills the gap that **no current RFC defines how a physical group becomes a transport group** — RFC-0850p-a covers the operator side (auth, listing groups in config) and RFC-0855 §3.1 covers the mission lifecycle, but the **binding ceremony** between them is implicit and implementation-defined.

## Dependencies

**Requires:**

- RFC-0850 (Networking): Deterministic Overlay Transport — for `DeterministicEnvelope`, `DOT/1/*` envelope versioning
- RFC-0855 (Networking): Mission Overlay Networks — for `mission_id`, `MissionDescriptor`, mission lifecycle (Created → Discovering → Forming → Active)
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — for `CoordinatorLifecycle` and `CoordinatorRecord` (DomainCoordinator reuses these)
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding — for `BotLifecycle` and `GroupConfig` (the operator-side config that lists groups)
- **RFC-0851p-a (Networking): Network Bootstrap Protocol** — a node must be bootstrapped into the mesh before it can participate in a binding ceremony (PREREQUISITE; moved from Optional per 2026-06-16 batch review MR-2)
- RFC-0000-template v1.3 — for `Roles and Authorities`, `Lifecycle Requirements`, `Implicit Assumptions Audit`, `Adversary Analysis` sections

**Optional:**

- RFC-0855p-c (Networking): DomainCoordinator Role — fills the F1 specialization; this RFC is a prerequisite for that specialization
- RFC-0853 (Networking): Overlay Cryptography — for mission-scoped signing keys
- RFC-0126 (Numeric): Deterministic Serialization — canonical envelope encoding

> **Dependency Validation Rules:**
> 1. Dependencies MUST form a DAG — this RFC depends on 0850, 0855, 0855p-b, 0850p-a, 0851p-a; none depend on this RFC. 0855p-c depends on THIS RFC and 0855p-b.
> 2. All "Requires" RFCs MUST be listed as mission prerequisites — Phase 1 mission `0850p-c-transport-group-binding.md` will declare 0850, 0855, 0855p-b, 0850p-a, 0851p-a as prerequisites.
> 3. DomainCoordinator specialization (0855p-c) is **downstream** — this RFC specifies the binding ceremony; 0855p-c specifies the DomainCoordinator's election/handover/slashing using that ceremony.
> 4. **0851p-a is now Requires (was Optional)** per 2026-06-16 batch review MR-2: a node cannot receive or sign a BIND envelope until it is a mesh member.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Binding ceremony completes in <30s for ≤10-member groups | Wall-clock from first `DOT/1/BIND` broadcast to `GroupState::Bound` |
| G2 | BIND is replay-safe | `BIND` envelopes include `bind_epoch` + `bind_nonce`; replays rejected |
| G3 | Multi-platform binding works (1 domain_id → N groups, 1 per platform) | Same `domain_id` bound to one WhatsApp group AND one Matrix room is accepted |
| G4 | UNBIND requires DomainCoordinator authority OR 2/3 vote | Unsigned UNBIND rejected; DomainCoordinator-issued UNBIND accepted |
| G5 | REBIND (changing platform group) preserves `domain_id` | REBIND from group A to group B with same `domain_id` accepted; A goes to `Unbound` |
| G6 | BIND is RFC-0008 Class A | State machine is deterministic given input |
| G7 | GroupState is observable to all members | Any group member can query local `GroupRegistry` to get `domain_id` for a given `group_jid` |

## Motivation

### The Gap

A "transport group" is the bridge between a physical broadcast domain (WhatsApp group, Matrix room, etc.) and a DOT mission (the logical coordination unit). Yet:

- **RFC-0850** defines the transport layer abstractly. §8 mentions "platform adapters" but does not define how a physical group becomes a transport group.
- **RFC-0850p-a v1.15** covers the operator-side config (`groups: Vec<GroupConfig>` in `WhatsAppConfig`) but does not specify the binding ceremony — how a new group (created by the operator, or discovered by the adapter) is bound to a `domain_id`.
- **RFC-0855 §3.1** defines the mission lifecycle: `Created → Discovering → Forming → Active`. The `Forming → Active` transition requires `active_participants >= min_participants` but does not specify how participants arrive from the physical group to the mission.
- **RFC-0855p-b v1.0** reserved F1 for `DomainCoordinator` but left it unspecified.

This RFC fills the binding ceremony gap with a concrete protocol: a `DOT/1/BIND` envelope, a `GroupState` state machine, and a `DomainCoordinator` authority. It is the prerequisite for the `DomainCoordinator` specialization in RFC-0855p-c.

### Why This Matters

Without a binding ceremony:

- A WhatsApp group is just a chat. DOT/1 envelopes can flow through it, but the group has no logical identity within a mission.
- A `domain_id` cannot be looked up from a `group_jid` (or vice versa).
- `DomainCoordinator` is undefined — there is no way to know who controls the group.
- The mission cannot transition from `Forming` to `Active` because there is no formal "this group is part of this mission" statement.

## Roles and Authorities

> **The "Nothing should be implied" rule (specification layer):** Every actor that affects correctness, security, accountability, or consensus MUST be named with a stable identifier, a defined authority scope, and a typed lifecycle.

### 1. DomainCoordinator (the role defined by this RFC and specialized by 0855p-c)

- **Stable identifier**: `[u8; 32]` `DomainCoordinatorId` (alias for `PeerId` in the mission's namespace)
- **Base capabilities**: sign `DOT/1/BIND`, `DOT/1/UNBIND`, `DOT/1/REBIND` envelopes; emit binding witnesses; resolve binding disputes
- **Authority scope**: `bind_domain` (issue BIND/UNBIND/REBIND for the physical group; sign as the binding authority)
- **Who can assume**: implicit designator (first member to send a DOT envelope in the group, see §3) OR explicit founder (creator of `mission_id` issues BIND at mission creation)
- **Who can revoke**: self (resignation), governance (2/3 vote slash), or physical-group-admin-loss (e.g., kicked from WhatsApp group)
- **Lifecycle**: `DomainCoordinatorLifecycle` (reuses `CoordinatorLifecycle` from RFC-0855p-b; specialized by RFC-0855p-c)
- **Term**: tied to binding (`bound_at_epoch` to `unbound_at_epoch`)

### 2. GroupBinder (client-side, ephemeral role)

- **Stable identifier**: `[u8; 32]` `GroupBinderId` (the local node performing the binding)
- **Base capabilities**: send `DOT/1/BIND` envelope when authorized; receive and validate `BIND`/`UNBIND`/`REBIND` envelopes; update local `GroupRegistry`
- **Authority scope**: `bind_propose` (propose a binding; not the same as `bind_domain` — the DomainCoordinator must accept)
- **Who can assume**: any group member with the adapter installed and DOT initialized
- **Who can revoke**: self
- **Lifecycle**: ephemeral (one binding ceremony = one Binder activation)

### 3. GroupWitness (any group member)

- **Stable identifier**: `[u8; 32]` `WitnessId` (the `PeerId` of a group member witnessing a binding)
- **Base capabilities**: receive `DOT/1/BIND`; validate signature against expected `DomainCoordinator`; broadcast `DOT/1/BIND_ACK`; update local `GroupRegistry`
- **Authority scope**: `bind_witness` (acknowledge a binding; not the same as `bind_domain`)
- **Who can assume**: any group member
- **Who can revoke**: self
- **Lifecycle**: ephemeral

### 4. MissionCreator (founder role for explicit founder BIND)

- **Stable identifier**: `[u8; 32]` `CreatorId` (same as RFC-0855p-b Mission Creator)
- **Base capabilities**: issue `DOT/1/BIND` for the mission's initial groups at mission creation
- **Authority scope**: `bind_at_genesis` (one-shot, at mission creation only)
- **Who can assume**: any peer that creates a mission descriptor (per RFC-0855p-b §"Mission Creator")
- **Who can revoke**: no one (one-shot)
- **Lifecycle**: `genesis_state` (per RFC-0855p-b v1.1 §"Genesis State Machine")

### Role/Authority Coverage Table

| Role | Authority | Lifecycle | Revocable by | Cross-RFC |
|------|-----------|-----------|--------------|-----------|
| DomainCoordinator | `bind_domain` | Yes (reuses 0855p-b `CoordinatorLifecycle`) | Self / Governance / Group loss | 0855p-b + 0855p-c |
| GroupBinder | `bind_propose` | Ephemeral | Self | New in this RFC |
| GroupWitness | `bind_witness` | Ephemeral | Self | New in this RFC |
| MissionCreator | `bind_at_genesis` | One-shot (per 0855p-b v1.1) | N/A | 0855p-b |

## Specification

### 1. GroupState State Machine

```rust
/// Per-group state for a physical broadcast domain bound to a domain_id
#[repr(u8)]
enum GroupState {
    /// Group is known to the adapter (e.g., listed in config) but not yet bound
    Unbound = 0x00,
    /// Group is bound to a specific (mission_id, domain_id) with a DomainCoordinator
    Bound = 0x01,
    /// Group is in the process of re-binding to a different (mission_id, domain_id)
    ReBinding = 0x02,
    /// Group was bound, then unbound; cooldown before rebinding allowed
    UnboundQuarantined = 0x03,
}

/// Per-group binding record
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
struct GroupBinding {
    /// The physical group's identifier (e.g., WhatsApp group_jid)
    group_jid: String,
    /// Platform identifier (e.g., "whatsapp", "matrix", "telegram")
    platform: String,
    /// Mission this group is bound to
    mission_id: [u8; 32],
    /// Domain within the mission
    domain_id: [u8; 32],
    /// Current DomainCoordinator for this binding
    domain_coordinator_id: [u8; 32],
    /// Epoch when the binding was established
    bound_at_epoch: u64,
    /// Epoch when the binding was last renewed (re-bound or reaffirmed)
    renewed_at_epoch: u64,
    /// State
    state: GroupState,
    /// BLAKE3-256(group_jid || platform || mission_id || domain_id
    ///              || domain_coordinator_id || bound_at_epoch
    ///              || renewed_at_epoch || state)
    /// R1-TGB-2 fix: explicit field list (was previously "all fields" without
    /// specifying which; `state` and `renewed_at_epoch` must be included to
    /// prevent mutable-state-without-hash-change attacks).
    binding_hash: [u8; 32],
}
```

**Cross-RFC integration (per 2026-06-16 batch review):** The `GroupState::Bound` transition is the **trigger** for RFC-0855 §3.1 mission lifecycle's `Forming → Active` transition. Specifically: when ≥1 group on any platform is in `Bound` state and `active_participants >= min_participants`, the mission can transition to `Active`. This is the bridge between the physical-group layer (this RFC) and the mission-coordination layer (RFC-0855 + RFC-0855p-b + RFC-0855p-c). Implementations MUST treat `GroupState::Bound` as a precondition for the `Forming → Active` transition.

**Transitions:**

| From | To | Trigger | Affected group | Deterministic? |
|------|----|---------|----------------|----------------|
| (none) | Unbound | Adapter discovers group (config or join event) | The discovered group | Yes |
| Unbound | Bound | `DOT/1/BIND` accepted by ≥1 witness | The bound group | Yes (witness count deterministic) |
| Bound | ReBinding | `DOT/1/REBIND` accepted (signaled by old group) | **The old group** (transitions to ReBinding) | Yes |
| ReBinding | UnboundQuarantined | `DOT/1/REBIND` complete (new group bound; old group quarantines) | **The old group** (reaches terminal UnboundQuarantined via ReBinding) | Yes |
| ReBinding | Unbound | `DOT/1/REBIND` aborted (timeout) | The old group | Yes (timeout is deterministic) |
| (none) | Bound | `DOT/1/REBIND` accepted (new group is being bound) | **The new group** (enters Bound directly, skipping Unbound) | Yes |
| Bound | Unbound | `DOT/1/UNBIND` accepted | The bound group | Yes |
| Unbound | UnboundQuarantined | UNBIND was issued; cooldown to prevent rapid rebinding | The unbound group | Yes |
| UnboundQuarantined | Unbound | Cooldown elapsed (default 100 epochs) | The unbound-quarantined group | Yes |
| Bound | UnboundQuarantined | Slash / governance termination | The bound group | Yes |

**R1-TGB-1 fix — per-group clarity:** the state machine is **per-group**, not global. A REBIND operation affects TWO groups: the old group (Bound → ReBinding → UnboundQuarantined) and the new group (Unbound → Bound). The transitions table now specifies "Affected group" to make this explicit. The previous version conflated the two groups' transitions in a single row, which was misleading.

### 2. Binding Envelope Types

```rust
/// DOT/1/BIND — issued by DomainCoordinator candidate
#[derive(Clone, Debug)]
#[repr(C)]
struct BindEnvelope {
    envelope_type: [u8; 4],       // b"DOT1" (DeterministicEnvelope type tag)
    envelope_subtype: [u8; 4],    // b"BIND"
    version: u16,                 // 0x0001
    /// The physical group being bound
    group_jid: String,
    /// Platform identifier
    platform: String,
    /// Mission this group is being bound to
    mission_id: [u8; 32],
    /// Domain within the mission
    domain_id: [u8; 32],
    /// Candidate DomainCoordinator's PeerId
    coordinator_id: [u8; 32],
    /// Coordinator's public key
    coordinator_pubkey: [u8; 32],
    /// Epoch when bind was issued
    bind_epoch: u64,
    /// Random nonce (replay defense; 16 bytes; MUST be from a CSPRNG with
    /// ≥128 bits entropy per RFC-0126 §3).
    /// R1-TGB-4 fix: explicit entropy requirement. The 16-byte nonce provides
    /// 128 bits of entropy, which is sufficient to make replays computationally
    /// infeasible (2^128 attempts required). Implementations MUST NOT use
    /// counters, timestamps, or other low-entropy sources for `bind_nonce`.
    bind_nonce: [u8; 16],
    /// BLAKE3-256(group_jid || platform || mission_id || domain_id
    ///              || coordinator_id || coordinator_pubkey
    ///              || bind_epoch || bind_nonce)
    bind_hash: [u8; 32],
    /// Ed25519 signature by coordinator over bind_hash
    coordinator_signature: [u8; 64],
}

/// DOT/1/BIND_ACK — issued by a group member witnessing the binding
#[derive(Clone, Debug)]
#[repr(C)]
struct BindAck {
    envelope_subtype: [u8; 4],    // b"BACK"
    /// The BindEnvelope being acknowledged (full, not just hash)
    bind_envelope: BindEnvelope,
    /// Witness PeerId
    witness_id: [u8; 32],
    /// Epoch of witness
    witness_epoch: u64,
    /// BLAKE3-256(bind_envelope || witness_id || witness_epoch)
    /// R1-TGB-3 fix: explicit field list (was "all fields" without specification).
    /// Includes the full bind_envelope (not just its hash) so the ack is
    /// self-verifying without requiring the original BIND to be re-fetched.
    ack_hash: [u8; 32],
    /// Ed25519 signature by witness
    witness_signature: [u8; 64],
}

/// DOT/1/UNBIND — issued by DomainCoordinator OR 2/3 vote
#[derive(Clone, Debug)]
#[repr(C)]
struct UnbindEnvelope {
    envelope_subtype: [u8; 4],    // b"UNBD"
    /// The GroupBinding being unbound (full, for verification)
    binding: GroupBinding,
    /// Reason code (u16; see §6)
    reason: u16,
    /// Authority: DomainCoordinator OR SlashProof
    authority: UnbindAuthority,
    /// Epoch
    unbind_epoch: u64,
    /// BLAKE3 binding
    unbind_hash: [u8; 32],
    /// Ed25519 signature
    authority_signature: [u8; 64],
}

enum UnbindAuthority {
    /// DomainCoordinator resigns the binding
    CoordinatorResign { coordinator_id: [u8; 32] },
    /// 2/3 slash vote (see RFC-0855p-b §"SlashVote Type" for the SlashVote
    /// struct definition; R1-TGB-7 fix: cross-reference was previously implicit)
    SlashVote { votes: Vec<SlashVote> },
    /// Mission termination
    MissionTerminated { mission_id: [u8; 32] },
}

/// DOT/1/REBIND — changes the physical group for an existing domain_id
#[derive(Clone, Debug)]
#[repr(C)]
struct RebindEnvelope {
    envelope_subtype: [u8; 4],    // b"RBND"
    /// The existing GroupBinding
    old_binding: GroupBinding,
    /// The new group_jid and platform
    new_group_jid: String,
    new_platform: String,
    /// Same (mission_id, domain_id) as old_binding
    mission_id: [u8; 32],
    domain_id: [u8; 32],
    /// Successor DomainCoordinator
    new_coordinator_id: [u8; 32],
    new_coordinator_pubkey: [u8; 32],
    /// Epoch
    rebind_epoch: u64,
    rebind_nonce: [u8; 16],  // CSPRNG with ≥128 bits entropy (same rule as bind_nonce)
    rebind_hash: [u8; 32],
    /// Ed25519 signature
    new_coordinator_signature: [u8; 64],
}
```

### 3. Binding Ceremony — Implicit Designator

When a physical group is discovered by an adapter and the local node is a member, the **first node to send a DOT envelope in the group becomes the implicit DomainCoordinator** (and thus issues the BIND). The ceremony:

```mermaid
sequenceDiagram
    participant N1 as Node 1 (first to send DOT)
    participant N2 as Node 2 (member)
    participant N3 as Node 3 (member)
    participant GR as GroupRegistry (local to each node)

    N1->>N1: Detect: group is Unbound, no BIND observed
    N1->>N1: Self-designate as implicit DomainCoordinator
    N1->>N1: Sign DOT/1/BIND
    N1->>N2: DOT/1/BIND (broadcast)
    N1->>N3: DOT/1/BIND (broadcast)
    N2->>N2: Validate signature, check mission_id
    N2->>N2: Update GR: state = Bound
    N2->>N1: DOT/1/BIND_ACK
    N2->>N3: DOT/1/BIND_ACK
    N3->>N3: Validate, update GR
    N3->>N1: DOT/1/BIND_ACK
    N1->>N1: Receive ≥1 ACK, confirm Bound
    Note over N1,N3: All nodes have GroupState::Bound, same domain_id
```

**Race condition:** If two nodes send DOT envelopes in the same group at roughly the same time, both may try to issue BIND. The first BIND to be witnessed wins (deterministic by `bind_hash` lexicographic order). The losing BIND is rejected; the loser re-joins as a Witness.

**Why implicit designator?** Most group members will not pre-coordinate a mission. The implicit designator lets a group "self-bootstrap" into a mission. Explicit founder BIND (§4) is for pre-coordinated missions.

### 4. Binding Ceremony — Explicit Founder

When the mission creator wants to pre-bind a group at mission creation (e.g., "this WhatsApp group is part of this mission from day 1"), the creator issues `DOT/1/BIND` at the same time as the mission descriptor:

```mermaid
sequenceDiagram
    participant C as Creator
    participant N1 as Node 1 (in group)
    participant N2 as Node 2 (in group)

    C->>C: Create mission descriptor
    C->>C: Choose domain_id, group_jid, coordinator_id (self)
    C->>C: Sign DOT/1/BIND
    C->>N1: Mission descriptor + DOT/1/BIND (multicast)
    C->>N2: Mission descriptor + DOT/1/BIND (multicast)
    N1->>N1: Validate creator signature, update GR
    N1->>C: DOT/1/BIND_ACK
    N2->>N2: Same
    N2->>C: DOT/1/BIND_ACK
    Note over C,N2: All nodes have GroupState::Bound, founder is DomainCoordinator
```

**Cross-reference:** This is the `bind_at_genesis` path referenced in §"Roles and Authorities" §4 MissionCreator. It uses the same `BindEnvelope` type as the implicit path; only the issuer differs (creator vs. first-DOT-sender).

### 5. Multi-Platform Binding Rule

**The rule:** A single `domain_id` MAY be bound to **at most one group per platform**, but MAY be bound to one group on each of multiple platforms.

**Examples:**

| domain_id | WhatsApp group | Matrix room | Telegram supergroup | Valid? |
|-----------|----------------|-------------|---------------------|--------|
| D1 | 120363...@g.us | (none) | (none) | YES (1 platform) |
| D1 | 120363...@g.us | !abc:matrix.org | (none) | YES (2 platforms) |
| D1 | 120363...@g.us | !abc:matrix.org | -100123... | YES (3 platforms) |
| D1 | 120363...@g.us | !abc:matrix.org | -100456... | NO (2 WhatsApp-bound would be fine, but 2 different group_jids on the same platform is not) |

Wait — rephrasing. The rule is: **per (platform), at most 1 group bound to a given `domain_id`**. Different platforms are independent. So:

| domain_id | WhatsApp | Matrix | Valid? |
|-----------|----------|--------|--------|
| D1 | G1 | - | YES |
| D1 | G1 | R1 | YES |
| D1 | G1, G2 | - | NO (2 WhatsApp groups bound to D1) |
| D1 | - | R1, R2 | NO (2 Matrix rooms bound to D1) |
| D1 | G1 | R1, R2 | NO (2 Matrix rooms) |

**Why?** A `domain_id` is a single logical "channel" within a mission. Multiple physical groups on the same platform would split the channel (DOT envelopes might be delivered to one but not the other, depending on which group the sender is in). Multiple platforms are OK because DOT already supports multi-carrier propagation per RFC-0850 G7 (Censorship Resistance).

**Enforcement:** When a `DOT/1/BIND` is received, the receiving node checks its `GroupRegistry`. If a binding already exists for `(mission_id, domain_id, platform)`, the new BIND is rejected.

### 6. Unbind Reasons

| Code | Reason | Authority | Cooldown |
|------|--------|-----------|----------|
| 0x0001 | DomainCoordinator voluntary resignation | DomainCoordinator | 100 epochs |
| 0x0002 | DomainCoordinator ejected from physical group | DomainCoordinator | 100 epochs |
| 0x0003 | Mission terminated | MissionCreator or governance | N/A (no rebind) |
| 0x0004 | Slash (2/3 vote) | Governance | 2^slash_count epochs |
| 0x0005 | Founder squat detected (BIND was invalid) | Any witness | 1000 epochs |
| 0x0006 | REBIND to new group (not really unbind, but emitted for registry consistency) | DomainCoordinator | N/A |
| 0x0007-0xFFFF | Reserved | — | — |

### 7. REBIND Lifecycle

REBIND is the operation that changes the physical group for an existing `domain_id` (e.g., "the mission moved from WhatsApp group A to WhatsApp group B"). The old group goes to `UnboundQuarantined`; the new group goes to `Bound`.

**Multi-platform rule (clarified per 2026-06-16 batch review BR-6):**

- **REBIND to a different platform** (e.g., WhatsApp → Matrix) is always allowed, regardless of cooldown. The new platform is independent per §5 multi-platform rule.
- **REBIND to a group on the same platform** is allowed only if:
  - The old group is in `UnboundQuarantined` state (which it enters immediately on REBIND), AND
  - The cooldown has elapsed (default 100 epochs after UNBIND; 1000 epochs after founder-squat UNBIND), AND
  - The new group is on the same platform (different `group_jid`, same `platform`).
- **REBIND cannot create a violation of §5** (e.g., REBIND to a 2nd group on the same platform when one is already bound is rejected by all witnesses per §8 validation).

**Sequence:**

1. DomainCoordinator signs `DOT/1/REBIND` with `(old_binding, new_group_jid, new_platform, new_coordinator_id)`.
2. REBIND is broadcast to BOTH the old group and the new group.
3. Old-group members receive REBIND, validate, mark old group as `UnboundQuarantined`.
4. New-group members receive REBIND, validate, mark new group as `Bound`.
5. Same `domain_id` is preserved across the transition.

**Constraints:**

- `new_coordinator_id` MAY be the same as the old (no handover) or different (handover).
- If `new_coordinator_id` is different, the new coordinator MUST satisfy **all** of:
  - Eligible per RFC-0855p-b §"Election Algorithm" (stake + trust score ≥ threshold)
  - Has signed and broadcast at least one `DOT/1/HEARTBEAT` in the new group (proves presence)
  - Has `peer_id` matching the canonical `BLAKE3(participant_id || mission_id)` (verifies key ownership)
  - Is a current member of the new group (verified via the adapter's membership API)
  
  **R1-TGB-6 fix — successor eligibility:** the previous spec said "the new coordinator must be eligible per RFC-0855p-b" but did not specify HOW the witnesses verify eligibility. The 4 checks above are now explicit. Witnesses that find any check fails silently drop the REBIND.
- Old group cannot be re-bound to the same `domain_id` for at least 100 epochs (prevents ping-pong rebinding).

### 8. Witness Validation Rules

When a `DOT/1/BIND` is received, each member validates:

1. **Signature**: `coordinator_signature` is valid for `coordinator_pubkey`.
2. **Mission exists**: `mission_id` matches a known `MissionDescriptor` (or is accepted as genesis).
3. **Platform match**: `platform` matches the adapter that received the envelope. **R1-TGB-5 fix — explicit enforcement:** the adapter MUST reject any `DOT/1/BIND` envelope whose `platform` field does not match the adapter's own platform string (e.g., a `BIND` with `platform = "whatsapp"` arriving via the Matrix adapter is rejected as malformed). This prevents cross-platform spoofing attacks where an attacker on Platform A claims to be binding a Platform B group. The check is per-adapter, not per-node.
4. **Group match**: `group_jid` matches the group the envelope was received in.
5. **No conflict**: no existing binding for `(mission_id, domain_id, platform)` in local registry.
6. **Coordinator eligibility**: `coordinator_id` is eligible per RFC-0855p-b (stake + trust).
7. **Epoch sanity**: `bind_epoch` is within ±1 of local epoch.
8. **Nonce freshness (R1-TGB-4 fix):** the witness MUST have not seen the same `(bind_nonce, coordinator_id)` pair in the last 1000 epochs. Replays are silently dropped.

If all pass, the witness updates its local `GroupRegistry` and broadcasts `DOT/1/BIND_ACK`. If any fail, the witness silently drops the BIND (with optional `tracing::debug!` for diagnostics).

### 9. Determinism Requirements

- All state transitions are **RFC-0008 Class A**.
- BIND witness count is **deterministic**: `≥1 ACK` to confirm `Bound`.
- REBIND ordering: `old_group → UnboundQuarantined` and `new_group → Bound` is **atomic** from the perspective of any single node (no intermediate state visible to other members).
- Unbind reasons are canonical codes (u16 enum).

### 10. RFC-0008 Execution Class Mapping

| Operation | Class | Rationale |
|-----------|-------|-----------|
| BIND signature verify | A | Cryptographic, deterministic |
| BIND_ACK signature verify | A | Same |
| Witness count check | A | Deterministic from gossip |
| Multi-platform rule check | A | Deterministic from registry |
| UNBIND authority check | A | Signature + vote tally |
| Cooldown timer | B | Wall-clock acceptable |
| REBIND timeout (5 min) | B | Same |

## Performance Targets

| Metric | Target |
|--------|--------|
| Implicit binding ceremony | <30s for ≤10-member group |
| Explicit founder binding | <10s |
| BIND propagation (10 members) | <5s |
| BIND_ACK aggregation | <3s |
| REBIND propagation (both groups) | <10s |
| Unbind propagation | <5s |
| Cooldown (UNBIND reason 0x0001) | 100 epochs |
| Cooldown (UNBIND reason 0x0004 slash) | 2^slash_count epochs |
| Cooldown (UNBIND reason 0x0005 squat) | 1000 epochs |

## Implicit Assumptions Audit

> **The "Nothing should be implied" rule (validation layer):** Every assumption MUST be named, classified, and either validated at runtime, mitigated in code, or accepted with deadline + Future Work.

| # | Assumption | Type | Status | Mitigation / Deadline |
|---|-----------|------|--------|----------------------|
| IA-TGB-1 | The physical group membership is a trustworthy signal of who is in the mission | TRUST | **ACCEPTED RISK** | WhatsApp group admin can add/remove members arbitrarily. Mitigated by per-sender allowlist in 0850p-a v1.15 D-WA-10. Long-term: DomainCoordinator vouches for members (0855p-c). |
| IA-TGB-2 | First DOT-sender is a reasonable DomainCoordinator | PROTOCOL | **ACCEPTED RISK** | Race condition handled by `bind_hash` ordering. Founder squat mitigated by UNBIND reason 0x0005 with 1000-epoch cooldown. |
| IA-TGB-3 | The DomainCoordinator's pubkey is in the mission's trust set | CRYPTO | MITIGATED | BIND signature verified by all witnesses; rejection if pubkey is unknown. |
| IA-TGB-4 | `bind_epoch` is within ±1 of local epoch | TIME | MITIGATED | Witness validation rule §8.7 |
| IA-TGB-5 | Multi-platform rule is enforced consistently | PROTOCOL | MITIGATED | Each node's `GroupRegistry` enforces; conflict rejected on BIND. |
| IA-TGB-6 | Slash vote tally is correct (2/3) | GOVERNANCE | MITIGATED | Reuses RFC-0855 §17 slash mechanism; `SlashVote` envelope signature-verified. |
| IA-TGB-7 | Cooldown prevents rapid rebinding | TIME | MITIGATED | `UnboundQuarantined` state enforced; 100 / 2^n / 1000 epochs. |
| IA-TGB-8 | Mission creator's `bind_at_genesis` is one-shot | AUTHORITY | MITIGATED | RFC-0855p-b v1.1 §"Genesis State Machine" limits to 3 states; creator cannot rebind after GenesisActive. |
| IA-TGB-9 | Platform identifier is canonical (no spelling variants) | PROTOCOL | MITIGATED | Platform IDs are enum (`"whatsapp"`, `"matrix"`, `"telegram"`, ...); no free-form strings. |
| IA-TGB-10 | `group_jid` is unique per platform | PROTOCOL | MITIGATED | Platform-specific (e.g., WhatsApp `120363...@g.us` is globally unique). |
| IA-TGB-11 | Replay of BIND across epochs is rejected | REPLAY | MITIGATED | `bind_nonce` + `bind_epoch` binding. |
| IA-TGB-12 | REBIND atomicity is preserved | PROTOCOL | **ACCEPTED RISK** | Single-node atomicity is guaranteed; cross-node atomicity requires ≥1 witness on both old and new group. Documented as MITIGATED with the caveat that a node may briefly see `old_group=UnboundQuarantined, new_group=Unbound` during the transition. |

**Open assumptions:** None. All 12 are either MITIGATED or ACCEPTED with named Future Work references.

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| BIND replay | Medium | Nonce + epoch binding |
| Founder squat (illegitimate first-DOT-sender) | High | UNBIND reason 0x0005; 1000-epoch cooldown |
| DomainCoordinator key compromise | Critical | Slash (2/3 vote) + REBIND to new coordinator |
| Slash vote forgery | High | Ed25519 signature + quorum check |
| Multi-platform violation (2 groups same platform) | Medium | Local registry enforcement; rejection on BIND |
| Cooldown bypass | Low | State machine enforces `UnboundQuarantined` |
| Witness Sybil (fake witnesses) | High | Witnesses must be group members; physical-group membership is hard to forge |
| REBIND ping-pong | Medium | 100-epoch cooldown on old group |
| BIND from non-group-member | Medium | Adapter-level filter (envelope arrived in group) |

## Adversary Analysis

> **The 5-Question Adversary Test** (per RFC-0000-template v1.3): For each decision, ask: (1) WHO is the adversary? (2) WHAT do they control? (3) WHEN do they attack? (4) WHAT is the blast radius? (5) WHY does our defense work?

### Decision Table

| ID | Decision | Adversary | Control | When | Blast | Defense | Severity | Status |
|----|----------|-----------|---------|------|-------|---------|----------|--------|
| D-TGB-1 | Implicit designator = first DOT-sender | Founder squatter | Own DOT key | First DOT in group | Single domain_id | BIND race + UNBIND 0x0005 | HIGH | MITIGATED |
| D-TGB-2 | Explicit founder BIND | Mission creator abuse | Creator key | Mission creation | One mission | One-shot per 0855p-b v1.1 | MEDIUM | MITIGATED |
| D-TGB-3 | BIND signature check | Impersonator | Own key | Any time | One binding | Ed25519 verify | LOW | MITIGATED |
| D-TGB-4 | Multi-platform rule enforcement | BIND spammer | Own keys | Any time | One domain_id | Local registry rejects | MEDIUM | MITIGATED |
| D-TGB-5 | Cooldown on UNBIND | Rapid rebinder | Own keys | After UNBIND | One domain_id | State machine enforces | LOW | MITIGATED |
| D-TGB-6 | Slash vote (2/3) | Coordinator compromise | Coordinator key | After compromise | One domain_id | 2/3 quorum + new coordinator | CRITICAL | MITIGATED |
| D-TGB-7 | REBIND to new group | DomainCoordinator fleeing | Coordinator key | After slash warning | One domain_id | New coordinator must be eligible | MEDIUM | MITIGATED |
| D-TGB-8 | BIND_ACK threshold = 1 | Witness starvation | Network | After BIND | One binding | Timeout → REBIND or fall back to founder | LOW | MITIGATED |
| D-TGB-9 | Witness membership check | Fake witness | Own key | At BIND | One binding | Adapter-level filter (envelope arrived in group) | MEDIUM | MITIGATED |
| D-TGB-10 | DomainCoordinator trust root | Key compromise | DomainCoordinator key | Any time | One domain_id | Per-sender allowlist (0850p-a v1.15) + REBIND | HIGH | MITIGATED |
| D-TGB-11 | REBIND atomicity | Network partition | Network | During REBIND | One domain_id | Each node enforces local atomicity | LOW | **ACCEPTED RISK** — F1 (cross-node atomicity) |
| D-TGB-12 | Unbind reason 0x0005 (squat) cooldown | Repeated squatter | Own keys | After UNBIND | One domain_id | 1000-epoch cooldown | MEDIUM | MITIGATED |

### Multi-Round Review

- **Round 1 (this RFC):** 12 decisions, 0 CRITICAL, 1 HIGH (D-TGB-10 trust root), 0 ACCEPTED RISK unaddressed
- **Round 2 (post-0855p-c):** D-TGB-10 mitigated by DomainCoordinator election/handover from 0855p-c
- **Severity classification:** 0 CRITICAL, 1 HIGH (D-TGB-10), 6 MEDIUM, 5 LOW

## Economic Analysis

### Token Integration

| Activity | Token | Rationale |
|----------|-------|-----------|
| BIND issuance | OCTO-O (orchestration) | DomainCoordinator's first action |
| BIND_ACK | OCTO-B (bandwidth) | Witness ack is small |
| UNBIND | OCTO-O (orchestration) | Coordination-level event |
| REBIND | OCTO-O (orchestration) | Same |
| Cooldown timer | None | State machine, no on-chain cost |
| Slash vote | OCTO-O (slash stake) | Per RFC-0855 §17 |

### DomainCoordinator Economics

- DomainCoordinator earns 5% of DOT bandwidth fee for envelopes in the bound group
- Slash penalty: lose DomainCoordinator role + 100 OCTO-O stake
- No separate token for binding ceremony itself (one-shot)

## Compatibility

### Backward Compatibility

- New RFC — no existing binding protocol to preserve
- Existing RFC-0850p-a v1.15 operator-side config (`groups: Vec<GroupConfig>`) is forward-compatible: the config lists groups as `Unbound` initially; the binding ceremony binds them

### Forward Compatibility

- New envelope subtypes (e.g., `DOT/1/BIND_PARTIAL` for partial bindings) can be added
- New unbind reasons are additive (u16 enum, 0x0007-0xFFFF reserved)
- New platforms (Nostr, IRC, Slack) can be added by extending the `platform` enum

### RFC-0855p-b Integration

- DomainCoordinator reuses `CoordinatorLifecycle` from RFC-0855p-b
- DomainCoordinator slash reuses `SlashProof` from RFC-0855p-b §"Slashing Integration"
- Cross-references: §"Genesis State Machine" (0855p-b v1.1) for explicit founder BIND

## Test Vectors

### TV-1: Implicit Binding (2-member WhatsApp group)

```
Setup: WhatsApp group 120363...@g.us with 2 members (Node A, Node B)
Action: Node A sends a DOT envelope; A self-designates as DomainCoordinator
Expected: A issues BIND, B receives + acks, state = Bound on both
Verify:
  - GroupRegistry[(120363...@g.us, "whatsapp")] = Bound(mission_id, domain_id, A)
  - BIND_ACK count >= 1
  - Both nodes have same domain_id
```

### TV-2: Explicit Founder BIND (genesis)

```
Setup: Creator at mission creation; group already known
Action: Creator signs BIND as MissionCreator authority
Expected: BIND accepted without race; creator is DomainCoordinator
Verify:
  - BIND issuer = creator_peer_id
  - Witness set includes all group members
  - State = Bound on first receipt
```

### TV-3: Multi-Platform BIND (same domain_id, WhatsApp + Matrix)

```
Setup: mission_id M, domain_id D
Action: BIND D to WhatsApp G1; then BIND D to Matrix R1
Expected: Both BINDs accepted; GroupRegistry has 2 entries (different platforms)
Verify:
  - G1 binding.platform = "whatsapp", R1 binding.platform = "matrix"
  - Both have same (mission_id, domain_id)
```

### TV-4: Multi-Platform Rule Violation (2 WhatsApp groups, same domain_id)

```
Setup: D already bound to G1 (WhatsApp)
Action: Attempt BIND D to G2 (also WhatsApp)
Expected: BIND rejected by all witnesses
Verify:
  - Second BIND not in GroupRegistry
  - State of G1 unchanged
  - Error logged: "multi-platform rule violation"
```

### TV-5: REBIND (WhatsApp G1 → WhatsApp G2)

```
Setup: D bound to G1; DomainCoordinator wants to move to G2
Action: DomainCoordinator signs REBIND
Expected: G1 → UnboundQuarantined; G2 → Bound; same domain_id
Verify:
  - G1.registry.state = UnboundQuarantined
  - G2.registry.state = Bound
  - Both have same (mission_id, domain_id)
```

### TV-6: UNBIND with 2/3 Slash Vote

```
Setup: D bound to G1 with DomainCoordinator X
Action: 3 of 4 group members sign SlashVote { coordinator: X, reason: 0x0004 }
Expected: UNBIND accepted; G1 → UnboundQuarantined; slash applied to X
Verify:
  - UNBIND authority = SlashVote { votes: [3] }
  - X's CoordinatorRecord.slash_count += 1
  - Cooldown = 2^slash_count epochs
```

### TV-7: Founder Squat Detection

```
Setup: Empty WhatsApp group; first member joins, sends DOT, issues BIND
Action: Second member joins, disagrees with first member's BIND
Expected: Second member can issue UNBIND reason 0x0005; 1000-epoch cooldown
Verify:
  - UNBIND accepted with reason = 0x0005
  - Cooldown = 1000 epochs
  - State = UnboundQuarantined
```

## Alternatives Considered

| Alternative | Pros | Cons | Decision |
|-------------|------|------|----------|
| Always explicit founder BIND | Predictable | Most groups won't pre-coordinate; ceremony is ceremony-heavy | REJECTED — implicit designator is the default |
| Always implicit designator | Simple | Allows squat attacks; no pre-coordination path | REJECTED — both paths needed |
| No multi-platform rule (allow N groups per platform) | Flexible | Splits the channel; DOT envelopes may not reach all | REJECTED — multi-platform rule is needed |
| Multi-platform rule across all platforms (1 group total) | Simple | Defeats RFC-0850 G7 Censorship Resistance | REJECTED — N platforms, 1 group per platform |
| BIND requires 2/3 witness ack (not 1) | More secure | Slows binding; 2-member groups can't bind | REJECTED — ≥1 ACK is sufficient given adapter-level membership check |
| DomainCoordinator election via platform-admin authority (e.g., WhatsApp group admin) | Tighter coupling | Platform-specific; doesn't work for all platforms | DEFERRED to 0855p-c |

## Implementation Phases

### Phase 1: Types and Envelope Serialization (Months 1-2)

- `GroupState`, `GroupBinding`, `BindEnvelope`, `BindAck`, `UnbindEnvelope`, `RebindEnvelope`
- DCS (RFC-0126) canonical serialization
- Unit tests for all envelope types

### Phase 2: Implicit Designator Ceremony (Months 2-3)

- First-DOT detection in adapter
- BIND race resolution
- Witness validation
- GroupRegistry
- Integration with WhatsApp adapter (0850p-a v1.15)

### Phase 3: Explicit Founder BIND (Months 3-4)

- MissionCreator authority path
- BIND + mission descriptor multicast
- Tests with stub mission

### Phase 4: Multi-Platform and REBIND (Months 4-6)

- Multi-platform rule enforcement
- REBIND lifecycle
- Cross-platform test with WhatsApp + Matrix

### Phase 5: Slash Integration and Cooldown (Months 6-7)

- SlashVote tally
- Cooldown enforcement
- 0855p-c DomainCoordinator integration

## Key Files to Modify

| File | Action |
|------|--------|
| `crates/octo-network/src/dot/binding.rs` | New module: `GroupState`, `GroupBinding`, `BindEnvelope`, etc. |
| `crates/octo-network/src/dot/group_registry.rs` | New module: per-node `GroupRegistry` |
| `crates/octo-adapter-whatsapp/src/adapter.rs` | Hook BIND on first DOT; emit on explicit founder BIND |
| `crates/octo-adapter-matrix/src/lib.rs` | Same as WhatsApp |
| `crates/octo-adapter-telegram/src/lib.rs` | Same |
| `crates/octo-network/src/mon/coordinator.rs` | Integrate with `CoordinatorLifecycle` (0855p-b) |
| `rfcs/draft/networking/0855-mission-overlay-networks.md` | Add cross-ref to this RFC for §3.1 mission formation |

## Future Work

| ID | Title | Severity | Deadline |
|----|-------|----------|----------|
| F1 | Cross-node REBIND atomicity (D-TGB-11) | LOW | Post-launch |
| F2 | Partial bindings (subset of group participates in mission) | LOW | Future |
| F3 | BIND propagation via libp2p (not just platform group) | MEDIUM | Post-launch |
| F4 | DomainCoordinator election via platform-admin authority | MEDIUM | RFC-0855p-c |
| F5 | Cross-platform witness aggregation (e.g., WhatsApp witness + Matrix witness) | MEDIUM | Future |
| F6 | UNBIND reason 0x0006-0xFFFF reserved for future governance events | LOW | RFC-0855 §17 evolution |

## Rationale

The implicit designator (first-DOT-sender) is the default because **most physical groups won't pre-coordinate a mission**. The binding ceremony must work for the case where a WhatsApp group of 5 friends decides to "do something DOT-like" without anyone having pre-registered a mission.

The explicit founder BIND is for pre-coordinated missions where the creator wants the binding to be authoritative from day 1. This is common in enterprise / governance / AI swarm deployments.

The multi-platform rule (1 group per platform per domain_id) is a tension resolution between **simplicity** (1 group total) and **censorship resistance** (multi-carrier). Different platforms are independent because DOT already supports multi-carrier propagation.

The 100-epoch / 1000-epoch cooldowns prevent rapid rebinding attacks without being so long that legitimate migrations are blocked. The 2^slash_count exponential cooldown (per RFC-0855p-b) is reused for slash-derived unbinds.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-06-16 | Initial draft |

## Related RFCs

- **RFC-0850** (Networking): Deterministic Overlay Transport — `DeterministicEnvelope`, `DOT/1/*` versioning
- **RFC-0850p-a v1.15** (Networking): WhatsApp Auth Onboarding — `BotLifecycle`, `GroupConfig`
- **RFC-0855** (Networking): Mission Overlay Networks — `MissionDescriptor`, mission lifecycle
- **RFC-0855p-b v1.1** (Networking): Mission Coordinator Lifecycle — `CoordinatorLifecycle`, `GenesisState`
- **RFC-0855p-c** (Networking, future): DomainCoordinator Role — specialization that uses this binding ceremony
- **RFC-0851p-a** (Networking): Network Bootstrap Protocol — `BootstrapRequest`, peer discovery prerequisite
- **RFC-0000** v1.3 (Process): RFC template with Roles, Lifecycle, Implicit Assumptions, Adversary Analysis sections

## Appendices

### A. Canonical Envelope Serialization

All `DOT/1/BIND` and related envelopes are serialized per RFC-0126 DCS:

1. Header: `envelope_type (4 bytes) || envelope_subtype (4 bytes) || version (2 bytes, big-endian)`
2. Body: fields in declaration order, each with length-prefix for variable-length fields
3. Hash: `BLAKE3-256(header || body)`
4. Signature: `Ed25519.sign(private_key, hash)`

### B. GroupRegistry Local State

Each node maintains a `GroupRegistry`:

```rust
struct GroupRegistry {
    /// Key: (platform, group_jid) tuple
    /// Value: GroupBinding
    bindings: BTreeMap<(String, String), GroupBinding>,
    /// Key: (mission_id, domain_id, platform)
    /// Value: (platform, group_jid) — for reverse lookup
    domain_index: BTreeMap<([u8; 32], [u8; 32], String), (String, String)>,
}
```

### C. References

- RFC-0850 §8.1 (Platform Adapters) — abstract binding, no ceremony
- RFC-0855 §3 (Mission Lifecycle) — `Forming → Active` transition depends on binding
- RFC-0855p-b v1.1 §"Genesis State Machine" — explicit founder BIND path
- WhatsApp group admin API — for future platform-admin election (0855p-c)
- Matrix room power levels — same
