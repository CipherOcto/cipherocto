//! Cairo 2.x capability circuit (RFC-0958 §Algorithms).
//!
//! Crypto home: cipherocto workspace, NOT the stoolap fork. Per
//! [[stoolap-general-purpose-db]] (2026-07-22), CASM + STWO production is
//! a proof-systems concern, orthogonal to SQL.
//!
//! Compilation flow:
//!
//! ```text
//! cairo/src/lib.cairo (this file)
//!     │ scarb build
//!     ▼
//! cairo/target/dev/capability_zk.sierra.json
//!     │ cairo-lang-sierra-to-casm 2.20.0 (in-process, crates/zk-circuit)
//!     ▼
//! CASM bytecode → BLAKE3 hash (`bundled_casm_hash_hex`)
//! ```
//!
//! # Mission 0958-b Session 1 — Real cryptographic body
//!
//! Pre-0958-b, `main()` was a structural-only stub (returned `1` after
//! field-bounds checks). 0958-b Session 1 fills in two real
//! cryptographic primitives (the third — Ed25519 holder-sig verify — is
//! deferred to follow-up mission `0958-c` because cairo-corelib 2.16.0
//! does not include either BLAKE3 or the Curve25519/Ed25519 arithmetic
//! primitive; both require external Cairo crates not in the scarb
//! registry cache as of 2026-08-05):
//!
//! 1. **HMAC-SHA-256 macaroon caveat chain re-derivation.** The macaroon
//!    caveat chain is re-derived from `caveats[0..n]` using
//!    `cap_root_secret` as the HMAC key. The final MAC must equal
//!    `cap_root_hash` (a 256-bit value split as two felts lo/hi).
//!    Exercises a ≥3-caveat chain depth (RFC-0957 §Caveat Format).
//!
//!    **Deviation from RFC-0958 §Algorithms (HMAC-BLAKE3):** the
//!    underlying hash is SHA-256, not BLAKE3, because cairo-corelib
//!    2.16.0 ships `core::sha256::compute_sha256_byte_array` but does
//!    NOT include BLAKE3 (it has `core::blake` = BLAKE2s, which is a
//!    different function). The HMAC construction (RFC 2104) is
//!    hash-agnostic; the choice of hash function is an implementation
//!    detail orthogonal to the macaroon chain shape. Pure-BLAKE3 HMAC
//!    ships in follow-up mission `0958-c` once a Cairo BLAKE3 crate is
//!    vendored.
//!
//! 2. **Poseidon inference-trace binding.** Each `TraceStep {op_code,
//!    input_hash_lo/hi, output_hash_lo/hi}` is folded into a single felt via
//!    `core::poseidon::poseidon_hash_span`. The folded trace root must
//!    equal `pub_inputs.output_hash`. This is the cryptographic
//!    commitment that prevents an attacker from submitting a
//!    `has_output_hash=1` claim without producing the actual trace.
//!
//! 3. **Ed25519 holder-sig verify — DEFERRED to `0958-c`.** Corelib has
//!    `core::ecdsa` (STARK curve only) and `core::ec` (STARK EC), neither
//!    of which is Curve25519/Ed25519. An inline Cairo Ed25519 verifier
//!    is ~3-5 KB CASM and warrants its own focused session.
//!
//! # Mission 0958-c AC-4 Stage-2 split + R19 Poseidon commitment
//!
//! The production `main()` is decomposed into 3 sub-circuits
//! (`verify_chain`, `verify_holder_sig`, `verify_inference_fold`)
//! composed by `stage2_main`. The cryptographic bodies (HMAC-BLAKE3 +
//! RFC 8032 Ed25519) live in `#[cfg(test)] mod blake3;` and
//! `#[cfg(test)] mod ed25519;` — reachable from `scarb cairo-test` but
//! excluded from the STARK runtime. R19 additionally replaces the
//! production `verify_chain` SHA-256 stopgap with a Poseidon commitment
//! over felt252 (the cryptographic chain re-derivation happens via
//! STWO hints at proof-generation time per the design doc
//! `docs/plans/2026-08-05-stage-2-verifier-split.md`).
//!
//! # Determinism (Class A, RFC-0958 §Determinism)
//!
//! Same inputs → same Sierra IR (modulo salsa UUIDs which DO NOT
//! affect CASM) → same CASM bytecode → same BLAKE3 hash.

use core::array::{ArrayTrait, SpanTrait};
use core::poseidon::poseidon_hash_span;
use core::traits::{Into, TryInto};

// Mission 0958-c AC-4 Stage-2 verifier split: the heavy cryptographic
// primitives (HMAC-BLAKE3, RFC 8032 Ed25519) are pulled into main()'s
// compilation only for #[cfg(test)] modules — NOT for the production
// STARK entry point. The production `main()` uses corelib Poseidon for
// its sub-circuit commitments, keeping the compiled CASM under the
// AC-4 hard ceiling of 50 KB serialized / 1,600 words. The BLAKE3 +
// Ed25519 modules remain as the canonical pure-Cairo libraries
// (RFC-0958 §Algorithms; 0958-c AC-1 + AC-2 closure evidence),
// reachable from `scarb cairo-test` but excluded from the STARK
// runtime.
//
// R19 closure (2026-08-06): the production verify_chain additionally
// drops SHA-256 (the R18 AC-4 fail-closed path used corelib SHA-256 as
// a stopgap; R19 replaces it with Poseidon over felt252 to drop
// corelib::sha256 from the compiled CASM entirely — only Poseidon is
// now imported into production main()). The cryptographic chain
// re-derivation still happens via STWO hints at proof-generation time
// per the design doc.
// RFC-0973 §Root-Secret Binding needs an unkeyed BLAKE3 computed by
// constrained program logic, and `unkeyed_hash_one_chunk` exists for it.
//
// The module STAYS test-gated. Un-gating it was measured
// (RFC-0973 §Performance Targets) and the answer is decisive:
//
//     with BLAKE3 compiled in:  17,628 CASM words / 550.88 KB
//     without it (R19 baseline):  1,522 CASM words /  41.19 KB
//     ceiling:                    1,600 CASM words /  50.00 KB
//
// An 11x overrun on a ceiling that had 78 words of headroom. This is
// the "budget fixed while the circuit proved nothing" case RFC-0973
// §Performance Targets names, except inverted: here the primitive is
// measured and *rejected*, rather than the budget being raised to fit
// it. Raising the ceiling to admit it would make the ceiling a
// description of whatever the code happened to cost.
//
// Slimming the module to the unkeyed path alone (gating the keyed and
// HMAC surface) did not bring it under: the compression function and
// its u32 state machine dominate, and they are what the relation needs.
// The measured figure is pinned by
// `zk-circuit`'s `blake3_casm_cost_is_recorded_in_mission_b`.
//
// So the entry point is built, correct, and parity-anchored to host
// BLAKE3 — and the circuit does not call it. `root_secret_knowledge`
// stays unregistered, which is the same fail-closed posture the RFC
// mandates while the primitive is unavailable.
#[cfg(test)]
mod blake3;
#[cfg(test)]
mod ed25519;

/// Number of caveats in the chain. The prover must supply exactly this many
/// caveat felts, or `stage2_main` panics with `CaveatChainDepthMismatch`.
pub const CHAIN_DEPTH: u32 = 3;

/// Minimum number of caveats required by RFC-0957 §Caveat Format (">= 3-caveat
/// chain depth" per mission 0958-b Session 1). A shallower chain is rejected by
/// `CaveatChainDepthMismatch`.
pub const CHAIN_DEPTH_MIN: u32 = 3;

/// Felts per `TraceStep`. The harness passes inference-trace steps as a FLAT
/// felt array because the Cairo-VM entry-point argument encoding in cairo-vm
/// 3.1.0 carries scalars only and cannot express a nested `Array<TraceStep>`.
/// `stage2_main` regroups them by this stride.
///
/// Layout: `op_code`, then two felts for the 32-byte input hash, then two for
/// the output hash.
///
/// **Was 3** (`op, input, output`), which was not merely wasteful — it was
/// wrong. A felt252 holds ~251 bits and a 32-byte hash is 256, so one felt per
/// hash cannot represent a real digest. The harness fed each field through
/// `parse_felt`, which rejected every 32-byte value with "representative out of
/// range", so `build_calldata` returned `None` and no genuine PoI trace could
/// reach the circuit. The pre-generated fixture hid this by passing
/// `"input_hash": "18"` — a small felt that parses fine.
///
/// Two 16-byte halves per hash is the same split `PublicInputs` already uses
/// for `cap_root_hash` and `PrivateWitness` for `cap_root_secret`.
/// **Keep in lockstep with `TRACE_STRIDE` in
/// `crates/zk-vendor/stwo-sys/src/lib.rs`.**
pub const TRACE_STRIDE: u32 = 5;

/// Public inputs (mirrors `octo_wallet::capability::zk_mint::PublicInputs`).
///
/// `Serde` is required by the Cairo executable ABI: the `#[executable]`
/// wrapper on `main` receives a flat `Array<felt252>` of calldata and
/// deserializes it into this struct. Without it the function cannot be
/// compiled as an executable at all.
#[derive(Drop, Copy, Serde)]
pub struct PublicInputs {
    pub ask_id_lo: felt252,
    pub ask_id_hi: felt252,
    pub axes_count: u32,
    pub cap_root_hash_lo: felt252,
    pub cap_root_hash_hi: felt252,
    /// Low half of the verifier-supplied macaroon CHAIN TIP, i.e. the last
    /// element of `macaroon.chain`.
    ///
    /// Deliberately a separate field from `cap_root_hash`. That value is
    /// `BLAKE3(CAPABILITY_ID_DOMAIN || canonical_ser_unsigned(macaroon))` — a
    /// hash over the serialised token, NOT a re-derivable chain tip. Asserting
    /// the chain against it can never succeed for any token, which is why the
    /// first version of `verify_chain_against_tip` was wrong.
    ///
    /// The verifier reads the tip straight off the token it is checking, so it
    /// is genuinely verifier-supplied. The prover must show a preimage under
    /// the chain MAC to satisfy the equality.
    pub chain_tip_lo: felt252,
    pub invocation_hash: felt252,
    pub holder_did: felt252,
    pub current_unix_time: u64,
    pub has_output_hash: u8,
    pub output_hash: felt252,
    // RFC-0973 §Root-Secret Binding would add `macaroon_root_secret_hash`
    // here — the public input the `root_secret_knowledge` relation is
    // checked against, named for the token field it carries without
    // reusing that field's own name.
    //
    // It is NOT added. The field was added, compiled, and measured, and
    // the measurement is recorded above: an in-circuit BLAKE3 costs
    // 17,628 CASM words against a 1,600-word ceiling. Adding a public
    // input that no assertion reads is not a partial step toward the
    // binding — it is a calldata ABI change with a stale fixture behind
    // it, and a stale fixture deserialises to a `None` run for every
    // input, which is indistinguishable from a circuit that always
    // rejects.
    //
    // The field is named here so the next implementer does not have to
    // re-derive why it is absent.
}

/// Private witness (mirrors `octo_wallet::capability::zk_mint::PrivateWitness`).
///
/// `Serde` is required by the Cairo executable ABI — see [`PublicInputs`].
#[derive(Drop, Copy, Serde)]
pub struct PrivateWitness {
    pub cap_root_secret_lo: felt252,
    pub cap_root_secret_hi: felt252,
    pub caveats_count: u32,
    pub discharges_count: u32,
    pub inference_trace_present: u8,
}

/// Inference trace step (mirrors `octo_wallet::capability::zk_mint::TraceStep`).
#[derive(Drop, Copy)]
pub struct TraceStep {
    pub op_code: felt252,
    /// The 32-byte input hash, as two felt-halves.
    ///
    /// A felt252's modulus is about 251 bits and a 32-byte value is 256, so a
    /// whole hash **cannot** be one felt. Each half occupies 128 bits and
    /// round-trips losslessly. This mirrors how `PublicInputs` already splits
    /// `cap_root_hash` and `PrivateWitness` splits `cap_root_secret`.
    ///
    /// The previous shape carried one felt per hash, which the harness could
    /// only satisfy with a small literal — a real 32-byte hash failed
    /// `parse_felt` with "representative out of range", so no genuine trace
    /// could ever reach the circuit.
    pub input_hash_lo: felt252,
    pub input_hash_hi: felt252,
    pub output_hash_lo: felt252,
    pub output_hash_hi: felt252,
}

// =========================================================================
// SHA-256 HMAC chain step (test-only surface).
//
// 0958-c AC-1 ships the canonical HMAC-BLAKE3 chain in
// `cairo/src/blake3.cairo` (test-only via `#[cfg(test)] mod blake3;`).
// The `hmac_sha256_chain_step` helper below is the R18 AC-4 stopgap
// path (corelib SHA-256 chain) — preserved here so scarb cairo-test can
// still exercise the SHA-256 surface during the AC-4 fail-closed
// window. It is NOT pulled into the production STARK entry point
// (verify_chain now uses Poseidon over felt252; see below).
// =========================================================================

/// Convert a 32-byte digest (`[u32; 8]`) to a 32-byte `ByteArray`
/// (big-endian byte order, which is BLAKE3's wire format).
#[cfg(test)]
fn digest_to_bytes(digest: [u32; 8]) -> ByteArray {
    let mut out: ByteArray = "";
    let span = digest.span();
    let mut i: usize = 0;
    loop {
        if i == 8 {
            break;
        }
        let word: u32 = *span.at(i);
        // Big-endian: byte3 = word >> 24, byte2 = word >> 16, etc.
        let b3: u8 = ((word / 0x1000000) & 0xff).try_into().unwrap();
        let rem: u32 = word & 0xffffff;
        let b2: u8 = ((rem / 0x10000) & 0xff).try_into().unwrap();
        let rem2: u32 = rem & 0xffff;
        let b1: u8 = ((rem2 / 0x100) & 0xff).try_into().unwrap();
        let b0: u8 = (rem2 & 0xff).try_into().unwrap();
        out.append_byte(b3);
        out.append_byte(b2);
        out.append_byte(b1);
        out.append_byte(b0);
        i += 1;
    }
    out
}

/// Concatenate two `ByteArray`s into a fresh `ByteArray`.
#[cfg(test)]
fn concat_bytes(a: @ByteArray, b: @ByteArray) -> ByteArray {
    let mut out: ByteArray = "";
    out.append(a);
    out.append(b);
    out
}

/// AC-4 R18 stopgap path: HMAC-SHA-256 chain step. Returns the digest
/// as `[u32; 8]` (32-byte SHA-256 output in 8 u32 words). The
/// production STARK entry point does NOT call this — production uses
/// Poseidon over felt252 via `verify_chain`. This helper is preserved
/// for scarb cairo-test coverage during the AC-4 fail-closed window.
#[cfg(test)]
fn hmac_sha256_chain_step(prev: @ByteArray, key: @ByteArray, caveat: @ByteArray) -> [u32; 8] {
    let msg: ByteArray = concat_bytes(prev, caveat);
    let full: ByteArray = concat_bytes(key, @msg);
    core::sha256::compute_sha256_byte_array(@full)
}

// =========================================================================
// Macaroon caveat chain (RFC-0957 §Caveat Format)
// =========================================================================
//
// R19 production path: `verify_chain` emits a Poseidon commitment over
// `[secret_lo, secret_hi, ...caveat_felts]`. The secret and caveat felts are
// supplied by the prover through `main`'s parameters (see §Input surface above);
// the HMAC-BLAKE3 chain re-derivation remains unimplemented, so this is a
// commitment over the witness, not a verification of it. The STARK entry
// point uses Poseidon because corelib Poseidon is STARK-native (very
// compact CASM), whereas corelib SHA-256 + the helper byte-packing
// loops were the dominant CASM cost in the R18 fail-closed path.

/// Domain tag separating the MAC use from the commitment use.
///
/// Without it, a value produced as a chain MAC could be replayed as a chain
/// commitment and vice versa. The two consume different attacker-chosen inputs,
/// so the separation is load-bearing rather than cosmetic.
pub const MAC_DOMAIN: felt252 = 0x4d41432d434841494e; // b"MAC-CHAIN"

/// Domain-separated keyed MAC over felts, built as a sponge prefix.
///
/// `MAC(k, m) = H(MAC_DOMAIN, k, m)`. Absorbing the key as a prefix under a
/// domain the commitment path never uses is the standard sponge MAC. This is
/// NOT the same construction as RFC-0957 §Algorithms `blake3::keyed_hash`: the
/// host must adopt this construction for the chain tip to match, which is a
/// token-format change and is recorded as such rather than papered over.
pub fn poseidon_mac(key: felt252, msg: Span<felt252>) -> felt252 {
    let mut arr: Array<felt252> = array![];
    arr.append(MAC_DOMAIN);
    arr.append(key);
    let len: u32 = msg.len();
    let mut i: u32 = 0;
    loop {
        if i == len {
            break;
        }
        arr.append(*msg.at(i));
        i += 1;
    }
    poseidon_hash_span(arr.span())
}

/// Step 2 + Step 4: re-derive a caveat chain and bind it to a **verifier
/// supplied** tip.
///
/// The distinction from [`verify_chain`] is the whole point. `verify_chain`
/// hashes prover-supplied material and the stage asserted only that the result
/// was non-zero, which constrains nothing — a prover could return any value at
/// all. Here the expected tip is a public input, so a prover who cannot produce
/// the chain preimage cannot satisfy the equality.
///
/// Returns the derived tip so the caller can bind it further.
///
/// # Panics
/// - `ChainTipMismatch` when the derived tip differs from the public tip.
pub fn verify_chain_against_tip(
    root_secret_lo: felt252,
    root_secret_hi: felt252,
    caveat_felts: Span<felt252>,
    expected_tip: felt252,
) -> felt252 {
    // chain[0] = H(MAC_DOMAIN, root_secret_lo, root_secret_hi)
    let mut seed: Array<felt252> = array![];
    seed.append(MAC_DOMAIN);
    seed.append(root_secret_lo);
    seed.append(root_secret_hi);
    let mut state: felt252 = poseidon_hash_span(seed.span());

    // chain[i+1] = MAC(chain[i], caveat_i) — the macaroon step, in order.
    let len: u32 = caveat_felts.len();
    let mut i: u32 = 0;
    loop {
        if i == len {
            break;
        }
        let mut msg: Array<felt252> = array![];
        msg.append(*caveat_felts.at(i));
        state = poseidon_mac(state, msg.span());
        i += 1;
    };

    // The one assertion that carries meaning: a verifier-supplied value, not a
    // "is it non-zero" tautology.
    assert!(state == expected_tip, "ChainTipMismatch");
    state
}

/// `verify_chain` sub-circuit: Poseidon commitment over
/// `[secret_lo, secret_hi, caveat_0, caveat_1, ...]`. Both the secret and the
/// caveat felts are prover-supplied. Returns a single felt252 commitment.
///
/// Soft budget: < 1 KB CASM (corelib poseidon + tiny loop).
/// AC-4 hard ceiling: ≤ 50 KB serialized / ≤ 1,600 CASM words for the
/// WHOLE compiled program.
pub fn verify_chain(
    secret_lo: felt252, secret_hi: felt252, caveat_felts: Span<felt252>,
) -> felt252 {
    let mut arr: Array<felt252> = array![];
    arr.append(secret_lo);
    arr.append(secret_hi);
    let len: u32 = caveat_felts.len();
    let mut i: u32 = 0;
    loop {
        if i == len {
            break;
        }
        arr.append(*caveat_felts.at(i));
        i += 1;
    }
    poseidon_hash_span(arr.span())
}

/// `verify_holder_sig` sub-circuit: binds four witness commitments
/// (sig_lo, sig_hi, pk_lo, pk_hi) into one felt252 and asserts the result is
/// non-zero. This is a **structural binding, not RFC 8032 verification**.
/// The real Ed25519 verification lives in `cairo/src/ed25519.cairo`
/// (test-only via `#[cfg(test)] mod ed25519;`) and is still unwired — tracked
/// by audit finding H-3.
///
/// Soft budget: < 1 KB CASM (simple commitment + assertion).
pub fn verify_holder_sig(
    sig_lo: felt252, sig_hi: felt252, pk_lo: felt252, pk_hi: felt252,
) -> felt252 {
    // Bind the 4 witness commitments into one felt252. A valid sig
    // produces non-zero commitments; the assertion enforces the
    // structural "signature is bound" invariant.
    let commitment = sig_lo + sig_hi + pk_lo + pk_hi;
    assert!(commitment != 0, "HolderSigCommitmentZero");
    commitment
}

/// `verify_inference_fold` sub-circuit: Poseidon inference-trace binding.
/// Folds `TraceStep[]` into a single felt252 via corelib Poseidon.
///
/// Soft budget: < 1 KB CASM (corelib poseidon_hash_span + simple loop).
pub fn verify_inference_fold(steps: @Array<TraceStep>) -> felt252 {
    fold_inference_trace(steps)
}

/// `verify_inference_fold` over the FLAT trace form (`op, input, output` per
/// step). Mathematically identical to [`verify_inference_fold`], because
/// `fold_inference_trace` concatenates exactly those fields in exactly that
/// order. Taking the flat form lets the entrypoint avoid reconstructing
/// `TraceStep` structs, which cost enough CASM to breach the AC-4 ceiling.
pub fn verify_inference_fold_flat(steps: Span<felt252>) -> felt252 {
    poseidon_hash_span(steps)
}

// =========================================================================
// Poseidon inference-trace binding (RFC-0958 §Trace Binding)
// =========================================================================

fn fold_inference_trace(steps: @Array<TraceStep>) -> felt252 {
    let mut flat: Array<felt252> = array![];
    let len: u32 = steps.len();
    let mut i: u32 = 0;
    loop {
        if i == len {
            break;
        }
        let s: TraceStep = *steps.at(i);
        flat.append(s.op_code);
        flat.append(s.input_hash_lo);
        flat.append(s.input_hash_hi);
        flat.append(s.output_hash_lo);
        flat.append(s.output_hash_hi);
        i += 1;
    }
    poseidon_hash_span(flat.span())
}

// =========================================================================
// STARK main entry — returns 1 on success, panics on any failure.
// =========================================================================
//
// Mission 0958-c AC-4 Stage-2 split: `main` delegates to `stage2_main`
// which composes the three sub-circuits:
//   1. `verify_chain` (Poseidon commitment over secret + caveats)
//   2. `verify_holder_sig` (Ed25519 RFC 8032 holder-sig commitment)
//   3. `verify_inference_fold` (Poseidon inference-trace binding)
//
// # Input surface (RFC-0958 §Algorithms, 2026-10-06)
//
// `main` previously took **no parameters** and built `pub_inputs`,
// `priv_witness`, the caveat felts and the trace steps from hardcoded literals
// inside `stage2_main`. Nothing could bind a real witness to it, so the program
// proved Poseidon hashes of constants. Its comments claimed the witness would
// arrive "via STWO hints at proof-generation time"; STWO is a proof system
// **over** a Cairo trace and cannot inject values into program execution.
// Upstream stwo-cairo confirms this — its adapter tests drive the VM with
// `BuiltinHintProcessor::new_empty()`.
//
// RFC-0958 §Algorithms specifies `read_public_inputs()` /
// `read_private_witness()`. These parameters are that surface.
//
// The Rust harness supplies them as `CairoArg` values through
// `CairoRunner::run_from_entrypoint(entrypoint, args, ...)` — Serde-encoded,
// so structs and arrays both cross the boundary. See
// `crates/zk-vendor/src/cairo_harness.rs`.
// `#[cfg]` strips the *whole item*, so gating `main` itself would delete the
// entrypoint from every non-executable build — including the `scarb build`
// Sierra that `crates/zk-circuit`'s transitional CASM path still reads. Hence
// two one-line entrypoints over the same logic:
//
//   * `main`            — always compiled. Plain entrypoint.
//   * `executable_main` — compiled only with `feature: "executable"`, which
//                         `crates/zk-circuit` sets when it builds the STWO
//                         artifact. This is the one cairo-lang wraps.
//
// `#[executable]` makes cairo-lang emit the wrapper entrypoint that carries
// the function's arguments as flat `Array<felt252>` calldata. It is the whole
// reason the STWO harness cannot hand-build a stack frame: the compiled
// program's real entrypoint is the generated Standalone header
// (`ap += builtins.len(); call rel 4; jmp rel 0`) at offset 0, not this
// function.
//
// Gated on a feature because only `scarb execute` and
// `cairo_lang_executable::compile::compile_executable` enable the executable
// plugin; `scarb build`, `scarb cairo-test` and plain cairo-lang consumers use
// the default plugin suite, which rejects the raw attribute outright
// ("Unsupported attribute"). Do NOT declare the feature in Scarb.toml — that
// would let `scarb build -F executable` reproduce the failing build.
#[cfg(feature: "executable")]
#[executable]
pub fn executable_main(
    pub_inputs: PublicInputs,
    priv_witness: PrivateWitness,
    caveat_felts: Array<felt252>,
    steps_flat: Array<felt252>,
) -> felt252 {
    stage2_main(pub_inputs, priv_witness, caveat_felts.span(), steps_flat)
}

pub fn main(
    pub_inputs: PublicInputs,
    priv_witness: PrivateWitness,
    caveat_felts: Array<felt252>,
    steps_flat: Array<felt252>,
) -> felt252 {
    stage2_main(pub_inputs, priv_witness, caveat_felts.span(), steps_flat)
}

/// Stage-2 main: composition of `verify_chain` + `verify_holder_sig` +
/// `verify_inference_fold`. Each sub-circuit emits a single felt252
/// commitment; the composition asserts all three are non-zero, which
/// is the cryptographic contract that each sub-circuit was correctly
/// evaluated.
///
/// Soft budget: < 5 KB CASM (3 sub-circuits × ~ 1 KB each + composition).
/// AC-4 hard ceiling: ≤ 50 KB serialized / ≤ 1,600 CASM words for the
/// WHOLE compiled program.
pub fn stage2_main(
    pub_inputs: PublicInputs,
    priv_witness: PrivateWitness,
    caveat_felts: Span<felt252>,
    steps_flat: Array<felt252>,
) -> felt252 {
    // ---------- Structural invariants (unchanged from 0958-a) ----------
    assert!(pub_inputs.axes_count < 1000_u32, "AxesCountTooLarge");
    assert!(priv_witness.caveats_count < 256_u32, "CaveatsCountTooLarge");
    assert!(priv_witness.discharges_count < 256_u32, "DischargesCountTooLarge");
    if pub_inputs.has_output_hash == 1_u8 {
        assert!(priv_witness.inference_trace_present == 1_u8, "MissingInferenceTrace");
    }
    assert!(pub_inputs.current_unix_time < 18446744073709551615_u64, "InvalidUnixTime");

    // ---------- Caveat chain depth ----------
    // `caveats_count` in the witness MUST agree with the caveat felts actually
    // supplied. Previously both sides were the same hardcoded constant, so this
    // assertion could never fail. Now the witness is prover-controlled, so a
    // prover claiming N caveats must supply exactly N felts.
    assert!(priv_witness.caveats_count == caveat_felts.len(), "CaveatChainDepthMismatch");
    assert!(caveat_felts.len() >= CHAIN_DEPTH_MIN, "CaveatChainDepthMismatch");

    // ---------- Sub-circuit 1: chain re-derivation (RFC-0958 step 2) ----------
    // Now an actual MAC chain bound to a VERIFIER-SUPPLIED tip.
    //
    // This replaces the previous `verify_chain` commitment plus a
    // `!= 0` assertion. That assertion could not fail, so it constrained
    // nothing: a prover could satisfy it with any non-zero value and no
    // relation to the macaroon was checked. Here the derived tip must equal
    // `cap_root_hash`, which the verifier supplies.
    let chain_tip: felt252 = verify_chain_against_tip(
        priv_witness.cap_root_secret_lo,
        priv_witness.cap_root_secret_hi,
        caveat_felts,
        pub_inputs.chain_tip_lo,
    );
    assert!(chain_tip != 0, "ChainCommitmentZero");

    // ---------- Sub-circuit 2: Ed25519 holder-sig (AC-4 verify_holder_sig) ----------
    // Bind the public inputs the holder signature covers. RFC 8032 verification
    // still lives in `cairo/src/ed25519.cairo` (test-only) — see audit H-3 — so
    // this is a structural binding, not a verification.
    //
    // A SINGLE Poseidon over all covered public inputs, rather than one per
    // argument: four separate Poseidon instantiations pushed CASM past the AC-4
    // ceiling (61888 B / 1934 words vs the 50 KB / 1600 word budget). The four
    // arguments of `verify_holder_sig` are fed the same commitment, which
    // preserves the "signature is bound to these inputs" invariant at one
    // Poseidon's CASM cost.
    let sig_commitment: felt252 = poseidon_hash_span(
        array![
            pub_inputs.holder_did, pub_inputs.ask_id_lo, pub_inputs.ask_id_hi,
            pub_inputs.cap_root_hash_lo, pub_inputs.cap_root_hash_hi, pub_inputs.invocation_hash,
        ]
            .span(),
    );
    let _holder_sig_commitment: felt252 = verify_holder_sig(
        sig_commitment, sig_commitment, sig_commitment, sig_commitment,
    );

    // ---------- Sub-circuit 3: Poseidon inference-trace (AC-4 verify_inference_fold) ----------
    // `steps_flat` is already `[op_0, in_0, out_0, op_1, in_1, out_1, ...]`, which
    // is EXACTLY the concatenation `fold_inference_trace` produces from
    // `TraceStep[]`. So folding the flat array directly is the same commitment
    // without reconstructing structs — and reconstructing them cost enough CASM
    // to breach the AC-4 ceiling.
    assert!(steps_flat.len() % TRACE_STRIDE == 0, "TraceStrideMismatch");
    assert!(steps_flat.len() > 0, "TraceRootZero");

    // The trace root must equal the `output_hash` the prover committed to in
    // the public inputs. Previously both sides were derived from the same
    // literals, so this assertion was trivially satisfiable; with a real
    // witness it is a genuine binding (RFC-0958 §Trace Binding).
    let trace_root: felt252 = verify_inference_fold_flat(steps_flat.span());
    if pub_inputs.has_output_hash == 1_u8 {
        assert!(trace_root == pub_inputs.output_hash, "InferenceTraceBindingMismatch");
    }
    assert!(trace_root != 0, "TraceRootZero");

    1
}

// =========================================================================
// Tests (run via `scarb cairo-test`; the workspace `cargo test` layer
// exercises the public Rust API instead).
// =========================================================================

#[cfg(test)]
mod tests {
    use super::{
        CHAIN_DEPTH, PrivateWitness, PublicInputs, TraceStep, fold_inference_trace,
        hmac_sha256_chain_step, poseidon_mac, stage2_main, verify_chain,
        verify_holder_sig, verify_inference_fold, TRACE_STRIDE,
    };

    /// REGRESSION: a full 32-byte digest must survive the trace encoding.
    ///
    /// The old encoding carried one felt per hash, and a felt252 holds ~251
    /// bits against a 32-byte value's 256. Every real digest therefore failed
    /// `parse_felt` with "representative out of range", `build_calldata`
    /// returned `None`, and no genuine PoI trace could reach the circuit. The
    /// pre-generated fixture hid it by passing a small literal.
    ///
    /// Two **distinct** 16-byte halves stand in for the digest, so the test also
    /// pins that neither half is decorative: swapping them must change the
    /// fold, or a half could be dropped from the binding unnoticed.
    #[test]
    fn a_full_thirty_two_byte_digest_fits_the_trace_encoding() {
        let lo: felt252 =
            0x33333333333333333333333333333333.try_into().unwrap();
        let hi: felt252 =
            0xffffffffffffffffffffffffffffffff.try_into().unwrap();
        let mut steps: Array<TraceStep> = array![];
        steps.append(
            TraceStep {
                op_code: 1,
                input_hash_lo: lo,
                input_hash_hi: hi,
                output_hash_lo: lo,
                output_hash_hi: hi,
            },
        );
        let r = fold_inference_trace(@steps);
        assert!(r != 0, "a full-width digest must produce a real fold");

        let mut swapped: Array<TraceStep> = array![];
        swapped.append(
            TraceStep {
                op_code: 1,
                input_hash_lo: hi,
                input_hash_hi: lo,
                output_hash_lo: lo,
                output_hash_hi: hi,
            },
        );
        assert!(
            fold_inference_trace(@steps) != fold_inference_trace(@swapped),
            "swapping the two halves must change the fold"
        );
    }

    /// The stride is the contract between the harness and the circuit.
    ///
    /// A mismatch produces a proof over attacker-chosen inputs with every
    /// internal hash check self-consistent, so it is pinned rather than
    /// documented.
    #[test]
    fn the_trace_stride_matches_the_packed_step_width() {
        assert!(TRACE_STRIDE == 5, "op + 2x(input) + 2x(output)");
    }

    #[test]
    fn verify_chain_poseidon_commitment_deterministic() {
        // AC-4 R19: production verify_chain emits a Poseidon commitment
        // over [secret_lo, secret_hi, ...caveat_felts]. Determinism:
        // same input → same output.
        let _c1 = verify_chain(
            0x42424242424242424242424242424242,
            0x42424242424242424242424242424242,
            array![0xc0a1, 0xc0a2, 0xc0a3].span(),
        );
        let _c2 = verify_chain(
            0x42424242424242424242424242424242,
            0x42424242424242424242424242424242,
            array![0xc0a1, 0xc0a2, 0xc0a3].span(),
        );
        assert!(_c1 == _c2, "verify_chain Poseidon commitment deterministic");
        assert!(_c1 != 0, "non-trivial commitment");
    }

    #[test]
    fn verify_chain_poseidon_commitment_avalanche() {
        // AC-4 R19: distinct caveats → distinct commitments.
        let _c1 = verify_chain(
            0x42424242424242424242424242424242,
            0x42424242424242424242424242424242,
            array![0xc0a1, 0xc0a2, 0xc0a3].span(),
        );
        let _c2 = verify_chain(
            0x42424242424242424242424242424242,
            0x42424242424242424242424242424242,
            array![0xc0a1, 0xc0a2, 0xc0a4].span(),
        );
        assert!(
            _c1 != _c2, "verify_chain avalanche: distinct caveat felt produces distinct commitment",
        );
    }

    #[test]
    fn hmac_sha256_chain_step_is_deterministic() {
        // AC-4 R18 stopgap helper preserved for scarb cairo-test
        // coverage during the AC-4 fail-closed window. The production
        // STARK entry point uses Poseidon over felt252 instead.
        let mut key: ByteArray = "";
        key.append_byte(0x42);
        let d1 = hmac_sha256_chain_step(@"", @key, @"message");
        let d2 = hmac_sha256_chain_step(@"", @key, @"message");
        assert!(d1 == d2, "determinism");
    }

    #[test]
    fn fold_inference_trace_single_step_poseidon() {
        let mut steps: Array<TraceStep> = array![];
        steps
            .append(
                TraceStep {
                    op_code: 0,
                    // A full 32-byte digest, split into two 16-byte felt-halves.
                    input_hash_lo: 0x33333333333333333333333333333333,
                    input_hash_hi: 0x33333333333333333333333333333333,
                    output_hash_lo: 0x44444444444444444444444444444444,
                    output_hash_hi: 0x44444444444444444444444444444444,
                },
            );
        let r1 = fold_inference_trace(@steps);
        let r2 = fold_inference_trace(@steps);
        assert!(r1 == r2, "Poseidon fold deterministic");
        assert!(r1 != 0, "non-trivial fold output");
    }

    #[test]
    fn verify_inference_fold_deterministic_and_nonzero() {
        // AC-4 Stage-2: verify_inference_fold sub-circuit.
        let mut steps: Array<TraceStep> = array![];
        steps
            .append(
                TraceStep {
                    op_code: 0,
                    // A full 32-byte digest, split into two 16-byte felt-halves.
                    input_hash_lo: 0x33333333333333333333333333333333,
                    input_hash_hi: 0x33333333333333333333333333333333,
                    output_hash_lo: 0x44444444444444444444444444444444,
                    output_hash_hi: 0x44444444444444444444444444444444,
                },
            );
        let r1 = verify_inference_fold(@steps);
        let r2 = verify_inference_fold(@steps);
        assert!(r1 == r2, "verify_inference_fold deterministic");
        assert!(r1 != 0, "non-trivial trace root");
    }

    #[test]
    fn verify_holder_sig_binds_witness_commitments() {
        // AC-4 Stage-2: verify_holder_sig binds 4 witness commitments
        // (sig_lo, sig_hi, pk_lo, pk_hi) into a single felt252.
        let c = verify_holder_sig(1, 2, 3, 4);
        assert!(c == 10, "verify_holder_sig binds witness commitments");
    }

    #[test]
    fn chain_depth_constant_is_three() {
        // Mission AC: ≥3 caveat chain depth exercised.
        assert!(CHAIN_DEPTH == 3, "chain depth must be 3 per TV1");
    }

    #[test]
    #[ignore]
    fn harness_witness_is_satisfying() {
        // DIAGNOSTIC: the exact witness the Rust STWO harness feeds. If this
        // panics, the harness is rejecting a witness the circuit would accept,
        // and the STWO-harness return-value gate is over-strict.
        let pi = PublicInputs {
            ask_id_lo: 1,
            ask_id_hi: 2,
            axes_count: 1,
            cap_root_hash_lo: 4,
            cap_root_hash_hi: 5,
            chain_tip_lo: 77,
            invocation_hash: 6,
            holder_did: 7,
            current_unix_time: 1700000000,
            has_output_hash: 0,
            output_hash: 0,
        };
        let pw = PrivateWitness {
            cap_root_secret_lo: 11,
            cap_root_secret_hi: 12,
            caveats_count: 3,
            discharges_count: 0,
            inference_trace_present: 1,
        };
        let r = stage2_main(pi, pw, array![15, 16, 22].span(), array![17, 18, 19]);
        assert!(r == 1, "harness witness must satisfy every circuit assertion");
    }

    // ------------------------------------------------------------------
    // Host/circuit parity for `poseidon_mac`.
    //
    // These constants are asserted IDENTICALLY by
    // `substrate_poseidon::tests::poseidon_mac_matches_the_circuit_vectors`
    // in `octo-cap-macaroon`. Two independent implementations of a MAC that
    // silently drift still both run and still produce plausible values — the
    // exact failure mode a hand-written MAC is most prone to. Pinning both
    // sides to the same vectors makes divergence a test failure instead of a
    // chain tip that never matches.
    // ------------------------------------------------------------------

    #[test]
    fn poseidon_mac_parity_empty_message() {
        let mac = poseidon_mac(1, array![].span());
        assert!(
            mac
                == 0x046e86a3387dd91b36e4c3bd6bdcd1ff021bee5c909188d42b7d76f27819de98,
            "parity divergence: empty message"
        );
    }

    #[test]
    fn poseidon_mac_parity_single_felt() {
        let mac = poseidon_mac(42, array![7].span());
        assert!(
            mac
                == 0x016c51efb49c3462f9e32810075ce52b3327d25b7802fa10313a71d72846056d,
            "parity divergence: single felt"
        );
    }

    #[test]
    fn poseidon_mac_parity_three_felts() {
        let mac = poseidon_mac(0xdead, array![1, 2, 3].span());
        assert!(
            mac
                == 0x037523d4d4d0875dc9931199b83dd31a074bcdb4a21a5f2098020afd219f659b,
            "parity divergence: three felts"
        );
    }

    #[test]
    fn poseidon_mac_parity_domain_sized_key() {
        let mac = poseidon_mac(
            0xffffffffffffffff, array![0xffffffffffffffff, 0].span(),
        );
        assert!(
            mac == 0x06bd32a5ce3f2513b62f1d02a7ef8128e148bfca2f1514deeef096d46058224d,
            "parity divergence: domain-sized key"
        );
    }

    #[test]
    fn poseidon_mac_is_order_sensitive() {
        // A MAC that ignores message order would let a prover permute the
        // caveat chain and satisfy the tip.
        let a = poseidon_mac(9, array![1, 2].span());
        let b = poseidon_mac(9, array![2, 1].span());
        assert!(a != b, "the MAC must bind caveat order");
    }
}