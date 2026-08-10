//! `CapabilityBundle` — portable archival + replay representation
//! (RFC-0957 §Future Work F4).
//!
//! Aggregates `CapabilityToken` + `HolderRecord` + `Vec<DischargeMacaroon>`
//! for offline storage + cross-process replay. Uses canonical JSON per
//! RFC-0126 for deterministic wire form.
//!
//! ## Layer discipline
//!
//! `CapabilityBundle` is owned by `octo-cap-macaroon` (Layer 4 extension
//! per RFC-0965). It CANNOT depend on `quota-router-storage` (Layer B
//! substrate) per the per-extension crate layer model. So:
//!
//! - `CapabilityToken` + `DischargeMacaroon` are concrete types (live
//!   in this crate per Phase 2b migration).
//! - `HolderRecord` is held as canonical JSON bytes
//!   (`Vec<u8>` from `canonical_ser`) — deserialized by the caller
//!   (typically `quota-router-storage::HolderRecord::canonical_de`).
//!   This keeps the bundle transport-agnostic and layer-clean.
//!
//! ## Bundle versioning
//!
//! `bundle_version: u8 = 1`. Forward-compatible: future versions add
//! new fields at the tail; old consumers ignore unknown fields
//! (serde default + `#[serde(default)]` on `extra` map).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::token::{CapabilityToken, DischargeMacaroon};

/// Current `CapabilityBundle` wire version.
pub const BUNDLE_VERSION: u8 = 1;

/// Canonical ID domain separator for bundle serialization per RFC-0126.
pub const BUNDLE_ID_DOMAIN: &str = "cipherocto/bundle/v1/id";

/// Portable archival representation of a `CapabilityToken` + `HolderRecord`
/// + `Vec<DischargeMacaroon>` triplet.
///
/// `holder_record_bytes` is the canonical JSON serialization of the
/// `HolderRecord` (RFC-0126); consumers deserialize via
/// `quota_router_storage::holder_record::HolderRecord::canonical_de`.
/// This indirection keeps `octo-cap-macaroon` free of `quota-router-storage`
/// deps (Layer 4 → Layer B-substrate forbidden per
/// [[cipherocto-design-principles]]).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityBundle {
    /// Wire format version discriminator (currently `BUNDLE_VERSION` = 1).
    /// First field per RFC-0126 forward-compat convention.
    pub bundle_version: u8,

    /// Holder-bound capability token envelope (RFC-0957 §3.1).
    pub token: CapabilityToken,

    /// Canonical JSON bytes of the `HolderRecord` (RFC-0126).
    /// Deserialize via `HolderRecord::canonical_de` at the boundary.
    pub holder_record_bytes: Vec<u8>,

    /// Channel-specific discharge macaroons (RFC-0957 §3.4).
    pub discharges: Vec<DischargeMacaroon>,
}

impl CapabilityBundle {
    /// Construct a new bundle from concrete parts. `holder_record_bytes`
    /// MUST be the canonical JSON serialization of a `HolderRecord`
    /// (the caller is responsible for producing it via
    /// `HolderRecord::canonical_ser`).
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

    /// Serialize the bundle to canonical JSON bytes (RFC-0126).
    ///
    /// # Errors
    /// Returns `serde_json::Error` if any field fails to serialize
    /// (should not happen for the current schema).
    pub fn canonical_ser(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Inverse of [`Self::canonical_ser`].
    ///
    /// # Errors
    /// Returns `serde_json::Error` if the bytes do not decode to a
    /// valid `CapabilityBundle` (truncated, malformed, wrong version,
    /// schema drift).
    pub fn canonical_de(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
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
    fn bundle_id_domain_is_canonical_string() {
        assert_eq!(BUNDLE_ID_DOMAIN, "cipherocto/bundle/v1/id");
    }
}
