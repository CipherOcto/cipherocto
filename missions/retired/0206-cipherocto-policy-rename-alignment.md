---
name: 0206-cipherocto-policy-rename-alignment
description: Open 2026-08-19; RFC-0206 §Future Work phantom pointer 4/4 — `crates/cipherocto-policy/` → `crates/octo-policy/` rename + first adapter impl (`octo-policy-storage`) introducing the `PolicyStore` owner trait per RFC-0206 §Roles. Names the review-RFC requirement (one RFC per adapter per §Promotion Path Condition 4).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T23:55:00.000Z
---

> **SUPERSEDED by 0206-006-cipherocto-policy-rename-alignment**

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
- **(c) Trait introduction + adapter impl** — declare `PolicyStore`
  in `crates/octo-policy/src/lib.rs`; create the per-owner adapter
  crate `crates/octo-policy-storage/` with
  `impl PolicyStore for StoolapPolicyStore` and a
  `register(Arc<Database>) -> Arc<dyn PolicyStore>` constructor.
- **(d) RFC requirement** — this mission requires a separate
  per-adapter RFC per RFC-0206 §Promotion Path Condition 4. The
  rename itself (steps a, b) is a chore-level change; the trait
  and adapter implementation (step c) require the RFC.

## Acceptance Criterion

Mission complete when:

1. `crates/cipherocto-policy/` directory does not exist.
2. `crates/octo-policy/` directory exists with identical contents
   and `Cargo.toml [package].name = "octo-policy"`.
3. All consumer `Cargo.toml` dependencies are updated and the
   workspace build is green.
4. `pub trait PolicyStore { ... }` is declared in
   `crates/octo-policy/src/lib.rs`.
5. `crates/octo-policy-storage/` exists and implements `PolicyStore`
   for the Stoolap-backed struct.
6. The naming-convention lint no longer flags `octo-policy/`.

## Cross-references

- RFC-0206 §Future Work
- RFC-0206 §Roles + Authorities
- RFC-0206 §Promotion Path Condition 4
- Mission `0206-octo-storage-naming-convention-lint`

## Out of scope

- Substrate retirement (owned by `0206-octo-storage-core-deprecation`)
- Substrate versioning policy (owned by `0206-octo-storage-facade-versioning`)
- Naming-convention lint implementation (owned by
  `0206-octo-storage-naming-convention-lint`)
- Market adapter (owned by `0206-octo-market-storage-adapter`)

## Version History

| Version | Date       | Change                                                      |
| ------- | ---------- | ----------------------------------------------------------- |
| retired | 2026-08-22 | Superseded by `0206-006-cipherocto-policy-rename-alignment` |
