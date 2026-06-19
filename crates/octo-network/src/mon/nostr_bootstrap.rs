//! Nostr-based bootstrap mode (mission 0851p-a-nostr-mode-d).
//!
//! ## Status
//!
//! Per the mission text, this is a **Future** mission (not
//! post-launch) because the Nostr ecosystem is still maturing.
//! This module provides the data model + parsing for the DOT
//! capability claim (kind 30078 with `d` tag = `dot-capability`),
//! plus a stub bootstrap adapter that integrates with the
//! `BootstrapMode` enum from `bootstrap.rs`.
//!
//! ## Why a stub?
//!
//! The full mission calls for `nostr-sdk` integration (NIP-05
//! resolution, kind 3 contact list fetch, kind 30078 verification).
//! That requires a non-trivial async runtime setup and pinned
//! `nostr-sdk` version. We defer the full implementation to a
//! follow-up mission and ship the data model + verification
//! helpers now so that downstream code can be written against
//! the stable types.

use serde::{Deserialize, Serialize};

/// The NIP-05 identifier for a Nostr user (e.g.,
/// `user@example.com`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nip05Identifier {
    pub user: String,
    pub domain: String,
}

impl Nip05Identifier {
    /// Parse a NIP-05 identifier like `user@domain`.
    ///
    /// Rejects identifiers where:
    /// - either part is empty
    /// - the user is longer than 64 chars
    /// - the user contains anything other than `[a-zA-Z0-9._-]`
    ///   (the strict NIP-05 local-part charset; defends against
    ///   URL-injection into the resolution URL)
    /// - the domain is longer than 253 chars (DNS host max)
    /// - the domain contains anything other than `[a-zA-Z0-9.-]`
    ///   (no scheme/path/whitespace/control chars)
    pub fn parse(s: &str) -> Result<Self, Nip05Error> {
        let mut parts = s.splitn(2, '@');
        let user = parts
            .next()
            .ok_or(Nip05Error::InvalidIdentifier)?
            .to_string();
        let domain = parts
            .next()
            .ok_or(Nip05Error::InvalidIdentifier)?
            .to_string();
        if user.is_empty() || domain.is_empty() {
            return Err(Nip05Error::InvalidIdentifier);
        }
        // user must be a simple local-part: strict whitelist
        // [a-zA-Z0-9._-] only. No path separators, whitespace,
        // control chars, or URL-special chars.
        if user.len() > 64
            || !user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
        {
            return Err(Nip05Error::InvalidIdentifier);
        }
        // domain must be a hostname: strict whitelist
        // [a-zA-Z0-9.-] only. No scheme/path/whitespace.
        if domain.len() > 253
            || !domain
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err(Nip05Error::InvalidIdentifier);
        }
        Ok(Self { user, domain })
    }

    /// Returns the `https://domain/.well-known/nostr.json?name=user`
    /// URL.
    pub fn resolution_url(&self) -> String {
        format!(
            "https://{}/.well-known/nostr.json?name={}",
            self.domain, self.user
        )
    }
}

/// A DOT capability claim (Nostr event kind 30078 with `d` tag =
/// `dot-capability`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DotCapabilityClaim {
    /// The Nostr pubkey (hex, 64 chars).
    pub pubkey: String,
    /// The `d` tag value (must be `dot-capability`).
    pub d_tag: String,
    /// The libp2p peer_id this Nostr pubkey controls.
    pub peer_id: String,
    /// Optional: where to fetch the bootstrap list.
    #[serde(default)]
    pub bootstrap_list_url: Option<String>,
    /// The event's signature (Schnorr over secp256k1).
    pub signature: Vec<u8>,
    /// Unix epoch seconds when the event was created.
    pub created_at: u64,
}

impl DotCapabilityClaim {
    /// Returns true if the claim has the correct `d` tag.
    pub fn has_dot_capability_tag(&self) -> bool {
        self.d_tag == "dot-capability"
    }
}

/// Errors for the Nostr adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Nip05Error {
    InvalidIdentifier,
    NetworkError(String),
    NotFound,
    /// The Nostr pubkey is malformed (must be 64 hex chars).
    InvalidPubkey,
    /// The DOT capability claim is missing or has an invalid
    /// signature.
    InvalidCapabilityClaim,
}

/// A Nostr-based bootstrap adapter stub.
///
/// The full implementation (NIP-05 resolution, kind 3 contact
/// list fetch, kind 30078 verification) is deferred. This
/// stub provides the data model + verification helpers so
/// downstream code can be written against the stable types.
#[derive(Clone, Debug, Default)]
pub struct NostrBootstrapAdapter {
    /// The operator's NIP-05 identifier.
    pub identifier: Option<Nip05Identifier>,
    /// The resolved Nostr pubkey (hex).
    pub pubkey: Option<String>,
    /// The DOT capability claims for the operator's contacts.
    pub capabilities: Vec<DotCapabilityClaim>,
}

impl NostrBootstrapAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the operator's NIP-05 identifier.
    pub fn set_identifier(&mut self, identifier: Nip05Identifier) {
        self.identifier = Some(identifier);
    }

    /// Verify a DOT capability claim.
    ///
    /// The full signature verification (Schnorr over secp256k1)
    /// is deferred. This stub checks:
    /// 1. The `d_tag` is `dot-capability`
    /// 2. The `pubkey` is 64 hex chars
    /// 3. The `peer_id` is non-empty
    pub fn verify_capability(&self, claim: &DotCapabilityClaim) -> Result<(), Nip05Error> {
        if !claim.has_dot_capability_tag() {
            return Err(Nip05Error::InvalidCapabilityClaim);
        }
        if claim.pubkey.len() != 64 || !claim.pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Nip05Error::InvalidPubkey);
        }
        if claim.peer_id.is_empty() {
            return Err(Nip05Error::InvalidCapabilityClaim);
        }
        // Signature verification deferred to a follow-up mission
        // (requires pinned `nostr-sdk` version with Schnorr
        // verification).
        Ok(())
    }

    /// Add a verified capability claim to the adapter.
    pub fn add_capability(&mut self, claim: DotCapabilityClaim) -> Result<(), Nip05Error> {
        self.verify_capability(&claim)?;
        self.capabilities.push(claim);
        Ok(())
    }

    /// Returns the verified bootstrap peer_ids.
    pub fn bootstrap_peer_ids(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .map(|c| c.peer_id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nip05_identifier_parse() {
        let id = Nip05Identifier::parse("alice@example.com").unwrap();
        assert_eq!(id.user, "alice");
        assert_eq!(id.domain, "example.com");
        assert_eq!(
            id.resolution_url(),
            "https://example.com/.well-known/nostr.json?name=alice"
        );
    }

    #[test]
    fn nip05_identifier_rejects_invalid() {
        assert!(Nip05Identifier::parse("not-an-id").is_err());
        assert!(Nip05Identifier::parse("").is_err());
        assert!(Nip05Identifier::parse("@nodomain.com").is_err());
        assert!(Nip05Identifier::parse("noat.com").is_err());
    }

    #[test]
    fn nip05_identifier_rejects_path_traversal() {
        // Path separators and control characters in either
        // part must be rejected to prevent URL-injection attacks
        // against the resolution URL.
        assert!(Nip05Identifier::parse("alice@../etc/passwd").is_err());
        assert!(Nip05Identifier::parse("../etc@example.com").is_err());
        assert!(Nip05Identifier::parse("alice@evil.com/foo").is_err());
        assert!(Nip05Identifier::parse("alice@evil.com\\foo").is_err());
        assert!(Nip05Identifier::parse("alice bob@example.com").is_err());
        assert!(Nip05Identifier::parse("alice\0@example.com").is_err());
        assert!(Nip05Identifier::parse("alice@example.com\n").is_err());
    }

    #[test]
    fn nip05_identifier_rejects_oversize_user() {
        // user > 64 chars must be rejected.
        let long = "a".repeat(65);
        assert!(Nip05Identifier::parse(&format!("{long}@example.com")).is_err());
    }

    #[test]
    fn nip05_identifier_rejects_url_special_chars() {
        // The strict whitelist ([a-zA-Z0-9._-] for user,
        // [a-zA-Z0-9.-] for domain) must reject any URL-special
        // character that could inject a second query parameter,
        // fragment, or path component into the resolution URL.
        assert!(Nip05Identifier::parse("alice?@example.com").is_err());
        assert!(Nip05Identifier::parse("alice#@example.com").is_err());
        assert!(Nip05Identifier::parse("alice&@example.com").is_err());
        assert!(Nip05Identifier::parse("alice=@example.com").is_err());
        assert!(Nip05Identifier::parse("alice%@example.com").is_err());
        assert!(Nip05Identifier::parse("alice+@example.com").is_err());
        assert!(Nip05Identifier::parse("alice,@example.com").is_err());
        assert!(Nip05Identifier::parse("alice@example.com:80").is_err());
        assert!(Nip05Identifier::parse("alice@example.com/path").is_err());
    }

    #[test]
    fn nip05_identifier_rejects_oversize_domain() {
        // domain > 253 chars (DNS max) must be rejected.
        let long_domain = format!("{}.com", "a".repeat(254));
        assert!(Nip05Identifier::parse(&format!("alice@{long_domain}")).is_err());
    }

    #[test]
    fn dot_capability_tag_check() {
        let claim = DotCapabilityClaim {
            pubkey: "a".repeat(64),
            d_tag: "dot-capability".into(),
            peer_id: "peer-1".into(),
            bootstrap_list_url: None,
            signature: vec![],
            created_at: 1700000000,
        };
        assert!(claim.has_dot_capability_tag());
    }

    #[test]
    fn verify_capability_rejects_wrong_d_tag() {
        let claim = DotCapabilityClaim {
            pubkey: "a".repeat(64),
            d_tag: "other".into(),
            peer_id: "peer-1".into(),
            bootstrap_list_url: None,
            signature: vec![],
            created_at: 0,
        };
        let mut adapter = NostrBootstrapAdapter::new();
        let result = adapter.add_capability(claim);
        assert!(matches!(result, Err(Nip05Error::InvalidCapabilityClaim)));
    }

    #[test]
    fn verify_capability_rejects_invalid_pubkey() {
        let claim = DotCapabilityClaim {
            pubkey: "short".into(),
            d_tag: "dot-capability".into(),
            peer_id: "peer-1".into(),
            bootstrap_list_url: None,
            signature: vec![],
            created_at: 0,
        };
        let mut adapter = NostrBootstrapAdapter::new();
        let result = adapter.add_capability(claim);
        assert!(matches!(result, Err(Nip05Error::InvalidPubkey)));
    }

    #[test]
    fn verify_capability_rejects_empty_peer_id() {
        let claim = DotCapabilityClaim {
            pubkey: "a".repeat(64),
            d_tag: "dot-capability".into(),
            peer_id: "".into(),
            bootstrap_list_url: None,
            signature: vec![],
            created_at: 0,
        };
        let mut adapter = NostrBootstrapAdapter::new();
        let result = adapter.add_capability(claim);
        assert!(matches!(result, Err(Nip05Error::InvalidCapabilityClaim)));
    }

    #[test]
    fn valid_capability_accepted() {
        let claim = DotCapabilityClaim {
            pubkey: "0123456789abcdef".repeat(4), // 64 hex
            d_tag: "dot-capability".into(),
            peer_id: "12D3KooP...".into(),
            bootstrap_list_url: Some("https://seeds.example.com/list.json".into()),
            signature: vec![1, 2, 3],
            created_at: 1700000000,
        };
        let mut adapter = NostrBootstrapAdapter::new();
        adapter.add_capability(claim.clone()).unwrap();
        assert_eq!(
            adapter.bootstrap_peer_ids(),
            vec!["12D3KooP...".to_string()]
        );
    }

    #[test]
    fn nip05_identifier_serde_roundtrip() {
        let id = Nip05Identifier::parse("alice@example.com").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        let back: Nip05Identifier = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
