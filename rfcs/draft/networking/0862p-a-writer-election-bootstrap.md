# RFC-0862p-a — Writer Election Bootstrap (patch-targeted cite disambiguator)

**Status:** Draft (2026-09-18) — patch-split stub created per RFC-0011-h §Substrate-Faithful disambiguator convention; v1.4.0 amendment content remains merged in base RFC-0862 (`rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`); this file documents the patch-targeted cite surface.

> **Disambiguator convention:** RFC-0011-h references patch-targeted cites with `RFC-0862p-a` to disambiguate from base `RFC-0862` (stoolap-data-sync surface). Bare `RFC-0862` continues to refer to base + merged amendments. The `p-a` suffix follows the RFC-0855p-a/b/c precedent (Layer B writer-election amendment chain).

## Relationship to base

This RFC exists ONLY as a disambiguator stub. The substantive specification is base RFC-0862 (v1.4.0 amendment merged 2026-08-11, including `WriterElection` protocol + `RaftLikeWriterElection` impl + `BootstrapOrchestrator`-driven peer discovery + CRDT-extension hooks F12 + F13). Patch-specific content is NOT duplicated here; see base for full spec.

## Scope (patch-targeted cites)

| Cite target | Cross-ref |
|---|---|
| `WriterElection` struct + `state() → ElectionState` + `cast_vote(candidate_id, weight)` | base RFC-0862 §Writer Election |
| `RaftLikeWriterElection` impl | base RFC-0862 §RaftLike Writer Election |
| `BootstrapOrchestrator` struct + `start_bootstrap(BootstrapConfig) → BootstrapOutcome` + `status() → BootstrapState` + `BootstrapConfig::{from_toml, save_toml}` | base RFC-0862 §Bootstrap Orchestrator |
| `SeedListAuthority::rotate_post_fork(new_authority, governance_quorum_proof) → Result<SeedListAuthority, SeedAuthorityError>` | base RFC-0862 §Authority Rotation |
| `VotingTally::into_canonical` reuse from governance tally CLI surface | base RFC-0862 §Tally Canonicalisation |

## Version History

| Version | Date | Notes |
|---|---|---|
| 0.1.0 | 2026-09-18 | Stub created per RFC-0011-h §Substrate-Faithful disambiguator convention. v1.4.0 amendment content remains merged in base; this file documents the patch-targeted cite surface only. |

## Authors

- @cipherocto (primary)
- @mmacedoeu (review)
