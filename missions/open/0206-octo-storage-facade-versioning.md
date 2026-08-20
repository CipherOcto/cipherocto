---
name: 0206-octo-storage-facade-versioning
description: Open 2026-08-19; RFC-0206 §Future Work phantom pointer 1/3 — facade semver policy for `octo-storage`. Additive-only within minor version; per-adapter major bump requires RFC.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T22:50:00.000Z
---

# Mission `0206-octo-storage-facade-versioning` — OPEN 2026-08-19

## Scope

Define the semver policy for the `octo-storage` Layer B facade crate.
Covers:

- Minor version bumps = additive (new re-export, new adapter crate)
- Major version bumps = breaking (removed re-export, removed adapter)
  requires an RFC
- Patch version bumps = bug-fix only (no new types, no removed types)
- The curated 8-pub-use cap (per §Cargo.toml Templates Layer B
  facade) is a hard constraint — adding a 9th requires both an RFC
  AND a major version bump

## Acceptance Criterion

`Cargo.toml` `[package].version` policy documented in `docs/runbooks/octo-storage-release.md`;
CI gate verifies that any `Cargo.toml` edit to `crates/octo-storage/`
that adds a new `pub use` also bumps `version.minor`; TV-0206-A2
(`pub use` count = 8) gates the build.

## Cross-references

- RFC-0206 §Future Work (this mission is the bullet's real pointer)
- RFC-0206 §Cargo.toml Templates Layer B facade (curated re-export)
- TV-0206-A2 (8-pub-use cap CI gate)

## Out of scope

- Substrate retirement (owned by `0206-octo-storage-core-deprecation`)
- Drop-fork migration (owned by `0205-stoolap-fork-retirement`)
- Naming-convention lint (owned by `0206-octo-storage-naming-convention-lint`)
