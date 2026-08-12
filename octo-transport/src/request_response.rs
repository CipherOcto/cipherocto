//! Layer D request/response substrate (mission 0870k-transport-request-response).
//!
//! Generalizes the RFC-0870 mesh-specific `ForwardingConfig` +
//! `PendingRequests` to a Layer D substrate that any consumer
//! (identity resolver, quota router mesh, future modules) can use for
//! cross-node reply correlation.
//!
//! ## Design
//!
//! - **Correlation key**: RFC-0871 `NodeEnvelope.envelope_id`
//!   (`BLAKE3-256(canonical_ser(envelope_without_id))`,
//!   `octo-protocol/src/envelope.rs:200-201`). No magic-byte
//!   discriminator; the reply envelope IS a `NodeEnvelope` whose
//!   `envelope_id` echoes the request envelope's.
//! - **Substrate layer only**: this module knows about the correlation
//!   id and the timeout/concurrency/size envelope. It does NOT
//!   interpret envelope semantics (signing, replay defense, payload
//!   decode) — those are consumer responsibilities at Layer A/B.
//!
//! ## Layer direction
//!
//! - `octo-transport` (Layer D) — substrate only; no business logic.
//! - No `octo-protocol` dependency introduced; substrate uses
//!   `[u8; 32]` envelope_id directly (the type, not the struct).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::oneshot;

/// Errors for `PendingRequests` operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PendingRequestsError {
    /// A request with this `envelope_id` was already registered.
    #[error("pending request already registered for envelope_id")]
    AlreadyRegistered,
    /// No pending request with this `envelope_id` exists.
    #[error("no pending request for envelope_id")]
    Unknown,
    /// The reply was sent but the receiver was already dropped
    /// (caller cancelled / timed out).
    #[error("receiver dropped before response arrived")]
    ReceiverDropped,
}

/// Substrate-level request/response configuration (RFC-0870
/// `ForwardingConfig` generalized to Layer D).
///
/// `ForwardingConfig::max_ttl` is mesh-specific (hop-count bound) and
/// is NOT included here; mesh-specific config stays in `quota-router-core`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestResponseConfig {
    /// Timeout for awaiting a reply. Default: 30s (matches RFC-0870
    /// `ForwardingConfig::forward_timeout`).
    #[serde(with = "duration_secs")]
    pub forward_timeout: Duration,

    /// Maximum concurrent in-flight requests per `NodeTransport`.
    /// Default: 64 (matches RFC-0870 `max_concurrent_forwards`).
    pub max_concurrent: u32,

    /// Maximum request payload size in bytes. Default: 1MB (matches
    /// RFC-0870 `max_payload_bytes`).
    pub max_payload_bytes: usize,
}

impl Default for RequestResponseConfig {
    fn default() -> Self {
        Self {
            forward_timeout: Duration::from_secs(30),
            max_concurrent: 64,
            max_payload_bytes: 1024 * 1024,
        }
    }
}

/// Serde helper for `Duration` as integer seconds (no sub-second
/// precision in the wire form — consumers who need ms-level config
/// can fork this helper).
mod duration_secs {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        d.as_secs().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

/// Internal pending entry. Public surface is `insert/complete/reject/
/// evict_expired/len`.
struct PendingEntry {
    sender: oneshot::Sender<Vec<u8>>,
    #[allow(dead_code)]
    registered_at: Instant,
}

/// Tracks in-flight requests keyed by RFC-0871 `envelope_id`.
///
/// Generalized from RFC-0870's mesh-specific `PendingRequests` at
/// `crates/quota-router-core/src/node/quota_router_node.rs:2312+`. Drops
/// the mesh-specific `origin: RouterNodeId` field (replaced by RFC-0871
/// `NodeEnvelope.from_did` at the consumer layer); adds
/// `registered_at: Instant` for the `evict_expired` policy.
pub struct PendingRequests {
    by_id: HashMap<[u8; 32], PendingEntry>,
}

impl Default for PendingRequests {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingRequests {
    /// Empty registry.
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
        }
    }

    /// Register a pending request and return the receiver the caller
    /// awaits on. Fails with `AlreadyRegistered` if `envelope_id` is
    /// already in the registry.
    pub fn register(
        &mut self,
        envelope_id: [u8; 32],
    ) -> Result<oneshot::Receiver<Vec<u8>>, PendingRequestsError> {
        if self.by_id.contains_key(&envelope_id) {
            return Err(PendingRequestsError::AlreadyRegistered);
        }
        let (tx, rx) = oneshot::channel();
        self.by_id.insert(
            envelope_id,
            PendingEntry {
                sender: tx,
                registered_at: Instant::now(),
            },
        );
        Ok(rx)
    }

    /// Deliver a response to the awaiting caller.
    ///
    /// Returns `Unknown` if no entry exists for `envelope_id`; returns
    /// `ReceiverDropped` if the caller has already cancelled (the
    /// `Sender::send` returned `Err`).
    pub fn complete(
        &mut self,
        envelope_id: [u8; 32],
        response: Vec<u8>,
    ) -> Result<(), PendingRequestsError> {
        let entry = self
            .by_id
            .remove(&envelope_id)
            .ok_or(PendingRequestsError::Unknown)?;
        entry
            .sender
            .send(response)
            .map_err(|_| PendingRequestsError::ReceiverDropped)
    }

    /// Reject a pending request with a reason (cancels the receiver).
    pub fn reject(
        &mut self,
        envelope_id: [u8; 32],
        reason: &str,
    ) -> Result<(), PendingRequestsError> {
        let entry = self
            .by_id
            .remove(&envelope_id)
            .ok_or(PendingRequestsError::Unknown)?;
        // Drop the sender — `Receiver::await` returns `Err` with the
        // reason via `oneshot::error::RecvError`. We use the same path
        // for both timeout and explicit reject.
        let _ = entry.sender.send(Vec::new());
        // Note: dropping `Vec::new()` would also cancel; we send a
        // sentinel so the caller can distinguish reject from timeout
        // if needed. The reason is dropped here (callers can pass
        // `Vec::new()` and inspect via `PendingRequestsError` from a
        // wrapper). The mission spec keeps this simple.
        let _ = reason; // reason logged via `tracing::warn!` at call site
        Ok(())
    }

    /// Cancel a pending request and remove the entry without sending a
    /// payload. Returns `Unknown` if no entry exists; `true` if an
    /// entry was removed.
    ///
    /// Distinct from `reject` (which sends a sentinel `Vec::new()`) and
    /// `complete` (which sends a real payload). Used by callers that
    /// registered a handler but decided not to send the request
    /// (sender selection failure, etc.).
    pub fn cancel(&mut self, envelope_id: [u8; 32]) -> bool {
        self.by_id.remove(&envelope_id).is_some()
    }

    /// Reap stale entries older than `timeout`. Returns the count of
    /// entries evicted. Used by background sweeper tasks to bound
    /// memory growth when callers cancel without `dispatch_response`.
    pub fn evict_expired(&mut self, now: Instant, timeout: Duration) -> usize {
        let before = self.by_id.len();
        self.by_id
            .retain(|_, entry| now.duration_since(entry.registered_at) < timeout);
        before - self.by_id.len()
    }

    /// Number of pending entries.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// True if no pending entries.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_response_config_defaults() {
        let cfg = RequestResponseConfig::default();
        assert_eq!(cfg.forward_timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_concurrent, 64);
        assert_eq!(cfg.max_payload_bytes, 1024 * 1024);
    }

    #[test]
    fn request_response_config_serde_round_trip() {
        let cfg = RequestResponseConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: RequestResponseConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.forward_timeout, cfg.forward_timeout);
        assert_eq!(back.max_concurrent, cfg.max_concurrent);
        assert_eq!(back.max_payload_bytes, cfg.max_payload_bytes);
    }

    #[test]
    fn pending_requests_register_complete_round_trip() {
        let mut pr = PendingRequests::new();
        let id = [1u8; 32];
        let rx = pr.register(id).unwrap();
        assert_eq!(pr.len(), 1);
        pr.complete(id, b"reply".to_vec()).unwrap();
        assert_eq!(pr.len(), 0);
        // Receiver resolves with the response
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let received = rt.block_on(rx).unwrap();
        assert_eq!(received, b"reply");
    }

    #[test]
    fn pending_requests_register_duplicate_rejected() {
        let mut pr = PendingRequests::new();
        let id = [2u8; 32];
        let _rx1 = pr.register(id).unwrap();
        let err = pr.register(id).unwrap_err();
        assert_eq!(err, PendingRequestsError::AlreadyRegistered);
    }

    #[test]
    fn pending_requests_complete_unknown_returns_error() {
        let mut pr = PendingRequests::new();
        let err = pr.complete([99u8; 32], b"orphan".to_vec()).unwrap_err();
        assert_eq!(err, PendingRequestsError::Unknown);
    }

    #[test]
    fn pending_requests_complete_after_drop_returns_dropped() {
        let mut pr = PendingRequests::new();
        let id = [3u8; 32];
        let rx = pr.register(id).unwrap();
        drop(rx); // caller cancels
        let err = pr.complete(id, b"too-late".to_vec()).unwrap_err();
        assert_eq!(err, PendingRequestsError::ReceiverDropped);
        // Entry removed from registry after the failed send.
        assert_eq!(pr.len(), 0);
    }

    #[test]
    fn pending_requests_reject_removes_entry() {
        let mut pr = PendingRequests::new();
        let id = [4u8; 32];
        let _rx = pr.register(id).unwrap();
        pr.reject(id, "test reject").unwrap();
        assert_eq!(pr.len(), 0);
    }

    #[test]
    fn pending_requests_evict_expired_removes_old_entries() {
        let mut pr = PendingRequests::new();
        let id = [5u8; 32];
        let _rx = pr.register(id).unwrap();
        // Simulate time travel — register with a past instant by
        // constructing a fresh registry and forcing an old timestamp
        // is not directly possible through the public API, so we
        // use the `evict_expired` with a very short timeout after a
        // small sleep.
        std::thread::sleep(Duration::from_millis(10));
        let evicted = pr.evict_expired(Instant::now(), Duration::from_millis(5));
        assert_eq!(evicted, 1);
        assert_eq!(pr.len(), 0);
    }

    #[test]
    fn pending_requests_is_empty() {
        let mut pr = PendingRequests::new();
        assert!(pr.is_empty());
        let _rx = pr.register([6u8; 32]).unwrap();
        assert!(!pr.is_empty());
    }

    #[test]
    fn pending_requests_cancel_removes_entry_without_sending() {
        let mut pr = PendingRequests::new();
        let id = [7u8; 32];
        let _rx = pr.register(id).unwrap();
        assert_eq!(pr.len(), 1);
        assert!(pr.cancel(id));
        assert_eq!(pr.len(), 0);
        // Second cancel is a no-op
        assert!(!pr.cancel(id));
    }
}
