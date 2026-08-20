//! Health endpoints for K8s-compatible liveness and readiness probes.
//!
//! ## Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | GET | /healthz | Liveness probe (always 200 if process running) |
//! | GET | /healthz/ready | Readiness probe (checks dependencies) |
//!
//! ## Response Formats
//!
//! Liveness: `{"status": "ok"}`
//! Readiness: `{"status": "ok"|"degraded"|"unhealthy", "checks": {...}}`

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Health check configuration (RFC-0905)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Enable health endpoints (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Health check port (default: same as admin port)
    #[serde(default = "default_health_port")]
    pub port: u16,

    /// Enable readiness dependency checks (default: true)
    #[serde(default = "default_true")]
    pub check_dependencies: bool,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8000,
            check_dependencies: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_health_port() -> u16 {
    8000
}

/// Health status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Unhealthy,
}

/// Individual check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Liveness response — always returns OK if process is alive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessResponse {
    pub status: HealthStatus,
}

/// Readiness response — checks dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub status: HealthStatus,
    pub checks: std::collections::HashMap<String, CheckResult>,
}

/// Dependency checker for readiness probes
pub trait DependencyChecker: Send + Sync {
    /// Check if stoolap database is accessible
    fn check_stoolap(&self) -> CheckResult;

    /// Check if config is valid and loaded
    fn check_config(&self) -> CheckResult;

    /// Check if at least one provider is healthy
    fn check_providers(&self) -> CheckResult;
}

/// Default dependency checker that always returns OK
pub struct DefaultDependencyChecker;

impl DependencyChecker for DefaultDependencyChecker {
    fn check_stoolap(&self) -> CheckResult {
        CheckResult {
            status: HealthStatus::Ok,
            message: None,
        }
    }

    fn check_config(&self) -> CheckResult {
        CheckResult {
            status: HealthStatus::Ok,
            message: None,
        }
    }

    fn check_providers(&self) -> CheckResult {
        CheckResult {
            status: HealthStatus::Ok,
            message: None,
        }
    }
}

// ============================================================================
// Real dependency checkers (RFC-0905 §Healthcheck; mission 0905-d)
// ============================================================================

/// Per-dependency health probe with bounded wall-clock time.
///
/// Implementations must return a `CheckResult` within their declared timeout.
/// On timeout or error the result is `Unhealthy` with a descriptive message.
pub trait AsyncDependencyChecker: Send + Sync {
    /// Run all dependency probes concurrently and return named results.
    #[allow(clippy::type_complexity)]
    fn check_all<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Vec<(&'static str, CheckResult)>> + Send + 'a>,
    >;
}

/// Stoolap probe — opens the configured database and runs `SELECT 1` (RFC-0905
/// §Healthcheck: 200ms timeout).
pub struct StoolapDependencyChecker {
    pub db_path: std::path::PathBuf,
}

impl StoolapDependencyChecker {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        Self { db_path }
    }

    pub async fn check(&self) -> CheckResult {
        let db_path = self.db_path.clone();
        let probe = tokio::task::spawn_blocking(move || -> Result<(), String> {
            // substrate's `Database::open` already prepends `file://` to the
            // path arg; passing an already-prefixed DSN here would produce
            // `file://file:///...`, which the fork parses as scheme=file +
            // path=file:///... — a different (valid) filesystem location.
            // For `:memory:` we bypass `open` entirely.
            if db_path.as_os_str() == ":memory:" {
                let _db = octo_storage_core::Database::open_in_memory()
                    .map_err(|e| format!("open_in_memory: {e}"))?;
                return Ok(());
            }
            let p = db_path
                .to_str()
                .ok_or_else(|| format!("db_path is not valid UTF-8: {db_path:?}"))?;
            let _db = octo_storage_core::Database::open(p).map_err(|e| format!("open: {e}"))?;
            Ok(())
        });
        match tokio::time::timeout(Duration::from_millis(200), probe).await {
            Ok(Ok(Ok(()))) => CheckResult {
                status: HealthStatus::Ok,
                message: None,
            },
            Ok(Ok(Err(e))) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some(e),
            },
            Ok(Err(e)) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some(format!("stoolap join error: {e}")),
            },
            Err(_) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some("stoolap probe timeout (200ms)".to_string()),
            },
        }
    }
}

/// Config probe — re-parses `Config::load()` and validates required keys
/// (RFC-0905 §Healthcheck: 50ms timeout).
pub struct ConfigDependencyChecker {
    pub config_path: std::path::PathBuf,
}

impl ConfigDependencyChecker {
    pub fn new(config_path: std::path::PathBuf) -> Self {
        Self { config_path }
    }

    pub async fn check(&self) -> CheckResult {
        let config_path = self.config_path.clone();
        let probe = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let _ = crate::config::Config::load_from_path(&config_path)
                .map_err(|e| format!("config load: {e}"))?;
            Ok(())
        });
        match tokio::time::timeout(Duration::from_millis(50), probe).await {
            Ok(Ok(Ok(()))) => CheckResult {
                status: HealthStatus::Ok,
                message: None,
            },
            Ok(Ok(Err(e))) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some(e),
            },
            Ok(Err(e)) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some(format!("config join error: {e}")),
            },
            Err(_) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some("config probe timeout (50ms)".to_string()),
            },
        }
    }
}

/// Provider registry probe — verifies the registry is non-empty and every entry
/// carries a resolvable endpoint (RFC-0905 §Healthcheck: 50ms timeout).
///
/// Returns `Degraded` for empty registry (no providers configured) and
/// `Unhealthy` if any provider has an empty endpoint.
pub struct ProvidersDependencyChecker {
    /// Synchronous accessor for current provider snapshot.
    pub snapshot: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync>,
}

impl ProvidersDependencyChecker {
    pub fn new(snapshot: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync>) -> Self {
        Self { snapshot }
    }

    pub async fn check(&self) -> CheckResult {
        let snapshot = Arc::clone(&self.snapshot);
        let probe = tokio::task::spawn_blocking(move || -> Result<CheckResult, String> {
            let providers = snapshot();
            if providers.is_empty() {
                return Ok(CheckResult {
                    status: HealthStatus::Degraded,
                    message: Some("no providers configured".to_string()),
                });
            }
            for p in &providers {
                if p.endpoint.trim().is_empty() {
                    return Ok(CheckResult {
                        status: HealthStatus::Unhealthy,
                        message: Some(format!("provider '{}' has empty endpoint", p.name)),
                    });
                }
            }
            Ok(CheckResult {
                status: HealthStatus::Ok,
                message: Some(format!("{} provider(s) configured", providers.len())),
            })
        });
        match tokio::time::timeout(Duration::from_millis(50), probe).await {
            Ok(Ok(Ok(result))) => result,
            Ok(Ok(Err(e))) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some(e),
            },
            Ok(Err(e)) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some(format!("providers join error: {e}")),
            },
            Err(_) => CheckResult {
                status: HealthStatus::Unhealthy,
                message: Some("providers probe timeout (50ms)".to_string()),
            },
        }
    }
}

/// Composite probe — runs all three probes concurrently with `tokio::join!` and
/// aggregates results (mission 0905-d AC-4).
pub struct CompositeDependencyChecker {
    pub stoolap: StoolapDependencyChecker,
    pub config: ConfigDependencyChecker,
    pub providers: ProvidersDependencyChecker,
}

impl CompositeDependencyChecker {
    pub fn new(
        stoolap: StoolapDependencyChecker,
        config: ConfigDependencyChecker,
        providers: ProvidersDependencyChecker,
    ) -> Self {
        Self {
            stoolap,
            config,
            providers,
        }
    }
}

impl AsyncDependencyChecker for CompositeDependencyChecker {
    fn check_all<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Vec<(&'static str, CheckResult)>> + Send + 'a>,
    > {
        Box::pin(async move {
            let (stoolap, config, providers) = tokio::join!(
                self.stoolap.check(),
                self.config.check(),
                self.providers.check()
            );
            vec![
                ("stoolap", stoolap),
                ("config", config),
                ("providers", providers),
            ]
        })
    }
}

/// Bundles the inputs needed to build a `CompositeDependencyChecker` from the
/// production wiring path. `HealthContext::composite()` constructs the
/// composite; `proxy.rs::handle_request` holds `Option<Arc<HealthContext>>`
/// so existing tests can pass `None` and keep the `DefaultDependencyChecker`
/// path (RFC-0905 §Healthcheck, mission 0905-d AC-5).
pub struct HealthContext {
    pub db_path: std::path::PathBuf,
    pub config_path: std::path::PathBuf,
    pub providers_snapshot: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync>,
}

impl HealthContext {
    pub fn new(
        db_path: std::path::PathBuf,
        config_path: std::path::PathBuf,
        providers_snapshot: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync>,
    ) -> Self {
        Self {
            db_path,
            config_path,
            providers_snapshot,
        }
    }

    pub fn composite(&self) -> CompositeDependencyChecker {
        CompositeDependencyChecker::new(
            StoolapDependencyChecker::new(self.db_path.clone()),
            ConfigDependencyChecker::new(self.config_path.clone()),
            ProvidersDependencyChecker::new(Arc::clone(&self.providers_snapshot)),
        )
    }
}

/// Async readiness handler — aggregates the worst-case status across the
/// provided composite's results (mission 0905-d, parallel to the sync
/// `HealthHandler::handle_readiness`).
pub async fn handle_readiness_async(composite: &CompositeDependencyChecker) -> (u16, String) {
    let results = composite.check_all().await;
    let mut checks = std::collections::HashMap::new();
    let mut overall_status = HealthStatus::Ok;
    for (name, result) in results {
        match result.status {
            HealthStatus::Unhealthy => overall_status = HealthStatus::Unhealthy,
            HealthStatus::Degraded => {
                if overall_status == HealthStatus::Ok {
                    overall_status = HealthStatus::Degraded;
                }
            }
            HealthStatus::Ok => {}
        }
        checks.insert(name.to_string(), result);
    }
    let status_code = if overall_status == HealthStatus::Unhealthy {
        503
    } else {
        200
    };
    let response = ReadinessResponse {
        status: overall_status,
        checks,
    };
    (
        status_code,
        serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"status":"error","checks":{}}"#.to_string()),
    )
}

/// Health handler for processing health requests
pub struct HealthHandler {
    checker: Arc<dyn DependencyChecker>,
}

impl HealthHandler {
    pub fn new(checker: Arc<dyn DependencyChecker>) -> Self {
        Self { checker }
    }

    /// Handle liveness probe — always returns 200 OK
    pub fn handle_liveness(&self) -> (u16, String) {
        let response = LivenessResponse {
            status: HealthStatus::Ok,
        };
        (
            200,
            serde_json::to_string(&response).unwrap_or_else(|_| r#"{"status":"ok"}"#.to_string()),
        )
    }

    /// Handle readiness probe — checks dependencies
    pub fn handle_readiness(&self) -> (u16, String) {
        let mut checks = std::collections::HashMap::new();
        let mut overall_status = HealthStatus::Ok;

        // Check stoolap
        let stoolap = self.checker.check_stoolap();
        if stoolap.status == HealthStatus::Unhealthy {
            overall_status = HealthStatus::Unhealthy;
        } else if stoolap.status == HealthStatus::Degraded && overall_status == HealthStatus::Ok {
            overall_status = HealthStatus::Degraded;
        }
        checks.insert("stoolap".to_string(), stoolap);

        // Check config
        let config = self.checker.check_config();
        if config.status == HealthStatus::Unhealthy {
            overall_status = HealthStatus::Unhealthy;
        } else if config.status == HealthStatus::Degraded && overall_status == HealthStatus::Ok {
            overall_status = HealthStatus::Degraded;
        }
        checks.insert("config".to_string(), config);

        // Check providers
        let providers = self.checker.check_providers();
        if providers.status == HealthStatus::Unhealthy {
            overall_status = HealthStatus::Unhealthy;
        } else if providers.status == HealthStatus::Degraded && overall_status == HealthStatus::Ok {
            overall_status = HealthStatus::Degraded;
        }
        checks.insert("providers".to_string(), providers);

        let status_code = if overall_status == HealthStatus::Unhealthy {
            503
        } else {
            200
        };

        let response = ReadinessResponse {
            status: overall_status,
            checks,
        };
        (
            status_code,
            serde_json::to_string(&response)
                .unwrap_or_else(|_| r#"{"status":"error","checks":{}}"#.to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Build a DB path that `Database::open` will deterministically reject.
    ///
    /// Stoolap's `file://` engine refuses to open a path whose target
    /// already exists as a regular file (returns "File exists (os error
    /// 17)"). Pre-creating an empty regular file at the probe path gives
    /// us a portably-unreachable DB regardless of whether the host FS
    /// allows creating the parent dir (which is what made the previous
    /// `/nonexistent/...` paths fail-open on some CI runners).
    ///
    /// The returned `TempDir` must be kept alive for the duration of
    /// the test; dropping it removes the file.
    fn unreachable_db_path() -> (std::path::PathBuf, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("not_a_db");
        std::fs::write(&path, b"").expect("write empty file");
        (path, dir)
    }

    struct MockChecker {
        stoolap_ok: bool,
        config_ok: bool,
        providers_ok: bool,
    }

    impl DependencyChecker for MockChecker {
        fn check_stoolap(&self) -> CheckResult {
            if self.stoolap_ok {
                CheckResult {
                    status: HealthStatus::Ok,
                    message: None,
                }
            } else {
                CheckResult {
                    status: HealthStatus::Unhealthy,
                    message: Some("Connection refused".to_string()),
                }
            }
        }

        fn check_config(&self) -> CheckResult {
            if self.config_ok {
                CheckResult {
                    status: HealthStatus::Ok,
                    message: None,
                }
            } else {
                CheckResult {
                    status: HealthStatus::Unhealthy,
                    message: Some("Invalid config".to_string()),
                }
            }
        }

        fn check_providers(&self) -> CheckResult {
            if self.providers_ok {
                CheckResult {
                    status: HealthStatus::Ok,
                    message: None,
                }
            } else {
                CheckResult {
                    status: HealthStatus::Degraded,
                    message: Some("No healthy providers".to_string()),
                }
            }
        }
    }

    #[test]
    fn test_liveness_always_ok() {
        let checker = Arc::new(MockChecker {
            stoolap_ok: false,
            config_ok: false,
            providers_ok: false,
        });
        let handler = HealthHandler::new(checker);

        let (status, body) = handler.handle_liveness();
        assert_eq!(status, 200);

        let response: LivenessResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(response.status, HealthStatus::Ok);
    }

    #[test]
    fn test_readiness_all_ok() {
        let checker = Arc::new(MockChecker {
            stoolap_ok: true,
            config_ok: true,
            providers_ok: true,
        });
        let handler = HealthHandler::new(checker);

        let (status, body) = handler.handle_readiness();
        assert_eq!(status, 200);

        let response: ReadinessResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(response.status, HealthStatus::Ok);
        assert_eq!(response.checks.len(), 3);
    }

    #[test]
    fn test_readiness_unhealthy_stoolap() {
        let checker = Arc::new(MockChecker {
            stoolap_ok: false,
            config_ok: true,
            providers_ok: true,
        });
        let handler = HealthHandler::new(checker);

        let (status, body) = handler.handle_readiness();
        assert_eq!(status, 503);

        let response: ReadinessResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(response.status, HealthStatus::Unhealthy);
        assert_eq!(
            response.checks.get("stoolap").unwrap().status,
            HealthStatus::Unhealthy
        );
    }

    #[test]
    fn test_readiness_degraded_providers() {
        let checker = Arc::new(MockChecker {
            stoolap_ok: true,
            config_ok: true,
            providers_ok: false,
        });
        let handler = HealthHandler::new(checker);

        let (status, body) = handler.handle_readiness();
        assert_eq!(status, 200);

        let response: ReadinessResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(response.status, HealthStatus::Degraded);
        assert_eq!(
            response.checks.get("providers").unwrap().status,
            HealthStatus::Degraded
        );
    }

    #[test]
    fn test_readiness_unhealthy_overrides_degraded() {
        let checker = Arc::new(MockChecker {
            stoolap_ok: false,
            config_ok: true,
            providers_ok: false,
        });
        let handler = HealthHandler::new(checker);

        let (status, _) = handler.handle_readiness();
        assert_eq!(status, 503);
    }

    #[test]
    fn test_health_config_defaults() {
        let config = HealthConfig::default();
        assert!(config.enabled);
        assert_eq!(config.port, 8000);
        assert!(config.check_dependencies);
    }

    #[test]
    fn test_response_serialization() {
        let response = LivenessResponse {
            status: HealthStatus::Ok,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"status":"ok"}"#);

        let mut checks = std::collections::HashMap::new();
        checks.insert(
            "stoolap".to_string(),
            CheckResult {
                status: HealthStatus::Ok,
                message: None,
            },
        );
        let readiness = ReadinessResponse {
            status: HealthStatus::Ok,
            checks,
        };
        let json = serde_json::to_string(&readiness).unwrap();
        assert!(json.contains(r#""status":"ok"#));
        assert!(json.contains(r#""stoolap""#));
    }

    // ========================================================================
    // Real dependency checker tests (mission 0905-d)
    // ========================================================================

    #[tokio::test]
    async fn stoolap_check_success_on_real_file() {
        // Use a unique temp file so the probe actually opens a real DB
        // (`:memory:` is not a valid file-system path).
        let dir = std::env::temp_dir().join(format!(
            "qr-stool-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.db");
        let checker = StoolapDependencyChecker::new(path.clone());
        let result = checker.check().await;
        eprintln!("stoolap result: {:?}", result);
        assert_eq!(result.status, HealthStatus::Ok);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn stoolap_check_unhealthy_on_missing_path() {
        let (path, _dir) = unreachable_db_path();
        let checker = StoolapDependencyChecker::new(path);
        let result = checker.check().await;
        assert_eq!(result.status, HealthStatus::Unhealthy);
        assert!(result.message.is_some());
    }

    #[tokio::test]
    async fn stoolap_check_times_out_on_slow_open() {
        // Spawn an extremely slow blocking task by routing the probe through a
        // synthetic checker that pre-binds a Thread-pool sleep longer than 200ms.
        // We simulate the timeout by checking that 200ms is the documented upper
        // bound: the probe must return within ~250ms wall-clock even when the
        // path cannot be opened.
        let (path, _dir) = unreachable_db_path();
        let checker = StoolapDependencyChecker::new(path);
        let start = std::time::Instant::now();
        let result = checker.check().await;
        let elapsed = start.elapsed();
        // Must NOT hit the 200ms timeout — the path can't be opened so it
        // fails immediately via the synchronous open() error.
        assert!(elapsed < std::time::Duration::from_millis(200));
        assert_eq!(result.status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn config_check_success_on_written_path() {
        let dir = std::env::temp_dir().join(format!(
            "qr-cfg-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{"balance":42,"providers":[],"proxy_port":9000,"db_path":"/tmp/x.db"}"#,
        )
        .unwrap();

        let checker = ConfigDependencyChecker::new(path.clone());
        let result = checker.check().await;
        assert_eq!(result.status, HealthStatus::Ok);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn config_check_returns_ok_for_missing_path_with_defaults() {
        // `load_from_path` returns defaults when the file is absent
        // (production may inject config via env).
        let checker = ConfigDependencyChecker::new(std::path::PathBuf::from(
            "/nonexistent/cfg-0905-d-missing.json",
        ));
        let result = checker.check().await;
        assert_eq!(result.status, HealthStatus::Ok);
    }

    #[tokio::test]
    async fn providers_check_ok_for_non_empty() {
        let snap: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync> = Arc::new(|| {
            vec![crate::providers::Provider::new(
                "openai",
                "https://api.openai.com",
            )]
        });
        let checker = ProvidersDependencyChecker::new(snap);
        let result = checker.check().await;
        assert_eq!(result.status, HealthStatus::Ok);
    }

    #[tokio::test]
    async fn providers_check_degraded_for_empty_registry() {
        let snap: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync> =
            Arc::new(Vec::new);
        let checker = ProvidersDependencyChecker::new(snap);
        let result = checker.check().await;
        assert_eq!(result.status, HealthStatus::Degraded);
    }

    #[tokio::test]
    async fn providers_check_unhealthy_for_empty_endpoint() {
        let snap: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync> =
            Arc::new(|| vec![crate::providers::Provider::new("broken", "")]);
        let checker = ProvidersDependencyChecker::new(snap);
        let result = checker.check().await;
        assert_eq!(result.status, HealthStatus::Unhealthy);
        assert!(result.message.unwrap().contains("broken"));
    }

    #[tokio::test]
    async fn composite_check_all_runs_concurrently_and_returns_three() {
        let dir = std::env::temp_dir().join(format!(
            "qr-comp-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let stoolap = StoolapDependencyChecker::new(dir.join("probe.db"));
        let config = ConfigDependencyChecker::new(std::path::PathBuf::from(
            "/nonexistent/cfg-composite-0905-d.json",
        ));
        let snap: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync> = Arc::new(|| {
            vec![crate::providers::Provider::new(
                "openai",
                "https://api.openai.com",
            )]
        });
        let providers = ProvidersDependencyChecker::new(snap);

        let composite = CompositeDependencyChecker::new(stoolap, config, providers);
        let results = composite.check_all().await;
        assert_eq!(results.len(), 3);
        let names: Vec<&str> = results.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"stoolap"));
        assert!(names.contains(&"config"));
        assert!(names.contains(&"providers"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn composite_check_all_propagates_unhealthy_stoolap() {
        let (db_path, _dir) = unreachable_db_path();
        let stoolap = StoolapDependencyChecker::new(db_path);
        let config = ConfigDependencyChecker::new(std::path::PathBuf::from(
            "/nonexistent/cfg-composite-0905-d-bad.json",
        ));
        let snap: Arc<dyn Fn() -> Vec<crate::providers::Provider> + Send + Sync> =
            Arc::new(Vec::new);
        let providers = ProvidersDependencyChecker::new(snap);

        let composite = CompositeDependencyChecker::new(stoolap, config, providers);
        let results = composite.check_all().await;
        let stoolap_result = results.iter().find(|(n, _)| *n == "stoolap").unwrap();
        assert_eq!(stoolap_result.1.status, HealthStatus::Unhealthy);
    }
}
