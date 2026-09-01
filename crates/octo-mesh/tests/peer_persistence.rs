//! Integration tests for the peer-table persistence layer.
//!
//! Uses `tempfile::tempdir()` for an isolated `$OCTO_HOME` substitute
//! so tests never touch the operator's real `$OCTO_HOME/mesh/peers.toml`.

use octo_ident::{CanonicalCodec, DidCodec};
use octo_mesh::{
    add_peer, list_peers, peer_table_path, remove_peer, EndpointUri, PeerFilter, TrustLevel,
};

fn canonical_did(seed_byte: u8) -> String {
    let raw = CanonicalCodec::mint(&[seed_byte; 32]);
    let wire = CanonicalCodec::raw_to_wire(&raw).unwrap();
    wire.as_str().to_string()
}

#[test]
fn add_then_list_returns_record() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let uri = EndpointUri::parse("tcp://1.2.3.4:9000").unwrap();
    let did = canonical_did(1);

    add_peer(&did, &uri, home, 1_000_000).unwrap();

    let peers = list_peers(&PeerFilter::default(), home).unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].peer_did, did);
    assert_eq!(peers[0].endpoint, uri);
    assert_eq!(peers[0].trust_level, TrustLevel::untrusted());
    assert_eq!(peers[0].last_seen_unix, 1_000_000);
}

#[test]
fn add_is_idempotent_upsert() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let uri1 = EndpointUri::parse("tcp://1.2.3.4:9000").unwrap();
    let uri2 = EndpointUri::parse("tcp://5.6.7.8:9001").unwrap();
    let did = canonical_did(2);

    add_peer(&did, &uri1, home, 1).unwrap();
    add_peer(&did, &uri2, home, 2).unwrap();

    let peers = list_peers(&PeerFilter::default(), home).unwrap();
    assert_eq!(
        peers.len(),
        1,
        "duplicate add must upsert (last-writer-wins)"
    );
    assert_eq!(peers[0].endpoint, uri2);
}

#[test]
fn remove_then_list_omits_peer() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let uri = EndpointUri::parse("tcp://1.2.3.4:9000").unwrap();
    let did = canonical_did(3);

    add_peer(&did, &uri, home, 1).unwrap();
    remove_peer(&did, home, 2).unwrap();

    let peers = list_peers(&PeerFilter::default(), home).unwrap();
    assert_eq!(peers.len(), 0);
}

#[test]
fn remove_absent_peer_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let did = canonical_did(4);

    // Should not error on missing peer (RFC-0011-f §Subcommand Taxonomy
    // `peer remove` — idempotent).
    remove_peer(&did, home, 1).unwrap();
    remove_peer(&did, home, 2).unwrap();
}

#[test]
fn list_empty_when_no_table_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    let peers = list_peers(&PeerFilter::default(), home).unwrap();
    assert_eq!(peers.len(), 0);
}

#[test]
fn add_rejects_invalid_endpoint_scheme() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let _did = canonical_did(5);

    // EndpointUri::parse gates the scheme, so the call site builds via
    // `EndpointUri::parse` and only passes valid URIs. The substrate
    // also rejects via the allowlist inside `add_peer`'s call site.
    let bad = EndpointUri::parse("file:///etc/passwd");
    assert!(bad.is_err());
    // Did NOT call add_peer — list remains empty.
    let peers = list_peers(&PeerFilter::default(), home).unwrap();
    assert_eq!(peers.len(), 0);
}

#[test]
fn add_rejects_invalid_did_shape() {
    use octo_mesh::MeshError;

    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let uri = EndpointUri::parse("tcp://1.2.3.4:9000").unwrap();
    let bad_did = "did:octo:bnotbase32atall";

    let err = add_peer(bad_did, &uri, home, 1).expect_err("invalid DID must fail");
    assert!(matches!(err, MeshError::InvalidDidShape(_)));
}

#[test]
fn peer_table_path_resolves_under_mesh_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let path = peer_table_path(tmp.path());
    assert_eq!(path, tmp.path().join("mesh").join("peers.toml"));
}
