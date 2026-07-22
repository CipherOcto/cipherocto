# cairo/ — Cairo 2.6.0 capability circuit (RFC-0958 Phase B.2 + C.2)

Per master plan §4 row B.2 + C.2, the ZK capability circuit + STWO plugin
stable-rust vendoring live in the stoolap fork (`feat/blockchain-sql` branch).
This directory in the cipherocto repo holds:

- `capability_zk.cairo` — Cairo 2.6.0 pseudocode for the capability attestation
  circuit. Per RFC-0958 §Algorithms with R1 fixes (C1-C5, H1-H14) applied.
  Compiles via scarb/cairo-compile in the fork; compiled CASM is committed
  to `bundled.rs` constants in the cipherocto repo (`crates/octo-wallet/src/zk_verify/`).
- `build.sh` — shell script invoked at build time to compile the circuit
  (R1 H12 fix: was `cairo/build.rs`; Cairo is not Rust so `.rs` was incorrect).
  MVP stub returns 0 if `cairo-compile` is not in PATH; production wires
  scarb/asdf.

## Cross-Repo Coordination

Per RFC-0958 §Cross-Repo Coordination:

1. **Cipherocto PR first** (defines interfaces: `CapabilityClass` enum,
   `ProofBundle` struct, `verify_capability_zk` signature, `ZkMintError`
   variants). See `crates/quota-router-core/src/zk_verify/mod.rs` and
   `crates/octo-wallet/src/cap/zk_mint.rs`.
2. **Stoolap fork PR second** (implements CASM compilation + STWO plugin
   stable-rust vendoring).
3. Both PRs reviewed together for atomic landing.

## MVP Status

- `capability_zk.cairo` authored (pseudocode; not compiled locally).
- `build.sh` is a no-op stub (no Cairo toolchain in cipherocto CI).
- CASM compilation + STWO verify delegate to the stoolap fork.
- Cipherocto-side verifier (`verify_capability_zk`) accepts matching public
  inputs + CASM hash (MVP stub); STWO verify is a no-op until fork wiring.