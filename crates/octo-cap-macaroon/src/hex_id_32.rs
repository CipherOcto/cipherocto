//! `#[serde(with = "hex_id_32")]` adapter for 32-byte id-bearing fields.
//!
//! Serialises as a 64-char lowercase hex string — the canonical form that
//! `Caveat::canonical_ser` emits for every 32-byte id it renders.
//! Deserialises from either a 64-char hex string (canonical) OR a 32-element
//! byte array (legacy form preserved for migration).
//!
//! Two things this doc previously got wrong, both caught by the adversarial
//! review of RFC-0011 §Caveat Form Amendment:
//!
//! * It listed the arms that EMIT hex as though they all routed through this
//!   adapter. They do not. `Caveat::Vault` does. `WrappedOnly`,
//!   `RedemptionContext`, `InvocationHashBind`, and `AskBinding` emit hex from
//!   `canonical_ser` but still use the derived array form on input, so they
//!   reject the hex they write. See §Known deviations 3 in the RFC, and
//!   `tv_cf_20` / `tv_cf_21` for the current set.
//! * It did not mention `PaymentCaveat::asset_id` and `::nonce`, which now
//!   route through here. They previously used a parallel private adapter that
//!   accepted hex only, so the same canonical form parsed for `Vault` and was
//!   rejected by its siblings in the same crate. `tv_cf_19` pins the parity.
//!
//! Dispatch is via `deserialize_any`, which requires a SELF-DESCRIBING serde
//! format. JSON and MessagePack qualify; bincode and postcard do not. A type
//! carrying a 32-byte id cannot be read through a non-self-describing format
//! once this adapter is on the path, and the failure is a runtime error, not a
//! compile error. Nothing in this crate does that today, but the constraint is
//! load-bearing for anyone adding a format.
//!
//! RFC-0011 §Caveat Form Amendment: this adapter is the substrate-owned wire
//! form for any 32-byte id-bearing field that crosses the canonical boundary.

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
