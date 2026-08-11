//! Typed chain-namespace substrate (RFC-0010 v1.4 §ChainId Namespace
//! Extension).
//!
//! `ChainId` is the literal-string handle (operational ergonomics);
//! `ChainNamespace` is the canonical typed representation that anchors
//! the BLAKE3-256 chain derivation domain. The two are linked via
//! `ChainId::namespace()` which resolves any literal against the
//! RFC-allocated namespace table.
//!
//! # RFC-0010 v1.4 (additive on v1.3)
//!
//! v1.3 ships `pub struct ChainId(pub String)` as a stringly-typed
//! newtype. v1.4 adds:
//!
//! 1. RFC-allocated namespace constants (per
//!    [[cipherocto-design-principles]] §Extension over enumeration).
//! 2. `Namespace` newtype wrapper with 17-byte canonical serialization.
//! 3. Validation at the type boundary: `ChainId::new` returns
//!    `Result<Self, ChainNamespaceError>`; `ChainId::new_unchecked`
//!    preserves v1.3 escape behavior for internal callers.
//!
//! # Layer discipline
//!
//! Per [[cipherocto-design-principles]]:
//! - `octo-ident` (Layer B) — `ChainId` + `ChainNamespace` + error.
//! - `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
//!   gains `chain_id` column in a follow-on migration.
//! - `octo-identity-resolver-node` (Layer C) — `IdentityResolverConfig`
//!   gains `chains: Vec<ChainId>` slot in a follow-on mission.

#![forbid(unsafe_code)]

use thiserror::Error;

/// Maximum allowed chain-namespace literal length.
///
/// 64 chars is enough for any RFC-allocated namespace handle
/// (e.g. `cipherocto-mainnet` = 18 chars) + user-extension handles.
pub const MAX_NAMESPACE_LEN: usize = 64;

/// BLAKE3-256 domain separator for chain-namespace tag derivation.
///
/// Per RFC-0010 v1.4 §ChainId Namespace Extension: tag = first 15 bytes
/// of `BLAKE3(BINDING_DOMAIN || literal)`. The domain separator pins the
/// tag derivation to a specific protocol version so a future RFC can
/// re-allocate the tag space without colliding with prior literals.
const CHAIN_NAMESPACE_BINDING_DOMAIN: &[u8] = b"cipherocto/chain-namespace/v1";

/// Precomputed BLAKE3-256(CHAIN_NAMESPACE_BINDING_DOMAIN ||
/// "cipherocto-mainnet"), truncated to 15 bytes.
///
/// Used by the RFC-allocated `CIPHEROCTO_MAINNET` namespace tag.
/// Pinned as a `const` so the canonical bytes can be asserted in
/// test vectors without re-computing the hash at test time.
pub const CIPHEROCTO_MAINNET_TAG: [u8; 15] = [
    0xeb, 0x30, 0x71, 0xb5, 0xe1, 0x13, 0x33, 0x0c, 0x87, 0x63, 0x09, 0x54, 0xe3, 0xcc, 0x08,
];

/// All RFC-allocated chain-namespace tags (RFC-0010 v1.4).
///
/// Production CipherOcto deployments MUST use a tag from this table;
/// user-extension chains fall in the `User` variant and require
/// operator attestation per RFC-0862 §Governance.
pub const RFC_CHAIN_NAMESPACES: &[[u8; 15]] = &[CIPHEROCTO_MAINNET_TAG];

/// Literal handle for the canonical mainnet namespace.
pub const CIPHEROCTO_MAINNET: &str = "cipherocto-mainnet";

/// Typed chain-namespace discriminator (RFC-0010 v1.4).
///
/// v1.3 ships `ChainId` as a stringly-typed `pub struct ChainId(pub String)`.
/// v1.4 ADDS validation at construction time (`ChainId::new` now returns
/// `Result<Self, ChainNamespaceError>`) but the underlying type stays
/// `String` — v1.3 callers using `ChainId::new_unchecked` (preserved as
/// an escape hatch) continue to work verbatim.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct ChainId(pub String);

impl ChainId {
    /// Construct a `ChainId` from a literal string. Validates the
    /// namespace shape per RFC-0010 v1.4 §Validation:
    /// - non-empty
    /// - length ≤ [`MAX_NAMESPACE_LEN`] (64 chars)
    /// - no control characters
    ///
    /// v1.3 callers using `ChainId::new(s)` with a valid literal
    /// (`"cipherocto-mainnet"`, `"partner-mainnet"`, etc.) gain a
    /// stricter constructor that rejects previously-accepted malformed
    /// inputs (empty string, control chars, > 64 chars).
    ///
    /// # Errors
    /// - `ChainNamespaceError::Empty` if `s.is_empty()`.
    /// - `ChainNamespaceError::TooLong { len, max }` if `s.len() > 64`.
    /// - `ChainNamespaceError::ControlChar(c)` if any char is a control
    ///   character (`c.is_control()`).
    pub fn new(s: impl Into<String>) -> Result<Self, ChainNamespaceError> {
        let inner = s.into();
        Self::validate(&inner)?;
        Ok(Self(inner))
    }

    /// Wrap a literal string WITHOUT validation. Internal use only —
    /// the produced `ChainId` may fail later validation paths.
    /// Prefer `ChainId::new` for all external entry points.
    ///
    /// This is the v1.3 escape hatch preserved verbatim per
    /// RFC-0010 v1.4 §Compatibility.
    #[must_use]
    pub fn new_unchecked(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Resolve the `ChainId` to its `ChainNamespace`.
    ///
    /// The RFC-allocated constant table (`RFC_CHAIN_NAMESPACES`) is
    /// consulted first; user-extension chains parse through the
    /// `User` variant. The length byte in the canonical encoding
    /// disambiguates same-tag-different-length collisions (per
    /// RFC-0010 v1.4 TV-8).
    ///
    /// # Errors
    /// Returns `ChainNamespaceError` variants if the underlying
    /// literal fails validation. Validation runs in `from_literal`
    /// before the tag derivation so malformed literals are rejected
    /// before any hashing.
    pub fn namespace(&self) -> Result<ChainNamespace, ChainNamespaceError> {
        ChainNamespace::from_literal(&self.0)
    }

    /// Validate the namespace literal shape (length + control chars).
    /// Pure function; called by `ChainId::new` + `ChainNamespace::from_literal`.
    fn validate(s: &str) -> Result<(), ChainNamespaceError> {
        if s.is_empty() {
            return Err(ChainNamespaceError::Empty);
        }
        if s.len() > MAX_NAMESPACE_LEN {
            return Err(ChainNamespaceError::TooLong {
                len: s.len(),
                max: MAX_NAMESPACE_LEN,
            });
        }
        for c in s.chars() {
            if c.is_control() {
                return Err(ChainNamespaceError::ControlChar(c));
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Default for ChainId {
    /// Defaults to the canonical mainnet namespace (RFC-0010 v1.4 §Compatibility).
    fn default() -> Self {
        // SAFETY: `CIPHEROCTO_MAINNET` is a 17-char literal that passes
        // `ChainId::validate` (non-empty, ≤ 64 chars, no control chars).
        // We use `new_unchecked` here to avoid unwrap at the `Default`
        // trait impl boundary — the `CIPHEROCTO_MAINNET` literal is
        // a compile-time const so the validate-then-unwrap path would
        // be a no-op assertion.
        Self::new_unchecked(CIPHEROCTO_MAINNET)
    }
}

/// RFC-allocated namespace discriminant (per
/// [[cipherocto-design-principles]] §Extension over enumeration).
///
/// 128-bit UUID discriminator pattern adapted to a 17-byte canonical
/// form. The variant byte picks the namespace class (RFC vs user vs
/// reserved); the 15-byte tag is the BLAKE3 truncation; the length
/// byte disambiguates same-tag-different-length collisions.
///
/// Layout:
///
/// ```text
/// [ variant: u8 (Rfc=0x01 | User=0x02 | Reserved=0x00/0x03-0xFF)
/// | tag: [u8; 15]
/// | length: u8 ]
/// ```
///
/// Canonical serialization = 17 bytes
/// (`ChainNamespace::canonical_bytes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainNamespace {
    variant: NamespaceVariant,
    tag: [u8; 15],
    length: u8,
}

impl ChainNamespace {
    /// Resolve an RFC-0010 literal (`"cipherocto-mainnet"`, etc.) to
    /// its typed `ChainNamespace`.
    ///
    /// Returns the `Rfc` variant for tags in `RFC_CHAIN_NAMESPACES`
    /// (after validation), the `User` variant for unrecognized but
    /// valid literals. The length byte is the literal length as `u8`
    /// (must fit in a `u8`; the validation rule `len <= 64` guarantees
    /// this).
    ///
    /// # Errors
    /// - `ChainNamespaceError::Empty` / `TooLong` / `ControlChar` if
    ///   the literal fails `ChainId::validate`.
    pub fn from_literal(literal: &str) -> Result<Self, ChainNamespaceError> {
        ChainId::validate(literal)?;
        let tag = compute_namespace_tag(literal);
        let variant = if RFC_CHAIN_NAMESPACES.contains(&tag) {
            NamespaceVariant::Rfc
        } else {
            NamespaceVariant::User
        };
        Ok(Self {
            variant,
            tag,
            length: literal.len() as u8,
        })
    }

    /// 17-byte canonical serialization (used in WAL entries,
    /// `GovernanceAttestation.chain_id` field, RPC audit logs).
    #[must_use]
    pub fn canonical_bytes(&self) -> [u8; 17] {
        let mut out = [0u8; 17];
        out[0] = self.variant as u8;
        out[1..16].copy_from_slice(&self.tag);
        out[16] = self.length;
        out
    }

    /// Reverse of `canonical_bytes`. Returns `Err` on unknown variant.
    ///
    /// # Errors
    /// - `ChainNamespaceError::ReservedVariant(v)` if `bytes[0]` is
    ///   not `0x01` (Rfc) or `0x02` (User).
    pub fn from_canonical_bytes(bytes: &[u8; 17]) -> Result<Self, ChainNamespaceError> {
        let variant = match bytes[0] {
            0x01 => NamespaceVariant::Rfc,
            0x02 => NamespaceVariant::User,
            v => return Err(ChainNamespaceError::ReservedVariant(v)),
        };
        let mut tag = [0u8; 15];
        tag.copy_from_slice(&bytes[1..16]);
        Ok(Self {
            variant,
            tag,
            length: bytes[16],
        })
    }

    /// Variant tag (Rfc, User, Reserved).
    #[must_use]
    pub fn variant(&self) -> NamespaceVariant {
        self.variant
    }

    /// 15-byte BLAKE3 truncation.
    #[must_use]
    pub fn tag(&self) -> &[u8; 15] {
        &self.tag
    }

    /// Literal length (the original `&str` length that produced this
    /// namespace; same-tag-different-length disambiguator).
    #[must_use]
    pub fn length(&self) -> u8 {
        self.length
    }
}

/// Namespace variant (RFC-0010 v1.4 §Data Structures).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NamespaceVariant {
    /// Reserved for future amendments (variant byte `0x00`,
    /// `0x03`-`0xFF`).
    Reserved = 0x00,
    /// RFC-allocated namespace (variant byte `0x01`).
    Rfc = 0x01,
    /// User-extension namespace (variant byte `0x02`).
    User = 0x02,
}

impl NamespaceVariant {
    /// Round-trip a variant to/from its canonical byte.
    #[must_use]
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// Errors from `ChainId` / `ChainNamespace` validation.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ChainNamespaceError {
    /// Empty namespace literal.
    #[error("empty chain namespace")]
    Empty,

    /// Namespace literal exceeds [`MAX_NAMESPACE_LEN`] (64) chars.
    #[error("chain namespace too long: {len} > max {max}")]
    TooLong {
        /// Observed length.
        len: usize,
        /// Maximum allowed.
        max: usize,
    },

    /// Namespace literal contains a control character.
    #[error("chain namespace contains control character: {0:?}")]
    ControlChar(char),

    /// Reserved namespace variant byte at `from_canonical_bytes` time.
    #[error("reserved namespace variant: {0}")]
    ReservedVariant(u8),
}

/// Compute the 15-byte BLAKE3 namespace tag for a literal.
///
/// Tag = first 15 bytes of `BLAKE3(CHAIN_NAMESPACE_BINDING_DOMAIN || literal)`.
/// Domain separator pins the tag space to RFC-0010 v1.4 so a future
/// RFC can re-allocate without colliding with prior literals.
fn compute_namespace_tag(literal: &str) -> [u8; 15] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CHAIN_NAMESPACE_BINDING_DOMAIN);
    hasher.update(literal.as_bytes());
    let full = hasher.finalize();
    let bytes = full.as_bytes();
    let mut tag = [0u8; 15];
    tag.copy_from_slice(&bytes[..15]);
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_id_new_accepts_valid_literal() {
        let c = ChainId::new("cipherocto-mainnet").unwrap();
        assert_eq!(c.as_str(), "cipherocto-mainnet");
    }

    #[test]
    fn chain_id_new_rejects_empty() {
        assert_eq!(ChainId::new("").unwrap_err(), ChainNamespaceError::Empty);
    }

    #[test]
    fn chain_id_new_rejects_too_long() {
        let s = "a".repeat(MAX_NAMESPACE_LEN + 1);
        let err = ChainId::new(s.clone()).unwrap_err();
        assert_eq!(
            err,
            ChainNamespaceError::TooLong {
                len: MAX_NAMESPACE_LEN + 1,
                max: MAX_NAMESPACE_LEN
            }
        );
    }

    #[test]
    fn chain_id_new_rejects_control_char() {
        let err = ChainId::new("cipherocto\u{0000}mainnet").unwrap_err();
        assert!(matches!(err, ChainNamespaceError::ControlChar('\0')));
    }

    #[test]
    fn chain_id_new_unchecked_skips_validation() {
        // Empty literal: validation would reject, but `new_unchecked`
        // produces a `ChainId` verbatim (v1.3 escape hatch).
        let c = ChainId::new_unchecked("");
        assert_eq!(c.as_str(), "");
        // But `namespace()` re-validates and rejects.
        assert!(matches!(c.namespace(), Err(ChainNamespaceError::Empty)));
    }

    #[test]
    fn default_is_mainnet() {
        let c: ChainId = Default::default();
        assert_eq!(c.as_str(), CIPHEROCTO_MAINNET);
    }

    #[test]
    fn mainnet_namespace_resolves_to_rfc_variant() {
        let c = ChainId::new(CIPHEROCTO_MAINNET).unwrap();
        let ns = c.namespace().unwrap();
        assert_eq!(ns.variant(), NamespaceVariant::Rfc);
        assert_eq!(ns.tag(), &CIPHEROCTO_MAINNET_TAG);
        assert_eq!(ns.length(), CIPHEROCTO_MAINNET.len() as u8);
    }

    #[test]
    fn partner_namespace_resolves_to_user_variant() {
        let c = ChainId::new("partner-mainnet").unwrap();
        let ns = c.namespace().unwrap();
        assert_eq!(ns.variant(), NamespaceVariant::User);
        // Length is preserved (the disambiguator).
        assert_eq!(ns.length(), "partner-mainnet".len() as u8);
    }

    #[test]
    fn canonical_bytes_round_trip() {
        let c = ChainId::new(CIPHEROCTO_MAINNET).unwrap();
        let ns = c.namespace().unwrap();
        let bytes = ns.canonical_bytes();
        let back = ChainNamespace::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(back, ns);
    }

    #[test]
    fn from_canonical_bytes_rejects_reserved_variant() {
        // Variant byte 0x00 = Reserved; 0x03+ also reserved.
        let mut bytes = [0u8; 17];
        bytes[0] = 0x00;
        assert_eq!(
            ChainNamespace::from_canonical_bytes(&bytes).unwrap_err(),
            ChainNamespaceError::ReservedVariant(0x00)
        );
        bytes[0] = 0x03;
        assert_eq!(
            ChainNamespace::from_canonical_bytes(&bytes).unwrap_err(),
            ChainNamespaceError::ReservedVariant(0x03)
        );
    }

    #[test]
    fn distinct_literals_produce_distinct_tags() {
        let c1 = ChainId::new("cipherocto-mainnet").unwrap();
        let c2 = ChainId::new("partner-mainnet").unwrap();
        assert_ne!(
            c1.namespace().unwrap().tag(),
            c2.namespace().unwrap().tag(),
            "distinct literals must produce distinct BLAKE3 tags"
        );
    }

    #[test]
    fn precomputed_mainnet_tag_matches_live_compute() {
        // Regression guard: if CHAIN_NAMESPACE_BINDING_DOMAIN ever
        // changes, this test fires BEFORE the precomputed constant
        // gets out of sync with the live derivation.
        let live = compute_namespace_tag(CIPHEROCTO_MAINNET);
        assert_eq!(
            live, CIPHEROCTO_MAINNET_TAG,
            "CIPHEROCTO_MAINNET_TAG out of sync with live BLAKE3 derivation"
        );
    }

    #[test]
    fn error_display_messages_are_nonempty() {
        // Sanity: every variant produces a non-empty `Display` so
        // operator observability is preserved.
        for err in [
            ChainNamespaceError::Empty,
            ChainNamespaceError::TooLong { len: 70, max: 64 },
            ChainNamespaceError::ControlChar('\n'),
            ChainNamespaceError::ReservedVariant(0x03),
        ] {
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    fn namespace_variant_byte_round_trip() {
        assert_eq!(NamespaceVariant::Rfc.as_byte(), 0x01);
        assert_eq!(NamespaceVariant::User.as_byte(), 0x02);
        assert_eq!(NamespaceVariant::Reserved.as_byte(), 0x00);
    }
}
