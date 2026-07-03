//! Mock HTTP server for testing proxy and admin handlers.
//!
//! Provides a lightweight HTTP server that can be used to test
//! request handling without external dependencies.

use std::net::SocketAddr;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Mock HTTP server for testing.
pub struct MockHttpServer {
    pub addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl MockHttpServer {
    /// Start a mock server with a custom handler.
    pub async fn start<F>(handler: F) -> Self
    where
        F: Fn(Request<Incoming>) -> Response<String> + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let handler = Arc::new(handler);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        if let Ok((stream, _)) = result {
                            let handler = handler.clone();
                            tokio::spawn(async move {
                                let io = TokioIo::new(stream);
                                let service = service_fn(move |req| {
                                    let handler = handler.clone();
                                    async move { Ok::<_, std::convert::Infallible>(handler(req)) }
                                });
                                let _ = hyper::server::conn::http1::Builder::new()
                                    .serve_connection(io, service)
                                    .await;
                            });
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        MockHttpServer {
            addr,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Start a server that returns a fixed response.
    pub async fn with_response(status: StatusCode, body: &str) -> Self {
        let body = body.to_string();
        Self::start(move |_req| {
            Response::builder()
                .status(status)
                .body(body.clone())
                .unwrap()
        })
        .await
    }

    /// Start a server that returns JSON.
    pub async fn with_json(data: &serde_json::Value) -> Self {
        let body = data.to_string();
        Self::start(move |_req| {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(body.clone())
                .unwrap()
        })
        .await
    }

    /// Start a server that echoes the request back.
    pub async fn echo() -> Self {
        Self::start(|req| {
            let method = req.method().to_string();
            let uri = req.uri().to_string();
            let body = format!("{} {}", method, uri);
            Response::builder()
                .status(StatusCode::OK)
                .body(body)
                .unwrap()
        })
        .await
    }

    /// Start a server that always returns 500.
    pub async fn error() -> Self {
        Self::with_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").await
    }

    /// Start a server that always returns 429 (rate limited).
    pub async fn rate_limited() -> Self {
        Self::with_response(StatusCode::TOO_MANY_REQUESTS, "Rate Limited").await
    }

    /// Start a server that always returns 401 (unauthorized).
    pub async fn unauthorized() -> Self {
        Self::with_response(StatusCode::UNAUTHORIZED, "Unauthorized").await
    }

    /// Start a server that always returns 403 (forbidden).
    pub async fn forbidden() -> Self {
        Self::with_response(StatusCode::FORBIDDEN, "Forbidden").await
    }

    /// Start a server that always returns 503 (service unavailable).
    pub async fn unavailable() -> Self {
        Self::with_response(StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable").await
    }

    /// Start a server that always returns 408 (timeout).
    pub async fn timeout() -> Self {
        Self::with_response(StatusCode::REQUEST_TIMEOUT, "Request Timeout").await
    }

    /// Start a server that always returns 504 (gateway timeout).
    pub async fn gateway_timeout() -> Self {
        Self::with_response(StatusCode::GATEWAY_TIMEOUT, "Gateway Timeout").await
    }

    /// Get the base URL for this server.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Shut down the server.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_server_echo() {
        let server = MockHttpServer::echo().await;
        let url = format!("{}/test", server.base_url());
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.text().await.unwrap();
        assert!(body.contains("GET"));
        assert!(body.contains("/test"));
    }

    #[tokio::test]
    async fn mock_server_json() {
        let data = serde_json::json!({"key": "value"});
        let server = MockHttpServer::with_json(&data).await;
        let resp = reqwest::get(server.base_url()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["key"], "value");
    }

    #[tokio::test]
    async fn mock_server_error() {
        let server = MockHttpServer::error().await;
        let resp = reqwest::get(server.base_url()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn mock_server_rate_limited() {
        let server = MockHttpServer::rate_limited().await;
        let resp = reqwest::get(server.base_url()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
