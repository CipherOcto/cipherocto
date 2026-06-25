# Mission: 0863p-a — Domain-Governed Transport

## Status

Claimed (2026-06-25)

## RFC

RFC-0863p-a (Networking): Domain-Governed Transport

## Dependencies

- Mission 0863b-node-transport (claimed) — NodeTransport exists
- Mission 0851p-b-dotdomain-bootstrap-mode (claimed) — DotDomain types required

## Acceptance Criteria

- [ ] `GovernedTransport` struct defined with all fields
- [ ] `GovernedTransportLifecycle` enum with `from_domain_trust()` constructor
- [ ] `AdapterConfig`, `Credentials`, `DomainRole` types defined
- [ ] `DcLifecycleEvent` type defined
- [ ] `ReceivedMessage` type defined
- [ ] `FLAG_DEGRADED_DOMAIN` constant defined
- [ ] `NodeTransport::builder()` pattern implemented
- [ ] `GovernedTransport::ready()` method
- [ ] `GovernedTransport::send_best()` with governance checks
- [ ] `GovernedTransport::receive()` with governance checks
- [ ] `find_domain_for_sender()` and `find_domain_for_adapter()` helpers
- [ ] `on_domain_loss()` domain loss detection
- [ ] Auto-bootstrap pipeline (classify → DotDomain → seed list → merge)
- [ ] Unit tests for all lifecycle transitions
- [ ] Unit tests for governance-gated send/receive

### Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `GovernedTransport` | This mission |
| `GovernedTransportLifecycle` | This mission |
| `AdapterConfig` / `Credentials` / `DomainRole` | This mission |
| `DcLifecycleEvent` | This mission |
| `ReceivedMessage` | This mission |
| `FLAG_DEGRADED_DOMAIN` | This mission |

## Claimant

Jcode Agent

## Notes

Implementation is in `octo-transport` crate. Types go in new `governed_transport.rs` module. Builder pattern on `NodeTransport`.
