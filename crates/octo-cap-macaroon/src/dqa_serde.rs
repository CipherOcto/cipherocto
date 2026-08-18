//! Manual `Serialize`/`Deserialize` impls for `octo_determin::Dqa`.
//!
//! `Dqa` does not derive `Serialize`/`Deserialize` (per
//! `determin/src/dqa.rs:104` — only `Clone, Copy, Debug, PartialEq, Eq, Hash`).
//! The canonical wire format is `DqaEncoding` (16-byte BE:
//! `value: i64` (8 bytes) + `scale: u8` (1 byte) + `_reserved: [u8; 7]`),
//! used here as the on-the-wire serializer/deserializer.
//!
//! Equivalent to `quota-router-storage/src/dqa_serde` (consumers
//! independently own the helper — `octo-cap-macaroon` cannot depend on
//! `quota-router-storage` per layer model; duplication is the
//! correct trade until `octo-determin` exposes the helper itself).

use octo_determin::{Dqa, DqaEncoding, DqaError};

/// Wire size of the encoded `Dqa` (matches `DqaEncoding` size assertion).
const WIRE_BYTES: usize = 16;

/// Encode a `Dqa` to its canonical 16-byte BE wire representation.
#[must_use]
pub fn dqa_to_bytes(d: &Dqa) -> [u8; WIRE_BYTES] {
    let enc = DqaEncoding::from_dqa(d);
    let mut out = [0u8; WIRE_BYTES];
    out[0..8].copy_from_slice(&enc.value.to_le_bytes());
    out[8] = enc.scale;
    out[9..16].copy_from_slice(&enc._reserved);
    out
}

/// Decode a `Dqa` from its canonical 16-byte BE wire representation.
///
/// # Errors
/// Returns `DqaError::InvalidScale` if scale exceeds `MAX_SCALE`,
/// `DqaError::InvalidEncoding` if reserved bytes are non-zero, or
/// `DqaError::InvalidValue` if the value is malformed.
pub fn dqa_from_bytes(bytes: &[u8]) -> Result<Dqa, DqaError> {
    if bytes.len() != WIRE_BYTES {
        return Err(DqaError::InvalidEncoding);
    }
    let mut value_be = [0u8; 8];
    value_be.copy_from_slice(&bytes[0..8]);
    let scale = bytes[8];
    let mut reserved = [0u8; 7];
    reserved.copy_from_slice(&bytes[9..16]);
    let enc = DqaEncoding {
        value: i64::from_be_bytes(value_be).swap_bytes(),
        scale,
        _reserved: reserved,
    };
    enc.to_dqa()
}

/// `#[serde(with = "...")]` helper for `Dqa` fields inside
/// serde-derived structs.
pub mod field {
    use super::{dqa_from_bytes, dqa_to_bytes, Dqa};
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Dqa, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&dqa_to_bytes(d))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Dqa, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Dqa;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "16-byte BE DqaEncoding")
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                dqa_from_bytes(v).map_err(|e| de::Error::custom(format!("Dqa decode: {e:?}")))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut buf = Vec::with_capacity(super::WIRE_BYTES);
                while let Some(b) = seq.next_element::<u8>()? {
                    buf.push(b);
                }
                dqa_from_bytes(&buf).map_err(|e| de::Error::custom(format!("Dqa decode: {e:?}")))
            }
        }
        d.deserialize_bytes(V)
    }
}
