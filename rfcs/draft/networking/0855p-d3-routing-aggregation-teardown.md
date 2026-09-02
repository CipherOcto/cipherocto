# RFC-0855p-d3 (Networking): Routing + Aggregation + Teardown

## Status

Draft (2026-09-02) — spec elaboration closed (v1.3); part of 0855p-d restructure chain (d1/d2/d3). Supersedes monolithic RFC-0855p-d v1.2 §Cross-Sub-Group Messaging + §Sub-Group Decommission + relevant §Data Structure (P2SR/S2PA/SGTP envelopes + MemberAttestation + TeardownProof) + relevant §Security Considerations + relevant §Test Vectors.

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Part 3 of 3 (0855p-d1 / d2 / d3). Defines cross-sub-group envelope family (`DOT/1/CGROUP_SUB` subtypes `b"P2SR"` / `b"S2PA"` / `b"SGTP"`). `P2SR` = parent-to-sub-group route envelope (broadcast scoped to a child sub-domain). `S2PA` = sub-to-parent aggregate envelope (cross-sub-group witness collection rolled up to parent). `SGTP` = sub-group teardown proof (final attestation that releases resources + cascades dissolution). Aggregator uses signers_bitmap covered by mesh_aggregated_signature + hodn_quorum check + distinct-signer enforcement. Teardown bounded by `TEARDOWN_GRACE_EPOCHS = 50`. Depends on RFC-0855p-d1 (subgroup must exist + be `Bound` or `Dissolving`) and RFC-0855p-d2 (delegation must be valid for non-parent-DC sub-DCs).

## Dependencies

- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-initiated group creation
- RFC-0855p-b — Mission Coordinator Lifecycle and slash policy
- RFC-0855p-c — DomainCoordinator authority and lifecycle
- RFC-0855p-d1 — Sub-Group Creation & State (prerequisite: subgroup must exist + state machine)
- RFC-0855p-d2 — Sub-DC Delegation Lifecycle (prerequisite: non-parent sub-DC requires valid delegation)
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0853 — Overlay Cryptography (OCrypt); §Cryptographic Primitives mandates BLAKE3-256
- RFC-0009 — Identity substrate

## Layer placement

| Concern                                                                                                                                                                              | Layer       | Justification                                                                                                                               |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `P2SR` / `S2PA` / `SGTP` outer wire (10-byte canonical header per RFC-0850p-c §A)                                                                                                    | **Layer B** | Transport envelope; no Layer-C role knowledge embedded in outer wire                                                                        |
| `ParentToSubRouteEnvelope` / `SubToParentAggregateEnvelope` / `TeardownProofEnvelope` inner DCS                                                                                      | **Layer B** | Wire-format semantics; canonical form per RFC-0126                                                                                          |
| `MemberAttestation` (typed discriminator + bounded payload)                                                                                                                          | **Layer C** | Sub-DC-side governance policy; HOW attested = mesh-aggregate witness, but the schema lives in Layer C because it reflects local trust model |
| `hodn_quorum(witness_set_size: usize) -> usize` witness-coverage check                                                                                                               | **Layer C** | Coordinator-side governance policy                                                                                                          |
| `TEARDOWN_GRACE_EPOCHS = 50` + `MAX_AGGREGATE_ATTESTATIONS = 1024` consts                                                                                                            | **Layer C** | Coordinator-side governance policy (lifecycle bound)                                                                                        |
| `SubGroupAction::route` / `SubGroupAction::aggregate` typed-discriminator entries (RFC-0855p-d1 owns the struct + namespace; this RFC owns the `0x0004` / `0x0005` registry entries) | **Layer C** | Layer-C action dispatch                                                                                                                     |
| Re-export (`pub use rfc_0853::Ed25519PublicKey`)                                                                                                                                     | **Layer A** | Crypto primitive; re-export only                                                                                                            |
| BLS12-381 G1 48-byte compressed aggregate (RFC-0855p-b; canonical form per RFC-0853 §Cryptographic Primitives)                                                                       | **Layer A** | Crypto primitive; re-export only                                                                                                            |

Direction A→B→C/D/E verified: this RFC depends on RFC-0853 (Layer A crypto), RFC-0009 (Layer B identity), RFC-0850p-c (Layer B transport), RFC-0126 (Layer A canonical encoding), RFC-0855p-c (RFC-0855p-c Layer C DC authority), RFC-0855p-d1 (Layer C subgroup creation/state), RFC-0855p-d2 (Layer C delegation). No upward dependency.

No Layer C module may parse raw Layer B envelopes. No Layer A codec changes for routing/aggregation/teardown fields. Unknown envelope subtypes fail closed.

## Design Goals

1. Bound cross-sub-group broadcast amplification: each P2SR covers at most one child sub-domain.
2. Bound aggregate witness coverage: `MAX_AGGREGATE_ATTESTATIONS = 1024` distinct signer attestations per S2PA.
3. Enforce distinct signers: no repeated bitmap indices; mesh_aggregated_signature covers the bitmap.
4. Enforce quorum coverage: `signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)` BEFORE accepting aggregate.
5. Bound teardown grace: `TEARDOWN_GRACE_EPOCHS = 50`; SGTP required for `Dissolving → Dissolved` transition.
6. Prevent SGTP fabrication: parent DC OR valid delegated sub-DC (RFC-0855p-d2) signs SGTP proof.
7. Cascade parent dissolution to every descendant; permit independent child dissolution (parent teardown is optional; child teardown is mandatory before parent can reach final state).

## Motivation

Parent-to-child route envelopes (P2SR) and child-to-parent aggregate envelopes (S2PA) are the cross-sub-group primitive. Without explicit envelopes, parent can only address each child as a full broadcast group — broadcast amplification and witness collection become O(N²). Without an SGTP envelope for teardown, dissolution transitions cannot complete cleanly (mesh still routes to `Dissolving` sub-groups). This RFC closes the cross-sub-group surface with explicit envelopes + distinct-signer bitmap quorum + bounded teardown grace.

## Roles and Authorities

| Role        | Issue P2SR                                   | Issue S2PA                            | Issue SGTP                          |
| ----------- | -------------------------------------------- | ------------------------------------- | ----------------------------------- |
| Origin      | None                                         | None                                  | None                                |
| Parent DC   | Yes                                          | Yes                                   | Yes (dissolves own sub-group)       |
| Sub-DC      | Yes (when delegated; RFC-0855p-d2)           | Yes (when delegated; RFC-0855p-d2)    | Yes (when delegated; RFC-0855p-d2)  |
| Member      | May attest via S2PA; cannot issue S2PA       | No (must be aggregated by sub-DC)     | No                                  |
| Parent mesh | Validate envelope; route to child sub-domain | Validate aggregate; forward to parent | Validate teardown; transition state |

Sub-DC cannot route across non-delegated sub-domains, issue aggregates beyond `MAX_AGGREGATE_ATTESTATIONS`, or forge teardown for non-delegated sub-domains.

## Adversary Analysis

### Five-Question Test

1. **Who?** Parent DC, sub-DC, malicious member, external sender, compromised mesh node, expired sub-DC, revoked sub-DC.
2. **Capability?** Craft cross-sub-group envelopes, replay old envelopes, forge aggregated signature, fabricate distinct-signer bitmap, forge teardown proof, amplify broadcast beyond bound.
3. **Target?** Cross-sub-group authority, witness quorum, teardown grace bound, parent state machine, child state machine.
4. **Controls?** Canonical encoding, parent or delegation-validated sub-DC signature, distinct-signer enforcement, bitmap-vs-quorum coverage check, teardown grace bound, replay-key index, state machine.
5. **Residual risk?** Mesh-aggregator compromise, replay cache loss, transport-platform outage.

### Threat Matrix

| Threat                   | Required control                                                                                      | Failure result                |
| ------------------------ | ----------------------------------------------------------------------------------------------------- | ----------------------------- |
| Cross-sub-group replay   | Nonce in replay tuple `(P2SR, dc_id, parent_domain_id, sub_domain_id, term_id, current_epoch, nonce)` | Reject                        |
| Aggregate quorum forgery | mesh_aggregated_signature covers signers_bitmap; bitmap.count_ones() >= hodn_quorum(witness_set_size) | Reject `QuorumForgery`        |
| Distinct-signer bypass   | Reject bitmaps with repeated indices; distinct index count == bitmap.count_ones()                     | Reject `DuplicateSignerIndex` |
| Teardown fabrication     | SGTP signed by parent DC OR valid delegated sub-DC (RFC-0855p-d2); signature over teardown_proof      | Reject                        |
| Teardown grace bypass    | SGTP only valid after `TEARDOWN_GRACE_EPOCHS = 50` elapsed since `Bound → Dissolving`                 | Reject `TeardownTooEarly`     |
| Broadcast amplification  | P2SR covers exactly one child sub-domain; child sub-group membership bounded by child creation        | Reject cross-child P2SR       |
| Aggregate amplification  | S2PA bounded to `MAX_AGGREGATE_ATTESTATIONS = 1024` distinct signers                                  | Reject `AggregateTooLarge`    |
| Sub-DC expired/dissolved | RFC-0855p-d2 delegation record must be current at envelope acceptance                                 | Reject                        |

## Implicit Assumptions Audit

| Assumption                 | Audit precondition                                                                 | Enforcement                                   | Failure handling                         |
| -------------------------- | ---------------------------------------------------------------------------------- | --------------------------------------------- | ---------------------------------------- |
| Subgroup exists            | `(parent_domain_id, sub_domain_id)` is `Bound` (RFC-0855p-d1)                      | SubGroupQuery before P2SR/SGTP accept         | Reject `SubGroupNotBound`                |
| Subgroup transitioning     | For SGTP: state ∈ `{Dissolving}` and grace elapsed                                 | State query + grace check                     | Reject `TeardownTooEarly` for early SGTP |
| Sub-DC delegated           | For non-parent sub-DC: delegation record valid (RFC-0855p-d2)                      | Delegation query before P2SR/S2PA/SGTP accept | Reject `SubDCNotDelegated`               |
| Witness set size known     | `witness_set_size` queried from active child membership                            | SubGroupQuery returns witness_set_size        | Reject `WitnessSetUnknown`               |
| Replay-key index current   | Mesh keeps replay entries per (envelope_type, dc_id, scope, term_id, epoch, nonce) | Nonce index query                             | Reject `ReplayDetected`                  |
| Quorum policy known        | `hodn_quorum(witness_set_size)` returns witness-coverage requirement               | Layer-C policy lookup                         | Reject `QuorumPolicyMissing`             |
| Grace elapsed for teardown | `current_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS`                        | Epoch arithmetic                              | Reject `TeardownTooEarly`                |

Assumption "existing mesh" is not sufficient. Node MUST query canonical subgroup state (RFC-0855p-d1 §Layer-C Substrate Surface), delegation record (RFC-0855p-d2 §SubDCDelegationPolicy), witness-set size, grace elapsed, and replay-key index. Missing data fails closed.

## Security Considerations

### Replay and freshness

Every cross-sub-group envelope carries a `current_epoch` field plus a `nonce`. Replay-key tuples are envelope-type-specific:

- P2SR: `(P2SR, dc_id, parent_domain_id, sub_domain_id, term_id, current_epoch, nonce)`
- S2PA: `(S2PA, dc_id, parent_domain_id, sub_domain_id, term_id, current_epoch, nonce)`
- SGTP: `(SGTP, dc_id, parent_domain_id, sub_domain_id, term_id, current_epoch, nonce)`

Forward-skew bound (clock-drift tolerance): reject envelope where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS` (`MAX_FSKEW_EPOCHS = 4` per RFC-0855p-d1 §Layer placement table; cross-RFC invariant). Backward-replay bound (`RACE_EPOCHS = 32` per RFC-0855p-d1 §Layer placement table) bounds stale-envelope amplification.

mesh_aggregated_signature is a BLS12-381 G1 48-byte compressed aggregate over the per-signer individual signatures; this aggregate MUST cover the signers_bitmap (per W10.5 L3 C1 finding). Substrate implementations MUST verify the aggregate signature covers the bitmap BEFORE applying the bitmap-vs-quorum coverage check — order-sensitive validation.

### Availability and liveness

`TEARDOWN_GRACE_EPOCHS = 50` bounds how long a `Dissolving` sub-group remains routable. After grace elapses, child MUST issue SGTP; absent SGTP, parent can manually force-submit a parent-signed SGTP (parent has authority over its own lineage). `MAX_AGGREGATE_ATTESTATIONS = 1024` bounds S2PA size to prevent witness-collection DoS amplification.

Distinct-signer enforcement: bitmap indices are unique; `bitmap.iter().enumerate().filter(|(_, b)| *b).count() == bitmap.count_ones()`. No repeated indices; otherwise `DuplicateSignerIndex` error.

Bitmap-vs-quorum coverage check ordering: `signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)` MUST be applied AFTER mesh_aggregated_signature verification (BLS signature first, then count). Per W10.5 L3 C2 finding.

## Specification

### Envelope Type

| Envelope Type      | Subtype tag | Direction                   | Description                                                                   |
| ------------------ | ----------- | --------------------------- | ----------------------------------------------------------------------------- |
| `DOT/1/CGROUP_SUB` | `b"P2SR"`   | Parent DC or sub-DC → child | Route envelope from parent (or delegated sub-DC) to a single child sub-domain |
| `DOT/1/CGROUP_SUB` | `b"S2PA"`   | Sub-DC → parent             | Aggregate envelope from sub-DC to parent                                      |
| `DOT/1/CGROUP_SUB` | `b"SGTP"`   | Parent DC or sub-DC → mesh  | Sub-group teardown proof; transitions `Dissolving → Dissolved`                |

Subtype dispatch uses explicit typed parsing. Unsupported subtype remains unknown and never falls back to CGSB/CGROUP/SDCD/SDRV/SDRT. Sibling envelopes owned by RFC-0855p-d1 (CGSB) and RFC-0855p-d2 (SDCD/SDRV/SDRT).

### Data Structure

```rust
pub const MAX_AGGREGATE_ATTESTATIONS: usize = 1024;
pub const MAX_BIND_AWAIT_EPOCHS: u64 = 32;        // re-export from RFC-0855p-d1
pub const MAX_BIND_RETRY_COUNT: u8 = 3;           // re-export from RFC-0855p-d1
pub const TEARDOWN_GRACE_EPOCHS: u64 = 50;
pub const MAX_FSKEW_EPOCHS: u64 = 4;              // re-export from RFC-0855p-d1
pub const RACE_EPOCHS: u64 = 32;                  // re-export from RFC-0855p-d1 (backward-replay bound)
pub const SUBGROUP_ROUTE_CONTEXT: &str = "DOT/1/CGROUP_SUB/route";
pub const SUBGROUP_AGGREGATE_CONTEXT: &str = "DOT/1/CGROUP_SUB/aggregate";
pub const SUBGROUP_TEARDOWN_CONTEXT: &str = "DOT/1/CGROUP_SUB/teardown";
pub const SUBGROUP_ROUTE: [u8; 4] = *b"P2SR";
pub const SUBGROUP_AGGREGATE: [u8; 4] = *b"S2PA";
pub const SUBGROUP_TEARDOWN: [u8; 4] = *b"SGTP";

// Re-exports per RFC-0855p-d1 Layer placement table — these types are NOT
// redefined here per Stable Abstractions Principle + A→B→C dep direction
// rule. RFC-0853 owns crypto primitives; RFC-0009 owns the canonical Did;
// RFC-0855p-d1 owns the SubGroupRecord / SubGroupState / SubGroupLabel.
pub use rfc_0853::Ed25519PublicKey;
pub use rfc_0009::Did;
pub use crate::rfc_0855p_d1::{SubGroupRecord, SubGroupState, SubGroupLabel, DelegationId};
pub use crate::rfc_0855p_d2::{CoordinatorTermId, SubDCDelegationPolicy};

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct ParentToSubRouteEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
    pub route_payload: BoundedBytes<1024>,
    pub dc_signature: [u8; 64],
}

// `MemberAttestation` is the typed-discriminator + bounded-payload scheme
// per RFC-0855p-d1 §Layer-C Substrate Surface (SubGroupAction). This RFC
// owns the route/aggregate dispatch entries; the discriminator namespace
// is RFC-0855p-d1's responsibility.
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct MemberAttestation {
    pub member_did: Did,
    pub attested_epoch: u64,
    pub attestation_payload: BoundedBytes<256>,
    pub attestation_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignersBitmap {
    bits: Vec<u8>, // bit i set iff witness index i attested
}

impl SignersBitmap {
    pub fn from_indices(indices: &[u16]) -> Result<Self, BitmapError> {
        if indices.is_empty() {
            return Err(BitmapError::Empty);
        }
        if indices.len() > MAX_AGGREGATE_ATTESTATIONS {
            return Err(BitmapError::TooLarge);
        }
        // Distinct-signer enforcement: reject duplicate indices BEFORE
        // constructing the bitmap (per W10.5 L3 C3 finding).
        let mut sorted: Vec<u16> = indices.to_vec();
        sorted.sort_unstable();
        if sorted.windows(2).any(|w| w[0] == w[1]) {
            return Err(BitmapError::DuplicateSignerIndex);
        }
        let max_index = *sorted.last().unwrap() as usize;
        let byte_len = (max_index / 8) + 1;
        let mut bits = vec![0u8; byte_len];
        for &idx in &sorted {
            let byte = idx / 8;
            let bit = idx % 8;
            bits[byte] |= 1 << bit;
        }
        Ok(Self { bits })
    }

    pub fn count_ones(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bits
    }
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubToParentAggregateEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
    pub attestations: Vec<MemberAttestation>, // bounded by MAX_AGGREGATE_ATTESTATIONS at decode
    pub signers_bitmap: SignersBitmap,
    pub aggregate_id: [u8; 32],
    pub mesh_aggregated_signature: [u8; 48], // BLS12-381 G1 48-byte compressed
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct TeardownProof {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub tearing_down_dc_id: [u8; 32],
    pub dissolving_epoch: u64,
    pub teardown_epoch: u64,
    pub reason: TeardownReasonCode,
    pub dc_signature: [u8; 64],
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct TeardownProofEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub teardown_proof: TeardownProof,
}

#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TeardownReasonCode {
    ChildVoluntary,
    ParentCascade,
    CoordinatorRotation,
    SubDCRevoked,
    MissionConcluded,
}

fn derive_aggregate_id(
    parent_domain_id: [u8; 32],
    sub_domain_id: [u8; 32],
    attestations: &[MemberAttestation],
    signers_bitmap: &SignersBitmap,
    current_epoch: u64,
    nonce: &[u8; 16],
) -> [u8; 32] {
    let key = blake3::derive_key(SUBGROUP_AGGREGATE_CONTEXT);
    let mut input = Vec::with_capacity(32 + 32 + 8 + 16 + attestations.len() * 256 + signers_bitmap.bytes().len());
    input.extend_from_slice(&parent_domain_id);
    input.extend_from_slice(&sub_domain_id);
    input.extend_from_slice(&current_epoch.to_be_bytes());
    input.extend_from_slice(nonce);
    for att in attestations {
        input.extend_from_slice(att.member_did.as_bytes());
        input.extend_from_slice(&att.attested_epoch.to_be_bytes());
        input.extend_from_slice(att.attestation_payload.as_bytes());
    }
    input.extend_from_slice(signers_bitmap.bytes());
    blake3::keyed_hash(&key, &input).as_bytes().to_owned()
}

pub fn hodn_quorum(witness_set_size: usize) -> usize {
    // Layer-C governance policy: 2/3 super-majority of witness set.
    // `witness_set_size` is the active child membership queried via
    // `SubGroupQuery` (RFC-0855p-d1). The check that consumes this
    // (signers_bitmap.count_ones() >= hodn_quorum(witness_set_size))
    // MUST run AFTER mesh_aggregated_signature verification, not before
    // (per W10.5 L3 C2 finding).
    if witness_set_size == 0 {
        return 0;
    }
    (witness_set_size * 2).div_ceil(3)
}
```

### Cross-Sub-Group Messaging (P2SR + S2PA)

**P2SR — parent (or delegated sub-DC) to child route**

1. Issuer (parent DC or delegated sub-DC per RFC-0855p-d2) constructs envelope with `route_payload`.
2. Issuer signs canonical DCS encoding over `SUBGROUP_ROUTE_CONTEXT` derivation-key domain.
3. Envelope dispatched to child sub-domain (single sub-group, not broadcast across all children).

**S2PA — sub-DC to parent aggregate**

1. Sub-DC collects `MemberAttestation`s from active child members.
2. Sub-DC constructs `signers_bitmap` from attested member indices via `SignersBitmap::from_indices`.
3. Sub-DC computes BLS12-381 G1 aggregate signature `mesh_aggregated_signature` covering both the per-member attestations AND the signers_bitmap (per W10.5 L3 C1 finding).
4. Sub-DC derives `aggregate_id = BLAKE3_keyed(SUBGROUP_AGGREGATE_CONTEXT, parent_domain_id || sub_domain_id || current_epoch || nonce || attestations || signers_bitmap)` (canonical derivation per RFC-0855p-d1 §Sub-Domain Derivation Invariant pattern).
5. Sub-DC submits envelope to parent.

**Acceptance (recipient)**

P2SR recipient:

1. DCS decode succeeds; `envelope_subtype == b"P2SR"`; `version` is supported.
2. Forward-skew bound: reject envelope where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS`.
3. Subgroup is `Bound` (RFC-0855p-d1 §SubGroupState); envelope's `parent_domain_id` MUST equal the queried subgroup's `parent_domain_id` (parent-binding check; added v1.3 per W11 L1 M-fbat finding — prevents cross-parent enumeration).
4. Issuer is parent DC OR valid delegated sub-DC (RFC-0855p-d2).
5. Signature verifies over `SUBGROUP_ROUTE_CONTEXT`.
6. Nonce unconsumed under replay key `(P2SR, dc_id, parent_domain_id, sub_domain_id, term_id, current_epoch, nonce)`.

S2PA recipient:

1. DCS decode succeeds; `envelope_subtype == b"S2PA"`; `version` is supported.
2. Forward-skew bound: reject envelope where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS`.
3. Subgroup is `Bound`; envelope's `parent_domain_id` MUST equal the queried subgroup's `parent_domain_id` (parent-binding check; same W11 L1 M-fbat finding).
4. Issuer is sub-DC (not parent — parent aggregates go via different path).
5. Witness-set size queried: `witness_set_size = SubGroupQuery(parent, sub).witness_set_size`.
6. `attestations.len() <= MAX_AGGREGATE_ATTESTATIONS = 1024`.
7. mesh_aggregated_signature verifies over (attestations + signers_bitmap). // BLS FIRST (per W11 L3 H1 finding — re-ordered; previously step 9)
8. Distinct-signer pre-check: `signers_bitmap` has no repeated indices (defensive re-verification per W11 L3 M-fbat finding; reject `BitmapError::DuplicateSignerIndex` before count).
9. `signers_bitmap.count_ones() == attestations.len()` (distinct-signer invariant; reject `BitmapError::AttestationCountMismatch`).
10. `signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)` (quorum coverage).
11. `aggregate_id` recomputed from canonical input matches claimed value.
12. Nonce unconsumed under replay key `(S2PA, dc_id, parent_domain_id, sub_domain_id, term_id, current_epoch, nonce)`.

Order of checks 8 → 9 → 10 → 11 is mandatory (per W10.5 L3 C1/C2 findings). Skip any check → reject.

### Sub-Group Decommission (SGTP)

Sub-group transitions `Bound → Dissolving` either voluntarily (sub-DC or parent DC signed intent) or via parent cascade (parent UNBIND). After `TEARDOWN_GRACE_EPOCHS = 50` epochs, child issues SGTP to transition `Dissolving → Dissolved`.

**Issuance**

1. Issuer (parent DC or delegated sub-DC per RFC-0855p-d2) constructs `TeardownProof { parent_domain_id, sub_domain_id, tearing_down_dc_id, dissolving_epoch, teardown_epoch, reason, dc_signature }`.
2. `teardown_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS`.
3. Issuer signs canonical DCS encoding over `SUBGROUP_TEARDOWN_CONTEXT`.
4. Envelope dispatched to mesh.

**Acceptance (recipient)**

1. DCS decode succeeds; `envelope_subtype == b"SGTP"`; `version` is supported.
2. Forward-skew bound: reject envelope where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS`.
3. Subgroup exists and state ∈ `{Dissolving}`.
4. Grace elapsed: `local_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS` (recipient's local clock; envelope `teardown_epoch` field is informational/audit-only).
5. Issuer is parent DC OR valid delegated sub-DC; delegation record active at envelope acceptance time per RFC-0855p-d2 §SubDCDelegationPolicy (no matching SDRV in `(parent, sub, sub_dc, term)`).
6. Signature verifies over `SUBGROUP_TEARDOWN_CONTEXT`.
7. Nonce unconsumed under replay key `(SGTP, dc_id, parent_domain_id, sub_domain_id, term_id, current_epoch, nonce)`.
8. Transition state `Dissolving → Dissolved`; purge live transport handles; stop delivery (RFC-0855p-d1 §State Machine).

Parent can manually force-submit SGTP for own lineage if child fails to issue one within grace; this is recovery path, not normal path.

### Recipient Verification (P2SR / S2PA / SGTP)

Common path:

1. DCS decode succeeds; `version` is supported.
2. Forward-skew bound: reject envelope where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS`.
3. Subgroup state machine matches envelope purpose (Bound for P2SR/S2PA; Dissolving + grace for SGTP).
4. Issuer authority verified (parent DC OR valid delegated sub-DC per RFC-0855p-d2).
5. Signature verifies over envelope-specific context.
6. Replay-key index query.
7. (Post-validation) Record replay-key entry only after steps 1–6 pass.

Per-envelope additions:

- S2PA: aggregate witness coverage (quorum + distinct-signer + mesh signature).
- SGTP: grace elapsed check + state transition `Dissolving → Dissolved`.

## RFC-0008 Execution Class Mapping

| Operation                                                   | Class | Justification                                                                         |
| ----------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------- |
| P2SR envelope accept + route                                | C     | Cross-node state mutation (routing table update); non-deterministic under partition   |
| S2PA envelope accept + aggregate                            | C     | Cross-node consensus-aggregated witness collection; non-deterministic under partition |
| SGTP envelope accept + state transition                     | C     | Cross-node state mutation (Dissolving → Dissolved); non-deterministic under partition |
| `MemberAttestation` construction                            | A     | Pure-function field construction; deterministic                                       |
| `SignersBitmap::from_indices` (distinct-signer enforcement) | A     | Pure-function validation; deterministic                                               |
| `aggregate_id` derivation (BLAKE3 keyed_hash)               | A     | Pure-function derivation; deterministic                                               |
| `mesh_aggregated_signature` verification (BLS12-381 G1)     | A     | Crypto primitive; deterministic                                                       |
| `hodn_quorum(witness_set_size)` (Layer-C policy lookup)     | C     | Coordinator-side governance policy                                                    |

## Determinism Requirements

- **Canonical encoding**: `aggregate_id = BLAKE3_keyed(SUBGROUP_AGGREGATE_CONTEXT, ...)` recomputed by recipient from attestations + signers_bitmap; recipient MUST recompute deterministically (RFC-0855p-d1 §Sub-Domain Derivation Invariant pattern).
- **Replay-key tuple encoding**: P2SR/S2PA/SGTP `current_epoch` field uses canonical BE bytes (RFC-0126 array-of-u8 form).
- **Distinct-signer enforcement**: bitmap indices MUST be unique; check BEFORE construction.
- **Quorum coverage**: `signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)` runs AFTER mesh_aggregated_signature verification.
- **Grace arithmetic**: `current_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS` deterministic given local epoch.

## Lifecycle Requirements

| Constant                     | Value | Purpose                                                              |
| ---------------------------- | ----- | -------------------------------------------------------------------- |
| `TEARDOWN_GRACE_EPOCHS`      | 50    | `Dissolving → Dissolved` deadline; SGTP required within this window  |
| `MAX_AGGREGATE_ATTESTATIONS` | 1024  | Maximum distinct MemberAttestations per S2PA                         |
| `MAX_FSKEW_EPOCHS`           | 4     | Forward epoch-skew tolerance (cross-RFC invariant with RFC-0855p-d1) |
| `RACE_EPOCHS`                | 32    | Backward-replay bound (cross-RFC invariant with RFC-0855p-d1)        |

State machine: `Bound → Dissolving → Dissolved` (cross-RFC invariant with RFC-0855p-d1). SGTP triggers the final transition; absent SGTP, parent can manually force-submit one (recovery path).

## Performance Targets

- Cross-sub-group route: under 1 ms p95 excluding durable state writes.
- Aggregate verification: under 5 ms p95 for `MAX_AGGREGATE_ATTESTATIONS = 1024`.
- Teardown grace tracking: constant-time per sub-group.

Targets are local p95 on agreed reference hardware.

## Compatibility

RFC-0850p-d CGROUP consumers filter on supported subtype. They see unknown P2SR/S2PA/SGTP safely, log unsupported subtype, and do not decode or act. No CGROUP field changes. Old clients cannot issue cross-sub-group envelopes because P2SR/S2PA/SGTP-specific parsing and CLI routes are unavailable. Wire addition is backward compatible. CGSB (RFC-0855p-d1) and SDCD/SDRV/SDRT (RFC-0855p-d2) subtypes also fail closed. Version negotiation uses outer `version`; unsupported version is rejected without fallback.

## Test Vectors

### TV-SG-8 — S2PA quorum coverage

Input:

- `witness_set_size = 6` (active child members)
- `signers_bitmap = {0, 1, 2, 3}` (4 attestations)
- `hodn_quorum(6) = 4` (2/3 super-majority)
- `attestations.len() = 4`
- Valid BLS12-381 G1 aggregate signature covering (attestations + bitmap)

Expected:

- `signers_bitmap.count_ones() = 4 >= 4 = hodn_quorum(6)` passes.
- mesh_aggregated_signature verifies.
- `aggregate_id` recomputed matches.
- Replay-key index records entry.

### TV-SG-9 — SGTP teardown grace enforcement

Input (early teardown):

- `dissolving_epoch = 100`
- `current_epoch = 120`
- `TEARDOWN_GRACE_EPOCHS = 50`
- `teardown_epoch = 120`
- `reason: ChildVoluntary`

Expected:

- `current_epoch - dissolving_epoch = 20 < 50 = TEARDOWN_GRACE_EPOCHS`.
- Reject `TeardownTooEarly`.
- State remains `Dissolving`.

Input (grace elapsed):

- `dissolving_epoch = 100`
- `current_epoch = 150`
- `teardown_epoch = 150`
- `reason: ChildVoluntary`

Expected:

- `current_epoch - dissolving_epoch = 50 >= 50 = TEARDOWN_GRACE_EPOCHS`.
- State transitions `Dissolving → Dissolved`.
- Transport handles purged; delivery stopped.

### TV-SG-9b — Distinct-signer rejection

Input:

- `signers_bitmap = {0, 1, 2, 2}` (duplicate index 2)
- `attestations.len() = 4`

Expected:

- `SignersBitmap::from_indices([0, 1, 2, 2])` returns `Err(BitmapError::DuplicateSignerIndex)`.
- Substrate rejects envelope.
- No quorum check runs.

### TV-SG-9c — Quorum check order

Input:

- `signers_bitmap = {0, 1, 2}` (3 attestations)
- `witness_set_size = 6`
- `hodn_quorum(6) = 4`
- Invalid BLS signature (forged)

Expected:

- mesh_aggregated_signature verification FAILS first.
- Quorum check (which would have failed too: 3 < 4) does not run.
- Substrate rejects envelope with `BLSVerificationFailed`, not `InsufficientQuorum`.

## Alternatives Considered

### Implicit cascade via parent UNBIND only

Rejected. Parent UNBIND initiates cascade but cannot finalize children's state machine without an SGTP acceptance per child. Without SGTP, `Dissolving` state persists indefinitely; transport handles linger; resource leak.

### Per-child quorum with parent signature

Rejected. Single-child quorum provides no fault tolerance. 2/3 super-majority (hodn_quorum) balances safety against child availability loss.

### Reuse CGROUP BIND for SGTP

Rejected. CGROUP BIND is a transport event; SGTP is a governance event. Reusing BIND would couple transport lifecycle to governance lifecycle and prevent independent recovery paths.

### Allow sub-DC to bypass delegation for own teardown

Rejected. Sub-DC acting without delegation could dissolve a sub-group that parent DC intends to preserve for policy reasons. Sub-DC MUST have valid delegation (RFC-0855p-d2) for SGTP.

## Implementation Phases

### Phase 1 — Envelope and wire

- Add `ParentToSubRouteEnvelope` + `SubToParentAggregateEnvelope` + `TeardownProofEnvelope` + canonical proof structs.
- Add DCS derives + dispatch + signature coverage.
- Add `TEARDOWN_GRACE_EPOCHS = 50` + `MAX_AGGREGATE_ATTESTATIONS = 1024` constants.
- Add compatibility tests proving CGROUP ignores P2SR/S2PA/SGTP.

### Phase 2 — Routing + aggregation + teardown

- Add `MemberAttestation` + `SignersBitmap` + distinct-signer enforcement.
- Add mesh_aggregated_signature verification (BLS12-381 G1 48-byte compressed).
- Add hodn_quorum policy lookup.
- Add grace-elapsed check for SGTP.
- Add state machine transitions `Dissolving → Dissolved`.

### Phase 3 — Client API (cross-RFC surface)

- Add `octo-mesh` subcommands for route / aggregate / decommission.
- Show routing table, aggregate coverage, teardown grace remaining.
- Require explicit confirmation for decommission.
- Publish dashboards for routing latency, aggregate coverage, and teardown grace.

## Key Files to Modify

- `crates/octo-network/src/dot/subgroup_routing.rs` (new per restructure, formerly part of monolithic `sub_group.rs`) — `ParentToSubRouteEnvelope` + `SubToParentAggregateEnvelope` + `MemberAttestation` + `SignersBitmap` + `aggregate_id` derivation + `hodn_quorum` policy + mesh_aggregated_signature verification + distinct-signer enforcement + bitmap-vs-quorum coverage check ordering.
- `crates/octo-network/src/dot/subgroup_teardown.rs` (new per restructure, formerly part of monolithic `sub_group.rs`) — `TeardownProofEnvelope` + grace-elapsed check + state transition `Dissolving → Dissolved`.

## Economic Analysis

DEFER to RFC-0917 and RFC-0960. This RFC defines cross-sub-group routing + aggregation + teardown only. No direct token transfer, fee, reward, stake, settlement, or accounting surface here.

## Future Work

- **F-2, cross-sub-group routing:** Resolved inline through P2SR envelope + bounded child sub-domain broadcast.
- **F-3, aggregate witness collection:** Resolved inline through S2PA envelope + distinct-signer enforcement + hodn_quorum coverage.
- **F-4, sub-group teardown:** Resolved inline through SGTP envelope + grace-elapsed bound.
- **F-5, parent-cascade teardown:** Resolved inline through parent UNBIND + descendant cascade per RFC-0855p-d1 §State Machine.
- **F-9, witness-set size query:** Resolved inline through SubGroupQuery (RFC-0855p-d1 §Layer-C Substrate Surface) returning `witness_set_size`.

## Rationale

Separate envelope family (`P2SR` / `S2PA` / `SGTP`) preserves CGROUP ABI. Adding optional fields to CGROUP or CGSB would weaken old-client compatibility and make cross-sub-group semantics non-obvious. Explicit subtype dispatch lets old consumers filter safely and new recipients apply the right verification path.

`TEARDOWN_GRACE_EPOCHS = 50` balances grace period against resource leak: short enough to bound teardown latency, long enough to allow cross-sub-group propagation. `MAX_AGGREGATE_ATTESTATIONS = 1024` balances witness-collection fault tolerance against DoS amplification.

Distinct-signer enforcement (no repeated bitmap indices) prevents quorum-bypass via duplicate indices. Without this check, an attacker could craft a bitmap with `index = 0` repeated N times to fake quorum coverage.

Bitmap-vs-quorum coverage check ordering (BLS first, then count) ensures cryptographic verification precedes policy evaluation. Without ordering, an attacker could submit invalid BLS signature with valid bitmap count; policy check passes; envelope accepted; subsequent valid signature verification is skipped.

mesh_aggregated_signature covering signers_bitmap prevents quorum forgery: without coverage, an attacker could submit any bitmap with valid aggregate signature over per-signer signatures only; the bitmap becomes a free parameter.

## Version History

| Version | Date       | Changes                                                                                  |
| ------- | ---------- | ---------------------------------------------------------------------------------------- |
| 1.3     | 2026-09-02 | Split from v1.2. See fix-log §v1.3. d3 owns P2SR/S2PA/SGTP + bitmap + quorum + teardown. |

## Related RFCs

- RFC-0850 — Deterministic Overlay Transport
- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-Initiated Transport Group Creation & Invite
- RFC-0853 — Overlay Cryptography (OCrypt); §Cryptographic Primitives mandates BLAKE3-256; RFC-0855p-b §Witness Set Aggregation mandates BLS12-381 G1 48-byte compressed
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0009 — Identity substrate
- RFC-0855p-b — Mission Coordinator Lifecycle (witness set aggregation)
- RFC-0855p-c — DomainCoordinator Role and parent DC authority scope
- RFC-0855p-d — Monolithic predecessor (now slim INDEX; superseded by d1/d2/d3)
- RFC-0855p-d1 — Sub-Group Creation & State (prerequisite: subgroup must exist + state machine)
- RFC-0855p-d2 — Sub-DC Delegation Lifecycle (prerequisite: non-parent sub-DC requires valid delegation)
- RFC-0855p-e — Mission Coordinator Handover Envelope (sibling RFC)

## Appendices

(Added v1.3 per W11 L5 H4 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

### A. Teardown grace arithmetic

Recipient-local-clock comparison per W11 L1 C2 finding:

```
teardown_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS  // envelope-supplied
```

replaced at acceptance time by:

```
local_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS     // recipient-local
```

Envelope `teardown_epoch` field becomes informational/audit-only.

### B. Bitmap-vs-quorum ordering

Per W10.5 L3 C2 finding; re-confirmed W11 L3 H1:

```
1. mesh_aggregated_signature verifies   // BLS first
2. signers_bitmap.count_ones() == attestations.len()  // distinct-signer
3. signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)  // quorum coverage
```

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — Cross-sub-group messaging
