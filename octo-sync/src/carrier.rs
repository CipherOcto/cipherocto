//! Cross-carrier sync (per RFC-0862 Phase 4, mission 0862g).
//!
//! Fans out a single Sync envelope to multiple `Carrier` implementations
//! (e.g., NativeP2P + Webhook + one social adapter). Each carrier's
//! `send()` is called; the broadcaster returns the count of successful
//! sends. Health tracking is per-carrier; a carrier with success_rate < 50%
//! (5,000 basis points) is considered unhealthy and skipped.
//!
//! # Determinism
//!
//! All health metrics use u64 saturating arithmetic (no floating-point).
//! Success rates are basis points (0-10,000 = 0%-100%). Latency is
//! microseconds (u64). Timestamps are logical unix seconds, not wall-clock.
//! Per RFC-0862 §Determinism.

use std::collections::HashMap;
use std::sync::Arc;

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

/// Per-carrier health tracking (deterministic, no floating-point).
///
/// All values are u64 for deterministic arithmetic per RFC-0862 §Determinism.
/// Success rate is basis points (0-10,000 = 0%-100%). Latency is microseconds.
#[derive(Debug, Clone)]
pub struct CarrierHealth {
    /// The carrier name (e.g., "nativep2p", "webhook", "telegram").
    pub name: String,
    /// Last successful send timestamp (logical unix seconds).
    pub last_successful_send_secs: u64,
    /// Success rate in basis points (0-10,000 = 0%-100%).
    /// EMA with alpha_bp basis points weight on new samples.
    pub success_rate_bp: u64,
    /// Average latency in microseconds (u64, EMA).
    pub avg_latency_us: u64,
    /// The last error (if any).
    pub last_error: Option<String>,
    /// EMA alpha in basis points (0-10,000). Default: 1,000 (10%).
    pub alpha_bp: u64,
    /// Health threshold in basis points. Default: 5,000 (50%).
    pub health_threshold_bp: u64,
}

/// Default EMA alpha: 1,000 basis points = 10% weight on new samples.
pub const DEFAULT_EMA_ALPHA_BP: u64 = 1_000;

/// Default health threshold: 5,000 basis points = 50%.
pub const DEFAULT_HEALTH_THRESHOLD_BP: u64 = 5_000;

/// 10,000 basis points = 100%.
const BP_SCALE: u64 = 10_000;

impl CarrierHealth {
    /// Create a new `CarrierHealth` with default values (perfect health).
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_params(name, DEFAULT_EMA_ALPHA_BP, DEFAULT_HEALTH_THRESHOLD_BP)
    }

    /// Create a new `CarrierHealth` with custom EMA alpha and health threshold.
    pub fn with_params(name: impl Into<String>, alpha_bp: u64, health_threshold_bp: u64) -> Self {
        Self {
            name: name.into(),
            last_successful_send_secs: 0,
            success_rate_bp: BP_SCALE, // 100%
            avg_latency_us: 0,
            last_error: None,
            alpha_bp,
            health_threshold_bp,
        }
    }

    /// Return `true` if the carrier is healthy (success rate >= threshold).
    pub fn is_healthy(&self) -> bool {
        self.success_rate_bp >= self.health_threshold_bp
    }

    /// Update the health stats after a send attempt.
    ///
    /// `success`: whether the send succeeded.
    /// `latency_us`: send latency in microseconds.
    /// `now_secs`: current logical timestamp (unix seconds).
    /// `error`: error message if the send failed.
    pub fn record_attempt(
        &mut self,
        success: bool,
        latency_us: u64,
        now_secs: u64,
        error: Option<String>,
    ) {
        // EMA: new = (1 - alpha) * old + alpha * sample
        // Using basis points: alpha_bp / 10,000 = fractional alpha
        let alpha = self.alpha_bp;
        let one_minus_alpha = BP_SCALE.saturating_sub(alpha);

        if success {
            self.success_rate_bp =
                (one_minus_alpha * self.success_rate_bp + alpha * BP_SCALE) / BP_SCALE;
            self.avg_latency_us =
                (one_minus_alpha * self.avg_latency_us + alpha * latency_us) / BP_SCALE;
            self.last_successful_send_secs = now_secs;
            self.last_error = None;
        } else {
            self.success_rate_bp = (one_minus_alpha * self.success_rate_bp) / BP_SCALE;
            self.avg_latency_us =
                (one_minus_alpha * self.avg_latency_us + alpha * latency_us) / BP_SCALE;
            self.last_error = error;
        }
    }
}

/// A multi-carrier sync broadcaster.
///
/// Holds a list of carriers and per-carrier health stats. `broadcast` fans
/// out an envelope to all healthy carriers concurrently and returns the
/// count of successful sends.
///
/// Optionally holds a `MissionCrypto` for per-mission key isolation.
/// When present, PRIVATE mission payloads are encrypted before sending.
///
/// **Deprecated:** Use `octo_transport::NodeTransport` instead for
/// general-purpose transport. `NodeTransport` provides fan-out, failover,
/// and health tracking via the `NetworkSender` trait.
#[deprecated(
    since = "0.2.0",
    note = "Use octo_transport::NodeTransport instead"
)]
pub struct MultiCarrierSync {
    /// The carriers (primary + secondaries).
    carriers: Vec<Arc<dyn Carrier>>,
    /// Per-carrier health stats.
    health: Mutex<HashMap<String, CarrierHealth>>,
    /// Optional per-mission encryption (Phase 4, mission 0862l).
    crypto: Option<Arc<crate::mission_crypto::MissionCrypto>>,
}

#[allow(deprecated)]
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
            crypto: None,
        }
    }

    /// Create a new `MultiCarrierSync` with carriers and per-mission encryption.
    pub fn with_crypto(
        carriers: Vec<Arc<dyn Carrier>>,
        crypto: Arc<crate::mission_crypto::MissionCrypto>,
    ) -> Self {
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
            crypto: Some(crypto),
        }
    }

    /// Broadcast an envelope to all healthy carriers.
    ///
    /// Returns the number of carriers that successfully sent. The function
    /// does NOT block: it uses `futures::future::join_all` to send concurrently.
    /// If a carrier is unhealthy (success_rate < 5,000 bp = 50%), it is skipped.
    ///
    /// If `crypto` is set (PRIVATE mission), the payload is encrypted before sending.
    /// The 12-byte nonce is prepended to the ciphertext for the receiver.
    pub async fn broadcast(&self, envelope: &[u8]) -> usize {
        // Prepare payload (encrypt if PRIVATE mission)
        let wire_payload = match &self.crypto {
            Some(crypto) => crypto.prepare_for_send(envelope, b"sync-envelope"),
            None => envelope.to_vec(),
        };

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
            let payload = wire_payload.clone();
            async move {
                let start = std::time::Instant::now();
                let result = c.send(&payload).await;
                let latency_us = start.elapsed().as_micros() as u64;
                (c.name().to_string(), result, latency_us)
            }
        });
        let results = futures::future::join_all(send_futures).await;
        // Update health and count successes
        let now_secs = now_unix_secs();
        let mut health = self.health.lock();
        let mut success_count = 0;
        for (name, result, latency_us) in results {
            if let Some(h) = health.get_mut(&name) {
                match result {
                    Ok(()) => {
                        h.record_attempt(true, latency_us, now_secs, None);
                        success_count += 1;
                    }
                    Err(e) => {
                        h.record_attempt(false, latency_us, now_secs, Some(e.to_string()));
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

/// Get current logical timestamp (unix seconds).
///
/// In production, this comes from the DGP logical clock.
/// For tests, it uses system time.
fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[allow(deprecated)]
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
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 0));
        let c2: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c2", 5));
        let m = MultiCarrierSync::new(vec![c1, c2]);
        let count = m.broadcast(b"envelope").await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn carrier_becomes_unhealthy_after_failures() {
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 0));
        let m = MultiCarrierSync::new(vec![c1]);
        for _ in 0..20 {
            m.broadcast(b"envelope").await;
        }
        let h = m.health("c1").unwrap();
        assert!(!h.is_healthy());
        let count = m.broadcast(b"envelope").await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn health_updates_after_send() {
        let c1: Arc<dyn Carrier> = Arc::new(TestCarrier::new("c1", 0));
        let m = MultiCarrierSync::new(vec![c1]);
        for _ in 0..10 {
            m.broadcast(b"envelope").await;
        }
        let h = m.health("c1").unwrap();
        assert!(h.success_rate_bp < 5_000);
        assert!(!h.is_healthy());
    }

    #[test]
    fn carrier_health_is_healthy_threshold() {
        let mut h = CarrierHealth::new("test");
        assert!(h.is_healthy());
        h.success_rate_bp = 5_000;
        assert!(h.is_healthy());
        h.success_rate_bp = 4_999;
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

    #[test]
    fn health_record_attempt_success() {
        let mut h = CarrierHealth::new("test");
        h.record_attempt(true, 1000, 100, None); // 1ms latency, t=100
        assert_eq!(h.success_rate_bp, BP_SCALE); // 100% (EMA: 0.9*10000 + 0.1*10000 = 10000)
        assert_eq!(h.avg_latency_us, 100); // 100us (EMA: 0.9*0 + 0.1*1000 = 100)
        assert_eq!(h.last_successful_send_secs, 100);
        assert!(h.last_error.is_none());
    }

    #[test]
    fn health_record_attempt_failure() {
        let mut h = CarrierHealth::new("test");
        h.record_attempt(false, 5000, 100, Some("timeout".into()));
        assert!(h.success_rate_bp < BP_SCALE);
        assert!(h.last_error.is_some());
    }

    #[test]
    fn health_ema_converges() {
        let mut h = CarrierHealth::with_params("test", 5_000, 5_000); // alpha=50%
                                                                      // After 1 success at 1000us: success_rate = 0.5*10000 + 0.5*10000 = 10000
        h.record_attempt(true, 1000, 0, None);
        assert_eq!(h.success_rate_bp, 10_000);
        // After 1 failure: success_rate = 0.5*10000 + 0.5*0 = 5000
        h.record_attempt(false, 1000, 1, None);
        assert_eq!(h.success_rate_bp, 5_000);
        // After 1 more failure: success_rate = 0.5*5000 + 0.5*0 = 2500
        h.record_attempt(false, 1000, 2, None);
        assert_eq!(h.success_rate_bp, 2_500);
    }
}
