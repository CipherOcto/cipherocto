# RFC-0850p-e (Networking): Kick & Platform Membership Change Detection

## Status

Draft (2026-06-17)

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Specifies how transport-group adapters detect when the local bot is removed or kicked from a bound physical group, how non-DC members detect their own removal, and how the DomainCoordinator (DC) and witnesses are notified to initiate REBIND, UNBIND, or quarantine. **Closes the CRITICAL E2E implicit spec IS-5.1** (kick detection) — without it, a kicked bot silently misses all subsequent DOT envelopes and the group becomes a zombie partition. Defines the `DOT/1/KICK_DETECTED`, `DOT/1/SELF_KICKED`, and `DOT/1/MEMBER_REMOVED` envelope types, per-adapter detection strategies (WhatsApp, Matrix, Telegram), and the state machine that handles kick events.

## Dependencies

**Required (must be Accepted or in Active Implementation):**

- RFC-0850 (Networking): Deterministic Overlay Transport — `DeterministicEnvelope`, `DOT/1/*` versioning
- RFC-0855p-c (Networking): DomainCoordinator Role — DC authority
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `GroupState::UnboundQuarantined`, BIND / REBIND / UNBIND envelopes
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — CGROUP/INVITE/UNBIND_ALL envelopes (sister RFC; co-implemented)
- RFC-0126 (Numeric): DCS — canonical serialization

**Refines:** RFC-0850p-c §1 (`UnboundQuarantined` state) and §3 (PlatformLoss envelope).

## Design Goals

1. **Real-time detection.** A kick event MUST be detected within `KICK_DETECTION_DEADLINE = 5` epochs of the platform-side event.
2. **Platform-neutral envelope.** The protocol semantics MUST be identical across WhatsApp, Matrix, Telegram; only the adapter's detection mechanism differs.
3. **Local-first action.** When a node detects its own kick, it MUST immediately transition to `UnboundQuarantined` locally and emit `SELF_KICKED`, even if the overlay is unreachable.
4. **Witness-validated kick.** A node MUST NOT trust a `KICK_DETECTED` from another node without independent platform-side verification (witness assertion per RFC-0850p-d §D).
5. **False-positive tolerance.** A transient platform glitch (e.g., WhatsApp reconnect) MUST NOT be misclassified as a kick. The `KICK_DETECTION_GRACE_PERIOD = 2` epochs allows the platform to re-add the member before triggering quarantine.
6. **No silent zombie state.** A node that cannot determine its kick status for `KICK_DETECTION_TIMEOUT = 50` epochs MUST transition to `UnboundQuarantined` and emit `KICK_DETECTED` with `reason_code = 0x1001` (StatusTimeout).

## Motivation

The E2E test plan `docs/e2e/2026-06-16-e2e-test-plan.md` flags **IS-5.1 (CRITICAL)**: "Bot is removed from a bound WhatsApp group; the bot must detect the removal within 5 epochs and emit KICK_DETECTED; otherwise the group becomes a zombie partition that the DC cannot REBIND."

RFC-0850p-c §3.5 mentions `PlatformLoss` envelope for "loss of platform connectivity" but does not specify kick detection specifically. RFC-0850p-d adds `UNBIND_ALL` for decommission but does not address involuntary removal. This RFC fills the gap.

### Use Case Link

- `docs/use-cases/social-platform-transport-layer.md` — "Failure Modes" section
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario families S-G1 / S-G2 / S-G5 (failure-mode sub-scenarios)
- `docs/e2e/2026-06-16-e2e-test-plan.md` — Implicit spec IS-5.1 (CRITICAL)

## Roles and Authorities

| Role | Authority for SELF_KICKED | Authority for KICK_DETECTED | Authority for MEMBER_REMOVED |
|------|---------------------------|------------------------------|------------------------------|
| **Local node (any role)** | Yes (self-report) | No (only the witness or DC can assert) | No |
| **Witness** | No (witness receives) | Yes (after platform-side verification) | No |
| **DC** | No (DC receives) | Yes (after platform-side verification) | Yes (DC may report any member's removal) |

A node that is itself kicked MUST emit `SELF_KICKED`. A witness or DC that observes another node's kick (via platform-side query) MAY emit `KICK_DETECTED` for that node. A DC may emit `MEMBER_REMOVED` for any non-DC member.

## Specification

### Envelope Types Added

| Envelope Type | Subtype | Direction | Description |
|---------------|---------|-----------|-------------|
| `DOT/1/SELF_KICKED` | 0x20 | Local node → mesh (broadcast) | The local bot detected its own removal from a bound group |
| `DOT/1/KICK_DETECTED` | 0x21 | Witness / DC → mesh (broadcast) | A witness or DC detected that another node was kicked |
| `DOT/1/MEMBER_REMOVED` | 0x22 | DC → mesh (broadcast) | A non-DC member was removed (informational; not a quorum event) |
| `DOT/1/REJOIN_REQUEST` | 0x23 | Local node → DC (unicast) | A kicked node requests the DC to re-invite it (per RFC-0850p-d §E) |
| `DOT/1/REJOIN_GRANT` | 0x24 | DC → requesting node (unicast) | DC grants the rejoin request and re-issues an INVITE envelope |

The canonical 10-byte envelope header from RFC-0850p-c is reused.

### Data Structures

```rust
/// Self-kick notification (DOT/1/SELF_KICKED).
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SelfKickedEnvelope {
    pub envelope_subtype: u8,        // 0x20
    pub version: u8,                 // 0x01
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: Platform,
    pub platform_event: PlatformKickEvent,   // what the platform reported
    pub detected_at_epoch: u64,
    pub nonce: [u8; 16],
    pub signature: [u8; 64],         // node's own key
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub enum PlatformKickEvent {
    YouGotKicked         = 0x00,  // WhatsApp "you were removed" event
    YouLeft              = 0x01,  // we left voluntarily (shouldn't happen, but tracked)
    GroupDissolved       = 0x02,  // group was deleted
    GroupDisappeared     = 0x03,  // group is unreachable (e.g., banned)
    SessionLost          = 0x04,  // platform session expired; cannot determine status
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct KickDetectedEnvelope {
    pub envelope_subtype: u8,        // 0x21
    pub version: u8,                 // 0x01
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: Platform,
    pub kicked_node_id: [u8; 32],     // the peer_id of the kicked node
    pub witness_assertion: WitnessAssertion,   // per RFC-0850p-d §D
    pub detected_at_epoch: u64,
    pub nonce: [u8; 16],
    pub signature: [u8; 64],         // witness or DC's signature
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct MemberRemovedEnvelope {
    pub envelope_subtype: u8,        // 0x22
    pub version: u8,                 // 0x01
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: Platform,
    pub removed_node_id: [u8; 32],
    pub platform_event: PlatformKickEvent,
    pub detected_at_epoch: u64,
    pub nonce: [u8; 16],
    pub signature: [u8; 64],         // DC's signature
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct RejoinRequestEnvelope {
    pub envelope_subtype: u8,        // 0x23
    pub version: u8,                 // 0x01
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: Platform,
    pub requesting_node_id: [u8; 32],
    pub platform_event: PlatformKickEvent,
    pub detected_at_epoch: u64,
    pub nonce: [u8; 16],
    pub signature: [u8; 64],         // requesting node's signature
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct RejoinGrantEnvelope {
    pub envelope_subtype: u8,        // 0x24
    pub version: u8,                 // 0x01
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: Platform,
    pub granted_node_id: [u8; 32],
    pub new_invite_token: [u8; 32],  // a fresh INVITE envelope (per RFC-0850p-d §E)
    pub expires_at_epoch: u64,
    pub nonce: [u8; 16],
    pub signature: [u8; 64],         // DC's signature
}
```

### Per-Adapter Detection Strategies

#### WhatsApp (`octo-adapter-whatsapp`)

- **Event-based detection** (primary): subscribe to the WhatsApp WebSocket event `GroupParticipantRemove`. The event payload includes the `phone_number` of the removed participant.
- **Match phone number** to the local `GroupConfig.operator_phone` field; if matched, this node was kicked.
- **Cross-check via `get_group_info(group_jid)`** within `KICK_DETECTION_GRACE_PERIOD = 2` epochs. If the node is re-added (e.g., transient glitch), suppress the kick event.
- **Fallback: heartbeat** — if the WebSocket is silent for `KICK_DETECTION_TIMEOUT = 50` epochs AND `get_group_info` returns an error, assume the group was dissolved and emit `SELF_KICKED` with `platform_event = GroupDissolved`.

#### Matrix (`octo-adapter-matrix`)

- **Event-based detection** (primary): subscribe to the Matrix `/sync` endpoint with `state: { type: m.room.member }` filter. The state event includes `membership: leave | ban` for the local user.
- **On `membership: ban`**, emit `SELF_KICKED` with `platform_event = YouGotKicked`.
- **On `membership: leave`** (voluntary or kicked), cross-check the `event.sender` field: if the sender is the room admin, it was a kick; if the sender is the local user, it was a voluntary leave.
- **Cross-check via `GET /rooms/{roomId}/state/m.room.member/{userId}`** within `KICK_DETECTION_GRACE_PERIOD = 2` epochs.

#### Telegram (`octo-adapter-telegram`)

- **Event-based detection** (primary): subscribe to the Telegram Bot API `ChatMember` updates. The update includes `status: kicked | left`.
- **On `status: kicked`**, emit `SELF_KICKED` with `platform_event = YouGotKicked`.
- **On `status: left`**, treat as voluntary; no envelope emitted (the DC will detect via the `ChatMember` count).
- **Cross-check via `getChatMember(chat_id, user_id)`** within `KICK_DETECTION_GRACE_PERIOD = 2` epochs.

### State Machine — GroupState

RFC-0850p-c §1 `GroupState` is extended with new transitions:

| From | To | Trigger | Deterministic? | Side Effects | Signing |
|------|----|---------|----------------|--------------|---------|
| `Bound` | `UnboundQuarantined` | Local node detects `SELF_KICKED` | Yes (event-driven) | Slash 0x0011 (SelfKicked); emit `SELF_KICKED` | SELF_KICKED |
| `Bound` | `UnboundQuarantined` | Witness / DC emits `KICK_DETECTED` for this node | Yes (≥ 1 witness with valid assertion) | Slash 0x0011 | KICK_DETECTED |
| `UnboundQuarantined` | `Bound` | DC re-invites (REJOIN_GRANT) + REBIND completes | Yes | Clear quarantine, emit BIND | REJOIN_GRANT + BIND |
| `Bound` | `Bound` (no transition) | Non-DC member removed (DC emits `MEMBER_REMOVED` for tracking) | Yes | Update `membership_log`; do NOT trigger REBIND | MEMBER_REMOVED |
| `Bound` | `UnboundQuarantined` | `KICK_DETECTION_TIMEOUT = 50` epochs with no status | Yes | Slash 0x0011 with `reason_code = 0x1001` | SELF_KICKED |

### Algorithms

#### A. Self-Kick Detection (local node)

1. The adapter receives a platform-side event (e.g., WhatsApp `GroupParticipantRemove`).
2. The adapter checks whether the removed participant matches the local identity.
3. If yes, the adapter waits `KICK_DETECTION_GRACE_PERIOD = 2` epochs and re-queries the platform.
4. If the local identity is still absent at the end of the grace period, the adapter emits `SELF_KICKED` with the `platform_event` matching the platform's report.
5. The local `GroupRegistry` transitions `Bound → UnboundQuarantined` immediately (do not wait for mesh consensus).
6. The adapter continues to attempt to re-join the group (per algorithm D) if instructed by the DC.

#### B. Witness Kick Detection (for another node)

1. The witness adapter receives a platform-side event indicating a participant removal.
2. The witness checks whether the removed participant's phone number / user id matches a known `peer_id` in the `GroupRegistry`.
3. If yes, the witness emits `KICK_DETECTED` with a `WitnessAssertion` (per RFC-0850p-d §D).
4. The witness broadcasts `KICK_DETECTED` to the mesh.
5. The DC sees `KICK_DETECTED` and decides whether to REJOIN_GRANT or UNBIND_ALL (per algorithm C).

#### C. DC Decision Tree on Kick

1. If the kicked node is the DC itself → DC is gone; trigger `CoordinatorLifecycle::Active → Suspect → Handover` (per RFC-0855p-b).
2. If the kicked node is a witness → emit `KICK_DETECTED`; quorum drops by 1; check if quorum is still met.
3. If the kicked node is a regular member → emit `MEMBER_REMOVED` (informational); do NOT trigger REBIND.
4. If multiple nodes are kicked (≥ 2 within `KICK_DETECTION_GRACE_PERIOD`) → emit `UNBIND_ALL` (per RFC-0850p-d §F); the group may be under attack.
5. If the DC is the kicked node's target of a rejoin, the DC emits `REJOIN_GRANT` with a fresh `INVITE` envelope (per RFC-0850p-d §E).

#### D. Rejoin (after kick)

1. The kicked node emits `REJOIN_REQUEST` to the DC (unicast).
2. The DC verifies the request (signature + nonce).
3. If the DC's policy allows rejoin (e.g., `MAX_REJOIN_ATTEMPTS = 3` per mission), the DC emits `REJOIN_GRANT` with a fresh `invite_token`.
4. The kicked node uses the new `invite_token` to call `adapter.join_group(group_jid, invite_token)`.
5. On successful platform join, the kicked node emits a fresh BIND (per RFC-0850p-c) with `is_reconnect: true`.
6. On BIND, the local `GroupRegistry` transitions `UnboundQuarantined → Bound`.

### Lifecycle Requirements

- **TTL:** 100 epochs (SELF_KICKED, KICK_DETECTED, MEMBER_REMOVED); 100 epochs (REJOIN_REQUEST, REJOIN_GRANT).
- **Replay protection:** `NonceReplayTable` per RFC-0850p-c §8.
- **Detection deadline:** 5 epochs (per E2E IS-5.1); 50 epochs for status timeout.

### Determinism Requirements

All envelope types MUST serialize deterministically per RFC-0126 (DCS). Specifically:
- `String` fields (e.g., `group_jid`) MUST be UTF-8 with no trailing null bytes.
- The 10-byte canonical header precedes all envelope-specific fields.
- The `PlatformKickEvent` enum MUST be serialized as a single byte (not a varint).
- The `kick_detection_grace_period` value (in epochs) is a configuration parameter; different nodes MAY have different values, but the same node MUST use a consistent value.

### RFC-0008 Execution Class Mapping

| Operation | Class | Rationale |
|-----------|-------|-----------|
| SELF_KICKED sign + broadcast | B | Triggers `UnboundQuarantined` transition; must be deterministic |
| KICK_DETECTED sign + broadcast | B | Triggers slash 0x0011; must be deterministic |
| MEMBER_REMOVED sign + broadcast | C | Informational only; no state transition |
| REJOIN_REQUEST sign + send | C | Out-of-band; no consensus |
| REJOIN_GRANT sign + send | C | DC-unicast; no consensus |
| `UnboundQuarantined` transition | B | Shared state; must be deterministic |
| `Bound` transition (rejoin) | B | Shared state; must be deterministic |

### Error Handling

#### Error codes — `SELF_KICKED.platform_event`

| Code | Name | Description |
|------|------|-------------|
| 0x00 | YouGotKicked | Platform reported "you were removed" (e.g., WhatsApp `GroupParticipantRemove` with `phone == self`) |
| 0x01 | YouLeft | Platform reported "you left" (e.g., Telegram `status: left`) |
| 0x02 | GroupDissolved | Platform reported "group was deleted" |
| 0x03 | GroupDisappeared | Group is unreachable (e.g., banned, network error) |
| 0x04 | SessionLost | Platform session expired; cannot determine status |

#### Error codes — `KICK_DETECTED.reason_code`

| Code | Name | Description |
|------|------|-------------|
| 0x1001 | StatusTimeout | Local node could not determine its status within `KICK_DETECTION_TIMEOUT = 50` epochs |
| 0x1002 | WitnessObservation | A witness independently observed the kick via platform query |
| 0x1003 | DcObservation | The DC observed the kick via platform query |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Kick detection (event-based) | <1 epoch (≈1s) | Platform event-driven |
| Kick detection (heartbeat fallback) | <50 epochs (≈50s) | Worst case |
| SELF_KICKED propagation | <5s | Bounded by overlay propagation (RFC-0850) |
| REJOIN_GRANT → re-BIND end-to-end | <10s | DC re-issue + platform join + BIND |
| False-positive rate | <1% | Tuned by `KICK_DETECTION_GRACE_PERIOD` |

## Implicit Assumptions Audit

| Assumption | Where Relied Upon | Blast Radius if False | Mitigation / Status |
|------------|-------------------|----------------------|---------------------|
| The platform emits a kick event for the local user (not just a roster change) | §Algorithms A | Bot does not detect its own kick; group becomes zombie | Mitigation: heartbeat fallback via `KICK_DETECTION_TIMEOUT` |
| The witness's `peer_id` ↔ phone / user id mapping is correct | §Algorithm B | Witness reports a false-positive kick | Mitigation: slash 0x0010 on false witness assertion (per RFC-0850p-d) |
| The DC has a `MAX_REJOIN_ATTEMPTS` policy per mission | §Algorithm D.3 | DC re-issues REJOIN indefinitely; quota exhaustion | Mitigation: per-mission policy; default `MAX_REJOIN_ATTEMPTS = 3` |
| The platform's "group dissolved" event is distinguishable from "session lost" | §Per-Adapter (WhatsApp) | Bot reports `GroupDissolved` when it was actually a session timeout | Mitigation: WhatsApp-specific cross-check; refine `PlatformKickEvent` enum |
| The DC's `invite_token` is fresh per rejoin (not reused) | §Algorithm D.5 | Replay attack: kicked node re-joins with old token | Mitigation: nonce table per RFC-0850p-c §8; `invite_token` includes nonce |

### Categories to Audit

- **Operator trust** — Assumes the operator's phone / user id is not shared. If shared, a kick for the operator affects the bot. Mitigation: the operator can configure a separate "kick" phone if needed.
- **Platform trust** — Assumes the platform emits a kick event. If the platform silently drops the bot, the heartbeat fallback triggers.
- **Network partition** — Assumes the bot can reach the platform. If partitioned, the heartbeat fallback triggers.
- **Identity stability** — Assumes the bot's `peer_id` is stable across the kick/rejoin. If not, the BIND after rejoin may be rejected. Mitigation: `is_reconnect: true` allows the same `peer_id` to re-BIND.
- **Resource availability** — Assumes the platform allows rejoin. If not, the bot remains in `UnboundQuarantined`. Mitigation: the DC can manually dissolve the group.

## Security Considerations

- **Consensus attacks:** A malicious witness could falsely emit `KICK_DETECTED` for a non-kicked node. Mitigation: slash 0x0010 + `WitnessAssertion` required.
- **Economic exploits:** A kicked node could spam `REJOIN_REQUEST` to consume DC resources. Mitigation: `MAX_REJOIN_ATTEMPTS = 3` per mission.
- **Proof forgery:** `KICK_DETECTED` requires a `WitnessAssertion`; forging requires a witness's key.
- **Replay attacks:** All envelopes have a 16-byte nonce; `NonceReplayTable` per RFC-0850p-c §8.
- **Determinism violations:** All envelopes serialize via DCS; the state machine transitions are deterministic.

## Adversary Analysis

### Decision Table

| Decision | Q1 Beneficiary | Q2 Cost to Attacker | Q3 Gain if Successful | Q4 Defense (cost to legit op) | Q5 Residual Risk |
|----------|----------------|---------------------|------------------------|------------------------------|------------------|
| Accept SELF_KICKED from a bound node | Ejected bot | Burn bot identity (slash 0x0011) | Force the group into `UnboundQuarantined` | Slash 0x0011 on false SELF_KICKED; rate-limit | Acceptable: bot is slashable; quorum drops by 1, DC can REBIND |
| Accept KICK_DETECTED from a witness | Ejected bot's adversary | Burn witness identity (slash 0x0010) | DoS a non-kicked node | Slash 0x0010 on false `WitnessAssertion` | Acceptable: witness is slashable |
| DC grants REJOIN_REQUEST | Ejected bot | None (legitimate rejoin is free) | None | DC controls `MAX_REJOIN_ATTEMPTS` | Acceptable: rejoin is a legitimate use case |
| MEMBER_REMOVED triggers REBIND | Adversary | Burn DC identity | Force REBIND storms | MEMBER_REMOVED is informational; only triggers `membership_log` update | Acceptable: REBIND is not triggered by MEMBER_REMOVED |

### Severity Classification

| Severity | Issue | Action |
|----------|-------|--------|
| HIGH | False-positive KICK_DETECTED from compromised witness | Slash 0x0010 + `WitnessAssertion` required |
| HIGH | Zombie partition (kicked bot not detected) | `KICK_DETECTION_TIMEOUT = 50` epochs + slash 0x0011 |
| MEDIUM | REJOIN spam | `MAX_REJOIN_ATTEMPTS = 3` per mission |
| LOW | MEMBER_REMOVED confusion (informational vs. action) | Documented in spec; no state transition |

## Economic Analysis

This RFC has no token-economic implications. The slash codes (0x0010, 0x0011) are tracked in RFC-0855p-b §5.

## Compatibility

- **Backward compatibility:** This RFC adds new envelope types; existing BIND / REBIND / UNBIND envelopes are unchanged.
- **Forward compatibility:** `version: u8` is reserved; future versions may add new fields but MUST NOT change the field order or remove existing fields.
- **Adapter compatibility:** Adapters that do not support kick detection MUST return `PlatformError::Unsupported`; the node MUST then transition to `UnboundQuarantined` with `reason_code = 0x1001` (StatusTimeout).

## Test Vectors

Test vectors are defined in `crates/octo-network/src/dot/binding/test_vectors.rs`. At minimum:

1. **SELF_KICKED round-trip** — Sign and verify a SELF_KICKED with a known `domain_id`, `group_jid`, and `platform_event`.
2. **KICK_DETECTED with WitnessAssertion** — Sign and verify a KICK_DETECTED with a valid `WitnessAssertion`.
3. **REJOIN flow** — A node is kicked, emits SELF_KICKED, transitions to `UnboundQuarantined`, receives REJOIN_GRANT, re-joins, and re-BINDs successfully.
4. **Heartbeat fallback** — A node loses platform connectivity; after 50 epochs, transitions to `UnboundQuarantined` with `reason_code = 0x1001`.
5. **DC decision tree** — DC is kicked; transition `Active → Suspect → Handover` (per RFC-0855p-b).

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| **Option A: Event-based + heartbeat fallback (this RFC)** | Real-time; works across platforms | Requires per-adapter event subscription |
| Option B: Heartbeat-only (no event subscription) | Simple | High latency; false positives |
| Option C: Periodic `get_group_info` poll | Works without events | High overhead; latency |
| Option D: Re-derive group state from platform events | Most accurate | Requires complex event correlation |

## Implementation Phases

### Phase 1: Envelope types + state machine

- [ ] Add `SelfKickedEnvelope`, `KickDetectedEnvelope`, `MemberRemovedEnvelope`, `RejoinRequestEnvelope`, `RejoinGrantEnvelope` types in `crates/octo-network/src/dot/binding.rs`.
- [ ] Extend `GroupRegistry` with `unbound_quarantined_at: Option<Epoch>` and `rejoin_attempts: BTreeMap<[u8; 32], u16>`.
- [ ] Implement `GroupState::Bound → UnboundQuarantined` transition in `crates/octo-network/src/dot/group_registry.rs`.
- [ ] Slash 0x0011 (SelfKicked) in `crates/octo-network/src/dot/slash.rs`.
- [ ] Unit tests: round-trip serialization, state machine transitions, slash codes.

### Phase 2: WhatsApp adapter integration

- [ ] Subscribe to `GroupParticipantRemove` WebSocket event in `octo-adapter-whatsapp`.
- [ ] Match `phone_number` to local `GroupConfig.operator_phone`.
- [ ] Cross-check via `get_group_info` within `KICK_DETECTION_GRACE_PERIOD`.
- [ ] Heartbeat fallback: 50-epoch timeout, emit `SELF_KICKED` with `GroupDissolved`.
- [ ] Integration test: bot is removed from WhatsApp group; detects within 5 epochs; emits `SELF_KICKED`.

### Phase 3: Matrix and Telegram adapter integration

- [ ] Subscribe to Matrix `/sync` m.room.member events in `octo-adapter-matrix`.
- [ ] Subscribe to Telegram `ChatMember` updates in `octo-adapter-telegram`.
- [ ] Implement kick detection logic for both adapters.
- [ ] Integration test: bot is removed from Matrix room / Telegram supergroup; detects within 5 epochs.

### Phase 4: REJOIN flow

- [ ] Implement `REJOIN_REQUEST` emission on the kicked node.
- [ ] Implement `REJOIN_GRANT` logic on the DC.
- [ ] Implement `MAX_REJOIN_ATTEMPTS` enforcement.
- [ ] Integration test: kicked node re-joins via REJOIN_GRANT, re-BINDs successfully.

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/octo-network/src/dot/binding.rs` | Add SELF_KICKED, KICK_DETECTED, MEMBER_REMOVED, REJOIN_REQUEST, REJOIN_GRANT envelope types |
| `crates/octo-network/src/dot/group_registry.rs` | Add `UnboundQuarantined` transition, `rejoin_attempts` counter |
| `crates/octo-network/src/dot/slash.rs` | Add slash code 0x0011 (SelfKicked), 0x0010 (FalseWitness) |
| `crates/octo-network/src/dot/dc.rs` (new) | DC decision tree for kick events (algorithm C) |
| `crates/octo-adapter-whatsapp/src/adapter.rs` | Subscribe to `GroupParticipantRemove` event |
| `crates/octo-adapter-matrix/src/lib.rs` | Subscribe to m.room.member state events |
| `crates/octo-adapter-telegram/src/lib.rs` | Subscribe to `ChatMember` updates |

## Future Work

- **F-1: Multi-platform kick correlation** — Detect coordinated kicks across multiple platforms (e.g., WhatsApp + Matrix); emit `KICK_DETECTED` for all platforms simultaneously.
- **F-2: Anti-sybil kick** — Detect if a single operator is being kicked repeatedly across missions; rate-limit the operator's missions.
- **F-3: Rejoin audit log** — Log all rejoin attempts for forensic analysis.
- **F-4: Graceful UNBIND on dissolution** — When `GroupDissolved` is detected, emit `UNBIND_ALL` automatically (cross-RFC with RFC-0850p-f).

## Rationale

- **Why event-based + heartbeat fallback?** Event-based is real-time (≤ 1 epoch) but platform-dependent. The heartbeat fallback ensures the bot is never stuck in a zombie state.
- **Why grace period = 2 epochs?** Most platform glitches (reconnect, transient admin action) resolve within 2 seconds. A 2-epoch grace period filters out these false positives without sacrificing real-time detection.
- **Why separate SELF_KICKED vs. KICK_DETECTED?** SELF_KICKED is self-report (no witness required). KICK_DETECTED is a third-party claim (witness required). Different trust models warrant different envelope types.
- **Why MEMBER_REMOVED is informational only?** A non-DC member's removal does not affect the binding; the group is still functional. Informational envelopes allow audit logging without triggering expensive REBINDs.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-17 | Initial draft |

## Related RFCs

- RFC-0850 (Networking): Deterministic Overlay Transport
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle
- RFC-0855p-c (Networking): DomainCoordinator Role
- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite
- RFC-0850p-f (Networking): Transport Group Decommission (planned; sister RFC)
- RFC-0126 (Numeric): Deterministic Canonical Serialization

## Related Use Cases

- `docs/use-cases/social-platform-transport-layer.md` — "Failure Modes" section
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-G1 / S-G2 / S-G5 (failure-mode sub-scenarios)
- `docs/e2e/2026-06-16-e2e-test-plan.md` — Implicit spec IS-5.1 (CRITICAL)

## Appendices

### A. Worked Example: Bot Kicked from WhatsApp Group

```
T+0:    Bot is a member of WhatsApp group "120363012345678901@g.us" bound to
        domain_id: BLAKE3("mission-alpha:domain-vote-recount").

T+5s:   A group admin removes the bot. WhatsApp WebSocket emits
        `GroupParticipantRemove { group: "120363012345678901@g.us",
                                   phone: "+5511999999999" }`.

T+5s:   The adapter matches the phone to local GroupConfig.operator_phone.
        The adapter waits KICK_DETECTION_GRACE_PERIOD = 2 epochs.

T+7s:   The adapter calls get_group_info("120363012345678901@g.us"). The bot
        is still absent. The adapter emits SELF_KICKED with
        platform_event = YouGotKicked.

T+7s:   Local GroupRegistry transitions Bound → UnboundQuarantined. Slash
        0x0011 (SelfKicked) is recorded.

T+8s:   The bot broadcasts SELF_KICKED to the mesh. Witnesses see it and
        update their local GroupRegistry to UnboundQuarantined.

T+10s:  The DC sees SELF_KICKED for a regular member. The DC emits
        MEMBER_REMOVED (informational) and updates the membership_log.
        No REBIND is triggered.

T+15s:  The bot emits REJOIN_REQUEST to the DC. The DC verifies and emits
        REJOIN_GRANT with a fresh invite_token.

T+20s:  The bot uses the new invite_token to call
        adapter.join_group("120363012345678901@g.us", invite_token). WhatsApp
        re-adds the bot.

T+21s:  The bot emits a fresh BIND with is_reconnect: true. The local
        GroupRegistry transitions UnboundQuarantined → Bound.
```

### B. Heartbeat Fallback Pseudocode

```rust
async fn heartbeat_check(group_jid: &str) -> Option<PlatformKickEvent> {
    let deadline = current_epoch() + KICK_DETECTION_TIMEOUT;
    while current_epoch() < deadline {
        match adapter.get_group_info(group_jid).await {
            Ok(info) if info.members.contains(&self_phone()) => return None,  // still in group
            Ok(_) => return Some(PlatformKickEvent::YouGotKicked),
            Err(PlatformError::NotFound) => return Some(PlatformKickEvent::GroupDissolved),
            Err(_) => sleep(1).await,  // retry
        }
    }
    Some(PlatformKickEvent::GroupDissolved)  // fallback
}
```

---

**Version:** 1.0
**Submission Date:** 2026-06-17
**Last Updated:** 2026-06-17
