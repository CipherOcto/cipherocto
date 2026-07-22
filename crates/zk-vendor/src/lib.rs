//! CipherOcto zk-vendor: vendored STWO source with stable-rust patches.
//!
//! **Crypto home:** this crate is the cargo home for STWO (a STARK prover
//! that depends on nightly-only `curve25519-dalek` SIMD). Extracted from the
//! stoolap fork per [[stoolap-general-purpose-db]] (2026-07-22). Stable
//! rust only — MSRV pinned in `rust-toolchain.toml`.
//!
//! ## Vendor state (2026-07-22)
//!
//! STUB. Real STWO source drop deferred to mission 0958-a S05 task C.2.1.
//! When the source lands, it must:
//! - Drop `#![feature(simd_x86_*, portable_simd)]` from upstream
//! - Replace `curve25519_dalek::scalar::Scalar::from_bits` SIMD path with
//!   `stable_curve25519::Scalar::from_bits`
//! - Re-export the verify entry point via `pub use stwo::verify;`
//!
//! Until then, the stub below makes the cargo dep-graph work + imports
//! cleanly downstream.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]

/// Marker for whether STWO source is vendored.
///
/// - `Stub`: SHA-based stub in `zk-verifier` (current state, 2026-07-22).
/// - `Vendored`: real STWO source drop in `zk-vendor/stwo/` (pending
///   mission 0958-a S05 task C.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VendorState {
    Stub,
    Vendored,
}

/// Returns the current vendor state.
#[must_use]
pub const fn vendor_state() -> VendorState {
    VendorState::Stub // TBD: flip to Vendored when source drop ships.
}

/// BLAKE3 marker hash (deterministic; NOT a STARK proof).
///
/// Reserved for future stub-removal migration: when `VendorState::Vendored`
/// ships, callers should switch to `stwo::verify(...)` directly. This stub
/// hash exists for binary verification shape only.
#[must_use]
pub fn stwo_stub_marker() -> [u8; 32] {
    *blake3::hash(b"zk-vendor:stwa-stub:v0").as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_state_is_stub_for_now() {
        assert_eq!(vendor_state(), VendorState::Stub);
    }

    #[test]
    fn stwo_stub_marker_is_deterministic() {
        assert_eq!(stwo_stub_marker(), stwo_stub_marker());
    }
}
