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
