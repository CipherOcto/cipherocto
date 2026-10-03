//! `#[serde(with = "hex_id_32")]` adapter for 32-byte id newtypes.
//!
//! Serialises as a 64-char lowercase hex string (the canonical form
//! already emitted by `Caveat::canonical_ser` for `Vault`, `AskBinding`,
//! `WrappedOnly`, `RedemptionContext`, `InvocationHashBind`).
//! Deserialises from either a 64-char hex string OR a 32-element byte
//! array (legacy form preserved for migration).
//!
//! RFC-0011 §Caveat Form Amendment: this adapter is the substrate-owned
//! wire form for any 32-byte id newtype that crosses the canonical
//! boundary.

use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};
use std::fmt;

pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&hex::encode(bytes))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = [u8; 32];
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "32-byte array or 64-char hex string")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            let bytes = hex::decode(v)
                .map_err(|e| de::Error::custom(format!("hex_id_32 hex decode: {e}")))?;
            if bytes.len() != 32 {
                return Err(de::Error::custom(format!(
                    "hex_id_32 expected 32 bytes, got {n}",
                    n = bytes.len()
                )));
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&bytes);
            Ok(out)
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(v.as_str())
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut buf = Vec::with_capacity(32);
            while let Some(b) = seq.next_element::<u8>()? {
                buf.push(b);
            }
            if buf.len() != 32 {
                return Err(de::Error::custom(format!(
                    "hex_id_32 expected 32 bytes, got {n}",
                    n = buf.len()
                )));
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&buf);
            Ok(out)
        }
    }
    d.deserialize_any(V)
}
