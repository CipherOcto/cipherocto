//! Mesh peer-table substrate — RFC-0011-f.
//!
//! This module is the public surface for the peer-table substrate. The
//! concrete logic lives in four focused sibling modules (declared in
//! `lib.rs`):
//!
//! - `crate::trust_level`: `TrustLevel` typed-discriminator + canonical
//!   UUID table (RFC-0011-f §Trust Level Canonical UUIDs).
//! - `crate::endpoint`: `EndpointUri` allowlist-validated URI wrapper +
//!   `ALLOWED_ENDPOINT_SCHEMES` (RFC-0011-f §Peer Summary Shape).
//! - `crate::peer_record`: `PeerRecord` (on-disk), `PeerSummary`
//!   (operator-facing), `PeerFilter` (query).
//! - `crate::peer_table`: atomic TOML persistence
//!   (`$OCTO_HOME/mesh/peers.toml`, 0700 perms, write-to-tmp + fsync +
//!   rename) + substrate functions `add_peer`, `remove_peer`,
//!   `list_peers`.
//!
//! ## TrustLevel — typed-discriminator, not central enum
//!
//! Per [[cipherocto-design-principles]] "Extension over enumeration (no
//! central enums)", `TrustLevel` is a `String` newtype wrapping a
//! canonical UUID discriminator (RFC-0871 typed-discriminator pattern).
//! New trust signals extend the canonical UUID table without substrate
//! or CLI enum edits; old code fails-closed on unknown discriminators
//! per [[cipherocto-design-principles]] "Open/Closed".

// Re-export the public surface so existing `crate::peer::TrustLevel`
// import paths keep working unchanged across the Wave 1.5 split. The
// source modules are declared in `lib.rs`.
pub use crate::endpoint::{EndpointUri, ALLOWED_ENDPOINT_SCHEMES};
pub use crate::peer_record::{PeerFilter, PeerRecord, PeerSummary};
pub use crate::peer_table::{add_peer, list_peers, peer_table_path, remove_peer};
pub use crate::trust_level::{trust_level_uuids, TrustLevel};

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::DidCodec;

    #[test]
    fn endpoint_uri_accepts_allowlisted_schemes() {
        for s in [
            "tcp://1.2.3.4:9000",
            "quic://host.example:4433",
            "bluetooth://AA:BB:CC:DD:EE:FF",
        ] {
            let uri = EndpointUri::parse(s).expect("allowlisted scheme must parse");
            assert_eq!(uri.as_str(), s);
        }
    }

    #[test]
    fn endpoint_uri_rejects_disallowed_schemes() {
        for s in ["file:///etc/passwd", "http://x", "ws://x", "ftp://x"] {
            let err = EndpointUri::parse(s).expect_err("disallowed scheme must fail");
            assert!(
                matches!(err, crate::error::MeshError::InvalidEndpointScheme { .. }),
                "{err:?}"
            );
        }
    }

    #[test]
    fn endpoint_uri_rejects_empty_payload() {
        let err = EndpointUri::parse("tcp://").expect_err("empty payload must fail");
        assert!(matches!(
            err,
            crate::error::MeshError::InvalidEndpointScheme { .. }
        ));
    }

    #[test]
    fn peer_filter_empty_includes_all() {
        let f = PeerFilter::default();
        let p = PeerSummary {
            peer_did: "did:octo:zabc".to_string(),
            endpoint: EndpointUri("tcp://1.2.3.4:1".to_string()),
            trust_level: TrustLevel::untrusted(),
            last_seen_unix: 0,
            capabilities: vec![],
        };
        assert!(f.matches(&p));
    }

    #[test]
    fn peer_filter_trust_levels_is_inclusive_set() {
        let f = PeerFilter {
            trust_levels: vec![
                TrustLevel::new(trust_level_uuids::TRUSTED),
                TrustLevel::new(trust_level_uuids::VERIFIED),
            ],
        };
        let mut p = PeerSummary {
            peer_did: "did:octo:zabc".to_string(),
            endpoint: EndpointUri("tcp://1.2.3.4:1".to_string()),
            trust_level: TrustLevel::new(trust_level_uuids::TRUSTED),
            last_seen_unix: 0,
            capabilities: vec![],
        };
        assert!(f.matches(&p));
        p.trust_level = TrustLevel::new(trust_level_uuids::VERIFIED);
        assert!(f.matches(&p));
        p.trust_level = TrustLevel::new(trust_level_uuids::UNTRUSTED);
        assert!(!f.matches(&p));
    }

    #[test]
    fn validate_peer_did_accepts_canonical_form() {
        // RFC-0010 canonical form is `did:octo:z<base58btc of 32 bytes>`.
        // Use the substrate `CanonicalCodec::mint` + `raw_to_wire` to
        // produce a guaranteed-valid canonical wire form (avoid hand-
        // crafting base58btc — the decoder is strict on the alphabet).
        let raw = octo_ident::CanonicalCodec::mint(&[1u8; 32]);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        crate::peer_table::validate_peer_did(wire.as_str()).expect("canonical z-form must parse");
    }

    #[test]
    fn validate_peer_did_rejects_legacy_form() {
        // Legacy `did:octo:b<base32 of 52 bytes>` (62 chars after prefix).
        let payload = "b".to_string() + &"a".repeat(62);
        let did = format!("did:octo:{payload}");
        let err = crate::peer_table::validate_peer_did(&did).expect_err("legacy form must fail");
        assert!(matches!(err, crate::error::MeshError::InvalidDidShape(_)));
    }
}
