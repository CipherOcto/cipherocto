# PATCHES.md — vendored STWO stable-rust patches

This document tracks the stable-rust patches applied to the upstream
`keep-stwo/stwo` source when vendored into the cipherocto workspace
at `crates/zk-vendor/stwo/` (mission 0958-a S05 Phase C.2; per
[[stoolap-general-purpose-db]] extraction, 2026-07-22).

Each patch is recorded as:
- **What changed** — code-level diff summary
- **Why** — the stable-rust blocker or compatibility rationale
- **Upstream commit** — the upstream commit hash being patched (when
  known)

## Status (2026-07-31)

**Scaffolding only.** The vendored crate currently ships a
deterministic mock implementation that mirrors the upstream STWO API
surface (`Prover`, `verify`, `Proof`, `PublicInputs`, `ProverError`,
`VerifyError`). Real source drop is pending the `cipherocto-stable`
tag on the cipherocto fork of `keep-stwo/stwo` (the upstream tag does
not exist as of 2026-07-31). When the real source lands, the mock
bodies are replaced with upstream STWO function calls; the API
surface does not change.

## Patch 1 — curve25519-dalek SIMD intrinsics → stable alternative

- **What changed:** Upstream STWO imports `curve25519-dalek` with the
  `simd_backend` feature enabled, which gates nightly-only SIMD
  intrinsics (`#[target_feature(enable = "avx2")]`) behind
  `#![feature(avx2_target_feature)]`. The vendored fork replaces this
  with `curve25519-dalek` at default features only (scalar backend)
  for stable-rust compilation.
- **Why:** CipherOcto workspace stays stable-rust per master plan
  §8 Risk #6; nightly toolchain is forbidden.
- **Upstream commit:** TBD (pending real source drop).

## Patch 2 — `#![feature(...)]` attributes removed

- **What changed:** Upstream STWO carries a handful of
  `#![feature(...)]` attributes (e.g., `avx2_target_feature`,
  ` Portable SIMD` placeholders in dev-only benches). The vendored
  fork strips these.
- **Why:** Stable-rust invariant; CI verifies no `#![feature(...)]`
  via `git grep "#!\[feature" -- crates/zk-vendor/stwo/`.
- **Upstream commit:** TBD (pending real source drop).

## Patch 3 — Pin internal deps

- **What changed:** Pin transitive deps to versions that compile on
  stable:
  - `ruint = "=1.17.2"` — newer ruint requires rustc 1.90+ which
    stabilizes `iter_array_chunks` (breaking stwo 2.1.0's feature flag).
  - `cairo-vm = "=3.1.0"` — newer cairo-vm removed
    `cairo_vm::stdlib::collections::HashMap`, which stwo-cairo-adapter
    still imports.
  - `stwo = "=2.1.0"` — pin to exact version 2.1.0; stwo-cairo v1.1.0
    was built against stwo 2.1.0; newer 2.x versions renamed
    `FriConfig.line_fold_step` → `fold_step`, breaking the
    `cairo-serialize` crate at compile time.
- **Why:** Matches `stoolap/stwo-plugin/Cargo.toml` pin set; preserves
  cross-crate compatibility.
- **Upstream commit:** TBD (pending real source drop).

## Verification

```bash
# MSRV gate (must succeed on stable rust 1.75.0)
cargo +1.75.0 build -p stwo

# No nightly anywhere in zk-vendor
git grep "+nightly" -- crates/zk-vendor/         # must be empty
git grep "#!\[feature" -- crates/zk-vendor/stwo/  # must be empty

# Workspace stable build
cargo +stable build --workspace --all-targets
cargo +stable test -p octo-wallet --lib
cargo +stable test --workspace --lib
```

## Next steps

When the real `keep-stwo/stwo@cipherocto-stable` tag lands:
1. Replace the mock bodies in `src/lib.rs` with upstream STWO
   function calls (the API surface stays the same).
2. Update the `[dependencies]` block in `Cargo.toml` to include
   upstream STWO deps (already pinned to `=2.1.0` / `=1.17.2` /
   `=3.1.0`).
3. Update this file with actual upstream commit hashes per patch.