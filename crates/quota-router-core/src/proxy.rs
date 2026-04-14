//! Proxy server for forwarding LLM requests to providers.
//!
//! This module handles the actual LLM proxy functionality - forwarding
//! requests to providers like OpenAI, Anthropic, etc. It is entirely
//! separate from the admin API (admin.rs) which manages keys and teams.

use crate::balance::Balance;
use crate::providers::Provider;
use http::{Request, StatusCode};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Response;
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

pub struct ProxyServer {
    balance: Arc<Mutex<Balance>>,
    provider: Provider,
    port: u16,
}

impl ProxyServer {
    pub fn new(balance: Balance, provider: Provider, port: u16) -> Self {
        Self {
            balance: Arc::new(Mutex::new(balance)),
            provider,
            port,
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(addr).await?;

        info!("Proxy server listening on http://{}", addr);

        let balance = Arc::clone(&self.balance);
        let provider = self.provider.clone();

        tokio::spawn(async move {
            let balance = Arc::clone(&balance);
            let provider = provider.clone();

            while let Ok((stream, _)) = listener.accept().await {
                let balance = Arc::clone(&balance);
                let provider = provider.clone();

                tokio::spawn(async move {
                    let io = TokioIo::new(stream);

                    if let Err(err) = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(move |req| {
                                let balance = Arc::clone(&balance);
                                let provider = provider.clone();
                                async move {
                                    Ok::<_, Infallible>(handle_request(req, &balance, &provider))
                                }
                            }),
                        )
                        .await
                    {
                        eprintln!("Error serving connection: {}", err);
                    }
                });
            }
        })
        .await?;

        Ok(())
    }
}

fn handle_request<B>(
    _req: Request<B>,
    balance: &Arc<Mutex<Balance>>,
    provider: &Provider,
) -> Response<String> {
    // Check balance for proxy requests
    {
        let bal = balance.lock();
        if bal.check(1).is_err() {
            return Response::builder()
                .status(StatusCode::PAYMENT_REQUIRED)
                .body("Insufficient OCTO-W balance".to_string())
                .unwrap();
        }
    }

    // Get API key from environment
    let api_key = match provider.get_api_key() {
        Some(key) => key,
        None => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("API key not set in environment".to_string())
                .unwrap();
        }
    };

    // Deduct balance
    {
        let mut bal = balance.lock();
        bal.deduct(1);
    }

    // Forward request to provider (simplified - just return success for MVE)
    info!(
        "Request forwarded with API key: {}...",
        &api_key[..8.min(api_key.len())]
    );

    Response::builder()
        .status(StatusCode::OK)
        .body("Request forwarded successfully".to_string())
        .unwrap()
}
