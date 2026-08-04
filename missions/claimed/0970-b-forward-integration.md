# Mission: ForwardRequestPayload Extension + RFC-0870 §Roles Update (RFC-0970 §Phase 4)

## Status

Claimed (2026-08-04)

## RFC

RFC-0970 (Networking): Forwarding-Hop Authorization Envelope — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0970-forwarding-hop-auth-envelope.md` (top-level decomposition mission)

## Summary

Implement RFC-0970 §Phase 4: `ForwardRequestPayload` extension + RFC-0870 §Roles cross-reference update. Extend `ForwardRequestPayload` (RFC-0870 Router role substrate) with a `hop_envelope: Option<HopEnvelope>` field. Update RFC-0870 §Roles documentation to cross-reference RFC-0970 (per RFC-0970 §Appendix C).

This sub-mission is purely additive — no new types, no new algorithms. It is the integration glue between sub-mission 0970-a's hop envelope primitive and the RFC-0870 Router substrate.

## Acceptance Criteria

### ForwardRequestPayload extension

- [ ] `crates/quota-router-core/src/node/forward.rs` (MODIFY) — `ForwardRequestPayload` struct gains `hop_envelope: Option<HopEnvelope>` field. Existing fields preserved.
- [ ] Constructor `ForwardRequestPayload::with_hop_envelope(inner: InnerRequest, hop_envelope: HopEnvelope) -> Self` — explicit opt-in to forwarding with hop envelope.
- [ ] Default constructor preserved: `ForwardRequestPayload::new(inner: InnerRequest) -> Self` — `hop_envelope = None`. Pure forward path per RFC-0970 §pure_forward + RFC-0971 §Pure Forwarder Exception.

### RFC-0870 §Roles cross-reference

- [ ] Update RFC-0870 §Roles documentation: add `RFC-0970` cross-reference in the Router role description. Cite the `hop_envelope` extension.
- [ ] Documentation-only change. No code change in RFC-0870.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0870 — Router role substrate (`ForwardRequestPayload`)

**Requires (mission gates):**

- `missions/open/0970-forwarding-hop-auth-envelope.md` (top-level)
- `missions/open/0970-a-hop-envelope.md` — `HopEnvelope` + `InnerRequest` types MUST exist before this sub-mission compiles

```yaml
depends_on:
  - 0970-a-hop-envelope # HopEnvelope + InnerRequest
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `ForwardRequestPayload` extension (`hop_envelope: Option<HopEnvelope>` field)
- RFC-0870 §Roles cross-reference (documentation-only)

## Location

- `crates/quota-router-core/src/node/forward.rs` (MODIFY)
- `rfcs/accepted/networking/0870-router-network-layer.md` (MODIFY) — §Roles cross-reference update

## Claimant

@mmacedoeu (ForwardRequestPayload extension stub)

## Pull Request

(unset)

## Notes

- This sub-mission has NO test vectors. It is purely additive glue + a documentation cross-reference. Per RFC-0970 §Phase 4 description.
- The `hop_envelope: Option<HopEnvelope>` field is opt-in: requests forwarded without a hop envelope use `ForwardRequestPayload::new(inner)`. Pure forward path is the default.
- RFC-0870 §Roles update is documentation-only. The actual cross-reference text lives in RFC-0970 §Appendix C; this mission copies it into RFC-0870 §Roles per the cross-reference rule.
- The developer guide is an inline §Developer Guide section in this sub-mission (inline in this mission). Sections: `ForwardRequestPayload` extension, hop_envelope opt-in field, RFC-0870 §Roles cross-reference, wire format v2 envelope attachment. Include in the same PR as the `ForwardRequestPayload` extension.
