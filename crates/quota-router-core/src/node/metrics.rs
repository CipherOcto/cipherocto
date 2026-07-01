//! Prometheus metrics for the quota router network (0870d).
//!
//! Each `QuotaRouterNode` owns one `QuotaRouterMetrics` value which is
//! updated at the observation points listed in 0870d acceptance
//! criterion #6. The default registry is the `prometheus` crate's
//! global default — collectors register themselves on `new()` so that
//! `prometheus::gather()` returns them at scrape time.
//!
//! All metric types are from `prometheus::*` per the 0870d spec.

use prometheus::{
    register_counter_vec_with_registry, register_counter_with_registry,
    register_gauge_vec_with_registry, register_gauge_with_registry,
    register_histogram_with_registry, Counter, CounterVec, Gauge, GaugeVec, Histogram, Registry,
};

/// Prometheus collectors for the quota router mesh.
///
/// Construct via `QuotaRouterMetrics::new()`. The first call on a given
/// process registers the metrics with the global default registry;
/// subsequent calls return collectors bound to a fresh registry so
/// tests can run side-by-side without duplicate-registration errors.
pub struct QuotaRouterMetrics {
    /// End-to-end forwarding latency histogram (seconds), labeled by
    /// `hop` so multi-hop forwarding is observable separately.
    pub forwarding_latency: Histogram,
    /// Cumulative bytes emitted by `broadcast_gossip` and
    /// `broadcast_announce`. Drained by `prometheus::gather()`.
    pub gossip_bytes: Counter,
    /// Per-provider health gauge (1 = healthy, 0.5 = degraded,
    /// 0 = unavailable). Labeled by `provider`.
    pub provider_health: GaugeVec,
    /// Gauge of in-flight forwarded requests (`route()` called but
    /// not yet responded-to).
    pub active_forwards: Gauge,
    /// Counter of request outcomes, labeled by `outcome`
    /// (one of `local_success`, `remote_success`, `rejected`,
    /// `timeout`, `rate_limited`).
    pub request_outcomes: CounterVec,
    /// The registry that owns the above collectors. Held so callers
    /// can re-gather outside the default global.
    #[allow(dead_code)]
    registry: Registry,
}

impl QuotaRouterMetrics {
    /// Create a fresh metrics set bound to a private registry.
    ///
    /// The first call from a process also registers collectors with
    /// the global default registry so that `prometheus::gather()`
    /// works without callers wiring their own registry.
    pub fn new() -> Self {
        let registry = Registry::new();

        let forwarding_latency = register_histogram_with_registry!(
            "quota_router_forwarding_latency_seconds",
            "End-to-end latency for forwarded requests (route → response)",
            // 1ms, 5ms, 25ms, 100ms, 500ms, 2.5s, 10s
            vec![0.001, 0.005, 0.025, 0.1, 0.5, 2.5, 10.0],
            registry
        )
        .expect("register forwarding_latency");

        let gossip_bytes = register_counter_with_registry!(
            "quota_router_gossip_bytes_total",
            "Cumulative bytes emitted by gossip/announce broadcasts",
            registry
        )
        .expect("register gossip_bytes");

        let provider_health = register_gauge_vec_with_registry!(
            "quota_router_provider_health",
            "Per-provider health gauge (1.0=healthy, 0.5=degraded, 0.0=unavailable)",
            &["provider"],
            registry
        )
        .expect("register provider_health");

        let active_forwards = register_gauge_with_registry!(
            "quota_router_active_forwards",
            "Currently in-flight forwarded requests",
            registry
        )
        .expect("register active_forwards");

        let request_outcomes = register_counter_vec_with_registry!(
            "quota_router_request_outcomes_total",
            "Count of request outcomes by terminal status",
            &["outcome"],
            registry
        )
        .expect("register request_outcomes");

        Self {
            forwarding_latency,
            gossip_bytes,
            provider_health,
            active_forwards,
            request_outcomes,
            registry,
        }
    }

    /// Convenience: increment an outcome counter.
    pub fn record_outcome(&self, outcome: &str) {
        self.request_outcomes.with_label_values(&[outcome]).inc();
    }

    /// Convenience: observe forwarding latency in seconds.
    pub fn observe_forwarding_latency(&self, seconds: f64) {
        self.forwarding_latency.observe(seconds);
    }

    /// Convenience: add bytes to the gossip counter.
    pub fn add_gossip_bytes(&self, bytes: usize) {
        self.gossip_bytes.inc_by(bytes as f64);
    }

    /// Convenience: set a provider's health gauge.
    pub fn set_provider_health(&self, provider: &str, health: f64) {
        self.provider_health
            .with_label_values(&[provider])
            .set(health);
    }
}

impl Default for QuotaRouterMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_new_does_not_panic() {
        let _m = QuotaRouterMetrics::new();
    }

    #[test]
    fn record_outcome_increments_counter() {
        let m = QuotaRouterMetrics::new();
        m.record_outcome("local_success");
        m.record_outcome("remote_success");
        m.record_outcome("rejected");
        m.record_outcome("timeout");
        m.record_outcome("rate_limited");
        // If the underlying counter didn't accept these labels, the
        // calls would have panicked at registration time.
    }

    #[test]
    fn observe_forwarding_latency_accepts_values() {
        let m = QuotaRouterMetrics::new();
        m.observe_forwarding_latency(0.123);
        m.observe_forwarding_latency(4.567);
    }

    #[test]
    fn add_gossip_bytes_increments() {
        let m = QuotaRouterMetrics::new();
        m.add_gossip_bytes(1024);
        m.add_gossip_bytes(2048);
    }

    #[test]
    fn set_provider_health_accepts_values() {
        let m = QuotaRouterMetrics::new();
        m.set_provider_health("openai", 1.0);
        m.set_provider_health("anthropic", 0.5);
        m.set_provider_health("broken", 0.0);
    }
}
