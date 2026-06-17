# Mission: 0850p-e — Kick & Platform Membership Change Detection

## Status

Open (2026-06-17)

## RFC

RFC-0850p-e (Networking): Kick & Platform Membership Change Detection — `rfcs/draft/networking/0850p-e-kick-detection.md`

## Summary

Implement the kick detection and rejoin flow specified in RFC-0850p-e. **Closes the CRITICAL E2E implicit spec IS-5.1**: bot is removed from a bound physical group; the bot must detect the removal within 5 epochs and emit `KICK_DETECTED`; otherwise the group becomes a zombie partition that the DC cannot REBIND. This mission adds the new envelope types (`SELF_KICKED`, `KICK_DETECTED`, `MEMBER_REMOVED`, `REJOIN_REQUEST`, `REJOIN_GRANT`), implements per-adapter kick detection (WhatsApp `GroupParticipantRemove`, Matrix `m.room.member`, Telegram `ChatMember`), adds the heartbeat fallback (`KICK_DETECTION_TIMEOUT = 50` epochs), implements the DC decision tree for kicks, and adds the REJOIN flow (`MAX_REJOIN_ATTEMPTS = 3` per mission).

## Dependencies

**Prerequisites (RFCs that must be Accepted or in Active Implementation):**

- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `GroupState::UnboundQuarantined`, BIND / REBIND / UNBIND envelopes (from `0850p-c-base.md` mission)
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — `WitnessAssertion` is shared with third-party BIND
- RFC-0855p-c (Networking): DomainCoordinator Role — DC authority
- RFC-0126 (Numeric): DCS — canonical serialization

**Hard prerequisite:** Base mission `0850p-c-base.md` (Open) must be claimed first to provide the `GroupState` enum and `GroupRegistry`.

## Acceptance Criteria

### Phase 1: Envelope types

- [ ] `SelfKickedEnvelope` (subtype 0x20) in `crates/octo-network/src/dot/binding.rs` with `domain_id`, `group_jid`, `platform`, `platform_event: PlatformKickEvent`, `detected_at_epoch`, `nonce`, `signature`
- [ ] `KickDetectedEnvelope` (subtype 0x21) with `kicked_node_id` and `witness_assertion: WitnessAssertion`
- [ ] `MemberRemovedEnvelope` (subtype 0x22) for informational use by the DC
- [ ] `RejoinRequestEnvelope` (subtype 0x23) and `RejoinGrantEnvelope` (subtype 0x24) for the REJOIN flow
- [ ] `PlatformKickEvent` enum: `YouGotKicked = 0x00`, `YouLeft = 0x01`, `GroupDissolved = 0x02`, `GroupDisappeared = 0x03`, `SessionLost = 0x04`
- [ ] DCS serialization (RFC-0126) for all envelope types; round-trip byte equality test
- [ ] Unit tests: signature verification, nonce uniqueness, 10-byte canonical header

### Phase 2: GroupRegistry extensions

- [ ] Add `unbound_quarantined_at: Option<Epoch>` field to `GroupBinding`
- [ ] Add `rejoin_attempts: BTreeMap<[u8; 32], u16>` to `GroupRegistry`
- [ ] Implement `Bound → UnboundQuarantined` transition on local `SELF_KICKED` emission
- [ ] Implement `Bound → UnboundQuarantined` transition on `KICK_DETECTED` from a witness with valid `WitnessAssertion`
- [ ] Implement `UnboundQuarantined → Bound` transition on `REJOIN_GRANT` + successful re-BIND
- [ ] Implement `MEMBER_REMOVED` does NOT trigger `REBIND` (informational only)
- [ ] Implement `KICK_DETECTION_TIMEOUT = 50` epoch fallback: if status cannot be determined, transition to `UnboundQuarantined` with `reason_code = 0x1001` (StatusTimeout)
- [ ] Unit tests: each transition path; state machine determinism

### Phase 3: Slash codes

- [ ] Slash 0x0011 (SelfKicked) in `crates/octo-network/src/dot/slash.rs` — emitted on `SELF_KICKED` (legitimate or false-positive)
- [ ] Slash 0x0010 (FalseWitness) — already defined in RFC-0850p-d §Slash codes; reused here for false `KICK_DETECTED`
- [ ] Unit tests: slash codes emitted on the correct triggers

### Phase 4: WhatsApp adapter integration

- [ ] Subscribe to WhatsApp `GroupParticipantRemove` WebSocket event in `octo-adapter-whatsapp`
- [ ] Match the removed `phone_number` to the local `GroupConfig.operator_phone`
- [ ] Cross-check via `get_group_info(group_jid)` within `KICK_DETECTION_GRACE_PERIOD = 2` epochs; suppress the kick if the bot is re-added
- [ ] Heartbeat fallback: 50-epoch timeout, emit `SELF_KICKED` with `platform_event = GroupDissolved` (per RFC-0850p-e §Per-Adapter WhatsApp)
- [ ] Integration test (CRITICAL): bot is removed from a WhatsApp group; the bot detects within 5 epochs; emits `SELF_KICKED`; the group transitions to `UnboundQuarantined`; the DC emits `MEMBER_REMOVED` (informational)

### Phase 5: Matrix and Telegram adapter integration

- [ ] Subscribe to Matrix `/sync` `m.room.member` state events in `octo-adapter-matrix`
- [ ] On `membership: ban` → emit `SELF_KICKED` with `platform_event = YouGotKicked`
- [ ] On `membership: leave` → cross-check the `event.sender` to determine if it was a kick or voluntary leave
- [ ] Subscribe to Telegram `ChatMember` updates in `octo-adapter-telegram`
- [ ] On `status: kicked` → emit `SELF_KICKED` with `platform_event = YouGotKicked`
- [ ] On `status: left` → treat as voluntary; no envelope emitted
- [ ] Cross-check via `getChatMember(chat_id, user_id)` within `KICK_DETECTION_GRACE_PERIOD`
- [ ] Integration tests for both adapters

### Phase 6: DC decision tree

- [ ] `DcOrchestrator::handle_kick(kick_event) -> Decision` per RFC-0850p-e §Algorithm C
- [ ] Decision: kicked DC → `CoordinatorLifecycle::Active → Suspect → Handover` (per RFC-0855p-b)
- [ ] Decision: kicked witness → emit `KICK_DETECTED`; quorum drops by 1; check if quorum is still met
- [ ] Decision: kicked regular member → emit `MEMBER_REMOVED` (informational); do NOT trigger REBIND
- [ ] Decision: ≥ 2 nodes kicked within `KICK_DETECTION_GRACE_PERIOD` → emit `UNBIND_ALL` (per RFC-0850p-d §F)
- [ ] Unit tests: each decision path

### Phase 7: REJOIN flow

- [ ] `REJOIN_REQUEST` emission on the kicked node (unicast to DC)
- [ ] `REJOIN_GRANT` logic on the DC (with `MAX_REJOIN_ATTEMPTS = 3` enforcement)
- [ ] Re-join via `adapter.join_group(group_jid, fresh_invite_token)`
- [ ] Re-BIND with `is_reconnect: true` (per RFC-0850p-c)
- [ ] Integration test: kicked node re-joins via REJOIN_GRANT, re-BINDs successfully, transitions back to `Bound`

### Quality gates

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes
- [ ] `cargo test -p octo-adapter-whatsapp` passes
- [ ] `cargo test -p octo-adapter-matrix` passes
- [ ] `cargo test -p octo-adapter-telegram` passes
- [ ] No regression in `0850p-c-base.md` and `0850p-d-dc-initiated-group-creation.md` missions' existing tests
- [ ] **CRITICAL test (E2E IS-5.1):** bot is kicked from a bound group; detection occurs within 5 epochs; `SELF_KICKED` is emitted; `GroupState` transitions to `UnboundQuarantined`

## Location

- `crates/octo-network/src/dot/binding.rs` (additive: 5 new envelope types + `PlatformKickEvent` enum)
- `crates/octo-network/src/dot/group_registry.rs` (additive: `unbound_quarantined_at`, `rejoin_attempts`, transitions)
- `crates/octo-network/src/dot/slash.rs` (additive: slash code 0x0011)
- `crates/octo-network/src/dot/dc.rs` (additive: `handle_kick` decision tree; shared with `0850p-d-dc-initiated-group-creation.md` mission)
- `crates/octo-adapter-whatsapp/src/adapter.rs` (additive: kick event subscription, heartbeat fallback)
- `crates/octo-adapter-matrix/src/lib.rs` (additive: same)
- `crates/octo-adapter-telegram/src/lib.rs` (additive: same)

## Complexity

High (~1500 lines; 5 envelope types, 3 state transitions, 3 adapter integrations with per-platform event subscriptions, heartbeat fallback logic, DC decision tree, REJOIN flow, slash codes, integration tests).

## Prerequisites

- Base mission `0850p-c-base.md` (Open) — must be claimed first to provide the `GroupState::UnboundQuarantined` variant (defined in RFC-0850p-c §1)
- Mission `0850p-d-dc-initiated-group-creation.md` (Open) — sister mission; can be claimed in parallel; the `WitnessAssertion` type is shared
- RFC-0850p-c status: Accepted
- RFC-0850p-d status: Draft (co-implemented with mission)
- RFC-0850p-e status: Draft (this RFC)
- RFC-0855p-c status: Accepted
- RFC-0126 status: Accepted

## Notes

### Why this is the most time-sensitive mission

The E2E test plan `docs/e2e/2026-06-16-e2e-test-plan.md` flags IS-5.1 (kick detection) as **CRITICAL** because:
- A kicked bot silently misses all subsequent DOT envelopes
- The group becomes a zombie partition that the DC cannot REBIND
- The bot may continue to sign envelopes (now meaningless) using stale group state

This mission MUST be prioritized before the public launch of any social-platform transport group (WhatsApp, Matrix, Telegram).

### Why is the heartbeat fallback critical?

Event-based detection (via WebSocket / sync / ChatMember) is platform-dependent. If the platform's event system is down, the bot may never receive the kick event. The heartbeat fallback (50-epoch timeout) ensures the bot transitions to `UnboundQuarantined` even if the event system is broken.

### Why separate SELF_KICKED vs. KICK_DETECTED?

- `SELF_KICKED` is self-report (no witness required). The bot knows it was kicked because the platform told it directly.
- `KICK_DETECTED` is a third-party claim (witness required). A witness or DC observed the kick via platform-side query.

Different trust models warrant different envelope types. This separation also enables different slash semantics: a false `SELF_KICKED` is slash 0x0011 (SelfKicked), while a false `KICK_DETECTED` from a witness is slash 0x0010 (FalseWitness).

### Why MAX_REJOIN_ATTEMPTS = 3?

A kicked node could spam `REJOIN_REQUEST` to consume DC resources. The default `MAX_REJOIN_ATTEMPTS = 3` per mission balances legitimate rejoin (≤ 3) with anti-spam. Per-mission policy can override this default (e.g., for a high-trust mission, `MAX_REJOIN_ATTEMPTS = 10`).

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)
