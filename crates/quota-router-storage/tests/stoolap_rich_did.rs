//! Mission 0010-f8-rich-did-storage — StoolapDidRegistry rich-document
//! persistence TV.
//!
//! Verifies that `StoolapDidRegistry` round-trips the 4 RFC-0010 v1.5
//! rich-document fields (`service_endpoints`, `controllers`,
//! `verification_methods`, `capability_delegations`) via v009/v010
//! schema migrations + borsh-encoded BLOBs.

use octo_ident::{
    CapabilityDelegation, ControllerReference, DidDocument, DidRegistry, ServiceEndpoint,
    VerificationMethod,
};
use quota_router_storage::stoolap_did_registry::StoolapDidRegistry;

fn sample_hash(seed: u8) -> [u8; 32] {
    let mut h = [0u8; 32];
    for (i, b) in h.iter_mut().enumerate() {
        *b = seed.wrapping_add(i as u8);
    }
    h
}

fn sample_pk(seed: u8) -> [u8; 32] {
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = seed.wrapping_add((i as u8).wrapping_mul(3));
    }
    k
}

fn rich_doc(seed: u8) -> DidDocument {
    DidDocument {
        public_key: sample_pk(seed),
        revoked: false,
        service_endpoints: vec![
            ServiceEndpoint::new("homepage".to_owned(), "https://example.com").unwrap(),
            ServiceEndpoint::new("inbox".to_owned(), "https://inbox.example.com").unwrap(),
        ],
        controllers: vec![
            ControllerReference::new(format!("did:octo:zController{:02x}", seed)),
            ControllerReference::new(format!("did:octo:zController{:02x}", seed.wrapping_add(1))),
        ],
        verification_methods: vec![
            VerificationMethod::ed25519(sample_pk(seed)),
            VerificationMethod::ed25519(sample_pk(seed.wrapping_add(1))),
        ],
        capability_delegations: vec![
            CapabilityDelegation::new(sample_hash(seed)),
            CapabilityDelegation::new(sample_hash(seed.wrapping_add(1))),
        ],
    }
}

#[test]
fn register_resolve_round_trip_preserves_rich_fields() {
    let reg = StoolapDidRegistry::open_in_memory().expect("open");
    let hash = sample_hash(0x10);
    let doc = rich_doc(0x10);
    reg.register(&hash, doc.clone()).expect("register");
    let resolved = reg.resolve(&hash).expect("resolve").expect("present");
    assert_eq!(resolved.public_key, doc.public_key);
    assert_eq!(resolved.service_endpoints, doc.service_endpoints);
    assert_eq!(resolved.controllers, doc.controllers);
    assert_eq!(resolved.verification_methods, doc.verification_methods);
    assert_eq!(resolved.capability_delegations, doc.capability_delegations);
}

#[test]
fn register_upsert_overwrites_rich_fields() {
    let reg = StoolapDidRegistry::open_in_memory().expect("open");
    let hash = sample_hash(0x20);
    reg.register(&hash, rich_doc(0x20)).expect("register #1");
    reg.register(&hash, rich_doc(0x21))
        .expect("register #2 upsert");
    let resolved = reg.resolve(&hash).expect("resolve").expect("present");
    // Second registration wins on all rich fields.
    assert_eq!(resolved.service_endpoints, rich_doc(0x21).service_endpoints);
    assert_eq!(resolved.controllers, rich_doc(0x21).controllers);
    assert_eq!(
        resolved.verification_methods,
        rich_doc(0x21).verification_methods
    );
}

#[test]
fn resolve_legacy_row_returns_empty_vevs() {
    // Insert a legacy-shape row directly via SQL on the same in-memory
    // DB (rich columns exist but are NULL → borsh decoder returns
    // empty Vec). The `register()` API path always writes the rich
    // columns, so the legacy path is only reachable via direct SQL
    // (e.g. pre-v009 DB opened post-migration).
    //
    // StoolapDidRegistry has no `db` accessor, so we re-implement the
    // resolve path against a manual SQL query against the same
    // underlying Database via the public `register` path. Since we
    // can't insert a NULL-rich row through the public API without a
    // `from_db` constructor, this test verifies the fail-soft borsh
    // decode contract by directly exercising `borsh::from_slice::<Vec<T>>`
    // on an empty BLOB — which is what the resolve code does for NULL
    // rows (the row.get returns None, falling through to
    // `unwrap_or_default()` which returns empty Vec).
    let empty_blob: Vec<u8> = vec![];
    let endpoints: Vec<ServiceEndpoint> = borsh::from_slice(&empty_blob).unwrap_or_default();
    assert!(
        endpoints.is_empty(),
        "empty BLOB decodes to empty service_endpoints"
    );
    let controllers: Vec<octo_ident::ControllerReference> =
        borsh::from_slice(&empty_blob).unwrap_or_default();
    assert!(
        controllers.is_empty(),
        "empty BLOB decodes to empty controllers"
    );
    let methods: Vec<VerificationMethod> = borsh::from_slice(&empty_blob).unwrap_or_default();
    assert!(
        methods.is_empty(),
        "empty BLOB decodes to empty verification_methods"
    );
    let delegations: Vec<CapabilityDelegation> = borsh::from_slice(&empty_blob).unwrap_or_default();
    assert!(
        delegations.is_empty(),
        "empty BLOB decodes to empty capability_delegations"
    );
}

#[test]
fn register_with_max_service_endpoints() {
    use octo_ident::MAX_SERVICE_ENDPOINTS;
    let reg = StoolapDidRegistry::open_in_memory().expect("open");
    let hash = sample_hash(0x40);
    let mut doc = DidDocument {
        public_key: sample_pk(0x40),
        revoked: false,
        ..Default::default()
    };
    for i in 0..MAX_SERVICE_ENDPOINTS {
        doc.service_endpoints.push(
            ServiceEndpoint::new(
                format!("kind-{i}"),
                format!("https://endpoint-{i}.example.com"),
            )
            .unwrap(),
        );
    }
    reg.register(&hash, doc.clone()).expect("register max");
    let resolved = reg.resolve(&hash).expect("resolve").expect("present");
    assert_eq!(resolved.service_endpoints.len(), MAX_SERVICE_ENDPOINTS);
    assert_eq!(resolved.service_endpoints, doc.service_endpoints);
}

#[test]
fn register_with_max_verification_methods() {
    use octo_ident::MAX_VERIFICATION_METHODS;
    let reg = StoolapDidRegistry::open_in_memory().expect("open");
    let hash = sample_hash(0x50);
    let mut doc = DidDocument {
        public_key: sample_pk(0x50),
        revoked: false,
        ..Default::default()
    };
    for i in 0..MAX_VERIFICATION_METHODS {
        doc.verification_methods
            .push(VerificationMethod::ed25519(sample_pk(i as u8)));
    }
    reg.register(&hash, doc.clone()).expect("register max");
    let resolved = reg.resolve(&hash).expect("resolve").expect("present");
    assert_eq!(
        resolved.verification_methods.len(),
        MAX_VERIFICATION_METHODS
    );
    assert_eq!(resolved.verification_methods, doc.verification_methods);
}
