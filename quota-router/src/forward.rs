use std::collections::BTreeMap;
use std::sync::Mutex;

use super::provider::{NetworkId, ProviderId, RouterNodeId};
use super::request::RequestContext;

/// ForwardRequest envelope — sent between nodes to relay a consumer
/// request through the mesh. `hmac` is only verified when the
/// forwarding peer is configured `PeerTrust::Verified` (RFC v1.10,
/// 0870d acceptance criterion #1).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ForwardRequestPayload {
    pub request_id: [u8; 32],
    pub network_id: NetworkId,
    pub context: RequestContext,
    pub payload: Vec<u8>,
    pub ttl: u8,
    pub origin_node: RouterNodeId,
    pub hop_count: u8,
    pub created_at: u64,
    /// BLAKE3 keyed-MAC over the canonical pre-image of this payload
    /// (with `hmac` zeroed). Verified only when the sending peer is
    /// `PeerTrust::Verified`; otherwise treated as opaque bytes.
    pub hmac: [u8; 32],
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ForwardResponsePayload {
    pub request_id: [u8; 32],
    pub response: Vec<u8>,
    pub executed_by: ProviderId,
    pub latency_ms: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ForwardRejectPayload {
    pub request_id: [u8; 32],
    pub peer_id: RouterNodeId,
    pub reason: ForwardRejectReason,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ForwardRejectReason {
    TtlExpired,
    NoProvider,
    ModelNotSupported,
    CapacityExhausted,
    ContextWindowExceeded,
    BudgetExceeded,
    AuthFailure,
    PayloadTooLarge,
}

pub enum ForwardOutcome {
    Completed(Vec<u8>),
    Rejected(ForwardRejectReason),
    Timeout,
}

pub struct PendingRequests {
    inner: Mutex<BTreeMap<[u8; 32], PendingEntry>>,
}

struct PendingEntry {
    tx: tokio::sync::oneshot::Sender<ForwardOutcome>,
    origin_node: RouterNodeId,
}

impl Default for PendingRequests {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingRequests {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn insert(
        &self,
        request_id: [u8; 32],
        tx: tokio::sync::oneshot::Sender<ForwardOutcome>,
        origin_node: RouterNodeId,
    ) {
        self.inner
            .lock()
            .unwrap()
            .insert(request_id, PendingEntry { tx, origin_node });
    }

    pub fn origin(&self, request_id: [u8; 32]) -> Option<RouterNodeId> {
        self.inner
            .lock()
            .unwrap()
            .get(&request_id)
            .map(|e| e.origin_node)
    }

    pub fn complete(&self, request_id: [u8; 32], response: Vec<u8>) {
        if let Some(entry) = self.inner.lock().unwrap().remove(&request_id) {
            let _ = entry.tx.send(ForwardOutcome::Completed(response));
        }
    }

    pub fn reject(&self, request_id: [u8; 32], reason: ForwardRejectReason) {
        if let Some(entry) = self.inner.lock().unwrap().remove(&request_id) {
            let _ = entry.tx.send(ForwardOutcome::Rejected(reason));
        }
    }

    /// Drop a pending entry without sending a response (e.g., when
    /// the send itself failed and there's nobody to notify).
    pub fn cancel(&self, request_id: [u8; 32]) {
        self.inner.lock().unwrap().remove(&request_id);
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CapacityRequestPayload {
    pub requester_id: RouterNodeId,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request() -> ForwardRequestPayload {
        ForwardRequestPayload {
            request_id: [1u8; 32],
            network_id: NetworkId([2u8; 32]),
            context: crate::request::RequestContext {
                model: "gpt-4o".into(),
                preferred_provider: None,
                model_group: None,
                input_tokens: None,
                max_output_tokens: None,
                tags: None,
                max_price_per_1k_tokens: None,
                max_latency_ms: None,
                policy_override: None,
                consumer_id: [0u8; 32],
                priority: 0,
                deadline: None,
            },
            payload: b"hello".to_vec(),
            ttl: 3,
            origin_node: RouterNodeId([9u8; 32]),
            hop_count: 0,
            created_at: 100,
            hmac: [0u8; 32],
        }
    }

    #[test]
    fn forward_request_roundtrip() {
        let req = test_request();
        let encoded = bincode::serialize(&req).unwrap();
        let decoded: ForwardRequestPayload = bincode::deserialize(&encoded).unwrap();
        assert_eq!(decoded.request_id, req.request_id);
        assert_eq!(decoded.network_id, req.network_id);
        assert_eq!(decoded.context.model, "gpt-4o");
        assert_eq!(decoded.ttl, 3);
        assert_eq!(decoded.hop_count, 0);
        assert_eq!(decoded.payload, b"hello".to_vec());
    }

    #[test]
    fn forward_response_roundtrip() {
        let resp = ForwardResponsePayload {
            request_id: [3u8; 32],
            response: b"result".to_vec(),
            executed_by: ProviderId([4u8; 32]),
            latency_ms: 150,
        };
        let encoded = bincode::serialize(&resp).unwrap();
        let decoded: ForwardResponsePayload = bincode::deserialize(&encoded).unwrap();
        assert_eq!(decoded.request_id, [3u8; 32]);
        assert_eq!(decoded.response, b"result".to_vec());
        assert_eq!(decoded.latency_ms, 150);
    }

    #[test]
    fn forward_reject_roundtrip() {
        let reject = ForwardRejectPayload {
            request_id: [5u8; 32],
            peer_id: RouterNodeId([6u8; 32]),
            reason: ForwardRejectReason::TtlExpired,
        };
        let encoded = bincode::serialize(&reject).unwrap();
        let decoded: ForwardRejectPayload = bincode::deserialize(&encoded).unwrap();
        assert_eq!(decoded.request_id, [5u8; 32]);
        assert!(matches!(decoded.reason, ForwardRejectReason::TtlExpired));
    }

    #[test]
    fn capacity_request_roundtrip() {
        let req = CapacityRequestPayload {
            requester_id: RouterNodeId([7u8; 32]),
        };
        let encoded = bincode::serialize(&req).unwrap();
        let decoded: CapacityRequestPayload = bincode::deserialize(&encoded).unwrap();
        assert_eq!(decoded.requester_id, RouterNodeId([7u8; 32]));
    }

    #[tokio::test]
    async fn pending_insert_and_complete() {
        let pending = PendingRequests::new();
        let request_id = [1u8; 32];
        let origin = RouterNodeId([2u8; 32]);
        let (tx, rx) = tokio::sync::oneshot::channel();
        pending.insert(request_id, tx, origin);
        assert_eq!(pending.origin(request_id), Some(origin));
        pending.complete(request_id, b"response".to_vec());
        let outcome = rx.await.unwrap();
        match outcome {
            ForwardOutcome::Completed(data) => assert_eq!(data, b"response".to_vec()),
            _ => panic!("expected Completed"),
        }
    }

    #[tokio::test]
    async fn pending_insert_and_reject() {
        let pending = PendingRequests::new();
        let request_id = [3u8; 32];
        let (tx, rx) = tokio::sync::oneshot::channel();
        pending.insert(request_id, tx, RouterNodeId([0u8; 32]));
        pending.reject(request_id, ForwardRejectReason::NoProvider);
        let outcome = rx.await.unwrap();
        assert!(matches!(
            outcome,
            ForwardOutcome::Rejected(ForwardRejectReason::NoProvider)
        ));
    }

    #[tokio::test]
    async fn pending_cancel() {
        let pending = PendingRequests::new();
        let request_id = [4u8; 32];
        let (tx, rx) = tokio::sync::oneshot::channel();
        pending.insert(request_id, tx, RouterNodeId([0u8; 32]));
        pending.cancel(request_id);
        assert!(pending.origin(request_id).is_none());
        assert!(rx.await.is_err());
    }

    #[tokio::test]
    async fn pending_origin_lookup() {
        let pending = PendingRequests::new();
        let id_a = [10u8; 32];
        let id_b = [11u8; 32];
        let origin_a = RouterNodeId([20u8; 32]);
        let origin_b = RouterNodeId([21u8; 32]);
        let (tx1, _) = tokio::sync::oneshot::channel();
        let (tx2, _) = tokio::sync::oneshot::channel();
        pending.insert(id_a, tx1, origin_a);
        pending.insert(id_b, tx2, origin_b);
        assert_eq!(pending.origin(id_a), Some(origin_a));
        assert_eq!(pending.origin(id_b), Some(origin_b));
        assert_eq!(pending.origin([99u8; 32]), None);
    }

    #[test]
    fn forward_reject_all_variants() {
        let variants = [
            ForwardRejectReason::TtlExpired,
            ForwardRejectReason::NoProvider,
            ForwardRejectReason::ModelNotSupported,
            ForwardRejectReason::CapacityExhausted,
            ForwardRejectReason::ContextWindowExceeded,
            ForwardRejectReason::BudgetExceeded,
            ForwardRejectReason::AuthFailure,
            ForwardRejectReason::PayloadTooLarge,
        ];
        for v in &variants {
            let encoded = bincode::serialize(v).unwrap();
            let decoded: ForwardRejectReason = bincode::deserialize(&encoded).unwrap();
            std::mem::discriminant(v);
            assert_eq!(std::mem::discriminant(v), std::mem::discriminant(&decoded));
        }
    }
}
