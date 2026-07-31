# cairo/ — Cairo 2.6.0 capability circuit (RFC-0958 Phase B.2)

Per master plan §4 row B.2, the Cairo capability circuit lives in the
**cipherocto workspace**, not the stoolap fork. The 2026-07-22 v0.3 amendment
extracted CASM + STWO production from the stoolap fork (`feat/blockchain-sql`
branch) into cipherocto workspace crates (`zk-circuit`, `zk-verifier`,
`zk-vendor`) per [[stoolap-general-purpose-db]] — proof-systems concern,
orthogonal to SQL. The stoolap fork is untouched by this mission.

## Files

- **`capability_zk.cairo`** — Cairo 2.6.0 source for the capability
  attestation circuit. Per RFC-0958 §Algorithms with R1 fixes applied
  (C1 holder_sig in witness, C2 PartialEq, C3 determinism, C4 trace
  canonicalization). Compiles via scarb/cairo-compile; structural checks
  only — cryptographic verifications are off-circuit in the STWO prover
  + Rust verifier.
- **`build.sh`** — manual / CI entry point. Shells out to `cairo-compile`
  and prints the BLAKE3 hash. Fails loudly if `cairo-compile` is not in
  PATH (no silent skip — S1 risk mitigation).

## Build invocation

```bash
# Manual (after installing cairo-compile 2.6.0 via scarb/asdf):
cairo/build.sh
# → writes cairo/capability_zk.casm + cairo/capability_zk.casm.blake3

# Runtime (via zk-circuit crate, memoized):
# crates/zk-circuit::compile_from_source(include_str!("cairo/capability_zk.cairo"))
#   → shells out to cairo-compile, captures CASM bytes, BLAKE3 hashes
```

## Cairo toolchain pin

Pin `cairo-compile = 2.6.0` via scarb or asdf. CI installs the pin in
`.github/workflows/zk-capability-circuit.yml` (S3 deliverable). Local
development: install scarb (https://github.com/starkware-libs/cairo) and
run `scarb --version` to verify 2.6.0.

## CASM hash determinism contract

Same `capability_zk.cairo` source → same CASM bytecode → same BLAKE3 hash.
Across processes, across architectures, across platforms. STWO Fiat-Shamir
transform is Class A deterministic (RFC-0958 §Determinism Class A).

The check-in hash lives at `crates/zk-circuit/tests/casm_snapshot.rs`
(`EXPECTED_CASM_BLAKE3_HASH`). The snapshot test asserts the hash matches;
CI must install `cairo-compile` for the test to run (skipped locally if
the toolchain is absent — local dev can stub via `bundled_casm_hash`).

## Cross-Repo Coordination

**None.** Per v0.3 amendment (2026-07-22), this mission ships a single
cipherocto-side PR. The stoolap fork is not modified by mission 0958-a.