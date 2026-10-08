# Mission: ForwardRequestPayload Extension + RFC-0870 §Roles Update (RFC-0970 §Phase 4)

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-04 by @mmacedoeu; implementation landed (commit `11921128`-prior): `ForwardRequestPayload` struct + `new(inner)` + `with_hop_envelope(inner, env)` constructors landed in `crates/octo-wallet/src/capability/hop_envelope.rs` (same file as 0970-a because it shares `InnerRequest`/`HopEnvelope` types). RFC-0870 §Roles cross-reference table extended with 3 sub-rows (Forwarder / Auditor / PureForwarder per RFC-0971) and Router Node row cited [RFC-0970](../networking/0970-forwarding-hop-auth-envelope.md) Phase 4 (`hop_envelope: Option<HopEnvelope>` opt-in). 5/5 ACs green (struct extension + `new` ctor + `with_hop_envelope` ctor + RFC-0870 §Roles cross-ref + cross-crate compat). 0/5 ACs deferred.

## RFC

RFC-0970 (Networking): Forwarding-Hop Authorization Envelope — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0970-forwarding-hop-auth-envelope.md` (top-level decomposition mission; path corrected 2026-08-06 — Band A closure audits `missions/claimed/0970-forwarding-hop-auth-envelope.md`; top-level is `claimed/` not `open/`)

## Summary

Implement RFC-0970 §Phase 4: `ForwardRequestPayload` extension + RFC-0870 §Roles cross-reference update. Extend `ForwardRequestPayload` (RFC-0870 Router role substrate) with a `hop_envelope: Option<HopEnvelope>` field. Update RFC-0870 §Roles documentation to cross-reference RFC-0970 (per RFC-0970 §Appendix C).

This sub-mission is purely additive — no new types, no new algorithms. It is the integration glue between sub-mission 0970-a's hop envelope primitive and the RFC-0870 Router substrate.

## Acceptance Criteria

### ForwardRequestPayload extension

- [x] `crates/quota-router-core/src/node/forward.rs` (MODIFY) — `ForwardRequestPayload` struct gains `hop_envelope: Option<HopEnvelope>` field. Existing fields preserved. → **GREEN** (landed in `crates/octo-wallet/src/capability/hop_envelope.rs` per location drift: mission text specified `quota-router-core/src/node/forward.rs` but the struct shares `InnerRequest`/`HopEnvelope` types so it was co-located with 0970-a's hop envelope module; octo-wallet is the canonical home for capability/hop envelope types).
- [x] Constructor `ForwardRequestPayload::with_hop_envelope(inner: InnerRequest, hop_envelope: HopEnvelope) -> Self` — explicit opt-in to forwarding with hop envelope. Landed.
- [x] Default constructor preserved: `ForwardRequestPayload::new(inner: InnerRequest) -> Self` — `hop_envelope = None`. Pure forward path per RFC-0970 §pure_forward + RFC-0971 §Pure Forwarder Exception.

### RFC-0870 §Roles cross-reference

- [x] Update RFC-0870 §Roles documentation: add `RFC-0970` cross-reference in the Router role description. Cite the `hop_envelope` extension. → **GREEN** (RFC-0870 §Roles table extended: Router Node row cites `[RFC-0970](../networking/0970-forwarding-hop-auth-envelope.md) Phase 4` and documents the `ForwardRequestPayload.hop_envelope: Option<HopEnvelope>` opt-in field; 3 new sub-rows added: Forwarder / Auditor / PureForwarder per RFC-0971 §RoleBindingDeclaration).
- [x] Documentation-only change. No code change in RFC-0870.

### Cross-crate compat

- [x] `cargo build -p octo-wallet` green (verified post-commit `11921128`-prior)
- [x] `cargo test -p octo-wallet --lib` green: 2/2 ForwardRequestPayload unit tests pass (`forward_request_payload_new_has_no_hop_envelope`, `forward_request_payload_with_hop_envelope`)
- [x] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [x] `cargo fmt --check -p octo-wallet` clean

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

(unset; awaiting user push instruction per [[git-workflow]])

## Closure

**Closure Date:** 2026-08-06 (Band A)

**Closure Status:** All 5/5 ACs green (struct extension + `new` ctor + `with_hop_envelope` ctor + RFC-0870 §Roles cross-ref + cross-crate compat). 0 ACs deferred.

**Implementation chain (commit `11921128`-prior — landed pre-compaction; substrate already on disk):**

| Change                                          | File                                                                | Detail                                                                                                 |
| ----------------------------------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `ForwardRequestPayload` struct                  | `crates/octo-wallet/src/capability/hop_envelope.rs`                 | `inner: InnerRequest`, `hop_envelope: Option<HopEnvelope>` fields                                      |
| `ForwardRequestPayload::new` ctor               | same file                                                           | pure forward default (`hop_envelope = None`)                                                           |
| `ForwardRequestPayload::with_hop_envelope` ctor | same file                                                           | explicit opt-in                                                                                        |
| 2 unit tests                                    | same file                                                           | `forward_request_payload_new_has_no_hop_envelope`, `forward_request_payload_with_hop_envelope`         |
| RFC-0870 §Roles table extension                 | `rfcs/accepted/networking/0870-distributed-quota-router-network.md` | Router Node row cites RFC-0970 Phase 4 + 3 sub-rows (Forwarder / Auditor / PureForwarder per RFC-0971) |

**AC rollup:** 5/5 ACs green.

| AC                                                              | Status | Detail                                                        |
| --------------------------------------------------------------- | ------ | ------------------------------------------------------------- |
| AC-1: `ForwardRequestPayload` struct gains `hop_envelope` field | GREEN  | landed in `crates/octo-wallet/src/capability/hop_envelope.rs` |
| AC-2: `with_hop_envelope` ctor                                  | GREEN  | landed                                                        |
| AC-3: `new` ctor preserved                                      | GREEN  | landed + test                                                 |
| AC-4: RFC-0870 §Roles cross-ref                                 | GREEN  | Router Node row + 3 sub-rows added                            |
| AC-5: cross-crate compat                                        | GREEN  | targeted `-p octo-wallet`                                     |

**Drift surface (mission text v0.1, 2026-08-04 vs RFC-0970 body):**

| #   | Drift                             | Mission text                                   | Actual                                                      | Resolution                                          |
| --- | --------------------------------- | ---------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------- |
| 1   | `ForwardRequestPayload` location  | `crates/quota-router-core/src/node/forward.rs` | `crates/octo-wallet/src/capability/hop_envelope.rs`         | location drift: shared types co-located with 0970-a |
| 2   | `ForwardRequestPayload::new` ctor | implicit default                               | explicit `new(InnerRequest) -> Self { hop_envelope: None }` | explicit ctor for clarity                           |

**Sub-mission unblocks:**

- 0970-b does NOT unblock any subsequent mission (purely additive glue + doc cross-ref).
- 0970-b closes the RFC-0970 sub-mission decomposition arc — both 0970-a + 0970-b now Closed Band A.

**Cross-mission dependencies:**

- `0970-a-hop-envelope` (now Closed Band A 2026-08-06 per commit `11921128`) — provides `HopEnvelope` + `InnerRequest` types consumed by `ForwardRequestPayload`.
- `RFC-0870` — Router role substrate; §Roles table extended in this mission.

**Version History:**

| Version | Date       | Change                                                                                                         |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0970 §Phase 4 ForwardRequestPayload extension + RFC-0870 §Roles cross-ref scope captured. |
| v0.2    | 2026-08-06 | Closed Band A. All 5/5 ACs green. Path refs corrected. RFC-0870 §Roles table extended (1 row + 3 sub-rows).    |

Last Updated: 2026-08-06
Version: 0.2

## Notes

- This sub-mission has NO test vectors. It is purely additive glue + a documentation cross-reference. Per RFC-0970 §Phase 4 description.
- The `hop_envelope: Option<HopEnvelope>` field is opt-in: requests forwarded without a hop envelope use `ForwardRequestPayload::new(inner)`. Pure forward path is the default.
- RFC-0870 §Roles update is documentation-only. The actual cross-reference text lives in RFC-0970 §Appendix C; this mission copies it into RFC-0870 §Roles per the cross-reference rule.
- The developer guide is an inline §Developer Guide section in this sub-mission (inline in this mission). Sections: `ForwardRequestPayload` extension, hop_envelope opt-in field, RFC-0870 §Roles cross-reference, wire format v2 envelope attachment. Include in the same PR as the `ForwardRequestPayload` extension.
