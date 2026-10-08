---
name: 0013-governance-network-migration
description: Migrate `octo-network/mon/governance.rs` to consume `octo-governance-core` canonical types + preserve IO functions (snapshot/attest/vote) per RFC-0013 §Key Files to Modify Phase 2
metadata:
  node_type: substrate-consumer
  type: domain-migration
  originSessionId: RFC-0013 author session
  created: 2026-09-10
  v: "1.0"
  depends_on:
    - RFC-0013
    - mission 0013-governance-substrate-extraction
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0013-governance-network-migration — `octo-network/mon/governance.rs` substrate migration

**Status:** Claimed — domain migration to substrate (preserves IO functions)
**Substrate:** RFC-0013 §Key Files to Modify Phase 2 (`octo-network/src/mon/governance.rs`)
**Parent:** RFC-0013

## Scope

Per RFC-0013 §Key Files to Modify SUBSTRATE row 2, the canonical `ProposalState` + `DecisionType` + `GovernanceModel` + `GovernancePolicy` + `GovernanceProposal` types live in `octo-governance-core` (Layer A frozen). The domain `octo-network/mon/governance.rs` re-exports the canonical types via `pub use octo_governance_core::*` and KEEPS its IO functions (`snapshot`, `attest`, `vote`) per RFC-0013 §Substrate `[ADD]` — substrate is intentionally IO-free; domain owns IO.

### Deliverables

1. **`crates/octo-network/src/mon/governance.rs`** — replace local `ProposalState` + `DecisionType` + `GovernanceModel` + `EmergencyAuthority` enums with `pub use octo_governance_core::{ProposalState, DecisionType, GovernanceModel, EmergencyAuthority, GovernancePolicy, GovernanceProposal, voting_weight, tally_quorum}`. `repr(u16)` discriminants preserved byte-identically.
2. **IO functions preserved** — `snapshot`, `attest`, `vote` functions stay in `octo-network/mon/governance.rs` (domain-owned IO). They consume substrate canonical types via the `pub use` chain.
3. **Discriminant byte-identical test** — `cargo test -p octo-network mon::governance::tests::discriminants_byte_identical_to_rfc_0855_11` asserts the migrated discriminants match the canonical substrate spec.
4. **`StoolapGovernanceStore` (NEW)** — substrate-storage adapter for `GovernanceProposal` persistence; uses `Arc<Mutex<Database>>` interior mutability pattern (matches `StoolapStore` precedent in `quota-router-sm-engine`).

### Acceptance criteria

- [ ] AC-1: `cargo build -p octo-network` succeeds with zero warnings
- [ ] AC-2: `crates/octo-network/src/mon/governance.rs` no longer defines local `ProposalState` / `DecisionType` / `GovernanceModel` / `EmergencyAuthority` enums; uses `pub use octo_governance_core::*`
- [ ] AC-3: IO functions (`snapshot`, `attest`, `vote`) preserved in domain crate
- [ ] AC-4: Discriminant byte-identical test added and PASSES (`repr(u16)` value matches canonical substrate)
- [ ] AC-5: `StoolapGovernanceStore` impl lives in domain crate (storage adapter is domain-owned; substrate owns canonical types only)
- [ ] AC-6: Workspace `cargo build --workspace` succeeds
- [ ] AC-7: Workspace `cargo test -p octo-network --lib mon::governance::` passes (existing tests still green post-migration)
- [ ] AC-8: RFC-0013 VH row appended documenting network migration

### Dependencies

- `RFC-0013` — canonical substrate spec
- `mission 0013-governance-substrate-extraction` — must complete first (this mission consumes the substrate)
- `RFC-0855` §11 — source-of-truth for canonical variant discriminants
- `octo-network/mon` Layer D platform adapters (depend on governance substrate)

### Risk

- **HIGH** — Domain migration can break 3+ downstream consumer crates (`octo-coordinator-types`, `octo-reputation`, `octo-mesh`) if `pub use` chain breaks. Mitigation: workspace `cargo test --workspace` after migration; the `pub use` re-export preserves every public path.
- **MEDIUM** — `repr(u16)` discriminant drift. SQL persistence stores discriminants as INTEGER. Mitigation: AC-4 byte-identical assertion test + RFC-0013 §Compatibility §RFC-0855 §11 compatibility row.
- **LOW** — IO function drift if domain accidentally moves `snapshot`/`attest`/`vote` to substrate. Mitigation: AC-3 + module-level doc-comment in `octo-governance-core/src/lib.rs` declaring "substrate is intentionally IO-free".

### Cross-RFC invariants preserved

- `ProposalState` / `DecisionType` / `GovernanceModel` discriminants byte-identical to RFC-0855 §11
- `voting_weight` + `tally_quorum` pure helpers in substrate (no IO)
- IO functions (`snapshot`, `attest`, `vote`) in domain crate

### Test vectors (domain-level)

| ID | Scenario | Expected |
|----|----------|----------|
| `network-discriminant-invariant` | Migrated `ProposalState` discriminant vs canonical substrate | byte-identical (`repr(u16)` value matches) |
| `network-snapshot-preserved` | `mon::governance::snapshot` function exists + signature unchanged | succeeds (signature: `(policy, current_epoch) -> Result<Snapshot, GovernanceError>`) |
| `network-attest-preserved` | `mon::governance::attest` function exists + signature unchanged | succeeds (signature: `(subject_did, kind, snapshot_id) -> Result<AttestationReceipt, GovernanceError>`) |
| `network-vote-preserved` | `mon::governance::vote` function exists + signature unchanged | succeeds (signature: `(proposal_id, voter_did, choice, weight) -> Result<VoteReceipt, GovernanceError>`) |
| `network-btreemap-vote-tally` | Vote tally iteration order | `BTreeMap`-ordered (deterministic across replicas per RFC-0013 §Cross-Replica Tally Equivalence) |
