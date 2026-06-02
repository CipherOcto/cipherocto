//! Localhost OAuth callback listener.
//!
//! Used by OIDC and SSO login modes (mission 0850h-a) to receive the
//! authorization code from the homeserver's IdP. Binds to
//! `127.0.0.1:port` (NEVER `0.0.0.0` — PII rule from the design doc).
//!
//! Single-use: the listener serves exactly one request, captures the
//! raw query string (which includes both `code` and `state`), and
//! shuts down. The query string is passed to
//! `OAuth::finish_login(UrlOrQuery::Query(...))` so the SDK's state
//! validation can run.
//!
//! If the IdP returns an error (`error=...`), the listener returns it
//! via the result channel so the CLI can surface a meaningful error.

use anyhow::{Context, Result};
use axum::{extract::Query, response::Html, routing::get, Router};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tracing::{info, warn};

/// Result of a single OAuth callback.
#[derive(Debug)]
pub enum CallbackResult {
    /// IdP returned a valid `code` (and matching `state`).
    ///
    /// `raw_query` is the entire query string the IdP redirected
    /// with, e.g. `code=abc&state=xyz`. The CLI passes it to
    /// `OAuth::finish_login(UrlOrQuery::Query(raw_query))` so the
    /// SDK's state validation can run.
    Code { raw_query: String },
    /// IdP returned an error (`error=...&error_description=...`).
    IdpError { code: String, description: String },
}

#[derive(Debug)]
struct CallbackParams {
    code: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

impl CallbackParams {
    fn from_query_map(params: &HashMap<String, String>) -> Self {
        Self {
            code: params.get("code").cloned(),
            error: params.get("error").cloned(),
            error_description: params.get("error_description").cloned(),
        }
    }
}

/// Spawn a single-shot listener on `127.0.0.1:port` and return the
/// captured callback (code + state, or IdP error) when the IdP
/// redirects to it.
///
/// The redirect_uri the CLI must register with the IdP is
/// `http://127.0.0.1:{port}/callback`.
pub async fn listen_once(port: u16) -> Result<CallbackResult> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind {} — is the port already in use?", addr))?;

    let shutdown = Arc::new(Notify::new());
    let result_slot: Arc<tokio::sync::Mutex<Option<CallbackResult>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    let result_for_handler = result_slot.clone();
    let shutdown_for_handler = shutdown.clone();

    let app = Router::new().route(
        "/callback",
        get(move |Query(params): Query<HashMap<String, String>>| {
            let result_slot = result_for_handler.clone();
            let shutdown = shutdown_for_handler.clone();
            async move {
                let parsed = CallbackParams::from_query_map(&params);
                let response_html = if let Some(code) = parsed.code {
                    info!("Received OAuth code (length={})", code.len());
                    // Rebuild the raw query so the SDK can validate state.
                    let raw_query = rebuild_query(&code, params.get("state").map(String::as_str));
                    let mut slot = result_slot.lock().await;
                    *slot = Some(CallbackResult::Code { raw_query });
                    "<html><body><h1>octo-matrix-onboard: success</h1>\
                         <p>You can close this tab and return to the terminal.</p></body></html>"
                } else if let Some(err) = parsed.error {
                    let desc = parsed.error_description.unwrap_or_default();
                    warn!("IdP returned error: {} — {}", err, desc);
                    let mut slot = result_slot.lock().await;
                    *slot = Some(CallbackResult::IdpError {
                        code: err,
                        description: desc,
                    });
                    "<html><body><h1>octo-matrix-onboard: auth rejected</h1>\
                         <p>See terminal for details.</p></body></html>"
                } else {
                    warn!("Callback received without code or error");
                    "<html><body><h1>octo-matrix-onboard: malformed callback</h1></body></html>"
                };
                shutdown.notify_waiters();
                Html(response_html)
            }
        }),
    );

    let shutdown_signal = async move { shutdown.notified().await };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .context("Listener serve loop failed")?;

    let mut slot = result_slot.lock().await;
    slot.take()
        .ok_or_else(|| anyhow::anyhow!("Listener exited without capturing a callback"))
}

fn rebuild_query(code: &str, state: Option<&str>) -> String {
    match state {
        Some(s) => format!("code={}&state={}", urlencoded(code), urlencoded(s)),
        None => format!("code={}", urlencoded(code)),
    }
}

fn urlencoded(s: &str) -> String {
    // Minimal percent-encoding for the two query keys we know about.
    // The IdP returns URL-encoded values; we re-encode here so the
    // SDK can parse the query string uniformly.
    s.replace('+', "%2B")
        .replace(' ', "+")
        .replace('%', "%25")
        .replace('&', "%26")
        .replace('=', "%3D")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_params_extracts_code() {
        let mut params = HashMap::new();
        params.insert("code".to_string(), "abc123".to_string());
        let parsed = CallbackParams::from_query_map(&params);
        assert_eq!(parsed.code.as_deref(), Some("abc123"));
        assert!(parsed.error.is_none());
    }

    #[test]
    fn callback_params_extracts_idp_error() {
        let mut params = HashMap::new();
        params.insert("error".to_string(), "access_denied".to_string());
        params.insert(
            "error_description".to_string(),
            "user cancelled".to_string(),
        );
        let parsed = CallbackParams::from_query_map(&params);
        assert!(parsed.code.is_none());
        assert_eq!(parsed.error.as_deref(), Some("access_denied"));
        assert_eq!(parsed.error_description.as_deref(), Some("user cancelled"));
    }

    #[test]
    fn rebuild_query_with_state() {
        let q = rebuild_query("abc", Some("xyz"));
        assert_eq!(q, "code=abc&state=xyz");
    }

    #[test]
    fn rebuild_query_without_state() {
        let q = rebuild_query("abc", None);
        assert_eq!(q, "code=abc");
    }
}
