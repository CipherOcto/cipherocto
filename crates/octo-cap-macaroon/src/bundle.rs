//! `CapabilityBundle` — portable archival + replay representation
//! (RFC-0957 §Future Work F4).
//!
//! Aggregates `CapabilityToken` + `HolderRecord` + `Vec<DischargeMacaroon>`
//! for offline storage + cross-process replay.
//!
//! ## Layer discipline
//!
//! `CapabilityBundle` is owned by `octo-cap-macaroon` (Layer 4 extension
//! per RFC-0965). It CANNOT depend on `quota-router-storage` (Layer B
//! substrate) per the per-extension crate layer model. So:
//!
//! - `CapabilityToken` + `DischargeMacaroon` are concrete types (live
//!   in this crate per Phase 2b migration).
//! - `HolderRecord` is held as serialized bytes
//!   (`Vec<u8>` from `HolderRecord::canonical_ser`) — deserialized by the
//!   caller (typically `quota_router_storage::HolderRecord::canonical_de`).
//!   This keeps the bundle transport-agnostic and layer-clean.
//!
//! ## Bundle versioning
//!
//! `bundle_version: u8 = 1`. Forward-compatible: future versions add
//! new fields at the tail; old consumers ignore unknown fields
//! (serde default). Mismatched versions are rejected at
//! [`CapabilityBundle::canonical_de`] via [`BundleError::UnsupportedVersion`].
//!
//! ## Determinism contract
//!
//! `canonical_ser` uses `serde_json::to_vec`, which emits JSON keys in
//! struct field declaration order. Bytes are stable iff the struct
//! source is stable. This is **not** RFC-8785 sorted-key canonical JSON
//! (see `HolderRecord::canonical_ser` doc for the same rationale).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::token::{CapabilityToken, DischargeMacaroon};

/// Current `CapabilityBundle` wire version.
pub const BUNDLE_VERSION: u8 = 1;

/// Domain separator for `bundle_id` derivation (BLAKE3 input).
///
/// `bundle_id = BLAKE3(BUNDLE_ID_DOMAIN || canonical_ser(bundle))`.
/// The string literal is part of the wire contract — bumping the
/// version (`BUNDLE_VERSION`) or changing the domain are breaking
/// changes for downstream verifiers.
pub const BUNDLE_ID_DOMAIN: &str = "cipherocto/bundle/v1/id";

/// Portable archival representation of a `CapabilityToken` + `HolderRecord`
/// + `Vec<DischargeMacaroon>` triplet.
///
/// `holder_record_bytes` is the serialized form of the `HolderRecord`;
/// consumers deserialize via
/// `quota_router_storage::holder_record::HolderRecord::canonical_de`.
/// This indirection keeps `octo-cap-macaroon` free of `quota-router-storage`
/// deps (Layer 4 → Layer B-substrate forbidden per
/// [[cipherocto-design-principles]]).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityBundle {
    /// Wire format version discriminator (currently `BUNDLE_VERSION` = 1).
    /// First field per forward-compat convention. MUST equal
    /// `BUNDLE_VERSION` on the wire; `canonical_de` rejects mismatches.
    pub bundle_version: u8,

    /// Holder-bound capability token envelope (RFC-0957 §3.1).
    pub token: CapabilityToken,

    /// Serialized bytes of the `HolderRecord`. Deserialize via
    /// `HolderRecord::canonical_de` at the boundary.
    pub holder_record_bytes: Vec<u8>,

    /// Channel-specific discharge macaroons (RFC-0957 §3.4).
    pub discharges: Vec<DischargeMacaroon>,
}

/// Error returned by [`CapabilityBundle::canonical_de`] for version-mismatched
/// payloads or malformed JSON.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    /// `bundle_version` field does not match `BUNDLE_VERSION`.
    #[error(
        "unsupported bundle_version {found} (this build supports {expected}); \
         upgrade or downgrade the bundle crate"
    )]
    UnsupportedVersion {
        /// The `bundle_version` byte found in the payload.
        found: u8,
        /// The `BUNDLE_VERSION` constant compiled into this crate.
        expected: u8,
    },
    /// Underlying JSON deserialization failure (malformed / truncated /
    /// schema drift).
    #[error("bundle deserialize error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl CapabilityBundle {
    /// Construct a new bundle from concrete parts. `holder_record_bytes`
    /// MUST be the serialized form of a `HolderRecord` (the caller is
    /// responsible for producing it via `HolderRecord::canonical_ser`).
    #[must_use]
    pub fn new(
        token: CapabilityToken,
        holder_record_bytes: Vec<u8>,
        discharges: Vec<DischargeMacaroon>,
    ) -> Self {
        Self {
            bundle_version: BUNDLE_VERSION,
            token,
            holder_record_bytes,
            discharges,
        }
    }

    /// Serialize the bundle to deterministic JSON bytes.
    ///
    /// **Not** RFC-8785 sorted-key canonical JSON — see module docs.
    /// Byte order is stable iff struct field declaration order is stable.
    ///
    /// # Errors
    /// Returns `serde_json::Error` if any field fails to serialize
    /// (should not happen for the current schema).
    pub fn canonical_ser(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Inverse of [`Self::canonical_ser`]. Rejects mismatched
    /// `bundle_version` with [`BundleError::UnsupportedVersion`].
    ///
    /// # Errors
    /// Returns [`BundleError::UnsupportedVersion`] if the embedded
    /// `bundle_version` does not match [`BUNDLE_VERSION`], or
    /// [`BundleError::Serde`] if the bytes are malformed / schema drift.
    pub fn canonical_de(bytes: &[u8]) -> Result<Self, BundleError> {
        let bundle: Self = serde_json::from_slice(bytes)?;
        if bundle.bundle_version != BUNDLE_VERSION {
            return Err(BundleError::UnsupportedVersion {
                found: bundle.bundle_version,
                expected: BUNDLE_VERSION,
            });
        }
        Ok(bundle)
    }

    /// Derive a content-addressable `bundle_id` (BLAKE3, 32 bytes).
    ///
    /// `bundle_id = BLAKE3(BUNDLE_ID_DOMAIN || canonical_ser(self))`.
    /// Used as the deterministic identifier in audit logs, revocation
    /// lists, and gossip fan-out indexes.
    #[must_use]
    pub fn bundle_id(&self) -> [u8; 32] {
        let ser = self
            .canonical_ser()
            .expect("CapabilityBundle serialization is infallible for the current schema");
        let mut hasher = blake3::Hasher::new();
        hasher.update(BUNDLE_ID_DOMAIN.as_bytes());
        hasher.update(&ser);
        *hasher.finalize().as_bytes()
    }
}

/// Manual redacting `Debug` impl: bearer-secret fields (`holder_record_bytes`)
/// are redacted; `discharges` `root_secret_hash` fields are redacted in
/// the nested `DischargeMacaroon` Debug impl.
impl fmt::Debug for CapabilityBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapabilityBundle")
            .field("bundle_version", &self.bundle_version)
            .field("token", &self.token)
            .field(
                "holder_record_bytes",
                &format_args!("<redacted {} bytes>", self.holder_record_bytes.len()),
            )
            .field("discharges", &self.discharges)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caveat::Caveat;
    use crate::signer::CapabilitySigner;

    /// Test signer: thin Ed25519 keypair wrapper implementing
    /// `CapabilitySigner` for fixture construction.
    struct TestSigner {
        key: [u8; 32],
        pub_bytes: [u8; 32],
    }

    impl CapabilitySigner for TestSigner {
        fn sign(&self, msg: &[u8]) -> Result<[u8; 64], crate::signer::CapabilitySignerError> {
            use ed25519_dalek::Signer;
            let sk = ed25519_dalek::SigningKey::from_bytes(&self.key);
            Ok(sk.sign(msg).to_bytes())
        }
        fn public_key_bytes(&self) -> [u8; 32] {
            self.pub_bytes
        }
    }

    /// Build a minimal `CapabilityBundle` for testing.
    fn fixture() -> CapabilityBundle {
        let root_secret = [0x42; 32];
        let holder = TestSigner {
            key: [0x42; 32],
            pub_bytes: {
                use ed25519_dalek::SigningKey;
                SigningKey::from_bytes(&[0x42; 32])
                    .verifying_key()
                    .to_bytes()
            },
        };
        let audience = "did:octo:zTestHolder".to_owned();
        let caveats = vec![Caveat::Model("gpt-4".to_owned())];
        let token =
            CapabilityToken::mint(&root_secret, &holder, &audience, &caveats).expect("mint");
        let holder_record_bytes = br#"{"cap_root_hash":"0000000000000000000000000000000000000000000000000000000000000000","kind":0,"holder_did":"did:octo:zTest","holder_pub":"0000000000000000000000000000000000000000000000000000000000000099","audience_did":"did:octo:zTest","caveats_canonical":"","ask_id":null,"mint_at_millis_unix":1700000000000,"ttl_millis_unix":1700003600000,"revoked_at_millis_unix":null}"#.to_vec();
        CapabilityBundle::new(token, holder_record_bytes, vec![])
    }

    #[test]
    fn bundle_version_is_1() {
        assert_eq!(BUNDLE_VERSION, 1);
        let bundle = fixture();
        assert_eq!(bundle.bundle_version, 1);
    }

    #[test]
    fn bundle_roundtrip_preserves_all_fields() {
        let bundle = fixture();
        let bytes = bundle.canonical_ser().expect("ser");
        let decoded = CapabilityBundle::canonical_de(&bytes).expect("de");
        assert_eq!(bundle, decoded, "roundtrip must preserve bytes");
    }

    #[test]
    fn debug_redacts_bearer_secrets() {
        let bundle = fixture();
        let s = format!("{:?}", bundle);
        assert!(
            s.contains("<redacted"),
            "Debug must redact holder_record_bytes, got {s}"
        );
        assert!(
            !s.contains("00000000000000000000000000000000"),
            "Debug must NOT leak raw holder_record content"
        );
    }

    #[test]
    fn bundle_canonical_de_rejects_malformed_bytes() {
        let garbage = b"not a bundle {{{";
        let result = CapabilityBundle::canonical_de(garbage);
        assert!(result.is_err(), "garbage must fail to deserialize");
    }

    #[test]
    fn bundle_canonical_de_rejects_unsupported_version() {
        // Round-trip a real fixture, mutate the bundle_version byte, and
        // verify the deserializer rejects it. This avoids crafting a hand-
        // rolled JSON payload (the token schema is non-trivial).
        let bundle = fixture();
        let mut bytes = bundle.canonical_ser().expect("ser");
        // First field is bundle_version (u8 in JSON = digit 0/1/...).
        // Find the first `:` and the digit that follows, replace it.
        let colon = bytes.iter().position(|b| *b == b':').expect("colon");
        let digit_idx = colon + 1;
        let original = bytes[digit_idx];
        assert!(
            original == b'1' || original == b'0',
            "expected bundle_version digit at byte {digit_idx}, got {original}"
        );
        // Replace with '9' (unknown future version).
        bytes[digit_idx] = b'9';
        let result = CapabilityBundle::canonical_de(&bytes);
        assert!(
            matches!(
                result,
                Err(BundleError::UnsupportedVersion {
                    found: 9,
                    expected: 1
                })
            ),
            "unknown version must return UnsupportedVersion, got {result:?}"
        );
    }

    #[test]
    fn bundle_canonical_de_rejects_v0_version() {
        // v0 reserved for the pre-versioned prototype; reject on the wire.
        let bundle = fixture();
        let mut bytes = bundle.canonical_ser().expect("ser");
        let colon = bytes.iter().position(|b| *b == b':').expect("colon");
        let digit_idx = colon + 1;
        let original = bytes[digit_idx];
        assert_eq!(original, b'1', "fixture must start at v1");
        bytes[digit_idx] = b'0';
        let result = CapabilityBundle::canonical_de(&bytes);
        assert!(
            matches!(
                result,
                Err(BundleError::UnsupportedVersion { found: 0, .. })
            ),
            "v0 must be rejected, got {result:?}"
        );
    }

    #[test]
    fn bundle_id_domain_is_canonical_string() {
        assert_eq!(BUNDLE_ID_DOMAIN, "cipherocto/bundle/v1/id");
    }

    #[test]
    fn bundle_id_is_32_bytes_and_changes_with_content() {
        let a = fixture();
        assert_eq!(a.bundle_id().len(), 32);
        // Same fixture → same id (deterministic).
        assert_eq!(
            a.bundle_id(),
            a.bundle_id(),
            "bundle_id must be deterministic"
        );
        // Mutate holder_record_bytes → id changes.
        let mut c = fixture();
        c.holder_record_bytes = b"different".to_vec();
        assert_ne!(
            a.bundle_id(),
            c.bundle_id(),
            "bundle_id must change when content changes"
        );
    }
}
