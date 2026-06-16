# Mission: 0855p-c — Cross-platform admin attestation

## Status

Open (2026-06-16) — pre-public-launch (CRITICAL)

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work" (mitigates D-DC-1)

## Summary

Each DomainCoordinator periodically publishes a `PLATFORM_ADMIN_ATTEST` envelope on the libp2p mesh under `/dot/admin/{domain_id}/{platform}` containing a fresh proof of admin status (e.g., a signed platform-API response). Other DomainCoordinators (and external auditors) verify and challenge invalid attestations. This is the long-term mitigation for the platform-admin-key-compromise risk (D-DC-1).

## Design

1. **Attestation format:**
   ```rust
   pub struct PlatformAdminAttest {
       pub domain_id: DomainId,
       pub platform: Platform (enum: WhatsApp, Telegram, Matrix, ...),
       pub platform_group_id: String, // e.g., the WhatsApp group JID
       pub dc_pubkey: PubKey,
       pub proof: PlatformApiResponse, // signed response from the platform API
       pub signed_at_epoch: Epoch,
   }
   ```
2. **Freshness:** `signed_at_epoch >= current_epoch - MAX_ATTEST_AGE_EPOCHS = 100` (~100 minutes at 1-min epochs).
3. **Cadence:** DomainCoordinators publish a fresh attest every `ATTEST_PERIOD_EPOCHS = 50` (~50 minutes). Stale attest (older than 100 epochs) is rejected.
4. **Verification:**
   - The `proof` is a signed response from the platform API (e.g., a WhatsApp Business API response signed by WhatsApp's root key).
   - The verifier checks: (a) proof is signed by the platform's root key; (b) proof asserts that `dc_pubkey` is a current admin of `platform_group_id`; (c) attest is fresh.
5. **Challenge mechanism:** Any peer can emit `ATTEST_CHALLENGE { domain_id, dc_pubkey, reason, evidence }` if they believe the attest is invalid (e.g., the platform revoked admin but the DC didn't update). The challenged DC must respond with a fresh attest within `CHALLENGE_RESPONSE_EPOCHS = 10`. If no response, the DC is considered compromised and slashed.
6. **Cross-platform check:** Each DomainCoordinator verifies the other DC's attest and cross-references with the mission-level coordinator (RFC-0855p-b) to ensure consistency.

## Acceptance Criteria

- [ ] `PlatformAdminAttest` envelope type
- [ ] `crates/octo-network/src/dc/admin_attest.rs` — attestation publisher
- [ ] `crates/octo-network/src/dc/attest_verify.rs` — verifier
- [ ] `MAX_ATTEST_AGE_EPOCHS = 100`, `ATTEST_PERIOD_EPOCHS = 50`, `CHALLENGE_RESPONSE_EPOCHS = 10`
- [ ] Gossip topic `/dot/admin/{domain_id}/{platform}`
- [ ] `ATTEST_CHALLENGE` envelope type
- [ ] Unit tests: fresh attest accepted, stale rejected, challenge flow, cross-platform consistency
- [ ] Integration test: full attest + challenge cycle with simulated platform API
- [ ] Documentation: platform-API integration per platform (WhatsApp, Telegram, Matrix)
- [ ] Documentation: how to verify a platform's root key (out-of-band trust anchor)

## Dependencies

Depends on:
- Platform API integration per platform (WhatsApp Business API, Telegram Bot API, Matrix Client-Server API)
- Out-of-band trust anchor for each platform's root key

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/dc/admin_attest.rs` (new); `crates/octo-network/src/dc/attest_verify.rs` (new).

## Complexity

High (~900 lines; per-platform API integration, attest publisher, verifier, challenge flow).

## Prerequisites

- Platform API credentials and root keys (out-of-band trust anchor)

## Notes

### Why per-platform API integration?

WhatsApp, Telegram, and Matrix all have different admin verification APIs. A unified interface (e.g., `PlatformApi::verify_admin()`) abstracts the platform-specific details.

### Why 100-epoch freshness?

100 minutes is enough time for the slowest platform's API to respond (some have rate limits) but short enough that a compromised admin is detected quickly. The cadence is `ATTEST_PERIOD_EPOCHS = 50` (every 50 minutes), so two consecutive misses would be detected within 100 minutes.

### Type Coverage

| RFC-0855p-c Type | Implemented By |
|-----------------|----------------|
| `PlatformAdminAttest` envelope type | This mission |
| `crates/octo-network/src/dc/admin_attest.rs` | This mission |
| `MAX_ATTEST_AGE_EPOCHS = 100` constant | This mission |
| `ATTEST_CHALLENGE` envelope type | This mission |

### Implementation Guide

Reference: WhatsApp Business API documentation; Telegram Bot API documentation; Matrix Client-Server API specification.

## Mitigates

D-DC-1 (platform-admin key compromise); D-DC-2 (silent platform-admin revocation)

## Deadline

Pre-public-launch
