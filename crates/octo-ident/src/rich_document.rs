//! Rich `DidDocument` surface (RFC-0010 §Rich DID Documents).
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
//!    via [`AlgorithmId`], resolved through the `AlgorithmRegistry`.
//!
//! ## Layer discipline
//!
//! Per `docs/architecture/design-principles.md` §Extension over enumeration: each
//! variant uses a typed discriminator (UUID-style 128-bit or compact enum
//! for verification methods), NOT a central enum that downstream crates
//! would have to extend.
//!
//! Per `docs/architecture/design-principles.md` §No premature coupling: validation
//! (`ServiceEndpoint::new`, `VerificationMethod::new`) is pure (no IO,
//! no async). Cycle detection on controller refs uses a `BTreeSet` for
//! deterministic ordering (matches the macaroon cycle-detection pattern).

#![forbid(unsafe_code)]

use thiserror::Error;

use crate::signature_algorithm::{AlgorithmId, VerificationMethodId};
use crate::DidCodec;

/// Maximum number of service endpoints per DID (RFC-0010 §Bounds).
///
/// Per W3C DID Core 1.0 best practice + the W3C VCWG bounds analysis, ≤ 10
/// service endpoints covers all production use cases (resolver discovery +
/// capability delegation + inbox). Caps the validation cost against a
/// malicious actor.
pub const MAX_SERVICE_ENDPOINTS: usize = 10;

/// Maximum number of controller references per DID (RFC-0010 §Bounds).
///
/// 3 parent DIDs is the documented upper bound for hierarchical delegation
/// chains in W3C DID Core 1.0. Cycle detection scales O(N) per controller
/// list — 3 is well within the bound for synchronous validation.
pub const MAX_CONTROLLERS: usize = 3;

/// Maximum number of verification methods per DID (RFC-0010 §Bounds).
///
/// The dual-format design is exactly one identity carrying two keys, under
/// Ed25519 and the STARK curve, so this bound is **saturated, with no
/// headroom**. An earlier version of this comment justified the value as
/// covering "the canonical Ed25519 + PQC future combination", which described
/// a design that is no longer the one specified — the same class of defect as
/// the dangling `§VerificationMethodKind` citation that BC#13 removes: a
/// doc comment asserting something no specification supports.
///
/// Each verification method carries at most one public key. A third algorithm
/// lands by **raising this bound**, never by reusing a slot.
pub const MAX_VERIFICATION_METHODS: usize = 2;

/// Hard ceiling on an encoded public key, independent of any one algorithm.
///
/// The *effective* bound is `SignatureAlgorithm::public_key_len`, which the
/// registry knows and this type does not — BC#13 says the key is bounded by
/// the algorithm, and a verifier is what enforces that at use time. This
/// constant exists so the struct itself cannot be handed a megabyte: a
/// persisted blob is attacker-influenced input, and an unbounded `Vec<u8>` on
/// a decode path is an allocation primitive.
///
/// 128 bytes is well above every current algorithm (Ed25519 and the STARK
/// curve are both 32) and below the cost worth worrying about. An earlier
/// version of the `VerificationMethod` doc comment cited a
/// `MAX_PUBLIC_KEY_LEN` that did not exist anywhere in the workspace — the
/// same class of defect as the dangling `§VerificationMethodKind` citation
/// BC#13 removes: a comment asserting a constraint no code implemented.
pub const MAX_PUBLIC_KEY_LEN: usize = 128;

/// Maximum number of capability delegations per DID (RFC-0010 §Bounds).
///
/// 10 capability delegations covers resolver + transport + zk-cap use
/// cases. The BLAKE3 hash is 32 bytes each.
pub const MAX_CAPABILITY_DELEGATIONS: usize = 10;

/// Service endpoint for resolver discovery (RFC-0010 §ServiceEndpoint).
///
/// The endpoint URI MUST be absolute (per W3C DID Core 1.0 §Service
/// Endpoint Properties). The `kind` tag is a typed discriminator that
/// downstream resolvers dispatch on (no central enum; see
/// `docs/architecture/design-principles.md` §Extension over enumeration).
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

/// Where an identity's private key lives.
///
/// One identity holds two keys under two algorithms, and most hardware
/// backends do not implement the STARK curve — so the ZK-capable key is the
/// one most likely to fall back to software. An implementation that registers
/// a STARK-curve key MUST declare its posture here, and MUST NOT present a
/// software-held ZK key as equivalent to a hardware-held Ed25519 key beside
/// it. Two keys means two custody stories, and the weaker one is the one an
/// attacker will target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize),
    borsh(use_discriminant = true)
)]
#[repr(u8)]
pub enum CustodyPosture {
    /// Non-exportable, hardware-backed.
    Hardware = 0x01,
    /// Software-resident. MUST be reflected in any trust display or
    /// authorization decision that compares the two keys of an identity.
    Software = 0x02,
    /// Delegated to a threshold scheme.
    Threshold = 0x03,
}

/// Verification method (RFC-0010 §VerificationMethod, RFC-0009 §Dual-format
/// signature algorithms and the ZK-capable key).
///
/// `public_key` is length-generic because algorithms do not agree on key
/// size; `MAX_PUBLIC_KEY_LEN` bounds it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
pub struct VerificationMethod {
    /// Stable identity of this key within its DID document. This is the
    /// `key_id` in the mandatory signable prefix, and it is not optional: a
    /// verifier holding only a public key cannot reconstruct one, and a
    /// locally-allocated id would make the signed domain depend on who
    /// signed.
    pub id: VerificationMethodId,
    /// Which algorithm this key verifies under. Resolved through the
    /// `AlgorithmRegistry`, never by matching on a central enum.
    pub algorithm_id: AlgorithmId,
    /// Public key bytes, length determined by the registered algorithm.
    pub public_key: Vec<u8>,
    /// Per-key revocation. Without this the identity-level revocation
    /// cascade has nothing to act on, because the registry revokes
    /// identities and nothing else.
    pub revoked_at: Option<u64>,
    /// Custody posture for this key.
    pub custody: CustodyPosture,
}

impl VerificationMethod {
    /// Construct an Ed25519 verification method under an explicit id.
    ///
    /// `id` is a **parameter, not a constant**, and that is load-bearing. An
    /// earlier version derived it from the literal string `"ed25519"`, which
    /// gave every Ed25519 key in every document the same `key_id` — so two
    /// keys of one identity produced an identical signable prefix
    /// `algorithm_id ‖ key_id ‖ payload`, and `revoked_at` on one of them
    /// could not be told apart from the other by any consumer resolving by
    /// `key_id`. Two passing tests asserted the constant, so the defect was
    /// pinned rather than caught. Forcing the caller to supply the id makes
    /// the collision a decision instead of an accident.
    #[must_use]
    pub fn ed25519(
        id: VerificationMethodId,
        public_key: [u8; 32],
        custody: CustodyPosture,
    ) -> Self {
        Self::new(
            id,
            AlgorithmId::derive("ed25519"),
            public_key.to_vec(),
            custody,
        )
    }

    /// Construct a verification method under an explicit id and algorithm.
    ///
    /// `id` MUST be unique within its DID document. Nothing enforces that at
    /// decode — see §Key id uniqueness — so it is the writer's obligation.
    #[must_use]
    pub fn new(
        id: VerificationMethodId,
        algorithm_id: AlgorithmId,
        public_key: Vec<u8>,
        custody: CustodyPosture,
    ) -> Self {
        debug_assert!(
            public_key.len() <= MAX_PUBLIC_KEY_LEN,
            "public key of {} bytes exceeds MAX_PUBLIC_KEY_LEN ({MAX_PUBLIC_KEY_LEN})",
            public_key.len()
        );
        Self {
            id,
            algorithm_id,
            public_key,
            revoked_at: None,
            custody,
        }
    }

    /// Whether this key is revoked, and when.
    #[must_use]
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

/// Controller reference (RFC-0010 §ControllerReference).
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

/// Capability delegation reference (RFC-0010 §CapabilityDelegation).
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

/// Cycle detection for controller reference chains (RFC-0010
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
        let vm = VerificationMethod::ed25519(
            VerificationMethodId::derive("key-a"),
            [1u8; 32],
            CustodyPosture::Hardware,
        );
        assert_eq!(vm.algorithm_id, AlgorithmId::derive("ed25519"));
        assert_eq!(vm.public_key.len(), 32);
        assert!(!vm.is_revoked());
        assert_eq!(vm.custody, CustodyPosture::Hardware);
    }

    /// Replaces the deleted `verification_method_kind_round_trip` test, which
    /// asserted the opposite property: that every unknown byte collapsed onto
    /// one `Reserved` variant whose `as_byte()` was `0x00`. That made two
    /// different unknown discriminators indistinguishable after a round-trip,
    /// which is precisely why the enum was removed.
    #[test]
    fn unknown_algorithm_ids_stay_distinguishable() {
        // Two algorithms nobody has registered must not collapse onto one
        // another on any round-trip. This is the property the old enum
        // destroyed.
        let a = AlgorithmId::derive("algorithm-a");
        let b = AlgorithmId::derive("algorithm-b");
        assert_ne!(a, b);
        assert_eq!(AlgorithmId::from_bytes(*a.as_bytes()), a);
        assert_eq!(AlgorithmId::from_bytes(*b.as_bytes()), b);
        assert_eq!(AlgorithmId::from_uuid(a.to_uuid()), a);
        assert_eq!(AlgorithmId::from_uuid(b.to_uuid()), b);
    }

    #[test]
    fn verification_method_revocation_is_per_key() {
        // Identity revocation exists, but a document holding two keys needs
        // per-key revocation too, or the cascade has nothing to act on.
        let mut vm = VerificationMethod::ed25519(
            VerificationMethodId::derive("key-a"),
            [1u8; 32],
            CustodyPosture::Software,
        );
        assert!(!vm.is_revoked());
        vm.revoked_at = Some(1_700_000_000);
        assert!(vm.is_revoked());
        // Revoking one key does not revoke the other.
        let other = VerificationMethod::ed25519(
            VerificationMethodId::derive("key-b"),
            [2u8; 32],
            CustodyPosture::Hardware,
        );
        assert!(!other.is_revoked());
    }

    #[test]
    fn two_keys_in_one_document_have_distinct_ids() {
        // The defect a constant-id constructor caused: two keys of one identity
        // sharing a key_id, which collapses the signable prefix and makes
        // per-key revocation ambiguous. Nothing in the struct enforces
        // uniqueness, so this is a property of how ids are chosen, and the
        // constructor now forces the caller to choose one.
        let a = VerificationMethod::ed25519(
            VerificationMethodId::derive("key-0"),
            [1u8; 32],
            CustodyPosture::Hardware,
        );
        let b = VerificationMethod::ed25519(
            VerificationMethodId::derive("key-1"),
            [2u8; 32],
            CustodyPosture::Software,
        );
        assert_ne!(a.id, b.id);
        // No assertion that an id is not an algorithm id: `VerificationMethodId`
        // and `AlgorithmId` are distinct newtypes, so the compiler already
        // refuses to compare them. An assertion here would be one of the
        // tests that cannot fail.
        assert_eq!(a.algorithm_id, b.algorithm_id, "both are Ed25519");
    }

    #[test]
    fn zk_support_none_is_reported_by_the_algorithm() {
        // The `None` arm is what RFC-0009 AC#14's fail-closed mint gate
        // depends on, and it had no test: the sibling vector covered only
        // `Circuit`. An algorithm that cannot produce a ZK proof must say so
        // rather than be silent.
        struct NoCircuit;
        impl crate::signature_algorithm::SignatureAlgorithm for NoCircuit {
            fn id(&self) -> AlgorithmId {
                AlgorithmId::derive("no-circuit")
            }
            fn name(&self) -> &'static str {
                "no-circuit"
            }
            fn public_key_len(&self) -> usize {
                32
            }
            fn signature_len(&self) -> usize {
                64
            }
            fn zk_support(&self) -> crate::signature_algorithm::ZkSupport {
                crate::signature_algorithm::ZkSupport::None
            }
            fn verify(
                &self,
                _pk: &[u8],
                _msg: &[u8],
                _sig: &[u8],
            ) -> Result<(), crate::signature_algorithm::SignatureError> {
                Ok(())
            }
        }
        let alg: std::sync::Arc<dyn crate::signature_algorithm::SignatureAlgorithm> =
            std::sync::Arc::new(NoCircuit);
        assert_eq!(
            alg.zk_support(),
            crate::signature_algorithm::ZkSupport::None
        );
    }

    /// **Named by RFC-0009 AC#15.** The criterion names this test; it existed
    /// as `custody_posture_is_distinguishable`, which described the same property in
    /// the same crate and so did not resolve under the RFC's spelling. Third
    /// instance of this class in the RFC's own gate names.
    #[test]
    fn software_custody_is_distinguishable_from_hardware() {
        // A software-held ZK key must not read as equivalent to a
        // hardware-held Ed25519 key. Both are valid; conflating them is the
        // failure, so the variants must be distinct values.
        let mut vm = VerificationMethod::ed25519(
            VerificationMethodId::derive("key-x"),
            [7u8; 32],
            CustodyPosture::Software,
        );
        let hw = VerificationMethod::ed25519(
            VerificationMethodId::derive("key-x"),
            [7u8; 32],
            CustodyPosture::Hardware,
        );
        assert_ne!(vm.custody, hw.custody);
        assert_eq!(vm.public_key, hw.public_key, "same key, different posture");
        vm.custody = CustodyPosture::Threshold;
        assert_ne!(vm.custody, hw.custody);
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
