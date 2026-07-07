//! HTTP health + readiness + Prometheus `/metrics` server.
//!
//! Phase 5 Part B of `docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase5.md`
//! §Observability. Three routes:
//!
//! - `GET /health` — liveness. Returns 200 when `is_live` is set
//!   (process is up + bound to the unix socket); 503 otherwise.
//! - `GET /ready`  — readiness. Returns 200 when `is_ready` is set
//!   (`connected && session_valid`); 503 otherwise.
//! - `GET /metrics` — Prometheus text exposition. **Always bearer-protected**;
//!   returns 401 on missing/wrong bearer, regardless of TCP bind.
//!
//! The server runs on loopback only — it refuses to bind to a
//! non-loopback address (security constraint, plan §A7). Operators
//! wanting remote access must proxy + authenticate at the proxy
//! layer.
//!
//! The bearer is loaded from `Metrics::bearer_token` /
//! [`crate::config::HealthConfig`] — a 256-bit hex string stored
//! in the env var named by `MetricsConfig::bearer_token_env`
//! (default `OCTO_WHATSAPP_METRICS_TOKEN`). When the env var is
//! unset AND `health.bearer_required = false`, `/metrics` accepts
//! every request but logs a `WARN` once on first hit.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, HeaderMap, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::metrics::Metrics;

/// Default env-var name holding the metrics bearer token. Operators
/// may override via `[observability.health] bearer_token_env` in the
/// runtime config.
pub const METRICS_BEARER_ENV: &str = "OCTO_WHATSAPP_METRICS_TOKEN";

/// Handle returned from [`run_health_server`]. Lets the supervisor
/// peek the bound address + observe a "server running" flag.
#[derive(Debug)]
pub struct HealthServerHandle {
    /// Bound local address (resolved after `bind` returns).
    pub addr: SocketAddr,
    /// Set to `true` once the axum server has finished binding +
    /// has started accepting connections. Read by tests that want
    /// to wait for the server to be live.
    pub is_running: Arc<AtomicBool>,
    /// Cancellation token used to stop the server from the outside.
    pub cancel: CancellationToken,
}

/// Resolved bearer config for `/metrics`. When `token` is `None`
/// AND `bearer_required` is `false`, the route accepts any request
/// (and emits a single warn-log on first hit).
#[derive(Debug, Clone)]
pub struct BearerConfig {
    pub token: Option<String>,
    pub bearer_required: bool,
}

impl BearerConfig {
    pub fn from_env_or_config(env_var: &str, bearer_required: bool) -> Self {
        let token = std::env::var(env_var)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Self {
            token,
            bearer_required,
        }
    }

    pub fn validate(&self, env_var: &str) -> Result<(), String> {
        if self.bearer_required && self.token.is_none() {
            return Err(format!(
                "bearer_required=true but env var {env_var} is not set"
            ));
        }
        Ok(())
    }
}

/// Reject a non-loopback bind address. Plan §A7 — health surfaces
/// must bind loopback only.
pub fn require_loopback(bind: SocketAddr) -> Result<(), String> {
    if bind.ip().is_loopback() {
        Ok(())
    } else {
        Err(format!(
            "health server bind {bind} is not loopback; refusing to start \
             (config: health surfaces must be loopback-only, plan §A7)"
        ))
    }
}

/// Spawn the HTTP server on `bind`. Returns once the bind + early
/// boot steps complete; the listen loop runs on the returned
/// `JoinHandle`.
///
/// `is_ready` and `is_live` are the readiness/liveness flags from
/// the daemon. `bearer` controls `/metrics` authorization.
pub async fn run_health_server(
    bind: SocketAddr,
    metrics: Arc<Metrics>,
    is_ready: Arc<AtomicBool>,
    is_live: Arc<AtomicBool>,
    bearer: BearerConfig,
    cancel: CancellationToken,
) -> Result<HealthServerHandle, String> {
    require_loopback(bind)?;
    bearer.validate(METRICS_BEARER_ENV)?;

    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| format!("bind {bind}: {e}"))?;
    let bound = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    let is_running = Arc::new(AtomicBool::new(false));
    let cancel_listener = cancel.clone();
    let is_running_listener = is_running.clone();
    let app: Router = build_router(metrics, is_ready, is_live, bearer);

    tokio::spawn(async move {
        let server = axum::serve(listener, app);
        let cancel_for_shutdown = cancel_listener.clone();
        let graceful = server.with_graceful_shutdown(async move {
            cancel_for_shutdown.cancelled().await;
        });
        info!(addr = %bound, "health server: starting");
        is_running_listener.store(true, Ordering::SeqCst);
        if let Err(e) = graceful.await {
            warn!(error = %e, addr = %bound, "health server: axum::serve exited with error");
        }
        is_running_listener.store(false, Ordering::SeqCst);
        info!(addr = %bound, "health server: stopped");
    });

    Ok(HealthServerHandle {
        addr: bound,
        is_running,
        cancel,
    })
}

pub(crate) fn build_router(
    metrics: Arc<Metrics>,
    is_ready: Arc<AtomicBool>,
    is_live: Arc<AtomicBool>,
    bearer: BearerConfig,
) -> Router {
    let state = HealthServerState {
        metrics,
        is_ready,
        is_live,
        bearer,
    };
    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

/// Internal axum state carried by `Router::with_state`.
#[derive(Clone)]
struct HealthServerState {
    metrics: Arc<Metrics>,
    is_ready: Arc<AtomicBool>,
    is_live: Arc<AtomicBool>,
    bearer: BearerConfig,
}

async fn health_handler(
    axum::extract::State(s): axum::extract::State<HealthServerState>,
) -> Response<Body> {
    if s.is_live.load(Ordering::SeqCst) {
        (StatusCode::OK, "alive\n").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not-alive\n").into_response()
    }
}

async fn ready_handler(
    axum::extract::State(s): axum::extract::State<HealthServerState>,
) -> Response<Body> {
    if s.is_ready.load(Ordering::SeqCst) {
        (StatusCode::OK, "ready\n").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not-ready\n").into_response()
    }
}

async fn metrics_handler(
    axum::extract::State(s): axum::extract::State<HealthServerState>,
    headers: HeaderMap,
) -> Response<Body> {
    // Authorization: bearer may be absent when `bearer_required = false`.
    // In that case the route accepts all requests but emits a one-shot
    // WARN so operators see the unprotected surface in logs.
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|x| x.to_string());
    let presented_bearer = presented
        .as_deref()
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|x| x.trim().to_string());

    let metrics = &s.metrics;
    let bearer = &s.bearer;
    match (bearer.token.as_deref(), presented_bearer.as_deref()) {
        (Some(expected), Some(presented)) if !expected.is_empty() => {
            // Constant-time comparison defends against trivial timing
            // side-channels; secret length is bounded (≤ a few KiB).
            let valid = constant_time_eq(expected.as_bytes(), presented.as_bytes());
            if !valid {
                metrics.inc_auth_failed(&peer_ip_label(&headers));
                return unauthorized_response();
            }
        }
        (Some(_), _) => {
            // Token configured but missing/invalid header.
            metrics.inc_auth_failed(&peer_ip_label(&headers));
            return unauthorized_response();
        }
        (None, _) if bearer.bearer_required => {
            // Should be impossible — `BearerConfig::validate` rejects
            // this combination at boot. Defensive double-check.
            return unauthorized_response();
        }
        (None, _) => {
            // No bearer configured, bearer_required=false — accept.
            // A single warn line is logged from the supervisor; we keep
            // the route hot-path quiet here.
            tracing::debug!("metrics: bearer not configured (bearer_required=false)");
        }
    }

    let text = match metrics.render() {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, "metrics: render failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "metrics render failed\n").into_response();
        }
    };

    let mut resp = (StatusCode::OK, text).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        "text/plain; version=0.0.4; charset=utf-8"
            .parse()
            .expect("static header value parses"),
    );
    resp
}

fn unauthorized_response() -> Response<Body> {
    (StatusCode::UNAUTHORIZED, "unauthorized\n").into_response()
}

/// Extract a peer-IP label from the request's `Forwarded` /
/// `X-Forwarded-For` / socket address (none available here, so
/// fall back to `127.0.0.1`).
fn peer_ip_label(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| {
            // Accept only literal IPv4/IPv6 — anything else means a
            // proxy returned junk.
            s.parse::<IpAddr>().is_ok()
        })
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_loopback_accepts_loopback_v4() {
        let ok: SocketAddr = "127.0.0.1:7778".parse().unwrap();
        assert!(require_loopback(ok).is_ok());
    }

    #[test]
    fn require_loopback_rejects_non_loopback_v4() {
        let bad: SocketAddr = "0.0.0.0:7778".parse().unwrap();
        let err = require_loopback(bad).unwrap_err();
        assert!(err.contains("not loopback"));
    }

    #[test]
    fn require_loopback_rejects_public_v4() {
        let bad: SocketAddr = "8.8.8.8:7778".parse().unwrap();
        assert!(require_loopback(bad).is_err());
    }

    #[test]
    fn bearer_config_validate_rejects_missing_token_when_required() {
        // Env var unset; force bearer_required=true.
        let prev = std::env::var(METRICS_BEARER_ENV).ok();
        std::env::remove_var(METRICS_BEARER_ENV);
        let cfg = BearerConfig {
            token: None,
            bearer_required: true,
        };
        assert!(cfg.validate(METRICS_BEARER_ENV).is_err());
        if let Some(v) = prev {
            std::env::set_var(METRICS_BEARER_ENV, v);
        }
    }

    #[test]
    fn bearer_config_validate_passes_when_token_set() {
        let cfg = BearerConfig {
            token: Some("abc".into()),
            bearer_required: true,
        };
        assert!(cfg.validate(METRICS_BEARER_ENV).is_ok());
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    // ---- TCP-based black-box tests ----
    //
    // These spin up the real axum server bound to `127.0.0.1:0`
    // (kernel-assigned port) and exercise each route over a raw
    // `tokio::net::TcpStream`. Fully hermetic; no network egress.

    use std::sync::atomic::AtomicBool;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    async fn spawn_test_server(
        bearer: BearerConfig,
    ) -> (
        SocketAddr,
        Arc<AtomicBool>,
        Arc<AtomicBool>,
        Arc<Metrics>,
        CancellationToken,
    ) {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        // Pre-bind a TCP listener to discover an ephemeral port, then
        // pass that port to the server. Avoids races with axum's
        // own `bind()` call.
        let probe = tokio::net::TcpListener::bind(bind).await.unwrap();
        let addr = probe.local_addr().unwrap();
        drop(probe);
        let metrics = Metrics::new(b"k").unwrap();
        let is_ready = Arc::new(AtomicBool::new(false));
        let is_live = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();
        let handle = run_health_server(
            addr,
            metrics.clone(),
            is_ready.clone(),
            is_live.clone(),
            bearer,
            cancel.clone(),
        )
        .await
        .unwrap();
        // Give axum a beat to enter the accept loop.
        for _ in 0..20 {
            if handle.is_running.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        (addr, is_ready, is_live, metrics, cancel)
    }

    async fn http_get(addr: SocketAddr, path: &str, bearer: Option<&str>) -> (u16, String) {
        let mut s = TcpStream::connect(addr).await.unwrap();
        let req = match bearer {
            Some(b) => format!(
                "GET {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {b}\r\nConnection: close\r\n\r\n"
            ),
            None => format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
        };
        s.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::with_capacity(4 * 1024);
        s.read_to_end(&mut buf).await.unwrap();
        let s_str = String::from_utf8_lossy(&buf).into_owned();
        let status = s_str
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        // Body is everything after the first \r\n\r\n.
        let body = s_str
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        (status, body)
    }

    #[tokio::test]
    async fn health_returns_503_when_not_live_then_200_when_live() {
        let (addr, _is_ready, is_live, _m, cancel) = spawn_test_server(BearerConfig {
            token: None,
            bearer_required: false,
        })
        .await;
        // Initially not live.
        let (status, body) = http_get(addr, "/health", None).await;
        assert_eq!(status, 503, "expected 503; body={body}");
        assert!(body.starts_with("not-alive"));
        // Flip liveness.
        is_live.store(true, Ordering::SeqCst);
        let (status, body) = http_get(addr, "/health", None).await;
        assert_eq!(status, 200, "expected 200; body={body}");
        assert!(body.starts_with("alive"));
        cancel.cancel();
    }

    #[tokio::test]
    async fn ready_returns_503_when_not_ready_then_200_when_ready() {
        let (addr, is_ready, _is_live, _m, cancel) = spawn_test_server(BearerConfig {
            token: None,
            bearer_required: false,
        })
        .await;
        let (status, _) = http_get(addr, "/ready", None).await;
        assert_eq!(status, 503);
        is_ready.store(true, Ordering::SeqCst);
        let (status, body) = http_get(addr, "/ready", None).await;
        assert_eq!(status, 200);
        assert!(body.starts_with("ready"));
        cancel.cancel();
    }

    #[tokio::test]
    async fn metrics_returns_401_without_bearer_when_required() {
        let (addr, _r, _l, _m, cancel) = spawn_test_server(BearerConfig {
            token: Some("hunter2-secret".into()),
            bearer_required: true,
        })
        .await;
        let (status, body) = http_get(addr, "/metrics", None).await;
        assert_eq!(status, 401, "expected 401; body={body}");
        cancel.cancel();
    }

    #[tokio::test]
    async fn metrics_returns_200_with_correct_bearer() {
        let (addr, _r, _l, metrics, cancel) = spawn_test_server(BearerConfig {
            token: Some("hunter2-secret".into()),
            bearer_required: true,
        })
        .await;
        metrics.inc_audit_row();
        metrics.inc_audit_row();
        let (status, body) = http_get(addr, "/metrics", Some("hunter2-secret")).await;
        assert_eq!(
            status,
            200,
            "expected 200; body={}",
            body.chars().take(80).collect::<String>()
        );
        assert!(
            body.contains("audit_rows_total 2"),
            "body missing metric: {body}"
        );
        cancel.cancel();
    }

    #[tokio::test]
    async fn metrics_returns_401_with_wrong_bearer() {
        let (addr, _r, _l, _m, cancel) = spawn_test_server(BearerConfig {
            token: Some("hunter2-secret".into()),
            bearer_required: true,
        })
        .await;
        let (status, _) = http_get(addr, "/metrics", Some("wrong-token")).await;
        assert_eq!(status, 401);
        cancel.cancel();
    }

    #[tokio::test]
    async fn metrics_bearer_failure_increments_auth_failed_total() {
        let (addr, _r, _l, metrics, cancel) = spawn_test_server(BearerConfig {
            token: Some("hunter2-secret".into()),
            bearer_required: true,
        })
        .await;
        let _ = http_get(addr, "/metrics", None).await;
        let _ = http_get(addr, "/metrics", Some("wrong")).await;
        let text = metrics.render().unwrap();
        assert!(text.contains("auth_failed_total{ip="), "metrics: {text}");
        cancel.cancel();
    }

    #[tokio::test]
    async fn metrics_open_when_no_bearer_configured_and_not_required() {
        let (addr, _r, _l, metrics, cancel) = spawn_test_server(BearerConfig {
            token: None,
            bearer_required: false,
        })
        .await;
        metrics.inc_audit_row();
        let (status, body) = http_get(addr, "/metrics", None).await;
        assert_eq!(status, 200);
        assert!(body.contains("audit_rows_total 1"));
        cancel.cancel();
    }

    #[tokio::test]
    async fn config_validation_rejects_non_loopback_bind() {
        let cfg = crate::config::WhatsAppRuntimeConfig::from_toml(
            br#"
                name = "x"
                [observability.health]
                http_listen = "0.0.0.0:9999"
            "#,
        );
        assert!(cfg.is_err(), "expected non-loopback bind to be rejected");
    }

    #[tokio::test]
    async fn config_validation_accepts_loopback_bind() {
        let cfg = crate::config::WhatsAppRuntimeConfig::from_toml(
            br#"
                name = "x"
                [observability.health]
                http_listen = "127.0.0.1:7779"
            "#,
        );
        assert!(
            cfg.is_ok(),
            "loopback must be accepted; err={:?}",
            cfg.err()
        );
    }
}
