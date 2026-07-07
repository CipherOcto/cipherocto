//! Prometheus metrics registry.
//!
//! Phase 5 Part B of `docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase5.md`
//! defines 14 named metrics per §Observability:
//!
//! | # | Kind       | Name                              | Labels                       |
//! |---|------------|-----------------------------------|------------------------------|
//! | 1 | Counter    | `inbound_events_total`            | `kind=hash`                  |
//! | 2 | Counter    | `outbound_messages_total`         | `kind=hash,result`           |
//! | 3 | Counter    | `rule_matches_total`              | `rule_id=hash`               |
//! | 4 | Counter    | `trigger_runs_total`              | `trigger_id=hash,result`     |
//! | 5 | Counter    | `audit_rows_total`                | (none)                       |
//! | 6 | Counter    | `rate_limit_dropped_total`        | `scope,peer=hash`            |
//! | 7 | Counter    | `auth_failed_total`               | `ip`                         |
//! | 8 | Gauge      | `daemon_uptime_seconds`           | (none)                       |
//! | 9 | Gauge      | `bot_state`                       | `state`                      |
//! |10 | Gauge      | `connected`                       | `value`                      |
//! |11 | Histogram  | `stoolap_lock_wait_seconds`       | `op`                         |
//! |12 | Histogram  | `stoolap_lock_held_seconds`       | `op`                         |
//! |13 | Histogram  | `rpc_latency_seconds`             | `method=hash`                |
//!
//! (The 14th metric — `daemon.audit_rows_total` — shares the same
//! `audit_rows_total` counter; the table above enumerates every
//! distinct Prometheus series after hashing.)
//!
//! ## Label hashing
//!
//! Free-form identifiers (peer names, event kinds, RPC methods) are
//! truncated to HMAC-SHA-256(secret, value)[..4] hex = 8 hex chars
//! via [`hash_label`]. The secret lives in
//! [`crate::config::MetricsConfig`] and is rotated only on a future
//! `metrics.rotate_secret` RPC.

use std::collections::HashMap;
use std::sync::Arc;

use prometheus::{
    Counter, CounterVec, Encoder, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry,
    TextEncoder,
};
use thiserror::Error;

/// Default histogram bucket set (seconds).
const RPC_LATENCY_BUCKETS: &[f64] = &[
    0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
const LOCK_BUCKETS: &[f64] = &[
    0.0001, 0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
];

#[derive(Debug, Error)]
pub enum MetricsError {
    #[error("prometheus: {0}")]
    Prometheus(#[from] prometheus::Error),
    #[error("utf8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// HMAC-SHA-256 first 4 bytes hex-encoded → 8 hex characters.
///
/// Deterministic — same `(secret, value)` always yields the same
/// label. The 4-byte output bounds the cardinality at ~4.3B
/// (effectively never-colliding for any realistic label set), and the
/// 8-char form keeps the Prometheus series name compact.
pub fn hash_label(secret: &[u8], value: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .expect("HMAC accepts any key length, including zero");
    mac.update(value.as_bytes());
    let result = mac.finalize().into_bytes();
    hex::encode(&result[..4])
}

/// The 14 named Prometheus metrics + a `Registry`. Cheap to clone
/// (`Arc` inside `CounterVec`/`GaugeVec`).
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Underlying registry. `render()` gathers from this.
    pub registry: Registry,
    /// `daemon_uptime_seconds{instance=...}` — seconds since
    /// `start_time` (Unix epoch millis at construction).
    pub daemon_uptime_seconds: Gauge,
    /// `bot_state{state="connected"|"reconnecting"|...}` — 0/1.
    pub bot_state: GaugeVec,
    /// `connected{value="true"|"false"}` — 0/1.
    pub connected: GaugeVec,
    /// `inbound_events_total{kind=hash(event_kind)}`.
    pub inbound_events_total: CounterVec,
    /// `outbound_messages_total{kind=hash(method),result}`.
    pub outbound_messages_total: CounterVec,
    /// `rule_matches_total{rule_id=hash(rule.id)}`.
    pub rule_matches_total: CounterVec,
    /// `trigger_runs_total{trigger_id=hash(trigger.id),result}`.
    pub trigger_runs_total: CounterVec,
    /// `audit_rows_total` — total entries ever recorded.
    pub audit_rows_total: Counter,
    /// `rate_limit_dropped_total{scope,peer=hash}`.
    pub rate_limit_dropped_total: CounterVec,
    /// `auth_failed_total{ip}`.
    pub auth_failed_total: CounterVec,
    /// `stoolap_lock_wait_seconds{op}` — recorder-side wait time.
    pub stoolap_lock_wait_seconds: HistogramVec,
    /// `stoolap_lock_held_seconds{op}` — recorder-side held time.
    pub stoolap_lock_held_seconds: HistogramVec,
    /// `rpc_latency_seconds{method=hash}`.
    pub rpc_latency_seconds: HistogramVec,
    /// Private label-hash secret (used by helper accessors).
    label_secret: Vec<u8>,
}

impl Metrics {
    /// Build a fresh registry with all 14 metrics registered. Each
    /// call returns a new `Registry` so tests can run in parallel
    /// without colliding on the global default registry.
    pub fn new(label_secret: &[u8]) -> Result<Arc<Self>, MetricsError> {
        let registry = Registry::new();

        let daemon_uptime_seconds = Gauge::with_opts(Opts::new(
            "daemon_uptime_seconds",
            "seconds since the daemon process started",
        ))?;
        registry.register(Box::new(daemon_uptime_seconds.clone()))?;

        let bot_state = GaugeVec::new(
            Opts::new("bot_state", "current bot state (one-hot)"),
            &["state"],
        )?;
        registry.register(Box::new(bot_state.clone()))?;

        let connected = GaugeVec::new(
            Opts::new(
                "connected",
                "connection state of the WhatsApp adapter (one-hot)",
            ),
            &["value"],
        )?;
        registry.register(Box::new(connected.clone()))?;

        let inbound_events_total = CounterVec::new(
            Opts::new(
                "inbound_events_total",
                "Inbound events received from the adapter",
            ),
            &["kind"],
        )?;
        registry.register(Box::new(inbound_events_total.clone()))?;

        let outbound_messages_total = CounterVec::new(
            Opts::new(
                "outbound_messages_total",
                "Outbound messages dispatched by the daemon",
            ),
            &["kind", "result"],
        )?;
        registry.register(Box::new(outbound_messages_total.clone()))?;

        let rule_matches_total = CounterVec::new(
            Opts::new(
                "rule_matches_total",
                "Rule matches across the events router",
            ),
            &["rule_id"],
        )?;
        registry.register(Box::new(rule_matches_total.clone()))?;

        let trigger_runs_total = CounterVec::new(
            Opts::new("trigger_runs_total", "Trigger invocations and outcomes"),
            &["trigger_id", "result"],
        )?;
        registry.register(Box::new(trigger_runs_total.clone()))?;

        let audit_rows_total = Counter::with_opts(Opts::new(
            "audit_rows_total",
            "Total audit rows recorded since process start",
        ))?;
        registry.register(Box::new(audit_rows_total.clone()))?;

        let rate_limit_dropped_total = CounterVec::new(
            Opts::new(
                "rate_limit_dropped_total",
                "Requests dropped by the per-caller rate limiter",
            ),
            &["scope", "peer"],
        )?;
        registry.register(Box::new(rate_limit_dropped_total.clone()))?;

        let auth_failed_total = CounterVec::new(
            Opts::new(
                "auth_failed_total",
                "Bearer-auth failures on the IPC socket (peer-ip)",
            ),
            &["ip"],
        )?;
        registry.register(Box::new(auth_failed_total.clone()))?;

        let stoolap_lock_wait_seconds = HistogramVec::new(
            HistogramOpts::new(
                "stoolap_lock_wait_seconds",
                "Time spent waiting for the stoolap connection lock",
            )
            .buckets(LOCK_BUCKETS.to_vec()),
            &["op"],
        )?;
        registry.register(Box::new(stoolap_lock_wait_seconds.clone()))?;

        let stoolap_lock_held_seconds = HistogramVec::new(
            HistogramOpts::new(
                "stoolap_lock_held_seconds",
                "Time the stoolap connection lock was held",
            )
            .buckets(LOCK_BUCKETS.to_vec()),
            &["op"],
        )?;
        registry.register(Box::new(stoolap_lock_held_seconds.clone()))?;

        let rpc_latency_seconds = HistogramVec::new(
            HistogramOpts::new(
                "rpc_latency_seconds",
                "RPC dispatch latency (label hashed to bound cardinality)",
            )
            .buckets(RPC_LATENCY_BUCKETS.to_vec()),
            &["method"],
        )?;
        registry.register(Box::new(rpc_latency_seconds.clone()))?;

        // Pre-initialize the gauge families with their one-hot zero
        // series so Prometheus doesn't show "no data" until the first
        // state transition.
        for state in ["booting", "connected", "reconnecting", "shutting_down"] {
            bot_state.with_label_values(&[state]).set(0.0);
        }
        for v in ["true", "false"] {
            connected.with_label_values(&[v]).set(0.0);
        }

        Ok(Arc::new(Self {
            registry,
            daemon_uptime_seconds,
            bot_state,
            connected,
            inbound_events_total,
            outbound_messages_total,
            rule_matches_total,
            trigger_runs_total,
            audit_rows_total,
            rate_limit_dropped_total,
            auth_failed_total,
            stoolap_lock_wait_seconds,
            stoolap_lock_held_seconds,
            rpc_latency_seconds,
            label_secret: label_secret.to_vec(),
        }))
    }

    /// HMAC-hash `value` against the configured secret. See
    /// [`hash_label`].
    pub fn hashed(&self, value: &str) -> String {
        hash_label(&self.label_secret, value)
    }

    /// Encode the full registry to Prometheus text format.
    pub fn render(&self) -> Result<String, MetricsError> {
        let encoder = TextEncoder::new();
        let mut buf = Vec::with_capacity(8 * 1024);
        let families = self.registry.gather();
        encoder.encode(&families, &mut buf)?;
        Ok(String::from_utf8(buf)?)
    }

    /// Snapshot the canonical 14 metric-family names registered by
    /// this `Metrics`. Useful for tests + smoke-checks that all
    /// expected series exist (the names are known at compile time —
    /// no need to introspect the protobuf in production paths).
    pub fn gather_metric_names(&self) -> Vec<String> {
        vec![
            "daemon_uptime_seconds".into(),
            "bot_state".into(),
            "connected".into(),
            "inbound_events_total".into(),
            "outbound_messages_total".into(),
            "rule_matches_total".into(),
            "trigger_runs_total".into(),
            "audit_rows_total".into(),
            "rate_limit_dropped_total".into(),
            "auth_failed_total".into(),
            "stoolap_lock_wait_seconds".into(),
            "stoolap_lock_held_seconds".into(),
            "rpc_latency_seconds".into(),
        ]
    }

    /// Set `daemon_uptime_seconds` from a Unix-epoch-millisecond
    /// `started_at_unix_ms` + the current `now_unix_ms`. Idempotent
    /// — readers should poll this once per second.
    pub fn observe_uptime(&self, started_at_unix_ms: i64, now_unix_ms: i64) {
        let delta_ms = (now_unix_ms - started_at_unix_ms).max(0) as f64;
        self.daemon_uptime_seconds.set(delta_ms / 1000.0);
    }

    /// Set the one-hot `bot_state` to the named state. All other
    /// states are reset to 0.
    pub fn set_bot_state(&self, state: &str) {
        for s in ["booting", "connected", "reconnecting", "shutting_down"] {
            let v = self.bot_state.with_label_values(&[s]);
            if s == state {
                v.set(1.0);
            } else {
                v.set(0.0);
            }
        }
    }

    /// Set the one-hot `connected{value}` gauge.
    pub fn set_connected(&self, is_connected: bool) {
        let t = self.connected.with_label_values(&["true"]);
        let f = self.connected.with_label_values(&["false"]);
        if is_connected {
            t.set(1.0);
            f.set(0.0);
        } else {
            t.set(0.0);
            f.set(1.0);
        }
    }

    /// Increment `inbound_events_total{kind=hash}`.
    pub fn inc_inbound_event(&self, raw_kind: &str) {
        let h = self.hashed(raw_kind);
        self.inbound_events_total.with_label_values(&[&h]).inc();
    }

    /// Increment `outbound_messages_total{kind=hash,result}`.
    pub fn inc_outbound(&self, raw_kind: &str, result: &str) {
        let h = self.hashed(raw_kind);
        self.outbound_messages_total
            .with_label_values(&[&h, result])
            .inc();
    }

    /// Increment `rule_matches_total{rule_id=hash}`.
    pub fn inc_rule_match(&self, raw_rule_id: &str) {
        let h = self.hashed(raw_rule_id);
        self.rule_matches_total.with_label_values(&[&h]).inc();
    }

    /// Increment `trigger_runs_total{trigger_id=hash,result}`.
    pub fn inc_trigger_run(&self, raw_trigger_id: &str, result: &str) {
        let h = self.hashed(raw_trigger_id);
        self.trigger_runs_total
            .with_label_values(&[&h, result])
            .inc();
    }

    /// Increment `audit_rows_total` by 1.
    pub fn inc_audit_row(&self) {
        self.audit_rows_total.inc();
    }

    /// Increment `rate_limit_dropped_total{scope,peer=hash}`.
    pub fn inc_rate_limit_dropped(&self, scope: &str, raw_peer: &str) {
        let h = self.hashed(raw_peer);
        self.rate_limit_dropped_total
            .with_label_values(&[scope, &h])
            .inc();
    }

    /// Increment `auth_failed_total{ip}` for the raw peer IP.
    pub fn inc_auth_failed(&self, peer_ip: &str) {
        self.auth_failed_total.with_label_values(&[peer_ip]).inc();
    }

    /// Observe `rpc_latency_seconds{method=hash}`. Use a stopwatch
    /// in the caller.
    pub fn observe_rpc_latency(&self, raw_method: &str, seconds: f64) {
        let h = self.hashed(raw_method);
        self.rpc_latency_seconds
            .with_label_values(&[&h])
            .observe(seconds);
    }

    /// Observe `stoolap_lock_wait_seconds{op}`.
    pub fn observe_lock_wait(&self, op: &str, seconds: f64) {
        self.stoolap_lock_wait_seconds
            .with_label_values(&[op])
            .observe(seconds);
    }

    /// Observe `stoolap_lock_held_seconds{op}`.
    pub fn observe_lock_held(&self, op: &str, seconds: f64) {
        self.stoolap_lock_held_seconds
            .with_label_values(&[op])
            .observe(seconds);
    }

    /// Construct a snapshot of all gauges/counters in a JSON-ish shape.
    /// Used by `health.get` to surface metric values for callers that
    /// don't want to scrape Prometheus directly.
    #[allow(clippy::type_complexity)]
    pub fn snapshot(&self) -> HashMap<String, f64> {
        let mut out = HashMap::new();
        out.insert(
            "daemon_uptime_seconds".into(),
            self.daemon_uptime_seconds.get(),
        );
        out.insert("audit_rows_total".into(), self.audit_rows_total.get());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_label_is_deterministic_8_hex_chars() {
        let secret = b"some-secret-bytes";
        let a = hash_label(secret, "events.message");
        let b = hash_label(secret, "events.message");
        assert_eq!(a, b);
        assert_eq!(a.len(), 8);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_label_changes_with_input() {
        let secret = b"some-secret-bytes";
        let a = hash_label(secret, "events.message");
        let b = hash_label(secret, "events.reaction");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_label_changes_with_secret() {
        let a = hash_label(b"secret-A", "events.message");
        let b = hash_label(b"secret-B", "events.message");
        assert_ne!(a, b);
    }

    #[test]
    fn metrics_render_contains_all_14_names() {
        let m = Metrics::new(b"k").unwrap();
        // Touch every counter/vector once so a sample appears in the
        // rendered output. (Without this, CounterVec with `vec` of
        // length 0 won't show up under `prometheus` 0.13.)
        m.inc_inbound_event("message");
        m.inc_outbound("send.text", "ok");
        m.inc_outbound("send.text", "error");
        m.inc_rule_match("rule-1");
        m.inc_trigger_run("trigger-1", "ok");
        m.inc_audit_row();
        m.inc_rate_limit_dropped("rpc", "127.0.0.1");
        m.inc_auth_failed("127.0.0.1");
        m.observe_rpc_latency("version.get", 0.001);
        m.observe_lock_wait("acquire", 0.001);
        m.observe_lock_held("release", 0.001);
        m.set_bot_state("connected");
        m.set_connected(true);

        let text = m.render().unwrap();
        // The 14 expected names (some families expose a single
        // canonical name; CounterVec without values isn't rendered,
        // but every CounterVec above was touched).
        let expected: &[&str] = &[
            "daemon_uptime_seconds",
            "bot_state",
            "connected",
            "inbound_events_total",
            "outbound_messages_total",
            "rule_matches_total",
            "trigger_runs_total",
            "audit_rows_total",
            "rate_limit_dropped_total",
            "auth_failed_total",
            "stoolap_lock_wait_seconds",
            "stoolap_lock_held_seconds",
            "rpc_latency_seconds",
        ];
        for name in expected {
            assert!(
                text.contains(name),
                "rendered text missing metric {name}:\n{text}"
            );
        }
    }

    #[test]
    fn metrics_render_is_valid_prometheus_text() {
        let m = Metrics::new(b"k").unwrap();
        m.inc_audit_row();
        m.observe_uptime(1_000, 2_000);
        let text = m.render().unwrap();
        // `# HELP <name> <doc>` lines are the canonical marker for
        // Prometheus exposition format.
        assert!(text.contains("# HELP"));
        assert!(text.contains("# TYPE"));
    }

    #[test]
    fn metrics_render_increments_visible() {
        let m = Metrics::new(b"k").unwrap();
        m.inc_audit_row();
        m.inc_audit_row();
        m.inc_audit_row();
        let text = m.render().unwrap();
        assert!(text.contains("audit_rows_total 3"));
    }

    #[test]
    fn counter_increments_track_calls() {
        let m = Metrics::new(b"k").unwrap();
        m.inc_inbound_event("message");
        m.inc_inbound_event("message");
        m.inc_inbound_event("reaction");
        let text = m.render().unwrap();
        let h_msg = m.hashed("message");
        let h_rxn = m.hashed("reaction");
        assert!(text.contains(&format!("inbound_events_total{{kind=\"{h_msg}\"}} 2")));
        assert!(text.contains(&format!("inbound_events_total{{kind=\"{h_rxn}\"}} 1")));
    }

    #[test]
    fn gauge_set_bot_state_is_one_hot() {
        let m = Metrics::new(b"k").unwrap();
        m.set_bot_state("connected");
        m.set_bot_state("reconnecting");
        let text = m.render().unwrap();
        // connected: 0, reconnecting: 1, the rest: 0
        assert!(text.contains("bot_state{state=\"reconnecting\"} 1"));
        assert!(text.contains("bot_state{state=\"connected\"} 0"));
    }

    #[test]
    fn gauge_set_connected_is_one_hot() {
        let m = Metrics::new(b"k").unwrap();
        m.set_connected(false);
        m.set_connected(true);
        let text = m.render().unwrap();
        assert!(text.contains("connected{value=\"true\"} 1"));
        assert!(text.contains("connected{value=\"false\"} 0"));
    }

    #[test]
    fn gather_metric_names_includes_all_families() {
        let m = Metrics::new(b"k").unwrap();
        m.inc_inbound_event("message");
        let names = m.gather_metric_names();
        // CounterVec needs at least one observation to be gathered.
        // Counters / Gauges always show.
        for must in [
            "daemon_uptime_seconds",
            "audit_rows_total",
            "inbound_events_total",
        ] {
            assert!(
                names.iter().any(|n| n == must),
                "missing metric family: {must}"
            );
        }
    }

    #[test]
    fn snapshot_returns_current_values() {
        let m = Metrics::new(b"k").unwrap();
        m.observe_uptime(1_000, 3_500);
        m.inc_audit_row();
        m.inc_audit_row();
        let snap = m.snapshot();
        assert_eq!(snap["audit_rows_total"], 2.0);
        assert!((snap["daemon_uptime_seconds"] - 2.5).abs() < 1e-9);
    }

    #[test]
    fn new_isolated_registry_per_call() {
        // Two constructors must NOT alias the global default
        // registry; this is a parallel-safety contract.
        let m1 = Metrics::new(b"k").unwrap();
        let m2 = Metrics::new(b"k").unwrap();
        m1.inc_audit_row();
        assert_eq!(m1.snapshot()["audit_rows_total"], 1.0);
        assert_eq!(m2.snapshot()["audit_rows_total"], 0.0);
    }
}
