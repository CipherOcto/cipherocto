//! DID encoding (per RFC-0862 v1.3 §Substrate types §canonical_hash +
//! §EncodedDidDocument).
//!
//! `canonical_hash` is the canonical BLAKE3-256 hash of a `DidDocument`
//! in its borsh-encoded form. The actual hash is computed in
//! `octo-ident` (where `DidDocument` lives); this module re-exports
//! the free fn so callers can write `octo_sync::substrate::did::canonical_hash`
//! without a direct `octo_ident` dependency.
//!
//! `EncodedDidDocument` is a small trait for callers that need to
//! produce the canonical bytes (e.g., for inclusion in a WAL entry
//! payload). The trait is NOT sealed (per RFC-0862 v1.3 §Substrate
//! types: "encoded via free fn + trait, NOT sealed").

use octo_ident::canonical_hash as ident_canonical_hash;
use octo_ident::DidDocument;

/// Re-exported from `octo_ident::canonical_hash`. Computes the
/// BLAKE3-256 hash of `borsh::to_vec(doc)`.
///
/// Per RFC-0862 v1.3 §Substrate types: free fn, NOT a trait method.
/// This avoids forcing every `DidDocument` extension to re-implement
/// the canonical encoding.
pub fn canonical_hash(doc: &DidDocument) -> [u8; 32] {
    ident_canonical_hash(doc)
}

/// Per-RFC-0862 v1.3 §Substrate types §EncodedDidDocument.
///
/// Trait for types that can be encoded into the canonical borsh form
/// used by `canonical_hash`. Default impl applies to `DidDocument`.
///
/// `Send + Sync` so the trait can be used in `Arc<dyn EncodedDidDocument>`
/// (per RFC-0862 v1.3 R12 H12 supertrait bound).
pub trait EncodedDidDocument: Send + Sync {
    /// Encode the document into the canonical borsh form.
    fn encode(&self) -> Vec<u8>;
}

impl EncodedDidDocument for DidDocument {
    fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("DidDocument borsh serialization is infallible")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;

    #[test]
    fn canonical_hash_matches_octo_ident() {
        let doc = DidDocument {
            public_key: [1u8; 32],
            revoked: false,
            ..Default::default()
        };
        let h1 = canonical_hash(&doc);
        let h2 = ident_canonical_hash(&doc);
        assert_eq!(h1, h2);
    }

    #[test]
    fn encoded_did_document_round_trips() {
        let doc = DidDocument {
            public_key: [1u8; 32],
            revoked: false,
            ..Default::default()
        };
        let encoded = doc.encode();
        let decoded = DidDocument::try_from_slice(&encoded).unwrap();
        assert_eq!(decoded.public_key, doc.public_key);
        assert_eq!(decoded.revoked, doc.revoked);
    }
}
