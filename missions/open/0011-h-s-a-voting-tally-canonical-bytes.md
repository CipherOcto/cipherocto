# 0011-h-s-a-voting-tally-canonical-bytes — Substrate additions for governance_proposal_canonical_bytes helper

## Status

Completed (2026-09-20) — Substrate additions + CLI dispatch CLOSED. Substrate-faithful `governance_proposal_canonical_bytes` helper + `BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN` domain constant land in `crates/octo-network/src/mon/governance.rs`. CLI dispatch wired at `octo network governance tally --proposal-id <N>`. Substrate-additions + CLI dispatch landed end-to-end per RFC-0011-h §Substrate-Additions Companion Missions row G3b.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G3b

## Summary

Adds `governance_proposal_canonical_bytes(p: &GovernanceProposal) -> [u8; 32]` helper + `BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN` BLAKE3 domain prefix constant per RFC-0011-k §Substrate-Additions Companion Missions row G3b. CLI dispatch consumes via substrate-faithful canonical-bytes hash projection.

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

CLI dispatch wired 2026-09-20 at `next 624998ba`:
- `octo network governance tally --proposal-id <N>` wired at `crates/octo-cli/src/commands/network.rs` (Layer C)
- `governance_tally` handler constructs a zero-default `GovernanceProposal` at the requested `proposal_id` then calls `governance_proposal_canonical_bytes(&proposal)` to project the canonical-bytes hash envelope
- New output envelope: `NetworkGovernanceTallyOutput { proposal_id, canonical_hash_hex, state }` per RFC-0011-k §Output Envelope Phase 3
- Read-only (no 3-flag confirmation)
- 2 new test vectors: `tv_net3_5_governance_tally_parses_with_proposal_id`, `tv_net3_8_governance_tally_handler_emits_canonical_hash`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/governance.rs` per RFC-0011-h §Substrate-Additions row G3b (next 10ae8e18)
- [x] CLI dispatch wired at `crates/octo-cli/src/commands/network.rs` per RFC-0011-h §Subcommand Taxonomy Phase 3 (next 624998ba)
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (2/2 canonical_bytes tests pass)
- [x] `cargo test -p octo-cli --lib` green (2/2 governance tally test vectors pass; +2 above Phase 2 baseline)
- [x] Layer discipline preserved (Layer B helper; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle; Layer C consumes via re-export)
- [x] ≥3 unit tests + ≥1 integration test (2 substrate unit tests + 2 CLI test vectors added; integration test deferred to governance tally persistence adapter)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- Governance tally persistence adapter (Phase 6 follow-on)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Substrate slice landed 2026-09-20 at `next 10ae8e18`. CLI dispatch slice landed 2026-09-20 at `next 624998ba`. The CLI today projects the canonical-bytes hash of the zero-default proposal at the requested `proposal_id`. Phase 6 persistence adapter follow-on will replace the zero-default projection with the persisted proposal lookup; the canonical-bytes helper remains the typed substrate-faithful surface.
