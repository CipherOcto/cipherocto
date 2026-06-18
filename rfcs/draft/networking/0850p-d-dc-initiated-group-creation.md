# RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite

## Status

Draft (2026-06-17)

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Specifies the envelope types, state machine extensions, and ceremony flows for a DomainCoordinator (DC) to create a new physical transport group (e.g., WhatsApp / Matrix / Telegram), optionally invite initial members, and atomically bind the new group to a logical `domain_id`. Closes scenario families **S-G1** (DC creates new group), **S-G2** (DC issues platform-side invite), **S-G3** (founder race resolution), **S-G5** (atomic CREATE+REBIND), and **S-G6** (third-party group BIND) identified in `docs/research/networking-rfc-cross-reference-analysis.md`. Complements RFC-0850p-c, which assumes the physical group already exists.

## Dependencies

**Required (must be Accepted or in Active Implementation):**

- RFC-0850 (Networking): Deterministic Overlay Transport — `DeterministicEnvelope`, `DOT/1/*` versioning
- RFC-0855 (Networking): Mission Overlay Networks — `mission_id`, `MissionDescriptor`
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — `CoordinatorLifecycle`, `CoordinatorRecord`
- RFC-0855p-c (Networking): DomainCoordinator Role — DC authority and capabilities
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding — `GroupConfig`, `BotLifecycle`
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `GroupBinding`, `GroupState`, BIND / REBIND / UNBIND envelopes
- RFC-0851p-a (Networking): Network Bootstrap Protocol — node must be bootstrapped before participating in a CGROUP
- RFC-0126 (Numeric): DCS — canonical serialization

**Refines / Extends:** RFC-0850p-c §1 (GroupState) and §3 (envelope types) and RFC-0855p-c §5a (DC envelope types).

## Design Goals

1. **Deterministic creation.** Two DCs creating the same `domain_id` at the same time MUST resolve to a single winner (no split-brain) using a deterministic tiebreak.
2. **Platform-neutral.** The protocol semantics MUST be identical across WhatsApp, Matrix, Telegram; only the adapter implementation differs.
3. **Atomic group provisioning.** A DC MUST be able to create a new group and bind it to a `domain_id` in a single transaction observable across the mesh. (R16 R1-L1 fix: the previous wording "Atomic CREATE+REBIND" conflated group creation with rebinding — "CREATE+REBIND" is a specific operation (create new + rebind old, per §C), while the general goal of "create and bind in one transaction" is broader. Renamed to "Atomic group provisioning"; the specific CREATE+REBIND mechanism is in §C "Atomic Migration via CREATE+REBIND".)
4. **Third-party group BIND.** A DC MUST be able to bind a pre-existing group (not created by this DC) to a `domain_id`, with a witness asserting the platform-side membership claim.
5. **Replay safety.** All CGROUP / INVITE / UNBIND_ALL envelopes MUST be nonce-protected per RFC-0850p-c §8 to prevent replay attacks.
6. **Squad continuity.** If the DC is removed from the new group before BIND completes, the binding MUST abort and the group MUST be quarantined per RFC-0850p-c §1 (`UnboundQuarantined` state).

## Motivation

RFC-0850p-c §1 and RFC-0855p-c §5a define a 4-state `GroupState` machine (`Unbound`, `Bound`, `ReBinding`, `UnboundQuarantined`) and 5 envelope types (BIND, BIND_ACK, REBIND, UNBIND, PlatformLoss). All of these assume the physical group already exists. The E2E test plan `docs/e2e/2026-06-16-e2e-test-plan.md` identifies scenarios IS-3.1 through IS-3.6 (founder race, DC creation flow, atomic CREATE+REBIND, third-party group binding, invite issuance, decommission) that are NOT covered by RFC-0850p-c. Without this RFC, the DC can only bind to groups that humans created manually, which is incompatible with the "MissionOverlay" use case (`docs/use-cases/social-platform-transport-layer.md`) where the DC drives the entire ceremony.

### Use Case Link

- `docs/use-cases/social-platform-transport-layer.md` — "DC-Initiated Group Creation" and "Invite Issuance" sections
- `docs/use-cases/mission-coordinator-lifecycle.md` — "DC Authority" (grants the DC the right to create groups on the platform)
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario families S-G1, S-G2, S-G3, S-G5, S-G6

## Roles and Authorities

| Role | Authority for CGROUP | Authority for INVITE | Authority for UNBIND_ALL |
|------|----------------------|----------------------|--------------------------|
| **DomainCoordinator (DC)** | Yes (primary) | Yes (primary) | Yes (primary) |
| **Witness** | No | No | No (witnesses only ACK UNBIND_ALL) |
| **MissionCreator** | No (may delegate sub-DC authority to a sub-coordinator for a specific sub-domain via RFC-0855p-d §Sub-DC delegation, out of scope for this RFC) | No | Yes (supersedes DC) |
| **MissionController** | No | No | Yes (supersedes DC) |
| **Governance** | No | No | Yes (governance vote per RFC-0855p-b §"Slashing Adjudicator") |

The DC is the only role that can sign a CGROUP. INVITE is restricted to the DC of the bound domain. UNBIND_ALL can be initiated by the DC, MissionCreator, MissionController, or a governance vote.

(R16 R1-M1 fix: the previous wording "can delegate via slash 0x000C" and "slash 0x000D" used the slash-code space loosely for delegation and governance-override. The mechanisms are NOT slash reason codes — delegation is a sub-DC authorization envelope (RFC-0855p-d) and governance override is a 2/3 governance vote (RFC-0855p-b). The slash reason codes 0x000C-0x000D are reserved for future allocation and are NOT consumed by these authority checks.)

## Specification

### Envelope Types Added

| Envelope Type | Subtype tag | Direction | Description |
|---------------|-------------|-----------|-------------|
| `DOT/1/CGROUP` | `b"CGRO"` | DC → mesh (broadcast) | Request to create a new physical group and bind to `domain_id` |
| `DOT/1/CGROUP_ACK` | `b"CGAC"` | Witness → DC | Witness confirms seeing the CGROUP and reserving the `domain_id` |
| `DOT/1/CGROUP_DONE` | `b"CGDA"` | DC → mesh (broadcast) | DC confirms the platform-side group was created, includes `group_jid` |
| `DOT/1/CGROUP_FAIL` | `b"CGFA"` | DC → mesh (broadcast) | DC failed to create the group on the platform; includes reason code |
| `DOT/1/INVITE` | `b"INVT"` | DC → single recipient (out-of-band) | One-shot signed invite to join a specific physical group |
| `DOT/1/UNBIND_ALL` | `b"UALL"` | Authority → mesh (broadcast) | Decommission a physical group bound to `domain_id` |
| `DOT/1/UNBIND_ALL_ACK` | `b"UAAC"` | Witness → Authority | Witness confirms UNBIND_ALL and tears down local state |

The canonical 10-byte envelope header from RFC-0850p-c §A "Canonical Envelope Serialization" is reused: `envelope_type (4 bytes, ASCII) || envelope_subtype (4 bytes, ASCII) || version (2 bytes, big-endian)`. All envelopes set `envelope_type = b"DOT1"`, the per-envelope subtype tag from the table above, and `version = 0x0001`. (R16 R1-C1 fix: migrated from the 1-byte subtype + 1-byte version stub in the v0.1 draft; the canonical format is the 4-byte ASCII + `u16` form per RFC-0850p-c.)

### Data Structures

```rust
/// DC-initiated group creation request (DOT/1/CGROUP).
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct CreateGroupEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1" (DeterministicEnvelope type tag)
    pub envelope_subtype: [u8; 4],      // b"CGRO"
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],
    pub mission_id: [u8; 32],
    pub platform: Platform,             // 0x01=WhatsApp, 0x02=Matrix, 0x03=Telegram
    pub proposed_group_metadata: ProposedGroupMetadata,
    pub initial_invite_count: u16,      // 0..=256
    pub dc_id: [u8; 32],                // DomainCoordinator's peer_id
    pub nonce: [u8; 16],
    pub current_epoch: u64,
    pub coordinator_term_id: [u8; 32],
    pub signature: [u8; 64],            // ed25519 over canonical bytes
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct ProposedGroupMetadata {
    pub display_name: String,         // max 256 bytes UTF-8
    pub topic: String,                // max 512 bytes UTF-8
    pub visibility: GroupVisibility,  // see GroupVisibility below
}

/// Group visibility mode (ProposedGroupMetadata.visibility).
/// Maps to the platform's visibility model:
/// - Private: only invited members can join (e.g., WhatsApp invite link, Matrix invite)
/// - Public: anyone with the `group_jid` / room ID can join (e.g., Matrix public room)
/// (R16 R1-M2 fix: enum was previously inlined as a comment "0x00=private, 0x01=public"
///  without a struct/enum definition; the type is now defined here.)
#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GroupVisibility {
    Private = 0x00,
    Public  = 0x01,
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct CreateGroupAckEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"CGAC" (CGROUP_ACK; R16 R2-H1 fix: this struct was missing from the v1.0/v1.1 RFCs; the envelope type was listed in the Envelope Types Added table and referenced multiple times in the State Machine, but no struct definition existed. Added here for completeness.)
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],
    pub cgroup_hash: [u8; 32],          // BLAKE3-256 of the CGROUP envelope being acked
    pub witness_id: [u8; 32],           // the witness's peer_id
    pub witness_epoch: u64,             // the witness's current epoch
    pub ack_hash: [u8; 32],             // BLAKE3-256(cgroup_hash || witness_id || witness_epoch)
    pub nonce: [u8; 16],
    pub signature: [u8; 64],            // witness's signature
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct CreateGroupDoneEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"CGDA"
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],
    pub group_jid: String,              // platform-assigned
    pub nonce: [u8; 16],                // matches CGROUP.nonce
    pub current_epoch: u64,
    pub signature: [u8; 64],
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct CreateGroupFailEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"CGFA"
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],
    pub reason_code: u16,               // see Error Handling §6
    pub platform_error: String,         // max 512 bytes UTF-8
    pub nonce: [u8; 16],
    pub current_epoch: u64,
    pub signature: [u8; 64],
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct InviteEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"INVT"
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],
    pub mission_id: [u8; 32],
    pub platform: Platform,
    pub group_jid: String,
    pub invitee_pubkey: [u8; 32],       // invitee's octo-network peer_id
    pub invite_token: [u8; 32],         // BLAKE3-256(domain_id || mission_id || invitee_pubkey || nonce)
    pub nonce: [u8; 16],
    pub current_epoch: u64,
    pub expires_at_epoch: u64,          // typically current_epoch + 100
    pub signature: [u8; 64],            // DC's signature
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct UnbindAllEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"UALL"
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: Platform,
    pub reason: UnbindReason,
    pub nonce: [u8; 16],
    pub current_epoch: u64,
    pub signature: [u8; 64],
}

/// Witness acknowledgement of an UNBIND_ALL (DOT/1/UNBIND_ALL_ACK).
/// (R16 R1 fix: previously listed in the Envelope Types Added table but
///  no struct was defined; the type is now defined to match the table.)
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct UnbindAllAckEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"UAAC"
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: Platform,
    pub nonce: [u8; 16],                // matches UNBIND_ALL.nonce
    pub current_epoch: u64,
    pub signature: [u8; 64],            // witness's signature
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub enum UnbindReason {
    DcDecommissioned    = 0x00,
    MissionTerminated   = 0x01,
    GovernanceVote      = 0x02,
    PlatformLoss        = 0x03,
    SquadContention     = 0x04,  // founder race / duplicate binding
}
```

### GroupState Additions

RFC-0850p-c §1 `GroupState` is extended with two new transient states:

| State | Value | Description |
|-------|-------|-------------|
| `Creating` | 0x04 | DC has signed CGROUP; awaiting `group_jid` from platform adapter |
| `Inviting` | 0x05 | Group exists on platform; DC is sending INVITE envelopes |

#### Transitions

| From | To | Trigger | Deterministic? | Side Effects | Signing |
|------|----|---------|----------------|--------------|---------|
| (none) | `Creating` | DC signs CGROUP | Yes | Reserve `domain_id` in `domain_index` | CGROUP envelope |
| `Creating` | `Bound` | `CGROUP_DONE` + BIND witnesses ≥ 1 | Yes | Insert into `GroupRegistry`, emit BIND envelope | CGROUP_DONE + BIND |
| `Creating` | `Unbound` | `CGROUP_FAIL` or timeout | Yes | Release `domain_id` reservation | CGROUP_FAIL |
| `Creating` | `UnboundQuarantined` | DC loses platform membership mid-create | Yes | Slash 0x000E | n/a |
| `Bound` | `Inviting` | DC signs at least one INVITE | Yes | Add invitee to `pending_invites` | INVITE envelope |
| `Inviting` | `Bound` | All INVITEs acknowledged or expired | Yes | Clear `pending_invites` | n/a |
| `Bound` | (terminal) | `UNBIND_ALL` + ≥ 1 ACK | Yes | Remove from `GroupRegistry` | UNBIND_ALL envelope |

### Algorithms

#### A. DC-Initiated Group Creation (single-DC)

1. DC validates that the `domain_id` is currently `Unbound` for the target platform (using `GroupRegistry.lookup_by_domain`).
2. DC signs a `CreateGroupEnvelope` with `nonce = random_16_bytes()`, `current_epoch = current_epoch()`, and `dc_id = self.peer_id`.
3. DC broadcasts the CGROUP via the overlay (RFC-0850 §3.2).
4. Witnesses verify and emit `CGROUP_ACK`; on ≥ 1 ACK, DC proceeds to platform-side creation.
5. DC calls the platform adapter's `create_group(metadata)` API; receives `group_jid`.
6. If creation succeeds: DC signs `CreateGroupDoneEnvelope` with the `group_jid` and the same `nonce`; broadcasts it.
7. Other nodes see `CGROUP_DONE` and emit their own `BIND` envelopes (per RFC-0850p-c ceremony).
8. If creation fails: DC signs `CreateGroupFailEnvelope` with `reason_code`; releases the `domain_id` reservation.
9. Timeout: if no `group_jid` is received within `CGROUP_TIMEOUT = 50` epochs, the DC emits `CGROUP_FAIL` with `reason_code = 0x0001` and transitions to `Unbound`.

#### B. Founder Race Resolution (multi-DC)

When two DCs CGROUP the same `domain_id` simultaneously (or within `CGROUP_RACE_WINDOW = 5` epochs):

1. Each DC sees the other's CGROUP via overlay broadcast.
2. Each DC compares `proposer_dc_id` lexicographically (BLAKE3 byte order, NOT signature verification order).
3. The DC with the lower `proposer_dc_id` continues; the higher-`dc_id` DC emits `CGROUP_FAIL` with `reason_code = 0x0002` (LostRace).
4. The losing DC's `GroupRegistry` MUST mark the `domain_id` as `Reserved` (transient) and NOT emit its own CGROUP.
5. The witness of the winning CGROUP emits `CGROUP_ACK` and proceeds.

This is consistent with the existing RFC-0850p-c founder race resolution (lexicographic comparison on `peer_id`).

#### C. Atomic Migration via CREATE+REBIND (R16 R1-L1 fix: previous title was "Atomic CREATE+REBIND")

Used when a DC needs to migrate a domain to a new platform (e.g., WhatsApp → Matrix):

1. DC for the new platform signs CGROUP (per algorithm A).
2. Once `CGROUP_DONE` is received, the DC emits a `REBIND` envelope (per RFC-0850p-c) with `old_group_jid` and `new_group_jid` set.
3. Witnesses treat the CGROUP and REBIND as a single logical transaction: ACK only on `CGROUP_DONE` + `REBIND.ack_count >= 1`.
4. If REBIND fails after CGROUP_DONE, the new group is left in `UnboundQuarantined` (per RFC-0850p-c §1) and the old group remains `Bound` until manual intervention.

#### D. Third-Party Group BIND

When the physical group was not created by the DC (e.g., a human admin created it on the platform):

1. DC verifies the group exists on the platform via `adapter.lookup_group(group_jid)`.
2. DC verifies the DC is a member of the group on the platform.
3. DC emits a `BIND` envelope (per RFC-0850p-c) with `is_reconnect: false` and a `witness_assertion: WitnessAssertion` field proving the platform-side membership claim.
4. Witnesses verify the `witness_assertion` by independently querying the platform (best-effort) and comparing.
5. On ≥ 1 witness ACK, the group is `Bound`.

```rust
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct WitnessAssertion {
    pub witness_id: [u8; 32],          // witness's peer_id
    pub group_jid: String,
    pub platform: Platform,
    pub dc_id: [u8; 32],               // the DC being asserted as a member
    pub observed_at_epoch: u64,
    pub observation_proof: Vec<u8>,    // platform-specific evidence (e.g., member list hash)
    pub nonce: [u8; 16],
    pub signature: [u8; 64],
}
```

The `WitnessAssertion` is a signed statement from a witness node that confirms it queried the platform and observed the DC as a member.

#### E. INVITE Issuance

1. DC signs an `InviteEnvelope` with the `invitee_pubkey` set to the invitee's `peer_id` and `invite_token = BLAKE3-256(domain_id || mission_id || invitee_pubkey || nonce)`.
2. The INVITE is sent **out-of-band** to the invitee (e.g., via SMS, QR code, deep link, or directly via libp2p) — it is NOT broadcast on the mesh.
3. The invitee receives the INVITE, verifies the DC's signature, and (if accepted) calls `adapter.join_group(group_jid, invite_token)`.
4. The platform-side join triggers the invitee's local BIND ceremony (per RFC-0850p-c).
5. On successful BIND, the invitee emits a BIND_ACK that includes the original INVITE's `invite_token`, allowing the DC to mark the invite as acknowledged.

#### F. UNBIND_ALL (Group Decommission)

1. The UNBIND_ALL authority (DC, MissionCreator, MissionController, or governance) signs the envelope.
2. The envelope is broadcast on the mesh.
3. Witnesses verify the authority and emit `UNBIND_ALL_ACK`.
4. On ≥ 1 ACK, all nodes remove the binding from their `GroupRegistry`.
5. The DC (if available) calls `adapter.leave_group(group_jid)` and (if it has admin rights) `adapter.dissolve_group(group_jid)`.

A dedicated `RFC-0850p-f` (Group Decommission) will elaborate UNBIND_ALL semantics (e.g., DC rotation, post-decommission audit log, platform-side leave race handling).

### Lifecycle Requirements

CGROUP, INVITE, and UNBIND_ALL envelopes MUST respect the same lifecycle as BIND envelopes (RFC-0850p-c §3):

- **TTL:** 100 epochs (CGROUP), 100 epochs (INVITE), 100 epochs (UNBIND_ALL).
- **Replay protection:** `NonceReplayTable` per RFC-0850p-c §8 (rule #4).
- **Coordinator term check:** `coordinator_term_id` MUST match the current DC's term.

### Determinism Requirements

All envelope types in this RFC MUST serialize deterministically per RFC-0126 (DCS). Specifically:

- `String` fields (e.g., `display_name`, `group_jid`) MUST be UTF-8 with no trailing null bytes.
- The 10-byte canonical header precedes all envelope-specific fields.
- Signature is computed over the canonical bytes of the envelope excluding the signature field.
- The `bind_hash` (when present in BIND after CGROUP_DONE) MUST include the `group_jid` and `is_reconnect: false`.

### RFC-0008 Execution Class Mapping

| Operation | Class | Rationale |
|-----------|-------|-----------|
| CGROUP sign + broadcast | C | Initiated by DC; no consensus impact |
| CGROUP_ACK verification | C | Local witness action; no consensus |
| CGROUP_DONE verification | C | Local witness action; no consensus |
| CGROUP_FAIL verification | C | Local witness action; no consensus |
| Platform-side `create_group` | C | External platform call; not consensus |
| Founder race resolution (lexicographic comparison) | B | Deterministic; must be identical across all nodes |
| INVITE sign + send | C | Out-of-band; no consensus |
| UNBIND_ALL sign + broadcast | B | Affects `GroupRegistry` state; must be deterministic |
| UNBIND_ALL_ACK verification | C | Local witness action |
| `GroupRegistry` insert / remove (BIND / UNBIND) | B | Shared state; must be deterministic |

### Error Handling

#### Error codes — `CGROUP_FAIL.reason_code`

| Code | Name | Description |
|------|------|-------------|
| 0x0001 | Timeout | Platform did not return `group_jid` within `CGROUP_TIMEOUT = 50` epochs |
| 0x0002 | LostRace | Another DC won the founder race for this `domain_id` |
| 0x0003 | PlatformError | Platform API returned an error (e.g., rate limit, permission denied) |
| 0x0004 | DomainAlreadyBound | `domain_id` is already `Bound` for this platform |
| 0x0005 | DcNotAuthorized | `dc_id` does not match the current DC for this `domain_id` |
| 0x0006 | InvalidMetadata | `display_name` or `topic` failed platform validation |
| 0x0007 | NetworkError | Platform adapter lost connectivity mid-create |

#### Error codes — INVITE

| Code | Name | Description |
|------|------|-------------|
| 0x0101 | Expired | `current_epoch > expires_at_epoch` |
| 0x0102 | InvalidSignature | DC's signature failed verification |
| 0x0103 | UnknownDomain | `domain_id` is not `Bound` on the invitee's adapter |
| 0x0104 | AlreadyMember | Invitee is already a member of the group |
| 0x0105 | PlatformJoinFailed | Platform rejected the join (e.g., banned, blocked) |

#### Error codes — UNBIND_ALL

| Code | Name | Description |
|------|------|-------------|
| 0x0201 | InsufficientAuthority | Signer is not the DC, MissionCreator, MissionController, or governance |
| 0x0202 | NotBound | `group_jid` is not in any node's `GroupRegistry` |
| 0x0203 | PlatformLeaveFailed | Platform rejected the leave; state is now inconsistent (the group may still be active on the platform but unbound in the mesh) |

### Slash Reason Codes Added

This RFC allocates three new slash reason codes in the canonical slash reason code space (per RFC-0855p-b §B and RFC-0850p-c §6 "Unbind Reasons", codes 0x000C-0x000D are reserved for non-slash mechanisms, 0x000E-0x0011 are allocated by the 0850p-family sister RFCs, 0x0012 is allocated by RFC-0855p-c §9b "Cross-Platform Slash", 0x0013-0xFFFF are reserved for future slash reasons). The allocation is coordinated across the 0850p-family sister RFCs (RFC-0850p-d, RFC-0850p-e) and RFC-0855p-c, and is now RATIFIED in RFC-0855p-b §B v1.2 and RFC-0850p-c §6 v0.1.2 (R16 R1 fix). See `docs/reviews/r16/r16-r1-adversarial-review.md` §2 "Slash code space allocation" for the canonical mapping.

| Code | Name | Definition | Trigger |
|------|------|------------|---------|
| 0x000E | `CreateGroupFailed` | The DC failed to create the group on the platform within `CGROUP_TIMEOUT = 50` epochs | `Creating → UnboundQuarantined` transition |
| 0x000F | `CgGroupSpam` | The DC issued CGROUP at a rate exceeding the per-domain rate limit (1 CGROUP per `domain_id` per 1000 epochs) | Rate-limit violation on CGROUP issuance |
| 0x0010 | `FalseWitness` | A witness signed a `WitnessAssertion` that is contradicted by the witness's own platform-side query result (or by a quorum of other witnesses) | Third-party BIND with false `WitnessAssertion` |

**Code 0x000F resolution note (R16 R1-C2 fix):** RFC-0855p-c §"Adversary Analysis" decision table (text) mentioned "Slash via 0x000F" for "Cross-platform witness collusion", creating a double-allocation. The 0855p-c reference is a stale text reference (the canonical slash reason code tables in 0855p-b §B and 0850p-c §6 do NOT reserve 0x000F for 0855p-c). The 0855p-c text is updated in the R16 R1 fix to refer to a non-conflicting code (0x0012) for cross-platform witness collusion, and 0x000F is canonically allocated to `CgGroupSpam` by this RFC.

**Code 0x0010 note:** RFC-0850p-e (kick detection) reuses 0x0010 for false `KICK_DETECTED` (a different `WitnessAssertion` failure mode). The two uses are semantically consistent: both are witness signed-statements that turned out to be false. The slash tally aggregates both forms of 0x0010.

**Codes 0x000C and 0x000D** are intentionally left unallocated by this RFC. The 0850p-d §"Roles and Authorities" table previously referenced "slash 0x000C" and "slash 0x000D" loosely for delegation and governance-override; these mechanisms are NOT slash reason codes (R16 R1-M1 fix) and the codes remain reserved for future allocation.

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| CGROUP end-to-end (sign → CGROUP_DONE broadcast) | <5s | Includes platform-side `create_group` API call |
| INVITE end-to-end (sign → invitee receives) | <2s | Out-of-band; depends on transport (SMS, libp2p) |
| UNBIND_ALL end-to-end (sign → all nodes acked) | <30s | Bounded by overlay propagation (RFC-0850) |
| Founder race resolution | <100ms | Local computation (lexicographic comparison) |
| WitnessAssertion verification | <2s | Platform-side query |

## Implicit Assumptions Audit

| Assumption | Where Relied Upon | Blast Radius if False | Mitigation / Status |
|------------|-------------------|----------------------|---------------------|
| The DC has authority to create groups on the platform (e.g., WhatsApp allows any user; Matrix requires room-creation rights) | §A.1, §A.5 | DC cannot create groups; mission stalls | Mitigation: detect at `CGROUP_TIMEOUT` and emit `CGROUP_FAIL` with `PlatformError`. **Future Work F-1: platform-side admin bootstrap** |
| The platform adapter exposes a `create_group` API that returns a `group_jid` | §A.5 | Cannot get the `group_jid`; BIND cannot complete | Mitigation: per-adapter implementation (WhatsApp: `GroupCreate` event; Matrix: `POST /createRoom`; Telegram: `createGroup` API) |
| The DC can verify the platform-side creation succeeded (i.e., the `group_jid` is real and the DC is a member) | §A.6, §A.7 | DC may have a phantom `group_jid` | Mitigation: cross-check via `adapter.lookup_group(group_jid)` before CGROUP_DONE; reject on mismatch |
| INVITE is delivered out-of-band (e.g., SMS) before the invitee joins the platform | §E.2 | Invitee cannot join; the INVITE is useless | Mitigation: the platform join ceremony proceeds without INVITE if the platform allows (e.g., WhatsApp invite-link), but the BIND witnesses will reject the BIND without a valid INVITE token. **Future Work F-2: relax INVITE requirement for platform-invite-link groups** |
| The DC is a member of the group after `create_group` returns | §A.5 | The BIND cannot proceed (DC must be a member to sign BIND) | Mitigation: verify via `adapter.get_members(group_jid)`; if DC is not a member, emit CGROUP_FAIL with `PlatformError` |
| The `group_jid` is stable (does not change after creation) | §A.6 | The BIND references a stale `group_jid` | Mitigation: all platforms in scope return stable JIDs / room IDs after creation |
| The platform's `create_group` rate limit is high enough for the mission's needs | §A.5 | Frequent CGROUP_FAIL with rate-limit reason | Mitigation: rate-limit CGROUP issuance at the DC level (e.g., 1 CGROUP per domain per 1000 epochs) |

### Categories to Audit

- **Operator trust** — Assumes the DC's operator (the human running the DC) is trusted. If the operator is compromised, they can issue CGROUP for arbitrary `domain_id`s. Mitigation: the operator's key is bound to the mission via `MissionCreator`; slash 0x0006 (key-compromise, per RFC-0855p-b §B) on operator compromise. (R16 R1 fix: the previous wording said "slash 0x000F on operator compromise" but 0x000F is canonically allocated to `CgGroupSpam` per this RFC; operator compromise uses the existing 0x0006 key-compromise reason from 0855p-b §B.)
- **Platform trust** — Assumes the platform returns a valid `group_jid`. If the platform lies (e.g., returns a JID that doesn't exist), the BIND will fail. Mitigation: cross-check via `adapter.lookup_group`.
- **Network partition** — Assumes DC can reach the platform's API. If partitioned, CGROUP_FAIL with `NetworkError`. The DC can retry.
- **Identity stability** — Assumes the DC's `peer_id` is stable across the ceremony. If the DC rotates its key mid-ceremony, the CGROUP_DONE may be rejected. Mitigation: `coordinator_term_id` check.
- **Resource availability** — Assumes the platform allows unlimited groups per account. If a hard cap exists, CGROUP_FAIL with `PlatformError`. Mitigation: monitor group count per platform account.

## Security Considerations

- **Consensus attacks:** A malicious DC could spam CGROUP for arbitrary `domain_id`s. Mitigation: rate limit + slash 0x000F.
- **Economic exploits:** A DC could create many groups to consume platform-side quotas (e.g., Matrix storage). Mitigation: rate limit + mission-level quota.
- **Proof forgery:** INVITE tokens are bound to the DC's signature; forging requires the DC's key.
- **Replay attacks:** All envelopes have a 16-byte nonce; `NonceReplayTable` per RFC-0850p-c §8.
- **Determinism violations:** All envelopes serialize via DCS; the founder race is lexicographic.

## Adversary Analysis

### Decision Table

| Decision | Q1 Beneficiary | Q2 Cost to Attacker | Q3 Gain if Successful | Q4 Defense (cost to legit op) | Q5 Residual Risk |
|----------|----------------|---------------------|------------------------|------------------------------|------------------|
| Accept CGROUP from any signed DC | Malicious DC | Burn DC identity | Create unlimited groups | Slash 0x000F + rate limit (1 CGROUP per domain per 1000 epochs) | Acceptable: bounded by DC stake |
| Founder race resolution by `dc_id` lexicographic | Colluding DCs | Coordinate to win both races | Capture a `domain_id` | `dc_id` is BLAKE3 of the DC's public key; coordination requires a key collision | Acceptable: probability of two DCs with same `dc_id` is 2^-256 |
| Allow third-party group BIND | DC with malicious intent | Stake a DC identity | Bind an attacker-controlled group | Witness assertion + slash 0x0010 on false assertion | Acceptable: witness is slashable |
| INVITE without platform-side presence | Invitee | Impersonate an invitee | Join a group without platform membership | INVITE requires `invitee_pubkey` signature + platform join | Acceptable: platform join is the gate |
| UNBIND_ALL by ex-DC | Ejected DC | No cost (key is public) | DoS the group | `coordinator_term_id` check; the ex-DC's term is `Inactive` | Acceptable: term check is cheap and deterministic |

### Severity Classification

| Severity | Issue | Action |
|----------|-------|--------|
| HIGH | CGROUP spam | Slash 0x000F on rate-limit violation |
| HIGH | Founder race capture | Lexicographic `dc_id` (2^-256 collision) |
| MEDIUM | INVITE replay | Nonce table (per RFC-0850p-c §8) |
| MEDIUM | Third-party group BIND false witness | Slash 0x0010 on false assertion |
| LOW | UNBIND_ALL by ex-DC | Require `coordinator_term_id` match |

## Economic Analysis

This RFC has no token-economic implications. The platform-side costs (e.g., WhatsApp group creation, Matrix room creation) are out of scope of the protocol.

## Compatibility

- **Backward compatibility:** This RFC adds new envelope types; existing BIND / REBIND / UNBIND envelopes are unchanged.
- **Forward compatibility:** `version: u16` is reserved; future versions may add new fields but MUST NOT change the field order or remove existing fields (per RFC-0008). (R16 R1-C1 fix: previous version said `version: u8`; the canonical 10-byte header per RFC-0850p-c §A reserves 2 bytes for `version`.)
- **Adapter compatibility:** Adapters that do not implement `create_group` MUST return `PlatformError::Unsupported` from CGROUP; nodes MUST then emit CGROUP_FAIL with `reason_code = 0x0003`.

## Test Vectors

Test vectors are defined in `crates/octo-network/src/dot/binding/test_vectors.rs` (TBD; to be created in the base mission `0850p-c-base.md`). At minimum:

1. **CGROUP round-trip** — Sign and verify a CGROUP with a known `domain_id`, `mission_id`, and `nonce`. Verify signature is reproducible byte-for-byte.
2. **CGROUP_DONE** — Sign and verify a CGROUP_DONE with a known `group_jid` and `nonce`. Verify the `nonce` matches the original CGROUP.
3. **Founder race** — Two DCs CGROUP the same `domain_id`; verify the lexicographic tiebreak produces a consistent winner.
4. **INVITE token** — Compute `invite_token` for a known `(domain_id, mission_id, invitee_pubkey, nonce)` and verify it matches `BLAKE3-256(domain_id || mission_id || invitee_pubkey || nonce)`.
5. **UNBIND_ALL authority check** — A non-DC node signs UNBIND_ALL; verify all witnesses reject with `InsufficientAuthority`.

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| **Option A: CGROUP envelope + INVITE (this RFC)** | Clean separation; deterministic; reuses existing nonce machinery | More envelope types to maintain |
| Option B: Extend BIND envelope with a `create: bool` flag | Fewer envelope types | Conflates creation with binding; complicates the BIND state machine |
| Option C: Out-of-band creation (human creates group, DC BINDs) | No new envelopes | Defeats the "DC drives the ceremony" use case |
| Option D: Use platform-specific invite APIs (e.g., WhatsApp `invite_v4`) | Native to platform | Not all platforms have a native invite API; breaks the platform-neutral goal |

## Implementation Phases

### Phase 1: CGROUP envelope + ceremony

- [ ] Add `CreateGroupEnvelope`, `CreateGroupDoneEnvelope`, `CreateGroupFailEnvelope` types in `crates/octo-network/src/dot/binding.rs`.
- [ ] Add `GroupState::Creating` (0x04) transition.
- [ ] Implement DC-side CGROUP sign + broadcast in `octo-adapter-*` adapters.
- [ ] Implement witness CGROUP_ACK emission.
- [ ] Implement founder race resolution (lexicographic `dc_id`).
- [ ] Implement `CGROUP_TIMEOUT = 50` epochs with CGROUP_FAIL.
- [ ] Unit tests: round-trip serialization, signature verification, founder race.
- [ ] Integration test: DC creates a new WhatsApp group, binds to `domain_id`, witness acks.

### Phase 2: INVITE envelope

- [ ] Add `InviteEnvelope` type.
- [ ] Add `GroupState::Inviting` (0x05) transition.
- [ ] Implement DC-side INVITE sign + out-of-band send.
- [ ] Implement invitee-side INVITE receive + verify + platform join.
- [ ] Add `pending_invites: BTreeMap<[u8; 32], InviteEnvelope>` to `GroupRegistry`.
- [ ] Unit tests: INVITE token computation, signature verification, expiry check.
- [ ] Integration test: DC invites 3 members, all join via INVITE, all BIND successfully.

### Phase 3: Third-party group BIND

- [ ] Add `WitnessAssertion` type.
- [ ] Extend BIND envelope (or add BIND_3RD_PARTY variant) with `witness_assertion: Option<WitnessAssertion>` field.
- [ ] Implement witness-side `adapter.lookup_group` verification.
- [ ] Slash 0x0010 on false `WitnessAssertion`.
- [ ] Unit tests: valid witness assertion, invalid witness assertion rejected.
- [ ] Integration test: human creates a WhatsApp group, DC BINDs with witness assertion.

### Phase 4: UNBIND_ALL

- [ ] Add `UnbindAllEnvelope`, `UnbindAllAckEnvelope` types.
- [ ] Implement DC, MissionCreator, MissionController, governance authority check.
- [ ] Implement platform-side `leave_group` + `dissolve_group` (if admin rights).
- [ ] Unit tests: each authority role can / can't sign UNBIND_ALL.
- [ ] Integration test: DC UNBIND_ALLs a bound domain, all nodes remove from `GroupRegistry`.

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/octo-network/src/dot/binding.rs` | Add `CreateGroupEnvelope`, `CreateGroupDoneEnvelope`, `CreateGroupFailEnvelope`, `InviteEnvelope`, `UnbindAllEnvelope`, `UnbindAllAckEnvelope`, `WitnessAssertion` types |
| `crates/octo-network/src/dot/group_registry.rs` | Add `GroupState::Creating`, `GroupState::Inviting` transitions; add `pending_invites: BTreeMap<[u8; 32], InviteEnvelope>` |
| `crates/octo-network/src/dot/witness.rs` | Add CGROUP_ACK, CGROUP_DONE, CGROUP_FAIL, UNBIND_ALL_ACK verification |
| `crates/octo-adapter-whatsapp/src/adapter.rs` | Add `create_group`, `join_group(invite_token)`, `leave_group`, `dissolve_group` |
| `crates/octo-adapter-matrix/src/lib.rs` | Same as above |
| `crates/octo-adapter-telegram/src/lib.rs` | Same as above |
| `crates/octo-network/src/dot/dc.rs` (new) | DomainCoordinator module: orchestrates CGROUP, INVITE, UNBIND_ALL |

## Future Work

- **F-1: Platform-side admin bootstrap** — For platforms that require admin rights to create groups (e.g., Matrix rooms in restricted spaces), define a bootstrap flow where a human operator grants the DC admin rights out-of-band.
- **F-2: Platform-invite-link groups** — For platforms with native invite-link APIs (e.g., WhatsApp invite links), relax the INVITE requirement so that BIND can proceed with just a `join_token` from the platform.
- **F-3: Sub-group creation** — A DC can create a sub-group for a sub-domain (see RFC-0855p-d).
- **F-4: DC migration** — A DC that needs to move to a new platform (e.g., WhatsApp → Matrix) can use the atomic CREATE+REBIND flow (algorithm C); a dedicated migration RFC may be useful.
- **F-5: Group metadata updates** — DC can update the group's name / topic via a new `DOT/1/MUPDATE` envelope (out of scope for this RFC).
- **F-6: RFC-0850p-f elaboration** — A dedicated RFC for group decommission, including DC rotation, post-decommission audit log, and platform-side leave race handling.

## Rationale

- **Why new envelope types vs. extending BIND?** The BIND state machine is already complex (4 states, 5 transitions). Adding CGROUP-specific fields to BIND would conflate creation with binding and complicate the witness validation pipeline. New envelope types keep each flow's invariants clear.
- **Why lexicographic founder race?** It is the same tiebreak used in RFC-0850p-c's BIND ceremony (R3-TGB-2), providing consistency. It is also deterministic and inexpensive to compute.
- **Why out-of-band INVITE?** Broadcasting INVITE on the mesh would leak membership information (privacy concern) and consume overlay bandwidth. Out-of-band delivery (SMS, QR, libp2p) is more efficient and privacy-preserving.
- **Why UNBIND_ALL separate from UNBIND?** UNBIND is per-node (RFC-0850p-c §3.4): a node leaves the group but the group remains bound for other nodes. UNBIND_ALL is a network-wide decommission: the group is removed from all `GroupRegistry`s. Different semantics warrant different envelope types.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-17 | Initial draft |
| 1.1 | 2026-06-17 | R16 R1 fix: (C1) migrated all 6 envelope structs from 1-byte subtype + 1-byte version stub to the canonical 10-byte header per RFC-0850p-c §A (4-byte ASCII `envelope_type` + 4-byte ASCII `envelope_subtype` + `u16` version); added `UnbindAllAckEnvelope` struct (was in the Envelope Types Added table but had no struct definition); (C2/M1) added "Slash Reason Codes Added" subsection allocating 0x000E=`CreateGroupFailed`, 0x000F=`CgGroupSpam`, 0x0010=`FalseWitness` (with note that 0x000F resolution: 0855p-c stale text reference is updated to 0x0012 for cross-platform witness collusion); 0x000C-0x000D remain unallocated (the previous "slash 0x000C" / "slash 0x000D" references in §"Roles and Authorities" were removed because they used the slash-code space loosely for delegation and governance-override, which are not slash reasons); (M2) added `GroupVisibility` enum (was previously inlined as a comment); (L1) renamed "Atomic CREATE+REBIND" Design Goal to "Atomic group provisioning" to avoid conflation with the §C migration algorithm. R16 R2 fix: added `CreateGroupAckEnvelope` struct (subtype `b"CGAC"`) — this struct was listed in the Envelope Types Added table but had no struct definition; the type was referenced multiple times in the State Machine and elsewhere in the RFC. |

## Related RFCs

- RFC-0850 (Networking): Deterministic Overlay Transport
- RFC-0855 (Networking): Mission Overlay Networks
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle
- RFC-0855p-c (Networking): DomainCoordinator Role
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding
- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-e (Networking): Kick & Platform Membership Change Detection (companion)
- RFC-0850p-f (Networking): Transport Group Decommission (companion, planned)
- RFC-0851p-a (Networking): Network Bootstrap Protocol
- RFC-0126 (Numeric): Deterministic Canonical Serialization

## Related Use Cases

- `docs/use-cases/social-platform-transport-layer.md` — DC-Initiated Group Creation, Invite Issuance, Third-Party Group BIND
- `docs/use-cases/mission-coordinator-lifecycle.md` — DC Authority, DC Migration
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario families S-G1, S-G2, S-G3, S-G5, S-G6
- `docs/e2e/2026-06-16-e2e-test-plan.md` — Implicit specs IS-3.1 through IS-3.6

## Appendices

### A. Worked Example: DC Creates a WhatsApp Group for a New Domain

```
T+0:    DC signs CreateGroupEnvelope {
          domain_id: BLAKE3("mission-alpha:domain-vote-recount"),
          mission_id: BLAKE3("mission-alpha"),
          platform: WhatsApp,
          proposed_group_metadata: { display_name: "Mission Alpha — Vote Recount", topic: "..." },
          initial_invite_count: 0,
          dc_id: <DC's peer_id>,
          nonce: 0x1234...,
          current_epoch: 1000,
          coordinator_term_id: 0xABCD...
        }
        DC broadcasts CGROUP via overlay.

T+1s:   Witness W1 verifies CGROUP signature, sees it's the first CGROUP for this domain_id,
        emits CGROUP_ACK.

T+2s:   DC sees CGROUP_ACK; calls WhatsApp API create_group("Mission Alpha — Vote Recount").
        WhatsApp returns group_jid = "120363012345678901@g.us".

T+3s:   DC signs CreateGroupDoneEnvelope { domain_id, group_jid, nonce, current_epoch: 1003, ... }
        DC broadcasts CGROUP_DONE.

T+4s:   All nodes see CGROUP_DONE; the GroupRegistry for each transitions to Creating → Bound.
        Each node emits a BIND envelope (per RFC-0850p-c ceremony) with is_reconnect: false.

T+10s:  3 BIND_ACKs received; group is fully Bound.
```

### B. Founder Race Tiebreak — Detailed Pseudocode

```rust
fn resolve_founder_race(
    local_cgroup: &CreateGroupEnvelope,
    remote_cgroup: &CreateGroupEnvelope,
) -> RaceOutcome {
    if local_cgroup.dc_id < remote_cgroup.dc_id {
        RaceOutcome::LocalWins
    } else if local_cgroup.dc_id > remote_cgroup.dc_id {
        RaceOutcome::RemoteWins
    } else {
        // dc_id is identical — should not happen with distinct DCs
        RaceOutcome::Ambiguous
    }
}
```

### C. INVITE Token Computation

```rust
fn compute_invite_token(
    domain_id: &[u8; 32],
    mission_id: &[u8; 32],
    invitee_pubkey: &[u8; 32],
    nonce: &[u8; 16],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain_id);
    hasher.update(mission_id);
    hasher.update(invitee_pubkey);
    hasher.update(nonce);
    let mut out = [0u8; 32];
    hasher.finalize_xof().fill(&mut out);
    out
}
```

---

**Version:** 1.0
**Submission Date:** 2026-06-17
**Last Updated:** 2026-06-17
