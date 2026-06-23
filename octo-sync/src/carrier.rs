//! Cross-carrier sync (per RFC-0862 Phase 4, mission 0862g).
//!
//! Fans out a single Sync envelope to multiple `Carrier` implementations
//! (e.g., NativeP2P + Webhook + one social adapter). Each carrier's
//! `send()` is called; the broadcaster returns the count of successful
//! sends. Health tracking is per-carrier; a carrier with success_rate < 0.5
//! is considered unhealthy and skipped.
//!
//! # Production architecture
//!
//! ```text
//! MultiCarrierSync
//!   ├── primary: Box<dyn Carrier>
//!   ├── secondaries: Vec<Box<dyn Carrier>>
//!   └── health: HashMap<carrier_name, CarrierHealth>
//!
//! broadcast(envelope):
//!   for carrier in healthy_carriers:
//!     let result = carrier.send(envelope).await
//!     update_health(carrier, result)
//!   return count of successes
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

use crate::error::SyncError;

/// A transport carrier for the cipherocto sync envelope.
///
/// Implementations wrap a `PlatformAdapter` (from `octo-network`) and handle
/// the actual wire transmission. The carrier is async because it does
/// network I/O; the cipherocto async runtime awaits the send.
#[async_trait::async_trait]
pub trait Carrier: Send + Sync {
    /// Return the carrier name (e.g., "nativep2p", "webhook", "telegram").
    fn name(&self) -> &str;

    /// Send an envelope. Returns `Ok(())` on success, or `Err(SyncError)`
    /// on failure. The error is logged into the carrier's health stats.
    async fn send(&self, envelope: &[u8]) -> Result<(), SyncError>;
}

/// Per-carrier health tracking.
#[derive(Debug, Clone)]
pub struct CarrierHealth {
    /// The carrier name (e.g., "nativep2p", "webhook", "telegram").
    pub name: String,
    /// The last heartbeat timestamp.
    pub last_heartbeat: Instant,
    /// The last successful send timestamp.
    pub last_successful_send: Instant,
    /// The success rate over the last N attempts (0.0 to 1.0).
    pub success_rate: f64,
    /// The average latency in milliseconds over the last N attempts.
    pub avg_latency_ms: f64,
    /// The last error (if any).
    pub last_error: Option<String>,
    /// EMA alpha for the health stats (0.0 to 1.0). Higher alpha = more
    /// weight on recent samples (faster reaction to changes but more
    /// noise); lower alpha = more weight on history (smoother but slower
    /// to react). Default: 0.1 (10% on new samples, 90% on history).
    pub alpha: f64,
    /// Health threshold: a carrier with `success_rate < health_threshold` is
    /// considered unhealthy and is skipped by `broadcast`. Default: 0.5
    /// (matches RFC-0862 §Performance Targets "≥ 50% success over 100 attempts").
    pub health_threshold: f64,
}

impl CarrierHealth {
    /// Create a new `CarrierHealth` with default values (perfect health).
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_params(name, DEFAULT_EMA_ALPHA, DEFAULT_HEALTH_THRESHOLD)
    }

    /// Create a new `CarrierHealth` with custom EMA alpha and health threshold.
    pub fn with_params(name: impl Into<String>, alpha: f64, health_threshold: f64) -> Self {
        let now = Instant::now();
        Self {
            name: name.into(),
            last_heartbeat: now,
            last_successful_send: now,
            success_rate: 1.0,
            avg_latency_ms: 0.0,
            last_error: None,
            alpha,
            health_threshold,
        }
    }

    /// Return `true` if the carrier is healthy (success rate ≥ threshold).
    pub fn is_healthy(&self) -> bool {
        self.success_rate >= self.health_threshold
    }

    /// Update the health stats after a send attempt.
    pub fn record_attempt(&mut self, success: bool, latency_ms: f64, error: Option<String>) {
        // Exponential moving average with `self.alpha` (default 0.1).
        // 10% weight on new samples, 90% on history.
        let alpha = self.alpha;
        self.success_rate =
            (1.0 - alpha) * self.success_rate + alpha * if success { 1.0 } else { 0.0 };
        self.avg_latency_ms = (1.0 - alpha) * self.avg_latency_ms + alpha * latency_ms;
        if success {
            self.last_successful_send = Instant::now();
            self.last_error = None;
        } else {
            self.last_error = error;
        }
    }
}

/// Default EMA alpha for `CarrierHealth` (10% weight on new samples).
pub const DEFAULT_EMA_ALPHA: f64 = 0.1;

/// Default health threshold (RFC-0862 §Performance Targets:
/// "≥ 50% success over 100 attempts" → 0.5).
pub const DEFAULT_HEALTH_THRESHOLD: f64 = 0.5;

/// A multi-carrier sync broadcaster.
///
/// Holds a list of carriers and per-carrier health stats. `broadcast` fans
/// out an envelope to all healthy carriers concurrently and returns the
/// count of successful sends.
pub struct MultiCarrierSync {
    /// The carriers (primary + secondaries).
    carriers: Vec<Arc<dyn Carrier>>,
    /// Per-carrier health stats.
    health: Mutex<HashMap<String, CarrierHealth>>,
}

impl MultiCarrierSync {
    /// Create a new `MultiCarrierSync` with the given carriers.
    pub fn new(carriers: Vec<Arc<dyn Carrier>>) -> Self {
        let mut health = HashMap::new();
        for carrier in &carriers {
            health.insert(
                carrier.name().to_string(),
                CarrierHealth::new(carrier.name()),
            );
        }
        Self {
            carriers,
            health: Mutex::new(health),
        }
    }

    /// Broadcast an envelope to all healthy carriers.
    ///
    /// Returns the number of carriers that successfully sent. The function
    /// does NOT block: it uses `tokio::join_all` to send concurrently.
    /// If a carrier is unhealthy (success_rate < 0.5), it is skipped.
    pub async fn broadcast(&self, envelope: &[u8]) -> usize {
        // Filter to healthy carriers
        let healthy: Vec<Arc<dyn Carrier>> = {
            let health = self.health.lock();
            self.carriers
                .iter()
                .filter(|c| {
                    health
                        .get(c.name())
                        .map(|h| h.is_healthy())
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        };
        // Send concurrently
        let send_futures = healthy.iter().map(|c| {
            let c = c.clone();
            let envelope = envelope.to_vec();
            async move {
                let start = Instant::now();
                let result = c.send(&envelope).await;
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                (c.name().to_string(), result, latency_ms)
            }
        });
        let results = futures::future::join_all(send_futures).await;
        // Update health and count successes
        let mut health = self.health.lock();
        let mut success_count = 0;
        for (name, result, latency_ms) in results {
            if let Some(h) = health.get_mut(&name) {
                match result {
                    Ok(()) => {
                        h.record_attempt(true, latency_ms, None);
                        success_count += 1;
                    }
                    Err(e) => {
                        h.record_attempt(false, latency_ms, Some(e.to_string()));
                    }
                }
            }
        }
        success_count
    }

    /// Return the list of healthy carrier names.
    pub fn healthy_carrier_names(&self) -> Vec<String> {
        let health = self.health.lock();
        self.carriers
            .iter()
            .filter(|c| {
                health
                    .get(c.name())
                    .map(|h| h.is_healthy())
                    .unwrap_or(false)
            })
            .map(|c| c.name().to_string())
            .collect()
    }

    /// Return the list of all carrier names (healthy and unhealthy).
    pub fn all_carrier_names(&self) -> Vec<String> {
        self.carriers.iter().map(|c| c.name().to_string()).collect()
    }

    /// Return the health stats for a specific carrier.
    pub fn health(&self, name: &str) -> Option<CarrierHealth> {
        self.health.lock().get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test carrier that succeeds or fails based on a configurable flag.
    ///
    /// `succeed_count` is the number of times `send` returns `Ok`; after that
    /// it always returns `Err(SyncError::AllCarriersFailed)`. Uses `Mutex<usize>`
    /// to avoid atomic underflow bugs.
    struct TestCarrier {
        name: String,
        succeed_remaining: Mutex<usize>,
    }

    impl TestCarrier {
        fn new(name: &str, succeed_count: usize) -> Self {
            Self {
                name: name.to_string(),
                succeed_remaining: Mutex::new(succeed_count),
            }
        }
    }

    #[async_trait::async_trait]
    impl Carrier for TestCarrier {
        fn name(&self) -> &str {
            &self.name
        }
        async fn send(&self, _envelope: &[u8]) -> Result<(), SyncError> {
            let mut n = self.succeed_remaining.lock();
            if *n > 0 {
                *n -= 1;
                Ok(())
            } else {
                Err(SyncError::AllCarriersFailed)
            }
        }
    }

    #[tokio::test]
    async fn healthy_carriers_send() {
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 3));
        let c2: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c2", 3));
        let m = MultiCarrierSync::new(vec![c1, c2]);
        let count = m.broadcast(b"envelope").await;
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn both_carriers_send_when_both_healthy() {
        // Both carriers start with success_rate = 1.0 (healthy), so both
        // are sent to. c1 has 0 successes remaining, so it fails on the
        // first send; c2 has 5, so it succeeds. After this broadcast, c1's
        // success_rate drops to 0.9 (still healthy, but barely).
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 0));
        let c2: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c2", 5));
        let m = MultiCarrierSync::new(vec![c1, c2]);
        let count = m.broadcast(b"envelope").await;
        // Both carriers were sent to (both were healthy). c1 fails, c2 succeeds.
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn carrier_becomes_unhealthy_after_failures() {
        // c1 always fails. After enough broadcasts, its success_rate drops
        // below 0.5 and it should be skipped.
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 0));
        let m = MultiCarrierSync::new(vec![c1]);
        // 20 broadcasts — c1 fails each time. success_rate = 0.9^20 ≈ 0.12
        for _ in 0..20 {
            m.broadcast(b"envelope").await;
        }
        let h = m.health("c1").unwrap();
        assert!(!h.is_healthy());
        // Next broadcast should skip c1 (count = 0)
        let count = m.broadcast(b"envelope").await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn health_updates_after_send() {
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 0));
        let m = MultiCarrierSync::new(vec![c1]);
        // Broadcast 10 times — each time c1 fails, so success_rate drops
        for _ in 0..10 {
            m.broadcast(b"envelope").await;
        }
        let h = m.health("c1").unwrap();
        // After 10 failures, success_rate should be very low
        assert!(h.success_rate < 0.5);
        assert!(!h.is_healthy());
    }

    #[test]
    fn carrier_health_is_healthy_threshold() {
        let mut h = CarrierHealth::new("test");
        assert!(h.is_healthy());
        h.success_rate = 0.5;
        assert!(h.is_healthy());
        h.success_rate = 0.49;
        assert!(!h.is_healthy());
    }

    #[test]
    fn all_carrier_names() {
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 1));
        let c2: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c2", 1));
        let m = MultiCarrierSync::new(vec![c1, c2]);
        let mut names = m.all_carrier_names();
        names.sort();
        assert_eq!(names, vec!["c1", "c2"]);
    }
}
