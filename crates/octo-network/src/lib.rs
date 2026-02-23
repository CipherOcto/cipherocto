//! CipherOcto Network
//!
//! Peer-to-peer networking for the CipherOcto protocol.
//!
//! Responsibilities:
//! - Peer discovery
//! - Message routing
//! - Provider coordination
//! - Network simulation (MVP)
//!
//! In Phase 1: Local simulation with loopback peers
//! In Phase 2+: libp2p-based decentralized networking

use anyhow::Result;
use tokio::sync::RwLock;

pub struct Network {
    peers: RwLock<Vec<String>>,
}

impl Network {
    /// Create a new network instance
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(vec![]),
        }
    }

    /// Add a peer to the network
    pub async fn add_peer(&self, peer_id: String) -> Result<()> {
        let mut peers = self.peers.write().await;
        let peer_count = peers.len() + 1;
        peers.push(peer_id.clone());
        println!("🌐 Peer added: {} (total: {} peers)", peer_id, peer_count);
        Ok(())
    }

    /// Get network status
    pub async fn status(&self) -> NetworkStatus {
        let peers = self.peers.read().await;
        NetworkStatus {
            peer_count: peers.len(),
            is_active: !peers.is_empty(),
        }
    }
}

pub struct NetworkStatus {
    pub peer_count: usize,
    pub is_active: bool,
}
