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
use crate::config::DispatchInfo;
use crate::key_rate_limiter::RateLimiterStore;
use crate::keys::compute_key_hash;
use crate::metrics::Metrics;
use crate::providers::Provider;
use crate::storage::KeyStorage;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body::{Body as HttpBody, Frame};
use http_body_util::BodyExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Response;
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tracing::info;

/// Extract client API key from request headers.
/// Priority: Authorization (Bearer) > X-API-Key > X-AnyLLM-Key
fn extract_client_key<B>(req: &Request<B>) -> Option<String> {
    // Authorization: Bearer <key>
    if let Some(auth) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if let Some(stripped) = auth_str.strip_prefix("Bearer ") {
                if !stripped.is_empty() {
                    return Some(stripped.to_string());
                }
            }
        }
    }
    // X-API-Key
    if let Some(key) = req.headers().get("x-api-key") {
        if let Ok(key_str) = key.to_str() {
            if !key_str.is_empty() {
                return Some(key_str.to_string());
            }
        }
    }
    // X-AnyLLM-Key (any-llm compatibility)
    if let Some(key) = req.headers().get("x-anyllm-key") {
        if let Ok(key_str) = key.to_str() {
            if !key_str.is_empty() {
                return Some(key_str.to_string());
            }
        }
    }
    None
}

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

    fn from_string(body: String) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let _ = tx.try_send(Ok(Bytes::from(body)));
        Self { receiver: rx }
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
    dispatch_map: Arc<HashMap<String, DispatchInfo>>,
    storage: Option<Arc<dyn KeyStorage>>,
    master_key: Option<String>,
    metrics: Option<Arc<Metrics>>,
    rate_limiter: Option<Arc<RateLimiterStore>>,
}

impl ProxyServer {
    pub fn new(
        balance: Balance,
        provider: Provider,
        port: u16,
        dispatch_map: HashMap<String, DispatchInfo>,
    ) -> Self {
        Self {
            balance: Arc::new(Mutex::new(balance)),
            provider,
            port,
            dispatch_map: Arc::new(dispatch_map),
            storage: None,
            master_key: None,
            metrics: None,
            rate_limiter: None,
        }
    }

    /// Set the key storage for gateway auth (RFC-0932)
    pub fn with_storage(mut self, storage: Arc<dyn KeyStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Set the master key for gateway auth bypass (RFC-0932)
    pub fn with_master_key(mut self, master_key: String) -> Self {
        self.master_key = Some(master_key);
        self
    }

    /// Set Prometheus metrics (RFC-0937)
    pub fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Set rate limiter for per-key RPM/TPM enforcement (RFC-0933)
    pub fn with_rate_limiter(mut self, rate_limiter: Arc<RateLimiterStore>) -> Self {
        self.rate_limiter = Some(rate_limiter);
        self
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(addr).await?;

        info!("Proxy server listening on http://{}", addr);

        let balance = Arc::clone(&self.balance);
        let provider = self.provider.clone();
        let dispatch_map = Arc::clone(&self.dispatch_map);
        let storage = self.storage.clone();
        let master_key = self.master_key.clone();
        let metrics = self.metrics.clone();
        let rate_limiter = self.rate_limiter.clone();

        // Initialize providers based on mode
        #[cfg(any(feature = "litellm-mode", feature = "full"))]
        crate::init_native_http_providers();

        tokio::spawn(async move {
            let balance = Arc::clone(&balance);
            let provider = provider.clone();
            let dispatch_map = Arc::clone(&dispatch_map);

            while let Ok((stream, _)) = listener.accept().await {
                let balance = Arc::clone(&balance);
                let provider = provider.clone();
                let dispatch_map = Arc::clone(&dispatch_map);
                let storage = storage.clone();
                let master_key = master_key.clone();
                let metrics = metrics.clone();
                let rate_limiter = rate_limiter.clone();

                tokio::spawn(async move {
                    let io = TokioIo::new(stream);

                    if let Err(err) = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(move |req| {
                                let balance = Arc::clone(&balance);
                                let provider = provider.clone();
                                let dispatch_map = Arc::clone(&dispatch_map);
                                handle_request(
                                    req,
                                    balance,
                                    provider,
                                    dispatch_map,
                                    storage.clone(),
                                    master_key.clone(),
                                    metrics.clone(),
                                    rate_limiter.clone(),
                                )
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
        api_base: json
            .get("api_base")
            .and_then(|v| v.as_str())
            .map(String::from),
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

/// Resolve API key with priority chain (RFC-0929 §5).
/// Priority: config_key (from DispatchInfo/litellm_params) → env var ({PROVIDER}_API_KEY)
fn resolve_api_key(provider: &Provider, config_key: Option<&str>) -> Option<String> {
    // Priority 1: Config key (from GatewayConfig deployment)
    if let Some(key) = config_key {
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    // Priority 2: Environment variable
    provider.get_api_key()
}

#[allow(clippy::too_many_arguments)]
async fn handle_request<B>(
    req: Request<B>,
    balance: Arc<Mutex<Balance>>,
    provider: Provider,
    dispatch_map: Arc<HashMap<String, DispatchInfo>>,
    storage: Option<Arc<dyn KeyStorage>>,
    master_key: Option<String>,
    metrics: Option<Arc<Metrics>>,
    rate_limiter: Option<Arc<RateLimiterStore>>,
) -> Result<Response<SseBody>, Infallible>
where
    B: http_body::Body + 'static,
    B::Data: Send,
    B::Error: Send + std::fmt::Debug,
{
    let start = std::time::Instant::now();

    // /metrics endpoint (RFC-0937) — bypass auth and proxy
    if req.uri().path() == "/metrics" {
        if let Some(ref m) = metrics {
            let body = m.encode();
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
                .body(SseBody::from_string(body))
                .unwrap();
            return Ok(resp);
        }
    }

    // Record request
    if let Some(ref m) = metrics {
        m.requests_total.inc();
    }

    // Gateway auth (RFC-0932) and rate limiting (RFC-0933)
    // Holds the validated ApiKey for rate limiting and rate limit header injection.
    let mut validated_api_key: Option<crate::keys::ApiKey> = None;

    if let Some(ref storage) = storage {
        // Extract client key from request headers
        // Priority: Authorization > X-API-Key > X-AnyLLM-Key
        let client_key = extract_client_key(&req);

        let client_key = match client_key {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("Missing API key".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        // Check master key bypass (constant-time comparison)
        let is_master = master_key
            .as_ref()
            .map(|mk| bool::from(ConstantTimeEq::ct_eq(client_key.as_bytes(), mk.as_bytes())))
            .unwrap_or(false);

        if !is_master {
            // Validate client key against storage
            let key_hash = compute_key_hash(&client_key);
            match storage.lookup_by_hash(&key_hash) {
                Ok(Some(api_key)) => {
                    // Key is valid — check RPM rate limit (RFC-0933)
                    if let (Some(ref limiter), Some(rpm_limit)) = (&rate_limiter, api_key.rpm_limit)
                    {
                        if rpm_limit > 0 {
                            match limiter.check_rpm_only(&api_key.key_id, rpm_limit as u32) {
                                Ok(_status) => {
                                    // RPM check passed — status available for headers
                                    validated_api_key = Some(api_key);
                                }
                                Err(crate::keys::KeyError::RateLimited { retry_after }) => {
                                    let body = serde_json::json!({
                                        "error": {
                                            "message": "Rate limit exceeded",
                                            "type": "rate_limit_error",
                                            "code": "rpm_limit_exceeded",
                                            "retry_after": retry_after
                                        }
                                    });
                                    let resp = Response::builder()
                                        .status(StatusCode::TOO_MANY_REQUESTS)
                                        .header("content-type", "application/json")
                                        .header("retry-after", retry_after.to_string())
                                        .body(SseBody::from_string(body.to_string()))
                                        .unwrap();
                                    return Ok(resp);
                                }
                                Err(e) => {
                                    let resp = Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .body(SseBody::from_error(format!(
                                            "Rate limit error: {}",
                                            e
                                        )))
                                        .unwrap();
                                    return Ok(resp);
                                }
                            }
                        } else {
                            // RPM limit is 0 (unlimited) — no rate limiting
                            validated_api_key = Some(api_key);
                        }
                    } else {
                        // No rate limiter or no RPM limit configured
                        validated_api_key = Some(api_key);
                    }
                }
                Ok(None) => {
                    let resp = Response::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .body(SseBody::from_error("API key not found".to_string()))
                        .unwrap();
                    return Ok(resp);
                }
                Err(e) => {
                    let resp = Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(SseBody::from_error(format!("Key validation error: {}", e)))
                        .unwrap();
                    return Ok(resp);
                }
            }
        }
    }

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

    // Parse request body first to extract model name for DispatchInfo lookup
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

    // Extract model name from JSON body for DispatchInfo lookup
    let request_model = serde_json::from_str::<serde_json::Value>(&body_str)
        .ok()
        .and_then(|v| v.get("model")?.as_str().map(String::from));

    // Look up DispatchInfo by model name or model_group
    let dispatch = request_model.as_ref().and_then(|model| {
        dispatch_map.values().find(|d| {
            d.model == *model
                || d.model_group.as_deref() == Some(model.as_str())
                || d.deployment_id == *model
        })
    });

    // Resolve API key with priority chain (RFC-0929 §5):
    // 1. Per-request key from DispatchInfo.api_key (config-time resolved)
    // 2. Environment variable ({PROVIDER_NAME}_API_KEY)
    let config_key = dispatch.and_then(|d| d.api_key.as_deref());
    let api_key = match resolve_api_key(&provider, config_key) {
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

    // Extract DispatchInfo fields for mode handlers
    let dispatch_api_base = dispatch.and_then(|d| d.api_base.clone());
    let _dispatch_max_retries = dispatch.and_then(|d| d.max_retries);

    let mut result = {
        #[cfg(any(feature = "litellm-mode", feature = "full"))]
        {
            handle_request_litellm(&body_str, &provider, &api_key, dispatch_api_base.as_deref())
                .await
        }

        #[cfg(not(any(feature = "litellm-mode", feature = "full")))]
        {
            handle_request_anyllm(&body_str, &provider, &api_key).await
        }
    };

    // Record request duration
    if let Some(ref m) = metrics {
        m.request_duration.observe(start.elapsed().as_secs_f64());
    }

    // Inject rate limit headers into response (RFC-0933 §Rate Limit Headers).
    // Uses the ApiKey validated during auth to report RPM limits.
    if let (Ok(ref mut resp), Some(ref api_key)) = (&mut result, &validated_api_key) {
        if let Some(rpm_limit) = api_key.rpm_limit {
            let headers = resp.headers_mut();
            headers.insert("x-ratelimit-limit", rpm_limit.to_string().parse().unwrap());
            // Remaining is approximated as the limit minus 1 (current request consumed 1).
            // For exact tracking, a separate counter or status from check_rpm_only would be needed.
            let remaining = (rpm_limit as u64).saturating_sub(1);
            headers.insert(
                "x-ratelimit-remaining",
                remaining.to_string().parse().unwrap(),
            );
            // Reset: 60 seconds from now (token bucket window)
            let reset = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60;
            headers.insert("x-ratelimit-reset", reset.to_string().parse().unwrap());
        }
    }

    result
}

#[cfg(any(feature = "litellm-mode", feature = "full"))]
async fn handle_request_litellm(
    body_str: &str,
    provider: &Provider,
    api_key: &str,
    dispatch_api_base: Option<&str>,
) -> Result<Response<SseBody>, Infallible> {
    let mut request = match parse_request_body(body_str) {
        Some(req) => req,
        None => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error("Invalid request body".to_string()))
                .unwrap();
            return Ok(resp);
        }
    };

    // Wire api_base from DispatchInfo if not set in request body
    if request.api_base.is_none() {
        if let Some(base) = dispatch_api_base {
            request.api_base = Some(base.to_string());
        }
    }

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

#[cfg(test)]
#[cfg(any(feature = "litellm-mode", feature = "full"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request_body_extracts_api_base() {
        let body = r#"{
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}],
            "api_base": "https://custom.azure.com/"
        }"#;
        let req = parse_request_body(body).unwrap();
        assert_eq!(req.api_base, Some("https://custom.azure.com/".to_string()));
    }

    #[test]
    fn test_parse_request_body_no_api_base() {
        let body = r#"{
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}]
        }"#;
        let req = parse_request_body(body).unwrap();
        assert_eq!(req.api_base, None);
    }

    #[test]
    fn test_resolve_api_key_config_priority() {
        std::env::set_var("TESTPROV_API_KEY", "env-key");
        let provider = Provider::new("testprov", "https://example.com");
        // Config key takes priority over env var
        assert_eq!(
            resolve_api_key(&provider, Some("config-key")),
            Some("config-key".to_string())
        );
        std::env::remove_var("TESTPROV_API_KEY");
    }

    #[test]
    fn test_resolve_api_key_env_fallback() {
        std::env::set_var("TESTPROV2_API_KEY", "env-key");
        let provider = Provider::new("testprov2", "https://example.com");
        // No config key → falls back to env var
        assert_eq!(
            resolve_api_key(&provider, None),
            Some("env-key".to_string())
        );
        std::env::remove_var("TESTPROV2_API_KEY");
    }

    #[test]
    fn test_resolve_api_key_none_when_missing() {
        std::env::remove_var("TESTPROV3_API_KEY");
        let provider = Provider::new("testprov3", "https://example.com");
        assert_eq!(resolve_api_key(&provider, None), None);
    }
}
