//! Manual `Serialize`/`Deserialize` impls for `octo_determin::Dqa`.
//!
//! `Dqa` does not derive `Serialize`/`Deserialize` (per
//! `determin/src/dqa.rs:104` — only `Clone, Copy, Debug, PartialEq, Eq, Hash`).
//! The canonical wire format is [`DqaEncoding`] (16-byte BE:
//! `value: i64` (8 bytes) + `scale: u8` (1 byte) + `_reserved: [u8; 7]`),
//! used here as the on-the-wire serializer/deserializer.
//!
//! Why manual impls (not `#[serde(transparent)]` over `DqaEncoding`):
//! consumer crates own [`Dqa`] directly (e.g. `settlement_event_repo::Row`
//! has `cost: Dqa`, not `cost: DqaEncoding`). The custom impl round-trips
//! through `DqaEncoding` to preserve canonicalization (consensus invariant:
//! two nodes replaying the same bytes MUST produce identical `Dqa` values).
//!
//! Wire format:
//! - `serialize_bytes(&[u8; 16])` — 16-byte BE `DqaEncoding`
//! - `deserialize_bytes` accepts 16 bytes, validates `scale <= MAX_SCALE`
//!   and `_reserved == 0`, then `DqaEncoding::to_dqa()` for final decode

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use octo_determin::{Dqa, DqaEncoding, DqaError};

/// Wire size of the encoded `Dqa` (matches `DqaEncoding` size assertion).
const WIRE_BYTES: usize = 16;

/// Encode a `Dqa` to its canonical 16-byte BE wire representation.
#[must_use]
pub fn dqa_to_bytes(d: &Dqa) -> [u8; WIRE_BYTES] {
    let enc = DqaEncoding::from_dqa(d);
    let mut out = [0u8; WIRE_BYTES];
    // `DqaEncoding::from_dqa` stores `enc.value` as the byte-swapped
    // (BE-form-in-memory) view of the canonical Dqa value. Reading
    // that representation back as native-endian bytes via
    // `to_le_bytes()` recovers the canonical big-endian wire form;
    // calling `to_be_bytes()` here would double-swap and corrupt
    // the encoding.
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
    // Mirror of `dqa_to_bytes`: the canonical BE form goes into the
    // `DqaEncoding`'s `value` field byte-swapped (so the in-memory
    // layout matches native endianness on LE machines). We swap
    // here to recover the numerical i64.
    let enc = DqaEncoding {
        value: i64::from_be_bytes(value_be).swap_bytes(),
        scale,
        _reserved: reserved,
    };
    enc.to_dqa()
}

/// `serde::Serialize`/`Deserialize` for `Dqa` via `#[serde(serialize_with/deserialize_with)]`.
///
/// Rust orphan rules prevent foreign-trait-on-foreign-type impls
/// (e.g. `impl Serialize for octo_determin::Dqa`), so consumers wire these
/// helpers explicitly on each `Dqa` field:
/// ```ignore
/// #[derive(Serialize, Deserialize)]
/// pub struct Foo {
///     #[serde(with = "crate::dqa_serde::field")]
///     pub amount: Dqa,
/// }
/// ```
pub mod field {
    use super::{Dqa, Serializer};
    use serde::de::{self, Visitor};
    use serde::Deserializer;

    pub fn serialize<S: Serializer>(d: &Dqa, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&super::dqa_to_bytes(d))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Dqa, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Dqa;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "16-byte BE DqaEncoding")
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                super::dqa_from_bytes(v)
                    .map_err(|e| de::Error::custom(format!("Dqa decode: {e:?}")))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut buf = Vec::with_capacity(super::WIRE_BYTES);
                while let Some(b) = seq.next_element::<u8>()? {
                    buf.push(b);
                }
                super::dqa_from_bytes(&buf)
                    .map_err(|e| de::Error::custom(format!("Dqa decode: {e:?}")))
            }
        }
        d.deserialize_bytes(V)
    }
}

/// `serde::Serialize` impl for `DqaSerde` — emits 16 raw bytes via `serialize_bytes`.
impl Serialize for DqaSerde {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&dqa_to_bytes(&self.0))
    }
}

/// `serde::Deserialize` impl for `DqaSerde` — reads 16 raw bytes, validates, decodes.
impl<'de> Deserialize<'de> for DqaSerde {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = DqaSerde;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "16-byte BE DqaEncoding")
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                dqa_from_bytes(v)
                    .map(DqaSerde)
                    .map_err(|e| de::Error::custom(format!("Dqa decode: {e:?}")))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut buf = Vec::with_capacity(WIRE_BYTES);
                while let Some(b) = seq.next_element::<u8>()? {
                    buf.push(b);
                }
                dqa_from_bytes(&buf)
                    .map(DqaSerde)
                    .map_err(|e| de::Error::custom(format!("Dqa decode: {e:?}")))
            }
        }
        d.deserialize_bytes(V)
    }
}

/// Newtype wrapper around `Dqa` that exposes serde `Serialize`/`Deserialize`.
///
/// Use this when placing a `Dqa` directly inside a serde-derived struct
/// (e.g. `#[derive(Serialize, Deserialize)] pub struct Row { cost: DqaSerde }`).
/// The wrapper is `#[repr(transparent)]` over `Dqa` for zero-cost access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct DqaSerde(pub Dqa);

impl DqaSerde {
    /// Wrap a `Dqa` for serde.
    #[must_use]
    pub const fn new(d: Dqa) -> Self {
        Self(d)
    }

    /// Unwrap to inner `Dqa`.
    #[must_use]
    pub const fn into_inner(self) -> Dqa {
        self.0
    }
}

impl From<Dqa> for DqaSerde {
    fn from(d: Dqa) -> Self {
        Self(d)
    }
}

impl From<DqaSerde> for Dqa {
    fn from(s: DqaSerde) -> Self {
        s.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_zero() {
        let z = DqaSerde::from(Dqa::new(0, 0).expect("zero"));
        let bytes = dqa_to_bytes(&z.0);
        assert_eq!(bytes, [0u8; 16]);
        let back = dqa_from_bytes(&bytes).expect("decode");
        assert_eq!(back, z.0);
    }

    #[test]
    fn round_trip_set_scale_12() {
        // `DqaEncoding::from_dqa` canonicalizes (strips trailing
        // zeros), so (1_000_000, 12) → (1, 6). The wire form is the
        // canonical form; round-trip preserves the canonical Dqa,
        // not the input — that is the consensus-bytes contract.
        let d = Dqa::new(1_000_000, 12).expect("non-overflow");
        let canonical = Dqa::new(1, 6).expect("non-overflow");
        let bytes = dqa_to_bytes(&d);
        // value = 1 (i64) = 0x0000000000000001
        assert_eq!(&bytes[0..8], &[0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(bytes[8], 6);
        assert_eq!(&bytes[9..16], &[0u8; 7]);
        let back = dqa_from_bytes(&bytes).expect("decode");
        assert_eq!(back, canonical);
    }

    #[test]
    fn rejects_reserved_nonzero() {
        let mut bytes = [0u8; 16];
        bytes[15] = 0xFF; // reserved byte non-zero
        let err = dqa_from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, DqaError::InvalidEncoding));
    }

    #[test]
    fn rejects_wrong_length() {
        let err = dqa_from_bytes(&[0u8; 8]).unwrap_err();
        assert!(matches!(err, DqaError::InvalidEncoding));
    }

    #[test]
    fn rejects_scale_over_max() {
        let mut bytes = [0u8; 16];
        bytes[8] = 19; // > MAX_SCALE (18)
        let err = dqa_from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, DqaError::InvalidScale));
    }

    #[test]
    fn serde_json_round_trip() {
        let d = Dqa::new(42, 6).expect("non-overflow");
        let wrapped = DqaSerde::from(d);
        let json = serde_json::to_vec(&wrapped).expect("serialize");
        let back: DqaSerde = serde_json::from_slice(&json).expect("deserialize");
        assert_eq!(back.0, d);
    }
}
