//! Runtime configuration loaded from a TOML file.
//!
//! Phase 1: minimal schema (name + paths + socket). Rules, triggers,
//! event-retention, observability, and security fields arrive in later
//! phases. The schema is intentionally additive.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use octo_adapter_whatsapp::WhatsAppConfig;

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
///
/// Phase 3 Part D: disk persistence knobs. `persistence_enabled` is
/// the master switch; `flush_interval_ms` controls the windowed
/// fsync cadence. `persistence_path` overrides the default
/// `$data_dir/events/events.ndjson` for operators who want a custom
/// location.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventsConfig {
    pub max_rows: usize,
    pub retention_days: u32,
    #[serde(default = "default_events_persistence_enabled")]
    pub persistence_enabled: bool,
    #[serde(default = "default_events_flush_interval_ms")]
    pub flush_interval_ms: u64,
    #[serde(default)]
    pub persistence_path: Option<std::path::PathBuf>,
}

/// Phase 0+ of `docs/plans/2026-07-11-whatsapp-query-layer-design.md`:
/// knobs for the embedded query layer. Read at boot by the
/// daemon and forwarded to `QuerySubsystem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueryConfig {
    /// Embedding provider: "local" (candle MiniLM-L6-v2 Q4), "mock"
    /// (deterministic hermetic), "none" (skip embedding entirely).
    /// `"local"` is the default but falls back to `"mock"` when
    /// the model weights aren't on disk.
    #[serde(default = "default_query_embed_provider")]
    pub embed_provider: String,
    /// Override the candle model cache dir. Empty = HF default.
    #[serde(default)]
    pub model_dir: Option<std::path::PathBuf>,
    /// Hard cap on in-flight embedding jobs. Excess are dropped
    /// (recovery: NDJSON replay back-fills missing embeddings).
    #[serde(default = "default_query_queue_capacity")]
    pub queue_capacity: usize,
    /// Worker batch size for the embedder queue.
    #[serde(default = "default_query_batch_size")]
    pub batch_size: usize,
    /// Worker batch window in milliseconds (drains earlier when
    /// `batch_size` is hit, otherwise waits this long).
    #[serde(default = "default_query_batch_window_ms")]
    pub batch_window_ms: u64,
    /// Per-subscriber channel depth for the live ingress.
    #[serde(default = "default_query_subscriber_capacity")]
    pub subscriber_capacity: usize,
    /// FTS index rebuild on boot. If `true` (default), the
    /// subsystem rehydrates from NDJSON at boot. If `false`,
    /// operators must run `octo-wa query index-rebuild` manually.
    #[serde(default = "default_query_rebuild_on_boot")]
    pub rebuild_on_boot: bool,
}

fn default_query_embed_provider() -> String {
    "local".into()
}
fn default_query_queue_capacity() -> usize {
    1024
}
fn default_query_batch_size() -> usize {
    32
}
fn default_query_batch_window_ms() -> u64 {
    50
}
fn default_query_subscriber_capacity() -> usize {
    4096
}
fn default_query_rebuild_on_boot() -> bool {
    true
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            embed_provider: "local".into(),
            model_dir: None,
            queue_capacity: 1024,
            batch_size: 32,
            batch_window_ms: 50,
            subscriber_capacity: 4096,
            rebuild_on_boot: true,
        }
    }
}

fn default_events_persistence_enabled() -> bool {
    true
}

fn default_events_flush_interval_ms() -> u64 {
    5_000
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            max_rows: 1_000_000,
            retention_days: 30,
            persistence_enabled: default_events_persistence_enabled(),
            flush_interval_ms: default_events_flush_interval_ms(),
            persistence_path: None,
        }
    }
}

impl EventsConfig {
    /// Resolve the on-disk file path. Operators can override via
    /// `persistence_path`; otherwise we use
    /// `default_persistence_path(data_dir)`.
    pub fn resolved_persistence_path(&self, data_dir: &std::path::Path) -> std::path::PathBuf {
        match &self.persistence_path {
            Some(p) => p.clone(),
            None => crate::events_persister::default_persistence_path(data_dir),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WhatsAppRuntimeConfig {
    pub name: String,
    /// Phase 6.1: stable multi-account identifier used to derive the
    /// per-account `session.db` directory. Distinct from `name`
    /// (operator-friendly label used for socket paths).
    #[serde(default = "default_account_id")]
    pub account_id: String,
    /// Phase 6.1: list of group JIDs the runtime should subscribe to.
    #[serde(default)]
    pub groups: Vec<String>,
    /// Phase 6.1: per-group sender allowlist (E.164 numbers or JIDs).
    #[serde(default)]
    pub sender_allowlist: std::collections::BTreeMap<String, Vec<String>>,
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
    /// Phase 1 (query layer): knobs for embedded SQL + Tantivy +
    /// candle. Absent when the `query` cargo feature is off — in
    /// that case the file load uses `QueryConfig::default()` so
    /// the daemon still boots.
    #[serde(default = "default_query_config_field")]
    pub query: QueryConfig,
    /// Phase 5 Part B: Prometheus + /health + /ready + OTLP knobs.
    /// All optional with safe defaults (off / loopback-only).
    #[serde(default)]
    pub observability: ObservabilityConfig,
    /// Phase 5 Part C: rules persistence knobs (storage path,
    /// WAL path, debounce window). Defaults are explicit
    /// (no silent fallbacks).
    #[serde(default)]
    pub rules: RulesConfig,
}

/// Phase 5 Part C: rules persistence configuration. Discovers the
/// on-disk location of `rules.toml`, the WAL file, and the
/// debounce window for coalescing rapid mutations into a single
/// atomic write. Defaults are safe-but-explicit.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RulesConfig {
    /// Path to `rules.toml`. Default
    /// `~/.local/share/octo/whatsapp/rules.toml` (`~` is resolved
    /// via `dirs::home_dir()` at startup).
    #[serde(default = "default_rules_storage_path")]
    pub storage_path: PathBuf,
    /// Coalescing window in milliseconds for rapid mutations.
    /// Multiple writes within this window collapse into one disk
    /// write. Default 100ms.
    #[serde(default = "default_rules_debounce_ms")]
    pub debounce_ms: u64,
    /// WAL path. Default = `storage_path` with `.wal` extension.
    #[serde(default)]
    pub wal_path: Option<PathBuf>,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            storage_path: default_rules_storage_path(),
            debounce_ms: default_rules_debounce_ms(),
            wal_path: None,
        }
    }
}

fn default_rules_storage_path() -> PathBuf {
    PathBuf::from("~/.local/share/octo/whatsapp/rules.toml")
}

fn default_rules_debounce_ms() -> u64 {
    100
}

impl RulesConfig {
    /// Resolve the effective storage path (expanding `~`).
    pub fn resolved_storage_path(&self) -> PathBuf {
        crate::rules::resolve_storage_path(&self.storage_path)
    }

    /// Resolve the effective WAL path (defaults to
    /// `<storage>.wal` when the user did not set one).
    pub fn resolved_wal_path(&self) -> PathBuf {
        match &self.wal_path {
            Some(p) => crate::rules::resolve_storage_path(p),
            None => {
                let mut stem = self.resolved_storage_path();
                let new_ext = match stem.extension() {
                    Some(e) => format!("{}.wal", e.to_string_lossy()),
                    None => "wal".to_string(),
                };
                stem.set_extension(new_ext);
                stem
            }
        }
    }
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
    /// **Security review F1, F10.** When `bearer_token_env` is unset
    /// and `bearer_required = true`, the daemon enters "hermetic
    /// mode". Pure read-only RPCs (`health.get`, `daemon.*`) continue
    /// to work; mutating RPCs (`rules.*`, `triggers.*`, `audit.*`,
    /// `actions.*`, `security.*`, outbound `send.*` / `messages.*`,
    /// chat mutations) are refused with `-32050 unauthorized`.
    ///
    /// When `hermetic_bypass = true` AND `bearer_required = false`
    /// AND `bearer_token_env` is unset, ALL RPCs are accepted
    /// unconditionally — this is the pre-Phase 5 hermetic-test
    /// contract. **Production deployments must set
    /// `hermetic_bypass = false`** (the default).
    #[serde(default)]
    pub hermetic_bypass: bool,
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
            hermetic_bypass: false,
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
            name: "default".to_string(),
            account_id: default_account_id(),
            groups: Vec::new(),
            sender_allowlist: std::collections::BTreeMap::new(),
            data_dir: default_data_dir(),
            log_dir: default_log_dir(),
            socket_dir: default_socket_dir(),
            media_buffer: MediaBufferConfig::default(),
            events: EventsConfig::default(),
            security: SecurityConfig::default(),
            observability: ObservabilityConfig::default(),
            rules: RulesConfig::default(),
            query: QueryConfig::default(),
        }
    }
}

fn default_query_config_field() -> QueryConfig {
    QueryConfig::default()
}

fn default_data_dir() -> PathBuf {
    // Allow operators to override the system default via env so the
    // daemon can run from a per-user install (e.g. `cargo install`)
    // without writing to /var/lib. Honors `OCTO_WHATSAPP_DATA_DIR`.
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_DATA_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("/var/lib/octo/whatsapp")
}

fn default_account_id() -> String {
    "default".to_string()
}
fn default_log_dir() -> PathBuf {
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_LOG_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("/var/log/octo/whatsapp")
}
fn default_socket_dir() -> PathBuf {
    // Allow per-user install override. `OCTO_WHATSAPP_SOCKET_DIR`
    // bypasses the system `/run` default so daemons launched outside
    // systemd can still bind a unix socket under the user's home.
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_SOCKET_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
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

    /// Derive the adapter-layer `WhatsAppConfig` from the runtime config.
    ///
    /// `session_path` is computed as `$data_dir/{account_id}.session.db`.
    /// Override with `OCTO_WHATSAPP_SESSION_PATH` for explicit operator
    /// control.
    pub fn adapter_config(&self) -> WhatsAppConfig {
        let session_path = if let Ok(p) = std::env::var("OCTO_WHATSAPP_SESSION_PATH") {
            if !p.is_empty() {
                PathBuf::from(p)
            } else {
                self.data_dir
                    .join(format!("{}.session.db", self.account_id))
            }
        } else {
            self.data_dir
                .join(format!("{}.session.db", self.account_id))
        };
        WhatsAppConfig {
            session_path: session_path.to_string_lossy().into_owned(),
            ws_url: None,
            pair_phone: None,
            pair_code: None,
            groups: self.groups.clone(),
            sender_allowlist: self.sender_allowlist.clone(),
            passkey_authenticator: None,
        }
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
        if self.account_id.is_empty() {
            return Err(ConfigError::InvalidName(
                "account_id must be non-empty".to_string(),
            ));
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
        // Phase 3 Part D: persistence tuning. flush_interval_ms
        // is bounded to keep the at-most-once window tight but
        // avoid pathological sub-second fsync churn. 0 is rejected
        // because it would block the actor loop on every event.
        if self.events.persistence_enabled
            && !(100..=60_000).contains(&self.events.flush_interval_ms)
        {
            return Err(ConfigError::InvalidName(format!(
                "events.flush_interval_ms must be in 100..=60000 when persistence is enabled (got {})",
                self.events.flush_interval_ms
            )));
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
        // Phase 5 Part C: validate rules config knobs.
        if self.rules.debounce_ms == 0 {
            return Err(ConfigError::InvalidObservability(
                "rules.debounce_ms must be > 0 (got 0); debouncing disabled is forbidden"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
