//! `IDENTITY_REGISTER` + `IDENTITY_REVOKE` handlers (RFC-0862 v1.3
//! §DidWriteCoordinator, mission 0871e-f7-impl-resolver-mediation).
//!
//! Receives:
//! - `IDENTITY_REGISTER`: `(canonical_did: String, document: DidDocument)`
//! - `IDENTITY_REVOKE`:   `(canonical_did: String)`
//!
//! Mediation flow (per RFC-0862 v1.3 R11 H2 + R13 M5 + mission
//! `0871e-f7-cross-instance-did-coordination`):
//!
//! 1. Validate `canonical_did` is a canonical wire form via
//!    `octo_ident::CanonicalCodec::parse(s, false)`.
//! 2. Decode canonical wire form → `RawDid`; use the leading 32-byte
//!    `hash` as the registry lookup key.
//! 3. Consult the injected `Arc<dyn DidWriteCoordinator>`. If no
//!    coordinator is configured, refuse with
//!    `IdentityResolveError::CoordinatorUnavailable` (fail-closed per
//!    RFC-0862 v1.3 R12 — the same fail-closed default that the trait's
//!    `submit_register_local_fallback` enforces).
//! 4. Coordinator returns OK → delegate to local `DidRegistry::register`
//!    (or `revoke`).
//! 5. Return a `RegisterResponse` / `RevokeResponse` with the canonical
//!    DID + chain ID for caller observability.
//!
//! ## Layer discipline
//!
//! Per [[cipherocto-design-principles]] §Layer discipline:
//! - `octo-ident` (Layer B) — `DidWriteCoordinator` trait + `ChainId`
//! - `octo-identity-resolver-node` (Layer C) — coordinator mediator
//! - `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
//!   stays pure local persistence (no coordinator dep)
//!
//! The coordinator is injected at the resolver-node construction
//! boundary; the `DidRegistry` backend stays decoupled from coordinator
//! wiring (same DI shape as `IdentityResolverNodeConfig.registry`).

use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::{ChainId, DidCodec, DidDocument, DidRegistry, DidWriteCoordinator};

use super::{HandlerOutput, IdentityResolveError};

/// Request payload for `IDENTITY_REGISTER`.
///
/// Wire form: borsh `(canonical_did: String, public_key: [u8; 32],
/// revoked: bool)`. The wire form uses raw fields rather than embedding
/// `DidDocument` to keep `octo-ident::DidDocument` free of borsh derives
/// (Layer B substrate stays decoupled from any specific wire codec).
/// The handler constructs the `DidDocument` from these fields before
/// delegating to the local `DidRegistry::register`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RegisterRequest {
    /// Canonical DID wire form (`did:octo:z<base58btc>`).
    pub canonical_did: String,
    /// 32-byte Ed25519 public key to bind the DID to.
    pub public_key: [u8; 32],
    /// Revoked flag at registration time (typically `false`; reserved
    /// for future `restore` flows per RFC-0010 v1.3 §Compatibility).
    pub revoked: bool,
}

impl RegisterRequest {
    /// Decode from borsh wire form.
    ///
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, IdentityResolveError> {
        borsh::from_slice(bytes).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }

    /// Encode to borsh wire form.
    ///
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, IdentityResolveError> {
        borsh::to_vec(self).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }

    /// Construct the `DidDocument` that will be passed to the registry.
    #[must_use]
    pub fn document(&self) -> DidDocument {
        DidDocument {
            public_key: self.public_key,
            revoked: self.revoked,
        }
    }
}

/// Response payload for `IDENTITY_REGISTER`.
///
/// Wire form: borsh `(canonical_did: String, chain_id: String)`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RegisterResponse {
    /// Canonical DID wire form (post-parse).
    pub canonical_did: String,
    /// Chain ID the coordinator mediated through (echoed for observability).
    pub chain_id: String,
}

/// Request payload for `IDENTITY_REVOKE`.
///
/// Wire form: borsh `(canonical_did: String)`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RevokeRequest(pub String);

impl RevokeRequest {
    /// Decode from borsh wire form.
    ///
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, IdentityResolveError> {
        borsh::from_slice(bytes).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }

    /// Encode to borsh wire form.
    ///
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, IdentityResolveError> {
        borsh::to_vec(self).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }
}

/// Response payload for `IDENTITY_REVOKE`.
///
/// Wire form: borsh `(canonical_did: String, chain_id: String)`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RevokeResponse {
    /// Canonical DID wire form (post-parse).
    pub canonical_did: String,
    /// Chain ID the coordinator mediated through (echoed for observability).
    pub chain_id: String,
}

/// `IDENTITY_REGISTER` handler.
///
/// Mediates `register` through the injected `DidWriteCoordinator` before
/// delegating to the local `DidRegistry::register`. The coordinator
/// defaults to `None` (fail-closed per RFC-0862 v1.3 R12) — operators
/// must inject a concrete coordinator for writes to succeed.
pub struct RegisterHandler {
    registry: Arc<dyn DidRegistry>,
    coordinator: Option<Arc<dyn DidWriteCoordinator>>,
    chain_id: ChainId,
}

impl RegisterHandler {
    /// Construct a new `RegisterHandler`.
    ///
    /// When `coordinator` is `None`, all register attempts are refused
    /// with `IdentityResolveError::CoordinatorUnavailable` (fail-closed
    /// per RFC-0862 v1.3 R12). Production deployments pass
    /// `Some(Arc::new(<concrete-coordinator-impl>))`.
    #[must_use]
    pub fn new(
        registry: Arc<dyn DidRegistry>,
        coordinator: Option<Arc<dyn DidWriteCoordinator>>,
        chain_id: ChainId,
    ) -> Self {
        Self {
            registry,
            coordinator,
            chain_id,
        }
    }

    /// Mediate `register` for the supplied request.
    ///
    /// # Errors
    /// - `IdentityResolveError::InvalidDid` if `canonical_did` is not a
    ///   canonical wire form.
    /// - `IdentityResolveError::CoordinatorUnavailable` if no coordinator
    ///   is configured (fail-closed default).
    /// - `IdentityResolveError::Coordinator` if the coordinator returned
    ///   a `DidWriteCoordinatorError`.
    /// - `IdentityResolveError::Storage` if the local registry call
    ///   fails.
    pub async fn handle(
        &self,
        req: &RegisterRequest,
    ) -> Result<HandlerOutput, IdentityResolveError> {
        // 1. Validate canonical DID shape.
        let wire = octo_ident::CanonicalCodec::parse(&req.canonical_did, false)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 2. Decode canonical wire form → RawDid for the lookup key.
        let raw = octo_ident::CanonicalCodec::wire_to_raw(&wire)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 3. Coordinator mediation (fail-closed if not configured).
        let coordinator = self.coordinator.as_ref().ok_or_else(|| {
            IdentityResolveError::CoordinatorUnavailable(
                "register: no coordinator configured".to_owned(),
            )
        })?;

        // 4. Coordinator owns the writer-election / WAL / cross-instance
        // dispatch; the local registry is touched only after coordinator
        // returns success.
        //
        // `submit_register_validated` is called (not `submit_register`)
        // because the caller has already provided the canonical hash via
        // the wire-form decode — the default impl's canonical-hash
        // re-validation would double-compute the BLAKE3 derivation.
        coordinator
            .submit_register_validated(&raw.hash, &self.chain_id, &req.document())
            .await
            .map_err(|e| IdentityResolveError::Coordinator(format!("{e:?}")))?;

        // 5. Delegate to the local registry (per-instance mutex +
        // FOR UPDATE row lock provides defense-in-depth).
        self.registry
            .register(&raw.hash, req.document())
            .map_err(IdentityResolveError::from)?;

        let response = RegisterResponse {
            canonical_did: wire.as_str().to_owned(),
            chain_id: self.chain_id.to_string(),
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| IdentityResolveError::Serialization(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::IDENTITY_REGISTER)
                .with_note(format!("registered {}", wire.as_str())),
        )
    }
}

/// `IDENTITY_REVOKE` handler.
///
/// Mediates `revoke` through the injected `DidWriteCoordinator` before
/// delegating to the local `DidRegistry::revoke`. Same fail-closed
/// semantics as `RegisterHandler` when no coordinator is configured.
pub struct RevokeHandler {
    registry: Arc<dyn DidRegistry>,
    coordinator: Option<Arc<dyn DidWriteCoordinator>>,
    chain_id: ChainId,
}

impl RevokeHandler {
    /// Construct a new `RevokeHandler`.
    #[must_use]
    pub fn new(
        registry: Arc<dyn DidRegistry>,
        coordinator: Option<Arc<dyn DidWriteCoordinator>>,
        chain_id: ChainId,
    ) -> Self {
        Self {
            registry,
            coordinator,
            chain_id,
        }
    }

    /// Mediate `revoke` for the supplied request.
    ///
    /// # Errors
    /// - `IdentityResolveError::InvalidDid` if `canonical_did` is not a
    ///   canonical wire form.
    /// - `IdentityResolveError::CoordinatorUnavailable` if no coordinator
    ///   is configured.
    /// - `IdentityResolveError::Coordinator` if the coordinator returned
    ///   a `DidWriteCoordinatorError`.
    /// - `IdentityResolveError::Storage` if the local registry call
    ///   fails.
    pub async fn handle(&self, req: &RevokeRequest) -> Result<HandlerOutput, IdentityResolveError> {
        let wire = octo_ident::CanonicalCodec::parse(&req.0, false)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;
        let raw = octo_ident::CanonicalCodec::wire_to_raw(&wire)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        let coordinator = self.coordinator.as_ref().ok_or_else(|| {
            IdentityResolveError::CoordinatorUnavailable(
                "revoke: no coordinator configured".to_owned(),
            )
        })?;

        coordinator
            .submit_revoke(&raw.hash, &self.chain_id)
            .await
            .map_err(|e| IdentityResolveError::Coordinator(format!("{e:?}")))?;

        self.registry
            .revoke(&raw.hash)
            .map_err(IdentityResolveError::from)?;

        let response = RevokeResponse {
            canonical_did: wire.as_str().to_owned(),
            chain_id: self.chain_id.to_string(),
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| IdentityResolveError::Serialization(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::IDENTITY_REVOKE)
                .with_note(format!("revoked {}", wire.as_str())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::InMemoryDidRegistry;

    fn sample_did() -> String {
        let mut pk = [0u8; 32];
        for (i, b) in pk.iter_mut().enumerate() {
            *b = i as u8;
        }
        let raw = octo_ident::CanonicalCodec::mint(&pk);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        wire.as_str().to_owned()
    }

    #[test]
    fn register_request_borsh_round_trip() {
        let req = RegisterRequest {
            canonical_did: sample_did(),
            public_key: [0xAAu8; 32],
            revoked: false,
        };
        let bytes = req.to_borsh().unwrap();
        let back = RegisterRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn revoke_request_borsh_round_trip() {
        let req = RevokeRequest(sample_did());
        let bytes = req.to_borsh().unwrap();
        let back = RevokeRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[tokio::test]
    async fn register_handler_refuses_without_coordinator() {
        // Mission 0871e-f7-impl-resolver-mediation fail-closed TV:
        // with no coordinator configured, register MUST refuse.
        let registry: Arc<dyn DidRegistry> = Arc::new(InMemoryDidRegistry::default());
        let handler = RegisterHandler::new(registry.clone(), None, ChainId::new("test-chain"));
        let req = RegisterRequest {
            canonical_did: sample_did(),
            public_key: [0xAAu8; 32],
            revoked: false,
        };
        let err = handler.handle(&req).await.unwrap_err();
        assert!(matches!(
            err,
            IdentityResolveError::CoordinatorUnavailable(_)
        ));
    }

    #[tokio::test]
    async fn revoke_handler_refuses_without_coordinator() {
        let registry: Arc<dyn DidRegistry> = Arc::new(InMemoryDidRegistry::default());
        let handler = RevokeHandler::new(registry, None, ChainId::new("test-chain"));
        let req = RevokeRequest(sample_did());
        let err = handler.handle(&req).await.unwrap_err();
        assert!(matches!(
            err,
            IdentityResolveError::CoordinatorUnavailable(_)
        ));
    }

    #[tokio::test]
    async fn register_handler_rejects_invalid_did() {
        let registry: Arc<dyn DidRegistry> = Arc::new(InMemoryDidRegistry::default());
        let handler = RegisterHandler::new(registry, None, ChainId::new("test-chain"));
        let req = RegisterRequest {
            canonical_did: "did:octo:bad".to_owned(),
            public_key: [0xAAu8; 32],
            revoked: false,
        };
        let err = handler.handle(&req).await.unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    #[tokio::test]
    async fn revoke_handler_rejects_invalid_did() {
        let registry: Arc<dyn DidRegistry> = Arc::new(InMemoryDidRegistry::default());
        let handler = RevokeHandler::new(registry, None, ChainId::new("test-chain"));
        let req = RevokeRequest("did:octo:bad".to_owned());
        let err = handler.handle(&req).await.unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }
}
