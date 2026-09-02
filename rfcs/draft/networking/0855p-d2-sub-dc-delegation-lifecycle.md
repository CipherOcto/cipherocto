# RFC-0855p-d2 (Networking): Sub-DC Delegation Lifecycle

## Status

Draft (2026-09-02) — spec elaboration closed (v1.3); part of 0855p-d restructure chain (d1/d2/d3). Supersedes monolithic RFC-0855p-d v1.2 §Sub-DC Delegation Protocol + relevant §Data Structure (SubDCDelegationProof + SubDCDelegationPolicy + SDCD/SDRV/SDRT envelopes) + relevant §Security Considerations + relevant §Test Vectors.

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Part 2 of 3 (0855p-d1 / d2 / d3). Defines sub-DC delegation envelope family (`DOT/1/CGROUP_SUB` subtypes `b"SDCD"` / `b"SDRV"` / `b"SDRT"`). `SDCD` = parent-DC-signed delegation proof that authorizes a non-parent sub-DC for a specific child sub-domain. `SDRV` = revocation proof terminating the delegation. `SDRT` = rotation proof replacing the sub-DC key without losing delegation continuity. Authority bounded by `MAX_ROOT_DELEGATION = 1` (at most one delegation under any parent) + `MAX_DELEGATION_CHAIN_PER_TERM = 256` (delegations per coordinator term). Depends on RFC-0855p-d1 (subgroup must exist + parent must be bound before delegation has any effect).

## Dependencies

- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-initiated group creation
- RFC-0855p-b — Mission Coordinator Lifecycle and slash policy
- RFC-0855p-c — DomainCoordinator authority and lifecycle
- RFC-0855p-d1 — Sub-Group Creation & State (parent subgroup must exist and be `Bound`)
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0853 — Overlay Cryptography (OCrypt)
- RFC-0009 — Identity substrate
- RFC-0855p-d3 — Routing + Aggregation + Teardown (downstream consumer of revocation events for cascade teardown)

## Layer placement

| Concern                                                                                   | Layer       | Justification                                                                                                                                       |
| ----------------------------------------------------------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SDCD` / `SDRV` / `SDRT` outer wire (10-byte canonical header per RFC-0850p-c §A)         | **Layer B** | Transport envelope; no Layer-C role knowledge embedded in outer wire                                                                                |
| `SubDCDelegationEnvelope` / `SubDCRevocationEnvelope` / `SubDCRotationEnvelope` inner DCS | **Layer B** | Wire-format semantics; canonical form per RFC-0126                                                                                                  |
| `SubDCDelegationProof` structure + canonical fields                                       | **Layer B** | Type-boundary invariant for delegation proof; auditable at decode time                                                                              |
| `MAX_ROOT_DELEGATION = 1` const + `MAX_DELEGATION_CHAIN_PER_TERM = 256` const             | **Layer C** | Coordinator-side governance policy (depth limits)                                                                                                   |
| `SubDCDelegationPolicy` validation                                                        | **Layer C** | Coordinator-side authority policy (delegation chain depth, term, expiry)                                                                            |
| `SubGroupAction::delegate` / `SubGroupAction::revoke` typed-discriminator entries         | **Layer C** | Layer-C action dispatch (RFC-0855p-d1 §Layer-C Substrate Surface owns the action namespace; this RFC owns the `0x0002` / `0x0003` registry entries) |
| Re-export (`pub use rfc_0853::Ed25519PublicKey`)                                          | **Layer A** | Crypto primitive; re-export only                                                                                                                    |
| Re-export (`pub use rfc_0009::Did`)                                                       | **Layer B** | Identity substrate; re-export only                                                                                                                  |

Direction A→B→C/D/E verified: this RFC depends on RFC-0853 (Layer A crypto), RFC-0009 (Layer B identity), RFC-0850p-c (Layer B transport), RFC-0126 (Layer A canonical encoding), RFC-0855p-c (Layer C DC authority), RFC-0855p-d1 (Layer C subgroup creation/state), RFC-0855p-d3 (Layer C routing/teardown). No upward dependency.

No Layer C module may parse raw Layer B envelopes. No Layer A codec changes for delegation fields. Unknown envelope subtypes fail closed.

## Design Goals

1. Constrain sub-DC authority to a single child sub-domain (no domain-overlapping delegation).
2. Bound delegation chain depth per coordinator term (`MAX_DELEGATION_CHAIN_PER_TERM = 256`); chain must reset on coordinator term rotation.
3. Bound root delegation breadth (`MAX_ROOT_DELEGATION = 1`); parent DC can delegate at most one child sub-DC per parent.
4. Make delegation, rotation, and revocation distinguishable on the wire so recipients apply the right verification path.
5. Refuse delegation replay by signing the exact (parent_domain_id, sub_domain_id, sub_dc_id, term_id, epoch, nonce) tuple.
6. Refuse cascading revocation under expired proofs; only verified SDCD + matching SDRV may transition a delegation out.
7. Parent signature mandatory on every delegation envelope; sub-DC key never delegates upward.

## Motivation

Single-parent authority (RFC-0855p-d1) leaves a gap: child domains may need an operational sub-DC distinct from the parent DC (geographic split, on-call rotation, scoped permissions). Without a delegation envelope family, the only path is parent DC signing every child operation — operationally untenable for high-throughput child domains. With unrestricted delegation, parent authority silently leaks and child terms outlive parent terms. This RFC closes the gap with three explicit envelopes + bounded chain depth + mandatory revocation envelope to terminate delegations cleanly.

## Roles and Authorities

| Role        | Delegate sub-DC                                                    | Rotate sub-DC key                                           | Revoke sub-DC                                          |
| ----------- | ------------------------------------------------------------------ | ----------------------------------------------------------- | ------------------------------------------------------ |
| Origin      | None                                                               | None                                                        | None                                                   |
| Parent DC   | Sign SDCD for child; chain depth ≤ 256                             | Sign SDRT replacing sub-DC key                              | Sign SDRV terminating delegation                       |
| Sub-DC      | Sign as delegator for descendant child IF delegation scope permits | Sign SDRT for own successor only when explicitly authorized | None (only parent can revoke)                          |
| Member      | None                                                               | None                                                        | None                                                   |
| Parent mesh | Validate envelopes + chain depth                                   | Validate envelopes + chain depth                            | Validate envelopes + revoke child descendant authority |

Sub-DC cannot delegate upward, expand its own scope, delegate across distinct child sub-domains, or survive parent revocation.

## Adversary Analysis

### Five-Question Test

1. **Who?** Parent DC, sub-DC, expired sub-DC, revoked sub-DC, malicious member, external sender, compromised mesh node.
2. **Capability?** Craft delegation proof, replay old proof, forge parent signature, extend chain beyond limit, delegate across non-allowed scopes.
3. **Target?** Parent DC authority, child domain integrity, delegation chain bounds, mission policy.
4. **Controls?** Canonical encoding, parent signature verification, scope match against (parent_domain_id, sub_domain_id), chain depth counter, term-window check, replay-key index.
5. **Residual risk?** Parent key compromise, transport-platform outage, replay cache loss.

### Threat Matrix

| Threat                              | Required control                                                                                      | Failure result                                 |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| Sub-DC impersonation                | Parent signature on every SDCD; signature covers exact scope + term + epoch + nonce                   | Reject                                         |
| Delegation replay                   | Nonce in replay tuple `(SDCD, parent_dc_id, sub_domain_id, sub_dc_id, term_id, current_epoch, nonce)` | Reject                                         |
| Privilege escalation (chain growth) | `MAX_DELEGATION_CHAIN_PER_TERM = 256`; chain depth query before accept                                | Reject `ChainLimitExceeded`                    |
| Privilege escalation (cross-domain) | `MAX_ROOT_DELEGATION = 1`; root delegation table per parent                                           | Reject `MultipleRootDelegations`               |
| Parent takeover                     | Sub-DC cannot sign SDCD for parent's domain; only parent DC signs SDCD                                | Substrate rejects cross-parent SDCD            |
| Rotation masquerade                 | SDRT carries parent signature AND old sub-DC signature; old sub-DC key explicitly retired             | Reject                                         |
| Revocation-bypass race              | Revocation MUST propagate before sub-DC re-delegates; SDRV published via parent fanout                | Recipient rejects SDRT post-SDRV if no overlap |
| Term expiration bypass              | SDRV chain age tracked; chain window = current term only                                              | Reject                                         |

## Implicit Assumptions Audit

| Assumption               | Audit precondition                                                        | Enforcement                      | Failure handling                 |
| ------------------------ | ------------------------------------------------------------------------- | -------------------------------- | -------------------------------- |
| Subgroup exists          | `(parent_domain_id, sub_domain_id)` is `Bound` (RFC-0855p-d1)             | SubGroupQuery before SDCD accept | Reject `SubGroupNotBound`        |
| Subgroup not dissolving  | State ∈ `{Bound}`; no `Dissolving` pending                                | State query                      | Reject `SubGroupDissolving`      |
| Sub-DC DID well-formed   | `sub_dc_id` decodes to canonical `Did` (RFC-0009)                         | Re-export + decode               | Reject `MalformedSubDcId`        |
| Parent DC term current   | SDCD signature timestamp within current term window                       | Term-window check                | Reject `TermExpired`             |
| Chain depth known        | `chain_depth ≤ MAX_DELEGATION_CHAIN_PER_TERM`                             | Counter per term                 | Reject `ChainLimitExceeded`      |
| Root delegation breadth  | At most 1 active sub-DC per parent DC                                     | `MAX_ROOT_DELEGATION = 1` table  | Reject `MultipleRootDelegations` |
| Replay-key index current | Mesh keeps replay entries per (parent, child, sub-DC, term, epoch, nonce) | Nonce index query                | Reject `ReplayDetected`          |

Assumption "existing mesh" is not sufficient. Node MUST query canonical subgroup state (RFC-0855p-d1 §Layer-C Substrate Surface), chain-depth counter, root-delegation table, and nonce index. Missing data fails closed.

## Security Considerations

### Privilege escalation

`MAX_DELEGATION_CHAIN_PER_TERM = 256` bounds how many delegations can occur in one coordinator term. Chain depth counter increments on each accepted SDCD; term reset decrements to 0. Cross-term accumulation cannot occur. `MAX_ROOT_DELEGATION = 1` prevents a single parent from delegating two distinct child sub-DCs for sibling sub-groups simultaneously — which would imply a fork in operational authority without explicit governance overhead. Revocation envelope (`SDRV`) propagates before any new delegation can issue for the same `(parent_domain_id, sub_domain_id)` — order-sensitive validation prevents the sub-DC from re-delegating after revocation but before observation.

Rotation envelope (`SDRT`) carries BOTH parent signature AND retiring sub-DC signature. The retiring sub-DC's signature confirms the key rotation is voluntary; if the retiring sub-DC is unreachable (e.g., lost key), the parent must use `SDRV` + a fresh `SDCD` instead. Substrate MUST refuse an SDRT that lacks either signature.

### Sub-DC scope expansion

Sub-DC cannot expand scope across distinct child sub-domains. Every SDCD carries an exact `sub_domain_id`; validation rejects SDCDs whose claimed `sub_domain_id` does not match the canonical child. Cross-domain operations (parent-to-child routes, child-to-parent aggregates) per RFC-0855p-d3 verify delegation is current at envelope acceptance time.

### Term boundary replay

A delegation issued under coordinator term `T1` MUST NOT be valid in term `T2` even if the underlying parent DC is unchanged. The chain counter resets on term transition (per `MAX_DELEGATION_CHAIN_PER_TERM`); a stale SDCD presents chain_depth that no longer matches the per-term table. Term expiration is computed at envelope acceptance, not at envelope signature time — late-arriving SDCDs are rejected.

## Specification

### Envelope Type

| Envelope Type      | Subtype tag | Direction                  | Description                                 |
| ------------------ | ----------- | -------------------------- | ------------------------------------------- |
| `DOT/1/CGROUP_SUB` | `b"SDCD"`   | Parent DC → mesh broadcast | Issue or refresh sub-DC delegation proof    |
| `DOT/1/CGROUP_SUB` | `b"SDRV"`   | Parent DC → mesh broadcast | Revoke sub-DC delegation                    |
| `DOT/1/CGROUP_SUB` | `b"SDRT"`   | Parent DC + sub-DC → mesh  | Rotate sub-DC key, retiring old key cleanly |

Subtype dispatch uses explicit typed parsing. Unsupported subtype remains unknown and never falls back to CGSB/CGROUP. Sibling envelopes owned by RFC-0855p-d1 (CGSB) and RFC-0855p-d3 (P2SR/S2PA/SGTP).

### Data Structure

`SubDCDelegationProof` is the canonical signed authorization, embedded as `delegation_proof` in `CreateSubGroupEnvelope` (RFC-0855p-d1) and re-validated on every subsequent sub-DC operation.

```rust
pub const MAX_DELEGATION_CHAIN_PER_TERM: u16 = 256;
pub const MAX_ROOT_DELEGATION: u8 = 1;
pub const SUBGROUP_DELEGATION_CONTEXT: &str = "DOT/1/CGROUP_SUB/delegation";
pub const SUBGROUP_REVOCATION_CONTEXT: &str = "DOT/1/CGROUP_SUB/revocation";
pub const SUBGROUP_ROTATION_CONTEXT: &str = "DOT/1/CGROUP_SUB/rotation";
pub const SUBGROUP_DELEGATION: [u8; 4] = *b"SDCD";
pub const SUBGROUP_REVOCATION: [u8; 4] = *b"SDRV";
pub const SUBGROUP_ROTATION: [u8; 4] = *b"SDRT";

// Re-export per RFC-0855p-d1 Layer placement table — these types are NOT
// redefined here per Stable Abstractions Principle + A→B→C dep direction
// rule. RFC-0853 owns the crypto primitive; RFC-0009 owns the canonical Did.
pub use rfc_0853::Ed25519PublicKey;
pub use rfc_0009::Did;
pub use crate::rfc_0855p_d1::{SubGroupLabel, DelegationId, MAX_ROOT_DEPTH};

#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CoordinatorTermId(pub [u8; 32]);

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubDCDelegationProof {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub sub_dc_id: [u8; 32],
    pub parent_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub epoch: u64,
    pub nonce: [u8; 16],
    pub valid_until_epoch: u64,
    pub chain_depth: u16,
    pub parent_signature: [u8; 64],
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubDCRevocationProof {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub sub_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub epoch: u64,
    pub revocation_reason: RevocationReasonCode,
    pub parent_signature: [u8; 64],
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubDCRotationProof {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub retiring_sub_dc_id: [u8; 32],
    pub successor_sub_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub epoch: u64,
    pub parent_signature: [u8; 64],
    pub retiring_sub_dc_signature: [u8; 64],
}

#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RevocationReasonCode {
    TermExpired,
    CoordinatorRotation,
    SubDCMisconduct,
    SubDCKeyCompromise,
    SubDCVoluntaryResignation,
    GroupDecommission,
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubDCDelegationEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub delegation_proof: SubDCDelegationProof,
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubDCRevocationEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub revocation_proof: SubDCRevocationProof,
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubDCRotationEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub rotation_proof: SubDCRotationProof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubDCDelegationPolicy {
    pub parent_domain_id: [u8; 32],
    pub current_term: CoordinatorTermId,
    pub current_chain_depth: u16,
    pub root_delegation_count: u8,
}

impl SubDCDelegationPolicy {
    pub fn check(
        &self,
        proof: &SubDCDelegationProof,
        subgroup_state_bound: bool,
    ) -> Result<(), DelegationPolicyError> {
        if proof.parent_domain_id != self.parent_domain_id {
            return Err(DelegationPolicyError::ParentMismatch);
        }
        if proof.term_id != self.current_term {
            return Err(DelegationPolicyError::TermExpired);
        }
        if !subgroup_state_bound {
            return Err(DelegationPolicyError::SubGroupNotBound);
        }
        if proof.chain_depth > MAX_DELEGATION_CHAIN_PER_TERM {
            return Err(DelegationPolicyError::ChainLimitExceeded);
        }
        // `MAX_ROOT_DELEGATION = 1`: reject new delegations when one already
        // exists for this parent. Existing delegations are revoked via SDRV
        // (RFC-0855p-d2 §RevocationProtocol) before a new root delegation
        // can issue for the same parent.
        if self.root_delegation_count >= MAX_ROOT_DELEGATION {
            return Err(DelegationPolicyError::MultipleRootDelegations);
        }
        Ok(())
    }

    pub fn reset_for_term(&mut self, new_term: CoordinatorTermId) {
        // Chain depth MUST reset on coordinator term rotation; cross-term
        // accumulation would let a parent term delegate unbounded children
        // without explicit governance overhead.
        self.current_term = new_term;
        self.current_chain_depth = 0;
    }
}

impl SubDCDelegationProof {
    pub fn validate_for(
        &self,
        proposed_sub_dc_id: [u8; 32],
        derived_sub_domain_id: [u8; 32],
    ) -> Result<(), DelegationProofError> {
        if self.sub_dc_id != proposed_sub_dc_id {
            return Err(DelegationProofError::SubDcMismatch);
        }
        if self.sub_domain_id != derived_sub_domain_id {
            return Err(DelegationProofError::SubDomainMismatch);
        }
        if self.chain_depth == 0 || self.chain_depth > MAX_DELEGATION_CHAIN_PER_TERM {
            return Err(DelegationProofError::InvalidChainDepth);
        }
        if self.valid_until_epoch < self.epoch {
            return Err(DelegationProofError::InvalidExpiry);
        }
        Ok(())
    }
}
```

### Sub-DC Delegation Protocol

Parent DC issues `SubDCDelegationEnvelope` to authorize a child sub-DC distinct from itself for a specific child sub-domain. Chain depth counter increments per accepted SDCD; chain depth reset on term rotation.

**Issuance**

1. Parent DC computes `chain_depth = current_chain_depth + 1` for the current term.
2. Parent DC signs the canonical DCS encoding of `SubDCDelegationProof { parent_domain_id, sub_domain_id, sub_dc_id, parent_dc_id, term_id, epoch, nonce, valid_until_epoch, chain_depth, parent_signature: placeholder }` over the canonical `SUBGROUP_DELEGATION_CONTEXT` derivation-key domain.
3. Envelope dispatched via mesh broadcast to current parent members / validators.
4. Subgroup MUST already be `Bound` per RFC-0855p-d1 §Recipient Verification before SDCD is accepted.

**Acceptance (recipient)**

1. DCS decode succeeds; `envelope_subtype == b"SDCD"`; `version` is supported.
2. Forward-skew bound (clock-drift tolerance): reject envelope where `epoch > local_epoch + MAX_FSKEW_EPOCHS` (RFC-0855p-d1 §Layer placement table; cross-RFC invariant).
3. `SubGroupLabel::new` accepts canonicalized bytes (already validated at CGSB time).
4. Subgroup is `Bound` (SubGroupQuery per RFC-0855p-d1).
5. `SubDCDelegationPolicy::check(proof, subgroup_state_bound=true)` passes — term, chain depth, root delegation breadth.
6. `SubDCDelegationProof::validate_for(sub_dc_id, derived_sub_domain_id)` passes.
7. CGROUP signature verifies under current parent DC public key (RFC-0850p-c §Signature Coverage).
8. Nonce unconsumed under replay key `(SDCD, parent_dc_id, sub_domain_id, sub_dc_id, term_id, epoch, nonce)`.
9. Record root delegation entry in `root_delegation_count` table; increment `current_chain_depth`.

### Revocation Protocol

Parent DC issues `SubDCRevocationEnvelope` to terminate a sub-DC delegation. Revocation MUST propagate before any new delegation can issue for the same `(parent_domain_id, sub_domain_id)`.

**Issuance**

1. Parent DC signs canonical DCS encoding of `SubDCRevocationProof { parent_domain_id, sub_domain_id, sub_dc_id, term_id, epoch, revocation_reason, parent_signature: placeholder }` over `SUBGROUP_REVOCATION_CONTEXT`.
2. Envelope dispatched via mesh broadcast.

**Acceptance (recipient)**

1. DCS decode succeeds; `envelope_subtype == b"SDRV"`; `version` is supported.
2. Forward-skew bound (clock-drift tolerance): reject envelope where `epoch > local_epoch + MAX_FSKEW_EPOCHS`.
3. Subgroup exists and is in `{Bound, Dissolving}` (RFC-0855p-d1 state machine).
4. Active delegation record matches `(parent_domain_id, sub_domain_id, sub_dc_id, term_id)` — only the currently delegated sub-DC may be revoked.
5. Parent signature verifies under current parent DC public key.
6. Nonce unconsumed under replay key `(SDRV, parent_dc_id, sub_domain_id, sub_dc_id, term_id, epoch, nonce)`.
7. Mark delegation record as revoked; decrement `root_delegation_count`; preserve chain-depth audit trail for the term.

### Rotation Protocol

Parent DC + retiring sub-DC jointly issue `SubDCRotationEnvelope` to replace the sub-DC key without losing delegation continuity.

**Issuance**

1. Retiring sub-DC and parent DC coordinate the successor key out-of-band.
2. Both sign canonical DCS encoding of `SubDCRotationProof { parent_domain_id, sub_domain_id, retiring_sub_dc_id, successor_sub_dc_id, term_id, epoch, parent_signature: placeholder, retiring_sub_dc_signature: placeholder }` over `SUBGROUP_ROTATION_CONTEXT`.
3. Both signatures MUST be present; missing either is invalid.
4. Envelope dispatched via mesh broadcast.

**Acceptance (recipient)**

1. DCS decode succeeds; `envelope_subtype == b"SDRT"`; `version` is supported.
2. Forward-skew bound: reject envelope where `epoch > local_epoch + MAX_FSKEW_EPOCHS`.
3. Active delegation record matches `(parent_domain_id, sub_domain_id, retiring_sub_dc_id, term_id)`.
4. BOTH parent signature and retiring sub-DC signature verify.
5. `successor_sub_dc_id != retiring_sub_dc_id` (rotation must replace, not re-affirm).
6. Nonce unconsumed under replay key `(SDRT, parent_dc_id, sub_domain_id, retiring_sub_dc_id, term_id, epoch, nonce)`.
7. Replace delegation record key from `retiring_sub_dc_id` to `successor_sub_dc_id`; preserve chain-depth audit trail.

If the retiring sub-DC is unreachable (lost key), parent MUST use `SDRV` + a fresh `SDCD` instead of `SDRT`.

### Recipient Verification (SDCD / SDRV / SDRT)

Common path for SDCD/SDRV/SDRT:

1. DCS decode succeeds; `version` is supported.
2. Forward-skew bound: reject envelope where `epoch > local_epoch + MAX_FSKEW_EPOCHS`.
3. Subgroup is `Bound` (or `Dissolving` for SDRV) per RFC-0855p-d1 §Recipient Verification.
4. Parent DC public key matches current parent state (RFC-0855p-c).
5. Term-window check (current term at acceptance time, not at signature time).
6. Policy check (`MAX_DELEGATION_CHAIN_PER_TERM`, `MAX_ROOT_DELEGATION`).
7. Replay-key index query.
8. (Post-validation) Record replay-key entry only after steps 1–7 pass; partial-validation envelopes never poison the replay cache.

SDCD-only: SubDCDelegationPolicy::check + SubDCDelegationProof::validate_for + root_delegation_count update.
SDRV-only: delegation record exists AND matches `(parent, child, sub_dc, term)`; revoke + decrement.
SDRT-only: BOTH signatures verify; key replacement preserves chain-depth audit trail.

### Chain-Depth Counter Reset

On coordinator term rotation, the chain-depth counter MUST reset to 0. Counter lives in the per-parent `SubDCDelegationPolicy` struct; `reset_for_term(new_term)` enforces this. Cross-term accumulation is forbidden.

## RFC-0008 Execution Class Mapping

| Operation                                                             | Class | Justification                                                                                                           |
| --------------------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------------------- |
| SDCD (SubDCDelegationEnvelope) envelope accept + chain counter update | C     | Cross-node consensus-aggregated state mutation (chain depth + root delegation table); non-deterministic under partition |
| SDRV (SubDCRevocationEnvelope) envelope accept                        | C     | Cross-node state mutation                                                                                               |
| SDRT (SubDCRotationEnvelope) envelope accept                          | C     | Cross-node key replacement                                                                                              |
| `SubDCDelegationProof::validate_for` (pure-function scope match)      | A     | BLAKE3 derivation + signature verification + scope equality; deterministic; no external state                           |
| `RevocationReasonCode` enum construction                              | A     | Pure-function typed discriminator                                                                                       |

## Determinism Requirements

- **Canonical encoding**: `SubDCDelegationProof` field order is fixed; signature covers the canonical DCS encoding per RFC-0126. Recipients recompute signature over the same canonical form.
- **Replay-key tuple encoding**: SDCD/SDRV/SDRT `epoch` field in replay tuple uses canonical BE bytes (RFC-0126 array-of-u8 form).
- **Chain-depth counter reset on term rotation**: deterministic; no randomness.
- **Cross-replica determinism**: given identical observed SDCD/SDRV/SDRT sequences, every replica reaches the same chain-depth counter, root delegation count, and delegation table state.

## Lifecycle Requirements

| Constant                        | Value | Purpose                                                                                |
| ------------------------------- | ----- | -------------------------------------------------------------------------------------- |
| `MAX_DELEGATION_CHAIN_PER_TERM` | 256   | Maximum SDCD accepts per coordinator term; chain-depth counter resets on term rotation |
| `MAX_ROOT_DELEGATION`           | 1     | At most one active sub-DC delegation per parent DC                                     |

State machine: SDCD accept → delegation record exists; SDRV accept → delegation record revoked; SDRT accept → delegation record key replaced. Delegation lifetime bounded by `valid_until_epoch` in the proof. Coordinator term rotation triggers chain-depth reset but does NOT auto-revoke existing delegations.

## Performance Targets

- Delegation validation target: under 1 ms p95 excluding durable state writes.
- Chain-depth counter update: one transaction per parent.
- Replay-key index: constant-time lookup by tuple.

Targets are local p95 on agreed reference hardware.

## Compatibility

RFC-0850p-d CGROUP consumers filter on supported subtype. They see unknown SDCD/SDRV/SDRT safely, log unsupported subtype, and do not decode or act. No CGROUP field changes. Old clients cannot create delegations because SDCD/SDRV/SDRT-specific parsing and CLI routes are unavailable. Wire addition is backward compatible. CGSB (RFC-0855p-d1) and P2SR/S2PA/SGTP (RFC-0855p-d3) subtypes also fail closed. Version negotiation uses outer `version`; unsupported version is rejected without fallback.

## Test Vectors

### TV-SG-6 — Valid SDCD accept

Input:

- parent depth: 1
- parent state: `Active`
- subgroup state: `Bound`
- `sub_dc_id`: distinct from parent DC
- `chain_depth`: 1
- `term_id`: current
- `nonce`: fresh

Expected:

- `SubDCDelegationPolicy::check` passes.
- `SubDCDelegationProof::validate_for` passes.
- Replay-key index records entry.
- `root_delegation_count` increments to 1.
- `current_chain_depth` increments to 1.

### TV-SG-7 — SDRV race with re-delegation

Input:

- Existing delegation under `sub_dc_id=A` for `(parent, child)`.
- SDRV arrives for `A`.
- New SDCD for `sub_dc_id=B` arrives at the same epoch before SDRV commits.

Expected:

- SDRV commits first; `sub_dc_id=A` delegation marked revoked.
- New SDCD for `sub_dc_id=B` rejected (`MultipleRootDelegations`).
- Re-delegation requires explicit `root_delegation_count` decrement.

## Alternatives Considered

### Allow multiple root delegations per parent

Rejected. Multiple concurrent sub-DCs for sibling sub-groups would create fork in operational authority without explicit governance overhead. `MAX_ROOT_DELEGATION = 1` forces explicit revoke-then-issue sequence.

### Implicit delegation via sub-DC signature only

Rejected. Sub-DC cannot self-delegate; parent authority must be explicit. Sub-DC signature alone is not sufficient; the proof must be parent-signed.

### Cross-term accumulation of chain depth

Rejected. Unbounded accumulation would let a parent term delegate unbounded children without explicit governance overhead. Term rotation resets counter.

### Sub-DC rotation without retiring signature

Rejected. The retiring sub-DC's signature confirms the rotation is voluntary; if the retiring sub-DC is unreachable, parent uses `SDRV` + fresh `SDCD` instead.

## Implementation Phases

### Phase 1 — Envelope and wire

- Add `SubDCDelegationEnvelope`, `SubDCRevocationEnvelope`, `SubDCRotationEnvelope` + canonical proof structs.
- Add DCS derives + dispatch + signature coverage.
- Add `MAX_DELEGATION_CHAIN_PER_TERM` + `MAX_ROOT_DELEGATION` constants.
- Add compatibility tests proving CGROUP ignores SDCD/SDRV/SDRT.

### Phase 2 — Policy and validator

- Add `SubDCDelegationPolicy` + chain-depth counter + root-delegation table.
- Add term-window check + chain-depth reset on term rotation.
- Add replay-key index entries for SDCD/SDRV/SDRT.
- Add descendant-cascade hooks: SDRV triggers RFC-0855p-d3 teardown propagation for affected sub-group.

### Phase 3 — Client API (cross-RFC surface)

- Add `octo-mesh` subcommands for sub-DC delegate / rotate / revoke.
- Show delegation chain depth, root delegation count, term window, and revocation reason.
- Require explicit confirmation for revocation and rotation.
- Publish dashboards for chain depth, root delegation breadth, and revocation latency.

## Key Files to Modify

- `crates/octo-network/src/dot/subgroup_delegation.rs` (new per restructure, formerly part of monolithic `sub_group.rs`) — `SubDCDelegationEnvelope` + `SubDCRevocationEnvelope` + `SubDCRotationEnvelope` + `SubDCDelegationPolicy` + chain-depth counter + root-delegation table + replay-key indexes + term-window enforcement; `MAX_DELEGATION_CHAIN_PER_TERM` + `MAX_ROOT_DELEGATION` enforcement.

## Economic Analysis

DEFER to RFC-0917 and RFC-0960. This RFC defines delegation authority only. No direct token transfer, fee, reward, stake, settlement, or accounting surface here. Slash tally propagation for `RevocationReasonCode::SubDCMisconduct` lives in RFC-0855p-b §Slash Tally.

## Future Work

- **F-1, sub-DC delegation:** Resolved inline through SDCD/SDRV/SDRT envelope family + chain-depth + root-delegation bounds.
- **F-8, rotation envelope:** Resolved inline through SDRT requiring BOTH parent and retiring signatures.

## Rationale

Separate envelope family (`SDCD` / `SDRV` / `SDRT`) preserves CGROUP ABI. Adding optional fields to CGROUP or CGSB would weaken old-client compatibility and make delegation semantics non-obvious. Explicit subtype dispatch lets old consumers filter safely and new recipients apply the right verification path.

`MAX_ROOT_DELEGATION = 1` enforces one active sub-DC per parent. Sibling sub-groups cannot have distinct operational authorities without explicit governance overhead (revoke-then-issue). This is intentional: multi-delegation is the rare case, single-delegation is the common case; forcing explicit revoke makes every delegation change auditable.

`MAX_DELEGATION_CHAIN_PER_TERM = 256` plus term-window reset bounds authority growth. Without this, a long-running coordinator term could delegate unbounded children. Term rotation forces explicit renewal under a new term's governance.

SDRT requiring BOTH signatures prevents unilateral rotation: a parent alone cannot rotate a sub-DC's key (collusion risk), and a sub-DC alone cannot rotate (forgery risk). Joint signing is the trustless path; `SDRV + fresh SDCD` is the unreachable-retiring-sub-DC path.

## Version History

| Version | Date       | Changes                                                                                                                                                                                     |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.3     | 2026-09-02 | Restructured from monolithic RFC-0855p-d v1.2 into 3-RFC chain (d1/d2/d3). See fix-log §v1.3. d2 owns SDCD/SDRV/SDRT + SubDCDelegationPolicy + chain-depth counter + root-delegation table. |

## Related RFCs

- RFC-0850 — Deterministic Overlay Transport
- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-Initiated Transport Group Creation & Invite
- RFC-0853 — Overlay Cryptography (OCrypt)
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0009 — Identity substrate
- RFC-0855p-b — Mission Coordinator Lifecycle (slash tally for `SubDCMisconduct` revocation)
- RFC-0855p-c — DomainCoordinator Role and parent DC authority scope
- RFC-0855p-d — Monolithic predecessor (now slim INDEX; superseded by d1/d2/d3)
- RFC-0855p-d1 — Sub-Group Creation & State (prerequisite: subgroup must exist and be `Bound`)
- RFC-0855p-d3 — Routing + Aggregation + Teardown (downstream consumer: revocation triggers cascade teardown)
- RFC-0855p-e — Mission Coordinator Handover Envelope (sibling RFC)

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — DC Delegation
