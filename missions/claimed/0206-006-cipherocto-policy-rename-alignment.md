---
name: 0206-006-cipherocto-policy-rename-alignment
description: Open 2026-08-20; rename crate `cipherocto-policy` → `octo-policy` — directory `crates/cipherocto-policy/` → `crates/octo-policy/` + Cargo.toml `[package].name` update + all internal `use cipherocto_policy::` references updated to `octo_policy::`. Required by 0206-004-adapter-crates.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-99c2545bccf7
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  supersedes: null
  depends_on:
    - RFC-0206
phase: orthogonal
layer: B
rfc_authority: RFC-0206
tvs: []
status: OPEN
---

# Mission `0206-006-cipherocto-policy-rename-alignment` — OPEN 2026-08-20

## Scope

Rename crate `cipherocto-policy` → `octo-policy` per 5 + §Summary Updates vs v1.8 corrections table. Workspace `Cargo.toml` uses `members = ["crates/*"]` glob, so renaming the directory is the load-bearing change; `[package].name` rename makes `use octo_policy::` paths resolve.

Covers:

- **Directory rename**: `git mv crates/cipherocto-policy crates/octo-policy`
- **`Cargo.toml [package].name`**: `"cipherocto-policy"` → `"octo-policy"`
- **Internal `use` references**: every `use cipherocto_policy::` → `use octo_policy::` across the workspace (verified via `rg 'cipherocto_policy::' crates/`)
- **Workspace `[workspace] members`** glob `["crates/*"]` already picks up the renamed directory — NO workspace Cargo.toml edit needed
- **Documentation cross-refs**: any RFC or memory card citing `cipherocto-policy` updated to `octo-policy`

## Acceptance Criterion

- `crates/octo-policy/` directory on disk (verified via `test -d crates/octo-policy` exits 0)
- `crates/cipherocto-policy/` directory absent (verified via `test ! -d crates/cipherocto-policy` exits 0)
- `rg 'cipherocto_policy::' crates/` exits 1 (zero hits)
- `rg '^\s*name\s*=\s*"cipherocto-policy"' crates/octo-policy/Cargo.toml` exits 1
- `rg '^\s*name\s*=\s*"octo-policy"' crates/octo-policy/Cargo.toml` exits 0
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` green
- `cargo fmt --all -- --check` green

## Files / Artifacts

- `git mv crates/cipherocto-policy → crates/octo-policy`
- Edit: `crates/octo-policy/Cargo.toml` (`[package].name` field)
- Edit: every file containing `use cipherocto_policy::` (path rename)

## Cross-references

- 5 (octo-policy adapter)
- 1.8 corrections (canonical naming)
- -Tier Architecture Tier 3 (adapter crate naming convention)
- RFC-0205 (coupled pair per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5)
- Mission `0206-004-adapter-crates` (requires `octo-policy/` directory on disk)

## Out of scope

- `PolicyStore` trait declaration (lives in `octo-policy/src/lib.rs` per ; owned by `0206-004-adapter-crates`)
- `octo-policy-storage/` adapter crate creation (owned by `0206-004-adapter-crates`)

## Dependencies

- RFC-0206 (acceptance precondition per BLUEPRINT.md rule 5)

## Version History

| Version | Date       | Change                                                                    |
| ------- | ---------- | ------------------------------------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing per R1 finding from `0206-004-adapter-crates` CRIT #2 + #3 |
| v3.0    | 2026-08-22 | Phase 3 close-out per long-horizon plan v1.5 §Mission layout. AC verification per memory card : LANDED 7833125e (2026-08-20). cipherocto-policy → octo-policy rename complete (4 source consumers migrated). Mission YAML edits per R10.5 scope discipline. Status transitions open→done. |
