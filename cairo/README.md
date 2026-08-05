# cairo/ — Cairo 2.x capability circuit (RFC-0958 mission 0958-a Phase B.2)

Per master plan §4 row B.2, the Cairo capability circuit lives in the
**cipherocto workspace**, not the stoolap fork. The 2026-07-22 v0.3 amendment
extracted CASM + STWO production from the stoolap fork (`feat/blockchain-sql`
branch) into cipherocto workspace crates (`zk-circuit`, `zk-verifier`,
`zk-vendor`) per [[stoolap-general-purpose-db]] — proof-systems concern,
orthogonal to SQL. The stoolap fork is untouched by this mission.

## Phase B.2 scope (LANDED 2026-08-04)

This mission lands the **Cairo 2.x source rewrite + scarb build pipeline
+ in-process Sierra→CASM pass** (mission 0958-a Phase B.2; commits
`ae4dc4f8` Session 1 + `9c996fba` Sessions 2 + 3 redo):

- `cairo/Scarb.toml` — scarb 2.16.0 project manifest
- `cairo/src/lib.cairo` — Cairo 2.x source (structs, `assert!` macro, lib target)
- `crates/zk-circuit/tests/casm_snapshot.rs` — Rust smoke test that invokes
  `scarb build`, parses the Sierra IR, drives the in-process
  `cairo-lang-sierra-to-casm` 2.20.0 pass, asserts CASM byte-determinism
  + BLAKE3 hash shape

## Why scarb and not `cairo-compile`

The prior assumption was that `cairo-compile 2.6.0` exists as a standalone
binary. **It does not.** Cairo 2.x removed the standalone compiler; the
compiler is embedded inside scarb's build pipeline. Cairo 1.x shipped a
`cairo-compile` binary that is no longer produced by any modern Cairo
toolchain.

The scarb-driven path is the only real Cairo 2.x compile route:

```text
cairo/src/lib.cairo (this directory's source)
    │ scarb build
    ▼
cairo/target/dev/capability_zk.sierra.json
    │ [crates/zk-circuit] in-process Sierra→CASM pass
    │   (cairo-lang-sierra-to-casm 2.20.0
    │    + cairo-lang-sierra-type-size 2.20.0)
    ▼
CASM bytecode → BLAKE3 hash → bundled_casm_hash()
```

## Toolchain pin

- **scarb** 2.16.0 (matches `~/.asdf/installs/scarb/2.16.0`)
- **cairo** 2.16.0 (embedded in scarb 2.16.0)
- **cairo-lang-sierra** 2.20.0 (Rust crates — Cargo handles dependency
  resolution; pin in `crates/zk-circuit/Cargo.toml`)
- **cairo-lang-sierra-to-casm** 2.20.0 (Rust crates — Cargo handles
  dependency resolution)
- Install scarb: https://docs.swmansion.com/scarb/download.html
- Or via asdf: `asdf plugin add scarb && asdf install scarb 2.16.0`
- Local dev must have scarb installed; the Rust smoke test hard-panics
  with an actionable message if scarb is missing.

## Determinism contract

The Rust smoke test (`crates/zk-circuit/tests/casm_snapshot.rs`) verifies:
1. `scarb build` succeeds.
2. Sierra IR is valid JSON with `version:1`.
3. The `funcs` array includes `capability_zk::main`.
4. The bundled `BUNDLED_CAIRO_SOURCE` constant in `crates/zk-circuit/src/lib.rs`
   matches `cairo/src/lib.cairo` on disk (the `include_str!` contract).
5. Two independent `scarb build` runs produce semantically identical IR
   (same type_declarations + funcs, modulo salsa UUIDs which only affect
   internal DB identifiers, not the IR semantics the downstream CASM
   pass consumes).
6. **CASM bytes are byte-identical across independent builds** — the
   in-process Sierra→CASM pass canonicalizes the IR via the `Program`
   AST (not the raw JSON bytes), so the UUID jitter is absorbed.

## CI

The CI workflow `.github/workflows/zk-capability-circuit.yml` (LANDED
2026-07-31 per R3 fix-up `0e0c3ee9`) installs scarb 2.16.0 via asdf +
runs the in-process Sierra→CASM pass under CI. The smoke test fails
loudly without scarb — no silent skip.

## Cross-repo coordination

**None.** Per v0.3 amendment (2026-07-22), this mission ships a single
cipherocto-side PR. The stoolap fork is not modified by mission 0958-a.