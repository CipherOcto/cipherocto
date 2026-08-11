//! `IDENTITY_RESOLVE_WITH_CHAIN` handler (RFC-0010 v1.4 §ChainId
//! Namespace Extension, mission `0010-f2-multi-chain-routing`).
//!
//! Receives: `(query: String, chain_id: String)` — canonical DID wire
//! form + RFC-0010 v1.4 `ChainId` literal.
//! Returns: `<canonical_did: String, public_key: [u8; 32]>` — same
//! shape as `IDENTITY_RESOLVE` (canonical DID + resolved public_key).
//!
//! Distinct from `IDENTITY_RESOLVE` (always resolves on mainnet
//! chain) and `IDENTITY_RESOLVE_CHAIN` (chain-of-resolvers, walks
//! `Vec<ResolverHop>` against multiple resolver-nodes). This
//! handler routes a single resolve request to a specific chain
//! namespace on a multi-chain deployment.
//!
//! The handler calls `DidRegistry::resolve_in_chain` which is an
//! additive trait method (mission `0010-f2-registry-namespacing`,
//! commit `a7efaabb`). The default impl forwards to `resolve` for
//! back-compat; production storage overrides with chain-aware SQL.

use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::ChainId;
use octo_ident::DidCodec;
use octo_ident::DidRegistry;

use super::{HandlerOutput, IdentityResolveError};

/// Request payload for `IDENTITY_RESOLVE_WITH_CHAIN`.
///
/// Wire form: borsh `(query, chain_id)`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveWithChainRequest {
    /// Canonical DID wire form (`did:octo:z<base58btc>`).
    pub query: String,
    /// RFC-0010 v1.4 `ChainId` literal (e.g. `"cipherocto-mainnet"`,
    /// `"partner-mainnet"`).
    pub chain_id: String,
}

impl ResolveWithChainRequest {
    /// Decode from borsh wire form.
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, IdentityResolveError> {
        borsh::from_slice(bytes).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }

    /// Encode to borsh wire form.
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, IdentityResolveError> {
        borsh::to_vec(self).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }
}

/// Response payload for `IDENTITY_RESOLVE_WITH_CHAIN`.
///
/// Wire form: borsh `(canonical_did, public_key)` — same shape as
/// `IDENTITY_RESOLVE`.
pub type ResolveWithChainResponse = super::ResolveResponse;

/// `IDENTITY_RESOLVE_WITH_CHAIN` handler.
pub struct ResolveWithChainHandler {
    registry: Arc<dyn DidRegistry>,
}

impl ResolveWithChainHandler {
    /// Construct a new `ResolveWithChainHandler` bound to the supplied
    /// registry.
    #[must_use]
    pub fn new(registry: Arc<dyn DidRegistry>) -> Self {
        Self { registry }
    }

    /// Resolve the DID query on the specified chain namespace.
    ///
    /// 1. Validates `query` is a canonical DID shape via
    ///    `octo_ident::CanonicalCodec::parse(s, false)`.
    /// 2. Validates `chain_id` literal shape via `ChainId::new` —
    ///    fail-closed on malformed (no implicit default to mainnet;
    ///    callers must opt in explicitly).
    /// 3. Decodes the canonical wire form → `RawDid`; uses the
    ///    leading 32-byte `hash` as the registry lookup key.
    /// 4. Calls `registry.resolve_in_chain(&chain_id, &raw.hash)`.
    ///    Returns `Ok(None)` for unknown AND revoked DIDs on the
    ///    given chain (fail-closed; the response surfaces
    ///    `Storage("unknown DID")` to the caller).
    /// 5. Returns the resolved `public_key` in `ResolveWithChainResponse`.
    ///
    /// # Errors
    /// - `IdentityResolveError::InvalidDid` if `query` is not a
    ///   canonical DID shape.
    /// - `IdentityResolveError::InvalidChainId` if `chain_id` fails
    ///   RFC-0010 v1.4 validation (empty, > 64 chars, control chars).
    /// - `IdentityResolveError::Storage` if the underlying registry
    ///   call fails.
    pub fn handle(
        &self,
        req: &ResolveWithChainRequest,
    ) -> Result<HandlerOutput, IdentityResolveError> {
        // 1. Validate canonical DID shape; reject legacy bare form.
        let wire = octo_ident::CanonicalCodec::parse(&req.query, false)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 2. Validate chain_id literal shape (fail-closed on malformed).
        let chain_id = ChainId::new(req.chain_id.clone())
            .map_err(|e| IdentityResolveError::InvalidChainId(e.to_string()))?;

        // 3. Decode canonical wire form → RawDid for lookup key.
        let raw = octo_ident::CanonicalCodec::wire_to_raw(&wire)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 4. Resolve on the specified chain namespace.
        let doc = self
            .registry
            .resolve_in_chain(&chain_id, &raw.hash)
            .map_err(|e| IdentityResolveError::Storage(format!("resolve_in_chain: {e}")))?
            .ok_or_else(|| IdentityResolveError::Storage("unknown DID".to_string()))?;

        let resp = ResolveWithChainResponse {
            canonical_did: wire.as_str().to_owned(),
            public_key: doc.public_key,
        };
        Ok(HandlerOutput::response(
            borsh::to_vec(&resp).map_err(|e| IdentityResolveError::Serialization(e.to_string()))?,
            octo_protocol::payload_kind::IDENTITY_RESOLVE_WITH_CHAIN,
        ))
    }
}
