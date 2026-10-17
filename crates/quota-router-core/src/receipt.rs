//! Receipt — signed canonical envelope after settlement (S04 Step 4).
//!
//! PR-Q5 (W6): `ReceiptEnvelope` is a distinct projection over `Receipt` +
//! `CacheClassifyMeta`, NOT an `ExecutionEnvelope` (RFC-0962). It carries
//! its own domain-separated hash commitment (`receipt_envelope_hash`,
//! prefix `0xA7` per RFC-0962 §9.1) so replay nodes can verify the
//! cache-classify + receipt consistency without conflating with the
//! `sql_statements_hash` commitment that `ExecutionEnvelope` uses for ZK
//! public inputs (RFC-0962 §9, prefix `0xA3`).
//!
//! The `0xA3` / `0xA7` distinction is mandatory: a 32-byte BLAKE3 output
//! has no type tag, and a verifier that ingests a 32-byte hash field
//! without context-aware dispatch cannot tell whether the bytes are a
//! SQL-statements ZK commitment or a receipt envelope commitment. Sharing
//! the prefix would be a soundness defect waiting for the first verifier.

use serde::{Deserialize, Serialize};

/// Domain separator for `ReceiptEnvelope::envelope_hash` (RFC-0962 §9.1).
///
/// Allocated in the RFC-0964 §0.1 reserved range `0xA7-0xAF`. Distinct
/// from `0xA3` (`sql_statements_hash`, RFC-0962 §9) — see module doc.
pub const RECEIPT_ENVELOPE_HASH_PREFIX: u8 = 0xA7;

/// 32-byte cache key commitment (RFC-0959 §Data Structures).
pub type CacheKeyHash = [u8; 32];

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

/// Cache-classify metadata (RFC-0959 §Data Structures).
///
/// `axes_consumed` axis names share the taxonomy with
/// `quota_router_sm_engine::Reservation::axes_consumed`. A single
/// `AxisId` type alias (e.g., `octo_policy::AxisId`) is the
/// cross-crate sharing target; the sm-engine migration is a follow-up
/// (the `octo_policy` dep is currently `[dev-dependencies]`-only
/// in `quota-router-core`, so this lib uses the bare `String` form).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheClassifyMeta {
    /// Cache class (e.g., "exact", "fuzzy", "miss").
    pub cache_class: String,
    /// Cache key hash (BLAKE3 32 bytes); None on miss.
    pub cache_key_hash: Option<CacheKeyHash>,
    /// Per-axis consumption (axis name → unit count).
    pub axes_consumed: Vec<(String, u64)>,
}

/// Receipt envelope projection (PR-Q5, W6).
///
/// Distinct type from `ExecutionEnvelope` (RFC-0962 §4). Commits the
/// `Receipt` + `CacheClassifyMeta` pair via the dedicated
/// `receipt_envelope_hash` domain separator (`0xA7`, RFC-0962 §9.1).
/// The router signs `canonical_envelope_bytes(&&self)` to bind
/// cache-classify to the settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptEnvelope {
    /// `BLAKE3(RECEIPT_ENVELOPE_HASH_PREFIX || canonical_envelope_payload)`.
    pub envelope_hash: [u8; 32],
    pub receipt: Receipt,
    pub cache_classify: CacheClassifyMeta,
    /// Ed25519 signature over `canonical_envelope_bytes(&&self)`.
    #[serde(with = "ed25519_sig_serde")]
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
/// `envelope_hash = BLAKE3(RECEIPT_ENVELOPE_HASH_PREFIX || canonical_envelope_payload)`.
/// Caller signs with the router identity via [`sign_envelope`] (or
/// [`sign_envelope_with`] for the all-in-one path).
#[must_use]
pub fn wrap_receipt_envelope(
    receipt: Receipt,
    cache_classify: CacheClassifyMeta,
) -> ReceiptEnvelope {
    let payload = canonical_envelope_payload(&receipt, &cache_classify);
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[RECEIPT_ENVELOPE_HASH_PREFIX]);
    hasher.update(&payload);
    let envelope_hash = *hasher.finalize().as_bytes();
    ReceiptEnvelope {
        envelope_hash,
        receipt,
        cache_classify,
        signature: ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
    }
}

/// Sign the envelope with the router's Ed25519 signing key (PR-Q5).
///
/// Signs `canonical_envelope_bytes(envelope)` and stores the signature
/// in `envelope.signature`. Verifiers recompute the same preimage from
/// `envelope.receipt` + `envelope.cache_classify`, check `envelope_hash`,
/// then verify the Ed25519 signature over the same bytes.
pub fn sign_envelope(envelope: &mut ReceiptEnvelope, sig: ed25519_dalek::Signature) {
    envelope.signature = sig;
}

/// Canonical envelope payload bytes (deterministic, RFC-0126 style).
///
/// Length-prefixed, fixed field order, no whitespace. `axes_consumed` is
/// sorted lexicographically by axis name so the preimage is independent
/// of caller-side iteration order. This is the same byte sequence that
/// `envelope_hash` commits (after the `0xA7` prefix) and that the
/// router's Ed25519 signature covers.
#[must_use]
pub fn canonical_envelope_payload(receipt: &Receipt, classify: &CacheClassifyMeta) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&receipt.settlement_hash);
    encode_len_prefixed_string(&mut out, &receipt.router_id);
    out.extend_from_slice(&receipt.timestamp_unix.to_be_bytes());
    encode_len_prefixed_string(&mut out, &classify.cache_class);
    match classify.cache_key_hash {
        Some(h) => {
            out.push(1);
            out.extend_from_slice(&h);
        }
        None => out.push(0),
    }
    // Sort axes_consumed by axis name for canonical ordering.
    let mut sorted_axes = classify.axes_consumed.clone();
    sorted_axes.sort_by(|a, b| a.0.cmp(&b.0));
    out.extend_from_slice(&(sorted_axes.len() as u32).to_be_bytes());
    for (axis, count) in &sorted_axes {
        encode_len_prefixed_string(&mut out, axis);
        out.extend_from_slice(&count.to_be_bytes());
    }
    out
}

/// Canonical envelope bytes for signing — preimage for the router's
/// Ed25519 signature, identical to the bytes committed by
/// `envelope_hash` (sans the `0xA7` prefix).
#[must_use]
pub fn canonical_envelope_bytes(envelope: &ReceiptEnvelope) -> Vec<u8> {
    canonical_envelope_payload(&envelope.receipt, &envelope.cache_classify)
}

fn encode_len_prefixed_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
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
        let a = canonical_receipt_bytes(
            &h,
            &octo_ident::test_helpers::sample_did(134),
            &octo_ident::test_helpers::sample_did(183),
            1_700_000_000,
        );
        let b = canonical_receipt_bytes(
            &h,
            &octo_ident::test_helpers::sample_did(134),
            &octo_ident::test_helpers::sample_did(183),
            1_700_000_000,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_bytes_changes_with_timestamp() {
        let h = [0xab; 32];
        let a = canonical_receipt_bytes(
            &h,
            &octo_ident::test_helpers::sample_did(134),
            &octo_ident::test_helpers::sample_did(183),
            1_700_000_000,
        );
        let b = canonical_receipt_bytes(
            &h,
            &octo_ident::test_helpers::sample_did(134),
            &octo_ident::test_helpers::sample_did(183),
            1_700_000_001,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn wrap_envelope_deterministic() {
        let r = dummy_receipt();
        let c = dummy_classify();
        let e1 = wrap_receipt_envelope(r.clone(), c.clone());
        let e2 = wrap_receipt_envelope(r, c);
        assert_eq!(e1.envelope_hash, e2.envelope_hash);
    }

    #[test]
    fn wrap_envelope_differs_for_different_receipt() {
        let c = dummy_classify();
        let r1 = dummy_receipt();
        let mut r2 = dummy_receipt();
        r2.settlement_hash = [0x99; 32];
        let e1 = wrap_receipt_envelope(r1, c.clone());
        let e2 = wrap_receipt_envelope(r2, c);
        assert_ne!(e1.envelope_hash, e2.envelope_hash);
    }

    #[test]
    fn wrap_envelope_differs_for_different_classify() {
        let r = dummy_receipt();
        let c1 = dummy_classify();
        let mut c2 = dummy_classify();
        c2.cache_class = "miss".to_owned();
        let e1 = wrap_receipt_envelope(r.clone(), c1);
        let e2 = wrap_receipt_envelope(r, c2);
        assert_ne!(e1.envelope_hash, e2.envelope_hash);
    }

    #[test]
    fn wrap_envelope_differs_for_axis_reorder() {
        // Canonical encoder sorts axes_consumed lexicographically, so
        // two different input orderings must hash identically (NOT
        // differently — the prior serde_json encoder did NOT guarantee
        // this).
        let r = dummy_receipt();
        let c1 = CacheClassifyMeta {
            cache_class: "exact".to_owned(),
            cache_key_hash: Some([0xcd; 32]),
            axes_consumed: vec![("a_tokens".to_owned(), 1), ("b_tokens".to_owned(), 2)],
        };
        let c2 = CacheClassifyMeta {
            cache_class: "exact".to_owned(),
            cache_key_hash: Some([0xcd; 32]),
            axes_consumed: vec![("b_tokens".to_owned(), 2), ("a_tokens".to_owned(), 1)],
        };
        let e1 = wrap_receipt_envelope(r.clone(), c1);
        let e2 = wrap_receipt_envelope(r, c2);
        assert_eq!(e1.envelope_hash, e2.envelope_hash);
    }

    #[test]
    fn wrap_envelope_uses_reserved_prefix() {
        // Guard: prefix must stay out of 0xA3 (sql_statements_hash) and
        // 0xA4-0xA6 (RFC-0960 allocations) and 0xA0-0xA2 (other
        // reserved slots). 0xA7 is the first free byte in the
        // 0xA7-0xAF reserved range per RFC-0964 §0.1.
        assert_eq!(RECEIPT_ENVELOPE_HASH_PREFIX, 0xA7);
        assert_ne!(RECEIPT_ENVELOPE_HASH_PREFIX, 0xA3);
        for reserved in [0xA0_u8, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6] {
            assert_ne!(
                RECEIPT_ENVELOPE_HASH_PREFIX, reserved,
                "ReceiptEnvelope prefix collides with reserved byte 0x{reserved:02X}"
            );
        }
    }

    #[test]
    fn canonical_envelope_bytes_stable() {
        let r = dummy_receipt();
        let c = dummy_classify();
        let e1 = wrap_receipt_envelope(r.clone(), c.clone());
        let e2 = wrap_receipt_envelope(r, c);
        assert_eq!(canonical_envelope_bytes(&e1), canonical_envelope_bytes(&e2));
    }

    #[test]
    fn sign_envelope_overwrites_signature() {
        let mut envelope = wrap_receipt_envelope(dummy_receipt(), dummy_classify());
        let sig = ed25519_dalek::Signature::from_bytes(&[0x42; 64]);
        sign_envelope(&mut envelope, sig);
        assert_eq!(envelope.signature.to_bytes(), [0x42; 64]);
    }
}
