//! UDP transport adapter for CipherOcto DOT (RFC-0850 §8.9)
//!
//! Provides `UdpAdapter` implementing `PlatformAdapter` for `PlatformType::Udp`.
//! Suitable for gossip broadcast, heartbeat, and discovery announcements.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};

/// Maximum datagram size: 1400 bytes (MTU-safe)
const MAX_DATAGRAM_SIZE: usize = 1400;

/// UDP transport adapter implementing `PlatformAdapter`.
pub struct UdpAdapter {
    socket: Arc<UdpSocket>,
    peers: Arc<RwLock<BTreeMap<[u8; 32], SocketAddr>>>,
    inbound_tx: mpsc::Sender<RawPlatformMessage>,
    inbound_rx: Arc<RwLock<mpsc::Receiver<RawPlatformMessage>>>,
    healthy: AtomicBool,
}

impl UdpAdapter {
    pub async fn new(listen_addr: SocketAddr) -> Result<Self, std::io::Error> {
        let socket = Arc::new(UdpSocket::bind(listen_addr).await?);
        let (inbound_tx, inbound_rx) = mpsc::channel(256);

        let adapter = Self {
            socket: socket.clone(),
            peers: Arc::new(RwLock::new(BTreeMap::new())),
            inbound_tx,
            inbound_rx: Arc::new(RwLock::new(inbound_rx)),
            healthy: AtomicBool::new(true),
        };

        let tx = adapter.inbound_tx.clone();
        tokio::spawn(async move {
            Self::recv_loop(socket, tx).await;
        });

        Ok(adapter)
    }

    pub async fn add_peer(&self, peer_id: [u8; 32], addr: SocketAddr) {
        self.peers.write().await.insert(peer_id, addr);
    }

    pub async fn remove_peer(&self, peer_id: &[u8; 32]) {
        self.peers.write().await.remove(peer_id);
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    pub fn addr_to_peer_id(addr: SocketAddr) -> [u8; 32] {
        *blake3::hash(addr.to_string().as_bytes()).as_bytes()
    }

    async fn recv_loop(socket: Arc<UdpSocket>, tx: mpsc::Sender<RawPlatformMessage>) {
        let mut buf = vec![0u8; MAX_DATAGRAM_SIZE + 16];

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, from_addr)) => {
                    if len == 0 {
                        continue;
                    }

                    let payload = buf[..len].to_vec();
                    let peer_id = Self::addr_to_peer_id(from_addr);

                    let msg = RawPlatformMessage {
                        platform_id: format!("{:?}", peer_id),
                        payload,
                        metadata: BTreeMap::new(),
                    };

                    if tx.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("UDP recv error: {}", e);
                }
            }
        }
    }
}

#[async_trait]
impl PlatformAdapter for UdpAdapter {
    async fn send_message(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let envelope_bytes = envelope.to_wire_bytes();
        // RFC-0850 §8.9: transmit envelope + payload in one datagram.
        let total = envelope_bytes.len() + payload.len();

        if total > MAX_DATAGRAM_SIZE {
            return Err(PlatformAdapterError::PayloadTooLarge {
                platform: "udp".to_string(),
                size: total,
                max: MAX_DATAGRAM_SIZE,
            });
        }

        let peers = self.peers.read().await;
        if peers.is_empty() {
            return Err(PlatformAdapterError::Unreachable {
                platform: "udp".to_string(),
                reason: "no known peers".to_string(),
            });
        }

        let mut datagram = Vec::with_capacity(total);
        datagram.extend_from_slice(&envelope_bytes);
        datagram.extend_from_slice(payload);

        let mut sent = 0;
        for (peer_id, addr) in peers.iter() {
            match self.socket.send_to(&datagram, addr).await {
                Ok(_) => sent += 1,
                Err(e) => {
                    tracing::warn!("UDP send to {:?} failed: {}", peer_id, e);
                }
            }
        }

        if sent == 0 {
            return Err(PlatformAdapterError::Unreachable {
                platform: "udp".to_string(),
                reason: "all sends failed".to_string(),
            });
        }

        Ok(DeliveryReceipt {
            platform_message_id: format!("{:?}", envelope.envelope_id),
            delivered_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let mut rx = self.inbound_rx.write().await;
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }
        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        DeterministicEnvelope::from_wire_bytes(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("envelope parse error: {}", e),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: MAX_DATAGRAM_SIZE,
            supports_fragmentation: false,
            supports_encryption: false,
            supports_raw_binary: true,
            rate_limit_per_second: 1000,
            media_capabilities: None,
            supports_receive_fragments: false,
            supports_edited_messages: false,
            max_fragment_size: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Udp, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Udp
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        if self.healthy.load(Ordering::Relaxed) {
            Ok(())
        } else {
            Err(PlatformAdapterError::Unreachable {
                platform: "udp".to_string(),
                reason: "adapter marked unhealthy".to_string(),
            })
        }
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        self.healthy.store(false, Ordering::Relaxed);
        self.peers.write().await.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn udp_adapter_create() {
        let adapter = UdpAdapter::new("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(adapter.platform_type(), PlatformType::Udp);
    }

    #[tokio::test]
    async fn udp_add_peer() {
        let adapter = UdpAdapter::new("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let peer_id = [1u8; 32];
        let addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
        adapter.add_peer(peer_id, addr).await;
        assert_eq!(adapter.peer_count().await, 1);
    }

    #[test]
    fn udp_capabilities() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = rt
            .block_on(UdpAdapter::new("127.0.0.1:0".parse().unwrap()))
            .unwrap();
        let caps = adapter.capabilities();
        assert!(caps.supports_raw_binary);
        assert_eq!(caps.max_payload_bytes, MAX_DATAGRAM_SIZE);
    }

    #[test]
    fn udp_domain_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = rt
            .block_on(UdpAdapter::new("127.0.0.1:0".parse().unwrap()))
            .unwrap();
        let domain = adapter.domain_id("127.0.0.1:4002");
        assert_eq!(domain.platform_type, PlatformType::Udp as u16);
    }

    #[tokio::test]
    async fn udp_addr_to_peer_id() {
        let addr1: SocketAddr = "127.0.0.1:4001".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:4002".parse().unwrap();
        let id1 = UdpAdapter::addr_to_peer_id(addr1);
        let id2 = UdpAdapter::addr_to_peer_id(addr2);
        assert_ne!(id1, id2);
    }
}
