//! Error types for the `octo-cable` crate.

use crate::base10::DecodeError;
use thiserror::Error;

/// Errors produced by the caBLE transport and HandshakeV2 codec.
#[derive(Debug, Error)]
pub enum CableError {
    /// The QR body contained a non-ASCII-digit character (after the
    /// `FIDO:/` prefix). Mirrors webauthn-rs's
    /// `cable::base10::DecodeError::ContainsNonDigitChars` semantics.
    #[error("base10 body contains non-digit characters")]
    ContainsNonDigitChars,

    /// The decoded base10 value did not fit in its declared chunk width
    /// (overflow). Equivalent to webauthn-rs's `OutOfRange`.
    #[error("base10 chunk value overflows its declared byte width")]
    OutOfRange,

    /// The base10 input length was not a valid sum of chunk widths
    /// (3, 5, 8, 10, 13, 15, or 17 digits per chunk).
    #[error("base10 input length is not a valid sum of chunk widths")]
    InvalidLength,

    /// CBOR parse failed (the bytes after base10 decode are not
    /// well-formed CBOR, or are not a map at the top level).
    #[error("CBOR decode failed: {0}")]
    Cbor(String),

    /// The decoded map contained a non-integer key. CTAP2 / HandshakeV2
    /// uses integer keys 0-6 exclusively.
    #[error("CBOR map key must be an integer, got {0:?}")]
    NonIntegerKey(String),

    /// The decoded map contained an unknown integer key (not in
    /// `0..=6`). Reserved for forward-compatibility: per the CTAP2 /
    /// caBLE spec, unknown keys MUST be rejected.
    #[error("unknown HandshakeV2 key: {0}")]
    UnknownKey(i128),

    /// A required HandshakeV2 field was absent from the decoded map.
    /// Fields 0, 1, 3, 5 are mandatory; 2/4/6 are optional with
    /// default values.
    #[error("missing required HandshakeV2 field: {0}")]
    MissingField(u8),

    /// A CBOR value had the wrong major type for the field it was
    /// bound to (e.g., field 0 must be a byte string, not text).
    #[error("field {field} has wrong type: expected {expected}, got {got}")]
    WrongType {
        /// The HandshakeV2 integer key (0-6).
        field: u8,
        /// What the field's CBOR type should be (e.g., "bytes", "uint").
        expected: &'static str,
        /// What we actually got (e.g., "text", "null").
        got: &'static str,
    },

    /// The `request_type` field (key 5) had an unrecognised string.
    /// Only `"ga"` (GetAssertion) and `"mc"` (MakeCredential) are
    /// defined by the caBLE v2 spec.
    #[error("unknown request_type: {0:?}")]
    UnknownRequestType(String),

    /// An empty QR body (no digits after `FIDO:/`).
    #[error("empty FIDO body")]
    EmptyBody,

    /// Missing `FIDO:/` prefix on the QR string.
    #[error("missing FIDO:/ prefix (got {0:?})")]
    MissingPrefix(String),

    /// Session 15: BLE advertiser could not be started. caBLE v2
    /// requires the responder to emit a service-data advertisement
    /// (UUID 0xfff9) carrying the encrypted Eid, which the phone's
    /// gms FIDO module scans for and uses to derive the matching
    /// PSK for the Noise handshake. The ad cannot be omitted: without
    /// it, the phone's Noise initial message either never arrives
    /// or arrives over a PSK-mismatched tunnel.
    ///
    /// Common causes: no Bluetooth adapter present, user not in
    /// the `bluetooth` group (D-Bus ACL), `bluetoothd` not running,
    /// adapter not yet powered on. The CLI surfaces a hint pointing
    /// the operator at `bluetoothctl power on` and the user-group
    /// fix.
    #[error("caBLE BLE advertisement failed: {0}")]
    Ble(String),
}

// Auto-convert base10 decode failures into `CableError` so callers can
// use `?` uniformly. `DecodeError` is `#[allow(dead_code)]` for its
// variants in production (we only encode), but the `From` impl is
// required by `?` in `HandshakeV2::from_fido_uri`.
impl From<DecodeError> for CableError {
    fn from(e: DecodeError) -> Self {
        match e {
            DecodeError::ContainsNonDigitChars => CableError::ContainsNonDigitChars,
            DecodeError::InvalidLength => CableError::InvalidLength,
            DecodeError::OutOfRange => CableError::OutOfRange,
        }
    }
}
