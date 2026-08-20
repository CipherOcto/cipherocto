---
name: 0205-stoolap-fork-retirement
description: Open 2026-08-19; RFC-0205 §Future Work phantom pointer 3/3 — drop-fork migration procedure: (a) Layer B crates migrate from `octo_storage_core::Database` re-export to direct `stoolap` crates-io semver; (b) `octo-storage-core` retires + handle re-export removed; (c) §Roles 1-4 lose steward/owner/reviewer duties; (d) RFC-0205 archived to `rfcs/archived/storage/`. Sister mission to RFC-0206 §Future Work `0205-stoolap-fork-retirement` cross-ref.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T22:50:00.000Z
---

# Mission `0205-stoolap-fork-retirement` — OPEN 2026-08-19

## Scope

Drop-fork migration procedure for the scenario where upstream Stoolap
merges the DQA features that justify the fork, and the fork is
retired. Covers four sub-tasks:

- **(a) Layer B migration** — every Layer B crate currently consuming
  `octo_storage_core::Database` (the re-exported handle) migrates to a
  direct `stoolap` crates-io semver dep. Cargo.toml edits per crate +
  removal of the Layer A `octo-storage-core` indirection.
- **(b) Substrate retirement** — `crates/octo-storage-core/` is
  retired; the `pub use stoolap::Database;` re-export in
  `crates/octo-storage-core/src/lib.rs` is removed; the
  `Cargo.toml` direct `rev = "<sha-N>"` pin is replaced by a
  crates-io semver dep.
- **(c) Roles retirement** — RFC-0205 §Roles entries 1-4 (Stoolap
  steward team, octo-storage-core owner, RFC reviewer, On-call
  security) lose the steward + `octo-storage-core owner` duties;
  RFC reviewer role persists (other RFCs need review); on-call
  security role persists (other forks need it).
- **(d) RFC-0205 archival** — RFC-0205 is archived to
  `rfcs/archived/storage/` post-migration per §Compatibility Forward
  drop-fork trigger. RFC-0206 §Compatibility Forward references this
  retirement bullet.

## Acceptance Criterion

Mission complete when:

1. Every Layer B crate's `Cargo.toml` shows `stoolap` (crates-io semver)
   instead of `octo-storage-core` (path dep) for the handle
2. `crates/octo-storage-core/` directory removed + workspace
   `Cargo.toml` member list updated
3. TV-0206-A1 (`pub use stoolap::Database;` exact-once in
   `crates/octo-storage-core/src/lib.rs`) returns 0 matches
4. RFC-0205 file moved from `rfcs/draft/storage/` to
   `rfcs/archived/storage/`

## Cross-references

- RFC-0205 §Future Work (this mission is the bullet's real pointer)
- RFC-0205 §Compatibility Forward (drop-fork trigger)
- RFC-0206 §Compatibility Forward (sister reference)
- RFC-0206 §Future Work bullet 3 (cross-RFC coordination; same
  drop-fork scenario, single mission file)

## Out of scope

- Fork feature upstreaming (owned by `0205-stoolap-fork-feature-upstreaming`)
- Release process (owned by `0205-octo-stoolap-frozen-release-process`)
- Phase 2 Task 5 (`stoolap-fork-feature-upstreaming.md`) — distinct
  mission file
