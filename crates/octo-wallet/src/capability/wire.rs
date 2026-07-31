//! Wire format for capability tokens (RFC-0957 §3.7 + RFC-0958 §4 wire).
//!
//! v1 wire format (3 segments, base64url no padding):
//! ```text
//! capability_token_v1 := base64url(macaroon_borsh)
//!                     || "."
//!                     || base64url(holder_sig)
//!                     || "."
//!                     || base64url(discharges_borsh)
//! ```
//!
//! v2 wire format (RFC-0958 — 4 segments, optional 4th for `proof_bundle_borsh`):
//! ```text
//! capability_token_v2 := base64url(macaroon_borsh)
//!                     || "."
//!                     || base64url(holder_sig)
//!                     || "."
//!                     || base64url(discharges_borsh)
//!                     || "."
//!                     || base64url(proof_bundle_borsh)   // v2 ONLY
//! ```
//!
//! **Forward compat:** v1 parsers (`deserialize_wire`) split on `.` and
//! take the first 3 segments (silently ignoring the v2 4th). v2 parsers
//! (`deserialize_wire_v2`) detect 4-segment wire and extract
//! `proof_bundle_borsh` for downstream STARK verify.
//!
//! **Backward compat:** v2 emitter (`serialize_wire_v2`) emits 4 segments
//! iff the caller supplies `Some(proof_bundle_bytes)`, else 3 segments
//! (the v1 shape). v2 parsers accept both 3 and 4 segment wires.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use super::CapabilityToken;

/// Result of parsing a v2 wire format: token + optional `proof_bundle_borsh`.
///
/// `CapabilityToken` does NOT derive `Eq` (ed25519 signature field), so we
/// derive `Clone` + `Debug` only. Field-by-field comparisons are explicit
/// at call sites.
#[derive(Debug, Clone)]
pub struct WireV2 {
    /// Capability token (segments 1..3).
    pub token: CapabilityToken,
    /// Optional v2 4th segment: `proof_bundle_borsh` (RFC-0958).
    pub proof_bundle: Option<Vec<u8>>,
}

/// Serialize a `CapabilityToken` to v1 wire format (3 segments).
///
/// **Forward-compat:** if the caller has a v2 wire with an embedded
/// `proof_bundle`, use [`serialize_wire_v2`] instead. This function
/// emits the v1 shape (3 segments) unconditionally.
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
/// **Forward-compat:** if the wire carries 4 segments (v2 format), the
/// 4th is silently ignored. v1 callers don't need to know about v2.
///
/// # Errors
/// Returns `WireError::Parse` on malformed wire format.
pub fn deserialize_wire(
    s: &str,
    holder_did: impl Into<String>,
    holder_pub: [u8; 32],
) -> Result<CapabilityToken, WireError> {
    let parsed = parse_wire_to_token(s, holder_did, holder_pub)?;
    // v1 path discards the v2 4th segment silently.
    Ok(parsed.token)
}

/// Serialize a `CapabilityToken` + optional `proof_bundle` to v2 wire
/// format. Emits 4 segments iff `opt_proof_bundle` is `Some(_)`, else 3
/// segments (the v1 shape).
///
/// # Errors
/// Returns `WireError::Serialize` on internal borsh failure.
pub fn serialize_wire_v2(
    token: &CapabilityToken,
    opt_proof_bundle: Option<&[u8]>,
) -> Result<String, WireError> {
    let base = serialize_wire(token)?;
    match opt_proof_bundle {
        Some(pb) => {
            let s4 = URL_SAFE_NO_PAD.encode(pb);
            Ok(format!("{base}.{s4}"))
        }
        None => Ok(base),
    }
}

/// Deserialize a v2 wire into token + optional proof bundle bytes.
///
/// Accepts both 3-segment (v1) and 4-segment (v2) wire formats. Returns
/// `proof_bundle = None` for v1 wire; `proof_bundle = Some(bytes)` for
/// v2. Used by `verify_capability_zk` downstream callers that need
/// access to the embedded STARK proof.
///
/// # Errors
/// Returns `WireError::Parse` on malformed wire format.
pub fn deserialize_wire_v2(
    s: &str,
    holder_did: impl Into<String>,
    holder_pub: [u8; 32],
) -> Result<WireV2, WireError> {
    let parsed = parse_wire_to_token(s, holder_did, holder_pub)?;
    Ok(WireV2 {
        token: parsed.token,
        proof_bundle: parsed.proof_bundle,
    })
}

/// Internal parser shared between v1 + v2 deserializers.
///
/// Accepts 3 or 4 segments. Returns the parsed `CapabilityToken` plus the
/// optional 4th segment bytes (base64url-decoded).
struct ParsedWire {
    token: CapabilityToken,
    proof_bundle: Option<Vec<u8>>,
}

fn parse_wire_to_token(
    s: &str,
    holder_did: impl Into<String>,
    holder_pub: [u8; 32],
) -> Result<ParsedWire, WireError> {
    // DoS guard #1: total wire length cap BEFORE split. Prevents
    // an attacker-supplied 100MB string from allocating on split
    // + b64 decode + JSON parse.
    if s.len() > MAX_WIRE_TOTAL {
        return Err(WireError::WireTooLong(s.len(), MAX_WIRE_TOTAL));
    }

    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 && parts.len() != 4 {
        return Err(WireError::SegmentCount(parts.len()));
    }

    // DoS guard #2: per-segment length cap. Each base64url-encoded
    // segment may not exceed MAX_SEGMENT_BYTES (1 MiB).
    for (i, seg) in parts.iter().enumerate() {
        if seg.len() > MAX_SEGMENT_BYTES {
            return Err(WireError::SegmentTooLong(i, seg.len(), MAX_SEGMENT_BYTES));
        }
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

    let proof_bundle = if parts.len() == 4 {
        let pb = URL_SAFE_NO_PAD
            .decode(parts[3])
            .map_err(|e| WireError::Parse(format!("proof_bundle b64: {e}")))?;
        Some(pb)
    } else {
        None
    };

    Ok(ParsedWire {
        token: CapabilityToken {
            macaroon,
            holder_pub,
            holder_did: holder_did.into(),
            holder_sig,
            discharges,
        },
        proof_bundle,
    })
}

/// Maximum total wire length (RFC-0958 §Performance; protects against
/// OOM via attacker-supplied huge s4 segments + b64 decode amplification).
/// 64 KiB is well over the maximum real wire (a SelfHost 500KB proof
/// would still fit since the 4th segment carries base64-encoded bytes,
/// 4/3×500KB ≈ 670KB; we set the cap to allow real-zk proofs once the
/// FFI ships).
pub const MAX_WIRE_TOTAL: usize = 2 * 1024 * 1024; // 2 MiB defensive cap
/// Maximum per-segment raw length (before base64 decode).
pub const MAX_SEGMENT_BYTES: usize = 1024 * 1024; // 1 MiB per segment

/// Wire format errors.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("serialize error: {0}")]
    Serialize(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("expected 3 or 4 wire segments, got {0}")]
    SegmentCount(usize),

    #[error("wire total length {0} exceeds cap of {1} bytes (DoS guard)")]
    WireTooLong(usize, usize),

    #[error("segment[{0}] length {1} exceeds cap of {2} bytes (DoS guard)")]
    SegmentTooLong(usize, usize, usize),
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
        assert_eq!(back.macaroon.root_id, token.macaroon.root_id);
        assert_eq!(back.macaroon.caveats, token.macaroon.caveats);
        assert_eq!(back.holder_did, token.holder_did);
        assert_eq!(back.holder_pub, token.holder_pub);
    }

    #[test]
    fn v1_parser_ignores_v2_fourth_segment() {
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
        // Synthesize v2 wire: append a 4th segment.
        let pb_bytes = b"fake_proof_bundle_for_v2_test";
        let wire_v2 = serialize_wire_v2(&token, Some(pb_bytes)).unwrap();
        let segments: Vec<&str> = wire_v2.split('.').collect();
        assert_eq!(segments.len(), 4);

        // v1 deserializer ignores segment 4 and recovers the token.
        let back = deserialize_wire(&wire_v2, "did:octo:test", token.holder_pub).unwrap();
        assert_eq!(back.macaroon.root_id, token.macaroon.root_id);
    }
}
