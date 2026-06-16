# Mission: 0855p-c — Platform-loss auto-rejoin

## Status

Open (2026-06-16) — future

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work"

## Summary

A kicked member (e.g., removed by platform admin) can request rejoin via a `REJOIN_REQUEST` envelope; the DomainCoordinator signs a rejoin ticket if the kick was unauthorized. This handles the case where a platform admin accidentally removes a member (e.g., a mass-kick bot, an admin's phone being compromised), without requiring a full UNBIND+BIND cycle.

## Design

1. **Rejoin request format:**
   ```rust
   pub struct RejoinRequest {
       pub domain_id: DomainId,
       pub kicked_peer_id: PeerId,
       pub kick_evidence: PlatformKickProof, // signed platform-API response showing the kick
       pub peer_pubkey: PubKey,
       pub reason: String, // human-readable: "I was kicked by mistake"
       pub signed_at_epoch: Epoch,
   }
   ```
2. **DC verification:**
   - Verify the `kick_evidence` (the platform's signed response showing the kick).
   - Check the kick reason: if it's `KICK_REASON_MASS_KICK` (e.g., the platform admin's phone was compromised and a mass-kick bot ran), allow rejoin.
   - Check the DC's `kick_log`: if the kick was authorized (e.g., the DC requested it), reject rejoin.
3. **Rejoin ticket:**
   ```rust
   pub struct RejoinTicket {
       pub domain_id: DomainId,
       pub peer_id: PeerId,
       pub rejoin_token: Signature,
       pub expires_at_epoch: Epoch, // 100 epochs (~100 minutes)
   }
   ```
4. **Rejoin process:** the kicked peer presents the `RejoinTicket` to the platform group (e.g., rejoins the WhatsApp group with the rejoin_token as proof of DC authorization).
5. **Rate limit:** a peer can request rejoin at most once per `REJOIN_COOLDOWN_EPOCHS = 1000` epochs (~16 hours). Prevents rejoin abuse.

## Acceptance Criteria

- [ ] `REJOIN_REQUEST` envelope type
- [ ] `RejoinTicket` envelope type
- [ ] `crates/octo-network/src/dc/rejoin.rs` — rejoin handler
- [ ] `REJOIN_COOLDOWN_EPOCHS = 1000` constant
- [ ] Platform-API integration for kick evidence verification
- [ ] Unit tests: authorized rejoin, unauthorized rejoin, rate limit, ticket expiry
- [ ] Integration test: full rejoin flow (kick → request → ticket → rejoin)
- [ ] Documentation: how to request rejoin (peer guide)
- [ ] Documentation: when DCs should sign rejoin tickets (best practices)

## Dependencies

Depends on:
- Platform API integration for kick evidence
- Mission 0855p-c-admin-attestation (the DC's authority verification flow)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/dc/rejoin.rs` (new).

## Complexity

Medium (~400 lines; rejoin request handler, ticket issuance, rate limiting).

## Prerequisites

- Mission 0855p-c-admin-attestation (the DC's authority)
- Platform API integration for kick evidence

## Notes

### Why 1000-epoch cooldown?

1000 minutes (~16 hours) prevents rejoin abuse (a peer requesting rejoin many times to spam the DC). After 16 hours, the cooldown expires and rejoin can be requested again.

### Why a ticket, not a direct admission?

A ticket can be verified offline (the peer shows the ticket to the platform group; the group verifies the DC's signature). A direct admission would require an online check with the DC, which may not be available.

### Type Coverage

| RFC-0855p-c Type | Implemented By |
|-----------------|----------------|
| `REJOIN_REQUEST` envelope type | This mission |
| `RejoinTicket` envelope type | This mission |
| `REJOIN_COOLDOWN_EPOCHS = 1000` constant | This mission |

### Implementation Guide

Reference: `crates/octo-network/src/dc/rejoin.rs` (new).

## Mitigates

D-DC-10 (accidental mass-kick recovery)

## Deadline

Future
