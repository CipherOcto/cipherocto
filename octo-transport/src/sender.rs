use async_trait::async_trait;

/// Context for sending a payload through the transport layer.
#[derive(Debug, Default)]
pub struct SendContext {
    /// Mission-scoped identifier (zero if not mission-scoped).
    pub mission_id: [u8; 32],
    /// Priority level (0 = lowest, 255 = highest).
    pub priority: u8,
    /// Source peer public key (identifies the sending node).
    pub source_peer: [u8; 32],
    /// Gateway that first injected this envelope.
    pub origin_gateway: [u8; 32],
}

/// Errors that can occur during transport operations.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// A single adapter failed to send.
    #[error("adapter failure: {0}")]
    AdapterFailure(String),

    /// All configured transports failed to deliver the payload.
    #[error("all transports failed")]
    AllTransportsFailed,

    /// Failed to construct a transport envelope from the payload.
    #[error("envelope construction failed: {0}")]
    EnvelopeConstruction(String),

    /// The transport is unhealthy and was skipped.
    #[error("transport unhealthy")]
    Unhealthy,
}

/// General-purpose outbound transport trait.
///
/// Any code that needs to send data through the network — sync engines,
/// agent runtimes, marketplace services — uses this trait. Implementors
/// bridge from platform-specific adapters to a uniform send interface.
#[async_trait]
pub trait NetworkSender: Send + Sync {
    /// Send a raw payload through this transport.
    async fn send(&self, payload: &[u8], context: &SendContext) -> Result<(), TransportError>;

    /// Return the transport name for diagnostics.
    fn name(&self) -> &str;

    /// Whether this transport is currently healthy and can send.
    fn is_healthy(&self) -> bool;
}
