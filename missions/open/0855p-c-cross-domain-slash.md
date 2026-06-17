# Mission: 0855p-c — Cross-domain slash via mission-level coordinator

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work" (mitigates D-DC-6)

## Summary

When a DomainCoordinator misbehaves, the mission-level coordinator (per RFC-0855p-b) can slash the DomainCoordinator. The slash is recorded in the DomainCoordinator's cross-domain reputation, which affects future election eligibility (per RFC-0855p-b F2). This is a cross-domain slash: the mission-level coordinator operates at the mission level, but the slash is applied to a DomainCoordinator that spans multiple `domain_id`s.

## Design

1. **Slash reason code:** extend the slash code range (defined in RFC-0855p-b §B "Slash Offense Codes") with `0x000F` = `domain_coordinator_misbehavior`. Sub-codes in `slash_reason_data`:
   - `0x000F.01` = `invalid_bind_envelope` (signed a BIND that violated the binding rules)
   - `0x000F.02` = `failed_attest` (didn't respond to ATTEST_CHALLENGE within `CHALLENGE_RESPONSE_EPOCHS`)
   - `0x000F.03` = `censored_legit_member` (refused to sign a legitimate admission)
   - `0x000F.04` = `signed_malicious_envelope` (signed an envelope that violated the mission's policy)
2. **Slash flow:**
   - The mission-level coordinator (RFC-0855p-b) gathers slash evidence (envelopes, attestations, challenges).
   - 2/3 of mission-level witnesses vote to slash the DC.
   - The slash is recorded in the DC's cross-domain reputation (RFC-0855p-c F6).
   - The DC enters `Demoting` state (per RFC-0855p-b); after `2^slash_count` epochs of cool-down, the DC can re-stand.
3. **Cross-domain effect:** The slash is broadcast on the libp2p mesh under `/dot/slash/dc/{dc_pubkey}`; all DomainCoordinators (on all domains the slashed DC manages) refuse to sign envelopes from the slashed DC until the cool-down expires.
4. **Recovery:** A slashed DC can appeal via a governance vote (RFC-0855 §11). Successful appeal restores the DC's reputation; failed appeal extends the cool-down by 2×.

## Acceptance Criteria

- [ ] `0x000F` slash reason code in RFC-0855p-b §B
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
| `0x000F` slash reason code | This mission |
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

### Why `0x000F`?

`0x000F` is the next free code in the reserved range. Sub-codes (`.01` invalid bind, `.02` failed attest, etc.) provide granularity.

### Why a separate slash code from mission-level slashing?

Mission-level slash codes (RFC-0855p-b) target mission coordinators. Domain-level slash codes target DomainCoordinators. The blast radius is different: a DC slash affects all `domain_id`s the DC manages.

## Mitigates

D-DC-6 (malicious DomainCoordinator affects multiple domains)

## Deadline

Post-launch
