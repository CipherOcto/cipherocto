//! Transport glue crate for `octo-cap-macaroon`.
//!
//! Owns the production-wired [`TransportDeliveryCatalog`] that drives
//! [`octo_cap_macaroon::macaroon::CapabilityCatalog`] + [`CapabilityGossip`]
//! via the canonical RFC-0862 gossip substrate
//! (`octo_transport::NodeTransport::broadcast`).
//!
//! ## Why this crate exists (Phase 2c-1)
//!
//! `octo-cap-macaroon` is a Layer 4 extension crate per RFC-0965
//! per-extension crate layout mandate. Layer 4 must NOT depend on Layer D
//! (transport adapters) — that violates the layer direction (A → B →
//! C → D/E, never the reverse). Prior to Phase 2c-1, the
//! `TransportDeliveryCatalog` struct + its `octo_transport::NodeTransport`
//! field lived in `octo-cap-macaroon::macaroon` and forced a
//! `octo-transport` dep on the macaroon substrate.
//!
//! Phase 2c-1 extracts the struct + its impls into this glue crate
//! (depends on `octo-cap-macaroon` + `octo-transport`), removing the
//! `octo-transport` dep from `octo-cap-macaroon`. The layer direction
//! is preserved: this glue crate sits between Layer 4 and Layer D, not
//! above Layer D.
//!
//! ## Algorithm (RFC-0959-A1 §Algorithms)
//!
//! `mission_id = BLAKE3-256(b"cipherocto:market-delivery:mission" || payload)[:32]`
//! — canonical mission-scoped binding that downstream DC reputation
//! stores (`octo-reputation::SlashReputationStoreCompat`) consume.
//!
//! ## Migration
//!
//! Callers previously using
//! `octo_cap_macaroon::macaroon::TransportDeliveryCatalog` now use
//! `octo_cap_macaroon_transport::TransportDeliveryCatalog`. The struct
//! + constructor signature are unchanged.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use octo_cap_macaroon::macaroon::{
    CapabilityCatalog, CapabilityGossip, CatalogGossipError, Macaroon,
};

/// Production-wired `CapabilityCatalog` that delivers `MarketDeliveryEnvelope`
/// payloads through the canonical RFC-0862 gossip channel
/// (`octo_transport::NodeTransport::broadcast`).
///
/// The struct is `Send + Sync` (the transport holds `Arc`s internally) and
/// is intended to be installed in the wallet's `CapabilityCatalogRegistry`
/// once per `NodeTransport` lifecycle (typically per node startup). Multiple
/// `TransportDeliveryCatalog` instances can coexist — e.g., one for HTTP
/// senders + one for TCP senders — but the canonical pattern is one catalog
/// per node hosting the unified `NodeTransport`.
///
/// # Identity context
///
/// `source_peer` is the 32-byte public key of the node broadcasting the
/// envelope (the seller-side producer). `origin_gateway` is the 32-byte
/// gateway identifier of the gateway that first injected the envelope
/// into the gossip network (often the same gateway as the seller, but
/// distinct field for the multi-hop case).
///
/// # Mission context
///
/// `mission_id` derivation per RFC-0959-A1 §Algorithms: `mission_id` is
/// the first 32 bytes of `BLAKE3-256(envelope_payload)` domain-separated
/// to `b"cipherocto:market-delivery:mission"` — this is the canonical
/// mission-scoped binding that downstream DC reputation stores
/// (`octo-reputation::SlashReputationStoreCompat`) consume.
pub struct TransportDeliveryCatalog {
    transport: Arc<octo_transport::NodeTransport>,
    source_peer: [u8; 32],
    origin_gateway: [u8; 32],
}

impl TransportDeliveryCatalog {
    /// Default gossip priority for `MarketDeliveryEnvelope` payloads
    /// (mid-band: above gossip churn, below real-time control traffic).
    pub const DEFAULT_GOSSIP_PRIORITY: u8 = 128;

    /// Construct a new `TransportDeliveryCatalog`.
    ///
    /// # Arguments
    ///
    /// * `transport` — shared `NodeTransport` (typically one per node).
    /// * `source_peer` — 32-byte public key of the gossiping node.
    /// * `origin_gateway` — 32-byte gateway identifier (may equal
    ///   `source_peer` for single-hop deployments).
    pub fn new(
        transport: Arc<octo_transport::NodeTransport>,
        source_peer: [u8; 32],
        origin_gateway: [u8; 32],
    ) -> Self {
        Self {
            transport,
            source_peer,
            origin_gateway,
        }
    }

    /// Derive the canonical `mission_id` for a `MarketDeliveryEnvelope`
    /// payload. Domain-separated BLAKE3-256, first 32 bytes.
    fn mission_id_for(payload: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cipherocto:market-delivery:mission");
        hasher.update(payload);
        let out = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&out.as_bytes()[..32]);
        id
    }
}

impl std::fmt::Debug for TransportDeliveryCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Per RFC-0957-A1 §Security: do not leak `source_peer` /
        // `origin_gateway` bytes in Debug (operator-facing diagnostic
        // only; manual redaction defends against accidental log
        // exposure — same defense-in-depth as `BridgeError` in
        // `octo-network::dc::slash_bridge`).
        f.debug_struct("TransportDeliveryCatalog")
            .field("transport", &"<Arc<NodeTransport>>")
            .field("source_peer", &"[REDACTED 32B]")
            .field("origin_gateway", &"[REDACTED 32B]")
            .finish()
    }
}

impl CapabilityCatalog for TransportDeliveryCatalog {
    fn get(&self, _id: &[u8; 32]) -> Option<&Macaroon> {
        // TransportDeliveryCatalog owns gossip delivery only; it is not
        // the canonical macaroon storage path. Production wallets
        // compose a `CompositeCapabilityCatalog` that delegates
        // `get` to the underlying storage catalog and `gossip_to_buyer`
        // to this struct. Returning `None` keeps the default fallback
        // behavior consistent with `NodeTransport`-only catalogs.
        None
    }

    fn implements_gossip(&self) -> bool {
        true
    }
}

#[async_trait]
impl CapabilityGossip for TransportDeliveryCatalog {
    async fn gossip_to_buyer(
        &self,
        _buyer_did: &str,
        env: &[u8],
    ) -> Result<(), CatalogGossipError> {
        let ctx = octo_transport::SendContext {
            mission_id: Self::mission_id_for(env),
            priority: Self::DEFAULT_GOSSIP_PRIORITY,
            source_peer: self.source_peer,
            origin_gateway: self.origin_gateway,
        };

        // `NodeTransport::broadcast` is `async fn` returning the count
        // of successful sender deliveries (plain `usize`, not `Result`).
        // A zero count is **not** an error (every node may be offline);
        // we surface success in that case so the bounded retry loop in
        // `gossip_envelope_to_buyer` does not mistake "no peers
        // reachable right now" for "transient failure".
        let _delivered = self.transport.broadcast(env, &ctx).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_id_is_32_bytes() {
        let id = TransportDeliveryCatalog::mission_id_for(b"hello");
        assert_eq!(id.len(), 32);
    }

    #[test]
    fn mission_id_is_deterministic() {
        let a = TransportDeliveryCatalog::mission_id_for(b"payload");
        let b = TransportDeliveryCatalog::mission_id_for(b"payload");
        assert_eq!(a, b);
    }

    #[test]
    fn mission_id_changes_with_payload() {
        let a = TransportDeliveryCatalog::mission_id_for(b"a");
        let b = TransportDeliveryCatalog::mission_id_for(b"b");
        assert_ne!(a, b);
    }

    #[test]
    fn debug_redacts_identity_fields() {
        let catalog = TransportDeliveryCatalog {
            transport: Arc::new(octo_transport::NodeTransport::new(vec![])),
            source_peer: [0xAB; 32],
            origin_gateway: [0xCD; 32],
        };
        let dbg = format!("{catalog:?}");
        assert!(
            !dbg.contains("abababab"),
            "source_peer MUST be redacted; got {dbg}"
        );
        assert!(
            !dbg.contains("cdcdcdcd"),
            "origin_gateway MUST be redacted; got {dbg}"
        );
        assert!(
            dbg.contains("[REDACTED 32B]"),
            "redaction marker MUST appear; got {dbg}"
        );
    }

    #[test]
    fn implements_gossip_returns_true() {
        let catalog = TransportDeliveryCatalog {
            transport: Arc::new(octo_transport::NodeTransport::new(vec![])),
            source_peer: [0; 32],
            origin_gateway: [0; 32],
        };
        assert!(catalog.implements_gossip());
    }

    #[test]
    fn get_returns_none() {
        let catalog = TransportDeliveryCatalog {
            transport: Arc::new(octo_transport::NodeTransport::new(vec![])),
            source_peer: [0; 32],
            origin_gateway: [0; 32],
        };
        assert!(catalog.get(&[0u8; 32]).is_none());
    }
}
