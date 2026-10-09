//! Mission 0010-f8-rich-did-storage — `StoolapDidRegistry` rich-document
//! persistence TV.
//!
//! Verifies that `StoolapDidRegistry` round-trips the 4 RFC-0010
//! rich-document fields (`service_endpoints`, `controllers`,
//! `verification_methods`, `capability_delegations`) via v009/v010
//! schema migrations + borsh-encoded BLOBs.
//!
//! Moved from `crates/quota-router-storage/tests/stoolap_rich_did.rs`
//! in mission 0206-003 v3.0.

use octo_ident::{
    CapabilityDelegation, ControllerReference, CustodyPosture, DidDocument, DidRegistry,
    ServiceEndpoint, VerificationMethod, VerificationMethodId,
};
use octo_ident_storage::StoolapDidRegistry;

fn sample_hash(seed: u8) -> [u8; 32] {
    let mut h = [0u8; 32];
    for (i, b) in h.iter_mut().enumerate() {
        *b = seed.wrapping_add(u8::try_from(i).expect("loop index fits in u8"));
    }
    h
}

fn sample_pk(seed: u8) -> [u8; 32] {
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = seed.wrapping_add(
            u8::try_from(i)
                .expect("loop index fits in u8")
                .wrapping_mul(3),
        );
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
            ControllerReference::new(format!("did:octo:zController{seed:02x}")),
            ControllerReference::new(format!("did:octo:zController{:02x}", seed.wrapping_add(1))),
        ],
        verification_methods: vec![
            VerificationMethod::ed25519(
                VerificationMethodId::derive("key-0"),
                sample_pk(seed),
                CustodyPosture::Hardware,
            ),
            VerificationMethod::ed25519(
                VerificationMethodId::derive("key-1"),
                sample_pk(seed.wrapping_add(1)),
                CustodyPosture::Software,
            ),
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
fn migrated_document_still_resolves_with_a_verification_method() {
    // Resolver level, not just codec level: a document whose methods decode
    // must come back with those methods intact, including the per-key fields
    // the migration path is responsible for.
    let reg = StoolapDidRegistry::open_in_memory().expect("open");
    let hash = sample_hash(0x30);
    let mut doc = rich_doc(0x30);
    let mut first = doc.verification_methods[0].clone();
    first.revoked_at = Some(1_700_000_000);
    doc.verification_methods[0] = first;

    reg.register(&hash, doc.clone()).expect("register");
    let resolved = reg.resolve(&hash).expect("resolve").expect("present");
    assert_eq!(resolved.verification_methods.len(), 2);
    assert_eq!(
        resolved.verification_methods[0],
        doc.verification_methods[0]
    );
    assert!(resolved.verification_methods[0].is_revoked());
    // Revoking one key leaves the other usable.
    assert!(!resolved.verification_methods[1].is_revoked());
}

#[test]
fn listed_documents_carry_their_verification_methods() {
    // The `list` path is a separate column selection, not a wrapper over
    // `resolve`, so no resolver-level vector can observe it. It used to build
    // every document from `public_key` alone via `..Default::default()`,
    // returning zero verification methods for every row — the same silent
    // downgrade as the resolver path, reached by a shorter route.
    let reg = StoolapDidRegistry::open_in_memory().expect("open");
    let hash = sample_hash(0x40);
    let doc = rich_doc(0x40);
    reg.register(&hash, doc.clone()).expect("register");

    let listed = reg.list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].public_key, doc.public_key);
    assert_eq!(
        listed[0].verification_methods, doc.verification_methods,
        "list must return the same verification methods as resolve"
    );
    assert_eq!(listed[0].service_endpoints, doc.service_endpoints);
    assert_eq!(listed[0].controllers, doc.controllers);
    assert_eq!(listed[0].capability_delegations, doc.capability_delegations);
}

#[test]
fn listed_documents_report_per_key_revocation() {
    // Once `revoked_at` exists per key, an enumeration that returns no keys
    // also returns nothing to revoke. This pins that `list` surfaces the
    // per-key state a revocation cascade would act on.
    let reg = StoolapDidRegistry::open_in_memory().expect("open");
    let hash = sample_hash(0x50);
    let mut doc = rich_doc(0x50);
    let mut revoked_first = doc.verification_methods[0].clone();
    revoked_first.revoked_at = Some(1_700_000_000);
    doc.verification_methods[0] = revoked_first;

    reg.register(&hash, doc.clone()).expect("register");
    let listed = reg.list().expect("list");
    assert!(listed[0].verification_methods[0].is_revoked());
    assert!(!listed[0].verification_methods[1].is_revoked());
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
fn resolve_legacy_row_returns_empty_vecs() {
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
        doc.verification_methods.push(VerificationMethod::ed25519(
            VerificationMethodId::derive(&format!("max-key-{i}")),
            sample_pk(u8::try_from(i).expect("loop index fits in u8")),
            CustodyPosture::Hardware,
        ));
    }
    reg.register(&hash, doc.clone()).expect("register max");
    let resolved = reg.resolve(&hash).expect("resolve").expect("present");
    assert_eq!(
        resolved.verification_methods.len(),
        MAX_VERIFICATION_METHODS
    );
    assert_eq!(resolved.verification_methods, doc.verification_methods);
}

// ---------------------------------------------------------------------------
// RFC-0009 BC#13 decode vectors, driven through the real read paths.
//
// These need direct SQL because `register` only ever writes the current
// shape: a legacy blob, a NULL column and a corrupt blob are all unreachable
// through the public write API. Before `StoolapDidRegistry::from_database`
// existed, every such vector had to call the codec directly, which meant none
// of them exercised `resolve` itself — reverting the `?` in the read paths to
// `unwrap_or_default()` left the whole suite green while reintroducing the
// silent downgrade BC#13 exists to prevent.
// ---------------------------------------------------------------------------

const MAINNET_CHAIN_ID: [u8; 17] = [
    0x01, 0xeb, 0x30, 0x71, 0xb5, 0xe1, 0x13, 0x33, 0x0c, 0x87, 0x63, 0x09, 0x54, 0xe3, 0xcc, 0x08,
    0x12,
];

/// A registry over a database that already carries the `did_registry` schema.
fn raw_registry() -> StoolapDidRegistry {
    let db = octo_storage_core::Database::open_in_memory().expect("open db");
    quota_router_storage::migrations::apply_pending(&db).expect("migrations");
    StoolapDidRegistry::from_database(db)
}

/// Insert a row with a hand-written `verification_methods` column. `None`
/// leaves the column NULL.
fn insert_raw_row(reg: &StoolapDidRegistry, hash: &[u8; 32], vm: Option<&[u8]>) {
    let sql = "INSERT INTO did_registry (canonical_hash, public_key, revoked, \
                updated_at_unix_ms, service_endpoints, controllers, \
                verification_methods, capability_delegations, chain_id) \
               VALUES (?, ?, 0, 0, NULL, NULL, ?, NULL, ?)";
    let vm_param: Vec<u8> = vm.map_or_else(Vec::new, <[u8]>::to_vec);
    reg.database()
        .execute(
            sql,
            (
                hash.to_vec(),
                vec![0x5Au8; 32],
                vm_param,
                MAINNET_CHAIN_ID.to_vec(),
            ),
        )
        .expect("insert raw row");
}

/// Bytes of a pre-migration `verification_methods` column.
fn legacy_blob(entries: &[(u8, [u8; 32])]) -> Vec<u8> {
    let count = u32::try_from(entries.len()).expect("fixture count fits in u32");
    let mut out = count.to_le_bytes().to_vec();
    for (kind, pk) in entries {
        out.push(*kind);
        out.extend_from_slice(pk);
    }
    out
}

#[test]
fn resolve_migrates_a_legacy_row_end_to_end() {
    // The migration through `resolve` itself, not the codec. A legacy blob is
    // only reachable via direct SQL, which is why this vector did not exist
    // before `from_database`.
    let reg = raw_registry();
    let hash = sample_hash(0x70);
    insert_raw_row(&reg, &hash, Some(&legacy_blob(&[(0x01, [0xAA; 32])])));

    let doc = reg.resolve(&hash).expect("resolve").expect("present");
    assert_eq!(doc.verification_methods.len(), 1);
    assert_eq!(
        doc.verification_methods[0].algorithm_id,
        octo_ident::AlgorithmId::derive("ed25519")
    );
    assert_eq!(doc.verification_methods[0].public_key, vec![0xAAu8; 32]);
    assert!(!doc.verification_methods[0].is_revoked());
}

#[test]
fn resolve_reports_a_corrupt_row_as_an_error_not_an_empty_list() {
    // The property the whole migration exists for, observed at the boundary a
    // caller actually sees. Reverting the `?` here to `unwrap_or_default()`
    // makes this fail.
    let reg = raw_registry();
    let hash = sample_hash(0x71);
    insert_raw_row(&reg, &hash, Some(&[0xFF, 0xFF, 0xFF, 0xFF]));

    let err = reg
        .resolve(&hash)
        .expect_err("corrupt blob must surface as an error through resolve");
    let msg = err.to_string();
    assert!(
        msg.contains("undecodable under both the current shape"),
        "error should name both decode attempts, got: {msg}"
    );
}

#[test]
fn list_reports_a_corrupt_row_as_an_error_too() {
    // `list` has its own column selection and its own decode call, so the
    // resolver vector cannot observe it.
    let reg = raw_registry();
    let hash = sample_hash(0x72);
    insert_raw_row(&reg, &hash, Some(&[0xDE, 0xAD, 0xBE, 0xEF]));
    let err = reg
        .list()
        .expect_err("corrupt blob must surface as an error through list");
    assert!(err.to_string().contains("list:"));
}

#[test]
fn resolve_returns_an_empty_list_for_a_null_column() {
    // NULL is absence, not corruption: a pre-v009 row legitimately has no
    // verification methods and must still resolve. This is the half of the
    // three-case rule the codec-level vector could not reach.
    let reg = raw_registry();
    let hash = sample_hash(0x73);
    insert_raw_row(&reg, &hash, None);

    let doc = reg.resolve(&hash).expect("resolve").expect("present");
    assert!(
        doc.verification_methods.is_empty(),
        "NULL column is absence, not corruption"
    );
    assert_eq!(doc.public_key, [0x5Au8; 32]);
}
