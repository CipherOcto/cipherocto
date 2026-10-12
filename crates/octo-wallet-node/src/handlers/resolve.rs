//! `WALLET_RESOLVE_DID` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<query: String>` — a DID string to resolve.
//! Returns: `<canonical_did: String, public_key: [u8; 32]>`.
//!
//! ## This handler cannot resolve an arbitrary DID, and says so
//!
//! A `did:octo` DID is a one-way hash of its subject's public key
//! (RFC-0010 §Data Structures: `BLAKE3-256(binding_domain || subject_pubkey)`).
//! Recovering the key from the DID is therefore not possible by computation —
//! it needs a registry, which this node does not have. RFC-0871 §Implementation
//! Phases Phase 2 lists an identity resolver node as NEW and mission-TBD, and
//! RFC-0957-A1 supplies the registry for capability holders specifically.
//!
//! The handler used to answer anyway: it validated the query's shape, discarded
//! the parse, and returned **its own** public key labelled with the *queried*
//! DID. Every DID resolved to the node's key. A consumer that resolved a peer's
//! DID and verified a signature against the returned key would have accepted a
//! signature from this node — which is a signing node, serving
//! `WALLET_SIGN_ED25519`.
//!
//! It now answers only what it can actually resolve: **its own DID**, checked
//! by re-deriving that DID from its own key and comparing. Any other query is
//! refused. That is a narrower capability than a resolver node provides, and it
//! is an honest one.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::DidCodec;
use octo_protocol::ProtocolError;
use octo_wallet::identity::IdentityKey;

use super::{did_error_to_protocol, HandlerOutput};

/// Request payload for `WALLET_RESOLVE_DID`.
///
/// Wire form: borsh (`query`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveDIDRequest(pub String);

impl ResolveDIDRequest {
    /// Decode from borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, ProtocolError> {
        borsh::from_slice(bytes).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }

    /// Encode to borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, ProtocolError> {
        borsh::to_vec(&self.0).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }
}

/// `WALLET_RESOLVE_DID` handler.
///
/// Resolves **only this node's own DID**. A general lookup needs a DID
/// registry, which does not exist yet — see the module docs for why the
/// missing backend is a refusal rather than a guess.
pub struct ResolveDIDHandler<'a> {
    identity: &'a IdentityKey,
}

impl<'a> ResolveDIDHandler<'a> {
    /// Construct a new `ResolveDIDHandler` bound to the given identity
    /// key. The bound identity is used for the response envelope's
    /// `from_did` (the resolver's identity, not the requester's).
    #[must_use]
    pub const fn new(identity: &'a IdentityKey) -> Self {
        Self { identity }
    }

    /// Resolve a DID query.
    ///
    /// Answers only for this node's own DID, proven by re-deriving the DID from
    /// its own key and comparing. Every other query is refused.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `query` is not a canonical DID
    /// shape, and `ProtocolError::AuthorizationFailed` if the query is a valid
    /// DID this node cannot resolve — it has no registry to resolve it from.
    pub fn handle(&self, req: &ResolveDIDRequest) -> Result<HandlerOutput, ProtocolError> {
        // Validate canonical DID shape; reject legacy bare form.
        octo_ident::CanonicalCodec::parse(&req.0, false).map_err(did_error_to_protocol)?;

        // The response encodes a fixed 32-byte key slot on the wire. That is a
        // protocol surface, not a verification site, so it is converted here
        // and refused for any other length rather than truncated.
        let public_key = self.identity.public_key_bytes_array().ok_or_else(|| {
            ProtocolError::AuthorizationFailed(
                "resolved identity key is not 32 bytes; the response schema is fixed-width"
                    .to_owned(),
            )
        })?;

        // Derive this node's own DID from its own key and compare. Anything
        // else is a DID this node has no way to resolve, and answering with a
        // key that does not hash to it would be worse than refusing.
        let own_did = octo_ident::CanonicalCodec::mint(&public_key);
        let own_wire = <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&own_did)
            .map_err(did_error_to_protocol)?;
        if own_wire.as_str() != req.0 {
            return Err(ProtocolError::AuthorizationFailed(format!(
                "cannot resolve {}: this node has no DID registry and resolves only its own \
                 DID, which is {}",
                req.0,
                own_wire.as_str()
            )));
        }

        let response = ResolveDIDResponse {
            canonical_did: own_wire.as_str().to_owned(),
            public_key,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::WALLET_RESOLVE_DID)
                .with_note(format!("resolved own DID {}", response.canonical_did)),
        )
    }
}

/// Response payload for `WALLET_RESOLVE_DID`.
///
/// Wire form: borsh (`canonical_did`, `public_key`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveDIDResponse {
    /// Canonical DID wire form (post-parse).
    pub canonical_did: String,
    /// 32-byte verifying key (placeholder for storage-pubkey form).
    pub public_key: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        IdentityKey::from_seed(seed)
    }

    /// A different identity, so its DID is a different DID.
    fn other_identity() -> IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(0x80);
        }
        IdentityKey::from_seed(seed)
    }

    /// The canonical `did:octo` wire form derived from an identity's key.
    fn did_for(id: &IdentityKey) -> String {
        let pk = id.public_key_bytes_array().expect("32-byte key");
        let raw = octo_ident::CanonicalCodec::mint(&pk);
        <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&raw)
            .expect("wire form")
            .as_str()
            .to_owned()
    }

    fn response_from(out: &HandlerOutput) -> ResolveDIDResponse {
        let payload = out
            .response_payload
            .as_ref()
            .expect("response payload present");
        borsh::from_slice(payload).expect("decode ResolveDIDResponse")
    }

    #[test]
    fn resolve_request_borsh_round_trip() {
        let req = ResolveDIDRequest(did_for(&sample_identity()));
        let bytes = req.to_borsh().unwrap();
        let back = ResolveDIDRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let id = sample_identity();
        let handler = ResolveDIDHandler::new(&id);
        let req = ResolveDIDRequest("did:octo:bad".into());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    /// The one DID this node can answer for: its own, proven by the key
    /// hashing back to the queried DID.
    #[test]
    fn the_nodes_own_did_resolves_to_its_own_key() {
        let id = sample_identity();
        let handler = ResolveDIDHandler::new(&id);
        let own = did_for(&id);
        let resp = response_from(&handler.handle(&ResolveDIDRequest(own.clone())).unwrap());
        assert_eq!(resp.canonical_did, own);
        assert_eq!(resp.public_key, id.public_key_bytes());
    }

    /// The substitution test. The handler used to answer **every** query with
    /// its own key, so resolving a peer's DID returned a key that does not
    /// hash to it. A consumer verifying a signature against that key would have
    /// accepted a signature from this node.
    #[test]
    fn resolving_another_identitys_did_is_refused_rather_than_answered_with_our_key() {
        let id = sample_identity();
        let handler = ResolveDIDHandler::new(&id);
        let their_did = did_for(&other_identity());
        let err = handler
            .handle(&ResolveDIDRequest(their_did.clone()))
            .unwrap_err();
        assert!(
            matches!(err, ProtocolError::AuthorizationFailed(_)),
            "an unresolvable DID must be refused, got {err:?}"
        );
        // And the refusal names the DID rather than silently succeeding.
        let ProtocolError::AuthorizationFailed(message) = &err else {
            panic!("expected an authorization failure, got {err:?}");
        };
        assert!(
            message.contains(&their_did),
            "the refusal must name the DID it could not resolve, got: {message}"
        );
    }

    /// Whatever comes back must be self-consistent: the returned key has to
    /// hash to the returned DID. This is the property the old handler broke,
    /// stated as an invariant over every answer rather than one case.
    #[test]
    fn any_answer_pairs_a_key_with_the_did_that_key_derives() {
        let id = sample_identity();
        let handler = ResolveDIDHandler::new(&id);
        let queries = [
            did_for(&id),
            did_for(&other_identity()),
            did_for(&sample_identity()),
        ];
        for q in queries {
            match handler.handle(&ResolveDIDRequest(q.clone())) {
                Ok(out) => {
                    let resp = response_from(&out);
                    assert_eq!(
                        resp.canonical_did, q,
                        "a successful resolution must echo the queried DID"
                    );
                    assert_eq!(
                        did_for_from_key(resp.public_key),
                        q,
                        "the returned key must derive the returned DID"
                    );
                }
                Err(_) => { /* refusal is a valid outcome */ }
            }
        }
    }

    fn did_for_from_key(pk: [u8; 32]) -> String {
        let raw = octo_ident::CanonicalCodec::mint(&pk);
        <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&raw)
            .expect("wire form")
            .as_str()
            .to_owned()
    }
}
