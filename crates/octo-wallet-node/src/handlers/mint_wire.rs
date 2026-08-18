//! `WALLET_MINT_CAPABILITY` wire form (mission 0862-c9 RETIRED).
//!
//! The typed `MintRequest` struct no longer carries borsh derives
//! (`PaymentCaveat::budget` is `Dqa`, which does not impl
//! `BorshSerialize` / `BorshDeserialize` in the upstream git dep).
//! This module defines the **JSON wire form** that the wallet-node
//! dispatch reads + writes at the envelope boundary, plus converters
//! to / from the typed struct.
//!
//! Wire form is JSON via `serde_json`. `holder_did` is a string;
//! `capability` is a 64-char hex string (32 bytes); `payment_caveat`
//! is `null` or the canonical `PaymentCaveat` JSON shape (which itself
//! uses the `dqa_serde::field` 16-byte BE form for `budget`).
//!
//! ## Why JSON
//!
//! Borsh was the previous wire form (mission 0862-c9 RETIRED). JSON
//! is the next-best substrate that requires no upstream `Dqa`
//! trait impls and round-trips typed ↔ wire without bespoke
//! serializer code. The eventual re-introduction of borsh for the
//! `WALLET_MINT_CAPABILITY` envelope awaits the follow-on mission
//! that ships `BorshSerialize`/`BorshDeserialize` for `Dqa` upstream.
use std::fmt::Write as _;

use octo_cap_macaroon::PaymentCaveat;
use serde_json::{json, Value};

/// 32-byte capability root encoded as a 64-char hex string.
fn bytes_to_hex(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn hex_to_bytes(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!(
            "expected 64-char hex string, got length {}",
            s.len()
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(chunk).map_err(|e| format!("hex utf8: {e}"))?;
        out[i] = u8::from_str_radix(pair, 16).map_err(|e| format!("hex parse: {e}"))?;
    }
    Ok(out)
}

/// Convert a `MintRequest` to its JSON wire form.
#[must_use]
pub fn request_to_value(req: &super::mint::MintRequest) -> Value {
    let payment_caveat = req
        .payment_caveat
        .as_ref()
        .map(|p| serde_json::to_value(p).expect("PaymentCaveat is JSON-safe"));
    json!({
        "holder_did": req.holder_did,
        "capability": bytes_to_hex(&req.capability),
        "payment_caveat": payment_caveat,
    })
}

/// Decode a `MintRequest` from a JSON wire form.
///
/// # Errors
/// Returns `String` describing the failure mode (missing field,
/// wrong type, out-of-range), suitable for surfacing as
/// `ProtocolError::AuthorizationFailed` at the dispatch boundary.
pub fn request_from_value(v: &Value) -> Result<super::mint::MintRequest, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "expected request object".to_owned())?;
    let holder_did = obj
        .get("holder_did")
        .and_then(Value::as_str)
        .ok_or_else(|| "holder_did missing or not string".to_owned())?
        .to_owned();
    let cap_hex = obj
        .get("capability")
        .and_then(Value::as_str)
        .ok_or_else(|| "capability missing or not string".to_owned())?;
    let capability = hex_to_bytes(cap_hex)?;
    let payment_caveat = match obj.get("payment_caveat") {
        Some(Value::Null) | None => None,
        Some(v) => {
            let p: PaymentCaveat =
                serde_json::from_value(v.clone()).map_err(|e| format!("payment_caveat: {e}"))?;
            Some(p)
        }
    };
    Ok(super::mint::MintRequest {
        holder_did,
        capability,
        payment_caveat,
    })
}

/// Encode the `MintRequest` as JSON bytes.
///
/// # Errors
/// Returns `String` describing any serialization failure.
pub fn request_to_bytes(req: &super::mint::MintRequest) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&request_to_value(req)).map_err(|e| format!("request ser: {e}"))
}

/// Decode a `MintRequest` from the dispatch wire bytes.
///
/// # Errors
/// Returns `String` describing the failure mode.
pub fn request_from_bytes(bytes: &[u8]) -> Result<super::mint::MintRequest, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("request de: {e}"))?;
    request_from_value(&v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_32_bytes() {
        let bytes: [u8; 32] = [0xab; 32];
        let hex = bytes_to_hex(&bytes);
        assert_eq!(hex.len(), 64);
        let back = hex_to_bytes(&hex).unwrap();
        assert_eq!(back, bytes);
    }

    #[test]
    fn hex_to_bytes_rejects_bad_length() {
        assert!(hex_to_bytes("abc").is_err());
    }

    #[test]
    fn hex_to_bytes_rejects_invalid_chars() {
        assert!(hex_to_bytes(&"zz".repeat(32)).is_err());
    }
}
