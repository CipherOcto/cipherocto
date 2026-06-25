# Mission: 0851p-b — DotDomain Bootstrap Types and Algorithm

## Status

Claimed (2026-06-25)

## RFC

RFC-0851p-b (Networking): DotDomain Bootstrap Mode

## Dependencies

- Mission 0851p-a-base-bootstrap-orchestrator (archived) — parent bootstrap orchestrator exists
- Mission 0863a-base-transport-crate (claimed) — octo-transport crate exists

## Acceptance Criteria

- [ ] `DcTrustLevel` enum defined with `from_dc_lifecycle()` constructor
- [ ] `BroadcastDomainHint` struct defined with platform, domain_ref, expected_mission_id, expected_dc_id
- [ ] `DotDomainBootstrapConfig` struct defined with all fields
- [ ] `DomainBootstrapResult`, `RejectedPeer`, `RejectionReason` types defined
- [ ] `PlatformAdapterDotDomain` trait with `join_domain()`, `receive_attestation()`, `receive_gadv_responses()`
- [ ] `dotdomain_bootstrap()` algorithm implemented in BootstrapOrchestrator
- [ ] DC attestation verification (structural + signature + freshness)
- [ ] GroupRegistry state check integration
- [ ] Per-domain peer cap enforcement
- [ ] Parallel bootstrap merge (DotDomain + Mode A)
- [ ] Test vectors TV-DD-1 through TV-DD-5 passing
- [ ] Unit tests for DcTrustLevel derivation from all 8 CoordinatorLifecycle states

### Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `DcTrustLevel` | This mission |
| `BroadcastDomainHint` | This mission |
| `DotDomainBootstrapConfig` | This mission |
| `DomainBootstrapResult` | This mission |
| `RejectedPeer` / `RejectionReason` | This mission |
| `PlatformAdapterDotDomain` | This mission |

## Claimant

Jcode Agent

## Notes

Implementation is in `octo-transport` crate. Types go in new `dom_bootstrap.rs` module. Algorithm is added to existing `bootstrap.rs` BootstrapOrchestrator.
