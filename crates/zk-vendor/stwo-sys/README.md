# stwo-sys

STWO STARK verifier FFI shim (CipherOcto zk-vendor target).

## What is this?

A separate cargo project (NOT in cipherocto workspace) that builds a
cdylib (`libstwo_sys.so` / `.dylib` / `.dll`) loaded at runtime by
cipherocto via `libloading`. Decouples STWO upstream's nightly
toolchain requirement from cipherocto's stable-rust invariant.

## Build

```bash
cd crates/zk-vendor/stwo-sys
cargo build --release
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

## Real impl (TBD)

Mission 0958-a S05 task B. Replace stub `stwo_prove` / `stwo_verify`
bodies with calls into vendored `keep-stwo/stwo` patched for stable
rustc. Until then, stub proves a 32-byte XOR digest and verify checks
XOR equality — NOT a real STARK; cipherocto logs warnings at load time.
