//! Guard: corelib's SHA-512 can be enabled in the circuit, and enabling it
//! still buys nothing.
//!
//! This module exists so the finding is not re-litigated either way. The
//! mechanical question has a clean yes. The soundness question has a clean no,
//! and the no does not depend on the yes.
//!
//! **It works.** `ExecutableConfig { allow_syscalls: true, .. }` lifts
//! `compile_executable`'s refusal, corelib SHA-512 compiles into the circuit,
//! `cairo-lang-runner`'s `CairoHintProcessor` services the call through the
//! real Starknet syscall handler, and a full STARK prove-and-verify passes with
//! the syscall in the program — 434 s, tampered-proof rejection included. So
//! "can we turn it on" was the wrong thing to be uncertain about.
//!
//! **It proves nothing anyway.** The syscall does not lower to an instruction.
//! `build_syscalls` emits:
//!
//! ```cairo
//! hint StarknetHint::SystemCall { system: original_system };
//! let updated_gas_builtin = *(system++);
//! let response = *(system++);   // the digest comes back out of memory here
//! ```
//!
//! The digest is written into memory by a host-side Pythonic hint and read back
//! under a plain `assert`. That assert is the only constraint on it, and it
//! constrains the read to match the write the hint performed. The value is
//! therefore whatever the prover's handler returned.
//!
//! The proof statement cannot repair this. `ProverInput` is
//! `{ state_transitions, memory, pc_count, public_memory_addresses,
//! builtin_segments, public_segment_context }` — there is no hint field
//! anywhere in it. The verifier commits to the trace and the memory and never
//! sees which handler produced a cell.
//!
//! **Cost, for the record.** A one-byte message (two blocks) took the circuit
//! from 3,004 to 5,107 CASM words, so about 1,052 words per block. The real
//! challenge input `R || A || M` is roughly 200 bytes, so about 4,200 words.
//! That buys a value the prover chose.
//!
//! **And it is redundant anyway.** The challenge hash does not belong in the
//! circuit at all. `h = SHA-512(R || A || M) mod L` is public in Ed25519, so
//! the verifier recomputes it in Rust and passes it in; `verify_with_challenge`
//! in `ed25519.cairo` does that and the circuit only range-checks `h < L`. The
//! syscall path would re-derive a number the verifier already had, at a cost
//! the direct path does not have.
//!
//! **Not all corelib hashes are affected, and the split is the useful part.**
//! Every non-Poseidon hash is a syscall — `Keccak`, `Sha256ProcessBlock` and
//! `Sha512ProcessBlock` all lower through `build_syscalls`. Poseidon lowers to
//! `build_poseidon_permutation` and Pedersen likewise: pure CASM, no hint,
//! no syscall. That is why the committed chain already hashes with Poseidon
//! under `allow_syscalls: false`, and it is the reason Poseidon is the right
//! primitive to reach for rather than a syscall-backed hash.

#[cfg(test)]
mod tests {
    use core::sha512::compute_sha512_byte_array;

    /// FIPS 180-4 SHA-512("abc"). Passes, and would pass in any test runner
    /// that services syscalls — which is why this test is not evidence that the
    /// primitive is usable in-circuit.
    #[test]
    #[available_gas(20000000)]
    fn corelib_sha512_is_correct_under_cairo_test() {
        let [w0, w1, w2, w3, w4, w5, w6, w7] = compute_sha512_byte_array(@"abc");
        assert!(w0 == 0xddaf35a193617aba, "w0 mismatch");
        assert!(w1 == 0xcc417349ae204131, "w1 mismatch");
        assert!(w2 == 0x12e6fa4e89a97ea2, "w2 mismatch");
        assert!(w3 == 0x0a9eeee64b55d39a, "w3 mismatch");
        assert!(w4 == 0x2192992a274fc1a8, "w4 mismatch");
        assert!(w5 == 0x36ba3c23a3feebbd, "w5 mismatch");
        assert!(w6 == 0x454d4423643ce80e, "w6 mismatch");
        assert!(w7 == 0x2a9ac94fa54ca49f, "w7 mismatch");
    }

    /// The primitives a hand-written SHA-512 would need are all present:
    /// `u64` carries a native `BitXor`, and a widening add with carry is one
    /// `u256` add plus a comparison. So the real question for in-circuit
    /// SHA-512 was cost, never feasibility.
    #[test]
    fn the_primitives_for_a_handwritten_sha512_are_available() {
        let a: u64 = 0xf0f0;
        let b: u64 = 0x0ff0;
        assert!(a ^ b == 0xff00, "u64 native xor");

        let aw: u256 = 0xffff_ffff_ffff_ffff_u64.into();
        let bw: u256 = 2_u64.into();
        let s = aw + bw;
        let carry = s.low / 0x10000000000000000;
        let sum: u64 = (s.low % 0x10000000000000000).try_into().unwrap();
        assert!(sum == 1, "low sum mismatch");
        assert!(carry == 1, "carry mismatch");
    }
}