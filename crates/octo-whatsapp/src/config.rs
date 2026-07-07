//! Runtime configuration loaded from a TOML file.
//!
//! Phase 1: minimal schema (name + paths + socket). Rules, triggers,
//! event-retention, observability, and security fields arrive in later
//! phases. The schema is intentionally additive.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid name {0:?}: must match [a-z0-9_-]+")]
    InvalidName(String),
    #[error("invalid observability config: {0}")]
    InvalidObservability(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MediaBufferConfig {
    /// Maximum concurrent in-flight media uploads. Bounded to keep
    /// disk + memory under control. `0` is invalid.
    pub max_concurrent_uploads: usize,
    /// Root temp directory under which per-request `.bin` files live.
    pub root: PathBuf,
}

impl Default for MediaBufferConfig {
    fn default() -> Self {
        Self {
            max_concurrent_uploads: 4,
            root: std::env::temp_dir().join("octo-whatsapp"),
        }
    }
}

/// Phase 3: in-memory events retention. Bounded by `max_rows` (cap)
/// and `retention_days` (TTL, currently advisory — `max_rows` is the
/// primary bound). Default `max_rows = 1_000_000` and
/// `retention_days = 30` per design §InboundEvent retention.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventsConfig {
    pub max_rows: usize,
    pub retention_days: u32,
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            max_rows: 1_000_000,
            retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WhatsAppRuntimeConfig {
    pub name: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    #[serde(default = "default_socket_dir")]
    pub socket_dir: PathBuf,
    /// Media buffer tuning. Defaults to 4 concurrent uploads under
    /// `$TMPDIR/octo-whatsapp` (safe production default).
    #[serde(default)]
    pub media_buffer: MediaBufferConfig,
    /// Phase 3: events retention. Default 1M rows, 30 days.
    #[serde(default)]
    pub events: EventsConfig,
    /// Phase 4: security/audit knobs. All optional with safe defaults.
    #[serde(default)]
    pub security: SecurityConfig,
    /// Phase 5 Part B: Prometheus + /health + /ready + OTLP knobs.
    /// All optional with safe defaults (off / loopback-only).
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

/// Phase 4: security-related runtime configuration.
///
/// Design §Security + §Hot mutation safety:
/// - `auto_approve_rules` — when true, rules with no manual-approval
///   actions enter as `Approved` instead of `Draft`.
/// - `audit_max_rows` — ring-buffer cap. Default 100_000 per design.
/// - `audit_anchor_every` — every Nth chain head is appended to the
///   external anchor file. Default 100.
/// - `bearer_token_env` — env var name holding the initial bearer
///   token (`<id>.<secret_hex>`). Default `OCTO_WHATSAPP_TOKEN`.
///   Empty value disables bearer auth entirely (hermetic tests).
/// - `grace_path` — on-disk path for `grace.json`. Default
///   `$data_dir/tokens/grace.json`.
/// - `grace_period_ms` — default rotation grace window. Clamped to
///   1000..=300_000 ms. Default 60_000.
/// - `bearer_required` — if true, every RPC method MUST present a
///   valid bearer token. If false (default), bearer is optional —
///   hermetic tests and operator tools work without ceremony. Per
///   plan §A1: full enforcement is part of Part B (observability
///   surfaces), Part A provides the plumbing.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SecurityConfig {
    #[serde(default)]
    pub auto_approve_rules: bool,
    #[serde(default = "default_audit_max_rows")]
    pub audit_max_rows: usize,
    #[serde(default = "default_audit_anchor_every")]
    pub audit_anchor_every: u64,
    #[serde(default = "default_bearer_token_env")]
    pub bearer_token_env: String,
    #[serde(default)]
    pub grace_path: Option<PathBuf>,
    #[serde(default = "default_grace_period_ms")]
    pub grace_period_ms: i64,
    #[serde(default)]
    pub bearer_required: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            auto_approve_rules: false,
            audit_max_rows: 100_000,
            audit_anchor_every: 100,
            bearer_token_env: default_bearer_token_env(),
            grace_path: None,
            grace_period_ms: default_grace_period_ms(),
            bearer_required: false,
        }
    }
}

fn default_audit_max_rows() -> usize {
    100_000
}
fn default_audit_anchor_every() -> u64 {
    100
}
fn default_bearer_token_env() -> String {
    "OCTO_WHATSAPP_TOKEN".to_string()
}
fn default_grace_period_ms() -> i64 {
    60_000
}

/// Phase 5 Part B: Prometheus + HTTP health/ready + OTLP tracing knobs.
///
/// Each sub-section is optional; nothing is enabled by default.
/// Operators enable the surfaces they want (metrics scrape via the
/// HTTP `/metrics` endpoint; readiness via `/ready`; OTLP via the
/// `[observability.tracing]` block).
///
/// **Loopback-only binding:** `health.http_listen` MUST resolve to a
/// loopback IP. Non-loopback binds are rejected at startup. Plan §A7.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ObservabilityConfig {
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub health: HealthConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
}

/// `[observability.metrics]` — label-hash secret + optional
/// bearer-token ENV var for the `/metrics` HTTP endpoint.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MetricsConfig {
    /// Hex-encoded HMAC secret used to hash high-cardinality labels.
    /// Operators may set this to a 32-byte hex string; defaults to
    /// random-on-boot bytes. **Rotating changes all label hashes**
    /// — Prometheus series reappear with new label values, doubling
    /// cardinality briefly.
    #[serde(default)]
    pub label_hash_secret: Option<String>,
    /// Name of the env var holding the bearer token for the
    /// `/metrics` HTTP endpoint. The env-var value is the literal
    /// token (any length, kept opaque to the daemon). When the env
    /// var is unset AND `health.bearer_required = false`, `/metrics`
    /// is reachable without a bearer — this is the hermetic-test
    /// default; production deployments MUST set the env var.
    #[serde(default = "default_metrics_bearer_token_env")]
    pub bearer_token_env: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            label_hash_secret: None,
            bearer_token_env: default_metrics_bearer_token_env(),
        }
    }
}

/// `[observability.health]` — HTTP `/health`, `/ready`, `/metrics`.
///
/// `http_listen` is the only knob. When `None`, the HTTP server is
/// not started and `health.get` reports the operator-decided state.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HealthConfig {
    /// Loopback bind address (`<ip>:<port>`). Default is
    /// `127.0.0.1:7778`. The daemon refuses to start if the
    /// resolved address is not loopback (plan §A7).
    #[serde(default = "default_health_http_listen")]
    pub http_listen: Option<String>,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            http_listen: default_health_http_listen(),
        }
    }
}

/// `[observability.tracing]` — OTLP exporter config.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TracingConfig {
    /// OTLP gRPC endpoint (e.g. `http://localhost:4317`). `None`
    /// means tracing export is disabled.
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
    /// OTLP service name. Default `octo-whatsapp`.
    #[serde(default = "default_tracing_service_name")]
    pub service_name: String,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: None,
            service_name: default_tracing_service_name(),
        }
    }
}

fn default_metrics_bearer_token_env() -> String {
    "OCTO_WHATSAPP_METRICS_TOKEN".to_string()
}
fn default_health_http_listen() -> Option<String> {
    Some("127.0.0.1:7778".to_string())
}
fn default_tracing_service_name() -> String {
    "octo-whatsapp".to_string()
}

impl Default for WhatsAppRuntimeConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            data_dir: default_data_dir(),
            log_dir: default_log_dir(),
            socket_dir: default_socket_dir(),
            media_buffer: MediaBufferConfig::default(),
            events: EventsConfig::default(),
            security: SecurityConfig::default(),
            observability: ObservabilityConfig::default(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/octo/whatsapp")
}
fn default_log_dir() -> PathBuf {
    PathBuf::from("/var/log/octo/whatsapp")
}
fn default_socket_dir() -> PathBuf {
    PathBuf::from("/run/octo/whatsapp")
}

impl WhatsAppRuntimeConfig {
    pub fn from_toml(bytes: &[u8]) -> Result<Self, ConfigError> {
        let s = std::str::from_utf8(bytes).map_err(|e| {
            ConfigError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        let cfg: Self = toml::from_str(s)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let bytes = std::fs::read(path)?;
        Self::from_toml(&bytes)
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket_dir
            .join(format!("octo-whatsapp-{}.sock", self.name))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty()
            || !self
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(ConfigError::InvalidName(self.name.clone()));
        }
        if self.media_buffer.max_concurrent_uploads == 0 {
            return Err(ConfigError::InvalidName(
                "media_buffer.max_concurrent_uploads must be > 0 (got 0)".to_string(),
            ));
        }
        if self.media_buffer.root.as_os_str().is_empty() {
            return Err(ConfigError::InvalidName(
                "media_buffer.root must be non-empty".into(),
            ));
        }
        if self.events.max_rows == 0 {
            return Err(ConfigError::InvalidName(
                "events.max_rows must be > 0 (got 0)".to_string(),
            ));
        }
        if self.events.retention_days == 0 {
            return Err(ConfigError::InvalidName(
                "events.retention_days must be > 0 (got 0)".to_string(),
            ));
        }
        if self.security.audit_max_rows == 0 {
            return Err(ConfigError::InvalidName(
                "security.audit_max_rows must be > 0 (got 0)".to_string(),
            ));
        }
        if self.security.audit_anchor_every == 0 {
            return Err(ConfigError::InvalidName(
                "security.audit_anchor_every must be > 0 (got 0)".to_string(),
            ));
        }
        if self.security.bearer_token_env.is_empty() {
            return Err(ConfigError::InvalidName(
                "security.bearer_token_env must be non-empty".to_string(),
            ));
        }
        if !(1_000..=300_000).contains(&self.security.grace_period_ms) {
            return Err(ConfigError::InvalidName(format!(
                "security.grace_period_ms must be in 1000..=300000 (got {})",
                self.security.grace_period_ms
            )));
        }
        // Phase 5 Part B: validate the observability config —
        // strictly enforce loopback-only binds for the health server.
        if let Some(addr_str) = self.observability.health.http_listen.as_deref() {
            let addr: std::net::SocketAddr = addr_str.parse().map_err(|e| {
                ConfigError::InvalidObservability(format!(
                    "observability.health.http_listen {addr_str:?} is not a valid socket address: {e}"
                ))
            })?;
            if !addr.ip().is_loopback() {
                return Err(ConfigError::InvalidObservability(format!(
                    "observability.health.http_listen {addr} is not loopback; refusing to start \
                     (health surfaces must be loopback-only, plan §A7)"
                )));
            }
        }
        if let Some(endpoint) = self.observability.tracing.otlp_endpoint.as_deref() {
            if endpoint.is_empty() {
                return Err(ConfigError::InvalidObservability(
                    "observability.tracing.otlp_endpoint cannot be an empty string".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
