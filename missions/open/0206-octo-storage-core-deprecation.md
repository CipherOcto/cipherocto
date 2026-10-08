---
name: 0206-octo-storage-core-deprecation
description: Open 2026-08-19; RFC-0206 §Future Work phantom pointer 2/3 — `octo-storage-core` Layer A substrate retirement procedure when the fork upstream-merges the DQA features and the fork drops. Sister mission to RFC-0205 §Future Work `0205-stoolap-fork-retirement`.
metadata:
  node_type: mission
  type: project
  created: 2026-08-19T23:55:00.000Z
---

# Mission `0206-octo-storage-core-deprecation` — OPEN 2026-08-19

## Status

Open (2026-08-19). Claimable — no claimant assigned.

## Dependencies

Declared in the frontmatter `depends_on` list. This mission has no
prerequisite mission files; its gating condition is the parent RFC acceptance
recorded in Scope.

## Scope

Retire the Layer A substrate crate `crates/octo-storage-core/` when
the upstream Stoolap fork drops (per RFC-0205 §Compatibility
Forward drop-fork trigger). Covers:

- **(a) Layer B migration off the substrate** — every adapter
  crate currently consuming `octo_storage_core::Database` migrates
  to a direct `stoolap` crates-io semver dep. Per-adapter
  `Cargo.toml` edit + removal of the substrate path dep.
- **(b) Substrate crate removal** — `crates/octo-storage-core/`
  directory deleted; workspace `Cargo.toml` member list updated;
  the `pub use stoolap::Database;` re-export in
  `crates/octo-storage/src/lib.rs` (the facade carries the re-export
  post-retirement, RFC-0206 §Cargo.toml Templates Layer B facade)
  remains but now aliases the crates-io `stoolap::Database`
  directly.
- **(c) §Roles 2 retirement** — RFC-0206 §Roles entry 2 (Layer A
  substrate owner) loses the substrate-owner duty; facade owner
  (entry 3) absorbs the substrate-handle governance.
- **(d) RFC-0206 archival mirror** — this mission is RFC-0206's
  half of the joint `0205-stoolap-fork-retirement` +
  `0206-octo-storage-core-deprecation` sister-mission pair;
  execution requires both missions green together.

## Claim pairing

This mission is one half of a sister pair. It shares its deliverable
with `0205-stoolap-fork-retirement`, which owns the RFC-0205 half of
the same drop-fork retirement. Claim one and you must claim the other
in the same session, because both halves edit the same `Cargo.toml`
handle dependency and both must land green together.

## Acceptance Criteria

Mission complete when:

- [ ] Every per-owner adapter crate's `Cargo.toml` shows `stoolap`
      (crates-io semver) instead of `octo-storage-core` (path dep)
- [ ] `crates/octo-storage-core/` directory removed + workspace
      `Cargo.toml` member list updated
- [ ] TV-0206-A1 (`pub use stoolap::Database;` in `crates/octo-storage/src/lib.rs`,
      the facade carries post-retirement) returns exactly 1 match
- [ ] CI clippy + build green across the workspace

## Cross-references

- RFC-0206 §Future Work (this mission is the bullet's real pointer)
- RFC-0206 §Compatibility Forward (drop-fork trigger)
- RFC-0205 §Compatibility Forward (sister reference)
- Mission `0205-stoolap-fork-retirement` (Layer B migration + RFC-0205 archival)

## Out of scope

- Substrate versioning policy (owned by `0206-octo-storage-facade-versioning`)
- Naming-convention lint (owned by `0206-octo-storage-naming-convention-lint`)
- Policy adapter (owned by `0206-cipherocto-policy-rename-alignment`)
- Market adapter (owned by `0206-octo-market-storage-adapter`)
