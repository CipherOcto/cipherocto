---
name: 0206-cipherocto-policy-rename-alignment
description: Open 2026-08-19; RFC-0206 §Future Work phantom pointer 4/4 — `crates/cipherocto-policy/` → `crates/octo-policy/` rename + first adapter impl (`octo-policy-storage`) introducing the `PolicyStore` owner trait per RFC-0206 §Roles. Names the review-RFC requirement (one RFC per adapter per §Promotion Path Condition 4).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T23:55:00.000Z
---

# Mission `0206-cipherocto-policy-rename-alignment` — OPEN 2026-08-19

## Scope

Align the on-disk policy owner-trait crate naming per RFC-0206
§Roles + §Adapter Crate List. Covers:

- **(a) Crate rename** — `crates/cipherocto-policy/` directory
  renamed to `crates/octo-policy/` (aligns with the 6 other
  owner-trait crates `octo-ident/`, `octo-cap-macaroon/`,
  `octo-reputation/`, `octo-cap-macaroon-vault/`,
  `octo-matrix-session-store/`, `octo-vault/` which all use
  `octo-<x>/` form; `cipherocto-policy/` is the outlier).
- **(b) `Cargo.toml` package rename** —
  `[package].name = "cipherocto-policy"` →
  `"octo-policy"`; all consumer `Cargo.toml`s
  `[dependencies] cipherocto-policy` → `octo-policy` updated.
- **(c) Trait introduction + adapter impl** — `pub trait
  PolicyStore` declared in `crates/octo-policy/src/lib.rs`
  (currently zero traits per RFC-0206 §Roles); new per-owner
  adapter crate `crates/octo-policy-storage/` declares
  `impl PolicyStore for StoolapPolicyStore` + provides the
  `register(Arc<Database>) -> Arc<dyn PolicyStore>` constructor.
- **(d) RFC requirement** — this mission requires a separate
  per-adapter RFC per RFC-0206 §Promotion Path Condition 4; the
  rename itself (steps a, b) is a chore-level change, the trait
  + adapter impl (step c) requires the RFC.

## Acceptance Criterion

Mission complete when:

1. `crates/cipherocto-policy/` directory does not exist
2. `crates/octo-policy/` directory exists with identical
   contents + `Cargo.toml [package].name = "octo-policy"`
3. All consumer `Cargo.toml` deps updated;
   `cargo build` workspace-wide green
4. `pub trait PolicyStore { ... }` declared in
   `crates/octo-policy/src/lib.rs`
5. `crates/octo-policy-storage/` adapter crate exists +
   implements `PolicyStore` for the Stoolap-backed struct
6. The naming-convention lint
   (mission `0206-octo-storage-naming-convention-lint`) no
   longer flags `octo-policy/` for the divergence

## Cross-references

- RFC-0206 §Future Work (this mission is the bullet's real pointer)
- RFC-0206 §Roles + Authorities (PolicyStore NEW trait declaration)
- RFC-0206 §Promotion Path Condition 4 (one RFC per adapter)
- Mission `0206-octo-storage-naming-convention-lint` (the lint
  that originally flagged the divergence)

## Out of scope

- Substrate retirement (owned by `0206-octo-storage-core-deprecation`)
- Substrate versioning policy (owned by `0206-octo-storage-facade-versioning`)
- Naming-convention lint implementation (owned by
  `0206-octo-storage-naming-convention-lint`)
- Market adapter (owned by `0206-octo-market-storage-adapter`)
