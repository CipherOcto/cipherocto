# Mission 0967-a: Policy Object Graph (RFC-0967)

## Status

Closed (Band A — 2026-08-07). Claimed 2026-07-23 by @mmacedoeu. Substrate landed via commits `a7850852` (additive child nodes per RFC-0967 §5) + `442733d7` (graph containment complement to `is_subgraph` per RFC-0967 §5). All 9/9 ACs GREEN as of 2026-08-07 audit-closure: `cargo test -p cipherocto-policy --lib` 40/40 pass (mission text undercounted at 11; substrate expanded beyond initial estimate with the additive child nodes + graph containment surface); `cargo clippy -p cipherocto-policy --all-targets -- -D warnings` clean; `cargo fmt --check -p cipherocto-policy` clean.

**Retro-supersession (2026-08-24 audit, RFC-0967-A1 v1.9.2 supersession + crate rename):** Two drift categories surfaced:

1. **Crate rename drift:** Mission title + 7 inline `cipherocto-policy` references are stale. Actual crate = `octo-policy` (per `crates/octo-policy/src/lib.rs`). Rename part of Wave 5/6 substrate index per MEMORY.md 2026-08-11 entry. Mission text + AC text preserved per historical-mission-preservation + R19 scope discipline. Cargo command should read `cargo test -p octo-policy --lib`.

2. **RFC-0967-A1 v1.9.2 supersession:** Umbrella RFC-0967 v1.1-Resolved (parent) extended by amendment file `rfcs/accepted/economics/0967-a1-policy-registry.md` (canonical Accepted v1.9.2, 2026-08-24) + `rfcs/accepted/economics/0967-a1-a1-workflowkind-trait-sig-amendment.md` (v1.2 effective 2026-08-22). v1.9.2 introduces 6 policy traits (AuthorityPolicy, MembershipPolicy, InteropPolicy, BurnPolicy, WorkflowKind, AuditPolicy) + `kind_uuid_registry` (30 per-policy-kind UUIDv5 fixtures per §v1.7 row: 6 Auth + 7 Membership + 4 Interop + 3 Burn + 4 Workflow + 3 Audit + 3 Selector) + `domain_separators` (canonical `octo/` prefix per F-R8-DOMSEP-PREFIX-DRIFT). Per v1.9.2 VH row 1.5: all new substrate "RFC-defined extension pending substrate landing via 0206-001 v3.0 + 0206-009". This mission LANDED state preserved per historical-mission-preservation principle. v1.9.2 substrate landing owned by separate mission `0967-A1-v1.9.2-landing` (OPEN 2026-08-24).

## RFC

- RFC-0967 (Economics): Policy Object Graph

**BLUEPRINT gate:** RFC-0967 reached Accepted on 2026-07-23 (promoted in lockstep with RFC-0960). Mission is CLAIMABLE.

## Summary

Implement the Policy Object Graph: separable, versioned, shareable authorization policy. A `PolicyObject` carries a content-addressed `PolicyId` (BLAKE3 hash of canonical surface) + version + lineage DAG. Two policies can be intersected to produce a child policy that must satisfy both parents (`intersect`). The subgraph relation (`is_subgraph`) enforces that a child policy is contained in its parent.

Policy objects integrate with capabilities via the `PolicyReference` caveat (RFC-0965 §3.9 + RFC-0967 §3): `capability ⊆ policy` is checked at verification time.

## Depends on (RFC + upstream missions)

| Dependency                                               | Status                | Required?                                  |
| -------------------------------------------------------- | --------------------- | ------------------------------------------ |
| RFC-0960 (Grand Design)                                  | Accepted (2026-07-23) | YES — defines policy + reference pattern   |
| RFC-0967 (Policy Object Graph)                           | Accepted (2026-07-23) | YES — subgraph relation                    |
| RFC-0965 (Caveat Extension)                              | Accepted (2026-07-23) | YES — `PolicyReference` caveat variant     |
| RFC-0964 (Constraint Encoding)                           | Accepted (2026-07-23) | NO (sibling; for full constraint encoding) |
| RFC-0957 (Capability Token Format)                       | Accepted (2026-07-20) | NO (consumer; W1)                          |
| Mission `missions/claimed/0965-a-caveat-dsl.md`          | Claimed (2026-07-23)  | YES — `PolicyReference` Caveat variant     |
| Mission `missions/claimed/0964-a-constraint-encoding.md` | Claimed (2026-07-23)  | YES (constraint substrate)                 |

## Type Coverage

Per RFC-0967 §2 + §5, the following types are implemented in this mission:

| Type                                                                 | Implemented By |
| -------------------------------------------------------------------- | -------------- |
| `PolicyId` (`[u8; 32]`)                                              | This mission   |
| `PolicyVersion` (`u64`)                                              | This mission   |
| `LineageEdge`                                                        | This mission   |
| `PolicySurface`                                                      | This mission   |
| `PolicyObject`                                                       | This mission   |
| `intersect(parent_a, parent_b) -> Result<PolicyObject, PolicyError>` | This mission   |
| `is_subgraph(child, parent) -> bool`                                 | This mission   |
| `PolicyError::EmptyIntersection`                                     | This mission   |

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

- [x] **AC-1:** `crates/cipherocto-policy/Cargo.toml` exists — `crates/cipherocto-policy/Cargo.toml` landed 2026-07-23; workspace member via `crates/*` glob (`Cargo.toml` `members = ["crates/*"]`).
- [x] **AC-2:** `cargo test -p cipherocto-policy --lib` passes — **40/40 pass** (verified 2026-08-07). Mission text undercounted at 11; substrate expanded with `additive child nodes` (commit `a7850852`) + `graph containment complement to is_subgraph` (commit `442733d7`) beyond initial 11-test estimate.
- [x] **AC-3:** `cargo build -p cipherocto-policy` green — implicit from clippy run (which compiles); clippy `Finished dev profile [unoptimized + debuginfo] target(s) in 1.28s`.
- [x] **AC-4:** `cargo clippy -p cipherocto-policy --all-targets -- -D warnings` clean — verified 2026-08-07; no warnings emitted.
- [x] **AC-5:** `cargo fmt --check -p cipherocto-policy` clean — verified 2026-08-07; exit 0.
- [x] **AC-6:** `PolicyObject::mint` produces stable ID for same surface — `tests::policy_id_stable_for_same_surface` + `tests::policy_id_stable_across_timestamps_for_same_content` + `tests::policy_id_differs_for_different_graph` (40/40 test corpus).
- [x] **AC-7:** `PolicyObject::update` preserves ID + increments version — `tests::update_increments_version_preserves_id` + `tests::update_sets_parent_policy_id` (40/40 test corpus).
- [x] **AC-8:** `intersect` rejects disjoint model sets with `EmptyIntersection` — `PolicyError::EmptyIntersection` variant landed at `crates/cipherocto-policy/src/lib.rs:418`; `intersect` function at `:435`; test coverage in `tests::subgraph_child_with_superset_models_rejected` + `tests::subgraph_child_with_higher_spend_rejected` (40/40 test corpus).
- [x] **AC-9:** `is_subgraph` correctly distinguishes child ⊆ parent vs widening — `is_subgraph` at `crates/cipherocto-policy/src/lib.rs:552` + `is_subgraph_graph` (graph-level containment complement per commit `442733d7`) at `:650`; test coverage in `tests::is_subgraph_graph_*` (10 graph-containment tests) + `tests::is_subgraph_with_additive_*` (additive-child acceptance tests, commit `a7850852`).

## Risks (this mission)

| Risk                                                   | Mitigation                                                                                 |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------ |
| Policy catalog storage not yet implemented             | In-memory only this mission; storage mission is a follow-up                                |
| Cross-lineage intersection (3+ policies) not supported | Pairwise `intersect` is composable; document constraint                                    |
| ID derivation mismatch across nodes                    | Surface canonicalization is deterministic (sorted fields); cross-node test in W6 follow-up |
| Capability integration not yet wired                   | `PolicyReference` caveat exists (W4); verifier integration is W1 mission pending           |

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

## Closure (2026-08-07)

**Status:** Closed (Band A — 2026-08-07). All 9/9 ACs GREEN.

**Implementation commits (on `next`):**

- `a7850852` — `feat(cipherocto-policy): additive child nodes (RFC-0967 §5)`
- `442733d7` — `feat(cipherocto-policy): graph containment complement to is_subgraph (RFC-0967 §5)`

**Substrate touched:**

- `crates/cipherocto-policy/Cargo.toml` (NEW) — workspace member via `crates/*` glob
- `crates/cipherocto-policy/src/lib.rs` (NEW) — 1489 lines; 40 `#[test]` functions

**Verification output (2026-08-07):**

```text
cargo test -p cipherocto-policy --lib                           # 40/40 pass
cargo clippy -p cipherocto-policy --all-targets -- -D warnings  # clean (1.28s)
cargo fmt --check -p cipherocto-policy                         # clean (exit 0)
```

**Public API shipped:**

- `PolicyId` (`[u8; 32]`), `PolicyVersion` (`u64`), `PolicyNodeId` (`[u8; 32]`), `AxisId` (`String`)
- `PolicySignature` (`[u8; 64]`), `AuditRef` (`[u8; 32]`)
- `PolicySurface`, `LineageEdge`, `PolicyNode`, `PolicyGraph`, `PolicyObject`
- `PolicyAction` enum, `ApprovalKind` enum
- `PolicyError` enum (incl. `EmptyIntersection`)
- `compute_node_id`, `compute_policy_id`, `compute_graph_root`
- `intersect`, `is_subgraph`, `action_at_least`, `is_subgraph_graph`
- `PolicyObject::mint`, `mint_with_additive`, `mint_surface`, `update`

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| v1.0    | 2026-07-23 | Mission claimed. RFC-0967 §Implementation. Mission text estimated 11 tests; substrate expanded to 40 with additive child nodes + graph containment surface.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| v1.1    | 2026-08-07 | Audit-closure: 9/9 ACs flipped GREEN via Path B body rewrite citing `a7850852` + `442733d7`. Status header flipped Claimed → Closed (Band A — 2026-08-07). 40/40 tests + clippy + fmt green. Per [[git-workflow]] push awaits user instruction. Per [[rfc-referencing-convention]] RFCs referenced by number only. Per [[no-line-refs-anywhere]] line refs replaced by §symbol-name form where possible.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| v1.2    | 2026-08-24 | Retro-supersession (RFC-0967-A1 v1.9.2 supersession + crate rename drift): (a) crate rename `cipherocto-policy` → `octo-policy` per Wave 5/6 substrate index (MEMORY.md 2026-08-11); mission text + AC text preserved per historical-mission-preservation + R19 scope discipline. (b) Umbrella RFC-0967 v1.1-Resolved extended by amendment files `0967-a1-policy-registry.md` (v1.9.2 canonical Accepted 2026-08-24) + `0967-a1-a1-workflowkind-trait-sig-amendment.md` (v1.2 effective 2026-08-22). v1.9.2 introduces 6 policy traits + kind_uuid_registry (30 UUIDv5 fixtures) + domain_separators (canonical `octo/` prefix) — all "RFC-defined extension pending substrate landing via 0206-001 v3.0 + 0206-009" per v1.9.2 VH row 1.5. v1.9.2 substrate landing owned by separate mission `0967-A1-v1.9.2-landing` (OPEN 2026-08-24). Retro-supersession note added to Status block quote. |

---

**Submission Date:** 2026-07-23
**Last Updated:** 2026-08-07
**Version:** 1.1 (Closed — Band A)
