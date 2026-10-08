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

/// Bytes packed into one felt when encoding a caveat message for the chain MAC.
///
/// **31, not 32.** A felt252 must stay strictly below the field prime
/// (~2^251.5 + ...). A 32-byte big-endian value reaches 2^256, so packing 32
/// bytes would overflow for roughly half of all inputs. Those inputs would not
/// fail loudly — `Felt` reduction wraps them into *different, still-valid* felts,
/// so two distinct messages would collide and the MAC would bind the wrong
/// caveat. 31 bytes tops out at 2^248, comfortably inside the prime.
///
/// Every byte string is recoverable from its packing, and distinct byte strings
/// pack to distinct sequences. Both properties are tested, because a packing
/// that is merely "usually right" is a silent collision waiting for input.
pub const BYTES_PER_FELT: usize = 31;

/// Pack a byte string into felts for use as a MAC message.
///
/// Big-endian, `BYTES_PER_FELT` bytes per felt, **length-prefixed**: element 0
/// is the byte length, the rest are the packed groups.
///
/// The length prefix is not optional. Right-aligning each group looks sufficient
/// until you test it, at which point `[0x01, 0x02]` and `[0x00, 0x01, 0x02]` pack
/// identically — a MAC over that packing would bind the wrong caveat, and a
/// round-trip would return 31 bytes for a 2-byte input. The prefix is what makes
/// the encoding both injective and length-recoverable.
///
/// # Panics
/// Never. An empty input yields a one-element vector holding `0`.
#[must_use]
pub fn bytes_to_felts(bytes: &[u8]) -> Vec<FieldElement> {
    let mut out = Vec::with_capacity(1 + bytes.len().div_ceil(BYTES_PER_FELT));
    out.push(FieldElement::from(bytes.len() as u64));
    for chunk in bytes.chunks(BYTES_PER_FELT) {
        let mut buf = [0u8; 32];
        buf[32 - chunk.len()..].copy_from_slice(chunk);
        out.push(FieldElement::from_bytes_be(&buf));
    }
    out
}

/// Inverse of [`bytes_to_felts`].
///
/// Returns `None` for any witness this function did not produce: a missing
/// prefix, a length that disagrees with the group count, or a felt wider than
/// `BYTES_PER_FELT`. Truncating instead would decode a malformed witness into a
/// *different* valid message, which is the failure this guards.
///
/// # Panics
/// Never.
#[must_use]
pub fn felts_to_bytes(felts: &[FieldElement]) -> Option<Vec<u8>> {
    let (prefix, body) = felts.split_first()?;
    // Low limb: to_raw is big-endian, so index 0 would read the HIGH limb and
    // silently yield 0 for every small prefix.
    let len = usize::try_from(prefix.to_le_digits()[0]).ok()?;

    // The declared length must match the number of groups actually present.
    // Without this a witness could claim a short length and hide trailing groups.
    let expected_groups = len.div_ceil(BYTES_PER_FELT);
    if body.len() != expected_groups {
        return None;
    }
    if expected_groups == 0 {
        return (len == 0).then(Vec::new);
    }

    let mut out = Vec::with_capacity(len);
    let full_groups = len / BYTES_PER_FELT;
    let remainder = len % BYTES_PER_FELT;
    let expected = full_groups + usize::from(remainder != 0);
    debug_assert_eq!(expected_groups, expected);

    for (i, f) in body.iter().enumerate() {
        let be = f.to_bytes_be();
        // Only the FINAL group may be short, and only by `remainder`. Every
        // other group is a full `BYTES_PER_FELT` wide, so a non-zero leading
        // byte there means the felt exceeded the field and was reduced.
        let width = if i + 1 == body.len() {
            if remainder == 0 {
                BYTES_PER_FELT
            } else {
                remainder
            }
        } else {
            BYTES_PER_FELT
        };
        if be[..32 - width].iter().any(|b| *b != 0) {
            return None;
        }
        out.extend_from_slice(&be[32 - width..]);
    }
    debug_assert_eq!(out.len(), len);
    Some(out)
}

#[cfg(test)]
mod packing_tests {
    use super::*;

    fn bytes(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn every_felt_stays_inside_the_packing_width() {
        // The property 31 bytes buys. At 32, inputs with the top bit set exceed
        // the field prime and reduction wraps them — silently, into different
        // but still-valid felts, which is a collision rather than an error.
        for len in [1usize, 30, 31, 32, 62, 93, 200] {
            for f in bytes_to_felts(&bytes(len)) {
                let be = f.to_bytes_be();
                assert!(
                    be[..32 - BYTES_PER_FELT].iter().all(|b| *b == 0),
                    "len={len}: felt occupies more than {BYTES_PER_FELT} bytes, so it \
                     exceeded the field prime and was reduced"
                );
            }
        }
    }

    #[test]
    fn a_full_width_group_keeps_its_leading_byte() {
        // Anti-vacuity for the test above: a felt really does span the bytes it
        // was packed from, so the "no bytes above the width" check is
        // meaningful rather than vacuously true.
        let b = bytes(31);
        let packed = bytes_to_felts(&b);
        let be = packed[1].to_bytes_be();
        assert_eq!(
            be[1], b[0],
            "the first packed byte must survive at offset 1"
        );
        assert_eq!(
            be[31], b[30],
            "the last packed byte must survive at offset 31"
        );
    }

    #[test]
    fn packing_round_trips_every_length_around_the_group_boundary() {
        for len in [0usize, 1, 30, 31, 32, 61, 62, 63, 124] {
            let b = bytes(len);
            let packed = bytes_to_felts(&b);
            assert_eq!(
                felts_to_bytes(&packed).as_deref(),
                Some(&b[..]),
                "len={len}"
            );
        }
    }

    #[test]
    fn distinct_inputs_pack_to_distinct_sequences() {
        // The collision property. A message and a shifted variant must not be
        // confusable, which is what right-alignment of the final group buys.
        let a = bytes_to_felts(&[0x01, 0x02]);
        let b = bytes_to_felts(&[0x00, 0x01, 0x02]);
        let c = bytes_to_felts(&[0x01, 0x02, 0x00]);
        assert_ne!(
            a, b,
            "a leading zero must not be indistinguishable from padding"
        );
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert_ne!(a, bytes_to_felts(&[]));
    }

    #[test]
    fn a_felt_outside_the_packing_width_is_refused_rather_than_truncated() {
        // A witness carrying a value this function never produced must not be
        // silently decoded into a different message.
        let too_big = FieldElement::from_bytes_be(&[0xff; 32]);
        assert_eq!(felts_to_bytes(&[too_big]), None);
    }

    #[test]
    fn group_boundary_lengths_use_the_expected_felt_count() {
        // +1 for the length prefix.
        assert_eq!(bytes_to_felts(&bytes(0)).len(), 1);
        assert_eq!(bytes_to_felts(&bytes(1)).len(), 2);
        assert_eq!(bytes_to_felts(&bytes(31)).len(), 2);
        assert_eq!(bytes_to_felts(&bytes(32)).len(), 3);
        assert_eq!(bytes_to_felts(&bytes(62)).len(), 3);
        assert_eq!(bytes_to_felts(&bytes(63)).len(), 4);
    }

    #[test]
    fn a_length_that_disagrees_with_the_group_count_is_refused() {
        // A witness claiming fewer bytes than its groups can hold would
        // otherwise decode into a truncated message.
        let mut packed = bytes_to_felts(&bytes(64));
        packed[0] = FieldElement::from(1_u64); // claim 1 byte, supply 3 groups
        assert_eq!(felts_to_bytes(&packed), None);
    }

    #[test]
    fn a_missing_length_prefix_is_refused() {
        assert_eq!(felts_to_bytes(&[]), None);
        // A bare group with no prefix must not decode.
        assert_eq!(felts_to_bytes(&bytes_to_felts(&bytes(10))[1..]), None);
    }
}

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
