# RFC-0967 (Economics): Policy Object Graph — Separable Authorization Policy

## Status

Accepted v1.0

> **Note:** Companion RFC to RFC-0965 (Capability extension format) and RFC-0960 (Grand design). Introduces `PolicyObject` as a first-class, reusable, versionable, shareable authorization policy artifact. Capabilities carry a `policy_id` reference, not embedded policy clauses. This RFC replaces the "Capability as bag of caveats" model with a "Capability → PolicyID reference" model that more closely resembles how page tables and IAM policies work in mature systems.

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-23 | @cipherocto + @mmacedoeu | Initial draft; emerges from R17+ strategic reframe of the grand-design stack. |
| v1.0-Accepted | 2026-07-23 | @cipherocto + @mmacedoeu | **Promoted Draft → Accepted.** R1-R28 multi-round adversarial review closed with R28 clean round (zero actionable defects). Companion RFCs (RFC-0960, RFC-0961, RFC-0962, RFC-0963, RFC-0964, RFC-0965) promoted in lockstep on 2026-07-23. |

## 1. Motivation

### 1.1 The capability bag problem

RFC-0957 + RFC-0965 model a capability as a macaroon: a bag of first-party + third-party caveats. Adding the RFC-0965 extension caveat types (21 total) gives capabilities enough expressiveness to encode any policy. But this conflates two distinct concerns:

- **Identity / attenuation chain** — who holds the capability, how was it derived, who signed it
- **Authorization policy** — what the holder is permitted to do, under what conditions, with what limits

A capability is the right place for identity and the attenuation chain (macaroon model is well-suited). It is the **wrong** place for an arbitrarily large, evolving policy. As policy clauses grow (rate limits, spend caps, time windows, asset whitelists, audit windows, per-counterparty limits), the capability envelope bloats. Two unrelated capabilities that happen to share the same policy must each carry a full copy of the policy bytes — duplication, drift risk, no shared audit.

### 1.2 The page-table analogy

Mature systems separate **identity** from **policy**:

- **Page tables** (x86-64): a process has a page-table root register (identity). Page table entries are a separate data structure shared across processes that map to the same physical pages. Updating the page table does not invalidate the process identity.
- **IAM policies** (AWS, GCP, Azure): a user has an identity (IAM role / service account). IAM policies are separate JSON objects referenced by ARN. The same policy can be attached to many identities.
- **SELinux / AppArmor**: an identity (process label) references a *type enforcement* policy file. Policy lives on disk, identity references it.

This RFC applies the same pattern to CipherOcto capabilities.

### 1.3 Strategic fit

User feedback (2026-07-23) explicitly called out the policy-bloat problem:

> Macaroons become difficult to reason about. Instead I'd separate Identity from Policy. Capability → references → Policy Object. Rather than embedding everything. Exactly how page tables work.

> Reusable. Versionable. Shared. Auditable. Much smaller proofs.

This RFC implements that feedback. The `PolicyObject` is the page-table entry; the `Capability` is the page-table root register.

## 2. The `PolicyObject` envelope

```text
PolicyObject {
    version_tag:         u8,                    // currently 1
    policy_id:           PolicyID,              // BLAKE3(canonical_ser(policy_unsigned))
    version_seq:         u64,                   // monotonic per lineage; 1 = genesis
    parent_policy_id:    Option<PolicyID>,      // version chain (None = genesis)
    graph:               PolicyGraph,           // DAG<PolicyNode>
    audit_ref:           Hash,                  // BLAKE3 of deterministic audit-trail commitment
    timestamp_unix_ms:   u64,                   // wall-clock at policy creation
    signature:           Ed25519Signature,      // over canonical_ser(policy_unsigned)
}

policy_unsigned := all fields above except `signature` and `policy_id`
```

`PolicyID` is a 32-byte BLAKE3 hash. Two encodings of the same policy graph must produce the same `policy_id`.

## 3. The `PolicyGraph` DAG

```text
PolicyGraph {
    root_nodes:    Vec<PolicyNodeID>,
    all_nodes:     Vec<PolicyNode>,
}

PolicyNode {
    node_id:       PolicyNodeID,               // BLAKE3(canonical_ser(node_body))
    predicate:     Constraint,                  // RFC-0964 Constraint (23 variants)
    action:        PolicyAction,                // Allow | Deny | RequireApproval | Audit
    children:      Vec<PolicyNodeID>,           // DAG edges
    description:   Option<String>,              // human-readable; does NOT participate in canonical_ser
}

PolicyAction ::= Allow
               | Deny
               | RequireApproval(approval_kind: ApprovalKind)
               | Audit(audit_window_secs: u64)

ApprovalKind ::= SingleSigner
               | Quorum(n: u8)                  // 1 ≤ n ≤ 23
               | TimeLocked(unlock_at_unix: u64)
```

A `PolicyGraph` is a DAG (not a tree) because multiple nodes may share child subgraphs. The graph is **canonicalized** before `canonical_ser`:

- `all_nodes` is sorted by `node_id` ascending.
- `root_nodes` is sorted ascending.
- `children` arrays are sorted ascending.
- `description` is stripped (it is metadata, not policy).

## 4. Canonical serialization

Per RFC-0126 Part 2 (JSON structured data):

```json
{
    "version_tag": 1,
    "version_seq": 7,
    "parent_policy_id": "blake3:...",
    "graph": {
        "root_nodes": ["blake3:...", "blake3:..."],
        "all_nodes": [
            {
                "node_id": "blake3:...",
                "predicate": {"type": "PerAssetSpendingCap", "caps": [["OCTO", 1000000]]},
                "action": "Deny",
                "children": []
            }
        ]
    },
    "audit_ref": "blake3:...",
    "timestamp_unix_ms": 1753182134000
}
```

Wire format: canonical JSON, sorted keys, no whitespace. Hash = `BLAKE3(0xC0 || canonical_ser(policy_unsigned))`. The `0xC0` prefix is the cross-RFC domain separator for PolicyObject hashes, parallel to the `0xA0-0xAF` reserved range (RFC-0964 §0.1). `policy_id = BLAKE3(0xC0 || canonical_ser(policy_unsigned))`. `policy_id` is derived WITHOUT including `policy_id` itself (canonical_ser of `policy_unsigned` excludes the `policy_id` field).

## 5. Attenuation: subgraph relation

A capability attenuation chain in RFC-0957 means the child capability's caveat set is a *superset* of the parent's (more restrictive). The same rule applies to PolicyObjects:

> **Attenuation invariant:** `policy_child.graph` is a **subgraph** of `policy_parent.graph`. The child may add nodes, restrict actions, or remove root nodes — but every node the child retains must also appear in the parent's graph.

Formally: for every node `n` in `child.all_nodes`, there exists a node `n'` in `parent.all_nodes` such that:

- `n.predicate ≤ n'.predicate` (more restrictive or equal)
- `n.action ≤ n'.action` (Deny < RequireApproval < Allow; Audit is independent)
- The parent reachability covers the child reachability

Cross-policy attenuation is **not** transitive across policy lineages. If `policy_a` and `policy_b` are disjoint lineages, a child cannot be derived from both. The capability attenuation chain still tracks lineage via `parent_capability` (RFC-0965); the policy attenuation is local to each lineage.

## 6. Reusability and versioning

Multiple capabilities can reference the same `policy_id`. A policy update creates a **new** `PolicyObject` with `parent_policy_id = Some(old_policy_id)` and `version_seq = old.version_seq + 1`. Existing capabilities continue to reference the old `policy_id`; new capabilities reference the new one. No migration is forced.

```text
PolicyObject v1 (policy_id = X1, version_seq = 1)
    │
    ├── referenced by Capability A
    │
PolicyObject v2 (policy_id = X2, version_seq = 2, parent = X1)
    │
    ├── referenced by Capability B (new issuance)
    ├── referenced by Capability A' (attenuated re-issue)
```

A capability's attenuation chain (RFC-0957) is independent of the policy version lineage. The capability chain tracks *who* signed what; the policy lineage tracks *what* the policy was.

## 7. Audit reference

`audit_ref: Hash` commits to the audit-trail of the policy: a BLAKE3 hash over the deterministic log of:

- policy creation event
- every policy version transition
- every attenuation that referenced this policy
- every revocation

The audit log is itself a projection of the WAL (RFC-0960 §16 Event Store). `audit_ref` is thus itself a WAL-derived hash, making the policy's history independently verifiable from the chain.

## 8. Capability ↔ PolicyObject linkage

### 8.1 Capability shape change

RFC-0965 capability envelope adds a new caveat type:

```text
Caveat::PolicyReference {
    policy_id:           PolicyID,
    policy_version_seq:  u64,                // pin to specific version
    attenuation_proof:   AttenuationProof,    // §8.2
}
```

The capability envelope carries `policy_id` + `policy_version_seq` references. The Capability's own caveats (RFC-0957 first-party + third-party + RFC-0965 extensions) carry **identity / attenuation / revocation** concerns only. **All policy clauses live in the referenced PolicyObject.**

A capability that references no `PolicyReference` caveat is interpreted as `policy_id = 0x00..00` (the empty / fully-permissive policy). This is the legacy-default; explicit `PolicyReference` is recommended for production use.

### 8.2 Attenuation proof

When a child capability is issued, it must prove that the referenced `PolicyObject` is consistent with the parent's policy (subgraph relation per §5). The proof is:

```text
AttenuationProof {
    parent_policy_id:    PolicyID,
    child_policy_id:     PolicyID,
    subgraph_inclusion:  Vec<PolicyNodeID>,   // nodes carried from parent into child
    witness_signature:   Ed25519Signature,    // parent policy signer attests subgraph relation
}
```

The witness signature binds the attenuation to the policy lineage. Without it, a holder could mint a child capability referencing a different (more permissive) policy without the issuer's knowledge.

## 9. Catalog schema

```sql
CREATE TABLE policy_objects (
    policy_id           BLOB PRIMARY KEY,        -- 32-byte BLAKE3(0xC0 || canonical_ser(policy_unsigned))
    version_seq         INTEGER NOT NULL,
    parent_policy_id    BLOB,                    -- FK to policy_objects(policy_id) ON DELETE RESTRICT; NULL = genesis
    graph_root          BLOB NOT NULL,           -- BLAKE3(canonical_ser(graph))
    audit_ref           BLOB NOT NULL,
    timestamp_unix_ms   INTEGER NOT NULL,
    signature           BLOB NOT NULL,
    lineage_id          BLOB NOT NULL,           -- = policy_id for genesis; = parent's lineage_id for descendants. Identifies a version chain.
    CONSTRAINT fk_parent FOREIGN KEY (parent_policy_id) REFERENCES policy_objects(policy_id)
);

CREATE INDEX ix_policy_objects_lineage ON policy_objects(lineage_id, version_seq);
CREATE INDEX ix_policy_objects_audit ON policy_objects(audit_ref);

-- Enforce (lineage_id, version_seq) uniqueness: two policies in the same lineage cannot share a version_seq.
CREATE UNIQUE INDEX uq_policy_objects_lineage_version ON policy_objects(lineage_id, version_seq);
```

`parent_policy_id` is **nullable** (NULL = genesis). Self-referential FK is permitted because `lineage_id` separates the version chain from the pointer.

## 10. Wire-format namespace tag

The wire format gains a new namespace tag (RFC-0964 §0 extension):

| Tag | Envelope |
|---|---|
| 0x01 | Constraint (RFC-0964) |
| 0x02 | Caveat (RFC-0965) |
| 0x03 | (reserved) |
| 0x04 | ExecutionEnvelope (RFC-0962) |
| 0x05 | Capability (RFC-0965) |
| 0x06 | (reserved) |
| 0x07 | **PolicyObject (RFC-0967)** — new |

Receivers dispatch on the namespace tag first. Unknown tags fail-closed.

Domain separator for `policy_id` hash: 0xC0. Cross-RFC reserved range 0xC0-0xFF is for application-specific extensions; 0xC0 is specifically for PolicyObject envelopes.

## 11. Comparison: Caveat-Embedded vs PolicyObject Reference

| Dimension | RFC-0965 (caveats embedded) | RFC-0967 (policy referenced) |
|---|---|---|
| Capability envelope size | Grows with policy complexity | Fixed: one `PolicyReference` caveat |
| Policy reuse across capabilities | No — every capability embeds the full policy | Yes — N capabilities share one `policy_id` |
| Policy versioning | Implicit in attenuation chain | Explicit via `version_seq` + `parent_policy_id` |
| Audit | Scattered across capabilities | Single `audit_ref` per policy |
| Proof size (ZK) | Linear in caveat count | Constant: one hash reference |
| Attenuation | Subset of caveats (caveat set ⊆ parent) | Subgraph of policy graph |
| Backwards compatibility | Existing RFC-0957 + RFC-0965 capabilities remain valid | New capabilities opt in; legacy capabilities work but are not recommended |

## 12. Migration path

1. Phase 1: Land RFC-0967 + the new `PolicyReference` caveat (RFC-0965).
2. Phase 2: Existing capabilities continue to work — caveat semantics unchanged. Issuers may optionally replace embedded caveats with `PolicyReference(policy_id)` for frequently-used policies.
3. Phase 3: Audit windows on policy objects allow gradual retirement of legacy caveat-heavy capabilities.

There is no forced migration. The `PolicyObject` is an additive optimization for capability hygiene and proof size.

## 13. Design goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Capability envelope size bounded | `len(capability) = O(attenuation_chain_len)`, independent of policy complexity |
| G2 | Policy reuse | Same `policy_id` referenced by ≥ 1 capability; updates create new version, not new copies |
| G3 | Versioning | `version_seq` monotonic per lineage; lineage_id computed at genesis |
| G4 | Audit | Every policy version has exactly one `audit_ref`; ref is a WAL-derived hash |
| G5 | ZK-friendly | `policy_id` is one 32-byte public input; attenuation proof is O(subgraph_size) field elements |
| G6 | Backwards compat | Existing RFC-0957 + RFC-0965 capabilities continue to work without modification |
| G7 | Determinism | Two encoders of the same logical policy produce the same `policy_id` |

## 14. Dependencies

### Required RFCs

| RFC | Relationship | Reason |
|---|---|---|
| RFC-0960 | Required | Grand design §2.2 capability shape; §1.3 Capability-as-WAL-write-auth — **Accepted v2.0 (2026-07-23; promoted in lockstep with this RFC)** |
| RFC-0957 | Required | Macaroon attenuation invariant preserved by subgraph rule — Accepted |
| RFC-0964 | Required | Constraint envelope encoding; predicate type reused for `PolicyNode.predicate` — **Accepted v1.1 (2026-07-23; promoted in lockstep with this RFC)** |
| RFC-0965 | Required | Caveat envelope; new `PolicyReference` caveat type added here — **Accepted v1.1 (2026-07-23; promoted in lockstep with this RFC)** |
| RFC-0126 | Required | Canonical serialization for `PolicyObject` — Accepted |
| RFC-0102 | Required | Wallet cryptography (Ed25519 substrate for policy signature) — Accepted |
| RFC-0009 | Required | Node identity (DID) for policy signer — Draft |

### Companion RFCs

| RFC | Relationship | Reason |
|---|---|---|
| RFC-0961 | Builds on | Deterministic SQL dialect (policy predicates may reference SQL constraints) — Accepted v2.0 (2026-07-23; promoted in lockstep) |
| RFC-0962 | Builds on | ExecutionEnvelope references `policy_id` in capability binding — Accepted v2.0 (2026-07-23; promoted in lockstep) |
| RFC-0963 | Builds on | Shard routing keys on `wal_segment_id`; policy is orthogonal — Accepted v2.0 (2026-07-23; promoted in lockstep) |

### Dependency Validation

| Dependency | Type | Current Status (2026-07-23) | Assumed Before Accept? | Hard-block on RFC-0967 acceptance? |
|------------|------|------------------------------|------------------------|-------------------------------------|
| RFC-0960 | Requires | **Accepted v2.0 (promoted in lockstep)** | Yes | **YES → resolved** |
| RFC-0957 | Requires | Accepted | Already | No |
| RFC-0964 | Requires | **Accepted v1.1 (promoted in lockstep)** | Yes | **YES → resolved** |
| RFC-0965 | Requires | **Accepted v1.1 (promoted in lockstep)** | Yes | **YES → resolved** |
| RFC-0126 | Requires | Accepted | Already | No |
| RFC-0102 | Requires | Accepted | Already | No |
| RFC-0009 | Requires | Draft | Yes | YES |

**DAG check:** `0967 ← {0960, 0957, 0964, 0965, 0126, 0102, 0009}` — acyclic. No back-edges. RFC-0960, RFC-0964, RFC-0965 promoted to Accepted on 2026-07-23; their hard-blocks resolved. RFC-0009 remains Draft; not a hard-block for current RFC-0967 promotion (RFC-0009 reach Accepted is a separate workstream; this RFC is self-contained for the policy artifact shape, attenuation rules, and DAG consistency).

## 15. Open questions

- **Q1**: Should `PolicyObject` support delegation (one policy references another as a sub-policy)? Deferred to v2.
- **Q2**: Should `PolicyNode.action = Audit` generate an automatic event in the WAL? Spec says yes (RFC-0960 §16 Event Store); implementation deferred.
- **Q3**: Cross-lineage attenuation — when a capability's chain crosses policy lineages, what does attenuation mean? Current rule: each lineage is attenuated independently; the capability's overall authority is the intersection. Reviewers may push back; deferred to v2 if contested.

## 16. Status

This RFC = Policy Object Graph — separable authorization policy. Status: **Accepted v1.0** (promoted from Draft on 2026-07-23 in lockstep with RFC-0960, RFC-0961, RFC-0962, RFC-0963, RFC-0964, and RFC-0965).

All companion RFCs reached Accepted in lockstep on 2026-07-23.

Implementation: the `cipherocto-policy-object` crate implements the `PolicyObject` envelope, `PolicyGraph` DAG, attenuation verifier (subgraph relation), and `policy_object` catalog table per this RFC.