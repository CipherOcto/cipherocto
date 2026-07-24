# Mission 0967-a: Policy Object Graph (RFC-0967)

## Status

Claimed (2026-07-23)

## RFC

- RFC-0967 (Economics): Policy Object Graph

**BLUEPRINT gate:** RFC-0967 reached Accepted on 2026-07-23 (promoted in lockstep with RFC-0960). Mission is CLAIMABLE.

## Summary

Implement the Policy Object Graph: separable, versioned, shareable authorization policy. A `PolicyObject` carries a content-addressed `PolicyId` (BLAKE3 hash of canonical surface) + version + lineage DAG. Two policies can be intersected to produce a child policy that must satisfy both parents (`intersect`). The subgraph relation (`is_subgraph`) enforces that a child policy is contained in its parent.

Policy objects integrate with capabilities via the `PolicyReference` caveat (RFC-0965 §3.9 + RFC-0967 §3): `capability ⊆ policy` is checked at verification time.

## Depends on (RFC + upstream missions)

| Dependency | Status | Required? |
|------------|--------|-----------|
| RFC-0960 (Grand Design) | Accepted (2026-07-23) | YES — defines policy + reference pattern |
| RFC-0967 (Policy Object Graph) | Accepted (2026-07-23) | YES — subgraph relation |
| RFC-0965 (Caveat Extension) | Accepted (2026-07-23) | YES — `PolicyReference` caveat variant |
| RFC-0964 (Constraint Encoding) | Accepted (2026-07-23) | NO (sibling; for full constraint encoding) |
| RFC-0957 (Capability Token Format) | Accepted (2026-07-20) | NO (consumer; W1) |
| Mission `missions/claimed/0965-a-caveat-dsl.md` | Claimed (2026-07-23) | YES — `PolicyReference` Caveat variant |
| Mission `missions/claimed/0964-a-constraint-encoding.md` | Claimed (2026-07-23) | YES (constraint substrate) |

## Type Coverage

Per RFC-0967 §2 + §5, the following types are implemented in this mission:

| Type | Implemented By |
|------|----------------|
| `PolicyId` (`[u8; 32]`) | This mission |
| `PolicyVersion` (`u64`) | This mission |
| `LineageEdge` | This mission |
| `PolicySurface` | This mission |
| `PolicyObject` | This mission |
| `intersect(parent_a, parent_b) -> Result<PolicyObject, PolicyError>` | This mission |
| `is_subgraph(child, parent) -> bool` | This mission |
| `PolicyError::EmptyIntersection` | This mission |

## In Scope

1. **`crates/cipherocto-policy/`** — new workspace member under `crates/*` glob.
2. **`PolicyObject` mint + update** — version 1 minted; subsequent updates bump version + record lineage.
3. **`intersect` function** — pairwise intersection with empty-surface detection.
4. **`is_subgraph` function** — child ⊆ parent containment check.
5. **Content-addressed policy ID** — BLAKE3 hash of canonical surface (sorted fields, deterministic).
6. **Tests** — 11 unit tests cover stable IDs, intersection + subgraph rules, lineage tracking.

## Out of Scope (this mission only)

- Persisted policy catalog (DB table) → future mission; for now, in-memory `PolicyObject` only.
- Cross-lineage intersection (3+ policies) → pairwise `intersect`; multi-way is a follow-up.
- ZK subclass `PolicyGraph` proof → RFC-0958 (W1 sub-bullet), if relevant.
- Verification-time policy lookup at capability redeem → W1 mission pending.

## Implementation Guide

**File:** `crates/cipherocto-policy/src/lib.rs`

**Public API:**
```rust
pub type PolicyId = [u8; 32];
pub type PolicyVersion = u64;
pub struct PolicySurface { /* allowed_models, allowed_providers, per_axis_caps, max_total_spend, audit_window_secs, allowed_destinations */ }
pub struct PolicyObject { /* policy_id, version, surface, lineage */ }
pub enum PolicyError { EmptyIntersection }
pub fn intersect(parent_a: &PolicyObject, parent_b: &PolicyObject) -> Result<PolicyObject, PolicyError>;
pub fn is_subgraph(child: &PolicyObject, parent: &PolicyObject) -> bool;
```

**Policy ID derivation:** `BLAKE3("policy/v1" || fields_sorted)`. Same surface → same ID across nodes.

**Policy update:** preserves ID, increments version, records lineage edge to parent version.

**Intersection rules:**
- `allowed_models`: set intersection (HashSet.intersection); empty result → `EmptyIntersection`
- `allowed_providers`: same
- `allowed_destinations`: same
- `max_total_spend`: min of both
- `per_axis_caps`: per axis, min(a, b); drop axes asymmetric to either parent
- `audit_window_secs`: max of both (refinement: longer audit window)

## Acceptance Criteria

- [ ] **AC-1:** `crates/cipherocto-policy/Cargo.toml` exists
- [ ] **AC-2:** `cargo test -p cipherocto-policy --lib` passes (11 tests)
- [ ] **AC-3:** `cargo build -p cipherocto-policy` green
- [ ] **AC-4:** `cargo clippy -p cipherocto-policy --all-targets -- -D warnings` clean
- [ ] **AC-5:** `cargo fmt -p cipherocto-policy --check` clean
- [ ] **AC-6:** `PolicyObject::mint` produces stable ID for same surface
- [ ] **AC-7:** `PolicyObject::update` preserves ID + increments version
- [ ] **AC-8:** `intersect` rejects disjoint model sets with `EmptyIntersection`
- [ ] **AC-9:** `is_subgraph` correctly distinguishes child ⊆ parent vs widening

## Risks (this mission)

| Risk | Mitigation |
|------|------------|
| Policy catalog storage not yet implemented | In-memory only this mission; storage mission is a follow-up |
| Cross-lineage intersection (3+ policies) not supported | Pairwise `intersect` is composable; document constraint |
| ID derivation mismatch across nodes | Surface canonicalization is deterministic (sorted fields); cross-node test in W6 follow-up |
| Capability integration not yet wired | `PolicyReference` caveat exists (W4); verifier integration is W1 mission pending |

## Notes

### Hierarchical lattice integration

Per RFC-0960 §8 + RFC-0967 §5: hierarchical delegation uses `PolicyGraph` subgraph relation. parent policy ⊇ child policy iff child ⊆ parent in the DAG. This mission implements the pairwise relation; multi-node lineage is built by chaining `update` calls.

### Cross-RFC alignment

- RFC-0967 §5 subgraph relation: this mission's `is_subgraph` follows RFC-0967 §5 verbatim.
- RFC-0960 §8 hierarchical vaults: capability attenuation chain + `WrappedOnly` + `PolicyGraph` together form the lattice. RFC-0957 §Attenuation + RFC-0965 §3.7 `WrappedOnly` + RFC-0967 §5 `PolicyGraph` are the three layers.
- RFC-0965 §3.9 `PolicyReference` caveat: borsh-serialized `policy_id` (32-byte hash). Verifier fetches the policy and checks `capability ⊆ policy`.

### Future work

- Persisted policy catalog (sm-engine migration or new table).
- Cross-lineage intersection (3+ policies).
- Policy ZK proof (RFC-0958 subclass).
- `policy_catalog` table schema (RFC-0967 §8).

---

**Submission Date:** 2026-07-23
**Last Updated:** 2026-07-23
**Version:** 1.0 (Claimed)
