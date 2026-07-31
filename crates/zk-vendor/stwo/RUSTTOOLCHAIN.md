# RUSTTOOLCHAIN.md — vendored STWO MSRV invariant

**Status:** Stable-rust only. MSRV pinned at the workspace root
(`crates/zk-vendor/rust-toolchain.toml`) to **Rust 1.75.0**.

## Invariant

The vendored STWO crate MUST compile on stable rust 1.75.0 without
`+nightly` or `#![feature(...)]` attributes. CI verifies via:

```bash
cargo +1.75.0 build -p stwo
git grep "+nightly" -- crates/zk-vendor/         # must be empty
git grep "#!\[feature" -- crates/zk-vendor/stwo/  # must be empty
```

## Why 1.75.0

- Upstream STWO 2.1.0 pulls in `ruint` 1.17.x which requires stable
  rust features introduced in 1.75+ (`iter_array_chunks` was stabilized
  in 1.75 but only as a polyfill; pinning at 1.75 avoids toolchain
  drift).
- `curve25519-dalek` at default features (replacing SIMD) requires
  rustc 1.75+ for the `Scalar::from_canonical_bytes` API.
- Matches the workspace-wide MSRV already pinned in
  `crates/zk-vendor/rust-toolchain.toml`.

## Bumping MSRV

MSRV bumps require:
1. Update `crates/zk-vendor/rust-toolchain.toml` `channel` field.
2. Update this document with the new pin + rationale.
3. CI must pin the matching Rust version
   (`.github/workflows/zk-capability-circuit.yml`).
4. Re-run `cargo +stable build --workspace` + `cargo test --workspace`.

If bumping for upstream STWO reasons, prefer the lowest stable rust
version that satisfies all three pinned deps (`ruint 1.17.x`,
`cairo-vm 3.1.0`, `stwo 2.1.0`).