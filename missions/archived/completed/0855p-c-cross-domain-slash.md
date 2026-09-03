# Mission: 0855p-c — Cross-domain slash via mission-level coordinator

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work" (mitigates D-DC-6)

## Summary

When a DomainCoordinator misbehaves, the mission-level coordinator (per RFC-0855p-b) can slash the DomainCoordinator. The slash is recorded in the DomainCoordinator's cross-domain reputation, which affects future election eligibility (per RFC-0855p-b F2). This is a cross-domain slash: the mission-level coordinator operates at the mission level, but the slash is applied to a DomainCoordinator that spans multiple `domain_id`s.

## Design

1. **Slash reason code:** allocate a slot in the user-extension registry
   (per `octo_coordinator_types::SlashReasonCode::Extension`) at `0x0100`
   = `domain_coordinator_misbehavior`. Per CLAUDE.md §Extension over
   enumeration, the extension namespace is the canonical home for new
   slash reasons that lack a typed variant; this slot is the first user
   allocation after the canonical set `0x0001-0x0012` + F-7 reservation
   `0x0013-0x0016` (RFC-0855p-e) + reserved/rejected `0x0017-0x00FF`.
   Sub-codes in `slash_reason_data` (high 16 bits = `0x0100`; low 16 bits
   = sub-code):
   - `0x0100.01` = `invalid_bind_envelope` (signed a BIND that violated the binding rules)
   - `0x0100.02` = `failed_attest` (didn't respond to ATTEST_CHALLENGE within `CHALLENGE_RESPONSE_EPOCHS`)
   - `0x0100.03` = `censored_legit_member` (refused to sign a legitimate admission)
   - `0x0100.04` = `signed_malicious_envelope` (signed an envelope that violated the mission's policy)
2. **Slash flow:**
   - The mission-level coordinator (RFC-0855p-b) gathers slash evidence (envelopes, attestations, challenges).
   - 2/3 of mission-level witnesses vote to slash the DC.
   - The slash is recorded in the DC's cross-domain reputation (RFC-0855p-c F6).
   - The DC enters `Demoting` state (per RFC-0855p-b); after `2^slash_count` epochs of cool-down, the DC can re-stand.
3. **Cross-domain effect:** The slash is broadcast on the libp2p mesh under `/dot/slash/dc/{dc_pubkey}`; all DomainCoordinators (on all domains the slashed DC manages) refuse to sign envelopes from the slashed DC until the cool-down expires.
4. **Recovery:** A slashed DC can appeal via a governance vote (RFC-0855 §11). Successful appeal restores the DC's reputation; failed appeal extends the cool-down by 2×.

## Acceptance Criteria

- [ ] `0x0100` slash reason code (`Extension(0x0100)` per
  `SlashReasonCode::from_reason_id`) in RFC-0855p-c §9c
- [ ] `slash_reason_data: u32` field for sub-codes
- [ ] `crates/octo-network/src/dc/slash.rs` — DC slash handler
- [ ] Cross-domain reputation update on slash
- [ ] Gossip topic `/dot/slash/dc/{dc_pubkey}`
- [ ] Unit tests: each sub-code, witness vote aggregation, cool-down calculation, appeal flow
- [ ] Integration test: 2/3 vote slashes a DC, all domains see the slash
- [ ] Documentation: how DCs can avoid being slashed (best practices)
- [ ] Documentation: appeal process


### Implementation Guide

Reference: RFC-0855p-b §B (slash reason codes); `crates/octo-network/src/dc/slash.rs` (new).


### Type Coverage

| RFC-0855p-c Type | Implemented By |
|-----------------|----------------|
| `0x0100` slash reason code (`Extension(0x0100)`) | This mission |
| `slash_reason_data: u32` sub-codes | This mission |
| `crates/octo-network/src/dc/slash.rs` | This mission |

## Dependencies

Depends on:
- Mission 0855p-b (slash reason codes base)
- Mission 0855p-c-reputation (the cross-domain reputation store that gets updated)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/dc/slash.rs` (new).

## Complexity

Medium (~400 lines; slash flow integration, cross-domain gossip, appeal flow).

## Prerequisites

- RFC-0855p-b status: Accepted
- Mission 0855p-c-reputation (reputation store)

## Notes

### Why `0x0100`?

`0x0100` is the first free slot in the user-extension registry
`0x0100-0xFFFF` (RFC-0855p-c §9c allocation). The original mission
rationale claimed `0x000F`; that allocation conflicted with `CgGroupSpam`
canonically allocated by RFC-0850p-d §"Slash Reason Codes Added" and
mirrored in `octo-coordinator-types` §Appendix B. Per the R5-OOS-4
reviewer note in `docs/reviews/r16/r16-r5-adversarial-review.md`, the
slot was reassigned in this mission's substrate to `0x0100`, the first
extension-registry slot that survives
`SlashReasonCode::try_from_reason_id` (returns
`Self::Extension(0x0100)`). F-7 slots `0x0013-0x0016` were inspected and
rejected — already allocated to `FalseAttestation` /
`QuorumTimeout` / `TallyTamper` / `LateDelivery` per
RFC-0855p-e handover-substrate. The `0x0100` slot lives in the extension
namespace by design (per CLAUDE.md §Extension over enumeration), so
typed match sites route via `Self::Extension(0x0100) => ...` and delegate
to `octo_network::dc::slash::DcMisbehavior` for the 4 sub-codes.

### Why a separate slash code from mission-level slashing?

Mission-level slash codes (RFC-0855p-b) target mission coordinators. Domain-level slash codes target DomainCoordinators. The blast radius is different: a DC slash affects all `domain_id`s the DC manages.

## Mitigates

D-DC-6 (malicious DomainCoordinator affects multiple domains)

## Deadline

Post-launch
