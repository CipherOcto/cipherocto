---
name: 0206-octo-storage-naming-convention-lint
description: Open 2026-08-19; RFC-0206 §Future Work phantom pointer 3/3 — Clippy lint on owner-trait crate adapter naming (`octo-<owner>-storage` pattern) + Stoolap<OwnerTrait> reference lint (`octo_storage_no_direct_stoolap` per RFC-0206 §Cargo.toml Templates Layer A.2 v1.5 §Cargo.toml Templates Layer A.2 Clippy Lint). Defines the lint crate `crates/octo-clippy-lints/` + the `register_lints` function (Clippy-driver `register_lints` per RFC-0206 §Cargo.toml Templates Layer A.2 v1.5 §Cargo.toml Templates Layer A.2 Clippy Lint mechanism).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T23:55:00.000Z
---

# Mission `0206-octo-storage-naming-convention-lint` — OPEN 2026-08-19

## Scope

Define + land the Clippy lint crate `crates/octo-clippy-lints/`
that enforces RFC-0206 §Cargo.toml Templates Layer A.2 owner-trait
crate purity + per-owner adapter naming convention. Covers:

- **(a) Lint crate workspace member** — `crates/octo-clippy-lints/`
  added to workspace `Cargo.toml` member list; depends on
  `clippy_utils` + `rustc_session` + `rustc_span`.
- **(b) `octo_storage_no_direct_stoolap` lint** — flags any
  `use stoolap::` or `stoolap::Type` reference inside
  `crates/octo-<owner>/src/**` owner-trait crates (Layer A purity
  rule). Catches TV-0206-06 escapes that grep misses (string
  literals, generic bounds, macro args, build script).
- **(c) `octo_storage_adapter_naming` lint** — enforces that every
  crate implementing a per-owner adapter trait follows the
  `octo-<owner>-storage` naming convention (catches
  `crates/cipherocto-policy/` divergence).
- **(d) `register_lints` driver hook** — the lints MUST be
  registered via a `register_lints` function picked up by the
  Clippy driver (per RFC-0206 §Cargo.toml Templates Layer A.2 v1.5
  mechanism). NOT registered via `clippy.toml` (TOML config-only,
  cannot invoke lints).
- **(e) Scope coverage** — lint covers `--all-targets` (`src/`,
  `tests/`, `examples/`, `benches/`, `build.rs`, doc-tests) per
  RFC-0206 §Cargo.toml Templates Layer A.2 v1.5 requirement.

## Acceptance Criterion

Mission complete when:

1. `crates/octo-clippy-lints/` exists + workspace member
2. `cargo clippy --all-targets --all-features -D warnings` runs
   both lints across the workspace without false positives on
   the 7 owner-trait crates today
3. `cargo clippy --all-targets --all-features -D warnings` flags
   any owner-trait crate that adds `use stoolap::*` (regression TV)
4. `crates/cipherocto-policy/` still flagged for the naming
   divergence (regression TV until the rename lands)

## Cross-references

- RFC-0206 §Future Work (this mission is the bullet's real pointer)
- RFC-0206 §Cargo.toml Templates Layer A.2 v1.5 (the lint mechanism spec)
- RFC-0206 TV-0206-06 (the grep fallback, superseded by lint after
  this mission lands)
- Mission `0206-cipherocto-policy-rename-alignment` (the naming-
  divergence cleanup)

## Out of scope

- Substrate retirement (owned by `0206-octo-storage-core-deprecation`)
- Substrate versioning policy (owned by `0206-octo-storage-facade-versioning`)
- Policy adapter (owned by `0206-cipherocto-policy-rename-alignment`)
- Market adapter (owned by `0206-octo-market-storage-adapter`)
