//! Proxy server for forwarding LLM requests to providers.
//!
//! This module handles the actual LLM proxy functionality - forwarding
//! requests to providers like OpenAI, Anthropic, etc. It is entirely
//! separate from the admin API (admin.rs) which manages keys and teams.
//!
//! ⚠️ CRITICAL INVARIANT (RFC-0917):
//! This HTTP proxy server EXISTS in ALL modes (litellm-mode, any-llm-mode, full).
//! Mode gate controls HOW providers are called (reqwest vs PyO3), NOT whether this proxy exists.
//! NEVER think "litellm-mode = proxy only" — both proxy AND Python SDK exist in all modes.
//!
//! **Provider Integration by Mode:**
//! - litellm-mode: native_http (reqwest → REST APIs) via HttpProviderFactory
//! - any-llm-mode: py_bridge (PyO3 → Python SDKs) via python_sdk_entry
//! - full: Either path is available

use crate::balance::Balance;
use crate::providers::Provider;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body::{Body as HttpBody, Frame};
use http_body_util::BodyExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Response;
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::net::TcpListener;
use tracing::info;

// =============================================================================
// Feature-gated provider types
// =============================================================================

#[cfg(any(feature = "litellm-mode", feature = "full"))]
use crate::native_http::{
    HttpCompletionRequest as NativeHttpRequest, HttpProviderFactory, StreamingChunk,
    StreamingResponse,
};

#[cfg(any(feature = "litellm-mode", feature = "full"))]
use crate::shared_types::Message as SharedMessage;

// =============================================================================
// SSE Body
// =============================================================================

/// SSE streaming body that yields chunks from a channel
struct SseBody {
    receiver: tokio::sync::mpsc::Receiver<Result<Bytes, std::convert::Infallible>>,
}

impl SseBody {
    #[cfg(any(feature = "litellm-mode", feature = "full"))]
    fn new(receiver: tokio::sync::mpsc::Receiver<Result<Bytes, std::convert::Infallible>>) -> Self {
        Self { receiver }
    }

    fn from_error(message: String) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let _ = tx.try_send(Ok(Bytes::from(format!("data: Error: {}\n\n", message))));
        Self { receiver: rx }
    }
}

impl HttpBody for SseBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.receiver).poll_recv(cx) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
            Poll::Ready(Some(Err(infallible))) => Poll::Ready(Some(Err(infallible))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

// =============================================================================
// Proxy Server
// =============================================================================

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

        // Initialize providers based on mode
        #[cfg(any(feature = "litellm-mode", feature = "full"))]
        crate::init_native_http_providers();

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
                                handle_request(req, balance, provider)
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

// =============================================================================
// Request Parsing
// =============================================================================

#[cfg(any(feature = "litellm-mode", feature = "full"))]
fn parse_request_body(body: &str) -> Option<NativeHttpRequest> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;

    let messages: Vec<SharedMessage> = json
        .get("messages")?
        .as_array()?
        .iter()
        .filter_map(|m| {
            let role = m.get("role")?.as_str()?.to_string();
            let content = m.get("content")?.as_str()?.to_string();
            Some(SharedMessage::new(role, content))
        })
        .collect();

    let model = json.get("model")?.as_str()?.to_string();
    let stream = json.get("stream").and_then(|v| v.as_bool());
    let temperature = json
        .get("temperature")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    let max_tokens = json
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let top_p = json.get("top_p").and_then(|v| v.as_f64()).map(|v| v as f32);
    let stop: Option<Vec<String>> = json.get("stop").and_then(|v| {
        v.as_array()?
            .iter()
            .map(|s| s.as_str().map(String::from))
            .collect()
    });
    let n = json.get("n").and_then(|v| v.as_u64()).map(|v| v as u32);
    let presence_penalty = json
        .get("presence_penalty")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    let frequency_penalty = json
        .get("frequency_penalty")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    let user = json
        .get("user")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    Some(NativeHttpRequest {
        model,
        messages,
        stream,
        temperature,
        max_tokens,
        top_p,
        stop,
        n,
        presence_penalty,
        frequency_penalty,
        user,
    })
}

#[cfg(not(any(feature = "litellm-mode", feature = "full")))]
#[allow(dead_code)]
fn parse_request_body(_body: &str) -> Option<()> {
    // In any-llm-mode without native_http, we don't parse with NativeHttpRequest
    // The actual parsing happens via py_bridge
    None
}

// =============================================================================
// Request Handling
// =============================================================================

async fn handle_request<B>(
    req: Request<B>,
    balance: Arc<Mutex<Balance>>,
    provider: Provider,
) -> Result<Response<SseBody>, Infallible>
where
    B: http_body::Body + 'static,
    B::Data: Send,
    B::Error: Send + std::fmt::Debug,
{
    // Check balance for proxy requests
    {
        let bal = balance.lock();
        if bal.check(1).is_err() {
            let resp = Response::builder()
                .status(StatusCode::PAYMENT_REQUIRED)
                .body(SseBody::from_error(
                    "Insufficient OCTO-W balance".to_string(),
                ))
                .unwrap();
            return Ok(resp);
        }
    }

    // Get API key from environment
    let api_key = match provider.get_api_key() {
        Some(key) => key,
        None => {
            let resp = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(SseBody::from_error(
                    "API key not set in environment".to_string(),
                ))
                .unwrap();
            return Ok(resp);
        }
    };

    // Deduct balance
    {
        let mut bal = balance.lock();
        bal.deduct(1);
    }

    // Parse request body
    let (_, body) = req.into_parts();
    let full_body = match body.collect().await {
        Ok(bytes) => bytes.to_bytes(),
        Err(_) => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error(
                    "Failed to read request body".to_string(),
                ))
                .unwrap();
            return Ok(resp);
        }
    };
    let body_str = String::from_utf8_lossy(&full_body);

    #[cfg(any(feature = "litellm-mode", feature = "full"))]
    {
        handle_request_litellm(&body_str, &provider, &api_key).await
    }

    #[cfg(not(any(feature = "litellm-mode", feature = "full")))]
    {
        handle_request_anyllm(&body_str, &provider, &api_key).await
    }
}

#[cfg(any(feature = "litellm-mode", feature = "full"))]
async fn handle_request_litellm(
    body_str: &str,
    provider: &Provider,
    api_key: &str,
) -> Result<Response<SseBody>, Infallible> {
    let request = match parse_request_body(body_str) {
        Some(req) => req,
        None => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error("Invalid request body".to_string()))
                .unwrap();
            return Ok(resp);
        }
    };

    // Check if streaming is requested
    if request.stream == Some(true) {
        return handle_streaming(provider, api_key, request).await;
    }

    // Non-streaming request - use HttpProviderFactory
    let provider_name = &provider.name;
    let http_provider = match HttpProviderFactory::create(provider_name) {
        Some(p) => p,
        None => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error(format!(
                    "Provider '{}' not found",
                    provider_name
                )))
                .unwrap();
            return Ok(resp);
        }
    };

    // Make completion request
    match http_provider.completion(&request, api_key).await {
        Ok(resp) => {
            let body = serde_json::json!({
                "id": resp.id,
                "object": resp.object,
                "created": resp.created,
                "model": resp.model,
                "choices": resp.choices.into_iter().map(|c| {
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
                    "prompt_tokens": resp.usage.prompt_tokens,
                    "completion_tokens": resp.usage.completion_tokens,
                    "total_tokens": resp.usage.total_tokens
                }
            });

            // Wrap JSON in SseBody for consistent Response type
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx.try_send(Ok(Bytes::from(body.to_string())));
            let sse_body = SseBody { receiver: rx };

            let resp = Response::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(sse_body)
                .unwrap();
            Ok(resp)
        }
        Err(e) => {
            let resp = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(SseBody::from_error(format!("Provider error: {}", e)))
                .unwrap();
            Ok(resp)
        }
    }
}

#[cfg(not(any(feature = "litellm-mode", feature = "full")))]
async fn handle_request_anyllm(
    _body_str: &str,
    _provider: &Provider,
    _api_key: &str,
) -> Result<Response<SseBody>, Infallible> {
    // For any-llm-mode, delegate to py_bridge via python_sdk_entry
    // This is a placeholder - actual implementation would call python_sdk_entry
    let resp = Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(SseBody::from_error(
            "any-llm-mode proxy not yet implemented".to_string(),
        ))
        .unwrap();
    Ok(resp)
}

// =============================================================================
// Streaming (liteLLM mode only)
// =============================================================================

#[cfg(any(feature = "litellm-mode", feature = "full"))]
async fn handle_streaming(
    provider: &Provider,
    api_key: &str,
    request: NativeHttpRequest,
) -> Result<Response<SseBody>, Infallible> {
    let provider_name = &provider.name;

    let http_provider = match HttpProviderFactory::create(provider_name) {
        Some(p) => p,
        None => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error(format!(
                    "Provider '{}' not found",
                    provider_name
                )))
                .unwrap();
            return Ok(resp);
        }
    };

    // Check if provider supports streaming
    if !http_provider.supports_streaming() {
        let resp = Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(SseBody::from_error(format!(
                "Provider '{}' does not support streaming",
                provider_name
            )))
            .unwrap();
        return Ok(resp);
    }

    // Call streaming completion
    let streaming_resp = http_provider.streaming_completion(&request, api_key).await;

    match streaming_resp {
        Ok(StreamingResponse {
            mut receiver,
            content_type,
        }) => {
            // Create channel for the SSE body
            let (tx, rx) =
                tokio::sync::mpsc::channel::<Result<Bytes, std::convert::Infallible>>(100);

            // Spawn task to forward chunks from provider to channel
            // Note: task silently drops on panic - proper error tracking requires JoinHandle
            let _handle = tokio::spawn(async move {
                while let Some(chunk_result) = receiver.recv().await {
                    match chunk_result {
                        Ok(StreamingChunk::RawSSE(bytes)) => {
                            if tx.send(Ok(Bytes::from(bytes))).await.is_err() {
                                // Receiver dropped - normal close
                                break;
                            }
                        }
                        Ok(StreamingChunk::Structured(_)) => {
                            // Structured chunks would need conversion - for now skip
                        }
                        Err(e) => {
                            let error_data = format!("data: Error: {}\n\n", e);
                            if tx.send(Ok(Bytes::from(error_data))).await.is_err() {
                                break;
                            }
                            break;
                        }
                    }
                }
                // Send [DONE] marker
                let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n"))).await;
            });

            let body = SseBody::new(rx);

            let resp = Response::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, content_type)
                .header(http::header::TRANSFER_ENCODING, "chunked")
                .body(body)
                .unwrap();

            Ok(resp)
        }
        Err(e) => {
            let resp = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(SseBody::from_error(format!("Streaming error: {}", e)))
                .unwrap();
            Ok(resp)
        }
    }
}
