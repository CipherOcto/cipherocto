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

- [ ] `SelfKickedEnvelope` (subtype `b"SFCK"` per RFC-0850p-e §"Envelope Types Added"; R16 R2 fix — was subtype 0x20 in v1.0; the canonical format is the 4-byte ASCII tag per RFC-0850p-c §A) in `crates/octo-network/src/dot/binding.rs` with canonical 10-byte header (`envelope_type: [u8; 4] = b"DOT1"`, `envelope_subtype: [u8; 4] = b"SFCK"`, `version: u16 = 0x0001`) plus body fields: `domain_id`, `group_jid`, `platform`, `platform_event: PlatformKickEvent`, `detected_at_epoch`, `nonce`, `signature`
- [ ] `KickDetectedEnvelope` (subtype `b"KFDT"`; R16 R2 fix — was 0x21) with `kicked_node_id` and `witness_assertion: WitnessAssertion`
- [ ] `MemberRemovedEnvelope` (subtype `b"MREM"`; R16 R2 fix — was 0x22) for informational use by the DC
- [ ] `RejoinRequestEnvelope` (subtype `b"RJRQ"`) and `RejoinGrantEnvelope` (subtype `b"RJGT"`; R16 R2 fix — was 0x23/0x24) for the REJOIN flow
- [ ] `PlatformKickEvent` enum: `YouGotKicked = 0x00`, `YouLeft = 0x01`, `GroupDissolved = 0x02`, `GroupDisappeared = 0x03`, `SessionLost = 0x04` (kick-detection-layer classification; the canonical adapter event is RFC-0855p-c §3 `PlatformEvent::KickedFromGroup { group_jid, kick_epoch, kicker_participant_id }`, which the adapter maps to one of these `PlatformKickEvent` values — see RFC-0850p-e §"Per-Adapter Detection Strategies")
- [ ] DCS serialization (RFC-0126) for all envelope types; round-trip byte equality test
- [ ] Unit tests: signature verification, nonce uniqueness, 10-byte canonical header

### Phase 2: GroupRegistry extensions

- [ ] Add `unbound_quarantine: BTreeMap<(MissionId, DomainId, Platform), UnboundQuarantineEntry>` map to `GroupRegistry` (per RFC-0850p-c §B "GroupRegistry Local State", R16 R1-M7 fix — NOT a `GroupBinding.unbound_quarantined_at: Option<Epoch>` field, since `GroupBinding` is in RFC-0850p-c and should not be modified)
- [ ] Add `rejoin_attempts: BTreeMap<[u8; 32], u16>` to `GroupRegistry`
- [ ] Implement `Bound → UnboundQuarantined` transition on local `SELF_KICKED` emission (move binding entry from `bindings` to `unbound_quarantine`)
- [ ] Implement `Bound → UnboundQuarantined` transition on `KICK_DETECTED` from a witness with valid `WitnessAssertion` (same move)
- [ ] Implement `UnboundQuarantined → Bound` transition on `REJOIN_GRANT` + successful re-BIND (move entry back to `bindings`; if quarantine window expired, fail with `QuarantineExpired` error)
- [ ] Implement `MEMBER_REMOVED` does NOT trigger `REBIND` (informational only; quarantine state unchanged)
- [ ] Implement `KICK_DETECTION_TIMEOUT = 50` epoch fallback: if status cannot be determined, transition to `UnboundQuarantined` with `reason_code = 0xF001` (StatusTimeout; per RFC-0850p-e §"Reason Codes for KICK_DETECTED" — was 0x1001 in v1.0, R16 R1-M4 fix: moved to 0xF0xx kick-detection layer code space, out of slash reason code space 0x0001-0xFFFF)
- [ ] Implement periodic GC: purge `unbound_quarantine` entries where `current_epoch - unbound_at_epoch >= recovery_window_epochs` (recommended cadence: 1 epoch)
- [ ] Unit tests: each transition path; state machine determinism; GC correctness; rejoin within window vs. after expiry

### Phase 3: Slash codes

- [ ] Slash 0x0011 (`SelfKicked`) in `crates/octo-network/src/dot/slash.rs` — emitted ONLY on `SELF_KICKED` that is later determined to be FALSE (e.g., bot re-BINDS within `REJOIN_GRANT_TIMEOUT = 50` epochs, contradicting the claimed kick). NOT automatically applied on every `SELF_KICKED`.
- [ ] Slash 0x0010 (`FalseWitness`) — defined in RFC-0850p-d §"Slash Reason Codes Added"; reused here for false `KICK_DETECTED.witness_assertion` (the slash tally aggregates both forms of 0x0010)
- [ ] Unit tests: slash codes emitted on the correct triggers; aggregate tally correctness

### Phase 4: WhatsApp adapter integration

- [ ] Subscribe to WhatsApp `GroupParticipantRemove` WebSocket event in `octo-adapter-whatsapp` (R16 R1-M3 fix: was "Business API `getGroupParticipants` polling" in v1.0; the WA Web `GroupParticipantRemove` event is the canonical mechanism per `octo-adapter-whatsapp` crate)
- [ ] Match the removed `phone_number` to the local `GroupConfig.operator_phone`
- [ ] Cross-check via `get_group_info(group_jid)` within `KICK_DETECTION_GRACE_PERIOD = 2` epochs from event DELIVERY timestamp (not emission; R16 R1-H4 fix); suppress the kick if the bot is re-added
- [ ] On transient cross-check error (network glitch, rate limit), retry up to `KICK_DETECTION_GRACE_PERIOD * 5 = 10` epochs with exponential backoff (R16 R1-H4 fix)
- [ ] Heartbeat fallback: 50-epoch timeout, emit `SELF_KICKED` with `platform_event = GroupDissolved` (per RFC-0850p-e §Per-Adapter WhatsApp)
- [ ] Map the WA Web event to the canonical `PlatformEvent::KickedFromGroup` (per RFC-0855p-c §3), NOT a separate `PlatformKickEvent` enum (R16 R1-H1 fix)
- [ ] Integration test (CRITICAL): bot is removed from a WhatsApp group; the bot detects within 5 epochs; emits `SELF_KICKED`; the group transitions to `UnboundQuarantined`; the DC emits `MEMBER_REMOVED` (informational)

### Phase 5: Matrix and Telegram adapter integration

- [ ] Subscribe to Matrix `/sync` `m.room.member` state events in `octo-adapter-matrix` (R16 R1-M3 fix: Matrix `/sync` is the canonical mechanism, not polling)
- [ ] On `membership: ban` → emit `SELF_KICKED` with `platform_event = YouGotKicked`
- [ ] On `membership: leave` → cross-check the `event.sender` to determine if it was a kick or voluntary leave
- [ ] Subscribe to Telegram `Update.chat_member` (NOT `getChatMember` which is a request method, not an update subscription) in `octo-adapter-telegram` (R16 R1-M3 fix)
- [ ] On `Update.chat_member.new_chat_member.status == "kicked"` → emit `SELF_KICKED` with `platform_event = YouGotKicked`
- [ ] On `Update.chat_member.new_chat_member.status == "left"` → treat as voluntary; no envelope emitted
- [ ] Cross-check via `getChatMember(chat_id, user_id)` within `KICK_DETECTION_GRACE_PERIOD` (this is OK as a polling request for cross-check, NOT the primary subscription)
- [ ] Map all 3 adapters' native events to the canonical `PlatformEvent::KickedFromGroup` (per RFC-0855p-c §3)
- [ ] Integration tests for both adapters

### Phase 6: DC decision tree

- [ ] `DcOrchestrator::handle_kick(kick_event) -> Decision` per RFC-0850p-e §Algorithm C (R16 R1-C3 fix: was RFC-0850p-d §C in v1.0, which is the wrong section — the kick decision tree is in 0850p-e, not 0850p-d)
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

- `crates/octo-network/src/dot/binding.rs` (additive: 5 new envelope types; canonical 10-byte header per RFC-0850p-c §A)
- `crates/octo-network/src/dot/group_registry.rs` (additive: `unbound_quarantine` map per RFC-0850p-c §B, `rejoin_attempts`, transitions, GC sweep)
- `crates/octo-network/src/dot/slash.rs` (additive: slash codes 0x0010 `FalseWitness` and 0x0011 `SelfKicked`; consult RFC-0850p-d §"Slash Reason Codes Added" for 0x0010)
- `crates/octo-network/src/dot/dc.rs` (additive: `handle_kick` decision tree; shared with `0850p-d-dc-initiated-group-creation.md` mission; note: kick detection is local-node responsibility, NOT a DC instruction)
- `crates/octo-adapter-whatsapp/src/adapter.rs` (additive: kick event subscription, heartbeat fallback; emits `PlatformEvent::KickedFromGroup` per RFC-0855p-c §3, NOT `PlatformKickEvent` — see RFC-0850p-e §"Per-Adapter Detection Strategies")
- `crates/octo-adapter-matrix/src/lib.rs` (additive: same; uses `/sync` endpoint with `m.room.member` state events)
- `crates/octo-adapter-telegram/src/lib.rs` (additive: same; uses `Update.chat_member`)

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
