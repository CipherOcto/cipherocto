//! TCP transport adapter for CipherOcto DOT (RFC-0850 §8.8)
//!
//! Provides `TcpAdapter` implementing `PlatformAdapter` for `PlatformType::Tcp`.

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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};

const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

pub struct TcpAdapter {
    listen_addr: SocketAddr,
    peers: Arc<RwLock<BTreeMap<[u8; 32], SocketAddr>>>,
    inbound_tx: mpsc::Sender<RawPlatformMessage>,
    inbound_rx: Arc<RwLock<mpsc::Receiver<RawPlatformMessage>>>,
    healthy: AtomicBool,
}

impl TcpAdapter {
    pub async fn new(listen_addr: SocketAddr) -> Result<Self, std::io::Error> {
        let (inbound_tx, inbound_rx) = mpsc::channel(256);

        // Bind the listener before spawning to ensure it's ready
        let listener = TcpListener::bind(listen_addr).await?;
        let actual_addr = listener.local_addr()?;

        let adapter = Self {
            listen_addr: actual_addr,
            peers: Arc::new(RwLock::new(BTreeMap::new())),
            inbound_tx,
            inbound_rx: Arc::new(RwLock::new(inbound_rx)),
            healthy: AtomicBool::new(true),
        };

        let peers = adapter.peers.clone();
        let tx = adapter.inbound_tx.clone();
        tokio::spawn(async move {
            Self::accept_loop(listener, peers, tx).await;
        });

        Ok(adapter)
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<(), std::io::Error> {
        let stream = TcpStream::connect(addr).await?;
        let peer_id = Self::addr_to_peer_id(addr);
        // Spawn a reader for this outbound connection
        let tx = self.inbound_tx.clone();
        let peer_id_for_reader = peer_id;
        tokio::spawn(async move {
            Self::reader_loop(peer_id_for_reader, addr, stream, tx).await;
        });
        self.peers.write().await.insert(peer_id, addr);
        Ok(())
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    fn addr_to_peer_id(addr: SocketAddr) -> [u8; 32] {
        *blake3::hash(addr.to_string().as_bytes()).as_bytes()
    }

    async fn accept_loop(
        listener: TcpListener,
        peers: Arc<RwLock<BTreeMap<[u8; 32], SocketAddr>>>,
        tx: mpsc::Sender<RawPlatformMessage>,
    ) {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let peer_id = Self::addr_to_peer_id(peer_addr);
                    peers.write().await.insert(peer_id, peer_addr);
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        Self::reader_loop(peer_id, peer_addr, stream, tx).await;
                    });
                }
                Err(e) => {
                    tracing::warn!("TCP accept error: {}", e);
                }
            }
        }
    }

    async fn reader_loop(
        peer_id: [u8; 32],
        _peer_addr: SocketAddr,
        mut stream: TcpStream,
        tx: mpsc::Sender<RawPlatformMessage>,
    ) {
        // Wire format (RFC-0850 §8.8, Raw mode, single-frame):
        //   [4-byte total_len][DeterministicEnvelope wire bytes (282 bytes)][mesh payload bytes]
        //
        // One logical message = one RawPlatformMessage. The receiver splits
        // the frame internally: the first 282 bytes are the DOT envelope
        // (parsed by `canonicalize`); the remaining bytes are the mesh
        // payload fed to the inbound handler. This eliminates the
        // consumer-pairing hazard of the prior 2-frame design.
        loop {
            let mut len_buf = [0u8; 4];
            if stream.read_exact(&mut len_buf).await.is_err() {
                break;
            }

            let frame_len = u32::from_be_bytes(len_buf) as usize;
            if frame_len > MAX_FRAME_SIZE {
                break;
            }

            let mut payload = vec![0u8; frame_len];
            if stream.read_exact(&mut payload).await.is_err() {
                break;
            }

            let msg = RawPlatformMessage {
                platform_id: format!("{:?}", peer_id),
                payload,
                metadata: BTreeMap::new(),
            };

            if tx.send(msg).await.is_err() {
                break;
            }
        }
    }
}

#[async_trait]
impl PlatformAdapter for TcpAdapter {
    async fn send_message(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let envelope_bytes = envelope.to_wire_bytes();
        // Wire format (RFC-0850 §8.8, Raw mode, single-frame):
        //   [4-byte total_len][envelope wire bytes][payload bytes]
        //
        // One logical message = one contiguous frame. Receiver reads
        // total_len, then total_len bytes; splits internally.
        let total_len = (envelope_bytes.len() + payload.len()) as u32;
        let mut frame = Vec::with_capacity(4 + envelope_bytes.len() + payload.len());
        frame.extend_from_slice(&total_len.to_be_bytes());
        frame.extend_from_slice(&envelope_bytes);
        frame.extend_from_slice(payload);

        let peers = self.peers.read().await;
        if peers.is_empty() {
            return Err(PlatformAdapterError::Unreachable {
                platform: "tcp".to_string(),
                reason: "no connected peers".to_string(),
            });
        }

        let mut sent = 0;
        for (peer_id, addr) in peers.iter() {
            // Connect fresh for each send (simple but correct)
            match TcpStream::connect(addr).await {
                Ok(mut stream) => {
                    if stream.write_all(&frame).await.is_ok() {
                        sent += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("TCP connect to {:?} failed: {}", peer_id, e);
                }
            }
        }

        if sent == 0 {
            return Err(PlatformAdapterError::Unreachable {
                platform: "tcp".to_string(),
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
        // Wire format is `[envelope_bytes][payload_bytes]`. Parse only the
        // first `ENVELOPE_WIRE_LEN` bytes as the envelope; the remaining
        // bytes are the mesh payload and are extracted separately by
        // `PlatformAdapterPoller` (or other consumers).
        use octo_network::dot::envelope::ENVELOPE_WIRE_LEN;
        if raw.payload.len() < ENVELOPE_WIRE_LEN {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "envelope parse error: frame too short ({} bytes, need {})",
                    raw.payload.len(),
                    ENVELOPE_WIRE_LEN
                ),
            });
        }
        DeterministicEnvelope::from_wire_bytes(&raw.payload[..ENVELOPE_WIRE_LEN]).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("envelope parse error: {}", e),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: MAX_FRAME_SIZE,
            supports_fragmentation: false,
            supports_encryption: false,
            supports_raw_binary: true,
            rate_limit_per_second: 10000,
            media_capabilities: None,
            supports_receive_fragments: false,
            supports_edited_messages: false,
            max_fragment_size: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Tcp, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Tcp
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        if self.healthy.load(Ordering::Relaxed) {
            Ok(())
        } else {
            Err(PlatformAdapterError::Unreachable {
                platform: "tcp".to_string(),
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
    async fn tcp_adapter_create_and_connect() {
        let adapter1 = TcpAdapter::new("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr1 = adapter1.local_addr();

        // Wait for accept loop to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let adapter2 = TcpAdapter::new("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();

        adapter2.connect(addr1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert!(adapter1.peer_count().await >= 1);
    }

    #[test]
    fn tcp_platform_type() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = rt
            .block_on(TcpAdapter::new("127.0.0.1:0".parse().unwrap()))
            .unwrap();
        assert_eq!(adapter.platform_type(), PlatformType::Tcp);
    }

    #[test]
    fn tcp_capabilities() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = rt
            .block_on(TcpAdapter::new("127.0.0.1:0".parse().unwrap()))
            .unwrap();
        let caps = adapter.capabilities();
        assert!(caps.supports_raw_binary);
        assert_eq!(caps.max_payload_bytes, MAX_FRAME_SIZE);
    }

    #[test]
    fn tcp_domain_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = rt
            .block_on(TcpAdapter::new("127.0.0.1:0".parse().unwrap()))
            .unwrap();
        let domain = adapter.domain_id("127.0.0.1:4001");
        assert_eq!(domain.platform_type, PlatformType::Tcp as u16);
    }
}
