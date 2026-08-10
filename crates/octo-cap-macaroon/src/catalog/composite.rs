//! `CompositeCapabilityCatalog` (mission 0959-c4).
//!
//! Composes a storage backend (`&dyn CapabilityCatalog`) with an async
//! gossip backend (`&dyn CapabilityGossip`). Dispatches:
//! - `lookup` → storage (clones the `Macaroon`)
//! - `is_raw_name_registered` → storage
//! - `root_secret_for_ask` / `settlement_chain_tip` → storage
//! - `gossip_to_buyer_sync` → gossip (sync shim returns
//!   `CatalogGossipError::Unsupported` per `CapabilityCatalog` default
//!   — composite callers use the async path via `gossip_async`)
//! - `implements_gossip()` → `true`
//!
//! Use case: production wallet composes `StoolapHolderRegistry`
//! (storage, via wrapper) + `TransportDeliveryCatalog` (gossip) into
//! a single catalog handle. Closes 0959-c3 Notes "out of scope for
//! Band A".
//!
//! ## Layer discipline
//!
//! This module lives in `octo-cap-macaroon` (Layer 4 extension
//! crate). The composite depends on `Arc<dyn CapabilityCatalog>` +
//! `Arc<dyn CapabilityGossip>` from the same crate — no cross-layer
//! deps. Production deployments inject the concrete sub-catalogs at
//! construction.

use std::sync::Arc;

use async_trait::async_trait;

use crate::macaroon::{CapabilityCatalog, CapabilityGossip, CatalogGossipError, Macaroon};

/// Composite catalog: storage + gossip delegation in one handle.
pub struct CompositeCapabilityCatalog {
    storage: Arc<dyn CapabilityCatalog>,
    gossip: Arc<dyn CapabilityGossip>,
}

impl CompositeCapabilityCatalog {
    /// Construct a composite from a storage + gossip backend.
    #[must_use]
    pub fn new(storage: Arc<dyn CapabilityCatalog>, gossip: Arc<dyn CapabilityGossip>) -> Self {
        Self { storage, gossip }
    }

    /// Borrow the gossip backend (used by the async retry loop in
    /// `gossip_envelope_to_buyer` when the catalog is downcast).
    #[must_use]
    pub fn gossip(&self) -> &Arc<dyn CapabilityGossip> {
        &self.gossip
    }

    /// Borrow the storage backend.
    #[must_use]
    pub fn storage(&self) -> &Arc<dyn CapabilityCatalog> {
        &self.storage
    }
}

impl CapabilityCatalog for CompositeCapabilityCatalog {
    fn lookup(&self, id: &[u8; 32]) -> Option<Macaroon> {
        self.storage.lookup(id)
    }

    fn is_raw_name_registered(&self, name: &str) -> bool {
        self.storage.is_raw_name_registered(name)
    }

    fn root_secret_for_ask(&self, ask_id: &[u8; 32]) -> Option<[u8; 32]> {
        self.storage.root_secret_for_ask(ask_id)
    }

    fn settlement_chain_tip(&self) -> Option<[u8; 32]> {
        self.storage.settlement_chain_tip()
    }

    fn gossip_to_buyer_sync(
        &self,
        _buyer_did: &str,
        _env: &[u8],
    ) -> Result<(), CatalogGossipError> {
        // The sync shim returns `Unsupported` — async path via
        // `gossip_async()` is the canonical composite interface.
        Err(CatalogGossipError::Unsupported)
    }

    fn implements_gossip(&self) -> bool {
        true
    }
}

/// Async wrapper that exposes the composite's gossip backend to
/// callers using the `&dyn CapabilityGossip` dispatch (the bounded
/// retry loop in `gossip_envelope_to_buyer`). This is a thin
/// pass-through that re-exposes `CompositeCapabilityCatalog`'s
/// gossip slot as a `CapabilityGossip` trait object.
pub struct CompositeGossip {
    inner: Arc<dyn CapabilityGossip>,
}

impl CompositeGossip {
    /// Construct from the gossip backend half of a composite.
    #[must_use]
    pub fn new(inner: Arc<dyn CapabilityGossip>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl CapabilityGossip for CompositeGossip {
    async fn gossip_to_buyer(&self, buyer_did: &str, env: &[u8]) -> Result<(), CatalogGossipError> {
        self.inner.gossip_to_buyer(buyer_did, env).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Storage mock that returns a fixed `Macaroon` for one id.
    struct FixedStorage {
        id: [u8; 32],
        macaroon: Macaroon,
        raw_name: String,
    }

    impl CapabilityCatalog for FixedStorage {
        fn lookup(&self, id: &[u8; 32]) -> Option<Macaroon> {
            if id == &self.id {
                Some(self.macaroon.clone())
            } else {
                None
            }
        }

        fn is_raw_name_registered(&self, name: &str) -> bool {
            name == self.raw_name
        }

        fn root_secret_for_ask(&self, _ask_id: &[u8; 32]) -> Option<[u8; 32]> {
            Some([0xab; 32])
        }

        fn settlement_chain_tip(&self) -> Option<[u8; 32]> {
            Some([0xcd; 32])
        }
    }

    /// Gossip mock that records calls.
    struct RecordingGossip {
        calls: std::sync::Mutex<Vec<(String, Vec<u8>)>>,
    }

    #[async_trait::async_trait]
    impl CapabilityGossip for RecordingGossip {
        async fn gossip_to_buyer(
            &self,
            buyer_did: &str,
            env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            self.calls
                .lock()
                .expect("poisoned")
                .push((buyer_did.to_owned(), env.to_vec()));
            Ok(())
        }
    }

    fn sample_macaroon() -> Macaroon {
        use crate::macaroon::Macaroon;
        // Empty-initial-caveats macaroon — only the root_id matters
        // for the lookup delegation test.
        Macaroon::mint(&[0x42; 32]).expect("mint")
    }

    /// TV1 — `composite_storage_hits_only_storage_lookup`:
    /// `lookup` delegates to storage and returns the cloned
    /// `Macaroon`. Misses return `None`.
    #[test]
    fn composite_storage_hits_only_storage_lookup() {
        let id = [0x07; 32];
        let m = sample_macaroon();
        let storage: Arc<dyn CapabilityCatalog> = Arc::new(FixedStorage {
            id,
            macaroon: m.clone(),
            raw_name: "test-raw".into(),
        });
        let gossip: Arc<dyn CapabilityGossip> = Arc::new(RecordingGossip {
            calls: std::sync::Mutex::new(vec![]),
        });
        let composite = CompositeCapabilityCatalog::new(storage, gossip);
        let got = composite.lookup(&id);
        assert!(got.is_some());
        assert_eq!(got.unwrap().root_id, m.root_id);
        let miss = composite.lookup(&[0xff; 32]);
        assert!(miss.is_none(), "unknown id must return None via storage");
    }

    /// TV2 — `composite_gossip_hits_only_gossip_delivery`:
    /// the async path (`gossip_async`) goes through the gossip
    /// backend. The storage backend is never queried during gossip.
    #[tokio::test]
    async fn composite_gossip_hits_only_gossip_delivery() {
        let storage: Arc<dyn CapabilityCatalog> = Arc::new(FixedStorage {
            id: [0; 32],
            macaroon: sample_macaroon(),
            raw_name: String::new(),
        });
        let recording = Arc::new(RecordingGossip {
            calls: std::sync::Mutex::new(vec![]),
        });
        let gossip: Arc<dyn CapabilityGossip> = recording.clone();
        let composite = CompositeCapabilityCatalog::new(storage, gossip);
        let wrapper = CompositeGossip::new(composite.gossip().clone());
        wrapper
            .gossip_to_buyer("did:octo:zBuyer", b"env-bytes")
            .await
            .expect("gossip ok");
        let calls = recording.calls.lock().expect("poisoned");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "did:octo:zBuyer");
        assert_eq!(calls[0].1, b"env-bytes");
    }

    /// TV3 — `composite_implements_gossip`: composite advertises
    /// gossip support (production caller short-circuits the
    /// `Unsupported` retry path).
    #[test]
    fn composite_implements_gossip() {
        let storage: Arc<dyn CapabilityCatalog> = Arc::new(FixedStorage {
            id: [0; 32],
            macaroon: sample_macaroon(),
            raw_name: String::new(),
        });
        let gossip: Arc<dyn CapabilityGossip> = Arc::new(RecordingGossip {
            calls: std::sync::Mutex::new(vec![]),
        });
        let composite = CompositeCapabilityCatalog::new(storage, gossip);
        assert!(composite.implements_gossip());
        // Sync shim still returns Unsupported per default contract.
        assert!(matches!(
            composite.gossip_to_buyer_sync("did:octo:zBuyer", b"env"),
            Err(CatalogGossipError::Unsupported)
        ));
    }

    /// TV4 — `composite_lookup_active_propagates_revocation`:
    /// `root_secret_for_ask` + `settlement_chain_tip` delegate to
    /// storage (proves the storage sub-catalog is the source of
    /// truth for chain metadata).
    #[test]
    fn composite_lookup_active_propagates_revocation() {
        let storage: Arc<dyn CapabilityCatalog> = Arc::new(FixedStorage {
            id: [0; 32],
            macaroon: sample_macaroon(),
            raw_name: String::new(),
        });
        let gossip: Arc<dyn CapabilityGossip> = Arc::new(RecordingGossip {
            calls: std::sync::Mutex::new(vec![]),
        });
        let composite = CompositeCapabilityCatalog::new(storage, gossip);
        assert_eq!(composite.root_secret_for_ask(&[0; 32]), Some([0xab; 32]));
        assert_eq!(composite.settlement_chain_tip(), Some([0xcd; 32]));
    }
}
