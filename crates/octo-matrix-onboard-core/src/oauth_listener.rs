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
use axum::{extract::Request, response::Html, routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tracing::{info, warn};

/// Result of a single OAuth callback.
#[derive(Debug, PartialEq, Eq)]
pub enum CallbackResult {
    /// IdP returned a valid `code` (and matching `state`).
    ///
    /// `raw_query` is the **entire, unparsed query string** the IdP
    /// redirected with — e.g. `code=abc&&state=xyz`. The CLI passes it
    /// unchanged to `OAuth::finish_login(UrlOrQuery::Query(raw_query))`
    /// so the SDK's state validation can run. We do NOT decode and
    /// re-encode the values: that round-trip would mangle any
    /// non-trivial encoding (e.g. `+` ↔ `%2B`, `%` ↔ `%25`, `&&` ↔
    /// `%26`) and break codes the IdP returns with such characters.
    Code { raw_query: String },
    /// IdP returned an error (`error=...&&error_description=...`).
    IdpError { code: String, description: String },
}

/// Spawn a single-shot listener on `127.0.0.1:port` and return the
/// captured callback (code + state, or IdP error) when the IdP
/// redirects to it.
///
/// The redirect_uri the CLI must register with the IdP is
/// `http://127.0.0.1:{port}/callback`.
pub async fn listen_once(port: u16) -> Result<CallbackResult> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port)
        .parse()
        .expect("127.0.0.1:port is always a valid SocketAddr");
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
        get(move |req: Request| {
            let result_slot = result_for_handler.clone();
            let shutdown = shutdown_for_handler.clone();
            async move {
                // Read the raw query string verbatim. `Query<T>` would
                // percent-decode the values into a HashMap, after
                // which we cannot faithfully reconstruct the original
                // bytes (e.g. `+` and `%2B` collapse to the same
                // String). The SDK's `UrlOrQuery::Query` accepts the
                // raw string and re-parses it on its side, which is
                // the supported path.
                let raw_query = req.uri().query().unwrap_or("").to_owned();
                let response_html = if let Some(code) = parse_query_key(&raw_query, "code") {
                    info!("Received OAuth code (length={})", code.len());
                    let mut slot = result_slot.lock().await;
                    *slot = Some(CallbackResult::Code { raw_query });
                    "<html><body><h1>octo-matrix-onboard: success</h1>\
                         <p>You can close this tab and return to the terminal.</p></body></html>"
                } else if let Some(err) = parse_query_key(&raw_query, "error") {
                    let desc = parse_query_key(&raw_query, "error_description").unwrap_or_default();
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

/// Extract a single key's value from a raw query string WITHOUT
/// percent-decoding. This is good enough for the success/error
/// branching (we only need to know which keys are present and the
/// raw-bytes value of `code` for length logging). The full raw
/// string is preserved for the SDK to re-parse on its own side.
fn parse_query_key(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        if let Some((lhs, rhs)) = pair.split_once('=') {
            if lhs == key {
                return Some(rhs.to_owned());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_key_finds_code() {
        let q = "code=abc123&state=xyz";
        assert_eq!(parse_query_key(q, "code").as_deref(), Some("abc123"));
        assert_eq!(parse_query_key(q, "state").as_deref(), Some("xyz"));
        assert!(parse_query_key(q, "missing").is_none());
    }

    #[test]
    fn parse_query_key_preserves_percent_encoding() {
        // R1-H10 regression: the old Query<HashMap> extractor
        // percent-decoded `abc%23def` to `abc#def`, losing the
        // distinction. The raw extractor must keep the bytes
        // verbatim.
        let q = "code=abc%23def&state=foo%26bar";
        assert_eq!(parse_query_key(q, "code").as_deref(), Some("abc%23def"));
        assert_eq!(parse_query_key(q, "state").as_deref(), Some("foo%26bar"));
    }

    #[test]
    fn parse_query_key_handles_empty() {
        assert!(parse_query_key("", "code").is_none());
        assert!(parse_query_key("&", "code").is_none());
        assert!(parse_query_key("code", "code").is_none()); // no '='
    }

    #[test]
    fn parse_query_key_finds_error() {
        let q = "error=access_denied&error_description=user+cancelled";
        assert_eq!(
            parse_query_key(q, "error").as_deref(),
            Some("access_denied")
        );
        assert_eq!(
            parse_query_key(q, "error_description").as_deref(),
            Some("user+cancelled")
        );
    }

    /// Pick a free port by binding to port 0 (OS-assigned), reading
    /// the assigned port, then closing the handle. There's a small
    /// race window before `listen_once` re-binds, but on a quiet
    /// test box it's near-zero. R2-M9: replaces the previous
    /// hardcoded-port tests that silently skipped on collision.
    fn free_port() -> u16 {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("port 0 must be bindable on the test box");
        let port = l.local_addr().expect("local_addr on a fresh bind").port();
        drop(l);
        port
    }

    /// R1-M2 + R2-M9: a full listener round-trip on the IdP-error
    /// path. The listener binds, serves one request, and returns
    /// `CallbackResult::IdpError`. Uses an OS-assigned port via
    /// `free_port()` so a busy test box can never silently skip
    /// the test (the previous hardcoded `49152` would skip on
    /// collision).
    #[tokio::test]
    async fn listen_once_returns_idp_error_on_error_query() {
        let port = free_port();
        let listener_task = tokio::spawn(async move { listen_once(port).await });

        // Give the listener a beat to bind.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Send a raw HTTP/1.1 request to avoid pulling in a
        // reqwest dev-dep (the SDK already has reqwest, but the
        // core crate deliberately doesn't depend on it for the
        // listener path).
        let raw = b"GET /callback?error=access_denied&error_description=user+cancelled HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect to listener");
        use tokio::io::AsyncWriteExt;
        stream.write_all(raw).await.expect("send request");
        // Read until EOF (the server sends Connection: close).
        let mut buf = Vec::new();
        use tokio::io::AsyncReadExt;
        let _ = stream.read_to_end(&mut buf).await;
        let response = String::from_utf8_lossy(&buf);
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "callback returned {response}"
        );

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), listener_task)
            .await
            .expect("listener did not complete in 5s")
            .expect("listener task panicked")
            .expect("listen_once returned an error");

        match result {
            CallbackResult::IdpError { code, description } => {
                assert_eq!(code, "access_denied");
                assert_eq!(description, "user+cancelled");
            }
            other => panic!("expected IdpError, got {:?}", other),
        }
    }

    /// R1-M2 + R2-M9: same shape, but the IdP returns a valid
    /// `code` and the listener captures it. This is the happy-path
    /// counterpart to the IdpError test.
    #[tokio::test]
    async fn listen_once_returns_code_on_success_query() {
        let port = free_port();
        let listener_task = tokio::spawn(async move { listen_once(port).await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let raw = b"GET /callback?code=authcode123&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect to listener");
        use tokio::io::AsyncWriteExt;
        stream.write_all(raw).await.expect("send request");
        let mut buf = Vec::new();
        use tokio::io::AsyncReadExt;
        let _ = stream.read_to_end(&mut buf).await;

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), listener_task)
            .await
            .expect("listener did not complete in 5s")
            .expect("listener task panicked")
            .expect("listen_once returned an error");

        match result {
            CallbackResult::Code { raw_query } => {
                assert!(
                    raw_query.contains("code=authcode123"),
                    "raw_query={raw_query}"
                );
                assert!(raw_query.contains("state=xyz"), "raw_query={raw_query}");
            }
            other => panic!("expected Code, got {:?}", other),
        }
    }
}
