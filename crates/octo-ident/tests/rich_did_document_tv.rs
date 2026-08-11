//! 5 canonical rich-DID-document test vectors (RFC-0010 v1.5
//! §Rich DID Documents §Test Vectors).
//!
//! Covers the v1.5 additive surface: service endpoints, controllers,
//! verification methods, capability delegations, cycle detection.
//!
//! ## Test vectors
//!
//! - TV-1 rich_document_round_trip — DidDocument with all 4 v1.5 fields
//!   populated survives borsh round-trip; canonical_hash stable across
//!   v1.5 doc updates.
//! - TV-2 controller_cycle_rejected — `check_controller_cycles` detects
//!   a 2-node cycle (A → B → A).
//! - TV-3 capability_delegation_hash_verifies — `CapabilityDelegation`
//!   token_hash round-trips; the hash is a stable 32-byte BLAKE3
//!   identifier, not the wire form of the token.
//! - TV-4 service_endpoint_uri_absolute_only — non-absolute URIs
//!   (relative, bare word) rejected at construction time.
//! - TV-5 verification_method_type_discriminator — `Ed25519` + `Reserved`
//!   kinds round-trip via `as_byte` / `from_byte`; future PQC kinds land
//!   in `Reserved` slot.

#![allow(clippy::doc_lazy_continuation)]

use octo_ident::{
    canonical_hash, check_controller_cycles, CapabilityDelegation, ControllerCycleError,
    ControllerReference, DidCodec, DidDocument, ServiceEndpoint, ServiceEndpointError,
    VerificationMethod, VerificationMethodKind, MAX_SERVICE_ENDPOINTS,
};

fn sample_doc(public_key: [u8; 32]) -> DidDocument {
    DidDocument {
        public_key,
        revoked: false,
        ..Default::default()
    }
}

fn sample_pubkey(seed: u8) -> [u8; 32] {
    let mut k = [0u8; 32];
    k[31] = seed;
    k
}

/// TV-1 rich_document_round_trip — full DidDocument with v1.5 fields
/// round-trips through borsh; canonical_hash stays stable across doc
/// updates that only change rich fields (per W3C DID Core 1.0 invariant).
#[test]
fn tv1_rich_document_round_trip() {
    let pk = sample_pubkey(0xA1);
    let doc = DidDocument {
        public_key: pk,
        revoked: false,
        service_endpoints: vec![
            ServiceEndpoint::new("homepage", "https://example.com").unwrap(),
            ServiceEndpoint::new("inbox", "cipherocto://inbox.example/v1").unwrap(),
        ],
        controllers: vec![ControllerReference::new("did:octo:zparent")],
        verification_methods: vec![VerificationMethod::ed25519(pk)],
        capability_delegations: vec![CapabilityDelegation::new([0xB1; 32])],
    };

    // Borsh round-trip.
    let bytes = borsh::to_vec(&doc).unwrap();
    let back: DidDocument = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back, doc);

    // canonical_hash is `BINDING_DOMAIN || public_key` only — invariant:
    // adding service endpoints does NOT shift the DID identity.
    let h_full = canonical_hash(&doc);
    let h_minimal = canonical_hash(&sample_doc(pk));
    assert_eq!(
        h_full, h_minimal,
        "rich fields MUST NOT shift canonical_hash"
    );
}

/// TV-2 controller_cycle_rejected — `check_controller_cycles` walks the
/// controller chain via a resolver closure and returns `Err(Cycle)` when
/// a hash appears more than once. Uses `BTreeSet` ordering (per
/// `check_wrapped_chain` pattern).
#[test]
fn tv2_controller_cycle_rejected() {
    // Two DIDs in a cycle: A → B → A.
    let pk_a = sample_pubkey(0xC1);
    let pk_b = sample_pubkey(0xC2);
    let wire_a =
        octo_ident::CanonicalCodec::raw_to_wire(&octo_ident::CanonicalCodec::mint(&pk_a)).unwrap();
    let wire_b =
        octo_ident::CanonicalCodec::raw_to_wire(&octo_ident::CanonicalCodec::mint(&pk_b)).unwrap();
    let raw_a = octo_ident::CanonicalCodec::wire_to_raw(&wire_a).unwrap();
    let raw_b = octo_ident::CanonicalCodec::wire_to_raw(&wire_b).unwrap();

    let doc_a = DidDocument {
        public_key: pk_a,
        revoked: false,
        controllers: vec![ControllerReference::new(wire_b.as_str())],
        ..Default::default()
    };
    let doc_b = DidDocument {
        public_key: pk_b,
        revoked: false,
        controllers: vec![ControllerReference::new(wire_a.as_str())],
        ..Default::default()
    };

    let resolver = |hash: &[u8; 32]| -> Result<Option<DidDocument>, ControllerCycleError> {
        if hash == &raw_a.hash {
            Ok(Some(doc_a.clone()))
        } else if hash == &raw_b.hash {
            Ok(Some(doc_b.clone()))
        } else {
            Ok(None)
        }
    };

    let r = check_controller_cycles(&raw_a.hash, resolver);
    assert!(matches!(r, Err(ControllerCycleError::Cycle(_))));
}

/// TV-3 capability_delegation_hash_verifies — `CapabilityDelegation`
/// is a 32-byte BLAKE3 hash, NOT the wire form of the capability token
/// (per RFC-0957 §Capability Token + RFC-0010 v1.5 §CapabilityDelegation).
/// Two delegations with distinct token_hashes encode to distinct bytes.
#[test]
fn tv3_capability_delegation_hash_verifies() {
    let pk = sample_pubkey(0xD1);
    let hash_a = [0xABu8; 32];
    let hash_b = [0xCDu8; 32];
    let d_a = CapabilityDelegation::new(hash_a);
    let d_b = CapabilityDelegation::new(hash_b);
    assert_eq!(d_a.token_hash, hash_a);
    assert_ne!(d_a.token_hash, d_b.token_hash);

    let doc = DidDocument {
        public_key: pk,
        revoked: false,
        capability_delegations: vec![d_a, d_b],
        ..Default::default()
    };
    let bytes = borsh::to_vec(&doc).unwrap();
    let back: DidDocument = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back.capability_delegations.len(), 2);
    assert_eq!(back.capability_delegations[0].token_hash, hash_a);
    assert_eq!(back.capability_delegations[1].token_hash, hash_b);
}

/// TV-4 service_endpoint_uri_absolute_only — non-absolute URIs (relative,
/// bare word) rejected at construction time. Absolute URIs accepted.
#[test]
fn tv4_service_endpoint_uri_absolute_only() {
    // Absolute URIs accepted (RFC-3986 scheme).
    for uri in [
        "https://example.com",
        "http://example.com/path",
        "cipherocto://inbox.example/v1",
    ] {
        let ep = ServiceEndpoint::new("homepage", uri).unwrap();
        assert_eq!(ep.uri, uri);
    }

    // Non-absolute URIs rejected.
    for bad_uri in [
        "/foo/bar",     // path-absolute
        "foo/bar",      // relative
        "example.com",  // bare word (no scheme)
        "",             // empty
        "://no-scheme", // scheme delimiter but no scheme
    ] {
        let err = ServiceEndpoint::new("homepage", bad_uri).unwrap_err();
        assert_eq!(err, ServiceEndpointError::UriNotAbsolute, "uri: {bad_uri}");
    }

    // MAX_SERVICE_ENDPOINTS bound respected by consumers (the bound is
    // enforced at the `quota-router-storage` schema layer in v1.5; the
    // substrate just exposes the constant).
    const _: () = assert!(MAX_SERVICE_ENDPOINTS >= 1);
}

/// TV-5 verification_method_type_discriminator — `Ed25519` (0x01) +
/// `Reserved` (0x00 + any non-Ed25519 byte) round-trip via
/// `as_byte` / `from_byte`. Future PQC kinds land in `Reserved` and
/// are distinguishable from Ed25519.
#[test]
fn tv5_verification_method_type_discriminator() {
    let pk = sample_pubkey(0xE1);

    // Ed25519 round-trip.
    let vm = VerificationMethod::ed25519(pk);
    assert_eq!(vm.kind, VerificationMethodKind::Ed25519);
    assert_eq!(vm.kind.as_byte(), 0x01);
    assert_eq!(
        VerificationMethodKind::from_byte(0x01),
        VerificationMethodKind::Ed25519
    );

    // Reserved bytes round-trip as Reserved (fail-closed: unknown
    // discriminators get the catch-all kind rather than being
    // misinterpreted as Ed25519).
    for byte in [0x00, 0x02, 0x7F, 0xFF] {
        let kind = VerificationMethodKind::from_byte(byte);
        assert_eq!(kind, VerificationMethodKind::Reserved);
    }
    // `Reserved.as_byte()` is the canonical byte for the kind (0x00);
    // the source byte is not preserved (per W3C-style discriminator
    // pattern: unknown → canonical catch-all).
    assert_eq!(VerificationMethodKind::Reserved.as_byte(), 0x00);

    // DidDocument with mixed verification methods round-trips.
    let doc = DidDocument {
        public_key: pk,
        revoked: false,
        verification_methods: vec![
            VerificationMethod::ed25519(pk),
            VerificationMethod::new(VerificationMethodKind::Reserved, [0u8; 32]),
        ],
        ..Default::default()
    };
    let bytes = borsh::to_vec(&doc).unwrap();
    let back: DidDocument = borsh::from_slice(&bytes).unwrap();
    assert_eq!(back.verification_methods.len(), 2);
    assert_eq!(
        back.verification_methods[0].kind,
        VerificationMethodKind::Ed25519
    );
    assert_eq!(
        back.verification_methods[1].kind,
        VerificationMethodKind::Reserved
    );
}
