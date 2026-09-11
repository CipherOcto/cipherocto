---
name: 0013-governance-substrate-extraction
description: Extract canonical `ProposalState` + `DecisionType` + `GovernanceModel` + tally helpers to `octo-governance-core` Layer A frozen crate + create `octo-governance` Layer B façade per RFC-0013
metadata:
  node_type: substrate-core
  type: layer-a-frozen-extraction
  originSessionId: RFC-0013 author session
  created: 2026-09-10
  v: "1.0"
  completed: 2026-09-10
  commit: 3aff437d
  fix_commit: a2932181
  depends_on:
    - RFC-0013
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0013-governance-substrate-extraction — `octo-governance-core` + `octo-governance` per RFC-0013

**Status:** Completed — substrate extraction landed at commit `3aff437d` on `next`. R2 DRY review fix at `a2932181` (drop dead `GovernanceError::Caller` variant + semantic `QuorumNotReached` fix for total-vote-exceeds-100k). Two new crates (`octo-governance-core` Layer A frozen IO-free + `octo-governance` Layer B façade) + 8 lib unit tests pass + clippy clean. `repr(u16)` discriminants byte-identical to RFC-0855 §11.1-11.3. `BTreeMap`-keyed `tally_quorum` for cross-replica determinism per RFC-0013 §Cross-Replica Tally Equivalence. DRY review R1=2 LOW → R2 fixes → R3=0 → DRY CLOSED per `docs/audits/2026-09-10-0012-0013-0014-substrate-extraction-r3-dry-closure.md`. Push + PR + RFC VH row append user-owned per [[feedback_initiation_user_only]] + [[git-workflow]].
**Substrate:** RFC-0013 §Specification (`octo-governance-core` + `octo-governance` modules)
**Parent:** RFC-0013

## Scope

Per RFC-0013 §Key Files to Modify Phase 1 — substrate extraction. The substrate is intentionally IO-free: proposal persistence, snapshot refresh, attest/vote IO all live in domain crates (primarily `octo-network/mon/governance.rs`). The core owns canonical types + pure tally helpers; the façade re-exports ONLY from the core.

### Deliverables

1. **`crates/octo-governance-core/` (NEW)** — Layer A frozen core.
   - `src/lib.rs` — module re-exports + crate docs (RFC-0013 §Module Layout)
   - `src/policy.rs` — `GovernancePolicy` struct + `GovernanceModel` enum (5 variants: Centralized, Dao, Federated, AiAssisted, Autonomous) + `EmergencyAuthority` enum (3 variants)
   - `src/proposal.rs` — `GovernanceProposal` struct + `ProposalState` enum (6 variants: Created, Voting, Approved, Rejected, Executed, Expired) + `DecisionType` enum (7 variants: Admission, RoleAssignment, TopologyChange, MissionTermination, PolicyModification, EmergencyRekey, ParticipantExpulsion)
   - `src/tally.rs` — `voting_weight` + `tally_quorum` pure helpers (no IO; deterministic across replicas)
   - `src/error.rs` — `GovernanceError` enum (InvalidTransition, QuorumNotReached, etc.)
   - `Cargo.toml` — deps: `serde`, `thiserror` only (NO IO deps; NO storage deps)
2. **`crates/octo-governance/` (NEW)** — Layer B façade (~25 LoC).
   - `src/lib.rs` — `pub use octo_governance_core::*` + re-export domain extension enums from `octo-network::mon::governance`
   - `Cargo.toml` — depends on `octo-governance-core` only
3. **Workspace registration** — add both crates to root `Cargo.toml` `members` list
4. **Byte-identical extraction claim** — `octo-network/mon/governance.rs` canonical type discriminants (`repr(u16)`) preserved per RFC-0855 §11

### Acceptance criteria

- [ ] AC-1: `cargo build -p octo-governance-core` succeeds with zero warnings (clippy `--all-features -- -D warnings`)
- [ ] AC-2: `cargo build -p octo-governance` succeeds with zero warnings
- [ ] AC-3: All 6 §Module Layout sections present in `octo-governance-core/src/lib.rs`
- [ ] AC-4: `GovernanceModel` (5 variants) + `DecisionType` (7 variants) + `ProposalState` (6 variants) + `EmergencyAuthority` (3 variants) all `#[non_exhaustive]` per CLAUDE.md §Extension over enumeration
- [ ] AC-5: `repr(u16)` discriminants byte-identical to RFC-0855 §11
- [ ] AC-6: `voting_weight` + `tally_quorum` are pure functions (no IO, no clock, no randomness)
- [ ] AC-7: Workspace `cargo build --workspace` succeeds after registration
- [ ] AC-8: RFC-0013 VH row appended documenting substrate extraction + cite-hygiene PASS

### Out of scope (separate missions)

- Migration of `octo-network/mon/governance.rs` to use substrate → `missions/open/0013-governance-network-migration.md`
- 10 tally test vectors → `missions/open/0013-governance-tally-tests.md`
- IO functions (`snapshot`, `attest`, `vote`) → RFC-0011-g amendment chain (separate concern; CLI-side projection)

### Dependencies

- `RFC-0013` (accepted 2026-09-10) — canonical substrate spec
- `RFC-0855` §11 — source-of-truth for canonical `ProposalState` + `DecisionType` + `GovernanceModel` variant discriminants

### Risk

- **HIGH** — Layer A frozen core addition. Per CLAUDE.md §Architectural Principles + RFC-0013 §Security Considerations, the core MUST be RFC-frozen + semver-major only. Mitigation: explicit `Cargo.toml` comment pinning the frozen-core status + CLAUDE.md cross-link.
- **HIGH** — `repr(u16)` discriminant drift. SQL persistence stores discriminants as INTEGER; reordering breaks migration. Mitigation: AC-5 byte-identical assertion test.
- **MEDIUM** — Tally helpers drift between replicas. Mitigation: `BTreeMap` for vote tallies (ordered iteration per RFC-0013 §Cross-Replica Tally Equivalence) + 10 test vectors in `0013-governance-tally-tests.md`.

### Cross-RFC invariants preserved

- `ProposalState` discriminants byte-identical to RFC-0855 §11.3 (Created=0, Voting=1, Approved=2, Rejected=3, Executed=4, Expired=5)
- `DecisionType` discriminants byte-identical to RFC-0855 §11.3 (Admission=0, RoleAssignment=1, TopologyChange=2, MissionTermination=3, PolicyModification=4, EmergencyRekey=5, ParticipantExpulsion=6)
- `GovernanceModel` discriminants byte-identical to RFC-0855 §11.1 (Centralized=0, Dao=1, Federated=2, AiAssisted=3, Autonomous=4)
- PQC migration blast radius confined to Layer A frozen core

### Test vectors (substrate-level, RFC-0013 §Test Vectors 10 vectors)

See `missions/open/0013-governance-tally-tests.md` for the canonical 10 test vectors.
