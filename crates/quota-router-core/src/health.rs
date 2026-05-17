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
        (200, serde_json::to_string(&response).unwrap())
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
        (status_code, serde_json::to_string(&response).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
}
