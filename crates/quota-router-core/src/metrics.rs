use prometheus::{Encoder, Gauge, Histogram, IntCounter, Opts, Registry, TextEncoder};

/// Prometheus metrics for the quota router (RFC-0937).
pub struct Metrics {
    pub requests_total: IntCounter,
    pub request_duration: Histogram,
    pub request_tokens: IntCounter,
    pub rate_limit_hits: IntCounter,
    pub budget_spend: Gauge,
    pub budget_alerts: IntCounter,
    pub provider_errors: IntCounter,
    pub provider_latency: Histogram,
    pub routing_decisions: IntCounter,
    pub cache_hits: IntCounter,
    pub cache_misses: IntCounter,
    pub precall_check_failures: IntCounter,
    /// Callback delivery events dropped due to a full channel (RFC-0947).
    /// Wired via `CallbackExecutor::install_dropped_counter` at startup.
    pub callback_dropped_total: IntCounter,
    registry: Registry,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let requests_total = IntCounter::with_opts(Opts::new(
            "requests_total",
            "Total number of proxy requests",
        ))
        .unwrap();

        let request_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "request_duration_seconds",
                "Request duration in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
        )
        .unwrap();

        let request_tokens =
            IntCounter::with_opts(Opts::new("request_tokens_total", "Total tokens processed"))
                .unwrap();

        let rate_limit_hits = IntCounter::with_opts(Opts::new(
            "rate_limit_hits_total",
            "Total rate limit rejections",
        ))
        .unwrap();

        let budget_spend = Gauge::with_opts(Opts::new(
            "budget_spend_microdollars",
            "Current budget spend in microdollars",
        ))
        .unwrap();

        let budget_alerts = IntCounter::with_opts(Opts::new(
            "budget_alerts_total",
            "Total budget alerts fired",
        ))
        .unwrap();

        let provider_errors =
            IntCounter::with_opts(Opts::new("provider_errors_total", "Total provider errors"))
                .unwrap();

        let provider_latency = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "provider_latency_seconds",
                "Provider response latency in seconds",
            )
            .buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]),
        )
        .unwrap();

        let routing_decisions = IntCounter::with_opts(Opts::new(
            "routing_decisions_total",
            "Total routing decisions made",
        ))
        .unwrap();

        let cache_hits =
            IntCounter::with_opts(Opts::new("cache_hits_total", "Total cache hits")).unwrap();

        let cache_misses =
            IntCounter::with_opts(Opts::new("cache_misses_total", "Total cache misses")).unwrap();

        let precall_check_failures = IntCounter::with_opts(Opts::new(
            "precall_check_failures_total",
            "Total pre-call check failures",
        ))
        .unwrap();

        let callback_dropped_total = IntCounter::with_opts(Opts::new(
            "callback_dropped_total",
            "Callback events dropped due to bounded channel overflow (RFC-0947)",
        ))
        .unwrap();

        registry.register(Box::new(requests_total.clone())).unwrap();
        registry
            .register(Box::new(request_duration.clone()))
            .unwrap();
        registry.register(Box::new(request_tokens.clone())).unwrap();
        registry
            .register(Box::new(rate_limit_hits.clone()))
            .unwrap();
        registry.register(Box::new(budget_spend.clone())).unwrap();
        registry.register(Box::new(budget_alerts.clone())).unwrap();
        registry
            .register(Box::new(provider_errors.clone()))
            .unwrap();
        registry
            .register(Box::new(provider_latency.clone()))
            .unwrap();
        registry
            .register(Box::new(routing_decisions.clone()))
            .unwrap();
        registry.register(Box::new(cache_hits.clone())).unwrap();
        registry.register(Box::new(cache_misses.clone())).unwrap();
        registry
            .register(Box::new(precall_check_failures.clone()))
            .unwrap();
        registry
            .register(Box::new(callback_dropped_total.clone()))
            .unwrap();

        Self {
            requests_total,
            request_duration,
            request_tokens,
            rate_limit_hits,
            budget_spend,
            budget_alerts,
            provider_errors,
            provider_latency,
            routing_decisions,
            cache_hits,
            cache_misses,
            precall_check_failures,
            callback_dropped_total,
            registry,
        }
    }

    /// Encode all metrics as Prometheus text format.
    pub fn encode(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = Metrics::new();
        assert_eq!(metrics.requests_total.get(), 0);
        assert_eq!(metrics.rate_limit_hits.get(), 0);
        assert_eq!(metrics.provider_errors.get(), 0);
        assert_eq!(metrics.routing_decisions.get(), 0);
        assert_eq!(metrics.cache_hits.get(), 0);
        assert_eq!(metrics.cache_misses.get(), 0);
        assert_eq!(metrics.precall_check_failures.get(), 0);
        assert_eq!(metrics.budget_alerts.get(), 0);
        assert_eq!(metrics.callback_dropped_total.get(), 0);
    }

    #[test]
    fn test_metrics_default() {
        let metrics = Metrics::default();
        assert_eq!(metrics.requests_total.get(), 0);
    }

    #[test]
    fn test_metrics_encode() {
        let metrics = Metrics::new();
        let encoded = metrics.encode();
        assert!(encoded.contains("requests_total"));
        assert!(encoded.contains("rate_limit_hits_total"));
        assert!(encoded.contains("provider_errors_total"));
        assert!(encoded.contains("callback_dropped_total"));
    }

    #[test]
    fn test_metrics_counter_increment() {
        let metrics = Metrics::new();
        metrics.requests_total.inc();
        metrics.requests_total.inc();
        assert_eq!(metrics.requests_total.get(), 2);

        metrics.rate_limit_hits.inc();
        assert_eq!(metrics.rate_limit_hits.get(), 1);
    }

    #[test]
    fn test_metrics_histogram_observe() {
        let metrics = Metrics::new();
        metrics.request_duration.observe(0.5);
        metrics.request_duration.observe(1.0);
        assert_eq!(metrics.request_duration.get_sample_count(), 2);
    }

    #[test]
    fn test_metrics_gauge_set() {
        let metrics = Metrics::new();
        metrics.budget_spend.set(100.0);
        assert_eq!(metrics.budget_spend.get(), 100.0);
        metrics.budget_spend.set(200.0);
        assert_eq!(metrics.budget_spend.get(), 200.0);
    }
}
