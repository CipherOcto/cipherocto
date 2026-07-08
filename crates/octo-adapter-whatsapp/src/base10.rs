//! caBLE Base10 encoder/decoder.
//!
//! Used to encode the body of a `FIDO:/<digits>` cross-device
//! pairing/authentication URL (Chromium's `cable/v2_handshake.cc`
//! `BytesToDigits` / `DigitsToBytes`).
//!
//! Each 7-byte chunk is interpreted as a `u64` little-endian and
//! written as a 0-padded decimal of width 17 (so the byte order is
//! preserved bit-for-bit and the QR's numeric mode stays dense).
//! Tail chunks (1-6 bytes) get shorter widths (3, 5, 8, 10, 13, 15).
//!
//! This is a verbatim Rust port of the webauthn-rs implementation at
//! `webauthn-authenticator-rs/src/cable/base10.rs` (which itself ports
//! Chromium's `BytesToDigits`). Includes the test vectors from
//! `cable/handshake.rs::tests::decode_chrome` so future drift is
//! caught at unit-test time.
//!
//! **Scope:** Encoding only — the `[adapter.rs::Event::PairPasskeyRequest]`
//! arm produces the FIDO URI; the phone-side decoder is the WA app
//! (out of process), so this module does not need a decoder for
//! production. A decoder is included for round-trip testing.

use std::fmt::Write;

/// Prefix for a caBLE / WebAuthn hybrid transport URL: the FIDO URI
/// scheme registered as an Android intent filter by WA / Chrome /
/// Safari / Edge for cross-device authenticator handoff.
#[allow(dead_code)] // kept for Session 5+ (in-Rust authenticator driver)
pub const URL_PREFIX: &str = "FIDO:/";

/// Size of a chunk of data in its original form
const CHUNK_SIZE: usize = 7;

/// Size of a chunk of data in its encoded form
const CHUNK_DIGITS: usize = 17;

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // decoder is used by the round-trip tests; production is encode-only
pub enum DecodeError {
    /// The input value contained non-ASCII-digit characters.
    ContainsNonDigitChars,
    /// The input value was not a valid length.
    InvalidLength,
    /// The input value contained a value which was out of range.
    OutOfRange,
}

/// Encodes binary data into Base10 format. See Chromium's `BytesToDigits`.
#[allow(dead_code)] // kept for Session 5+ (in-Rust authenticator driver)
pub fn encode(i: &[u8]) -> String {
    i.chunks(CHUNK_SIZE).fold(String::new(), |mut out, c| {
        let chunk_len = c.len();
        let w = match chunk_len {
            CHUNK_SIZE => CHUNK_DIGITS,
            6 => 15,
            5 => 13,
            4 => 10,
            3 => 8,
            2 => 5,
            1 => 3,
            // This should never happen (chunks() returns 1..=7)
            _ => 0,
        };

        let mut chunk: [u8; 8] = [0; 8];
        chunk[0..chunk_len].copy_from_slice(c);
        let v = u64::from_le_bytes(chunk);
        let _ = write!(out, "{:0width$}", v, width = w);
        out
    })
}

/// Decodes Base10 formatted data into binary form. See Chromium's
/// `DigitsToBytes`.
///
/// Mirror of `encode()` for round-trip testing only — production
/// always encodes (the phone-side decoder is the WA app).
#[allow(dead_code)] // consumed only by #[cfg(test)] round-trip cases
pub fn decode(i: &str) -> Result<Vec<u8>, DecodeError> {
    // Check that i only contains ASCII digits
    if i.chars().any(|c| !c.is_ascii_digit()) {
        return Err(DecodeError::ContainsNonDigitChars);
    }

    // It's safe to operate on the string in bytes now because:
    //
    // - we've previously thrown an error for anything containing non-ASCII digits.
    // - each ASCII digit is exactly 1 byte in UTF-8.
    // - &str is always valid UTF-8.
    let mut o = Vec::with_capacity(i.len().div_ceil(CHUNK_DIGITS) * CHUNK_SIZE);

    i.as_bytes()
        .chunks(CHUNK_DIGITS)
        .map(|b| unsafe { std::str::from_utf8_unchecked(b) })
        .try_for_each(|s| {
            let d = s
                .parse::<u64>()
                .map_err(|_| DecodeError::ContainsNonDigitChars)?;
            let w = match s.len() {
                CHUNK_DIGITS => CHUNK_SIZE,
                15 => 6,
                13 => 5,
                10 => 4,
                8 => 3,
                5 => 2,
                3 => 1,
                // Empty input has zero digits — we never enter this loop.
                // Defensive: any other length is invalid.
                _ => return Err(DecodeError::InvalidLength),
            };
            // Reject chunks whose value doesn't fit in `w` bytes
            // (encode() guarantees width-bounded values via the
            // zero-padding).
            let b = d.to_le_bytes();
            if b.iter().skip(w).any(|byte| *byte != 0) {
                return Err(DecodeError::OutOfRange);
            }
            o.extend_from_slice(&b[0..w]);
            Ok::<(), DecodeError>(())
        })?;
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_chrome_handshake_round_trip() {
        // Verbatim copy of the test vector from
        // `webauthn-authenticator-rs/src/cable/handshake.rs::decode_chrome`
        // — the `FIDO:/<digits>` URI Chrome generates for a real
        // cross-device authenticator. `decode` must recover the exact
        // CBOR bytes, and `encode` of those bytes must reproduce the
        // digit string.
        let u = "FIDO:/162870791865632382552704231438327900152302540348097243854039966655366469794954476199158014113179232779520163209900691930075274801398564434658077048963842109321447142660";
        let digits = &u[URL_PREFIX.len()..];
        let bytes = decode(digits).expect("chrome URL must decode");
        let re_encoded = encode(&bytes);
        assert_eq!(re_encoded.as_str(), digits);
    }

    #[test]
    fn encode_safari_ios_round_trip() {
        // Same for the Safari-generated URL.
        let u = "FIDO:/089962132878132862898875319509818655951233947060166026934941652203853844930597225184066237811614893181300344014421205790072080843938838513707157859599106109321447142404";
        let digits = &u[URL_PREFIX.len()..];
        let bytes = decode(digits).expect("safari URL must decode");
        let re_encoded = encode(&bytes);
        assert_eq!(re_encoded.as_str(), digits);
    }

    #[test]
    fn encode_short_chunks_preserve_byte_order() {
        // Each short-chunk width round-trips.
        // Width table: 1B→3d, 2B→5d, 3B→8d, 4B→10d, 5B→13d,
        // 6B→15d, 7B→17d. Width is fixed by chunk size so the
        // decoder can split the digit stream without length prefixes.
        for (input, expected) in [
            (b"\x00".to_vec(), "000".to_string()),
            (b"\xff".to_vec(), "255".to_string()),
            (b"\xab\xcd".to_vec(), "52651".to_string()), // 0xCDAB LE = 52651
            (b"\x01\x02\x03".to_vec(), "00197121".to_string()), // 0x030201 LE = 197121, padded to 8d
            (b"\x01\x00\x00\x00".to_vec(), "0000000001".to_string()), // 4B→10d, 0x00000001 = 1
        ] {
            let s = encode(&input);
            assert_eq!(s, expected, "encode({input:02x?}) = {s}, want {expected}");
            let back = decode(&s).expect("must round-trip");
            assert_eq!(back, input, "round-trip {input:02x?}");
        }
    }

    #[test]
    fn encode_full_chunk_fits_seventeen_digits() {
        // 7-byte chunk: max u56 = 72057594037927935 (17 digits).
        // Verify the width stays 17 and stays zero-padded for a small value.
        let input = [1u8, 0, 0, 0, 0, 0, 0]; // 1 LE
        let s = encode(&input);
        assert_eq!(s.len(), 17);
        assert_eq!(s, "00000000000000001");
    }

    #[test]
    fn decode_rejects_non_digit() {
        assert_eq!(decode("FIDO:/abc"), Err(DecodeError::ContainsNonDigitChars));
    }
}
