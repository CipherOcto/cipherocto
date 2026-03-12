// Proxy server for quota-router-core
// Provides OpenAI-compatible endpoints

use crate::balance::Balance;
use crate::completion::{self, Message};
use crate::providers::Provider;
use http_body_util::BodyExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
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
        info!("OpenAI-compatible endpoints:");
        info!("  POST /v1/chat/completions");
        info!("  POST /v1/embeddings");
        info!("  GET  /v1/models");

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

                    if let Err(err) =
                        http1::Builder::new()
                            .serve_connection(
                                io,
                                service_fn(move |req| {
                                    let balance = Arc::clone(&balance);
                                    let provider = provider.clone();
                                    async move {
                                        Ok::<_, Infallible>(
                                            handle_request(&balance, &provider, req).await,
                                        )
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

async fn handle_request<B>(
    balance: &Arc<Mutex<Balance>>,
    provider: &Provider,
    req: Request<B>,
) -> Response<String>
where
    B: BodyExt + Send,
    B::Data: Send,
{
    let path = req.uri().path();
    let method = req.method().clone();

    info!("{} {}", method, path);

    // Route to appropriate handler
    match (method, path) {
        // OpenAI-compatible endpoints
        (Method::POST, "/v1/chat/completions") => {
            handle_chat_completions(balance, provider, req).await
        }
        (Method::POST, "/v1/embeddings") => handle_embeddings(balance, provider).await,
        (Method::GET, "/v1/models") => handle_models(balance, provider).await,

        // Health check
        (Method::GET, "/health") | (Method::GET, "/") => Response::builder()
            .status(StatusCode::OK)
            .body(r#"{"status":"ok"}"#.to_string())
            .unwrap(),

        // Default: 404
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(format!("Not found: {}", path))
            .unwrap(),
    }
}

async fn handle_chat_completions<B>(
    balance: &Arc<Mutex<Balance>>,
    provider: &Provider,
    req: Request<B>,
) -> Response<String>
where
    B: BodyExt + Send,
    B::Data: Send,
{
    // Check balance
    {
        let bal = balance.lock();
        if bal.check(1).is_err() {
            return Response::builder()
                .status(StatusCode::PAYMENT_REQUIRED)
                .body(
                    serde_json::json!({
                        "error": {
                            "message": "Insufficient OCTO-W balance",
                            "type": "insufficient_quota",
                            "code": "insufficient_quota"
                        }
                    })
                    .to_string(),
                )
                .unwrap();
        }
    }

    // Check API key
    if provider.get_api_key().is_none() {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(
                serde_json::json!({
                    "error": {
                        "message": "API key not set",
                        "type": "authentication_error",
                        "code": "invalid_api_key"
                    }
                })
                .to_string(),
            )
            .unwrap();
    }

    // Parse request body
    let body = match req.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Invalid request body".to_string())
                .unwrap();
        }
    };

    let json: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Invalid JSON".to_string())
                .unwrap();
        }
    };

    // Extract model and messages
    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4")
        .to_string();

    let messages: Vec<Message> = json
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let role = m.get("role")?.as_str()?.to_string();
                    let content = m.get("content")?.as_str()?.to_string();
                    Some(Message::new(role, content))
                })
                .collect()
        })
        .unwrap_or_default();

    // Deduct balance
    {
        let mut bal = balance.lock();
        bal.deduct(1);
    }

    // Generate completion (mock for MVE)
    let result = completion::completion(model, messages).unwrap();

    // Convert to OpenAI format
    let response = serde_json::json!({
        "id": result.id,
        "object": "chat.completion",
        "created": result.created,
        "model": result.model,
        "choices": result.choices.iter().map(|c| {
            serde_json::json!({
                "index": c.index,
                "message": {
                    "role": c.message.role,
                    "content": c.message.content
                },
                "finish_reason": c.finish_reason
            })
        }).collect::<Vec<_>>(),
        "usage": {
            "prompt_tokens": result.usage.prompt_tokens,
            "completion_tokens": result.usage.completion_tokens,
            "total_tokens": result.usage.total_tokens
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .body(response.to_string())
        .unwrap()
}

async fn handle_embeddings(
    balance: &Arc<Mutex<Balance>>,
    provider: &Provider,
) -> Response<String> {
    // Check balance
    {
        let bal = balance.lock();
        if bal.check(1).is_err() {
            return Response::builder()
                .status(StatusCode::PAYMENT_REQUIRED)
                .body(
                    serde_json::json!({
                        "error": {
                            "message": "Insufficient OCTO-W balance",
                            "type": "insufficient_quota",
                            "code": "insufficient_quota"
                        }
                    })
                    .to_string(),
                )
                .unwrap();
        }
    }

    // Check API key
    if provider.get_api_key().is_none() {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(
                serde_json::json!({
                    "error": {
                        "message": "API key not set",
                        "type": "authentication_error",
                        "code": "invalid_api_key"
                    }
                })
                .to_string(),
            )
            .unwrap();
    }

    // Return mock embedding response
    let response = serde_json::json!({
        "object": "list",
        "data": [
            {
                "object": "embedding",
                "embedding": vec![0.1f32; 384],
                "index": 0
            }
        ],
        "model": "text-embedding-3-small",
        "usage": {
            "prompt_tokens": 8,
            "total_tokens": 8
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .body(response.to_string())
        .unwrap()
}

async fn handle_models(_balance: &Arc<Mutex<Balance>>, _provider: &Provider) -> Response<String> {
    let models = serde_json::json!({
        "object": "list",
        "data": [
            {
                "id": "gpt-4",
                "object": "model",
                "created": 1687882411,
                "owned_by": "openai"
            },
            {
                "id": "gpt-4-turbo",
                "object": "model",
                "created": 1704067200,
                "owned_by": "openai"
            },
            {
                "id": "gpt-3.5-turbo",
                "object": "model",
                "created": 1677649963,
                "owned_by": "openai"
            },
            {
                "id": "text-embedding-3-small",
                "object": "model",
                "created": 1709596800,
                "owned_by": "openai"
            }
        ]
    });

    Response::builder()
        .status(StatusCode::OK)
        .body(models.to_string())
        .unwrap()
}
