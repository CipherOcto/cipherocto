//! 3-node integration TV for `NodeTransport::request_response`
//! (mission 0870k-transport-request-response AC-11).
//!
//! Models A → B → C as three `NodeTransport` instances where each
//! sender carries a `RequestRelay` closure that simulates one hop.
//! Each hop generates a fresh `envelope_id` for its own inner
//! forward (so the substrate's per-NodeTransport pending registry
//! doesn't collide); the chain correlation at each hop is verified
//! independently.
//!
//! No real network. No RFC-0871 envelope serialization (this is the
//! substrate test; the semantic-layer binding lands in mission
//! 0871b-cross-node-forwarding).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use octo_transport::{NetworkSender, NodeTransport, SendContext, TransportError};

/// Sender that forwards a request via a shared channel to a sibling
/// `NodeTransport` (the next hop) and returns its reply. Generates a
/// fresh `envelope_id` for the inner forward so the substrate's
/// per-NodeTransport pending registry stays consistent across hops.
struct RequestRelaySender {
    next_hop: Arc<NodeTransport>,
    /// Counter for generating unique inner envelope_ids.
    counter: Arc<std::sync::atomic::AtomicU32>,
}

#[async_trait]
impl NetworkSender for RequestRelaySender {
    async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        Ok(())
    }

    async fn send_request(
        &self,
        payload: &[u8],
        _envelope_id: [u8; 32],
        context: &SendContext,
        _timeout: Duration,
    ) -> Result<Vec<u8>, TransportError> {
        // Fresh inner envelope_id for the next-hop forward.
        let inner_id = self.next_inner_envelope_id();
        self.next_hop
            .request_response(payload, inner_id, context, Duration::from_secs(2))
            .await
    }

    fn name(&self) -> &str {
        "relay"
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

impl RequestRelaySender {
    fn next_inner_envelope_id(&self) -> [u8; 32] {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut id = [0u8; 32];
        id[0..4].copy_from_slice(&n.to_be_bytes());
        id
    }
}

/// Terminal sender that returns a canned response.
struct TerminalSender {
    response_template: Vec<u8>,
}

#[async_trait]
impl NetworkSender for TerminalSender {
    async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        Ok(())
    }

    async fn send_request(
        &self,
        _payload: &[u8],
        _envelope_id: [u8; 32],
        _context: &SendContext,
        _timeout: Duration,
    ) -> Result<Vec<u8>, TransportError> {
        Ok(self.response_template.clone())
    }

    fn name(&self) -> &str {
        "terminal"
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn three_node_chain_correlates_by_envelope_id() {
    let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

    // C is terminal: returns canned "c-reply" bytes.
    let c = Arc::new(NodeTransport::new(vec![Arc::new(TerminalSender {
        response_template: b"c-reply".to_vec(),
    })]));

    // B relays: forwards to C, returns C's reply.
    let b = Arc::new(NodeTransport::new(vec![Arc::new(RequestRelaySender {
        next_hop: c.clone(),
        counter: counter.clone(),
    })]));

    // A also relays: forwards to B, returns B's reply (which is C's).
    let a = Arc::new(NodeTransport::new(vec![Arc::new(RequestRelaySender {
        next_hop: b.clone(),
        counter: counter.clone(),
    })]));

    let ctx = SendContext::default();
    let envelope_id = [0xABu8; 32];

    let reply = a
        .request_response(b"a-request", envelope_id, &ctx, Duration::from_secs(2))
        .await
        .expect("3-hop chain round-trips");

    // A's reply IS C's canned response (B is transparent).
    assert_eq!(reply, b"c-reply");

    // Pending registries cleaned up at every hop.
    assert_eq!(a.pending_count().await, 0);
    assert_eq!(b.pending_count().await, 0);
    assert_eq!(c.pending_count().await, 0);
}

#[tokio::test]
async fn three_node_chain_timeout_propagates_as_unsupported() {
    let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

    // C is empty (no senders) — request will fail with `Unsupported`.
    let c = Arc::new(NodeTransport::new(vec![]));

    // B relays to C — but C has no senders, so relay returns Err.
    let b = Arc::new(NodeTransport::new(vec![Arc::new(RequestRelaySender {
        next_hop: c.clone(),
        counter: counter.clone(),
    })]));

    let a = Arc::new(NodeTransport::new(vec![Arc::new(RequestRelaySender {
        next_hop: b.clone(),
        counter: counter.clone(),
    })]));

    let ctx = SendContext::default();
    let envelope_id = [0xCDu8; 32];

    let result = a
        .request_response(b"req", envelope_id, &ctx, Duration::from_millis(100))
        .await;
    // C has no sender → relay returns Unsupported → propagates up.
    assert!(matches!(result, Err(TransportError::Unsupported(_))));

    // Pending registries cleaned up via cancel path.
    assert_eq!(a.pending_count().await, 0);
    assert_eq!(b.pending_count().await, 0);
}
