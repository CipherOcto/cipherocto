# stwo-sys

STWO STARK verifier FFI shim (CipherOcto zk-vendor target).

## What is this?

A separate cargo project (NOT in cipherocto workspace) that builds a
cdylib (`libstwo_sys.so` / `.dylib` / `.dll`) loaded at runtime by
cipherocto via `libloading`. Decouples STWO upstream's nightly
toolchain requirement from cipherocto's stable-rust invariant.

Mirrors stoolap's `stwo-plugin/` crate
(`/home/mmacedoeu/_w/databases/stoolap/stwo-plugin/`): same per-crate
`rust-toolchain.toml` nightly pin, same STWO + cairo-air deps, same
cdylib artifact shape.

## Build

```bash
cd crates/zk-vendor/stwo-sys
cargo +nightly-2025-06-23 build --release
# → target/release/libstwo_sys.so
```

`scripts/build-stwo-sys.sh` automates this + copies the artifact to
`dist/libstwo_sys.so` for the cipherocto deployment tarball.

## Deploy

Cipherocto expects the library at `/var/lib/cipherocto/libstwo_sys.so`
(or `$CIPHEROCTO_STWO_LIB` if set). Missing library → zk-vendor falls
back to stub verify with a logged warning.

## ABI

- `stwo_sys_version() -> *const c_char` — version string
- `stwo_prove(casm, witness, public) -> *mut ProofHandle` — STARK prove
- `stwo_verify(proof, public) -> i32` — 0 = Ok, non-zero = Err
- `stwo_free_proof(handle)` — release proof handle

Full FFI contract: see `src/lib.rs` module docs.

## Real impl (2026-07-22)

Real STWO `cairo-air` + `stwo` 2.1 + `stwo-cairo-prover` (from
starkware-libs/stwo-cairo.git v1.1.0). Proof wire format: JSON-encoded
`CairoProofForRustVerifier<Blake2sMerkleHasher>`. Verify path:
`cairo_air::verifier::verify_cairo::<Blake2sMerkleChannel>`.

Prove path: parses witness bytes as JSON `ProverInput`
(`stwo_cairo_adapter::ProverInput`), invokes
`stwo_cairo_prover::prover::prove_cairo::<Blake2sMerkleChannel>`, returns
proof as JSON-encoded `CairoProofForRustVerifier<Blake2sMerkleHasher>`.

Default prover params (Blake2s channel, log_blowup_factor=1, n_queries=70,
canonical preprocessed trace) match stoolap's
`stwo-plugin/src/verify.rs::create_default_prover_params`.
