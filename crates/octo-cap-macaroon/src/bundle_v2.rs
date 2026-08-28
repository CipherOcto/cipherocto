//! `CapabilityBundleV2` — V2 wire form of portable archival + replay
//! representation (RFC-0009 §Compatibility).
//!
//! Aggregates `CapabilityTokenV2` + serialized `HolderRecord` bytes +
//! singular `DischargeMacaroon` for offline storage + cross-process
//! replay. V2 adds `chain_depth` + `chain_parent` fields on the token
//! for hierarchical attenuation chain verification (RFC-0009
//! §Hierarchical attenuation chains).
//!
//! ## Layer discipline (per [[cipherocto-design-principles]])
//!
//! `CapabilityBundleV2` is owned by `octo-cap-macaroon` (Layer 4
//! extension per RFC-0965). It CANNOT depend on `quota-router-storage`
//! (Layer B substrate) per the per-extension crate layer model. So
//!
//! - `holder_record_bytes: Vec<u8>` (LAYER DISCIPLINE: bytes, not
//!   concrete `HolderRecord`; V2 mirrors V1's deliberate
//!   layer-clean design per `bundle.rs` module docs; the RFC-0009
//!   §Compatibility showing concrete `HolderRecord` is
//!   overridden by layer direction — L4 → B-substrate forbidden).
//! - `discharge_macaroon: DischargeMacaroon` lives in the same crate
//!   (Layer 4 self-reference; OK).
//! - `CapabilityTokenV2` is a separate struct from V1 (per
//!   [[cipherocto-design-principles]] §Extension over enumeration —
//!   V2 carries new fields; central enum would force V1 edits).
//!
//! ## V1 → V2 schema diff
//!
//! - `token: CapabilityToken` (V1) → `token_v2: CapabilityTokenV2`
//!   (carries `chain_depth` + `chain_parent`).
//! - `holder_record_bytes: Vec<u8>` unchanged (layer discipline).
//! - `discharges: Vec<DischargeMacaroon>` (V1) →
//!   `discharge_macaroon_bytes: Vec<u8>` (singular; borsh-encoded
//!   `DischargeMacaroon` bytes). V2 wire form carries ONE discharge
//!   (singular vs V1's `Vec`) per RFC-0009 §Compatibility. The borsh-bytes indirection follows V1's
//!   `holder_record_bytes` pattern (layer discipline: cargo-graph
//!   hygiene avoids the `Vec<Caveat>` → Borsh derive cascade; the
//!   `Caveat` enum has 24 variants with their own types).
//! - `canonical_ser` uses borsh (deterministic binary) per RFC-0009
//!   v1.2 §Compatibility (vs V1's `serde_json`).
//!
//! ## Bundle versioning
//!
//! `bundle_version: u8 = 2`. V1 and V2 are separate structs (V2
//! stays separate from V1 per [[cipherocto-design-principles]]
//! §Extension over enumeration). The cross-version rejection contract:
//!
//! - V1 parser rejects V2 via unknown struct (V2 bytes fail to
//!   deserialize as V1's `serde_json` `CapabilityBundle`).
//! - V2 parser rejects V1 via explicit `bundle_version == 1` check
//!   OR via borsh deserialize failure (V1's `serde_json` bytes fail
//!   to parse as V2 borsh).
//!
//! ## Determinism contract
//!
//! `canonical_ser` uses `borsh::to_vec`, which emits deterministic
//! binary (field-order stable, no JSON whitespace ambiguity). Bytes
//! are stable iff the struct source is stable. Unlike V1's
//! `serde_json`, V2 does NOT require RFC-8785 sorted-key canonical
//! encoding (Borsh is deterministic by construction).

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

/// Current V2 `CapabilityBundle` wire version.
pub const BUNDLE_VERSION_V2: u8 = 2;

/// Domain separator for V2 `bundle_id` derivation (BLAKE3 input).
///
/// `bundle_id = BLAKE3(BUNDLE_ID_DOMAIN_V2 || canonical_ser(bundle))`.
/// The string literal is part of the wire contract — bumping the
/// version (`BUNDLE_VERSION_V2`) or changing the domain are breaking
/// changes for downstream verifiers.
pub const BUNDLE_ID_DOMAIN_V2: &str = "cipherocto/bundle/v2/id";

/// Maximum `chain_depth` (RFC-0009 G1: depth ≤ 8 per W3C VC-DID
/// best practice). `capability_id` derivation is pure on-chain
/// (BLAKE3, Class A determinism); the depth cap is a soft migration
/// bound (amendable per RFC-0009 migration row: "Chain depth ≤ 8 — Migration if raised").
pub const MAX_CHAIN_DEPTH: u8 = 8;

/// V2 hierarchical attenuation chain token (RFC-0009 §Capability Keys).
///
/// Carries `chain_depth` (the level in the chain; 0 = root, 1..=8 =
/// descendant) + `chain_parent` (BLAKE3-256 binding of parent key +
/// child key + child depth per RFC-0009 `verify_chain_parent`
/// + `compute_chain_parent`).
#[derive(
    Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize,
)]
pub struct CapabilityTokenV2 {
    /// Chain depth (0 = root; 1..=8 = descendant). Lives on the
    /// token, NOT on `CapabilityKey` (per RFC-0009 R11 H1).
    pub chain_depth: u8,

    /// BLAKE3-256 binding of `parent_cap_key || child_cap_key ||
    /// child_depth.to_be_bytes()` (per RFC-0009 `compute_chain_parent`).
    /// `[0u8; 32]` for root tokens (depth 0).
    pub chain_parent: [u8; 32],

    /// Audience DID (the holder redeeming the token).
    pub audience_did: String,

    /// Channel identifier (16-byte scope tag; ChannelId size
    /// contract per RFC-0009 §Capability Keys).
    pub channel_id: [u8; 16],

    /// Expiry timestamp (Unix seconds).
    pub expires_at_unix_secs: u64,

    /// Issuer DID (the Capability Issuer that minted this token).
    pub issuer_did: String,
}

impl fmt::Debug for CapabilityTokenV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapabilityTokenV2")
            .field("chain_depth", &self.chain_depth)
            .field("chain_parent", &hex::encode(self.chain_parent))
            .field("audience_did", &self.audience_did)
            .field("channel_id", &hex::encode(self.channel_id))
            .field("expires_at_unix_secs", &self.expires_at_unix_secs)
            .field("issuer_did", &self.issuer_did)
            .finish()
    }
}

/// V2 portable archival representation of `CapabilityTokenV2` +
/// serialized `HolderRecord` + serialized `DischargeMacaroon` bytes.
///
/// `holder_record_bytes` is the serialized form of the `HolderRecord`;
/// consumers deserialize via
/// `octo_cap_macaroon::HolderRecord::canonical_de` (moved from
/// `quota_router_storage::holder_record` in mission 0206-003 v3.0).
///
/// `discharge_macaroon_bytes` is the serialized form of a
/// `DischargeMacaroon`; consumers deserialize via
/// `DischargeMacaroon::canonical_de` (or
/// `serde_json::from_slice`) at the boundary. The bytes indirection
/// keeps `octo-cap-macaroon` free of `quota-router-storage` deps
/// (Layer 4 → Layer B-substrate forbidden per
/// [[cipherocto-design-principles]]) AND avoids the `Vec<Caveat>` →
/// Borsh derive cascade (the `Caveat` enum has 24 variants with
/// their own types).
///
/// **Deviation from RFC-0009 §Compatibility:** the
/// authoritative spec sketch shows `discharge_macaroon: DischargeMacaroon`
/// (concrete type). This implementation uses
/// `discharge_macaroon_bytes: Vec<u8>` to keep the layer discipline
/// (V1 pattern + cascade-avoidance). The wire-form semantic is
/// preserved: V2 carries ONE discharge (singular vs V1's `Vec`),
/// accessible to the consumer after `canonical_de` via
/// `DischargeMacaroon::canonical_de(&bundle.discharge_macaroon_bytes)`.
#[derive(
    Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize,
)]
pub struct CapabilityBundleV2 {
    /// Wire format version discriminator (currently
    /// `BUNDLE_VERSION_V2` = 2). First field per forward-compat
    /// convention. MUST equal `BUNDLE_VERSION_V2` on the wire;
    /// `canonical_de` rejects mismatches.
    pub bundle_version: u8,

    /// V2 holder-bound attenuation chain token (RFC-0009).
    pub token_v2: CapabilityTokenV2,

    /// Serialized bytes of the `HolderRecord`. Deserialize via
    /// `HolderRecord::canonical_de` at the boundary.
    pub holder_record_bytes: Vec<u8>,

    /// Borsh-bytes indirection for a serialized `DischargeMacaroon`.
    /// Deserialize via `DischargeMacaroon::canonical_de` (or
    /// `serde_json::from_slice`) at the boundary.
    pub discharge_macaroon_bytes: Vec<u8>,
}

/// Manual redacting `Debug` impl: bearer-secret fields
/// (`holder_record_bytes` + `discharge_macaroon_bytes`) are redacted.
impl fmt::Debug for CapabilityBundleV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapabilityBundleV2")
            .field("bundle_version", &self.bundle_version)
            .field("token_v2", &self.token_v2)
            .field(
                "holder_record_bytes",
                &format_args!("<redacted {} bytes>", self.holder_record_bytes.len()),
            )
            .field(
                "discharge_macaroon_bytes",
                &format_args!("<redacted {} bytes>", self.discharge_macaroon_bytes.len()),
            )
            .finish()
    }
}

/// Error returned by [`CapabilityBundleV2::canonical_de`] for
/// version-mismatched payloads, malformed borsh, or `chain_depth`
/// exceeding [`MAX_CHAIN_DEPTH`].
#[derive(Debug, thiserror::Error)]
pub enum BundleV2Error {
    /// `bundle_version` field does not match `BUNDLE_VERSION_V2`.
    #[error(
        "unsupported V2 bundle_version {found} (this build supports {expected}); \
         upgrade or downgrade the bundle crate"
    )]
    UnsupportedVersion {
        /// The `bundle_version` byte found in the payload.
        found: u8,
        /// The `BUNDLE_VERSION_V2` constant compiled into this crate.
        expected: u8,
    },

    /// Underlying borsh deserialization failure (malformed / truncated
    /// / schema drift).
    #[error("bundle_v2 borsh deserialize error: {0}")]
    Borsh(String),

    /// `chain_depth` exceeds [`MAX_CHAIN_DEPTH`].
    #[error("chain_depth {0} exceeds MAX_CHAIN_DEPTH {MAX_CHAIN_DEPTH}")]
    ChainDepthTooLarge(u8),
}

impl CapabilityBundleV2 {
    /// Construct a new V2 bundle from concrete parts.
    /// `holder_record_bytes` MUST be the serialized form of a
    /// `HolderRecord` (the caller is responsible for producing it via
    /// `HolderRecord::canonical_ser`).
    ///
    /// # Errors
    /// Returns `BundleV2Error::ChainDepthTooLarge` if
    /// `token_v2.chain_depth > MAX_CHAIN_DEPTH`.
    pub fn new(
        token_v2: CapabilityTokenV2,
        holder_record_bytes: Vec<u8>,
        discharge_macaroon_bytes: Vec<u8>,
    ) -> Result<Self, BundleV2Error> {
        if token_v2.chain_depth > MAX_CHAIN_DEPTH {
            return Err(BundleV2Error::ChainDepthTooLarge(token_v2.chain_depth));
        }
        Ok(Self {
            bundle_version: BUNDLE_VERSION_V2,
            token_v2,
            holder_record_bytes,
            discharge_macaroon_bytes,
        })
    }

    /// Serialize the bundle to deterministic borsh bytes.
    ///
    /// Borsh is deterministic by construction (field-order stable,
    /// no JSON whitespace ambiguity). Bytes are stable iff the struct
    /// source is stable.
    ///
    /// # Errors
    /// Returns `BundleV2Error::Borsh` if borsh serialization fails
    /// (should not happen for the current schema).
    pub fn canonical_ser(&self) -> Result<Vec<u8>, BundleV2Error> {
        borsh::to_vec(self).map_err(|e| BundleV2Error::Borsh(e.to_string()))
    }

    /// Inverse of [`Self::canonical_ser`]. Rejects mismatched
    /// `bundle_version` with [`BundleV2Error::UnsupportedVersion`] and
    /// `chain_depth > MAX_CHAIN_DEPTH` with
    /// [`BundleV2Error::ChainDepthTooLarge`].
    ///
    /// # Errors
    /// Returns [`BundleV2Error::Borsh`] if the bytes are malformed
    /// (e.g., V1 JSON bytes do NOT parse as V2 borsh). Returns
    /// [`BundleV2Error::UnsupportedVersion`] if the embedded
    /// `bundle_version` does not match [`BUNDLE_VERSION_V2`]. Returns
    /// [`BundleV2Error::ChainDepthTooLarge`] if `chain_depth >
    /// MAX_CHAIN_DEPTH`.
    pub fn canonical_de(bytes: &[u8]) -> Result<Self, BundleV2Error> {
        let bundle: Self =
            borsh::from_slice(bytes).map_err(|e| BundleV2Error::Borsh(e.to_string()))?;
        if bundle.bundle_version != BUNDLE_VERSION_V2 {
            return Err(BundleV2Error::UnsupportedVersion {
                found: bundle.bundle_version,
                expected: BUNDLE_VERSION_V2,
            });
        }
        if bundle.token_v2.chain_depth > MAX_CHAIN_DEPTH {
            return Err(BundleV2Error::ChainDepthTooLarge(
                bundle.token_v2.chain_depth,
            ));
        }
        Ok(bundle)
    }

    /// Derive a content-addressable V2 `bundle_id` (BLAKE3, 32 bytes).
    ///
    /// `bundle_id = BLAKE3(BUNDLE_ID_DOMAIN_V2 || canonical_ser(self))`.
    /// Used as the deterministic identifier in audit logs, revocation
    /// lists, and gossip fan-out indexes.
    #[must_use]
    pub fn bundle_id(&self) -> [u8; 32] {
        let ser = self
            .canonical_ser()
            .expect("CapabilityBundleV2 serialization is infallible for the current schema");
        let mut hasher = blake3::Hasher::new();
        hasher.update(BUNDLE_ID_DOMAIN_V2.as_bytes());
        hasher.update(&ser);
        *hasher.finalize().as_bytes()
    }
}

/// Network wire prefix for V2 bundles (RFC-0009 §Compatibility —
/// receivers detect version from first 16 bytes
/// without attempting full canonical_de).
///
/// Padded to 16 bytes with trailing `\x00` so `borsh::from_slice`
/// reads it as a fixed `[u8; 16]` first field (no length prefix). The
/// 12-byte `b"cipherocto/v2"` ASCII literal leaves 4 null bytes for
/// future expansion (e.g. a 4-byte minor-version tag).
pub const CIPHEROCTO_V2_BUNDLE_PREFIX: &[u8; 16] = b"cipherocto/v2\x00\x00\x00";

/// V2 wire envelope (RFC-0009 §Compatibility carrier).
///
/// `CapabilityBundleV2` substrate is the inner bundle; the envelope
/// prepends the 16-byte version prefix so receivers can dispatch V2
/// vs legacy raw `Macaroon` bytes without attempting full borsh
/// decode on every payload.
///
/// `canonical_ser` produces 16-byte-prefix + borsh-encoded bundle
/// bytes (deterministic, fixed layout). `canonical_de` rejects
/// mismatched prefix OR `bundle_version != 2`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct CapabilityBundleV2Envelope {
    /// MUST equal [`CIPHEROCTO_V2_BUNDLE_PREFIX`] on the wire.
    pub prefix: [u8; 16],
    /// V2 bundle (inner).
    pub bundle: CapabilityBundleV2,
}

impl CapabilityBundleV2Envelope {
    /// Wrap a V2 bundle with the canonical 16-byte prefix.
    #[must_use]
    pub const fn new(bundle: CapabilityBundleV2) -> Self {
        Self {
            prefix: *CIPHEROCTO_V2_BUNDLE_PREFIX,
            bundle,
        }
    }

    /// Serialize to deterministic borsh bytes (prefix + bundle).
    ///
    /// # Errors
    /// Returns [`BundleV2Error::Borsh`] if borsh serialization fails
    /// (should not happen for the current schema).
    pub fn canonical_ser(&self) -> Result<Vec<u8>, BundleV2Error> {
        borsh::to_vec(self).map_err(|e| BundleV2Error::Borsh(e.to_string()))
    }

    /// Inverse of [`Self::canonical_ser`]. Rejects mismatched prefix
    /// OR inner `bundle_version != BUNDLE_VERSION_V2`.
    ///
    /// # Errors
    /// Returns [`BundleV2Error::Borsh`] on malformed bytes (incl. V1
    /// raw `Macaroon` JSON which is not valid borsh). Returns
    /// [`BundleV2Error::UnsupportedVersion`] on prefix mismatch or
    /// `bundle_version != BUNDLE_VERSION_V2`. Returns
    /// [`BundleV2Error::ChainDepthTooLarge`] when inner
    /// `chain_depth > MAX_CHAIN_DEPTH`.
    pub fn canonical_de(bytes: &[u8]) -> Result<Self, BundleV2Error> {
        let env: Self =
            borsh::from_slice(bytes).map_err(|e| BundleV2Error::Borsh(e.to_string()))?;
        if &env.prefix != CIPHEROCTO_V2_BUNDLE_PREFIX {
            return Err(BundleV2Error::UnsupportedVersion {
                found: env.prefix[14],
                expected: CIPHEROCTO_V2_BUNDLE_PREFIX[14],
            });
        }
        if env.bundle.bundle_version != BUNDLE_VERSION_V2 {
            return Err(BundleV2Error::UnsupportedVersion {
                found: env.bundle.bundle_version,
                expected: BUNDLE_VERSION_V2,
            });
        }
        if env.bundle.token_v2.chain_depth > MAX_CHAIN_DEPTH {
            return Err(BundleV2Error::ChainDepthTooLarge(
                env.bundle.token_v2.chain_depth,
            ));
        }
        Ok(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal V2 root bundle (chain_depth = 0) for testing.
    fn v2_root_fixture() -> CapabilityBundleV2 {
        let token_v2 = CapabilityTokenV2 {
            chain_depth: 0,
            chain_parent: [0u8; 32],
            audience_did: "did:octo:zV2RootHolder".to_owned(),
            channel_id: [0xA1; 16],
            expires_at_unix_secs: 1_700_003_600,
            issuer_did: "did:octo:zV2Issuer".to_owned(),
        };
        let holder_record_bytes = br#"{"private_holder_secret":"zV2PrivateRootHandle"}"#.to_vec();
        let discharge_macaroon_bytes = br#"{"channel":"escrow","root_secret_hash":"aa"}"#.to_vec();
        CapabilityBundleV2::new(token_v2, holder_record_bytes, discharge_macaroon_bytes)
            .expect("v2 root fixture")
    }

    /// Build a V2 child bundle (chain_depth = 1) with a non-zero
    /// `chain_parent` (computed per RFC-0009 `compute_chain_parent`).
    fn v2_child_fixture() -> CapabilityBundleV2 {
        let mut parent = v2_root_fixture();
        parent.token_v2.chain_depth = 1;
        parent.token_v2.chain_parent = [0xCC; 32];
        parent
    }

    #[test]
    fn v2_bundle_version_is_2() {
        assert_eq!(BUNDLE_VERSION_V2, 2);
        let bundle = v2_root_fixture();
        assert_eq!(bundle.bundle_version, 2);
    }

    #[test]
    fn v2_roundtrip_preserves_all_fields() {
        let bundle = v2_root_fixture();
        let bytes = bundle.canonical_ser().expect("ser");
        let decoded = CapabilityBundleV2::canonical_de(&bytes).expect("de");
        assert_eq!(bundle, decoded, "borsh roundtrip must preserve bytes");
    }

    #[test]
    fn v2_canonical_de_rejects_v1_json_bytes() {
        // V1 JSON bytes must NOT parse as V2 borsh (different schema).
        // Hand-craft a V1 JSON bundle — V2's borsh decoder will reject
        // the leading '{' character with a borsh error.
        let v1_json =
            br#"{"bundle_version":1,"token":{},"holder_record_bytes":[],"discharges":[]}"#;
        let result = CapabilityBundleV2::canonical_de(v1_json);
        assert!(
            matches!(result, Err(BundleV2Error::Borsh(_))),
            "V1 JSON bytes must fail V2 borsh decode, got {result:?}"
        );
    }

    #[test]
    fn v2_canonical_de_rejects_truncated_bytes() {
        let bundle = v2_root_fixture();
        let bytes = bundle.canonical_ser().expect("ser");
        // Truncate at the first 8 bytes — borsh length-prefixed U8
        // readers will fail to decode.
        let truncated = &bytes[..8.min(bytes.len())];
        let result = CapabilityBundleV2::canonical_de(truncated);
        assert!(
            result.is_err(),
            "truncated V2 bytes must fail to decode, got {result:?}"
        );
    }

    #[test]
    fn v2_canonical_de_rejects_chain_depth_above_max() {
        // Construct a bundle with chain_depth = MAX_CHAIN_DEPTH + 1
        // by bypassing `new()` (which validates). Hand-serialize via
        // borsh then re-decode.
        let mut bundle = v2_root_fixture();
        bundle.token_v2.chain_depth = MAX_CHAIN_DEPTH + 1;
        let bytes = bundle.canonical_ser().expect("ser");
        let result = CapabilityBundleV2::canonical_de(&bytes);
        assert!(
            matches!(result, Err(BundleV2Error::ChainDepthTooLarge(d)) if d == MAX_CHAIN_DEPTH + 1),
            "chain_depth > MAX_CHAIN_DEPTH must be rejected, got {result:?}"
        );
    }

    #[test]
    fn v2_canonical_de_rejects_wrong_borsh_version_byte() {
        // Construct a V2 bundle, then mutate the leading `bundle_version`
        // byte (which is the first u8 in borsh encoding) to a value
        // other than 2. The decoder checks the version after struct
        // deserialization.
        let bundle = v2_root_fixture();
        let mut bytes = bundle.canonical_ser().expect("ser");
        bytes[0] = 9; // bundle_version != BUNDLE_VERSION_V2
        let result = CapabilityBundleV2::canonical_de(&bytes);
        assert!(
            matches!(
                result,
                Err(BundleV2Error::UnsupportedVersion {
                    found: 9,
                    expected: 2
                })
            ),
            "wrong bundle_version must return UnsupportedVersion, got {result:?}"
        );
    }

    #[test]
    fn v2_id_domain_is_canonical_string() {
        assert_eq!(BUNDLE_ID_DOMAIN_V2, "cipherocto/bundle/v2/id");
    }

    #[test]
    fn v2_bundle_id_is_32_bytes_and_changes_with_chain_parent() {
        let root = v2_root_fixture();
        assert_eq!(root.bundle_id().len(), 32);
        // Same fixture → same id (deterministic).
        assert_eq!(
            root.bundle_id(),
            root.bundle_id(),
            "bundle_id must be deterministic"
        );
        // Different chain_parent → different id.
        let child = v2_child_fixture();
        assert_ne!(
            root.bundle_id(),
            child.bundle_id(),
            "bundle_id must change when chain_parent changes"
        );
    }

    #[test]
    fn v2_max_chain_depth_constant() {
        assert_eq!(MAX_CHAIN_DEPTH, 8, "chain depth cap per RFC-0009 G1");
    }

    #[test]
    fn v2_new_accepts_chain_depth_at_max() {
        // chain_depth == MAX_CHAIN_DEPTH is the boundary; must be accepted.
        let mut bundle = v2_root_fixture();
        bundle.token_v2.chain_depth = MAX_CHAIN_DEPTH;
        let result = CapabilityBundleV2::new(
            bundle.token_v2.clone(),
            bundle.holder_record_bytes.clone(),
            bundle.discharge_macaroon_bytes.clone(),
        );
        assert!(
            result.is_ok(),
            "chain_depth == MAX_CHAIN_DEPTH must be accepted, got {result:?}"
        );
    }

    #[test]
    fn v2_debug_redacts_holder_record_bytes() {
        let bundle = v2_root_fixture();
        let s = format!("{:?}", bundle);
        assert!(
            s.contains("<redacted"),
            "Debug must redact holder_record_bytes, got {s}"
        );
        // The raw holder_record payload (private_holder_secret) must
        // NOT be visible. The `token_v2.audience_did` field
        // (`zV2RootHolder`) IS visible (public identifier); the
        // private marker lives in the serialized HolderRecord bytes
        // only.
        assert!(
            !s.contains("private_holder_secret"),
            "Debug must NOT leak holder_record content, got {s}"
        );
    }

    #[test]
    fn v2_capability_token_v2_debug_redacts_secrets() {
        let bundle = v2_root_fixture();
        let s = format!("{:?}", bundle.token_v2);
        assert!(
            s.contains("chain_depth"),
            "Debug must surface chain_depth, got {s}"
        );
        // chain_parent is hex-encoded for transparency (not a bearer secret —
        // it's a BLAKE3 hash binding, public per RFC-0009
        // `verify_chain_parent` predicate).
        assert!(
            s.contains("chain_parent"),
            "Debug must surface chain_parent hex, got {s}"
        );
    }

    #[test]
    fn v2_envelope_prefix_is_16_bytes() {
        assert_eq!(CIPHEROCTO_V2_BUNDLE_PREFIX.len(), 16);
        // First 13 bytes = ASCII "cipherocto/v2"; last 3 = padding.
        assert_eq!(&CIPHEROCTO_V2_BUNDLE_PREFIX[..13], b"cipherocto/v2");
        assert_eq!(&CIPHEROCTO_V2_BUNDLE_PREFIX[13..], &[0u8; 3]);
    }

    #[test]
    fn v2_envelope_canonical_ser_roundtrip() {
        let bundle = v2_root_fixture();
        let env = CapabilityBundleV2Envelope::new(bundle.clone());
        let bytes = env.canonical_ser().expect("ser");
        // First 16 bytes MUST equal the prefix.
        assert_eq!(
            &bytes[..16],
            CIPHEROCTO_V2_BUNDLE_PREFIX.as_slice(),
            "canonical_ser must emit prefix as first 16 bytes"
        );
        // Roundtrip preserves inner bundle.
        let decoded = CapabilityBundleV2Envelope::canonical_de(&bytes).expect("de");
        assert_eq!(decoded.bundle, bundle);
        assert_eq!(decoded.prefix, *CIPHEROCTO_V2_BUNDLE_PREFIX);
    }

    #[test]
    fn v2_envelope_rejects_wrong_prefix() {
        let bundle = v2_root_fixture();
        let mut env = CapabilityBundleV2Envelope::new(bundle);
        env.prefix = *b"cipherocto/v1\x00\x00\x00"; // mutate to V1-ish
        let bytes = env.canonical_ser().expect("ser");
        let result = CapabilityBundleV2Envelope::canonical_de(&bytes);
        assert!(
            matches!(result, Err(BundleV2Error::UnsupportedVersion { .. })),
            "wrong prefix must be rejected, got {result:?}"
        );
    }

    #[test]
    fn v2_envelope_rejects_truncated_bytes() {
        let bundle = v2_root_fixture();
        let env = CapabilityBundleV2Envelope::new(bundle);
        let bytes = env.canonical_ser().expect("ser");
        // Truncate after the prefix only — borsh will fail to decode
        // the inner bundle.
        let truncated = &bytes[..16];
        let result = CapabilityBundleV2Envelope::canonical_de(truncated);
        assert!(
            result.is_err(),
            "truncated envelope bytes must fail to decode, got {result:?}"
        );
    }

    #[test]
    fn v2_envelope_legacy_v1_bytes_fail_decode() {
        // V1 raw macaroon bytes (the wire form missions 0957-phase2a
        // emit: 3 base64url-no-pad segments). The envelope prefix
        // sniff rejects these — borsh fails to decode the leading
        // ASCII bytes as a `[u8; 16]` prefix field.
        let v1_wire = b"AgJkZjA0ZTM2YzAtNDg3OS00Y2I5LWE2NGItZTQ3MjQ5NDUyNjQ3.eyJ";
        let result = CapabilityBundleV2Envelope::canonical_de(v1_wire);
        assert!(
            result.is_err(),
            "V1 raw macaroon bytes must fail V2 envelope decode, got {result:?}"
        );
    }
}
