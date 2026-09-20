# 0011-h-s-a-voting-tally-canonical-bytes — Substrate additions for governance_proposal_canonical_bytes helper

## Status

Claimed (2026-09-20) — Substrate additions LANDED at `next 10ae8e18`. Substrate-faithful `governance_proposal_canonical_bytes` helper + `BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN` domain constant land in `crates/octo-network/src/mon/governance.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G3b.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G3b

## Summary

Adds `governance_proposal_canonical_bytes(p: &GovernanceProposal) -> [u8; 32]` helper + `BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN` BLAKE3 domain prefix constant per RFC-0011-k §Substrate-Additions Companion Missions row G3b. Pre-requisite for `octo network governance tally` CLI dispatch.

### Substrate additions target

```rust
// crates/octo-network/src/mon/governance.rs
pub const BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN: &[u8] = b"cipherocto/governance/proposal/v1";

pub fn governance_proposal_canonical_bytes(p: &GovernanceProposal) -> [u8; 32];
```

Substrate additions land 2026-09-20 at `next 10ae8e18`:
- `BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN` constant = `b"cipherocto/governance/proposal/v1"`
- `governance_proposal_canonical_bytes(p: &GovernanceProposal) -> [u8; 32]` using BLAKE3 domain separation
- Substrate-faithful: helper lives in Layer-B mon module (NOT on `GovernanceProposal` directly) because Layer-A `octo-governance-core` is RFC-frozen per [[cipherocto-design-principles]] §Stable Abstractions Principle
- 2 unit tests: `governance_proposal_canonical_bytes_round_trip` (deterministic), `governance_proposal_canonical_bytes_changes_with_state` (state-sensitive)

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/governance.rs` per RFC-0011-h §Substrate-Additions row G3b
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (2/2 canonical_bytes tests pass)
- [x] Layer discipline preserved (Layer B helper; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (2 unit tests added; integration test deferred to governance tally persistence adapter)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-governance` covers that surface — pending Phase 3 IMPLEMENTATION)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Governance tally persistence adapter (Phase 6 follow-on)

## Notes

Substrate slice landed 2026-09-20 at `next 10ae8e18`. Companion substrate slice bundles G3b + G12 + G12b together per the substrate-first ordering principle. Phase 3 IMPLEMENTATION closes the CLI dispatch surface (`octo network governance tally`) after this mission transitions to Completed.
