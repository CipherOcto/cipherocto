# RFC-0855p-d (Networking): Sub-Group Nesting (INDEX)

## Status

Draft (2026-09-02) — INDEX RFC. Superseded by per-concern split:

- RFC-0855p-d1 — Sub-Group Creation & State
- RFC-0855p-d2 — Sub-DC Delegation Lifecycle
- RFC-0855p-d3 — Routing + Aggregation + Teardown

The monolithic RFC-0855p-d v1.2 (~1200 lines, single spec for CGSB + SDCD/SDRV/SDRT + P2SR/S2PA/SGTP + state machine + Layer-C substrate + test vectors) restructured into 3 sibling RFCs on 2026-09-02 per the plateau declaration (`docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`). 11-wave 5-lens adversarial review loop (W1-W10 + W10.5) confirmed non-convergence; per [[cipherocto-design-principles]] §Discipline at first call site + §No parallel abstractions, restructure chosen over continued wave cycle.

This INDEX retains cross-cutting concerns (dependencies, motivation, high-level overview) and indexes per-concern content to the sibling RFCs. Per-concern §Specification / §Data Structure / §Security / §Test Vectors live in d1/d2/d3. VH rows below preserve the v0.2 → v1.2 monolithic lineage; subsequent v1.3 entries point to the restructure event.

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Slim INDEX for the 0855p-d sub-group nesting RFC chain. Per-concern specification lives in d1 (creation + state), d2 (sub-DC delegation lifecycle), and d3 (routing + aggregation + teardown). This INDEX provides the chain-wide motivation, dependency graph, and version lineage.

## Dependencies (chain-wide)

Shared dependencies across d1/d2/d3:

- RFC-0850 — Deterministic Overlay Transport
- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-initiated group creation, CGROUP envelope, transport invite ceremony
- RFC-0855p-b — Mission Coordinator Lifecycle and slash policy
- RFC-0855p-c — DomainCoordinator authority and lifecycle
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0853 — Overlay Cryptography (OCrypt); §Cryptographic Primitives mandates BLAKE3-256
- RFC-0009 — Identity substrate (canonical `Did` re-exported per RFC-0855p-d1 §Layer placement)

Per-RFC dependencies (intra-chain):

- RFC-0855p-d1 — no intra-chain deps
- RFC-0855p-d2 — depends on RFC-0855p-d1 (subgroup must exist + be `Bound` before delegation has any effect)
- RFC-0855p-d3 — depends on RFC-0855p-d1 (subgroup state machine) + RFC-0855p-d2 (non-parent sub-DC requires valid delegation)

Sibling RFCs (parallel review chain):

- RFC-0855p-e — Mission Coordinator Handover Envelope (sibling RFC; uses `mission_id` 16B field; shares cross-RFC invariant `MAX_FSKEW_EPOCHS = 4`)

## Layer placement (chain-wide)

Direction A→B→C/D/E verified at chain level:

- **Layer A** (RFC-frozen): BLAKE3-256 primitive (RFC-0853), Ed25519 public key (RFC-0853), BLS12-381 G1 48-byte compressed aggregate (RFC-0855p-b §Witness Set Aggregation). Re-exported via `pub use` per RFC-0855p-d1 §Layer placement; no `pub type` alias (per W6 L2 L1 finding).
- **Layer B** (RFC-driven, additive; canonical `Did` per RFC-0009 owned at identity substrate; re-exported via `pub use` per RFC-0855p-d1 §Layer placement).
- **Layer B** (years-stable): all envelope wire types (CGSB, SDCD, SDRV, SDRT, P2SR, S2PA, SGTP outer 10-byte canonical header per RFC-0850p-c §A; inner DCS canonical form per RFC-0126). Unknown subtypes fail closed.
- **Layer C** (per-RFC): state machine (SubGroupState), delegation policy (SubDCDelegationPolicy), aggregation policy (hodn_quorum), teardown grace bound (TEARDOWN_GRACE_EPOCHS), cross-node reconciliation.
- **Layer D** (per-adapter): transport binding (RFC-0850p-c) — none of d1/d2/d3 define transport adapters.
- **Layer E** (per-extension): typed-discriminator registry for `SubGroupAction` (RFC-0855p-d1 owns the struct + namespace; d1 reserves 0x0001 INVITE + d3 reserves 0x0004 ROUTE + 0x0005 AGGREGATE; d2 reserves 0x0002 REVOKE + 0x0003 DISSOLVE; user-extension range 0x0100-0xFFFF).

No central enum for extension-bearing types (SubGroupState / SubGroupAction / RevocationReasonCode / TeardownReasonCode all use `#[non_exhaustive]` + typed discriminator). New variants land without central edits to the core module.

## Motivation (chain-wide)

Mission teams need nested working groups, committees, channels, and bounded administrative scopes beneath one domain. Flat domain IDs lose parentage, policy lineage, broadcast boundaries, and deterministic naming. They also provide no safe mechanism for delegating authority to one child domain.

## Design Goals (chain-wide)

(Added v1.3 per W12 L5 H1 finding — mandatory BLUEPRINT §RFC Process template sub-section. Per-concern design goals live in d1/d2/d3 §Design Goals.)

1. **Per-concern RFC split** — three sibling RFCs (d1 creation+state / d2 delegation / d3 routing+aggregation+teardown). Each owns its data structures, policies, and verification paths independently.
2. **Cross-RFC invariant surface** — `MAX_FSKEW_EPOCHS = 4`, `RACE_EPOCHS = 32`, `MAX_SUBGROUP_DEPTH = 8`, `MAX_ROOT_DELEGATION = 1`, `MAX_DELEGATION_CHAIN_PER_TERM = 256`, `TEARDOWN_GRACE_EPOCHS = 50`, `MAX_AGGREGATE_ATTESTATIONS = 1024`, `MAX_BIND_AWAIT_EPOCHS = 32`, `MAX_BIND_RETRY_COUNT = 3` canonicalized in the lowest layer that owns each (d1 owns creation-side constants; d2 owns delegation-side; d3 owns routing/teardown-side; e owns handover-side).
3. **Typed-discriminator over central enum** — `SubGroupState`, `SubGroupAction`, `RevocationReasonCode`, `TeardownReasonCode` all `#[non_exhaustive]` with RFC-allocated namespace 0x0001–0x00FF + user-extension range 0x0100–0xFFFF.
4. **Per-extension registry** — `SubGroupAction::invite/route/aggregate/...` register at startup; core dispatch unchanged.
5. **Layer A → B → C/D/E only** — no upward dependency. Crypto primitives re-exported via `pub use`, never owned.
6. **Fail-closed on unknown envelopes** — old clients reject unknown subtypes; novel discriminators queue at registry, no panic.

## Concern split (chain-wide)

Three concerns split per restructure (option B):

1. **Creation + state** (RFC-0855p-d1): envelope type, label canonicalization, domain derivation, parent-binding invariant, state machine, depth cap enforcement. Defines CGSB + SubGroupState + SubGroupRecord + SubGroupLabel + SubGroupQuery/Response/AuthorityCheck typed query boundary.
2. **Delegation lifecycle** (RFC-0855p-d2): envelope family for sub-DC authority. Defines SDCD/SDRV/SDRT + SubDCDelegationProof + SubDCDelegationPolicy + chain-depth counter + root-delegation table.
3. **Routing + aggregation + teardown** (RFC-0855p-d3): cross-sub-group messaging envelopes. Defines P2SR/S2PA/SGTP + MemberAttestation + SignersBitmap + hodn_quorum + TEARDOWN_GRACE_EPOCHS.

Cross-cutting invariants:

- `MAX_FSKEW_EPOCHS = 4` (forward-skew tolerance; cross-RFC with RFC-0855p-e)
- `RACE_EPOCHS = 32` (backward-replay bound)
- `MAX_SUBGROUP_DEPTH = 8` (creation cap; RFC-0855p-d1)
- `MAX_ROOT_DELEGATION = 1` (delegation breadth cap; RFC-0855p-d2)
- `MAX_DELEGATION_CHAIN_PER_TERM = 256` (delegation chain depth; RFC-0855p-d2)
- `TEARDOWN_GRACE_EPOCHS = 50` (dissolution bound; RFC-0855p-d3)
- `MAX_AGGREGATE_ATTESTATIONS = 1024` (witness collection cap; RFC-0855p-d3)
- `MAX_BIND_AWAIT_EPOCHS = 32` (BIND deadline; RFC-0855p-d1)
- `MAX_BIND_RETRY_COUNT = 3` (BIND retry cap; RFC-0855p-d1)

## Roles and Authorities (chain-wide)

Authority comes from active parent `GroupBinding`, parent DC coordinator term, mission policy, and optional child-scoped delegation proof (RFC-0855p-d2). Role labels never grant authority by themselves.

| Role            | Create (d1)                                                             | Delegate (d2)                          | Route/Aggregate/Teardown (d3)                    |
| --------------- | ----------------------------------------------------------------------- | -------------------------------------- | ------------------------------------------------ |
| Origin          | Propose label, parent, mission context; cannot create without authority | No                                     | No                                               |
| Coordinator     | Create child under active parent; supply parent signature               | Sign SDCD for child; chain depth ≤ 256 | Sign P2SR/SGTP for child                         |
| Member          | No                                                                      | No                                     | May attest via S2PA; cannot issue P2SR/S2PA/SGTP |
| Sub-coordinator | Create descendants only with explicit descendant scope (RFC-0855p-d2)   | Sign SDCD for descendant child         | Sign P2SR/S2PA/SGTP for own sub-domain           |
| Parent mesh     | Validate envelopes, CGROUP BIND, transport binding                      | Validate envelopes + chain depth       | Validate aggregate + teardown                    |

Per-concern role expansions: RFC-0855p-d1 §Roles and Authorities, RFC-0855p-d2 §Roles and Authorities, RFC-0855p-d3 §Roles and Authorities.

## Adversary Analysis (chain-wide)

Five-Question Test (per RFC): WHO / CAPABILITY / TARGET / CONTROLS / RESIDUAL. Per-concern threat matrices: RFC-0855p-d1 §Adversary Analysis, RFC-0855p-d2 §Adversary Analysis, RFC-0855p-d3 §Adversary Analysis.

Chain-wide residual risks:

- Parent DC key compromise enables parent-term actions (CGSB, SDCD, SDRV, SDRT, P2SR, S2PA, SGTP with parent signature) until coordinator rotation or parent revocation.
- Mesh-aggregator compromise could forge mesh_aggregated_signature; mitigated by BLS12-381 PoP at witness registration (RFC-0855p-b) + cross-aggregator verification.
- Transport-platform outage breaks BIND / UNBIND lifecycle independent of governance lifecycle.

## Implicit Assumptions Audit (chain-wide)

Per-concern audits: RFC-0855p-d1 §Implicit Assumptions Audit, RFC-0855p-d2 §Implicit Assumptions Audit, RFC-0855p-d3 §Implicit Assumptions Audit.

Chain-wide assumptions (all verified at acceptance time, not at envelope signature time):

- Subgroup exists and is in expected state (d1)
- Sub-DC delegated (d2, when applicable)
- Replay-key index current (chain-wide)
- Layer-A hash stable (RFC-0853 §Cryptographic Primitives)
- Coordinator term current (RFC-0855p-c)
- Mission policy immutable from child (parent-child invariant)

Missing data fails closed.

## Security Considerations (chain-wide)

Per-concern security: RFC-0855p-d1 §Security Considerations, RFC-0855p-d2 §Security Considerations, RFC-0855p-d3 §Security Considerations.

Chain-wide threats:

- **Replay and freshness**: covered by `MAX_FSKEW_EPOCHS = 4` + `RACE_EPOCHS = 32` bounds, replay-key tuple encoding (envelope-type-specific), nonce index.
- **Privilege escalation**: covered by `MAX_DELEGATION_CHAIN_PER_TERM = 256` (d2), `MAX_ROOT_DELEGATION = 1` (d2), depth cap (d1).
- **Quorum forgery**: covered by mesh_aggregated_signature covering signers_bitmap + distinct-signer enforcement + hodn_quorum coverage check ordering (d3).
- **Teardown fabrication**: covered by grace-elapsed check + state transition bound (d3).

## Specification (chain-wide)

### Envelope Type Catalog

Subtype tags registered under `DOT/1/CGROUP_SUB`:

| Subtype | Owner        | Purpose                                  |
| ------- | ------------ | ---------------------------------------- |
| CGSB    | RFC-0855p-d1 | Create sub-group under active parent     |
| SDCD    | RFC-0855p-d2 | Issue or refresh sub-DC delegation proof |
| SDRV    | RFC-0855p-d2 | Revoke sub-DC delegation                 |
| SDRT    | RFC-0855p-d2 | Rotate sub-DC key                        |
| P2SR    | RFC-0855p-d3 | Parent-to-sub-group route envelope       |
| S2PA    | RFC-0855p-d3 | Sub-to-parent aggregate envelope         |
| SGTP    | RFC-0855p-d3 | Sub-group teardown proof                 |

Future envelopes MUST NOT collide with these tags. Reserved future range: 0x08-0xFF (RFC-allocated). User-extension envelope tags: 0x0100-0xFFFF (registry).

### State Machine

`SubGroupState` (RFC-0855p-d1):

```rust
#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubGroupState {
    PendingBind,
    Bound,
    Dissolving,
    Dissolved,
}
```

Transitions: `Absent → PendingBind` (CGSB accept per RFC-0855p-d1 §Recipient Verification); `PendingBind → Bound` (BIND success); `PendingBind → Dissolved` (BIND failure or deadline expires); `PendingBind → Dissolving` (parent UNBIND between create-receipt and BIND-commit); `Bound → Dissolving` (explicit UNBIND, parent UNBIND cascade, sub-DC revocation per RFC-0855p-d2); `Dissolving → Dissolved` (SGTP accept per RFC-0855p-d3 §SGTP acceptance step 4 after `TEARDOWN_GRACE_EPOCHS` elapsed).

Future extensions (Suspended / Archived / Frozen) land without central edits to the core enum via `#[non_exhaustive]`.

### Layer-C Substrate Surface

RFC-0855p-d1 §Layer-C Substrate Surface defines the typed query/response boundary:

- `SubGroupQuery` / `SubGroupResponse` / `SubGroupAuthorityCheck` / `SubGroupAction`
- Response kind registry: 0x0001-0x00FF RFC-allocated, 0x0100-0xFFFF user-extension
- Action type registry: 0x0001-0x00FF RFC-allocated, 0x0100-0xFFFF user-extension

RFC-0855p-d2 + RFC-0855p-d3 register their action entries against the shared `SubGroupAction` namespace without modifying the registry module.

### Recipient Verification (chain-wide)

Common path:

1. DCS decode succeeds; `version` is supported.
2. Forward-skew bound: `current_epoch <= local_epoch + MAX_FSKEW_EPOCHS`.
3. Subgroup state matches envelope purpose.
4. Issuer authority verified (parent DC OR valid delegated sub-DC per RFC-0855p-d2).
5. Signature verifies over envelope-specific context.
6. Replay-key index query.
7. (Post-validation) Record replay-key entry only after steps 1–6 pass.

Per-envelope additions:

- S2PA (RFC-0855p-d3): aggregate witness coverage (quorum + distinct-signer + mesh signature).
- SGTP (RFC-0855p-d3): grace-elapsed check + state transition `Dissolving → Dissolved`.

## RFC-0008 Execution Class Mapping (chain-wide)

| Operation                                                  | Class | Owner        |
| ---------------------------------------------------------- | ----- | ------------ |
| CGSB envelope derive + recipient accept                    | A     | RFC-0855p-d1 |
| `SubGroupLabel::new` (UTS-39 confusable + NFC)             | A     | RFC-0855p-d1 |
| `SubGroupState` transition engine                          | C     | RFC-0855p-d1 |
| `SubGroupRecord` cross-node reconciliation                 | C     | RFC-0855p-d1 |
| SDCD/SDRV/SDRT envelope accept                             | C     | RFC-0855p-d2 |
| `SubDCDelegationProof::validate_for` (pure-function scope) | A     | RFC-0855p-d2 |
| `RevocationReasonCode` enum construction                   | A     | RFC-0855p-d2 |
| P2SR envelope accept + route                               | C     | RFC-0855p-d3 |
| S2PA envelope accept + aggregate                           | C     | RFC-0855p-d3 |
| SGTP envelope accept + state transition                    | C     | RFC-0855p-d3 |
| `MemberAttestation` / `SignersBitmap` / `aggregate_id`     | A     | RFC-0855p-d3 |
| `mesh_aggregated_signature` verification                   | A     | RFC-0855p-d3 |
| `hodn_quorum` policy lookup                                | C     | RFC-0855p-d3 |

## Determinism Requirements (chain-wide)

Per-concern determinism: RFC-0855p-d1 §Determinism Requirements, RFC-0855p-d2 §Determinism Requirements, RFC-0855p-d3 §Determinism Requirements.

Chain-wide invariants:

- Canonical encoding (RFC-0126 array-of-u8 form for all typed discriminator fields).
- Replay-key tuple encoding uses canonical BE bytes.
- BLAKE3-256 keyed_hash derivation form is the canonical recipe across the chain (RFC-0855p-d1 §Sub-Domain Derivation Invariant).
- BLS12-381 G1 48-byte compressed aggregate canonical form (RFC-0855p-b §Witness Set Aggregation).
- Cross-replica determinism: identical observed envelope sequences → identical state.

## Lifecycle Requirements (chain-wide)

| Constant                        | Value | Owner                                               |
| ------------------------------- | ----- | --------------------------------------------------- |
| `MAX_SUBGROUP_DEPTH`            | 8     | RFC-0855p-d1                                        |
| `MAX_ROOT_DEPTH`                | 1     | RFC-0855p-d1                                        |
| `MAX_ROOT_DELEGATION`           | 1     | RFC-0855p-d2                                        |
| `MAX_DELEGATION_CHAIN_PER_TERM` | 256   | RFC-0855p-d2                                        |
| `MAX_BIND_AWAIT_EPOCHS`         | 32    | RFC-0855p-d1                                        |
| `MAX_BIND_RETRY_COUNT`          | 3     | RFC-0855p-d1                                        |
| `MAX_FSKEW_EPOCHS`              | 4     | RFC-0855p-d1 + cross-RFC invariant with RFC-0855p-e |
| `RACE_EPOCHS`                   | 32    | RFC-0855p-d1                                        |
| `TEARDOWN_GRACE_EPOCHS`         | 50    | RFC-0855p-d3                                        |
| `MAX_AGGREGATE_ATTESTATIONS`    | 1024  | RFC-0855p-d3                                        |

## Performance Targets (chain-wide)

- Canonical validation target: under 1 ms p95 excluding durable state writes.
- Label resolution target: under 10 ms p95 from cache miss.
- Cross-sub-group route: under 1 ms p95.
- Aggregate verification: under 5 ms p95 for `MAX_AGGREGATE_ATTESTATIONS = 1024`.
- State writes: one transaction per child reservation / per delegation chain update.

Targets are local p95 on agreed reference hardware.

## Compatibility (chain-wide)

RFC-0850p-d CGROUP consumers filter on supported subtype. They see unknown CGSB/SDCD/SDRV/SDRT/P2SR/S2PA/SGTP safely, log unsupported subtype, and do not decode or act. No CGROUP field changes. Old clients cannot create sub-groups or cross-sub-group envelopes because per-subtype parsing and CLI routes are unavailable. Wire addition is backward compatible. Version negotiation uses outer `version`; unsupported version is rejected without fallback.

## Test Vectors (chain-wide)

Per-concern test vectors: RFC-0855p-d1 §Test Vectors (TV-SG-1..5), RFC-0855p-d2 §Test Vectors (TV-SG-6, TV-SG-7), RFC-0855p-d3 §Test Vectors (TV-SG-8, TV-SG-9, TV-SG-9b, TV-SG-9c).

## Alternatives Considered (chain-wide)

Per-concern alternatives: RFC-0855p-d1 §Alternatives Considered, RFC-0855p-d2 §Alternatives Considered, RFC-0855p-d3 §Alternatives Considered.

Chain-wide rejected alternatives:

- **Flat hierarchy only**: working groups need parentage, policy lineage, scoped delegation, and deterministic descendant routing.
- **Tags without nesting**: tag taxonomy cannot prove one active parent, bound child depth, child-scoped authority, or parent-dissolve cascade.
- **Extend CGROUP with optional extension**: optional fields weaken old-client compatibility and make per-concern semantics non-obvious.
- **Monolithic RFC**: confirmed non-convergent after 11-wave review; per-concern split chosen per [[cipherocto-design-principles]] §Discipline at first call site.

## Implementation Phases (chain-wide)

Per-concern phases: RFC-0855p-d1 §Implementation Phases, RFC-0855p-d2 §Implementation Phases, RFC-0855p-d3 §Implementation Phases.

Chain-wide sequencing:

1. **Phase 1** (d1 wire): CGSB envelope + canonical derivation + label validation.
2. **Phase 2** (d1 state + d2 wire): state machine + delegation envelope family.
3. **Phase 3** (d2 policy + d3 wire): delegation policy + cross-sub-group envelope family.
4. **Phase 4** (d3 routing/aggregation/teardown): aggregate verification + teardown grace enforcement.
5. **Phase 5** (cross-RFC surface): client API + dashboards.

Each phase requires prior-phase tests green + prettier clean + clippy clean.

## Key Files to Modify (chain-wide)

Per-concern files:

- `crates/octo-network/src/dot/subgroup_state.rs` (RFC-0855p-d1)
- `crates/octo-network/src/dot/subgroup_delegation.rs` (RFC-0855p-d2)
- `crates/octo-network/src/dot/subgroup_routing.rs` (RFC-0855p-d3)
- `crates/octo-network/src/dot/subgroup_teardown.rs` (RFC-0855p-d3)

Cross-cutting:

- `crates/octo-network/src/dot/mod.rs` — module organization + envelope dispatch.
- `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md` — closure doc for the 11-wave review.

## Economic Analysis

DEFER to RFC-0917 and RFC-0960. The 0855p-d chain defines identity, authority, lifecycle, and validation only. Sub-group creation, delegation, routing, aggregation, and teardown have no direct token transfer, fee, reward, stake, settlement, or accounting surface here.

## Future Work

Per-concern future work: RFC-0855p-d1 §Future Work, RFC-0855p-d2 §Future Work, RFC-0855p-d3 §Future Work.

Chain-wide deferred:

- F-10 (cross-mission children): future work; current invariant requires inherited `mission_id` from one parent.
- F-12 (substrate migration): migrate legacy `blake3::hash(parent_domain_id || sub_label)` to canonical keyed_hash form per RFC-0855p-d1 §Sub-Domain Derivation Invariant.

## Rationale (chain-wide)

The monolithic RFC-0855p-d v1.2 (~1200 lines) combined creation + state + delegation + routing + aggregation + teardown + Layer-C substrate + test vectors + alternative considerations + implementation phases in one document. 11-wave 5-lens adversarial review loop (W1-W10 + W10.5) confirmed non-convergence: each fix batch surfaces fresh C-level audit surface from the new fixes themselves. Per [[cipherocto-design-principles]] §Discipline at first call site + §No parallel abstractions, the disciplined answer is to split per concern rather than continue generating fresh audit surface.

Per-concern split (option B):

- **d1 (creation + state)**: ~400 lines focused on CGSB + state machine + canonical derivation.
- **d2 (delegation lifecycle)**: ~450 lines focused on SDCD/SDRV/SDRT + delegation policy.
- **d3 (routing + aggregation + teardown)**: ~500 lines focused on P2SR/S2PA/SGTP + distinct-signer enforcement + quorum coverage + teardown grace.
- **d INDEX**: ~250 lines cross-cutting concerns + chain-wide overview.

Total: ~1600 lines vs 1200 lines monolithic. Increase justified by reduced per-RFC audit surface + clearer per-concern validation paths + typed-discriminator extensions landing without central edits.

## Version History

| Version | Date       | Changes                                                                                                                                                                                                                                                                  |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 0.1     | 2026-08-15 | Initial draft (sibling RFC-0855p-b / RFC-0855p-c review chain).                                                                                                                                                                                                          |
| 0.2     | 2026-08-19 | Added §Security Considerations + §Implicit Assumptions Audit + §Test Vectors per BLUEPRINT template.                                                                                                                                                                     |
| 1.0     | 2026-08-29 | Spec-elaboration complete (post-W7.5 fix batch); cite sweep PASS.                                                                                                                                                                                                        |
| 1.1     | 2026-09-01 | BLS12-381 PoP at witness registration + UTS-39 confusable codepoint enumeration + HORQ snapshot canonical path + HORC payload_hash domain prefix + MAX_PENDING_ENVELOPES_PER_HODN bound..                                                                                |
| 1.2     | 2026-09-02 | W10 fix batch: HandoverAckPayload attests_to_predecessor_state + aggregate_id derivation + bitmap-vs-quorum coverage + SenderStateSnapshotOrdinal `#[non_exhaustive]` + HANDOVER_RACE_WINDOW const + layer placement rows + 3 BLUEPRINT template sub-sections per file.. |
| 1.3     | 2026-09-02 | **Restructured** into 3-RFC chain (d1/d2/d3). v1.2 1200L → d INDEX 280L + d1 440L + d2 470L + d3 520L = 1710L..                                                                                                                                                          |

## Appendices

(Added v1.3 per W11 L5 H4 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

### A. Subgroup envelope type catalog cross-reference

| Subtype | Owner        | Purpose                                  |
| ------- | ------------ | ---------------------------------------- |
| CGSB    | RFC-0855p-d1 | Create sub-group under active parent     |
| SDCD    | RFC-0855p-d2 | Issue or refresh sub-DC delegation proof |
| SDRV    | RFC-0855p-d2 | Revoke sub-DC delegation                 |
| SDRT    | RFC-0855p-d2 | Rotate sub-DC key                        |
| P2SR    | RFC-0855p-d3 | Parent-to-sub-group route envelope       |
| S2PA    | RFC-0855p-d3 | Sub-to-parent aggregate envelope         |
| SGTP    | RFC-0855p-d3 | Sub-group teardown proof                 |

### B. Glossary

- **Sub-DC**: A non-parent DC delegated authority for one child sub-domain (RFC-0855p-d2).
- **Witness set**: Active child membership eligible to attest in `SubToParentAggregateEnvelope` (RFC-0855p-d3).
- **BIND**: Transport-layer group binding ceremony per RFC-0850p-c.
- **DCS**: Deterministic Canonical Serialization per RFC-0126.

## Related RFCs

- RFC-0850 — Deterministic Overlay Transport
- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-Initiated Transport Group Creation & Invite
- RFC-0853 — Overlay Cryptography (OCrypt)
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0009 — Identity substrate
- RFC-0855p-b — Mission Coordinator Lifecycle and slash policy
- RFC-0855p-c — DomainCoordinator Role and parent DC authority scope
- RFC-0855p-d1 — Sub-Group Creation & State (sibling RFC in this chain)
- RFC-0855p-d2 — Sub-DC Delegation Lifecycle (sibling RFC in this chain)
- RFC-0855p-d3 — Routing + Aggregation + Teardown (sibling RFC in this chain)
- RFC-0855p-e — Mission Coordinator Handover Envelope (sibling RFC; cross-RFC invariant on `MAX_FSKEW_EPOCHS = 4`)

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — DC Delegation
- `docs/use-cases/social-platform-transport-layer.md` — Hierarchical Grouping
