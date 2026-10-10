//! STARK-curve public keys: the wire form and the circuit's `pk_lo` / `pk_hi`
//! split.
//!
//! # Why a key needs two pieces
//!
//! The Cairo STARK curve is `y² = x³ + x + beta` over `p`, and recovering a
//! point from an x-coordinate is inherently ambiguous: `(x, y)` and `(x, −y)`
//! are both on the curve. Roughly half of all `x` values are not on the curve
//! at all, so a caller can never assume a key is constructible.
//!
//! The 32-byte x-coordinate is therefore not a key on its own. It takes the
//! parity of the public key's y-coordinate to pin which of the two points is
//! meant, and that parity bit is **load-bearing, not a nicety**: recover the
//! wrong root and every honest signature verifies roughly half the time and
//! every forged one is equally hard to produce — the failure looks like
//! flakiness, not like a bug.
//!
//! This is the same trap recorded in `cairo/src/stark_schnorr.cairo`, where
//! Cairo's `ec_point_from_x` recovery was measured to return the *negative*
//! of the point a conventional implementation names `Q`. Rust recovery here
//! can ask for the root of a given parity directly, so the ambiguity is
//! resolved by construction instead of by a parity fix-up after the fact.
//!
//! # Two encodings, and they are not interchangeable
//!
//! - **Wire form** — 33 bytes: the 32-byte big-endian x-coordinate followed by
//!   one parity byte. This is what a DID document's `public_key` carries and
//!   what [`SIGNATURE_ALGORITHM`](crate::StarkSchnorrAlgorithm) reports as
//!   `public_key_len()`.
//! - **Circuit form** — `(pk_lo, pk_hi)`, the two felts RFC-0009 §Dual-format
//!   signature algorithms and the ZK-capable key specifies for
//!   `PublicInputs`.
//!
//! They are not the same bytes and the circuit form is **not** the workspace's
//! usual 31-bytes-per-felt digest convention. RFC-0009 is explicit that that
//! convention carries no parity bit and must not be copied here.

use octo_ident::SignatureError;
use starknet_types_core::felt::Felt;
use thiserror::Error;

/// Bytes per felt in the `pk_lo` half of a circuit-split key.
///
/// 31, not 32: `pk_lo` holds the low 31 bytes of a big-endian x-coordinate
/// reinterpreted little-endian, so it tops out at 2²⁴⁸ — comfortably inside
/// the field prime. A 32-byte half would reach 2²⁵⁶ and overflow.
const PK_LO_BYTES: usize = 31;

/// A STARK-curve public key: an x-coordinate plus a parity bit.
///
/// The x-coordinate is stored as canonical 32-byte big-endian bytes, so a key
/// round-trips through storage and the wire without depending on a felt's
/// internal representation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StarkSchnorrPublicKey {
    /// Canonical 32-byte big-endian x-coordinate. Strictly below the field
    /// prime; [`StarkSchnorrPublicKey::from_wire`] refuses anything else.
    x: [u8; 32],
    /// Parity of the public key's y-coordinate. `true` selects the odd root.
    parity: bool,
}

impl StarkSchnorrPublicKey {
    /// Wire length in bytes: 32 bytes of x-coordinate plus one parity byte.
    pub const WIRE_LEN: usize = 33;

    /// Build a key from an x-coordinate and a parity bit.
    ///
    /// # Errors
    /// Returns [`StarkSchnorrKeyError::NotACanonicalFieldElement`] if `x` is
    /// not strictly below the field prime. A 32-byte big-endian value reaches
    /// 2²⁵⁶, so roughly half of all inputs are out of range, and
    /// `Felt::from_bytes_be` reduces them **without complaining** — into
    /// different, still-valid keys. Two distinct wire keys would then verify
    /// the same signatures.
    pub fn new(x: [u8; 32], parity: bool) -> Result<Self, StarkSchnorrKeyError> {
        let felt = Felt::from_bytes_be(&x);
        if felt.to_bytes_be() != x {
            return Err(StarkSchnorrKeyError::NotACanonicalFieldElement);
        }
        Ok(Self { x, parity })
    }

    /// Parse the 33-byte wire form.
    ///
    /// # Errors
    /// - [`StarkSchnorrKeyError::BadWireLength`] on any length but 33.
    /// - [`StarkSchnorrKeyError::BadParityByte`] if the parity byte is
    ///   anything other than 0 or 1. **A parity byte of 0xFF must not be read
    ///   as `true`**: the bit that matters is the low one, so a naive
    ///   `byte & 1` silently accepts a malformed key and a naive
    ///   `byte != 0` silently accepts every one of the other 253 values.
    /// - [`StarkSchnorrKeyError::NotACanonicalFieldElement`] as in
    ///   [`StarkSchnorrPublicKey::new`].
    pub fn from_wire(bytes: &[u8]) -> Result<Self, StarkSchnorrKeyError> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(StarkSchnorrKeyError::BadWireLength { got: bytes.len() });
        }
        let parity = match bytes[32] {
            0 => false,
            1 => true,
            other => return Err(StarkSchnorrKeyError::BadParityByte { got: other }),
        };
        let mut x = [0u8; 32];
        x.copy_from_slice(&bytes[..32]);
        Self::new(x, parity)
    }

    /// The 33-byte wire form.
    #[must_use]
    pub fn to_wire(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[..32].copy_from_slice(&self.x);
        out[32] = u8::from(self.parity);
        out
    }

    /// The x-coordinate as a field element.
    #[must_use]
    pub fn x(&self) -> Felt {
        // Canonical by construction: `new` rejected anything that `from_bytes_be`
        // would have had to reduce.
        Felt::from_bytes_be(&self.x)
    }

    /// The parity bit.
    #[must_use]
    pub fn parity(&self) -> bool {
        self.parity
    }

    /// Split into the circuit's `(pk_lo, pk_hi)` felts.
    ///
    /// RFC-0009 §Dual-format signature algorithms and the ZK-capable key:
    ///
    /// ```text
    /// pk_lo = felt252 from the low 31 bytes of the x-coordinate, little-endian
    /// pk_hi = felt252 carrying the remaining byte, with the parity bit as its
    ///         only significant bit
    /// ```
    ///
    /// "Only significant bit" reads as *in addition to* the byte it carries:
    /// `pk_hi = (x[31] << 1) | parity`. That is the only reading under which
    /// the split is lossless — discarding the byte would make two keys with
    /// the same low 31 bytes indistinguishable, which is precisely the
    /// ambiguity the parity bit exists to resolve.
    #[must_use]
    pub fn split_into_felts(&self) -> (Felt, Felt) {
        let mut buf = [0u8; 32];
        // `x` is big-endian, so `x[0]` is the **most** significant byte and
        // it is `pk_hi`'s job. The low 31 bytes are `x[1..]`, and
        // little-endian places `x[j]` at bit position 8·(j−1) — which in a
        // big-endian buffer is index `31 − (j−1)`.
        //
        // The buffer's own leading byte stays zero, so `pk_lo` cannot exceed
        // 2²⁴⁸.
        for (offset, byte) in self.x[1..PK_LO_BYTES + 1].iter().enumerate() {
            buf[31 - offset] = *byte;
        }
        let lo = Felt::from_bytes_be(&buf);
        let hi = Felt::from((u64::from(self.x[0]) << 1) | u64::from(self.parity));
        (lo, hi)
    }

    /// Recombine the circuit's `(pk_lo, pk_hi)` felts into a key.
    ///
    /// # Errors
    /// - [`StarkSchnorrKeyError::PkLoOverflow`] if `lo` needs more than 31
    ///   bytes, which would mean the prover sent a half from a different
    ///   packing.
    /// - [`StarkSchnorrKeyError::PkHiOverflow`] if `hi` carries bits above the
    ///   carried byte and the parity bit — i.e. anything ≥ 2⁹.
    /// - [`StarkSchnorrKeyError::NotACanonicalFieldElement`] if the
    ///   recombined x-coordinate is not below the field prime.
    ///
    /// Every one of these is a prover supplying public inputs that no
    /// well-formed key could produce. RFC-0009 requires the verifier to
    /// recompute the halves from the DID-resolved key and reject a proof
    /// whose inputs disagree, so this is the gate — a verifier that trusted
    /// the halves would be accepting a key it never resolved.
    pub fn join_felts(lo: Felt, hi: Felt) -> Result<Self, StarkSchnorrKeyError> {
        let lo_bytes = lo.to_bytes_be();
        // The top byte must be zero: `pk_lo` is defined over 31 bytes, so a
        // non-zero byte here is a different packing rather than a value that
        // happens to be large.
        if lo_bytes[0] != 0 {
            return Err(StarkSchnorrKeyError::PkLoOverflow);
        }

        let hi_val = hi.to_le_digits()[0];
        if hi_val >= (1 << 9) {
            return Err(StarkSchnorrKeyError::PkHiOverflow { got: hi_val });
        }
        let parity = hi_val & 1 == 1;
        let high_byte = ((hi_val >> 1) & 0xFF) as u8;

        let mut x = [0u8; 32];
        // Inverse of the split: `x[j]` came from `buf[32 − j]`, so the low 31
        // bytes come back in reverse buffer order.
        for (offset, b) in x[1..PK_LO_BYTES + 1].iter_mut().enumerate() {
            *b = lo_bytes[31 - offset];
        }
        x[0] = high_byte;
        Self::new(x, parity)
    }

    /// Decode the wire form for a verifier, mapping a malformed key onto the
    /// algorithm's own error type.
    ///
    /// Convenience so [`SignatureAlgorithm::verify`] implementations and
    /// callers on the wire path report one vocabulary rather than two.
    ///
    /// # Errors
    /// [`SignatureError::Invalid`] for any malformed wire form.
    pub fn from_wire_for_verify(bytes: &[u8], alg: &'static str) -> Result<Self, SignatureError> {
        Self::from_wire(bytes).map_err(|_| SignatureError::Invalid(alg))
    }
}

/// Failure to construct or split a STARK-curve public key.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum StarkSchnorrKeyError {
    /// The wire form was not exactly 33 bytes.
    #[error("stark-schnorr public key must be 33 bytes, got {got}")]
    BadWireLength {
        /// Length supplied.
        got: usize,
    },
    /// The parity byte was neither 0 nor 1.
    #[error("stark-schnorr parity byte must be 0 or 1, got {got}")]
    BadParityByte {
        /// Byte supplied.
        got: u8,
    },
    /// The x-coordinate is not strictly below the field prime.
    ///
    /// Reduction would silently produce a *different* key rather than an
    /// error, so this is refused rather than wrapped.
    #[error("stark-schnorr x-coordinate is not a canonical field element")]
    NotACanonicalFieldElement,
    /// `pk_lo` does not fit in 31 bytes.
    #[error("stark-schnorr pk_lo exceeds the 31-byte half")]
    PkLoOverflow,
    /// `pk_hi` carries bits above the x-byte and the parity bit.
    #[error("stark-schnorr pk_hi carries bits above 2^9: {got}")]
    PkHiOverflow {
        /// Offending value.
        got: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key whose most significant byte is non-zero.
    ///
    /// This is the case the split exists for. A packing that kept only 31
    /// bytes would collide it with a key whose leading byte is zero, so the
    /// round-trip must be shown to preserve it rather than assumed.
    ///
    /// `x` is big-endian, so `x[0]` is the leading byte — and a leading byte
    /// of 0x13 is already above the field prime
    /// (`p = 0x0800_0000_0000_0110…`). 0x05 keeps it canonical; see
    /// `a_non_canonical_x_coordinate_is_refused_not_reduced` for the other
    /// side of that boundary.
    const HIGH_BYTE_NONZERO: [u8; 32] = {
        let mut k = [0u8; 32];
        k[0] = 0x05;
        k[1] = 0x7F;
        k[31] = 0x13;
        k
    };

    /// Canonical values for the leading byte, given the fixed low pattern
    /// below (whose top byte is 0x1F).
    const CANONICAL_HIGH_BYTES: [u8; 8] = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];

    /// Leading bytes that are *not* canonical field elements with that low
    /// pattern. 0x08 is the first one: on its own it would fit, but the
    /// following 0x1F pushes the value past `p`.
    const NON_CANONICAL_HIGH_BYTES: [u8; 5] = [0x08, 0x09, 0x0F, 0x5A, 0xFF];

    /// An x-coordinate whose leading byte is `byte` and whose low 31 bytes
    /// carry a distinct ascending pattern.
    ///
    /// The pattern matters twice over: it makes an endianness slip visible,
    /// and it keeps the low bytes small so the whole value stays below the
    /// field prime.
    fn x_with_high_byte(byte: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        for (offset, b) in x[1..].iter_mut().enumerate() {
            *b = (offset as u8).wrapping_add(1);
        }
        x[0] = byte;
        x
    }

    #[test]
    fn a_key_round_trips_through_the_wire_form() {
        for parity in [false, true] {
            let key = StarkSchnorrPublicKey::new(HIGH_BYTE_NONZERO, parity).expect("canonical");
            let bytes = key.to_wire();
            assert_eq!(bytes.len(), StarkSchnorrPublicKey::WIRE_LEN);
            assert_eq!(
                StarkSchnorrPublicKey::from_wire(&bytes).expect("decode"),
                key
            );
        }
    }

    #[test]
    fn the_wire_form_is_the_x_coordinate_then_the_parity_byte() {
        let key = StarkSchnorrPublicKey::new(HIGH_BYTE_NONZERO, true).expect("canonical");
        let bytes = key.to_wire();
        assert_eq!(&bytes[..32], &HIGH_BYTE_NONZERO[..]);
        assert_eq!(bytes[32], 1);
        let other = StarkSchnorrPublicKey::new(HIGH_BYTE_NONZERO, false).expect("canonical");
        assert_eq!(other.to_wire()[32], 0);
    }

    #[test]
    fn stark_schnorr_pk_lo_hi_round_trip() {
        // Sweep the high byte: 0x00 is the case a naive split silently
        // handles, and the rest are the ones it does not.
        for high in CANONICAL_HIGH_BYTES {
            for parity in [false, true] {
                let key =
                    StarkSchnorrPublicKey::new(x_with_high_byte(high), parity).expect("canonical");
                let (lo, hi) = key.split_into_felts();
                let back = StarkSchnorrPublicKey::join_felts(lo, hi).expect("round trip");
                assert_eq!(back, key, "high={high:#04x} parity={parity}");
            }
        }
    }

    #[test]
    fn pk_lo_holds_the_low_thirty_one_bytes_little_endian() {
        let mut x = [0u8; 32];
        // Distinct byte values so an endianness slip cannot pass.
        for (offset, byte) in x[1..].iter_mut().enumerate() {
            *byte = (offset as u8) + 1;
        }
        x[0] = 0x03;
        let key = StarkSchnorrPublicKey::new(x, true).expect("canonical");
        let (lo, _hi) = key.split_into_felts();
        let lo_be = lo.to_bytes_be();

        // `x[j]` sits at bit position 8·(j−1), so in a big-endian buffer it
        // lives at index 32−j. Asserting that layout directly is what pins
        // the endianness; a palindromic x could not tell a big-endian split
        // from a little-endian one.
        for j in 1..32 {
            assert_eq!(
                lo_be[32 - j],
                x[j],
                "x[{j}] must land at bit position {}·8",
                j - 1
            );
        }

        // And the leading byte is not in pk_lo at all — that is pk_hi's job.
        // The buffer's most significant byte is the one that must stay clear.
        assert_eq!(
            lo_be[0], 0,
            "pk_lo must stop at 31 bytes, leaving the leading byte for pk_hi"
        );
    }

    #[test]
    fn pk_hi_carries_the_remaining_byte_and_the_parity_bit() {
        for high in CANONICAL_HIGH_BYTES {
            for parity in [false, true] {
                let key =
                    StarkSchnorrPublicKey::new(x_with_high_byte(high), parity).expect("canonical");
                let (_lo, hi) = key.split_into_felts();
                assert_eq!(
                    hi.to_le_digits()[0],
                    (u64::from(high) << 1) | u64::from(parity),
                    "high={high:#04x} parity={parity}"
                );
            }
        }
    }

    #[test]
    fn stark_schnorr_pk_lo_hi_rejects_inconsistent_halves() {
        // Half a key no well-formed split could have produced. RFC-0009
        // requires the verifier to recompute the halves from the resolved key
        // and reject a proof whose inputs disagree, so these are the gate:
        // a verifier that trusted them would be accepting a key it never
        // resolved.

        // pk_hi bit 9 and above belong to neither the carried byte nor the
        // parity bit.
        let lo = Felt::ZERO;
        assert_eq!(
            StarkSchnorrKeyError::PkHiOverflow { got: 1 << 9 },
            StarkSchnorrPublicKey::join_felts(lo, Felt::from(1u64 << 9))
                .expect_err("bit 9 belongs to neither the byte nor the parity")
        );
        assert_eq!(
            StarkSchnorrKeyError::PkHiOverflow { got: u64::MAX },
            StarkSchnorrPublicKey::join_felts(lo, Felt::from(u64::MAX))
                .expect_err("every bit above 8 is out of range")
        );

        // One below the boundary parses — with a leading byte that is itself
        // inside the field, so the failure under test is the overflow and not
        // the canonicality check.
        let biggest = (0x05u64 << 1) | 1;
        assert_eq!(
            0x05,
            StarkSchnorrPublicKey::join_felts(lo, Felt::from(biggest))
                .expect("a well-formed pk_hi must parse")
                .to_wire()[0],
            "the leading byte must come back out of pk_hi"
        );

        // pk_lo needs 32 bytes, which is a different packing entirely.
        let too_big = Felt::from_bytes_be(&{
            let mut b = [0u8; 32];
            b[0] = 0x01;
            b
        });
        assert_eq!(
            StarkSchnorrKeyError::PkLoOverflow,
            StarkSchnorrPublicKey::join_felts(too_big, lo).expect_err("pk_lo is 31 bytes")
        );
    }

    #[test]
    fn a_non_canonical_x_coordinate_is_refused_not_reduced() {
        // Above the field prime. Wrapping would yield a *different* valid
        // key, so two distinct wire keys would verify the same signatures.
        let x = [0xFFu8; 32];
        assert_eq!(
            StarkSchnorrPublicKey::new(x, false),
            Err(StarkSchnorrKeyError::NotACanonicalFieldElement)
        );
        assert!(StarkSchnorrPublicKey::from_wire(&{
            let mut w = [0xFFu8; 33];
            w[32] = 0;
            w
        })
        .is_err());

        // The boundary is where the field prime puts it, not somewhere in
        // the middle of nowhere: every leading byte from 0x09 up is refused,
        // and every one below it is accepted.
        for high in NON_CANONICAL_HIGH_BYTES {
            assert_eq!(
                StarkSchnorrPublicKey::new(x_with_high_byte(high), false),
                Err(StarkSchnorrKeyError::NotACanonicalFieldElement),
                "leading byte {high:#04x} is above the field prime"
            );
        }
        for high in CANONICAL_HIGH_BYTES {
            assert!(
                StarkSchnorrPublicKey::new(x_with_high_byte(high), false).is_ok(),
                "leading byte {high:#04x} is inside the field"
            );
        }
    }

    #[test]
    fn a_parity_byte_other_than_zero_or_one_is_refused() {
        for bad in [2u8, 0x7F, 0xFF] {
            let mut wire = [0u8; 33];
            wire[32] = bad;
            assert_eq!(
                StarkSchnorrPublicKey::from_wire(&wire),
                Err(StarkSchnorrKeyError::BadParityByte { got: bad }),
                "parity byte {bad:#04x} must be refused, not coerced to a bool"
            );
        }
    }

    #[test]
    fn a_wrong_length_wire_form_is_refused() {
        for len in [0usize, 1, 31, 32, 34, 64] {
            assert_eq!(
                StarkSchnorrPublicKey::from_wire(&vec![0u8; len]),
                Err(StarkSchnorrKeyError::BadWireLength { got: len })
            );
        }
    }

    #[test]
    fn distinct_keys_do_not_collide_through_the_circuit_split() {
        // The property the split buys: the parity bit is carried, so two keys
        // sharing an x-coordinate stay distinguishable.
        let a = StarkSchnorrPublicKey::new(HIGH_BYTE_NONZERO, false).expect("canonical");
        let b = StarkSchnorrPublicKey::new(HIGH_BYTE_NONZERO, true).expect("canonical");
        assert_ne!(a.split_into_felts(), b.split_into_felts());

        // And so do two keys sharing 31 bytes and differing in the 32nd.
        let c = StarkSchnorrPublicKey::new(x_with_high_byte(0x00), true).expect("canonical");
        let d = StarkSchnorrPublicKey::new(x_with_high_byte(0x01), true).expect("canonical");
        assert_ne!(c.split_into_felts(), d.split_into_felts());
    }
}
