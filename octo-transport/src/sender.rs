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

    /// A governance check rejected the operation (e.g. domain
    /// decommissioned, sender kicked, lifecycle Rebooting).
    #[error("governance violation: {0}")]
    GovernanceViolation(String),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_context_default() {
        let ctx = SendContext::default();
        assert_eq!(ctx.mission_id, [0u8; 32]);
        assert_eq!(ctx.priority, 0);
        assert_eq!(ctx.source_peer, [0u8; 32]);
        assert_eq!(ctx.origin_gateway, [0u8; 32]);
    }

    #[test]
    fn send_context_with_values() {
        let ctx = SendContext {
            mission_id: [1u8; 32],
            priority: 255,
            source_peer: [2u8; 32],
            origin_gateway: [3u8; 32],
        };
        assert_eq!(ctx.mission_id, [1u8; 32]);
        assert_eq!(ctx.priority, 255);
        assert_eq!(ctx.source_peer, [2u8; 32]);
        assert_eq!(ctx.origin_gateway, [3u8; 32]);
    }

    #[test]
    fn transport_error_display() {
        let cases = vec![
            (TransportError::AdapterFailure("test".into()), "adapter failure: test"),
            (TransportError::AllTransportsFailed, "all transports failed"),
            (TransportError::EnvelopeConstruction("bad".into()), "envelope construction failed: bad"),
            (TransportError::Unhealthy, "transport unhealthy"),
            (TransportError::GovernanceViolation("denied".into()), "governance violation: denied"),
        ];
        for (err, expected) in cases {
            assert_eq!(format!("{}", err), expected);
        }
    }

    #[test]
    fn transport_error_debug() {
        let err = TransportError::AdapterFailure("test".into());
        let debug = format!("{:?}", err);
        assert!(debug.contains("AdapterFailure"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn send_context_debug() {
        let ctx = SendContext::default();
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("SendContext"));
        assert!(debug.contains("mission_id"));
    }
}
