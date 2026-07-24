//! Receipt — signed canonical envelope after settlement (S04 Step 4).
//!
//! PR-Q5 (W6): wraps cache-classify + receipt in `ExecutionEnvelope` (RFC-0962).
//! The `Receipt` struct remains the canonical shape on the wire; the
//! `ReceiptEnvelope` projection carries the receipt inside an envelope that
//! also commits the cache-classify metadata + axes-consumed.

use serde::{Deserialize, Serialize};

/// Receipt — proof that settlement completed successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Settlement hash (BLAKE3 32 bytes).
    pub settlement_hash: [u8; 32],
    /// Router identity (signs the receipt).
    pub router_id: String,
    /// Ed25519 signature over `canonical_ser(settlement_hash || asker_did || holder_did || timestamp_unix)`.
    #[serde(with = "ed25519_sig_serde")]
    pub router_sig: ed25519_dalek::Signature,
    /// Unix timestamp.
    pub timestamp_unix: u64,
}

/// Cache-classify metadata (RFC-0959 §Cache classification).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheClassifyMeta {
    /// Cache class (e.g., "exact", "fuzzy", "miss").
    pub cache_class: String,
    /// Cache key hash (BLAKE3 32 bytes); None on miss.
    pub cache_key_hash: Option<[u8; 32]>,
    /// Per-axis consumption (axis name → unit count).
    pub axes_consumed: Vec<(String, u64)>,
}

/// Receipt envelope projection (PR-Q5, W6).
///
/// Carries `Receipt` + `CacheClassifyMeta` inside an `ExecutionEnvelope`
/// wrapper. The wrapper commits both fields via `sql_statements_hash` (RFC-0962 §9 R6-F6)
/// so replay nodes can verify cache-classify + receipt consistency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptEnvelope {
    pub envelope_hash: [u8; 32],
    pub receipt: Receipt,
    pub cache_classify: CacheClassifyMeta,
    pub signature: ed25519_dalek::Signature,
}

/// Ed25519 Signature serde shim.
mod ed25519_sig_serde {
    use ed25519_dalek::Signature;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &Signature, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_bytes(&sig.to_bytes())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Signature, D::Error> {
        let bytes: Vec<u8> = Deserialize::deserialize(de)?;
        Signature::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}

/// Canonical receipt bytes for signing.
#[must_use]
pub fn canonical_receipt_bytes(
    settlement_hash: &[u8; 32],
    asker_did: &str,
    holder_did: &str,
    timestamp: u64,
) -> Vec<u8> {
    let mut msg = Vec::with_capacity(32 + asker_did.len() + holder_did.len() + 8);
    msg.extend_from_slice(settlement_hash);
    msg.extend_from_slice(asker_did.as_bytes());
    msg.extend_from_slice(holder_did.as_bytes());
    msg.extend_from_slice(&timestamp.to_le_bytes());
    msg
}

/// Wrap a `Receipt` + `CacheClassifyMeta` in a `ReceiptEnvelope` (PR-Q5).
///
/// The envelope commits both fields via `sql_statements_hash` (RFC-0962 §9 R6-F6):
/// `envelope_hash = BLAKE3(0xA3 || canonical_ser(receipt || cache_classify))`.
/// Caller signs with the router identity.
#[must_use]
pub fn wrap_receipt_envelope(
    receipt: Receipt,
    cache_classify: CacheClassifyMeta,
) -> (ReceiptEnvelope, Vec<u8>) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[0xA3_u8]); // RFC-0962 §9 R6-F6 sql_statements_hash prefix
    let payload = serde_json::json!({
        "receipt": {
            "settlement_hash": hex::encode(receipt.settlement_hash),
            "router_id": receipt.router_id,
            "timestamp_unix": receipt.timestamp_unix,
        },
        "cache_classify": {
            "cache_class": cache_classify.cache_class,
            "cache_key_hash": cache_classify.cache_key_hash.map(hex::encode),
            "axes_consumed": cache_classify.axes_consumed.iter()
                .map(|(axis, count)| (axis.clone(), *count))
                .collect::<Vec<_>>(),
        },
    });
    let payload_bytes = serde_json::to_vec(&payload).expect("serializable");
    hasher.update(&payload_bytes);
    let envelope_hash = *hasher.finalize().as_bytes();
    let envelope = ReceiptEnvelope {
        envelope_hash,
        receipt,
        cache_classify,
        signature: ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
    };
    (envelope, payload_bytes)
}

/// Sign the envelope via the router's signing key (PR-Q5).
pub fn sign_envelope(
    envelope: &mut ReceiptEnvelope,
    sig: ed25519_dalek::Signature,
) {
    envelope.signature = sig;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_receipt() -> Receipt {
        Receipt {
            settlement_hash: [0xab; 32],
            router_id: "router-test".to_owned(),
            router_sig: ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
            timestamp_unix: 1_700_000_000,
        }
    }

    fn dummy_classify() -> CacheClassifyMeta {
        CacheClassifyMeta {
            cache_class: "exact".to_owned(),
            cache_key_hash: Some([0xcd; 32]),
            axes_consumed: vec![
                ("input_tokens_per_1k".to_owned(), 100),
                ("output_tokens_per_1k".to_owned(), 50),
            ],
        }
    }

    #[test]
    fn canonical_bytes_stable() {
        let h = [0xab; 32];
        let a = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_000);
        let b = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_000);
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_bytes_changes_with_timestamp() {
        let h = [0xab; 32];
        let a = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_000);
        let b = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_001);
        assert_ne!(a, b);
    }

    #[test]
    fn wrap_envelope_deterministic() {
        let r = dummy_receipt();
        let c = dummy_classify();
        let (e1, _) = wrap_receipt_envelope(r.clone(), c.clone());
        let (e2, _) = wrap_receipt_envelope(r, c);
        assert_eq!(e1.envelope_hash, e2.envelope_hash);
    }

    #[test]
    fn wrap_envelope_differs_for_different_receipt() {
        let c = dummy_classify();
        let r1 = dummy_receipt();
        let mut r2 = dummy_receipt();
        r2.settlement_hash = [0x99; 32];
        let (e1, _) = wrap_receipt_envelope(r1, c.clone());
        let (e2, _) = wrap_receipt_envelope(r2, c);
        assert_ne!(e1.envelope_hash, e2.envelope_hash);
    }

    #[test]
    fn wrap_envelope_differs_for_different_classify() {
        let r = dummy_receipt();
        let c1 = dummy_classify();
        let mut c2 = dummy_classify();
        c2.cache_class = "miss".to_owned();
        let (e1, _) = wrap_receipt_envelope(r.clone(), c1);
        let (e2, _) = wrap_receipt_envelope(r, c2);
        assert_ne!(e1.envelope_hash, e2.envelope_hash);
    }

    #[test]
    fn sign_envelope_overwrites_signature() {
        let (mut envelope, _) = wrap_receipt_envelope(dummy_receipt(), dummy_classify());
        let sig = ed25519_dalek::Signature::from_bytes(&[0x42; 64]);
        sign_envelope(&mut envelope, sig);
        assert_eq!(envelope.signature.to_bytes(), [0x42; 64]);
    }
}
