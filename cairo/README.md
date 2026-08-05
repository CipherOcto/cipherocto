# cairo/ — Cairo 2.x capability circuit (RFC-0958 Session 1)

Per master plan §4 row B.2, the Cairo capability circuit lives in the
**cipherocto workspace**, not the stoolap fork. The 2026-07-22 v0.3 amendment
extracted CASM + STWO production from the stoolap fork (`feat/blockchain-sql`
branch) into cipherocto workspace crates (`zk-circuit`, `zk-verifier`,
`zk-vendor`) per [[stoolap-general-purpose-db]] — proof-systems concern,
orthogonal to SQL. The stoolap fork is untouched by this mission.

## Session 1 scope (current)

This session lands the **Cairo 2.x source rewrite + scarb build pipeline**:

- `cairo/Scarb.toml` — scarb 2.16.0 project manifest
- `cairo/src/lib.cairo` — Cairo 2.x source (structs, `assert!` macro, lib target)
- `crates/zk-circuit/tests/casm_snapshot.rs` — Rust smoke test that invokes
  `scarb build`, parses the Sierra IR, and asserts semantic determinism

The Sierra→CASM pass (producing the bytes whose BLAKE3 hash is the canonical
`compiled_casm_hash`) lives in **Session 2** — it requires wiring
`cairo-lang-sierra-to-casm` and `cairo-lang-compiler` into `crates/zk-circuit`.
The Session 1 prerequisite is that the Cairo source compiles deterministically
through the real scarb toolchain, which the smoke test verifies.

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
    │ [Session 2] cairo-lang-sierra-to-casm
    ▼
CASM bytecode → BLAKE3 hash
```

## Toolchain pin

- **scarb** 2.16.0 (matches `~/.asdf/installs/scarb/2.16.0`)
- **cairo** 2.16.0 (embedded in scarb 2.16.0)
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

**Why "semantically identical" rather than byte-identical:** scarb uses
salsa (an incremental-compiler database) which generates fresh UUIDs per
compile session. The raw JSON bytes differ, but the IR content — types,
function signatures, statement bodies — is deterministic. Session 2's
CASM emission will compare CASM bytes directly because the Sierra→CASM
pass canonicalizes the IR before lowering.

## CI

The CI workflow `.github/workflows/zk-capability-circuit.yml` (Session 3)
installs scarb 2.16.0 via asdf. Until that workflow lands, local dev must
install scarb manually. The smoke test fails loudly without scarb — no
silent skip.

## Cross-repo coordination

**None.** Per v0.3 amendment (2026-07-22), this mission ships a single
cipherocto-side PR. The stoolap fork is not modified by mission 0958-a.
