# RFC-0855p-d1 (Networking): Sub-Group Creation & State

## Status

Draft (2026-09-02) — spec elaboration closed (v1.3); part of 0855p-d restructure chain (d1/d2/d3). Supersedes monolithic RFC-0855p-d v1.2 §Specification Envelope Type Added CGSB + §Data Structure (CGSB types) + §State Machine + §Layer-C Substrate Surface (creation/query) + relevant §Security Considerations + relevant §Test Vectors.

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Part 1 of 3 (0855p-d1 / d2 / d3). Defines `DOT/1/CGROUP_SUB` (`b"CGSB"`) for authenticated sub-group creation + `SubGroupState` lifecycle (PendingBind / Bound / Dissolving / Dissolved) + `SubGroupLabel` typed constructor (UTS-39 confusable + bidi/zero-width/BOM reject) + `sub_domain_id` derivation (BLAKE3 keyed_hash) + parent-binding + depth-cap invariants. Ceremony reuses CGROUP from RFC-0850p-d.

## Dependencies

- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-initiated group creation, CGROUP envelope, transport invite ceremony
- RFC-0855p-b — Mission Coordinator Lifecycle and slash policy
- RFC-0855p-c — DomainCoordinator authority and lifecycle
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0853 — Overlay Cryptography (OCrypt); §Cryptographic Primitives mandates BLAKE3-256
- RFC-0009 — Identity substrate (`Did` re-export)
- RFC-0855p-d2 — Sub-DC Delegation Lifecycle (for `delegation_proof` validation)
- RFC-0855p-d3 — Routing + Aggregation + Teardown (for `Dissolving → Dissolved` teardown proof + MemberAttestation)

## Layer placement

| Concern                                                                                | Layer       | Justification                                                                                                              |
| -------------------------------------------------------------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------- |
| `SubGroupEnvelope` outer wire (10-byte canonical header per RFC-0850p-c §A)            | **Layer B** | Transport envelope; no Layer-C role knowledge embedded in outer wire                                                       |
| `SubGroupPayload` inner DCS (CGSB semantic body)                                       | **Layer B** | Wire-format semantics; canonical form per RFC-0126                                                                         |
| `SubGroupLabel::new` constructor (UTS-39 confusable + bidi/zero-width/BOM reject)      | **Layer B** | Type-boundary invariant; auditable at decode time                                                                          |
| `target_routing_endpoint_id: [u8; 32]` typed key in wire                               | **Layer B** | Layer-B/D split per §Layer-C Substrate Surface; no `Platform` enum embedding                                               |
| `SubGroupState` enum + transition engine                                               | **Layer C** | Coordinator-side governance policy                                                                                         |
| `SubGroupRecord` storage + cross-node reconciliation                                   | **Layer C** | Storage-backed substrate; typed query boundary per [[cipherocto-design-principles]] §Storage is not a protocol             |
| `SubGroupQuery` / `SubGroupResponse` / `SubGroupAuthorityCheck` typed boundary structs | **Layer C** | Protocol boundary per §Open/Closed principle                                                                               |
| `MAX_BIND_AWAIT_EPOCHS = 32` + `MAX_BIND_RETRY_COUNT = 3` + `RACE_EPOCHS = 32` consts  | **Layer B** | Wire-protocol replay bound + BIND ceremony deadlines; canonical home (re-exported by RFC-0855p-d3 per its §Data Structure) |
| `MAX_FSKEW_EPOCHS = 4` const                                                           | **Layer B** | Forward-skew clock-drift tolerance; cross-RFC invariant with RFC-0855p-e; canonical home (re-exported by RFC-0855p-d3)     |
| Re-export (`pub use rfc_0853::Ed25519PublicKey`)                                       | **Layer A** | Crypto primitive; re-export only (no `pub type` alias per W6 L2 L1 finding — actual code uses `pub use`)                   |
| Re-export (`pub use rfc_0009::Did`)                                                    | **Layer B** | Identity substrate; re-export only                                                                                         |

Direction A→B→C/D/E verified: this RFC depends on RFC-0853 (Layer A crypto), RFC-0009 (Layer B identity), RFC-0850p-c (Layer B transport), RFC-0126 (Layer A canonical encoding), RFC-0855p-c (Layer C DC authority), RFC-0855p-d2 (Layer C delegation), RFC-0855p-d3 (Layer C routing/teardown). No upward dependency.

No Layer C module may parse raw Layer B envelopes. No Layer A codec changes for sub-group fields. Unknown envelope subtypes fail closed.

## Design Goals

1. Enforce `MAX_SUBGROUP_DEPTH = 8` at create and every re-derive.
2. Enforce nonempty, normalized, slash-free `SubGroupLabel` with `BoundedBytes<256>`.
3. Derive `sub_domain_id` deterministically from parent domain and exact label bytes.
4. Require active parent `GroupBinding`; no orphan sub-group.
5. Bound sub-DC authority to one sub-domain; parent signature mandatory for any alternate sub-DC (delegation enforced via RFC-0855p-d2).
6. Cascade parent dissolution to every descendant; permit independent child dissolution (teardown proof per RFC-0855p-d3).
7. Keep CGROUP wire compatibility through a separate CGSB subtype.

## Motivation

Mission teams need nested working groups, committees, channels, and bounded administrative scopes beneath one domain. Flat domain IDs lose parentage, policy lineage, broadcast boundaries, and deterministic naming. They also provide no safe mechanism for delegating authority to one child domain. This RFC (d1) closes the creation + state surface; RFC-0855p-d2 closes delegation; RFC-0855p-d3 closes routing + aggregation + teardown.

## Roles and Authorities

Authority comes from active parent `GroupBinding`, parent DC coordinator term, mission policy, and optional child-scoped delegation proof (issued per RFC-0855p-d2). Role labels never grant authority by themselves.

| Role            | Create sub-group                                                                                | Inherit parent policy                                                             |
| --------------- | ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Origin          | Propose label, parent, and mission context; cannot create without coordinator authority         | Receives inherited policy; cannot redefine parent policy                          |
| Coordinator     | Create child under active parent; supply parent signature; same-child delegation needs no proof | Parent mission and policy authoritative; child may add only stricter local policy |
| Member          | No                                                                                              | May read and apply inherited policy; cannot widen authority                       |
| Sub-coordinator | Create descendants only when explicit descendant scope appears in delegation (RFC-0855p-d2)     | Inherits parent policy; local rules may only narrow inherited policy              |
| Parent mesh     | Validate envelopes, CGROUP BIND, and transport binding                                          | Store policy lineage with child record                                            |

Sub-DC cannot invite outside delegated `sub_domain_id`, alter `mission_id`, replace `parent_domain_id`, rotate its own authority upward, or dissolve parent.

## Adversary Analysis

### Five-Question Test

1. **Who?** Parent DC, member, sub-DC, revoked sub-DC, external sender, compromised mesh node, or parent DomainCoordinator.
2. **Capability?** Craft labels, replay valid envelopes, forge signatures, alter parent/domain fields, submit duplicate creates.
3. **Target?** Domain naming integrity, bounded depth, mission policy, parent authority.
4. **Controls?** Canonical encoding, BLAKE3 derivation, active-binding query, depth query, child-scoped delegation, signature checks, nonce replay cache, duplicate index, state machine.
5. **Residual risk?** Replay cache loss, compromised parent key, transport-platform outage. Key compromise enables parent-term actions until coordinator rotation or parent revocation.

### Threat Matrix

| Threat                            | Required control                                                               | Failure result                                           |
| --------------------------------- | ------------------------------------------------------------------------------ | -------------------------------------------------------- |
| Label collision under one parent  | Verify `sub_domain_id`; reject duplicate `(parent_domain_id, domain_id)`       | CGROUP BIND rejected                                     |
| Same label under different parent | Hash includes full parent `domain_id`                                          | Allowed independent namespaces; no cross-parent identity |
| Depth-bomb DoS                    | Reject parent depth `>= MAX_SUBGROUP_DEPTH`; cap aggregate descendants at 8    | Create rejected; no pending bind                         |
| Label URL injection               | `SubGroupLabel` rejects empty input, NUL/control bytes, and every `/` byte     | DCS decode rejected                                      |
| Parent-chain spoofing             | Resolve chain from active root; reject cycle, missing root, or depth overflow  | No logical bind                                          |
| Transport rebind confusion        | Keep logical parent fixed across platform changes; use new binding nonce/epoch | Reject different-parent rebind                           |

## Implicit Assumptions Audit

| Assumption                       | Audit precondition                                    | Enforcement                                                            | Failure handling                   |
| -------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------------------- | ---------------------------------- |
| Parent exists                    | Parent `domain_id` resolves to one group              | `GroupBinding::Active` required                                        | Reject create or bind              |
| Parent lineage valid             | Parent chain terminates at active root                | Resolve chain; no cycles, forks, depth overflow                        | Reject create or bind              |
| Depth ceiling known              | `MAX_SUBGROUP_DEPTH = 8`                              | Compare fresh parent depth before state mutation                       | Reject before `PendingBind`        |
| Label charset checked before DCS | Input bytes represent safe label                      | Construct through `SubGroupLabel::new`; no unchecked deserializer path | Reject before domain derivation    |
| Parent mission immutable         | Child `mission_id` equals parent mission              | Query parent; compare hash                                             | Reject and record policy violation |
| Coordinator term current         | Parent DC signature covers domain, term, epoch, nonce | Verify current term and term-transition window                         | Reject outside term                |
| Layer-A hash stable              | BLAKE3-256 primitive configured identically           | Fixed 32-byte digest; fixed NFC UTF-8 label bytes                      | Reject interoperability mismatch   |

Assumption "existing mesh" is not sufficient. Node must query canonical parent state, coordinator term, nonce index, and policy hash. Missing data fails closed.

## Security Considerations

### Label collision

Recipient recomputes digest from exact UTF-8 NFC bytes inside `SubGroupLabel`. No trusted sender-provided `domain_id`. Mesh rejects duplicate child under same parent before BIND. Equal labels under different parents remain valid because parent digest participates.

### Depth denial of service

Depth counts longest active chain from root; root depth is 1. A child under depth 8 is forbidden. Recipients perform pre-state query and reject before creating `PendingBind` or inviting members. Repeated attempts consume bounded request quota and cannot amplify nested state.

### URL-path injection

`SubGroupLabel` uses bounded bytes and rejects empty input, NUL, ASCII control bytes, every `/` byte, bidi-control codepoints (U+202A–U+202E, U+200E/F), bidi-isolate codepoints (U+2066–U+2069), narrow-no-break space (U+202F), zero-width codepoints (U+200B–U+200D), and the BOM (U+FEFF). Canonicalization requires nonempty UTF-8 NFC. No `.` or `..` whole-label path segments. Child routing uses resolved domain ID, never raw label concatenation. **`SubGroupLabel::new` rejects confusable-script codepoints (Cyrillic U+0400–U+04FF, Greek U+0370–U+03FF, Armenian U+0530–U+058F, Hebrew U+0590–U+05FF, Arabic U+0600–U+06FF, Cherokee U+13A0–U+13FF) via per-character UTS-39 reject set (enumerated v1.1 per W9 L3 C2 finding — prior wording "Cyrillic/Greek/Arabic/etc." was descriptive not exhaustive)**; **the full UTS-39 confusable-skeleton check against parent's existing labels remains recipient-side at validation step 4** (corrected v0.8 per L3 H1 finding — constructor does not have storage access to the `parent → existing_subgroup_labels` index, so the full skeleton comparison cannot live in `SubGroupLabel::new`). The constructor-level reject set blocks the worst per-character confusables; the recipient-side full skeleton check at validation step 4 catches the residual cross-label cases.

## Specification

### Envelope Type (this RFC owns CGSB only)

| Envelope Type      | Subtype tag | Direction           | Description                          |
| ------------------ | ----------- | ------------------- | ------------------------------------ |
| `DOT/1/CGROUP_SUB` | `b"CGSB"`   | DC → mesh broadcast | Create sub-group under active parent |

Subtype dispatch uses explicit typed parsing. Unsupported subtype remains unknown and never falls back to CGROUP.

Sibling envelopes owned by RFC-0855p-d2 (SDCD/SDRV/SDRT) and RFC-0855p-d3 (P2SR/S2PA/SGTP).

### Data Structure

RFC-0850p-d `CreateGroupEnvelope` fields remain unchanged. `CreateSubGroupEnvelope` adds `sub_group_extension` as a required field.

```rust
pub const MAX_SUBGROUP_DEPTH: u8 = 8;
pub const MAX_ROOT_DEPTH: u8 = 1;
pub const MAX_SUB_LABEL_BYTES: usize = 256;
pub const MAX_BIND_AWAIT_EPOCHS: u64 = 32;
pub const MAX_BIND_RETRY_COUNT: u8 = 3;
/// Backward-replay bound (canonical home; re-exported by RFC-0855p-d3 per its
/// §Data Structure). Distinct from `MAX_FSKEW_EPOCHS` so stale-replay
/// amplification is auditable independent of clock-drift tolerance.
pub const RACE_EPOCHS: u64 = 32;
/// Forward-skew tolerance (envelope epoch ahead of recipient head); separate
/// from `RACE_EPOCHS` (backward-replay bound) so clock-drift tolerance is
/// auditable independent of stale-replay amplification bound.
pub const MAX_FSKEW_EPOCHS: u64 = 4;
pub const SUBGROUP_DOMAIN_CONTEXT: &str = "DOT/1/CGROUP_SUB/domain";
pub const CREATE_SUBGROUP: [u8; 4] = *b"CGSB";

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
#[dcs(transparent)]
pub struct SubGroupLabel(BoundedBytes<MAX_SUB_LABEL_BYTES>);

// Sealed: no public inner accessor; construction MUST route through
// `SubGroupLabel::new` so every byte is canonicalized and validated.
mod sealed {
    pub trait Sealed {}
    impl Sealed for super::SubGroupLabel {}
}

impl SubGroupLabel {
    pub fn new(bytes: &[u8]) -> Result<Self, SubGroupLabelError> {
        if bytes.is_empty() {
            return Err(SubGroupLabelError::Empty);
        }
        if bytes.contains(&b'/') || bytes.contains(&0) || bytes.iter().any(u8::is_ascii_control) {
            return Err(SubGroupLabelError::InvalidByte);
        }
        if bytes == b"." || bytes == b".." {
            return Err(SubGroupLabelError::PathSegment);
        }
        let text = std::str::from_utf8(bytes)?;
        let normalized: String = text.nfc().collect();
        for ch in normalized.chars() {
            // Reject bidi-control, bidi-isolate, zero-width, NNBSP, and BOM
            // codepoints before any further normalization; full
            // confusable-skeleton check is the caller's responsibility per
            // §URL-path injection.
            match ch {
                '\u{202A}'..='\u{202E}'
                | '\u{2066}'
                | '\u{2067}'
                | '\u{2068}'
                | '\u{2069}'
                | '\u{202F}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{200B}'
                | '\u{200C}'
                | '\u{200D}'
                | '\u{FEFF}' => return Err(SubGroupLabelError::ConfusableChar),
                _ => {}
            }
        }
        // Authoritative length cap is POST-normalization: pre-normalize
        // input may grow during NFC composition.
        if normalized.as_bytes().len() > MAX_SUB_LABEL_BYTES {
            return Err(SubGroupLabelError::TooLong);
        }
        Ok(Self(BoundedBytes::new(normalized.as_bytes())?))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct ParentBindingInvariant {
    pub parent_domain_id: [u8; 32],
    pub required_state: GroupBindingState,
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubDomainDerivationInvariant {
    pub parent_domain_id: [u8; 32],
    pub sub_label: SubGroupLabel,
    pub derived_sub_domain_id: [u8; 32],
}

#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubGroupExtension {
    pub parent_domain_id: [u8; 32],
    pub sub_label: SubGroupLabel,
    pub sub_dc_id: [u8; 32],
    pub delegation_proof: Option<SubDCDelegationProof>, // RFC-0855p-d2 §Data Structure
    pub delegation_id: Option<DelegationId>,
}

#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DelegationId(pub [u8; 32]);

// Layer-A + Layer-B primitives are NOT redefined here per Stable Abstractions
// Principle + A→B→C dep direction rule. Both types come from upstream RFCs;
// PQC migration (RFC-0853) and DID canonicalization (RFC-0009 §Identity Struct)
// remain authoritative there.
pub use rfc_0853::Ed25519PublicKey;
pub use rfc_0009::Did;

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubGroupRecord {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub sub_label: SubGroupLabel,
    pub effective_dc_id: [u8; 32],
    pub state: SubGroupState,
    pub binding_state: GroupBindingState,
    pub created_epoch: u64,
    pub teardown_proof: Option<TeardownProof>, // RFC-0855p-d3 §TeardownProof
    pub inherited_policy_hash: [u8; 32],
}

impl SubGroupExtension {
    pub fn validate(
        &self,
        parent_dc_id: [u8; 32],
        parent_depth: u8,
        parent_binding: GroupBindingState,
    ) -> Result<[u8; 32], CreateSubGroupError> {
        if parent_binding != GroupBindingState::Active {
            return Err(CreateSubGroupError::ParentNotActive);
        }
        // Genesis-root path: parent depth MUST equal `MAX_ROOT_DEPTH = 1`
        // for the very first sub-group; any subsequent depth increment is
        // bounded by `MAX_SUBGROUP_DEPTH = 8`.
        if parent_depth < MAX_ROOT_DEPTH {
            return Err(CreateSubGroupError::DepthUnderflow);
        }
        if parent_depth >= MAX_SUBGROUP_DEPTH {
            return Err(CreateSubGroupError::DepthCap);
        }
        let derived = derive_sub_domain_id(self.parent_domain_id, self.sub_label.as_bytes());
        if self.sub_dc_id != parent_dc_id {
            // Match-and-validate without `.expect()` after `?`: a missing
            // proof returns `MissingDelegation`; a present proof is bound
            // to a local variable so the second access cannot panic.
            match self.delegation_proof.as_ref() {
                None => return Err(CreateSubGroupError::MissingDelegation),
                Some(proof) => proof.validate_for(self.sub_dc_id, derived)?,
            }
        } else if self.delegation_proof.is_some() {
            return Err(CreateSubGroupError::UnexpectedDelegation);
        }
        Ok(derived)
    }
}

#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct CreateSubGroupEnvelope {
    // Layer-B wire envelope: only identity, authority, and routing fields.
    // Transport choice (Layer D) and group-metadata policy (Layer C) live
    // behind typed query/response boundaries — see §Layer-C Substrate
    // Surface below.
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub domain_id: [u8; 32],
    pub mission_id: [u8; 16], // 16B BLAKE3-256(canonical_dcs(mission_did)) per RFC-0009 §Identity (cross-RFC consistency with RFC-0855p-e L182 16B field)
    pub target_routing_endpoint_id: [u8; 32],
    pub dc_id: [u8; 32],
    pub sub_group_extension: SubGroupExtension,
    pub nonce: [u8; 16],
    pub current_epoch: u64,
    pub coordinator_term_id: [u8; 32],
    pub signature: [u8; 64],
}

fn derive_sub_domain_id(parent_domain_id: [u8; 32], sub_label: &[u8]) -> [u8; 32] {
    // RFC-0853 §Cryptographic Primitives — BLAKE3 keyed-hash requires an
    // exact 32-byte key. `derive_key` expands the ASCII context string into
    // a deterministic 32-byte key; passing the raw `&str` slice would be a
    // type error (`&[u8; 32]` expected).
    let key = blake3::derive_key(SUBGROUP_DOMAIN_CONTEXT);
    let mut input = Vec::with_capacity(32 + sub_label.len());
    input.extend_from_slice(&parent_domain_id);
    input.extend_from_slice(sub_label);
    blake3::keyed_hash(&key, &input).as_bytes().to_owned()
}
```

`BoundedBytes<256>` enforces byte ceiling. `SubGroupLabel::new` enforces nonempty UTF-8, NFC, slash-free, NUL-free, and control-free input. Deserializer calls constructor before accepting the field. Domain derivation uses no separator because parent digest has fixed 32-byte length; label is exactly 1–256 UTF-8 bytes.

`ParentBindingInvariant` stores parent ID and required `GroupBindingState::Active`. `SubDomainDerivationInvariant` records parent, normalized label, and derived child ID. Both structs support deterministic test and audit output.

### Parent-Binding Invariant

No `PendingBind`, `Bound`, or transport invite exists for child unless parent `GroupBinding` resolves to `Active` at validation time. Parent chain must terminate at one active root. Logical parent identity never changes. A physical binding may move to another platform only through a new BIND nonce, epoch, parent authorization, and same `domain_id`; different logical parent is invalid.

### Sub-Domain Derivation Invariant

For all normalized bytes `L`, `sub_domain_id = BLAKE3_keyed("DOT/1/CGROUP_SUB/domain", parent_domain_id || L)`, where the ASCII context string `"DOT/1/CGROUP_SUB/domain"` is expanded to a 32-byte key via `blake3::derive_key` (RFC-0853 §Cryptographic Primitives). Returned digest has fixed 32 bytes. Recipient ignores claimed `domain_id` and uses verified derived value. Mesh index key is `(parent_domain_id, derived_sub_domain_id)`. No two active child records may share that key.

### Substrate Compliance

The canonical recipe in §Sub-Domain Derivation Invariant is the ONLY permitted form for `sub_domain_id` derivation. Substrate implementations (e.g. `crates/octo-network/src/dot/subgroup_state.rs` post-split) MUST compute `sub_domain_id` exactly as `blake3::keyed_hash(&blake3::derive_key("DOT/1/CGROUP_SUB/domain"), parent_domain_id || sub_label)`. The following non-compliant forms are forbidden:

- Plain `blake3::hash(input)` without the keyed_hash construction (omits both `derive_key` context expansion and the keyed_hash permutation).
- Plain `blake3::hash("DOT/1/CGROUP_SUB/domain" || parent_domain_id || sub_label)` — concatenates the context as raw bytes inside the input rather than expanding it through `derive_key`. This is NOT the same digest as the keyed_hash form and MUST NOT be substituted.
- Any derivation that omits `parent_domain_id` from the input (breaks cross-parent identity per §Implicit Assumptions Audit).

Test vectors TV-SG-1 and any future TV in this RFC family MUST be computed against the canonical keyed_hash form.

### State Machine

`SubGroupState` has four variants. Finite lifecycle values form one state machine; envelope extension does not add variants to it.

```rust
// `#[non_exhaustive]` is required: future RFCs adding `Suspended` /
// `Archived` / `Frozen` would otherwise force a cross-crate edit to every
// match site (transition table + substrate queries + Layer-C match sites).
// Existing match sites MUST include a bounded wildcard arm.
#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubGroupState {
    PendingBind,
    Bound,
    Dissolving,
    Dissolved,
}
```

| Current        | Trigger                                                          | Guard                                                                                                                                 | Next          | Side effect                                     |
| -------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------- | ----------------------------------------------- |
| Absent         | Verified CGSB create                                             | Parent active; depth `>= MAX_ROOT_DEPTH` and `< MAX_SUBGROUP_DEPTH`; derived ID fresh; proof rules pass                               | `PendingBind` | Reserve `(parent, child)`; queue CGROUP BIND    |
| `PendingBind`  | BIND succeeds                                                    | BIND nonce, parent authorization, and platform match; bind within `MAX_BIND_AWAIT_EPOCHS = 32`; retries <= `MAX_BIND_RETRY_COUNT = 3` | `Bound`       | Open invite delivery and child messaging        |
| `PendingBind`  | BIND fails or deadline expires                                   | Failure reason canonical; retry limit not exceeded                                                                                    | `Dissolved`   | Remove child reservation                        |
| `PendingBind`  | Parent UNBIND while child awaiting BIND                          | Parent flips to `Dissolving` between create-receipt and BIND-commit; parent-state check runs at BIND-commit, not create-receipt       | `Dissolving`  | Cancel reservation; start teardown grace window |
| `Bound`        | Explicit child UNBIND                                            | Parent DC or authorized sub-DC signs; no parent cascade                                                                               | `Dissolving`  | Withdraw invites; stop new routing              |
| `Bound`        | Parent UNBIND                                                    | Child is descendant; cascade transaction commits                                                                                      | `Dissolving`  | Dissolve every descendant transitively          |
| `Bound`        | Parent revokes child sub-DC                                      | Effective DC equals revoked delegation ID; transaction commits                                                                        | `Dissolving`  | Dissolve descendants using that sub-DC          |
| `Dissolving`   | `TEARDOWN_GRACE_EPOCHS = 50` elapsed and teardown proof recorded | State `Dissolving`; `SubGroupRecord.teardown_proof` populated via `TeardownProofEnvelope` (subtype `b"SGTP"`, RFC-0855p-d3)           | `Dissolved`   | Purge live transport handles and stop delivery  |
| `Dissolving`   | Verified retry or repair                                         | Only authorized repair; canonical failure reason                                                                                      | `Dissolving`  | Preserve teardown, update audit record          |
| Any non-absent | Depth overflow, parent-lineage loss, or duplicate-parent attempt | Reject, do not mutate                                                                                                                 | Same state    | Record violation                                |

Normal child transitions do not mutate parent. Parent UNBIND is explicit override: all active descendants enter `Dissolving`; only then child records may become `Dissolved` after their own teardown proofs (RFC-0855p-d3). Parent cannot remain `Bound` after root UNBIND.

### Depth Cap Enforcement

`MAX_SUBGROUP_DEPTH` is 8. Root depth equals 1. Every child depth equals parent depth plus 1. Recipient resolves fresh active parent chain, rejects cycle or missing parent, and rejects creation when parent depth is 8 or greater. It performs this query before reserving domain ID, creating state, or broadcasting invites. A descendant deeper than 8 cannot exist, even if intermediate mesh is compromised. The genesis-root invariant `MAX_ROOT_DEPTH = 1` is enforced in `SubGroupExtension::validate`: any parent depth `< MAX_ROOT_DEPTH` is rejected as `DepthUnderflow` before the `< MAX_SUBGROUP_DEPTH` upper-bound check.

### Layer-C Substrate Surface (creation + query subset)

Layer-C substrate exposes typed query/response structs so that no Layer-B wire parser reaches into substrate state. The data-owning module implements these operations against the `SubGroupState` store; callers from Layer-B envelopes resolve authority, depth, and membership through this boundary.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupQuery {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
}

// Layer-C response uses a typed discriminator (response_kind: u16) plus a
// bounded payload; this avoids a central enum that would force every new
// response variant to land as a cross-crate edit (violates §Extension over
// enumeration). The discriminator table below is RFC-allocated; substrate
// fails closed on unknown response_kind values.
pub const SUBGROUP_RESPONSE_NOT_FOUND: u16 = 0x0001;
pub const SUBGROUP_RESPONSE_PENDING_BIND: u16 = 0x0002;
pub const SUBGROUP_RESPONSE_BOUND: u16 = 0x0003;
pub const SUBGROUP_RESPONSE_DISSOLVING: u16 = 0x0004;
pub const SUBGROUP_RESPONSE_DISSOLVED: u16 = 0x0005;
// 0x0006-0x00FF: RFC-allocated response kinds for future use.
// 0x0100-0xFFFF: user-extension registry response kinds.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupResponse {
    pub response_kind: u16,
    pub response_payload: BoundedBytes<1024>,
}

impl SubGroupResponse {
    pub fn not_found() -> Self {
        Self {
            response_kind: SUBGROUP_RESPONSE_NOT_FOUND,
            response_payload: BoundedBytes::new_empty(),
        }
    }

    pub fn from_record(record: &SubGroupRecord, kind: u16) -> Result<Self, ResponseEncodeError> {
        let bytes = dcs_encode(record)?;
        Ok(Self {
            response_kind: kind,
            response_payload: BoundedBytes::new(&bytes)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupAuthorityCheck {
    pub sub_domain_id: [u8; 32],
    pub actor_dc_id: [u8; 32],
    // action dispatched by RFC-0855p-d2 (delegate/revoke) or RFC-0855p-d3 (route/aggregate/dissolve)
    pub action: SubGroupAction,
}
```

`SubGroupAction` itself is defined as a shared typed-discriminator (this RFC owns the struct + dispatch table; RFC-0855p-d2 + RFC-0855p-d3 register their action_type_id values against the shared table):

```rust
// Layer-C action uses a typed discriminator (action_type_id: u32) plus a
// bounded payload. The discriminator namespace below is RFC-allocated;
// 0x0100-0xFFFF is reserved for user-extension registry entries so new
// actions land without touching the core authority module.
pub const SUBGROUP_ACTION_INVITE: u32 = 0x0001;
pub const SUBGROUP_ACTION_ROUTE: u32 = 0x0004;
pub const SUBGROUP_ACTION_AGGREGATE: u32 = 0x0005;
// 0x0002-0x0003 reserved for RFC-0855p-d2 (REVOKE/DISSOLVE via SUBGROUP_ACTION_*).
// 0x0006-0x00FF: RFC-allocated action types for future use.
// 0x0100-0xFFFF: user-extension registry action types.

pub const SUBGROUP_ACTION_PAYLOAD_MAX: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubGroupAction {
    pub action_type_id: u32,
    pub action_payload: BoundedBytes<SUBGROUP_ACTION_PAYLOAD_MAX>,
}

impl SubGroupAction {
    pub fn invite() -> Self {
        Self {
            action_type_id: SUBGROUP_ACTION_INVITE,
            action_payload: BoundedBytes::new_empty(),
        }
    }

    pub fn route(payload: &ParentToSubRouteEnvelope) -> Result<Self, ActionEncodeError> {
        let bytes = dcs_encode(payload)?;
        Ok(Self {
            action_type_id: SUBGROUP_ACTION_ROUTE,
            action_payload: BoundedBytes::new(&bytes)?,
        })
    }

    pub fn aggregate(payload: &SubToParentAggregateEnvelope) -> Result<Self, ActionEncodeError> {
        let bytes = dcs_encode(payload)?;
        Ok(Self {
            action_type_id: SUBGROUP_ACTION_AGGREGATE,
            action_payload: BoundedBytes::new(&bytes)?,
        })
    }
}
```

The boundary is fail-closed: unknown `action_type_id` or `response_kind` values, or missing records, return `SubGroupResponse::not_found()`; authority checks return a `bool` plus reason code, never a parsed Layer-B envelope. The typed discriminator (RFC-allocated namespace 0x0001-0x00FF + user-extension range 0x0100-0xFFFF) lets new authority and response variants land in the user-extension registry without central edits to this module.

### Recipient Verification (CGSB)

Each CGSB recipient executes these checks in order before accepting or rebroadcasting. Steps 1–8 perform canonical authority validation BEFORE any state mutation; step 9 records the nonce-index entry only after steps 1–8 pass, so partial-validation envelopes never poison the replay cache.

1. DCS decode succeeds; `envelope_subtype == b"CGSB"`; version is supported.
   1a. **Forward-skew bound** (clock-drift tolerance): reject envelope where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS` before signature verification. Separate from `RACE_EPOCHS` (backward-replay bound) so clock-drift tolerance is auditable independent of stale-replay amplification. Bound = `MAX_FSKEW_EPOCHS = 4` matches RFC-0855p-e cross-RFC invariant (see RFC-0855p-e §Implicit Assumptions Audit; aligned to ±4 epochs for the family).
2. `SubGroupLabel::new` accepts normalized bytes; claimed parent and creator DC are nonzero and well formed.
3. `domain_id == BLAKE3_keyed("DOT/1/CGROUP_SUB/domain", parent_domain_id || sub_label)`.
4. No active or pending child occupies `(parent_domain_id, domain_id)`.
5. Parent `GroupBinding` resolves to one active record; parent chain terminates at active root and depth satisfies `MAX_ROOT_DEPTH <= parent_depth < MAX_SUBGROUP_DEPTH`.
6. `mission_id`, inherited-policy hash, coordinator term, epoch, and parent DC match canonical parent state.
7. If `sub_dc_id == parent_dc_id`, `delegation_proof` is `None`. Otherwise proof is present, current, non-replayed, parent-signed, and scoped to exact child domain and DC (delegation proof verification per RFC-0855p-d2 §Sub-DC Delegation Protocol). Descendant delegation is asserted through `SubDCDelegationPolicy::check(proof, subgroup_state_bound=true)` (RFC-0855p-d2) and `MAX_DELEGATION_CHAIN_PER_TERM`.
8. CGROUP signature verifies under current parent DC and covers domain, extension, nonce, epoch, coordinator term, and `delegation_proof` (when present) per signature-coverage invariant (added v1.3 per W11 L1 M4 finding).
9. (Post-validation) Nonce remains unconsumed under replay key `(CGSB, creator_dc_id, parent_domain_id, sub_domain_id, current_epoch, nonce)`. Only after steps 1–8 succeed does the recipient record the replay-key entry. Cross-node `PendingBind` reconciliation: the BIND-commit path re-runs the parent-state check (steps 5–6) at BIND-commit time, not at create-receipt, so a parent flip between create-receipt and BIND-commit causes `PendingBind → Dissolving` (state-machine row above) rather than silent orphan bind.
10. Parent broadcasts create only to current parent members or validators. Child BIND broadcasts only after child state becomes `Bound`.
11. Recipient lacking current parent state, proof index, or nonce index quarantines envelope; it does not guess success.

Create processing derives child ID from extension, not outer `domain_id`. Signature validation then treats outer `domain_id` and derivation invariant as one covered field. Derived value must match exactly.

## RFC-0008 Execution Class Mapping

(Added v1.1 per W10 L5 C4 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

| Operation                                                | Class | Justification                                                                                               |
| -------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------- |
| CGSB (CreateSubGroup) envelope derive + recipient accept | A     | BLAKE3 keyed_hash derivation + domain separation + signature verification; deterministic; no external state |
| `SubGroupLabel::new` (UTS-39 confusable + NFC)           | A     | Pure-function rejection; no external state                                                                  |
| `SubGroupState` transition engine                        | C     | Cross-node reconciliation requires consensus-aggregated state; non-deterministic under partition            |
| `SubGroupRecord` cross-node reconciliation               | C     | Storage-backed; SERIALIZABLE transactions; coordinator-side                                                 |

Sibling envelope execution classes: SDCD/SDRV/SDRT (RFC-0855p-d2); P2SR/S2PA/SGTP (RFC-0855p-d3).

## Determinism Requirements

(Added v1.1 per W10 L5 C5 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

- **Canonical derivation**: `sub_domain_id = BLAKE3_keyed("DOT/1/CGROUP_SUB/domain", parent_domain_id || sub_label)` — every recipient MUST recompute deterministically from the exact UTF-8 NFC bytes inside `SubGroupLabel`. No trusted sender-provided `domain_id`. Mesh rejects duplicate child under same parent before BIND.
- **Replay-key tuple encoding**: CGSB `current_epoch` field in replay tuple uses canonical BE bytes (RFC-0126 array-of-u8 form). Prevents cross-subtype dedup collision.
- **Canonical ordering**: replay-cache dedup uses canonical encoding of the replay-key tuple (lexicographic on serialized bytes). Two recipients with different lex orderings produce identical dedup decisions.
- **Epoch monotonicity**: recipients reject envelopes where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS` (`MAX_FSKEW_EPOCHS = 4`). Anti-entropy replay is bounded to ±4 epochs.
- **State machine determinism**: `SubGroupState` transitions are deterministic given the FSM + observed envelope sequence. No randomness, no leader election.
- **Cross-replica determinism**: given identical observed envelope sequences, every replica reaches the same `SubGroupState` (subject to consensus-aggregated witness quorum agreement).

## Lifecycle Requirements

(Added v1.1 per W10 L5 C6 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

| Constant                | Value | Purpose                                                                       |
| ----------------------- | ----- | ----------------------------------------------------------------------------- |
| `MAX_SUBGROUP_DEPTH`    | 8     | Maximum nesting depth (root → leaf); reject creation beyond                   |
| `MAX_ROOT_DELEGATION`   | 1     | Root can delegate to at most 1 sub-DC; reject additional delegations          |
| `MAX_BIND_AWAIT_EPOCHS` | 32    | `PendingBind → Bound` deadline; child MUST BIND within 32 epochs              |
| `MAX_BIND_RETRY_COUNT`  | 3     | Maximum `PendingBind → Dissolving → Bound` retry loops before forced teardown |
| `MAX_FSKEW_EPOCHS`      | 4     | Forward epoch-skew tolerance for envelope acceptance                          |

State machine lifecycle: `PendingBind → Bound → Dissolving → Dissolved` (with `Suspended` / `Archived` / `Frozen` extension points via `#[non_exhaustive]` on `SubGroupState`). Each transition has a deadline; expiry triggers escalation per RFC-0855p-b §Slash tally observability. Delegation-chain cap `MAX_DELEGATION_CHAIN_PER_TERM = 256` lives in RFC-0855p-d2 §Lifecycle Requirements. Teardown grace `TEARDOWN_GRACE_EPOCHS = 50` and aggregate caps live in RFC-0855p-d3 §Lifecycle Requirements.

## Performance Targets

- Canonical validation target: under 1 ms p95 excluding durable state writes.
- Label resolution target: under 10 ms p95 from cache miss through BLAKE3-256 and parent lookup.
- State writes: one transaction for child reservation.

Targets are local p95 on agreed reference hardware.

## Compatibility

RFC-0850p-d CGROUP consumers filter on supported subtype. They see unknown CGSB safely, log unsupported subtype, and do not decode or act. No CGROUP field changes. Old clients cannot create sub-groups because CGSB parse and CGSB-specific CLI route are unavailable; they may still read subgroup records through documented public state queries where policy permits. Wire addition is backward compatible. Unknown SDCD/SDRV/SDRT (RFC-0855p-d2) and P2SR/S2PA/SGTP (RFC-0855p-d3) subtypes also fail closed. Version negotiation uses outer `version`; unsupported version is rejected without fallback.

## Test Vectors

All vectors use fixed BLAKE3 digests for reproducibility. Hex values are representative test constants, not production identifiers.

### TV-SG-1 — Valid create-sub-group

Input:

- parent depth: 1
- parent state: `Active`
- label: `legal-review` (13 bytes)
- `sub_dc_id`: parent DC
- delegation proof: absent
- nonce: `01` repeated 16 times
- epoch: 10

Expected:

- `domain_id = BLAKE3_keyed("DOT/1/CGROUP_SUB/domain", parent_domain_id || "legal-review")` — i.e. derive a 32-byte key via `blake3::derive_key("DOT/1/CGROUP_SUB/domain")` and apply `blake3::keyed_hash(&key, parent_domain_id || "legal-review")`. The keyed_hash form per §Sub-Domain Derivation Invariant is the canonical recipe; the plain `BLAKE3("DOT/1/CGROUP_SUB/domain" || …)` form is NOT a valid test vector and MUST NOT be substituted by substrate.
- State transition: absent → `PendingBind`.
- Successful transport BIND: `PendingBind → Bound`.
- Proof, duplicate, depth, and replay checks pass.

### TV-SG-2 — Depth-cap rejection

Input:

- parent depth: 8
- parent state: `Active`
- valid label, current term, valid parent signature, fresh nonce.

Expected:

- Reject `CreateSubGroupError::DepthCap`.
- No `(parent, child)` reservation.
- No `PendingBind`, invite, or BIND.
- No child depth 9 exists.

### TV-SG-3 — Label containing `/` rejection

Inputs: `ops/review`, `../ops`, empty bytes, NUL byte, and ASCII LF byte. Each uses otherwise valid CGSB data.

Expected:

- `SubGroupLabel::new` returns corresponding input error.
- Deserializer rejects field before BLAKE3.
- No claimed domain is accepted, even if sender repeats an unrelated `domain_id`.

### TV-SG-4 — Nested rebind

Positive case:

- Existing child domain ID created under logical parent `P` on platform A.
- Parent remains `Active`; logical parent is still `P`.
- Parent-authorized BIND uses new epoch, nonce, binding ID, and platform B.

Expected:

- Logical parent, mission, child domain, and parent DC remain unchanged.
- Child transitions `Bound → PendingBind → Bound`; parent and siblings remain unchanged.

Negative case:

- Same child requests BIND under parent `Q` or returns old BIND nonce.

Expected:

- Reject `ParentIdentityMismatch` or replay.
- No lineage mutation, no new logical depth, no parent takeover.

### TV-SG-5 — Parent-dissolve cascade

Input:

- Root, child, and grandchild states: `Bound`.
- Parent DC submits root UNBIND with valid term, epoch, and nonce.

Expected:

- Root enters `Dissolving`.
- Child and grandchild enter `Dissolving` in same cascade transaction.
- Each teardown emits own BIND-close proof (RFC-0855p-d3) before becoming `Dissolved`.
- No child remains `Bound` after cascade commit.
- No sibling or child operation can unbind root.

## Alternatives Considered

### Flat hierarchy only

Rejected. Working groups need parentage, policy lineage, scoped delegation, and deterministic descendant routing. Flat tags lose these invariants.

### Tags without nesting

Rejected. A tag taxonomy cannot prove one active parent, bound child depth, child-scoped authority, or parent-dissolve cascade.

### DNS-style path addressing

Retained for human-readable labels, but raw path addressing is not identity. Routing resolves BLAKE3 domain ID. `/` stays forbidden to prevent path injection and ambiguity.

### Adjacency list only

Rejected. Pure adjacency omits immutable digest commitment, active parent invariant, duplicate label detection, and auditable derivation proof. Logical hierarchy remains in state; adjacency may serve traversal index.

### Extend CGROUP with optional extension

Rejected. Optional fields weaken old-client compatibility and make CGSB semantics non-obvious. Dedicated CGSB subtype lets old consumers filter safely and new recipients validate all required invariants.

### Parent DC identity for every sub-group automatically

Rejected as sole rule. One-child sub-DC delegation needs parent-signed, rotatable, revocable authority (RFC-0855p-d2). Implicit fallback remains only when explicit child delegation is absent.

## Implementation Phases

### Phase 1 — Envelope and wire (d1 subset)

- Add `CreateSubGroupEnvelope` and `SubGroupExtension`.
- Add validated `SubGroupLabel` over `BoundedBytes<256>`.
- Add DCS derives, CGSB dispatch, BLAKE3 derivation, and signature coverage.
- Add `RACE_EPOCHS = 32` (backward-replay bound) and `MAX_FSKEW_EPOCHS = 4` (forward-skew tolerance) constants. Enforce the race-window rule: any state-changing envelope whose `current_epoch` lies more than `RACE_EPOCHS` behind the recipient's current head epoch, OR more than `MAX_FSKEW_EPOCHS` ahead of it, is rejected before signature verification. The two bounds are kept separate so clock-drift tolerance is auditable independent of stale-replay amplification.
- Add compatibility tests proving CGROUP ignores CGSB.

### Phase 2 — State and validator (d1 subset)

- Add `SubGroupState`, parent binding queries, depth query, duplicate index, and replay index.
- Enforce recipient verification (CGSB-specific) and bounded nonce storage.

### Phase 3 — Client API (cross-RFC surface)

- Add `octo-mesh` commands for sub-group create (and delegate/rotate/revoke from RFC-0855p-d2; route/aggregate/decommission from RFC-0855p-d3).
- Show parent depth, effective sub-DC, delegation expiry, binding state, and policy hash.
- Require explicit confirmation for dissolve and revocation.
- Publish dashboards for depth, broadcast amplification, validation latency, and cascade time.

## Key Files to Modify

- `crates/octo-network/src/dot/subgroup_state.rs` (new per restructure, formerly `sub_group.rs` monolithic) — `CreateSubGroupEnvelope` + `SubGroupExtension` + `SubGroupLabel` + `SubGroupRecord` + `SubGroupState` transition engine + `SubGroupQuery` / `SubGroupResponse` / `SubGroupAuthorityCheck` typed query boundary + nonce + duplicate + parent-binding + depth-cap indexes; `MAX_BIND_AWAIT_EPOCHS` + `MAX_BIND_RETRY_COUNT` + `MAX_SUBGROUP_DEPTH` + `MAX_FSKEW_EPOCHS` enforcement; `MAX_ROOT_DEPTH = 1` enforcement; canonical BLAKE3 keyed_hash derivation; cross-node `PendingBind → Dissolving` reconciliation.
- `missions/archived/0855p-d-subgroup-nesting.md` — companion implementation and acceptance mission (Completed, Path B closure 2026-07-30). Update with 0855p-d1 split path.

## Economic Analysis

DEFER to RFC-0917 and RFC-0960. This RFC defines identity, authority, lifecycle, and validation only. Sub-group creation has no direct token transfer, fee, reward, stake, settlement, or accounting surface here. Any future quota, storage, or transport incentive MUST land in those RFCs through additive amendment.

## Future Work

- **F-6, label collision:** Resolved inline through parent-bound digest and duplicate-key enforcement.
- **F-7, label format:** Resolved inline through `BoundedBytes<256>` and validated `SubGroupLabel`.
- **F-10, cross-mission children:** Future work. Current invariant requires inherited `mission_id` from one parent.
- **F-12, substrate migration:** Migrate `crates/octo-network/src/dot/subgroup_state.rs` from legacy plain-`blake3::hash(parent_domain_id || sub_label)` to the canonical keyed_hash form per §Sub-Domain Derivation Invariant. Follow-on implementation mission; not part of this RFC iteration's substrate-truth. Substrate remains doc-only to follow this spec until that migration mission closes.

## Rationale

A separate `DOT/1/CGROUP_SUB` envelope preserves CGROUP ABI. Adding optional fields to CGROUP would make validity depend on a later variant and tempt old clients to ignore security-critical extension state. CGSB gives explicit dispatch, stable wire discriminator, and fail-closed compatibility.

`b"CGSB"` means CGroup Subgroup Body. Four-byte tag matches existing subtype encoding and remains cheap to filter. Unsupported tags never enter group-state parsing.

URL-style labels improve human routing, but raw labels never identify state. BLAKE3 binds normalized bytes to immutable parent ID, producing collision-resistant 256-bit identity. Slash prohibition removes path-segment ambiguity and injection.

`BoundedBytes<256>` avoids unbounded `String` allocation and supplies canonical byte-length check. Wrapping it in `SubGroupLabel` adds UTF-8, NFC, slash, NUL, control, empty, and path-segment rules at the type boundary. A bare `String` cannot enforce slash absence or allocation ceiling.

Parent-binding invariant prevents orphan domains and authority drift. Child-scoped delegation (RFC-0855p-d2) prevents parent takeover while preserving delegated operations. Separate physical GroupBinding preserves independent transport lifecycle without losing logical lineage.

## Version History

| Version | Date       | Changes                                                                             |
| ------- | ---------- | ----------------------------------------------------------------------------------- |
| 1.3     | 2026-09-02 | Split from v1.2.. d1 owns CGSB + state + label + record + Layer-C query (creation). |

## Appendices

(Added v1.3 per W11 L5 H4 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

### A. SubGroupLabel UTS-39 confusable reject set (constructor-level)

Per-character reject codepoints (v1.1 per W9 L3 C2 finding; v0.8 wording corrected per L3 H1 finding):

- Bidi-control: U+202A–U+202E
- Bidi-isolate: U+2066–U+2069
- Zero-width: U+200B–U+200D
- Narrow-no-break space: U+202F
- Byte order mark: U+FEFF

Full UTS-39 confusable-skeleton comparison against parent's existing labels remains recipient-side at validation step 4 (v0.8 correction per L3 H1 finding).

### B. Canonical derivation recipe

`sub_domain_id = BLAKE3_keyed("DOT/1/CGROUP_SUB/domain", parent_domain_id || sub_label)`

- `derive_key("DOT/1/CGROUP_SUB/domain")` expands ASCII context to 32-byte key (RFC-0853 §Cryptographic Primitives)
- `keyed_hash(&key, parent_domain_id || sub_label)` produces 32-byte digest
- Pre-normalization input may grow during NFC composition; length cap enforced POST-normalization

## Related RFCs

- RFC-0850 — Deterministic Overlay Transport
- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0850p-d — DC-Initiated Transport Group Creation & Invite
- RFC-0853 — Overlay Cryptography (OCrypt); §Cryptographic Primitives mandates BLAKE3-256
- RFC-0126 — DCS deterministic canonical serialization
- RFC-0009 — Identity substrate (canonical `Did` type re-exported in §Layer placement)
- RFC-0855p-b — Mission Coordinator Lifecycle, including slash tally and SlashOffenseCodes §B
- RFC-0855p-c — DomainCoordinator Role and parent DC authority scope
- RFC-0855p-d — Monolithic predecessor (now slim INDEX; superseded by d1/d2/d3)
- RFC-0855p-d2 — Sub-DC Delegation Lifecycle (sibling RFC in this chain)
- RFC-0855p-d3 — Routing + Aggregation + Teardown (sibling RFC in this chain)
- RFC-0855p-e — Mission Coordinator Handover Envelope (sibling RFC; uses `mission_id` 16B field)

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — DC Delegation
- `docs/use-cases/social-platform-transport-layer.md` — Hierarchical Grouping
