# ZK: compiling and proving a Cairo circuit with STWO

**Status:** verified end to end on 2026-10-07. A 6 289 495-byte STWO proof of
`cairo/src/lib.cairo` was produced and accepted by
`cairo_air::verifier::verify_cairo::<Blake2sMerkleChannel>`.

**Scope of this document.** This is the _recipe_, written to be transferable
between CipherOcto and **Stoolap** (which has its own `stwo-plugin`; see
§Relating this to Stoolap). It records the one path that works, the two
non-obvious traps that are not documented anywhere upstream, and the exact
toolchain that was verified.

---

## 1. The shape of the problem

You have a Cairo circuit you want to prove with STWO. The intuition — compile
it to CASM, hand the arguments to the VM on the stack, adapt the trace, prove —
**does not work**, and fails in a way that looks like a soundness bug.

What actually works is the **Cairo executable ABI**: compile the entrypoint
into a real `Executable` (wrapper + entrypoints), and pass the function's
arguments as flat `Array<felt252>` **calldata through the hint processor**, not
on the stack.

### Why the naive approach fails

`cairo-lang-executable` 2.20.0 wraps any `#[executable]` function in a
3-instruction header emitted at **offset 0** (`src/executable.rs`):

```text
ap += <wrapper.builtins.len()>;
call rel 4;
jmp rel 0;
```

`jmp rel 0` is a deliberate non-returning sentinel: execution stops when
control returns to pc = `entrypoint.offset + 4`, which is exactly the
`end` stwo-cairo passes to `Program::new_for_proof`.

Consequences that matter:

- The real entrypoint is **not** your function. It is the wrapper at offset 0.
- Your function's arguments are deserialised from calldata by wrapper hints,
  which read `CairoHintProcessor.user_args`.
- The wrapper adds an **`output`** builtin, so the return value is serialised
  into the output builtin segment — which is public memory the verifier checks.
- `entrypoint.builtins` is the set of builtins **declared** by the wrapper. It
  is _not_ the set of builtins the layout instantiates. See §4.

If you build a stack frame by hand instead, the circuit runs _something_, does
not necessarily reach your function's return, and writes **no panic payload**
anywhere in memory — because a Cairo `assert!` does not trap under cairo-vm
(see §5). The failure is silent and looks like the circuit rejected your
witness when it never ran your code at all.

---

## 2. The recipe

### 2.1 Circuit source

```cairo
#[derive(Drop, Copy, Serde)]   // the wrapper deserializes calldata
pub struct PublicInputs { /* felt252 / u32 / u64 / u8 members */ }

#[derive(Drop, Copy, Serde)]
pub struct PrivateWitness { /* ... */ }

// `#[executable]` is REQUIRED: find_executable_functions filters on the
// `executable_raw` attribute and silently returns an empty list otherwise.
#[cfg(feature: "executable")]
#[executable]
pub fn main(
    pub_inputs: PublicInputs,
    priv_witness: PrivateWitness,
    caveat_felts: Array<felt252>,
    steps_flat: Array<felt252>,
) -> felt252 {
    // ...
}
```

`#[derive(Serde)]` on the structs is mandatory. Struct Serde is a **plain
member sequence** in declaration order — no header, no offsets — so the calldata
layout for a `(PublicInputs, PrivateWitness, Array, Array)` entrypoint is:

```text
[ p0..p9 , w0..w4 ,  len, e0, e1, ...  ,  len, f0, f1, ...  ]
   10 felts   5 felts    Array<felt252>     Array<felt252>
```

and `Array<felt252>` serialises as `[len, elems…]` with the element pointer at
`elems[0]` and `array_len` reading `[ptr - 1]`
(`core/src/array.cairo`, `ArraySerde`).

#### Why the attribute must be `cfg`-gated — and why you need a second entrypoint

**These are the second and third traps, and both break your build if you miss
them.**

Only `scarb execute` and `cairo_lang_executable::compile::compile_executable`
enable the executable plugin. Everything else — **`scarb build`**,
`scarb cairo-test`, and any plain `cairo-lang-compiler` consumer — builds with
the default plugin suite, which rejects the raw attribute outright:

```
error[E2200]: Plugin diagnostic: Unsupported attribute.
 --> src/lib.cairo:345:1
#[executable]
```

So the attribute must be gated. Gate it on a **feature cfg that is off by
default and set explicitly by the code that needs the wrapper**:

```cairo
#[cfg(feature: "executable")]
#[executable]
pub fn executable_main(/* ... */) -> felt252 { /* ... */ }
```

**`#[cfg]` strips the entire item, not just the attribute.** Gating the one
entrypoint you have deletes it from every non-executable build — including the
Sierra that a downstream `scarb build` consumer still reads, which then fails
with a confusing error:

```
Sierra program has no `capability_zk::main` function;
the compiled artifact would have no entrypoint
```

Gating on `cfg(not(test))` does not help either: in a non-test build
`not(test)` is true, so the attribute is still applied and `scarb build` still
fails.

The fix is **two one-line entrypoints over the same logic** — one plain, one
executable:

```cairo
pub fn main(/* ... */) -> felt252 { stage2_main(/* ... */) }

#[cfg(feature: "executable")]
#[executable]
pub fn executable_main(/* ... */) -> felt252 { stage2_main(/* ... */) }
```

Do **not** declare the `executable` feature in `Scarb.toml`. Declaring it would
let `scarb build -F executable` reproduce exactly the build that fails; leaving
it undeclared guarantees it is always off for scarb.

### 2.2 Compile to an Executable

`prepare_db` does not set your feature, and its `detect_corelib()` is a fragile
search for a `corelib/src` directory. Build the database yourself:

```rust
use cairo_lang_compiler::db::{validate_corelib, RootDatabase};
use cairo_lang_executable_plugin::executable_plugin_suite;
use cairo_lang_filesystem::cfg::{Cfg, CfgSet};
use cairo_lang_filesystem::db::init_dev_corelib;

let mut builder = RootDatabase::builder();
builder
    .skip_auto_withdraw_gas()                       // NB: "withdraw", not "with_draw"
    .with_cfg(CfgSet::from_iter([
        Cfg::kv("gas", "disabled"),
        Cfg::kv("feature", "executable"),           // <-- turns the attribute on
    ]))
    .with_default_plugin_suite(executable_plugin_suite());
let mut db = builder.build()?;                      // detect_corelib is off by default
init_dev_corelib(&mut db, corelib_src_path)?;       // returns (), takes PathBuf by value
validate_corelib(&db)?;

let config = ExecutableConfig::default();
let result = compile_executable(
    &mut db,
    project_path,                             // dir containing cairo_project.toml
    Some("capability_zk::main"),
    DiagnosticsReporter::stderr(),            // there is no `empty()`; use stderr
    config,
)?;
let exec = Executable::new(result.compiled_function);
```

`Executable` is `Serialize + Deserialize`, so it round-trips cleanly as the
artifact that crosses the crate boundary. **Ship the upstream `Executable` type
as JSON** rather than hand-rolling a `{casm_hex, hints, main_pc}` shape — it
removes the entire class of wire-contract drift between producer and consumer.

### 2.3 Run it

```rust
use cairo_lang_runner::{build_hints_dict, Arg, CairoHintProcessor};
use cairo_vm::cairo_run::{cairo_run_program_with_initial_scope, CairoRunConfig};
use cairo_vm::types::{layout_name::LayoutName, program::Program,
                      relocatable::MaybeRelocatable};
use cairo_vm::Felt252;

let entrypoint = exec.entrypoints.iter()
    .find(|e| matches!(e.kind, EntryPointKind::Standalone))
    .expect("Standalone entrypoint");

let data: Vec<MaybeRelocatable> = exec.program.bytecode.iter()
    .map(Felt252::from).map(MaybeRelocatable::from).collect();
let (hints, string_to_hint) = build_hints_dict(&exec.program.hints);

let program = Program::new_for_proof(
    entrypoint.builtins.clone(),
    data,
    entrypoint.offset,
    entrypoint.offset + 4,          // the `call rel 4` return address
    hints,
    Default::default(), Default::default(), vec![], None,
)?;

let mut hp = CairoHintProcessor {
    runner: None,
    user_args: vec![vec![Arg::Array(calldata.into_iter().map(Arg::Value).collect())]],
    string_to_hint,
    starknet_state: Default::default(),
    run_resources: Default::default(),
    syscalls_used_resources: Default::default(),
    no_temporary_segments: false,
    markers: Default::default(),
    panic_traceback: Default::default(),
};

let runner = cairo_run_program_with_initial_scope(
    &program,
    &CairoRunConfig {
        trace_enabled: true,
        relocate_trace: false,
        layout: LayoutName::all_cairo_stwo,
        fill_holes: true,
        proof_mode: true,
        disable_trace_padding: true,
        ..Default::default()
    },
    &mut hp,
    ExecutionScopes::new(),
)?;
```

`user_args` is `Vec<Vec<Arg>>` — one inner `Vec` per **user parameter**. The
wrapper has exactly one user parameter (the calldata array), so the single
inner vector holds one `Arg::Array` wrapping every calldata felt as
`Arg::Value`.

`proof_mode: true` is what produces the canonical public memory the verifier
checks: `[initial_ap - 2] == initial_ap`, `[initial_ap - 1] == 0`.

### 2.4 Adapt — and fix the segment context

```rust
use stwo_cairo_adapter::PublicSegmentContext;

let mut prover_input = stwo_cairo_adapter::adapter::adapt(&runner)?;

// MANDATORY. `adapt()` hardcodes `PublicSegmentContext::bootloader_context()`,
// which is `[true; 11]` (its own `TODO(spapini)`). Override it. See §4.
prover_input.public_segment_context =
    PublicSegmentContext::new(&entrypoint.builtins);

let proof = prove_cairo::<Blake2sMerkleChannel>(prover_input, default_prover_params())?;
verify_cairo::<Blake2sMerkleChannel>(proof.into())?;
```

---

## 3. Prover parameters

Mirrors what stwo-cairo's CLI documents as its default:

```rust
ProverParameters {
    channel_hash: ChannelHash::Blake2s,
    channel_salt: 0,
    pcs_config: PcsConfig {
        pow_bits: 26,
        fri_config: FriConfig {
            log_last_layer_degree_bound: 0,
            log_blowup_factor: 1,
            n_queries: 70,
            fold_step: 1,          // named `line_fold_step` in stwo 2.1.0
        },
        lifting_log_size: None,
    },
    preprocessed_trace: PreProcessedTraceVariant::Canonical,
    store_polynomials_coefficients: false,
    include_all_preprocessed_columns: false,
}
```

All three `PreProcessedTraceVariant` values (`Canonical`,
`CanonicalSmall`, `CanonicalWithoutPedersen`) prove and verify on the
capability circuit. Measured proof sizes, same witness and parameters:

| Variant                    | Proof bytes | Verifies |
| -------------------------- | ----------- | -------- |
| `Canonical`                | 6,289,495   | yes      |
| `CanonicalSmall`           | 6,259,684   | yes      |
| `CanonicalWithoutPedersen` | 6,422,216   | yes      |

**The variant is not a size lever** — the spread is ~2.5%, and
`CanonicalWithoutPedersen` is the _largest_. If you are chasing proof size,
look at `n_queries` and the trace length instead. (An earlier note here
claimed all three behaved identically without recording sizes; the PoC's
`POC_VARIANT` switch also silently ignored the names `Padding`/`Sparse`, so
those runs were all `Canonical`. The table above is from a re-run with the
valid names.)

---

## 4. Trap 1 — `PublicSegmentContext` must mirror the _declared_ builtins

This is the single non-obvious step, and getting it wrong produces two
different, misleading errors.

### What the prover actually does

`extract_public_segments` (stwo-cairo-prover `src/witness/cairo.rs`) never
consults the runner's builtin segments. It reads segment pointers **off the
stack**:

```rust
let n_public_segments = public_segment_context.iter().filter(|&b| *b).count() as u32;
let start_ptrs = (initial_ap..initial_ap + n_public_segments).map(to_memory_value);
let end_ptrs   = (final_ap - n_public_segments..final_ap).map(to_memory_value);
```

`n` is the number of `true` entries in `public_segment_context`. Meanwhile
cairo-vm's `initialize_main_entrypoint` only pushes start pointers for
`program.builtins` — **not** for every builtin the layout instantiates.

So `present` must be derived from the **declared** set
(`entrypoint.builtins`), not the runner's full builtin list.

### Failure mode A — deriving it from the runner

`LayoutName::all_cairo_stwo` has **9 builtins, not 11** — it has no `ecdsa` and
no `keccak`:

```
[0] output        [1] pedersen   [2] range_check  [3] bitwise  [4] ec_op
[5] poseidon      [6] rc96       [7] add_mod      [8] mul_mod
```

Supplying the hardcoded `[true; 11]` zips 9 real ranges against 11 names, so
every segment from `bitwise` onward is mislabelled and Poseidon's 54 cells land
on `ec_op`. The verifier says:

```
assertion failed: ECDSA segment is not empty
  left: 1590   right: 2193
```

### Failure mode B — the shape is right but the source is wrong

Setting `present` to the correct `[T,T,T,F,T,T,F,T,T,T,T]` **masked** by the
runner's 9 builtins still fails, because `n` becomes 9 while the stack only
carries 3 pointers:

```
Builtin segment start doesn't match claim
  left: 2139   right: 2107
```

### The fix

```rust
prover_input.public_segment_context =
    PublicSegmentContext::new(&entrypoint.builtins);   // declared, not runner-wide
```

`PublicSegmentContext::new(&[BuiltinName])` is public and maps each builtin to
its slot in the canonical order
`output, pedersen, range_check, ecdsa, bitwise, ec_op, keccak, poseidon,
range_check96, add_mod, mul_mod`, leaving absent ones `false`.

---

## 5. Trap 2 — a Cairo `assert!` does not trap under cairo-vm

> **Corrected on the Executable path.** The bare-`main` behaviour described
> below is real, but it **does not apply once you use the `Executable`
> wrapper**, which is the path this recipe produces. The wrapper checks the
> panic flag and fails the run hard:
>
> ```text
> Error at pc=0:16: An ASSERT_EQ instruction failed: 0 != 1
> ```
>
> Verified for both an over-limit `axes_count` and a shallow caveat chain.
> So on this path `run_until_pc` returning `Err` **is** the assertion signal,
> and gating on it is a real gate rather than a formality. Keep reading this
> section only if you are running the bare entrypoint.

cairo-vm's `VirtualMachineError` has **no `Panic` variant**
(`vm/errors/vm_errors.rs`: `RunnerError | Memory | Math | TracerError |
MainScopeError | Other`). A Cairo 2.x `assert!` compiles into a call into
`core::panics::panic`, which writes its payload and **returns through the
caller**.

Consequences:

- A failing assertion produces a perfectly ordinary execution trace, and a
  `ProverInput`, with **no** error at the VM level.
- The run produces no `Err`, so you cannot rely on `run_until_pc` failing.
- The only reliable signal is the circuit's own return value, or the panic
  payload in memory.

This is why the diagnostic in §1 — "no panic payload anywhere in segment 1" —
was the decisive evidence that the circuit had never been entered: a real
`assert!` failure always leaves its message behind as a short-string felt.

**Guard against this.** A harness that only has rejection tests proves
nothing: a harness that rejects every witness satisfies all of them. Any gate
that reads a circuit's result needs a **positive control** — a witness the
circuit accepts and the harness must therefore accept — or it is untested.

---

## 6. Toolchain

| Component      | Version                | Notes                                                                                                                                                                                                                      |
| -------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `stwo-cairo`   | git tag **v1.3.0**     | v1.1.0 pins `cairo-vm = "3.0.0"` and re-exports `cairo_vm::stdlib::collections::HashMap`, which cairo-vm 3.2.0 removed. **v1.3.0 fixed this** (`use std::collections::HashMap`), which is what unblocks the 3.2.0 upgrade. |
| `stwo`         | 2.3.0                  | v1.1.0 used 2.1.0.                                                                                                                                                                                                         |
| `cairo-vm`     | 3.2.0                  | `final_pc` becomes `pub` (private in 3.1.0); adds the `cairo_run` module.                                                                                                                                                  |
| `cairo-lang-*` | 2.20.0                 | `executable`, `runner`, `compiler`, `casm`, `utils`, `diagnostics`.                                                                                                                                                        |
| scarb          | **2.20.0**             | `#[executable]` is unknown to 2.16. scarb never wires the executable plugin into `scarb build` — only `scarb execute` — which is why the Executable is produced by `compile_executable` in Rust.                           |
| rust           | **nightly-2026-01-15** | stwo-cairo declares `NIGHTLY_VERSION` in `.github/workflows/cairo-ci.yaml`. It has no `rust-toolchain.toml` of its own. Needs `portable_simd`, `iter_array_chunks`, `raw_slice_split`.                                     |

`cairo-vm` 3.2.0 accepts `cairo-lang-casm >= 2.12.0-dev.0`, so pinning
`=2.20.0` is compatible.

> **Nightly pinning is not optional.** stwo 2.1.0 uses
> `#![feature(iter_array_chunks)]`, which was **stabilised** in nightlies after
> 2025-07 and breaks the build. Any nightly newer than the declared date must be
> checked before adopting it.

### Corelib

`prepare_db` calls `detect_corelib()`, which searches for `corelib/src` relative
to `CARGO_MANIFEST_DIR`, the current executable, and the cwd. It returns `None`
→ `Failed to find development corelib.`

**Symlink the corelib into the crate directory** so the executable-relative
search finds it:

```bash
mkdir -p mycrate/corelib
ln -sfn "$(ls -d ~/.cache/scarb/registry/std/v*/core | tail -1)/src" mycrate/corelib/src
```

Verified working with a scarb 2.16 corelib under cairo-lang 2.20.

### Project config

`compile_executable` calls `setup_project`, which reads `ProjectConfig::from_directory`
— a **`cairo_project.toml`**, not a `Scarb.toml`:

```toml
[crate_roots]
capability_zk = "src"
```

---

## 7. Verified reference numbers

CipherOcto `capability_zk`, cairo-lang 2.20.0, scarb 2.20.0, stwo-cairo v1.3.0:

| Quantity                | Value                                                              |
| ----------------------- | ------------------------------------------------------------------ |
| Executable bytecode     | 1587 words                                                         |
| Hints                   | 39                                                                 |
| Standalone entrypoint   | offset 0, builtins `[output, range_check, poseidon]`               |
| Bootloader entrypoint   | offset 6, same builtins                                            |
| Calldata                | 23 felts (10 public + 5 witness + `[3,15,16,22]` + `[3,17,18,19]`) |
| Trace PCs               | 440                                                                |
| Public memory addresses | 1596                                                               |
| Proof size              | 6 289 495 bytes                                                    |
| Verify                  | `verify_cairo::<Blake2sMerkleChannel>` → OK                        |

For contrast, the hand-rolled stack-frame approach produced 1522 words and 33
hints — no wrapper, and `output` missing from the builtin set.

---

## 8. Relating this to Stoolap

Stoolap's `stwo-plugin` already has a `create_default_prover_params`
equivalent. What it must **not** do is hand-build a stack frame. The three
things to port:

1. `compile_executable` → `Executable::new` → `Program::new_for_proof` with
   `start = entrypoint.offset`, `end = entrypoint.offset + 4`.
2. Arguments via `CairoHintProcessor.user_args`, not on the stack.
3. `prover_input.public_segment_context = PublicSegmentContext::new(&entrypoint.builtins)`.

and the one thing to add:

4. A positive control — a witness that must be **accepted**.

Keep this document and Stoolap's copy in sync; the two `PublicSegmentContext`
failure modes are the parts most likely to be re-derived expensively.

---

## 8b. What this proof actually asserts — read before relying on it

This recipe makes a **real** STARK. It does not make the circuit **sound**.
Those are different properties and only the first one is fixed here.

The proof says: _"there exists a witness `w` such that running the capability
circuit with public inputs `P` and witness `w` succeeds, in the Cairo VM."_ That
is genuine, unforgeable, and verified by
`cairo_air::verifier::verify_cairo::<Blake2sMerkleChannel>`.

What it does **not** say is that `w` is the _right_ witness. A STARK commits to
its public inputs; anything you pass as **witness** is invisible to the
transcript. Concretely, in the capability circuit:

| Value                                                                                                        | Kind                 | Committed by the STARK? |
| ------------------------------------------------------------------------------------------------------------ | -------------------- | ----------------------- |
| `ask_id`, `axes_count`, `cap_root_hash`, `invocation_hash`, `holder_did`, `current_unix_time`, `output_hash` | **public**           | **yes**                 |
| `cap_root_secret`, `caveat_felts`, `steps_flat`                                                              | witness              | **no**                  |
| `chain_commitment = Poseidon(secret_lo, secret_hi, *caveat_felts)`                                           | derived from witness | **no**                  |

`stage2_main` asserts only `chain_commitment != 0`. It never relates that
commitment to the public `cap_root_hash`. So an unrelated secret and unrelated
caveat felts produce an equally valid proof — asserted by
`harness_accepts_an_unrelated_secret_because_the_circuit_does_not_bind_it` in
`crates/zk-vendor/stwo-sys/src/lib.rs`, which is written to fail when the real
binding lands.

The one live public↔witness binding today is
`trace_root == pub_inputs.output_hash`, and only when `has_output_hash == 1`.

**The practical rule:** to make a proof mean something, the thing you care
about must be a **public input**. Moving a value from witness to public inputs
is a circuit ABI change, not a harness change.

---

## 9. Upstream references

| What                                                | Where                                                                 |
| --------------------------------------------------- | --------------------------------------------------------------------- |
| Wrapper header `ap += N; call rel 4; jmp rel 0`     | `cairo-lang-executable` 2.20.0 `src/executable.rs`, `Executable::new` |
| `NOT_RETURNING_HEADER_SIZE = 6`                     | same file                                                             |
| `find_executable_functions` (requires the raw attr) | `src/compile.rs:159-176`                                              |
| `EntryCodeConfig::executable`                       | `src/compile.rs:250`, via `RunnableBuilder`                           |
| `extract_public_segments` (stack-based range read)  | `stwo-cairo-prover` `src/witness/cairo.rs:19-63`                      |
| `create_cairo_claim_generator`                      | same file, from line 107                                              |
| `PublicSegmentContext::new` / `bootloader_context`  | `stwo-cairo-adapter` `src/lib.rs:51-77`                               |
| `adapt()` + its `TODO(spapini)`                     | `stwo-cairo-adapter` `src/adapter.rs:11-53`                           |
| Reference runner                                    | `stwo-cairo-dev-utils` `src/vm_utils.rs`, `run_and_adapt`             |
| `ArraySerde`                                        | corelib `core/src/array.cairo:305-333`                                |
| Canonical proof-mode stack prefix                   | cairo-vm `vm/runners/cairo_runner.rs`, `initialize_main_entrypoint`   |
| `ORDERED_BUILTIN_LIST` (11 names)                   | cairo-vm `src/vm/runners/cairo_runner.rs:65-77`                       |
| No `Panic` error variant                            | cairo-vm `src/vm/errors/vm_errors.rs`                                 |
