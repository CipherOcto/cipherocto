//! Guard: corelib's SHA-512 cannot be used inside the capability circuit.
//!
//! This module exists so the finding is not re-litigated. Every one of these
//! tests PASSES, which is the point: corelib SHA-512 works perfectly under
//! `scarb cairo-test`, and that green result is exactly what makes it dangerous.
//!
//! What `core::sha512::compute_sha512_byte_array` actually does (corelib
//! `sha2_64_core::compute_sha2_64_u64_array`) is:
//!
//! ```cairo
//! let mut state = sha512_state_handle_init(initial_state);
//! while let Some(chunk) = input.multi_pop_front() {
//!     state = starknet::syscalls::sha512_process_block_syscall(state, *chunk).unwrap_syscall();
//! }
//! sha512_state_handle_digest(state)
//! ```
//!
//! The compression function is not written in Cairo. It lives in the Starknet
//! OS and is reached by syscall. Three consequences, all verified:
//!
//! 1. **`#[executable]` refuses it outright.** `cairo-lang-executable`'s
//!    `compile_executable` walks the Sierra libfunc declarations and bails on
//!    any whose generic id ends in `_syscall`, because syscalls are not proved
//!    in executables. `sha512_process_block_syscall` matches. Wiring
//!    `compute_sha512_byte_array` into `main` puts that libfunc in the program.
//! 2. **A syscall result is unconstrained.** Even with `allow_syscalls`, the
//!    prover chooses the returned digest. The STARK would constrain nothing
//!    about SHA-512, so the proof would attest nothing about the signature.
//! 3. **It costs almost nothing and proves almost nothing.** Measured with the
//!    real RFC 8032 verifier compiled in, adding corelib SHA-512 took the
//!    program from 44,443 to 44,975 CASM words: 532 words of call and return
//!    plumbing wrapped around a value nobody constrained.
//!
//! The way step 1 gets a challenge hash is `verify_with_challenge` in
//! `ed25519.cairo`, where the verifier recomputes `SHA-512(R || A || M) mod L`
//! in Rust and passes it in as a public input. Those three quantities are public
//! in Ed25519 anyway, so nothing is exposed.

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
    /// `u256` add plus a comparison. This is why option "write SHA-512 in
    /// Cairo" was costed rather than dismissed — it is buildable, and it is
    /// still the wrong answer, for the reason recorded at the top of this file
    /// and measured in RFC-0958 §Holder Signature Verification.
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