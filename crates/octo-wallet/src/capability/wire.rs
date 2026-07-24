//! Wire format for capability tokens (RFC-0957 §3.7).
//!
//! v1 wire format (3 segments, base64url no padding):
//! ```text
//! capability_token_v1 := base64url(macaroon_borsh)
//!                     || "."
//!                     || base64url(holder_sig)
//!                     || "."
//!                     || base64url(discharges_borsh)
//! ```
//! RFC-0958 adds an optional 4th segment for `proof_bundle_borsh`; S05 owns.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use super::CapabilityToken;

/// Serialize a `CapabilityToken` to v1 wire format (3 segments).
///
/// # Errors
/// Returns `WireError::Serialize` on internal borsh failure (should not
/// happen for valid tokens).
pub fn serialize_wire(token: &CapabilityToken) -> Result<String, WireError> {
    let macaroon_bytes =
        borsh_compat::to_vec(&token.macaroon).map_err(|e| WireError::Serialize(e.clone()))?;
    let sig_bytes = token.holder_sig.to_bytes();
    let discharges_bytes =
        borsh_compat::to_vec(&token.discharges).map_err(|e| WireError::Serialize(e.clone()))?;

    let s1 = URL_SAFE_NO_PAD.encode(&macaroon_bytes);
    let s2 = URL_SAFE_NO_PAD.encode(sig_bytes);
    let s3 = URL_SAFE_NO_PAD.encode(&discharges_bytes);

    Ok(format!("{s1}.{s2}.{s3}"))
}

/// Deserialize a `CapabilityToken` from v1 wire format (3 segments).
///
/// Holder DID + public key are NOT in the wire format — caller passes them
/// as parameters (resolved out-of-band from a DID registry).
///
/// # Errors
/// Returns `WireError::Parse` on malformed wire format.
pub fn deserialize_wire(
    s: &str,
    holder_did: impl Into<String>,
    holder_pub: [u8; 32],
) -> Result<CapabilityToken, WireError> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return Err(WireError::SegmentCount(parts.len()));
    }

    let macaroon_bytes = URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|e| WireError::Parse(format!("macaroon b64: {e}")))?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| WireError::Parse(format!("sig b64: {e}")))?;
    let discharges_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|e| WireError::Parse(format!("discharges b64: {e}")))?;

    if sig_bytes.len() != 64 {
        return Err(WireError::Parse(format!(
            "sig must be 64 bytes, got {}",
            sig_bytes.len()
        )));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let holder_sig = ed25519_dalek::Signature::from_slice(&sig_arr)
        .map_err(|e| WireError::Parse(format!("sig: {e}")))?;

    let macaroon: super::Macaroon = borsh_compat::from_slice(&macaroon_bytes)
        .map_err(|e| WireError::Parse(format!("macaroon borsh: {e}")))?;
    let discharges: Vec<super::DischargeMacaroon> = borsh_compat::from_slice(&discharges_bytes)
        .map_err(|e| WireError::Parse(format!("discharges borsh: {e}")))?;

    Ok(CapabilityToken {
        macaroon,
        holder_pub,
        holder_did: holder_did.into(),
        holder_sig,
        discharges,
    })
}

/// Wire format errors.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("serialize error: {0}")]
    Serialize(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("expected 3 wire segments, got {0}")]
    SegmentCount(usize),
}

/// Minimal borsh-compatible serialization shim using serde_json for S02 MVP.
///
/// **TODO (S05):** replace with proper borsh per RFC-0957 §3.7 wire spec
/// ("`proof_bundle_borsh`" implies borsh elsewhere too). For S02 we use
/// serde_json to avoid pulling in borsh crate; canonical_ser already provides
/// determinism for caveats.
mod borsh_compat {
    pub fn to_vec<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
        serde_json::to_vec(value).map_err(|e| e.to_string())
    }

    pub fn from_slice<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::caveat::Caveat;
    use crate::capability::macaroon::InMemoryCatalog;
    use crate::identity::IdentityKey;

    #[test]
    fn wire_format_three_segments() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let catalog = InMemoryCatalog::default();
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            "did:octo:test",
            vec![Caveat::Before(1_700_000_000)],
            &catalog,
        )
        .unwrap();
        let wire = serialize_wire(&token).unwrap();
        let segments: Vec<&str> = wire.split('.').collect();
        assert_eq!(segments.len(), 3);
        for seg in &segments {
            assert!(!seg.is_empty());
        }
    }

    #[test]
    fn wire_roundtrip() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let catalog = InMemoryCatalog::default();
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            "did:octo:test",
            vec![Caveat::Before(1_700_000_000)],
            &catalog,
        )
        .unwrap();
        let wire = serialize_wire(&token).unwrap();
        let back = deserialize_wire(&wire, "did:octo:test", token.holder_pub).unwrap();
        // Macaroon root_id + caveats must match.
        assert_eq!(back.macaroon.root_id, token.macaroon.root_id);
        assert_eq!(back.macaroon.caveats, token.macaroon.caveats);
        assert_eq!(back.holder_did, token.holder_did);
        assert_eq!(back.holder_pub, token.holder_pub);
    }
}
