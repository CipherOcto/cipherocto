//! `#[serde(with = "hex_id_32")]` adapter for 32-byte id-bearing fields.
//!
//! Serialises as a 64-char lowercase hex string — the canonical form that
//! `Caveat::canonical_ser` emits for every 32-byte id it renders.
//! Deserialises from either a 64-char hex string (canonical) OR a 32-element
//! byte array (legacy form preserved for migration).
//!
//! Three things this doc previously got wrong, all caught by the adversarial
//! review of RFC-0011 §Caveat Form Amendment:
//!
//! * It listed the arms that EMIT hex as though they all routed through this
//!   adapter. They did not. `Caveat::Vault` was the only arm that did, and
//!   eight others rendered a 32-byte id as hex from `canonical_ser` while still
//!   using the derived array form on input, so they rejected the hex they
//!   wrote: `InvocationHashBind`, `AskBinding`, `WrappedOnly`,
//!   `RedemptionContext`, `AssetBinding`, `Factory` (`target_vault_id`),
//!   `PolicyReference` (`policy_id`), and `Payment` (16-byte `budget`).
//!   An earlier revision of this doc named only four of those eight, because
//!   the list was cross-checked against other documents instead of against
//!   `canonical_ser`. **All eight now route through this adapter** as of
//!   2026-10-03, which closed seven of the eight; `Payment` still does not
//!   round-trip, for a different reason — see below.
//! * It did not mention `PaymentCaveat::asset_id` and `::nonce`, which now
//!   route through here. They previously used a parallel private adapter that
//!   accepted hex only, so the same canonical form parsed for `Vault` and was
//!   rejected by its siblings in the same crate. `tv_cf_19` pins the parity.
//! * It implied adopting this adapter closes an arm's asymmetry. For an arm
//!   whose `value` is an OBJECT, it does not: `Caveat` is adjacently tagged, so
//!   the encoder must also emit the object shape. The adapter alone leaves the
//!   canonical form unparseable, and a plan that fixes only the adapter will
//!   appear to succeed and change nothing. **Both halves are now done**:
//!   `canonical_ser` emits `{"value": {"<field>": "<hex>"}}` for
//!   `wrapped_only`, `redemption_context`, and `asset_binding`.
//!
//! `Payment` is the one arm still asymmetric, and neither half of the above is
//! its problem: `canonical_ser` emits 4 of `PaymentCaveat`'s 7 fields, so
//! re-input fails on a MISSING FIELD. Only a lossless projection fixes it, and
//! that changes every payment capability's `caveat_body_hash`. See
//! §Known deviations 1.
//!
//! `tv_cf_24` pins the contract this adapter now owes: every fixed-width-id arm
//! round-trips its own canonical form AND still accepts the legacy
//! 32-element array form on input. The second half is the one an
//! adapter-only change can silently break, and it is invisible to any test
//! that only ever sends hex.
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
