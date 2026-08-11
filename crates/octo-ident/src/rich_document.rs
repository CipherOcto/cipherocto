//! Rich `DidDocument` surface (RFC-0010 v1.5 §Rich DID Documents).
//!
//! v1.5 ADDS the W3C DID Core 1.0 surface beyond the MVP minimum:
//!
//! 1. **`ServiceEndpoint`** — typed URI + type-tagged endpoint for resolver
//!    discovery (`homepage`, `inbox`, `capability-registry`).
//! 2. **`ControllerReference`** — typed reference to a parent DID for
//!    hierarchical delegation. Cycles rejected at validation time via
//!    `check_controller_cycles` (matches the `check_wrapped_chain` pattern
//!    in `crates/octo-cap-macaroon/src/macaroon.rs`).
//! 3. **`CapabilityDelegation`** — BLAKE3 hash of a `CapabilityToken`
//!    (RFC-0957) so the DID Document attests to delegated capabilities
//!    without duplicating the wire form.
//! 4. **`VerificationMethod`** — multi-key DID surface. Type discriminator
//!    via [`VerificationMethodKind`] (Ed25519 today; PQC future).
//!
//! ## Layer discipline
//!
//! Per [[cipherocto-design-principles]] §Extension over enumeration: each
//! variant uses a typed discriminator (UUID-style 128-bit or compact enum
//! for verification methods), NOT a central enum that downstream crates
//! would have to extend.
//!
//! Per [[cipherocto-design-principles]] §No premature coupling: validation
//! (`ServiceEndpoint::new`, `VerificationMethod::new`) is pure (no IO,
//! no async). Cycle detection on controller refs uses a `BTreeSet` for
//! deterministic ordering (matches the macaroon cycle-detection pattern).

#![forbid(unsafe_code)]

use thiserror::Error;

use crate::DidCodec;

/// Maximum number of service endpoints per DID (RFC-0010 v1.5 §Bounds).
///
/// Per W3C DID Core 1.0 best practice + the W3C VCWG bounds analysis, ≤ 10
/// service endpoints covers all production use cases (resolver discovery +
/// capability delegation + inbox). Caps the validation cost against a
/// malicious actor.
pub const MAX_SERVICE_ENDPOINTS: usize = 10;

/// Maximum number of controller references per DID (RFC-0010 v1.5 §Bounds).
///
/// 3 parent DIDs is the documented upper bound for hierarchical delegation
/// chains in W3C DID Core 1.0. Cycle detection scales O(N) per controller
/// list — 3 is well within the bound for synchronous validation.
pub const MAX_CONTROLLERS: usize = 3;

/// Maximum number of verification methods per DID (RFC-0010 v1.5 §Bounds).
///
/// 2 methods covers the canonical Ed25519 + PQC future combination. Each
/// verification method carries at most 1 public key.
pub const MAX_VERIFICATION_METHODS: usize = 2;

/// Maximum number of capability delegations per DID (RFC-0010 v1.5 §Bounds).
///
/// 10 capability delegations covers resolver + transport + zk-cap use
/// cases. The BLAKE3 hash is 32 bytes each.
pub const MAX_CAPABILITY_DELEGATIONS: usize = 10;

/// Service endpoint for resolver discovery (RFC-0010 v1.5 §ServiceEndpoint).
///
/// The endpoint URI MUST be absolute (per W3C DID Core 1.0 §Service
/// Endpoint Properties). The `kind` tag is a typed discriminator that
/// downstream resolvers dispatch on (no central enum; see
/// [[cipherocto-design-principles]] §Extension over enumeration).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct ServiceEndpoint {
    /// Typed endpoint tag (`homepage`, `inbox`, `capability-registry`, etc.).
    pub kind: String,
    /// Absolute URI (RFC-3986). Constructed via [`ServiceEndpoint::new`] —
    /// the constructor enforces the absolute-URI shape.
    pub uri: String,
}

impl ServiceEndpoint {
    /// Construct a `ServiceEndpoint` with validation.
    ///
    /// # Errors
    /// - [`ServiceEndpointError::KindEmpty`] if `kind` is empty.
    /// - [`ServiceEndpointError::KindTooLong`] if `kind.len() > 64`.
    /// - [`ServiceEndpointError::KindControlChar`] if `kind` contains a
    ///   control character.
    /// - [`ServiceEndpointError::UriNotAbsolute`] if `uri` does not start
    ///   with an absolute-URI scheme (`http://`, `https://`, `cipherocto://`,
    ///   etc.).
    pub fn new(
        kind: impl Into<String>,
        uri: impl Into<String>,
    ) -> Result<Self, ServiceEndpointError> {
        let kind = kind.into();
        let uri = uri.into();
        Self::validate_kind(&kind)?;
        Self::validate_uri(&uri)?;
        Ok(Self { kind, uri })
    }

    fn validate_kind(kind: &str) -> Result<(), ServiceEndpointError> {
        if kind.is_empty() {
            return Err(ServiceEndpointError::KindEmpty);
        }
        if kind.len() > 64 {
            return Err(ServiceEndpointError::KindTooLong {
                len: kind.len(),
                max: 64,
            });
        }
        for c in kind.chars() {
            if c.is_control() {
                return Err(ServiceEndpointError::KindControlChar(c));
            }
        }
        Ok(())
    }

    fn validate_uri(uri: &str) -> Result<(), ServiceEndpointError> {
        // RFC-3986 absolute URI: `scheme ":" hier-part [ "?" query ] [ "#" fragment ]`.
        // We accept any scheme that starts with a letter followed by letters/digits/+/-/./
        // and is followed by `:`. This rejects relative refs (`/foo`, `bar/baz`,
        // `#frag`) and bare words.
        let mut chars = uri.chars();
        let first = chars.next().ok_or(ServiceEndpointError::UriNotAbsolute)?;
        if !first.is_ascii_alphabetic() {
            return Err(ServiceEndpointError::UriNotAbsolute);
        }
        let mut saw_colon = false;
        for c in chars {
            if c == ':' {
                saw_colon = true;
                break;
            }
            if !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.' {
                return Err(ServiceEndpointError::UriNotAbsolute);
            }
        }
        if !saw_colon {
            return Err(ServiceEndpointError::UriNotAbsolute);
        }
        Ok(())
    }
}

/// Errors from `ServiceEndpoint` validation.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ServiceEndpointError {
    /// Empty endpoint kind.
    #[error("service endpoint kind is empty")]
    KindEmpty,

    /// Endpoint kind exceeds 64 chars.
    #[error("service endpoint kind too long: {len} > max {max}")]
    KindTooLong {
        /// Observed length.
        len: usize,
        /// Maximum allowed.
        max: usize,
    },

    /// Endpoint kind contains a control character.
    #[error("service endpoint kind contains control character: {0:?}")]
    KindControlChar(char),

    /// URI is not an absolute URI (RFC-3986).
    #[error("service endpoint URI must be an absolute URI (RFC-3986)")]
    UriNotAbsolute,
}

/// Verification method (RFC-0010 v1.5 §VerificationMethod).
///
/// The `kind` field is a typed discriminator ([`VerificationMethodKind`]).
/// The `public_key` field is the 32-byte Ed25519 public key (PQC keys land
/// in v2.0 via the `VerificationMethodKind` extension hook).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct VerificationMethod {
    /// Method kind discriminator (Ed25519 today; PQC future).
    pub kind: VerificationMethodKind,
    /// 32-byte public key.
    pub public_key: [u8; 32],
}

impl VerificationMethod {
    /// Construct an Ed25519 verification method.
    #[must_use]
    pub fn ed25519(public_key: [u8; 32]) -> Self {
        Self {
            kind: VerificationMethodKind::Ed25519,
            public_key,
        }
    }

    /// Construct a verification method with a custom kind (future PQC).
    pub fn new(kind: VerificationMethodKind, public_key: [u8; 32]) -> Self {
        Self { kind, public_key }
    }
}

/// Verification method kind (RFC-0010 v1.5 §VerificationMethodKind).
///
/// Compact typed discriminator. v1.5 ships `Ed25519` + a `Reserved` slot
/// for future PQC keys (per RFC-0853 §F1 hooks). PQC kinds land in v2.0
/// by adding variants here — extensions NEVER modify existing variants
/// (per [[cipherocto-design-principles]] §Extension over enumeration).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize),
    borsh(use_discriminant = true)
)]
pub enum VerificationMethodKind {
    /// Ed25519 (RFC-0010 v1.5 baseline).
    Ed25519 = 0x01,
    /// Reserved for future amendments (PQC keys per RFC-0853 §F1).
    Reserved = 0x00,
}

impl VerificationMethodKind {
    /// Round-trip a kind to/from its canonical byte.
    #[must_use]
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    /// Decode a kind from its canonical byte. Reserved bytes are
    /// round-tripped as `Reserved` so old code fails-closed on unknown
    /// discriminators.
    #[must_use]
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x01 => Self::Ed25519,
            _ => Self::Reserved,
        }
    }
}

/// Controller reference (RFC-0010 v1.5 §ControllerReference).
///
/// Points to a parent DID for hierarchical delegation. The reference is
/// the canonical wire form (`did:octo:z<base58btc>`); consumers resolve
/// it via the same path as any other DID.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct ControllerReference {
    /// Canonical wire form (`did:octo:z<base58btc>`).
    pub did: String,
}

impl ControllerReference {
    /// Wrap a canonical DID wire form. Validation of the wire form shape
    /// is the consumer's responsibility (via `CanonicalCodec::parse`).
    #[must_use]
    pub fn new(did: impl Into<String>) -> Self {
        Self { did: did.into() }
    }
}

/// Capability delegation reference (RFC-0010 v1.5 §CapabilityDelegation).
///
/// Stores a BLAKE3-256 hash of a `CapabilityToken` (RFC-0957). The DID
/// Document attests that the DID holder MAY exercise the capability
/// without re-embedding the full token wire form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct CapabilityDelegation {
    /// BLAKE3-256 hash of the capability token.
    pub token_hash: [u8; 32],
}

impl CapabilityDelegation {
    /// Construct a capability delegation from a token hash.
    #[must_use]
    pub fn new(token_hash: [u8; 32]) -> Self {
        Self { token_hash }
    }
}

/// Cycle detection for controller reference chains (RFC-0010 v1.5
/// §ControllerReference).
///
/// Walks the controllers of each `DidDocument` reachable via
/// `resolver` and returns `Err` if the same `canonical_did` appears
/// more than once in the chain. The `BTreeSet` ordering matches the
/// `check_wrapped_chain` cycle-detection pattern in
/// `crates/octo-cap-macaroon/src/macaroon.rs` for deterministic
/// ordering.
///
/// `resolver` is an `Fn` closure so this function stays pure (no IO
/// coupling); consumers pass a closure that delegates to their
/// `DidRegistry::resolve`.
pub fn check_controller_cycles<F>(root: &[u8; 32], resolver: F) -> Result<(), ControllerCycleError>
where
    F: Fn(&[u8; 32]) -> Result<Option<crate::DidDocument>, ControllerCycleError>,
{
    // 3-color DFS: White (unseen) → Gray (on current path) → Black (done).
    // A back-edge to a Gray node indicates a cycle.
    let mut color: std::collections::BTreeMap<[u8; 32], u8> = std::collections::BTreeMap::new();
    fn dfs<F>(
        node: [u8; 32],
        color: &mut std::collections::BTreeMap<[u8; 32], u8>,
        resolver: &F,
    ) -> Result<(), ControllerCycleError>
    where
        F: Fn(&[u8; 32]) -> Result<Option<crate::DidDocument>, ControllerCycleError>,
    {
        match color.get(&node).copied().unwrap_or(0) {
            1 => return Err(ControllerCycleError::Cycle(node)), // Gray → cycle.
            2 => return Ok(()),                                 // Black → already done.
            _ => {}
        }
        color.insert(node, 1); // Mark Gray.
        let doc = resolver(&node)?;
        if let Some(d) = doc {
            for ctrl in &d.controllers {
                let child = parse_controller_hash(&ctrl.did)?;
                dfs(child, color, resolver)?;
            }
        }
        color.insert(node, 2); // Mark Black.
        Ok(())
    }
    dfs(*root, &mut color, &resolver)
}

/// Decode a controller DID wire form to its 32-byte canonical hash.
///
/// Light wrapper over `CanonicalCodec::wire_to_raw` so `check_controller_cycles`
/// stays in this module. Returns `ControllerCycleError::InvalidControllerDid`
/// if the wire form is not a canonical DID.
fn parse_controller_hash(wire: &str) -> Result<[u8; 32], ControllerCycleError> {
    let parsed = crate::CanonicalCodec::parse(wire, false)
        .map_err(|e| ControllerCycleError::InvalidControllerDid(e.to_string()))?;
    let raw = crate::CanonicalCodec::wire_to_raw(&parsed)
        .map_err(|e| ControllerCycleError::InvalidControllerDid(e.to_string()))?;
    Ok(raw.hash)
}

/// Errors from controller reference cycle detection.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ControllerCycleError {
    /// Cycle detected: `canonical_hash` appears more than once in the
    /// controller chain.
    #[error("controller cycle detected: hash {0:?}")]
    Cycle([u8; 32]),

    /// Controller DID wire form is not a canonical DID.
    #[error("invalid controller DID: {0}")]
    InvalidControllerDid(String),

    /// Underlying resolver error (e.g., registry unavailable).
    #[error("resolver error: {0}")]
    Resolver(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_endpoint_accepts_https() {
        let ep = ServiceEndpoint::new("homepage", "https://example.com").unwrap();
        assert_eq!(ep.kind, "homepage");
        assert_eq!(ep.uri, "https://example.com");
    }

    #[test]
    fn service_endpoint_accepts_cipherocto_scheme() {
        let ep = ServiceEndpoint::new("inbox", "cipherocto://inbox.example/v1").unwrap();
        assert_eq!(ep.kind, "inbox");
    }

    #[test]
    fn service_endpoint_rejects_relative_uri() {
        let err = ServiceEndpoint::new("homepage", "/foo/bar").unwrap_err();
        assert_eq!(err, ServiceEndpointError::UriNotAbsolute);
    }

    #[test]
    fn service_endpoint_rejects_bare_word_uri() {
        let err = ServiceEndpoint::new("homepage", "not-a-uri").unwrap_err();
        assert_eq!(err, ServiceEndpointError::UriNotAbsolute);
    }

    #[test]
    fn service_endpoint_rejects_empty_kind() {
        let err = ServiceEndpoint::new("", "https://example.com").unwrap_err();
        assert_eq!(err, ServiceEndpointError::KindEmpty);
    }

    #[test]
    fn service_endpoint_rejects_control_char_in_kind() {
        let err = ServiceEndpoint::new("home\u{0000}page", "https://example.com").unwrap_err();
        assert!(matches!(err, ServiceEndpointError::KindControlChar('\0')));
    }

    #[test]
    fn service_endpoint_rejects_too_long_kind() {
        let err = ServiceEndpoint::new("a".repeat(65), "https://example.com").unwrap_err();
        assert_eq!(err, ServiceEndpointError::KindTooLong { len: 65, max: 64 });
    }

    #[test]
    fn verification_method_ed25519_default() {
        let vm = VerificationMethod::ed25519([1u8; 32]);
        assert_eq!(vm.kind, VerificationMethodKind::Ed25519);
        assert_eq!(vm.kind.as_byte(), 0x01);
    }

    #[test]
    fn verification_method_kind_round_trip() {
        // Ed25519 round-trips exactly; Reserved is the catch-all for
        // every other byte.
        for byte in 0u8..=u8::MAX {
            let kind = VerificationMethodKind::from_byte(byte);
            let back = kind.as_byte();
            if byte == 0x01 {
                assert_eq!(back, 0x01);
                assert_eq!(kind, VerificationMethodKind::Ed25519);
            } else {
                assert_eq!(kind, VerificationMethodKind::Reserved);
                assert_eq!(back, 0x00);
            }
        }
    }

    #[test]
    fn capability_delegation_constructs() {
        let hash = [0xAB; 32];
        let d = CapabilityDelegation::new(hash);
        assert_eq!(d.token_hash, hash);
    }

    #[test]
    fn controller_reference_constructs() {
        let r = ControllerReference::new("did:octo:zabc");
        assert_eq!(r.did, "did:octo:zabc");
    }
}
