//! STARK-native substrate primitives.
//!
//! # Why this exists
//!
//! RFC-0958 §Algorithms step 2 requires a circuit to re-derive the caveat chain
//! and bind it to a verifier-supplied value. Verifying the RFC-0957 HMAC-BLAKE3
//! chain requires BLAKE3 in-field, which measured **17,628 CASM words** against a
//! 2,200-word ceiling. That is not a tuning problem; it is the wrong primitive
//! for a STARK.
//!
//! Poseidon is designed for exactly this and measures 2,182 words for the same
//! relation — **8.1x cheaper** — using the same crate family the Cairo toolchain
//! builds against, so host and circuit can be held to identical vectors instead
//! of an assumption.
//!
//! # What is NOT here
//!
//! The credential chain is still HMAC-BLAKE3. Nothing in this module changes how
//! a capability is minted, attenuated, or verified. It supplies the primitive
//! a ZK circuit needs, so that adopting it on the host is a separate, explicit
//! decision rather than something that happened as a side effect of a refactor.
//!
//! # Security note
//!
//! [`poseidon_mac`] is a domain-separated sponge-prefix construction:
//! `MAC(k, m) = H(MAC_DOMAIN, k, m)`. The key is absorbed as a prefix under a
//! domain the commitment path never uses, which is the standard sponge MAC.
//! This is **not** `blake3::keyed_hash`, and the two must never be treated as
//! interchangeable — the domain separation is what stops a value produced as a
//! MAC being replayed as a commitment, and the two consume different
//! attacker-chosen inputs.

use starknet_crypto::poseidon_hash_many;
use starknet_crypto::Felt as FieldElement;

/// Domain tag separating the MAC use from the commitment use.
///
/// `b"MAC-CHAIN"` as a felt. Must equal `MAC_DOMAIN` in `cairo/src/lib.cairo`;
/// the parity tests pin that they agree.
pub const MAC_DOMAIN: FieldElement = FieldElement::from_hex_unchecked("0x4d41432d434841494e");

/// Domain-separated keyed MAC over felts: `MAC(k, m) = H(MAC_DOMAIN, k, m)`.
///
/// Mirrors `poseidon_mac` in `cairo/src/lib.cairo` exactly. If one side changes,
/// the parity vectors fail and the mismatch is caught rather than silently
/// producing chain tips that never match.
///
/// # Panics
/// Never. An empty `msg` yields `H(MAC_DOMAIN, k)`.
#[must_use]
pub fn poseidon_mac(key: FieldElement, msg: &[FieldElement]) -> FieldElement {
    let mut felts: Vec<FieldElement> = Vec::with_capacity(2 + msg.len());
    felts.push(MAC_DOMAIN);
    felts.push(key);
    felts.extend_from_slice(msg);
    poseidon_hash_many(felts.iter())
}

/// Split a 32-byte value into `(lo, hi)` felts, little-endian half order.
///
/// Matches the convention used everywhere a 256-bit value crosses into the
/// circuit, so `cap_root_hash`, `chain_tip` and holder keys all split the same
/// way. Getting this backwards produces values that are individually in range
/// and collectively wrong, which is why it lives here once.
#[must_use]
pub fn split_felts(bytes: &[u8; 32]) -> (FieldElement, FieldElement) {
    // Each half is a 16-byte big-endian integer, so it must be right-aligned in
    // the 32-byte field representation (i.e. scaled by 2^128). Copying the 16
    // bytes straight into a 32-byte buffer would panic on length, and padding
    // on the wrong side would silently produce in-range but wrong values.
    let mut lo = [0u8; 32];
    let mut hi = [0u8; 32];
    lo[16..].copy_from_slice(&bytes[..16]);
    hi[16..].copy_from_slice(&bytes[16..]);
    (
        FieldElement::from_bytes_be(&lo),
        FieldElement::from_bytes_be(&hi),
    )
}

// Filled in by the parity test; kept beside it so the values the circuit asserts
// and the values asserted here are one readable block.
pub const PARITY_EMPTY: FieldElement = FieldElement::from_hex_unchecked(
    "0x046e86a3387dd91b36e4c3bd6bdcd1ff021bee5c909188d42b7d76f27819de98",
);
pub const PARITY_SINGLE: FieldElement = FieldElement::from_hex_unchecked(
    "0x016c51efb49c3462f9e32810075ce52b3327d25b7802fa10313a71d72846056d",
);
pub const PARITY_THREE: FieldElement = FieldElement::from_hex_unchecked(
    "0x037523d4d4d0875dc9931199b83dd31a074bcdb4a21a5f2098020afd219f659b",
);
pub const PARITY_LARGE: FieldElement = FieldElement::from_hex_unchecked(
    "0x06bd32a5ce3f2513b62f1d02a7ef8128e148bfca2f1514deeef096d46058224d",
);

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(v: u64) -> FieldElement {
        FieldElement::from(v)
    }

    /// Cross-implementation parity vectors.
    ///
    /// These constants are asserted **identically** by `poseidon_mac_parity` in
    /// `cairo/src/lib.cairo`. A mismatch means the two implementations have
    /// diverged and every circuit proof would fail against a real token — the
    /// one failure mode a hand-written MAC is most prone to, because both sides
    /// still run and still produce plausible values.
    fn parity_vectors() -> [(&'static str, FieldElement, Vec<FieldElement>, FieldElement); 4] {
        [
            ("empty message", fe(1), vec![], PARITY_EMPTY),
            ("single felt", fe(42), vec![fe(7)], PARITY_SINGLE),
            (
                "three felts",
                fe(0xdead),
                vec![fe(1), fe(2), fe(3)],
                PARITY_THREE,
            ),
            (
                "domain-sized key",
                fe(u64::MAX),
                vec![fe(u64::MAX), fe(0)],
                PARITY_LARGE,
            ),
        ]
    }

    #[test]
    fn poseidon_mac_matches_the_circuit_vectors() {
        for (name, key, msg, expected) in parity_vectors() {
            assert_eq!(
                poseidon_mac(key, &msg),
                expected,
                "host/circuit Poseidon MAC divergence on `{name}`"
            );
        }
    }

    #[test]
    fn mac_domain_is_the_circuit_domain() {
        // `b"MAC-CHAIN"` — the literal the Cairo side declares.
        assert_eq!(
            MAC_DOMAIN,
            FieldElement::from_hex_unchecked("0x4d41432d434841494e")
        );
    }

    #[test]
    fn domain_separation_changes_the_output() {
        // Without this, a value produced as a MAC could be replayed as a
        // commitment and vice versa.
        let k = fe(9);
        let mac = poseidon_mac(k, &[fe(1)]);
        let mut commitment_input = Vec::new();
        commitment_input.push(k);
        commitment_input.extend_from_slice(&[fe(1)]);
        assert_ne!(mac, poseidon_hash_many(commitment_input.iter()));
    }

    #[test]
    fn split_felts_is_low_half_first_and_round_trips_by_bytes() {
        let bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
        let (lo, hi) = split_felts(&bytes);
        let mut expect_lo = [0u8; 32];
        let mut expect_hi = [0u8; 32];
        expect_lo[16..].copy_from_slice(&bytes[..16]);
        expect_hi[16..].copy_from_slice(&bytes[16..]);
        assert_eq!(lo, FieldElement::from_bytes_be(&expect_lo));
        assert_eq!(hi, FieldElement::from_bytes_be(&expect_hi));
        // Right-alignment means each half is scaled by 2^128. Getting the side
        // wrong yields in-range felts that silently disagree with the circuit.
        let hex_lo = bytes[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            lo,
            FieldElement::from_hex_unchecked(&format!("0x{hex_lo}")),
            "the low half must be the leading 16 bytes, right-aligned"
        );
        // Halves must differ for a non-symmetric input, otherwise the split
        // could not be carrying both.
        assert_ne!(lo, hi);
    }
}
