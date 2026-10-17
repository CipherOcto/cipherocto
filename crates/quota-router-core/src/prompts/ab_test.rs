// A/B testing for prompt variants.
//
// `AbTest` exposes a deterministic selector (`select_version`) keyed on a
// request_id. Counter / accumulator state lives in `AbTestMetricsAtomic`
// so the registry can increment counters from multiple worker threads
// without taking a write lock on the registry itself.
//
// `AbTestMetrics` is the serde-stable snapshot. `AbTestMetricsAtomic`
// provides lock-free updates via `Arc<AtomicU64>` for each counter and
// `to_bits`/`from_bits` for f64 accumulators.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Serde-stable snapshot of A/B metrics. Used for persistence + over the
/// wire. Counter updates happen on `AbTestMetricsAtomic`; the snapshot is
/// taken at serialize time or when reading from storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestMetrics {
    pub requests_a: u64,
    pub requests_b: u64,
    pub avg_latency_a: f64,
    pub avg_latency_b: f64,
    pub error_rate_a: f64,
    pub error_rate_b: f64,
    pub avg_tokens_a: u64,
    pub avg_tokens_b: u64,
}

impl Default for AbTestMetrics {
    fn default() -> Self {
        Self {
            requests_a: 0,
            requests_b: 0,
            avg_latency_a: 0.0,
            avg_latency_b: 0.0,
            error_rate_a: 0.0,
            error_rate_b: 0.0,
            avg_tokens_a: 0,
            avg_tokens_b: 0,
        }
    }
}

/// Lock-free counter + accumulator state for A/B metrics.
///
/// Counters use `AtomicU64`. f64 accumulators encode their bit
/// representation in `AtomicU64` so updates remain atomic. Callers that
/// need to read a coherent snapshot should use `snapshot()` which reads
/// each field once (the values may drift across fields under concurrent
/// update; this is acceptable for telemetry).
#[derive(Debug)]
pub struct AbTestMetricsAtomic {
    requests_a: AtomicU64,
    requests_b: AtomicU64,
    avg_latency_a: AtomicU64,
    avg_latency_b: AtomicU64,
    error_rate_a: AtomicU64,
    error_rate_b: AtomicU64,
    avg_tokens_a: AtomicU64,
    avg_tokens_b: AtomicU64,
}

impl Default for AbTestMetricsAtomic {
    fn default() -> Self {
        Self::from_snapshot(&AbTestMetrics::default())
    }
}

impl AbTestMetricsAtomic {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build an atomic metrics holder from a snapshot. Each field copies
    /// the snapshot value into a fresh `AtomicU64` (f64 values via
    /// `to_bits`).
    pub fn from_snapshot(s: &AbTestMetrics) -> Self {
        Self {
            requests_a: AtomicU64::new(s.requests_a),
            requests_b: AtomicU64::new(s.requests_b),
            avg_latency_a: AtomicU64::new(s.avg_latency_a.to_bits()),
            avg_latency_b: AtomicU64::new(s.avg_latency_b.to_bits()),
            error_rate_a: AtomicU64::new(s.error_rate_a.to_bits()),
            error_rate_b: AtomicU64::new(s.error_rate_b.to_bits()),
            avg_tokens_a: AtomicU64::new(s.avg_tokens_a),
            avg_tokens_b: AtomicU64::new(s.avg_tokens_b),
        }
    }

    /// Take a coherent-enough snapshot. Reads each atomic once. Field
    /// ordering is not synchronized, so concurrent updates can produce a
    /// mixed snapshot — acceptable for telemetry.
    pub fn snapshot(&self) -> AbTestMetrics {
        AbTestMetrics {
            requests_a: self.requests_a.load(Ordering::Relaxed),
            requests_b: self.requests_b.load(Ordering::Relaxed),
            avg_latency_a: f64::from_bits(self.avg_latency_a.load(Ordering::Relaxed)),
            avg_latency_b: f64::from_bits(self.avg_latency_b.load(Ordering::Relaxed)),
            error_rate_a: f64::from_bits(self.error_rate_a.load(Ordering::Relaxed)),
            error_rate_b: f64::from_bits(self.error_rate_b.load(Ordering::Relaxed)),
            avg_tokens_a: self.avg_tokens_a.load(Ordering::Relaxed),
            avg_tokens_b: self.avg_tokens_b.load(Ordering::Relaxed),
        }
    }

    pub fn inc_requests_a(&self) {
        self.requests_a.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_requests_b(&self) {
        self.requests_b.fetch_add(1, Ordering::Relaxed);
    }
    pub fn add_latency_a(&self, latency_ms: f64) {
        self.avg_latency_a
            .store(latency_ms.to_bits(), Ordering::Relaxed);
    }
    pub fn add_latency_b(&self, latency_ms: f64) {
        self.avg_latency_b
            .store(latency_ms.to_bits(), Ordering::Relaxed);
    }
    pub fn set_error_rate_a(&self, rate: f64) {
        self.error_rate_a.store(rate.to_bits(), Ordering::Relaxed);
    }
    pub fn set_error_rate_b(&self, rate: f64) {
        self.error_rate_b.store(rate.to_bits(), Ordering::Relaxed);
    }
    pub fn add_tokens_a(&self, tokens: u64) {
        self.avg_tokens_a.fetch_add(tokens, Ordering::Relaxed);
    }
    pub fn add_tokens_b(&self, tokens: u64) {
        self.avg_tokens_b.fetch_add(tokens, Ordering::Relaxed);
    }
}

impl Serialize for AbTestMetricsAtomic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.snapshot().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AbTestMetricsAtomic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let snap = AbTestMetrics::deserialize(deserializer)?;
        Ok(Self::from_snapshot(&snap))
    }
}

/// Identifies which arm of an A/B test a request resolved to. Used when
/// recording outcomes so the caller doesn't have to compare version
/// strings (which are operator-chosen and arbitrary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbArm {
    A,
    B,
}

/// A/B test definition: two prompt versions + weight for version B.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTest {
    pub prompt_id: String,
    pub version_a: String,
    pub version_b: String,
    pub weight_b: f64,
    pub start_at: DateTime<Utc>,
    pub end_at: Option<DateTime<Utc>>,
    /// Concurrent counter state. `Arc` so updates from the proxy worker
    /// pool don't require cloning the metrics struct.
    pub metrics: Arc<AbTestMetricsAtomic>,
}

impl AbTest {
    /// Build an A/B test from the wire-stable fields + a fresh atomic
    /// metrics holder initialized from `metrics_snapshot`.
    pub fn new(
        prompt_id: String,
        version_a: String,
        version_b: String,
        weight_b: f64,
        start_at: DateTime<Utc>,
        end_at: Option<DateTime<Utc>>,
        metrics_snapshot: &AbTestMetrics,
    ) -> Self {
        Self {
            prompt_id,
            version_a,
            version_b,
            weight_b,
            start_at,
            end_at,
            metrics: Arc::new(AbTestMetricsAtomic::from_snapshot(metrics_snapshot)),
        }
    }

    /// Select version based on deterministic hashing of `request_id`.
    /// If the test has ended, returns `version_a` (control fallback).
    pub fn select_version(&self, request_id: &str) -> &str {
        if let Some(end_at) = self.end_at {
            if Utc::now() > end_at {
                return &self.version_a;
            }
        }
        let hash = simple_hash(request_id);
        if (hash % 1000) as f64 / 1000.0 < self.weight_b {
            &self.version_b
        } else {
            &self.version_a
        }
    }
}

/// DJB2-like hash; deterministic across runs.
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_metrics_default_zero() {
        let m = AbTestMetricsAtomic::default();
        let snap = m.snapshot();
        assert_eq!(snap.requests_a, 0);
        assert_eq!(snap.requests_b, 0);
        assert_eq!(snap.avg_latency_a, 0.0);
    }

    #[test]
    fn test_atomic_metrics_inc_requests_concurrent() {
        use std::thread;
        let m = Arc::new(AbTestMetricsAtomic::default());
        let mut handles = vec![];
        for _ in 0..8 {
            let m2 = m.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    m2.inc_requests_a();
                    m2.inc_requests_b();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let snap = m.snapshot();
        assert_eq!(snap.requests_a, 8000);
        assert_eq!(snap.requests_b, 8000);
    }

    #[test]
    fn test_atomic_metrics_from_snapshot_round_trip() {
        let snap = AbTestMetrics {
            requests_a: 42,
            requests_b: 17,
            avg_latency_a: 123.45,
            avg_latency_b: 678.9,
            error_rate_a: 0.01,
            error_rate_b: 0.02,
            avg_tokens_a: 1000,
            avg_tokens_b: 2000,
        };
        let m = AbTestMetricsAtomic::from_snapshot(&snap);
        let snap2 = m.snapshot();
        assert_eq!(snap.requests_a, snap2.requests_a);
        assert_eq!(snap.requests_b, snap2.requests_b);
        assert_eq!(snap.avg_latency_a, snap2.avg_latency_a);
        assert_eq!(snap.avg_latency_b, snap2.avg_latency_b);
        assert_eq!(snap.error_rate_a, snap2.error_rate_a);
        assert_eq!(snap.error_rate_b, snap2.error_rate_b);
        assert_eq!(snap.avg_tokens_a, snap2.avg_tokens_a);
        assert_eq!(snap.avg_tokens_b, snap2.avg_tokens_b);
    }

    #[test]
    fn test_atomic_metrics_serde_round_trip() {
        let m = AbTestMetricsAtomic::default();
        m.inc_requests_a();
        m.inc_requests_b();
        m.add_latency_a(50.0);
        m.add_tokens_a(123);
        let json = serde_json::to_string(&m).unwrap();
        let m2: AbTestMetricsAtomic = serde_json::from_str(&json).unwrap();
        let snap = m2.snapshot();
        assert_eq!(snap.requests_a, 1);
        assert_eq!(snap.requests_b, 1);
        assert_eq!(snap.avg_latency_a, 50.0);
        assert_eq!(snap.avg_tokens_a, 123);
    }

    #[test]
    fn test_ab_test_new_constructs_atomic_metrics() {
        let snap = AbTestMetrics {
            requests_a: 5,
            ..AbTestMetrics::default()
        };
        let test = AbTest::new(
            "p1".to_string(),
            "1.0.0".to_string(),
            "2.0.0".to_string(),
            0.5,
            Utc::now(),
            None,
            &snap,
        );
        assert_eq!(test.metrics.snapshot().requests_a, 5);
        assert_eq!(test.select_version("req-1"), test.select_version("req-1"));
    }

    #[test]
    fn test_ab_test_deterministic_selection() {
        let test = AbTest::new(
            "test".to_string(),
            "1.0.0".to_string(),
            "2.0.0".to_string(),
            0.5,
            Utc::now(),
            None,
            &AbTestMetrics::default(),
        );
        let v1 = test.select_version("req-123");
        let v2 = test.select_version("req-123");
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_ab_test_weight_boundaries() {
        let mut test = AbTest::new(
            "test".to_string(),
            "1.0.0".to_string(),
            "2.0.0".to_string(),
            0.0,
            Utc::now(),
            None,
            &AbTestMetrics::default(),
        );
        assert_eq!(test.select_version("any"), "1.0.0");

        test.weight_b = 1.0;
        assert_eq!(test.select_version("any"), "2.0.0");
    }

    #[test]
    fn test_ab_test_ended_fallback() {
        let test = AbTest::new(
            "test".to_string(),
            "1.0.0".to_string(),
            "2.0.0".to_string(),
            1.0,
            Utc::now() - chrono::Duration::hours(2),
            Some(Utc::now() - chrono::Duration::hours(1)),
            &AbTestMetrics::default(),
        );
        assert_eq!(test.select_version("any"), "1.0.0");
    }

    #[test]
    fn test_simple_hash_deterministic() {
        assert_eq!(simple_hash("hello"), simple_hash("hello"));
        assert_ne!(simple_hash("hello"), simple_hash("world"));
    }
}
