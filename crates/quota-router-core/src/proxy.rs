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
use crate::cache::ResponseCache;
use crate::config::DispatchInfo;
use crate::fallback::FallbackExecutor;
use crate::key_rate_limiter::RateLimiterStore;
use crate::keys::compute_key_hash;
use crate::metrics::Metrics;
use crate::providers::Provider;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use crate::py_bridge;
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

/// Extract model name from dispatch map based on path.
#[allow(dead_code)]
fn extract_model_from_path(
    _path: &str,
    dispatch_map: &HashMap<String, DispatchInfo>,
) -> Option<String> {
    dispatch_map.values().next().map(|d| d.model.clone())
}

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
    fallback: Option<Arc<FallbackExecutor>>,
    response_cache: Option<Arc<ResponseCache>>,
    callback_executor: Option<Arc<crate::callbacks::CallbackExecutor>>,
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
            fallback: None,
            response_cache: None,
            callback_executor: None,
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

    /// Set fallback executor for provider failure recovery (RFC-0902)
    pub fn with_fallback(mut self, fallback: FallbackExecutor) -> Self {
        self.fallback = Some(Arc::new(fallback));
        self
    }

    /// Set response cache for caching provider responses (RFC-0906)
    pub fn with_response_cache(mut self, cache: ResponseCache) -> Self {
        self.response_cache = Some(Arc::new(cache));
        self
    }

    /// Set callback executor for async callback delivery (RFC-0947)
    pub fn with_callback_executor(mut self, executor: crate::callbacks::CallbackExecutor) -> Self {
        self.callback_executor = Some(Arc::new(executor));
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
        let fallback = self.fallback.clone();
        let response_cache = self.response_cache.clone();
        let callback_executor = self.callback_executor.clone();

        // Initialize providers based on mode
        #[cfg(any(feature = "litellm-mode", feature = "full"))]
        crate::init_native_http_providers();
        #[cfg(any(feature = "any-llm-mode", feature = "full"))]
        crate::init_py_bridge_providers();

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
                let fallback = fallback.clone();
                let response_cache = response_cache.clone();
                let callback_executor = callback_executor.clone();

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
                                    fallback.clone(),
                                    response_cache.clone(),
                                    callback_executor.clone(),
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

    // Parse messages — content can be null for tool_calls messages (RFC-0939)
    let messages: Vec<SharedMessage> = json
        .get("messages")?
        .as_array()?
        .iter()
        .filter_map(|m| {
            let role = m.get("role")?.as_str()?.to_string();
            let content = m.get("content").and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(String::from)
                }
            });
            // Parse tool_calls if present
            let tool_calls = m.get("tool_calls").and_then(|v| {
                serde_json::from_value::<Vec<crate::shared_types::ToolCall>>(v.clone()).ok()
            });
            let tool_call_id = m
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let function_call = m.get("function_call").and_then(|v| {
                serde_json::from_value::<crate::shared_types::FunctionCall>(v.clone()).ok()
            });

            Some(SharedMessage {
                role,
                content,
                name: m.get("name").and_then(|v| v.as_str()).map(String::from),
                tool_calls,
                tool_call_id,
                function_call,
            })
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

    // Function calling fields (RFC-0939)
    let tools = json
        .get("tools")
        .and_then(|v| serde_json::from_value::<Vec<crate::shared_types::Tool>>(v.clone()).ok());
    let tool_choice = json
        .get("tool_choice")
        .and_then(|v| serde_json::from_value::<crate::shared_types::ToolChoice>(v.clone()).ok());
    let response_format = json.get("response_format").and_then(|v| {
        serde_json::from_value::<crate::shared_types::ResponseFormat>(v.clone()).ok()
    });
    let seed = json.get("seed").and_then(|v| v.as_i64());
    let logprobs = json.get("logprobs").and_then(|v| v.as_bool());
    let top_logprobs = json
        .get("top_logprobs")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let parallel_tool_calls = json.get("parallel_tool_calls").and_then(|v| v.as_bool());

    // Prompt management fields (RFC-0948)
    let prompt_id = json
        .get("prompt_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let prompt_variables = json.get("prompt_variables").and_then(|v| {
        serde_json::from_value::<std::collections::HashMap<String, String>>(v.clone()).ok()
    });

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
        tools,
        tool_choice,
        response_format,
        seed,
        logprobs,
        top_logprobs,
        parallel_tool_calls,
        prompt_id,
        prompt_variables,
    })
}

#[cfg(not(any(feature = "litellm-mode", feature = "full")))]
#[allow(dead_code)]
fn parse_request_body(body: &str) -> Option<()> {
    // In any-llm-mode, we parse minimally — just validate JSON is valid
    let _: serde_json::Value = serde_json::from_str(body).ok()?;
    Some(())
}

// =============================================================================
// Request Handling
// =============================================================================

/// Resolve API key with priority chain (RFC-0929 §5).
/// Priority: config_key (from DispatchInfo/litellm_params) → env var ({PROVIDER}_API_KEY)
/// Resolve API key with 3-tier precedence (RFC-0938):
/// 1. Config key (from GatewayConfig deployment) — highest priority
/// 2. ANY_LLM_KEY universal env var
/// 3. {PROVIDER}_API_KEY env var — lowest priority
fn resolve_api_key(provider: &Provider, config_key: Option<&str>) -> Option<String> {
    // Priority 1: Config key (from GatewayConfig deployment)
    if let Some(key) = config_key {
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }

    // Priority 2: ANY_LLM_KEY universal env var
    if let Ok(key) = std::env::var("ANY_LLM_KEY") {
        if !key.is_empty() {
            tracing::warn!(
                "Using ANY_LLM_KEY for provider '{}' — consider setting provider-specific key",
                provider.name
            );
            return Some(key);
        }
    }

    // Priority 3: Provider-specific env var
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
    fallback: Option<Arc<FallbackExecutor>>,
    response_cache: Option<Arc<ResponseCache>>,
    callback_executor: Option<Arc<crate::callbacks::CallbackExecutor>>,
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

        // Per-team budget check (RFC-0943)
        // TODO: Requires get_budget on KeyStorage trait
        // For now, team budget is checked at the storage level during spend recording
    }

    // Fire Start callback after key validation and rate limit checks (RFC-0947)
    if let Some(ref executor) = callback_executor {
        let event = crate::callbacks::CallbackEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            callback_type: crate::callbacks::CallbackType::Start,
            timestamp: chrono::Utc::now(),
            request: crate::callbacks::CallbackRequest {
                model: String::new(),
                messages: vec![],
                temperature: None,
                max_tokens: None,
                stream: false,
                provider: provider.name.clone(),
                key_id: validated_api_key.as_ref().map(|k| k.key_id.clone()),
                team_id: validated_api_key
                    .as_ref()
                    .and_then(|k| k.team_id.map(|id| id.to_string())),
                user_id: None,
            },
            response: None,
            error: None,
            key_metadata: validated_api_key
                .as_ref()
                .map(|k| crate::callbacks::KeyMetadata {
                    key_id: k.key_id.clone(),
                    key_prefix: k.key_prefix.clone(),
                    team_id: k.team_id.map(|id| id.to_string()),
                    user_id: None,
                    spend_usd: 0.0,
                    max_budget_usd: Some(k.budget_limit as f64 / 100.0),
                }),
            timing: crate::callbacks::CallbackTiming {
                request_start: chrono::Utc::now(),
                request_end: None,
                total_ms: 0,
                provider_latency_ms: 0,
                queue_time_ms: 0,
            },
        };
        let _ = executor.fire(event).await;
    }

    // Path-based routing (RFC-0917)
    let path = req.uri().path().to_string();

    // /v1/models endpoints — no body parsing needed
    if path == "/v1/models" || path.starts_with("/v1/models/") {
        let model_id = path.strip_prefix("/v1/models/").unwrap_or("");
        let resp = handle_models_endpoint(&dispatch_map, model_id);
        if let Some(ref m) = metrics {
            m.request_duration.observe(start.elapsed().as_secs_f64());
        }
        return Ok(resp);
    }

    // /v1/embeddings — needs body parsing but different handler
    if path == "/v1/embeddings" {
        // Check balance
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

        let request_model = serde_json::from_str::<serde_json::Value>(&body_str)
            .ok()
            .and_then(|v| v.get("model")?.as_str().map(String::from));

        let dispatch = request_model.as_ref().and_then(|model| {
            dispatch_map.values().find(|d| {
                d.model == *model
                    || d.model_group.as_deref() == Some(model.as_str())
                    || d.deployment_id == *model
            })
        });

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

        let dispatch_api_base = dispatch.and_then(|d| d.api_base.clone());

        let result =
            handle_embedding_request(&body_str, &provider, &api_key, dispatch_api_base.as_deref())
                .await;

        if let Some(ref m) = metrics {
            m.request_duration.observe(start.elapsed().as_secs_f64());
        }

        return result;
    }

    // /v1/completions — legacy text completions (RFC-0942)
    if path == "/v1/completions" {
        // Check balance
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

        let result = handle_completions_endpoint(&body_str, &provider, &dispatch_map).await;

        if let Some(ref m) = metrics {
            m.request_duration.observe(start.elapsed().as_secs_f64());
        }

        return result;
    }

    // /health and /ready — simple health checks
    if path == "/health" || path == "/ready" {
        let resp = Response::builder()
            .status(StatusCode::OK)
            .body(SseBody::from_string(r#"{"status":"ok"}"#.to_string()))
            .unwrap();
        return Ok(resp);
    }

    // /v1/moderations — content moderation (RFC-0942)
    if path == "/v1/moderations" {
        // Forward to OpenAI moderations API
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let base_url = dispatch_map
            .values()
            .find(|d| d.provider == "openai")
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/moderations", base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(full_body.to_vec())
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Moderation error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/messages — Anthropic Messages API (RFC-0942)
    if path == "/v1/messages" {
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        // Forward to Anthropic Messages API
        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let client = reqwest::Client::new();
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .body(full_body.to_vec())
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Messages error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/images/generations — image generation (RFC-0942)
    if path == "/v1/images/generations" {
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let base_url = dispatch_map
            .values()
            .find(|d| d.provider == "openai")
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/images/generations", base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(full_body.to_vec())
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Image error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/audio/* — audio endpoints (RFC-0942)
    if path.starts_with("/v1/audio/") {
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let base_url = dispatch_map
            .values()
            .find(|d| d.provider == "openai")
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let target_url = format!("{}{}", base_url, path);

        let client = reqwest::Client::new();
        let resp = client
            .post(&target_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .body(full_body.to_vec())
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Audio error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/responses — OpenAI Responses API (RFC-0942)
    if path == "/v1/responses" {
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let base_url = dispatch_map
            .values()
            .find(|d| d.provider == "openai")
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/responses", base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(full_body.to_vec())
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Responses error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/files — file management (RFC-0951)
    if path.starts_with("/v1/files") {
        let method = req.method().clone();
        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let file_id = path
            .strip_prefix("/v1/files/")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let base_url = dispatch_map
            .values()
            .find(|d| d.provider == "openai")
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let client = reqwest::Client::new();
        let upstream_path: String = match (&method, &file_id) {
            (&http::Method::GET, &None) => "/v1/files".into(),
            (&http::Method::GET, Some(id)) => format!("/v1/files/{}", id),
            (&http::Method::DELETE, Some(id)) => format!("/v1/files/{}", id),
            (&http::Method::POST, _) => "/v1/files".into(),
            _ => {
                let resp = Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body(SseBody::from_error("Method not allowed".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };
        let url = format!("{}{}", base_url, upstream_path);
        let mut req_builder = match method {
            http::Method::GET => client.get(&url),
            http::Method::DELETE => client.delete(&url),
            http::Method::POST => {
                if content_type.contains("multipart/form-data") {
                    client
                        .post(&url)
                        .header("Content-Type", &content_type)
                        .body(full_body.to_vec())
                } else {
                    client
                        .post(&url)
                        .header("Content-Type", "application/json")
                        .body(full_body.to_vec())
                }
            }
            _ => unreachable!(),
        };
        req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        let resp = req_builder.send().await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Files error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/batches — batch processing (RFC-0951)
    if path.starts_with("/v1/batches") {
        let method = req.method().clone();
        let path_after_prefix = path.strip_prefix("/v1/batches/").unwrap_or("");
        let is_cancel = path_after_prefix.ends_with("/cancel");
        let batch_id = if path_after_prefix.is_empty() {
            None
        } else if is_cancel {
            // Extract batch_id from "/v1/batches/{id}/cancel"
            path_after_prefix
                .strip_suffix("/cancel")
                .map(|s| s.to_string())
        } else {
            Some(path_after_prefix.to_string())
        };
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let base_url = dispatch_map
            .values()
            .find(|d| d.provider == "openai")
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let client = reqwest::Client::new();
        let upstream_path: String = match (&method, &batch_id, is_cancel) {
            (&http::Method::POST, &None, false) => "/v1/batches".into(),
            (&http::Method::GET, &None, false) => "/v1/batches".into(),
            (&http::Method::GET, Some(id), false) => format!("/v1/batches/{}", id),
            (&http::Method::POST, Some(id), true) => format!("/v1/batches/{}/cancel", id),
            _ => {
                let resp = Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body(SseBody::from_error("Method not allowed".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };
        let url = format!("{}{}", base_url, upstream_path);
        let mut req_builder = match method {
            http::Method::GET => client.get(&url),
            http::Method::POST => client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(full_body.to_vec()),
            _ => unreachable!(),
        };
        req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        let resp = req_builder.send().await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Batches error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/rerank — reranking API (RFC-0951)
    if path == "/v1/rerank" {
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let api_key = match resolve_api_key(&provider, None) {
            Some(key) => key,
            None => {
                let resp = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(SseBody::from_error("API key not set".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        // Rerank uses Cohere or Jina
        let base_url = dispatch_map
            .values()
            .find(|d| d.provider == "cohere" || d.provider == "jina")
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| "https://api.cohere.ai/v1".to_string());

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/rerank", base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(full_body.to_vec())
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body_bytes = r.bytes().await.unwrap_or_default();
                let resp = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(
                        String::from_utf8_lossy(&body_bytes).to_string(),
                    ))
                    .unwrap();
                return Ok(resp);
            }
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Rerank error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // /v1/realtime — WebSocket realtime API (RFC-0951)
    // Note: WebSocket requires special handling not available in this HTTP handler
    // This is a placeholder - actual implementation needs WebSocket support
    if path.starts_with("/v1/realtime") {
        let resp = Response::builder()
            .status(StatusCode::NOT_IMPLEMENTED)
            .body(SseBody::from_error(
                "WebSocket realtime API not yet implemented".to_string(),
            ))
            .unwrap();
        return Ok(resp);
    }

    // /{provider}/... — passthrough endpoints (RFC-0942)
    // Known provider prefixes for passthrough routing
    let known_providers = [
        "openai",
        "anthropic",
        "mistral",
        "gemini",
        "azure",
        "bedrock",
        "ollama",
        "groq",
        "together",
        "replicate",
        "databricks",
        "perplexity",
    ];
    let path_parts: Vec<&str> = path.trim_start_matches('/').splitn(2, '/').collect();
    if path_parts.len() == 2 && known_providers.contains(&path_parts[0]) {
        let provider_name = path_parts[0];
        let rest_path = path_parts[1];

        // Look up provider's API base from dispatch map
        let api_base = dispatch_map
            .values()
            .find(|d| d.provider == provider_name)
            .and_then(|d| d.api_base.clone())
            .unwrap_or_else(|| {
                // Default API bases
                match provider_name {
                    "openai" => "https://api.openai.com/v1".to_string(),
                    "anthropic" => "https://api.anthropic.com".to_string(),
                    "mistral" => "https://api.mistral.ai/v1".to_string(),
                    "groq" => "https://api.groq.com/openai/v1".to_string(),
                    "together" => "https://api.together.xyz/v1".to_string(),
                    _ => format!("https://api.{}.com/v1", provider_name),
                }
            });

        let target_url = format!("{}/{}", api_base, rest_path);

        // Forward request to provider
        let client = reqwest::Client::new();
        let (_, body) = req.into_parts();
        let full_body = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(SseBody::from_error("Failed to read body".to_string()))
                    .unwrap();
                return Ok(resp);
            }
        };

        let mut req_builder = client
            .post(&target_url)
            .header("Content-Type", "application/json");

        // Forward Authorization header if present
        // Note: We can't access original headers after into_parts(), so use provider API key
        let passthrough_key = dispatch_map
            .values()
            .find(|d| d.provider == provider_name)
            .and_then(|d| d.api_key.clone())
            .or_else(|| std::env::var(format!("{}_API_KEY", provider_name.to_uppercase())).ok());

        if let Some(key) = passthrough_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let resp = match req_builder.body(full_body.to_vec()).send().await {
            Ok(resp) => resp,
            Err(e) => {
                let resp = Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(SseBody::from_error(format!("Passthrough error: {}", e)))
                    .unwrap();
                return Ok(resp);
            }
        };

        let status =
            StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let body_bytes = resp.bytes().await.unwrap_or_default();

        let resp = Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(SseBody::from_string(
                String::from_utf8_lossy(&body_bytes).to_string(),
            ))
            .unwrap();
        return Ok(resp);
    }

    // Check balance for proxy requests (chat completions)
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

    // Per-user rate limiting (RFC-0943)
    // Extract user from request body `user` field
    let request_user = serde_json::from_str::<serde_json::Value>(&body_str)
        .ok()
        .and_then(|v| v.get("user")?.as_str().map(String::from));

    if let (Some(ref rl), Some(ref user_id)) = (&rate_limiter, &request_user) {
        // Check per-user RPM limit (use a default of 1000 RPM for now)
        // In production, this would come from a user config or database
        let user_rpm_limit: u32 = 1000;
        match rl.check_rpm_only(user_id, user_rpm_limit) {
            Ok(_) => {}
            Err(_) => {
                let resp = Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .header("Retry-After", "60")
                    .header("X-RateLimit-Limit", user_rpm_limit.to_string())
                    .header("X-RateLimit-Remaining", "0")
                    .body(SseBody::from_error(format!(
                        "Rate limit exceeded for user '{}'",
                        user_id
                    )))
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    // Deduct balance
    {
        let mut bal = balance.lock();
        bal.deduct(1);
    }

    // Extract DispatchInfo fields for mode handlers
    let dispatch_api_base = dispatch.and_then(|d| d.api_base.clone());
    let _dispatch_max_retries = dispatch.and_then(|d| d.max_retries);

    // Extract model name for fallback lookup
    let request_model = serde_json::from_str::<serde_json::Value>(&body_str)
        .ok()
        .and_then(|v| v.get("model")?.as_str().map(String::from))
        .unwrap_or_default();

    // Check response cache (RFC-0906)
    // Note: skip_cache is determined from body_str since req is already consumed
    let skip_cache = serde_json::from_str::<serde_json::Value>(&body_str)
        .ok()
        .and_then(|v| {
            v.get("cache_control")?
                .as_str()
                .map(|s| s.contains("no-cache"))
        })
        .unwrap_or(false);

    if !skip_cache {
        if let Some(ref cache) = response_cache {
            // Parse request for cache key generation
            let cache_messages: Vec<crate::shared_types::Message> =
                serde_json::from_str::<serde_json::Value>(&body_str)
                    .ok()
                    .and_then(|v| serde_json::from_value(v.get("messages")?.clone()).ok())
                    .unwrap_or_default();

            let cache_key = ResponseCache::cache_key(
                &request_model,
                &cache_messages,
                None, // temperature
                None, // max_tokens
            );

            if let Some(cached) = cache.get(&cache_key) {
                // Cache hit — return cached response
                if let Some(ref m) = metrics {
                    m.cache_hits.inc();
                }
                let resp = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .header("x-cache", "HIT")
                    .body(SseBody::from_string(cached))
                    .unwrap();

                // Record request duration
                if let Some(ref m) = metrics {
                    m.request_duration.observe(start.elapsed().as_secs_f64());
                }

                return Ok(resp);
            }

            if let Some(ref m) = metrics {
                m.cache_misses.inc();
            }
        }
    }

    // Execute with fallback support (RFC-0902)
    let mut result = {
        #[cfg(any(feature = "litellm-mode", feature = "full"))]
        {
            let primary_result = handle_request_litellm(
                &body_str,
                &provider,
                &api_key,
                dispatch_api_base.as_deref(),
                None, // TODO: wire prompt_registry from ProxyServer
            )
            .await;

            // Check if fallback is needed
            if let Some(ref executor) = fallback {
                match &primary_result {
                    Ok(resp) if resp.status().is_server_error() => {
                        // Provider returned 5xx — try fallback
                        let error = classify_http_error(resp.status());
                        if let Some(fallback_models) =
                            executor.config().get_fallback_models(&request_model, error)
                        {
                            // Try fallback models
                            try_fallback_models(
                                &fallback_models,
                                &dispatch_map,
                                &provider,
                                &body_str,
                                executor.config().max_retries,
                                executor.config().retry_delay(0),
                            )
                            .await
                            .unwrap_or(primary_result)
                        } else {
                            primary_result
                        }
                    }
                    _ => primary_result,
                }
            } else {
                primary_result
            }
        }

        #[cfg(not(any(feature = "litellm-mode", feature = "full")))]
        {
            handle_request_anyllm(&body_str, &provider, &api_key, dispatch_api_base.as_deref())
                .await
        }
    };

    // Store successful response in cache (RFC-0906)
    if let (Ok(ref resp), Some(ref cache)) = (&result, &response_cache) {
        if resp.status().is_success() && !skip_cache {
            // Extract response body for caching
            // Note: This is a simplified approach. For production, we'd need to
            // clone the response body before consuming it.
            let cache_messages: Vec<crate::shared_types::Message> =
                serde_json::from_str::<serde_json::Value>(&body_str)
                    .ok()
                    .and_then(|v| serde_json::from_value(v.get("messages")?.clone()).ok())
                    .unwrap_or_default();

            let cache_key = ResponseCache::cache_key(&request_model, &cache_messages, None, None);

            // For now, we'll skip caching the actual response body
            // because the response body is consumed by the SSE stream.
            // A proper implementation would need to tee the response.
            // This is a known limitation documented in the mission.
            let _ = cache_key;
            let _ = cache;
        }
    }

    // Record request duration
    if let Some(ref m) = metrics {
        m.request_duration.observe(start.elapsed().as_secs_f64());
    }

    // Structured request logging (RFC-0944)
    let duration_ms = start.elapsed().as_millis();
    let status_code = match &result {
        Ok(resp) => resp.status().as_u16(),
        Err(_) => 500,
    };
    tracing::info!(
        method = "POST",
        path = %path,
        model = %request_model,
        provider = %provider.name,
        status = status_code,
        duration_ms = duration_ms,
        user = request_user.as_deref().unwrap_or(""),
        "request completed"
    );

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

/// Resolve prompt template and inject as system message (RFC-0948).
/// If prompt_id is set, looks up the prompt, renders template with variables,
/// and prepends a system message to the request.
#[cfg(any(feature = "litellm-mode", feature = "full"))]
fn resolve_prompt(
    request: &mut NativeHttpRequest,
    prompt_registry: Option<&mut crate::prompts::PromptRegistry>,
) -> Result<(), String> {
    let prompt_id = match &request.prompt_id {
        Some(id) => id.clone(),
        None => return Ok(()), // No prompt to resolve
    };

    let registry = match prompt_registry {
        Some(r) => r,
        None => return Err("Prompt registry not available".to_string()),
    };

    // Generate request_id for A/B testing (priority: user field > generated UUID)
    let request_id = request
        .user
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Resolve prompt (handles A/B testing)
    let prompt = registry
        .resolve(&prompt_id, &request_id)
        .map_err(|e| format!("Prompt resolution failed: {}", e))?;

    // Get variables (use provided or defaults)
    let variables = request
        .prompt_variables
        .as_ref()
        .cloned()
        .unwrap_or_default();

    // Render template
    let rendered = crate::prompts::template::TemplateEngine::render(
        &prompt.template,
        &variables,
        &prompt.defaults,
    )
    .map_err(|e| format!("Template render failed: {}", e))?;

    // Prepend system message
    let system_msg = crate::shared_types::Message {
        role: "system".to_string(),
        content: Some(rendered),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        function_call: None,
    };
    request.messages.insert(0, system_msg);

    Ok(())
}

#[cfg(any(feature = "litellm-mode", feature = "full"))]
async fn handle_request_litellm(
    body_str: &str,
    provider: &Provider,
    api_key: &str,
    dispatch_api_base: Option<&str>,
    prompt_registry: Option<&mut crate::prompts::PromptRegistry>,
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

    // Resolve prompt template if prompt_id is set (RFC-0948)
    if let Err(e) = resolve_prompt(&mut request, prompt_registry) {
        let resp = Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(SseBody::from_error(e))
            .unwrap();
        return Ok(resp);
    }

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

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[allow(dead_code)]
async fn handle_request_anyllm(
    body_str: &str,
    provider: &Provider,
    api_key: &str,
    dispatch_api_base: Option<&str>,
) -> Result<Response<SseBody>, Infallible> {
    // Parse JSON to extract model and messages
    let json: serde_json::Value = match serde_json::from_str(body_str) {
        Ok(v) => v,
        Err(_) => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error("Invalid JSON".to_string()))
                .unwrap();
            return Ok(resp);
        }
    };

    // Extract model name
    let model = match json.get("model").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error("Missing 'model' field".to_string()))
                .unwrap();
            return Ok(resp);
        }
    };

    // Extract provider name from model string (e.g., "openai/gpt-4o" -> "openai")
    let (provider_name, model_name) = if let Some((p, m)) = model.split_once('/') {
        (p.to_string(), m.to_string())
    } else {
        (provider.name.clone(), model.clone())
    };

    // Extract messages
    let messages: Vec<crate::types::Message> = json
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let role = m.get("role")?.as_str()?.to_string();
                    let content = m
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(crate::types::Message { role, content })
                })
                .collect()
        })
        .unwrap_or_default();

    // Call py_bridge via spawn_blocking for GIL safety
    let provider_name_clone = provider_name.clone();
    let model_name_clone = model_name.clone();
    let api_key_clone = api_key.to_string();
    let api_base_clone = dispatch_api_base.map(|s| s.to_string());

    let result = tokio::task::spawn_blocking(move || {
        py_bridge::factory::completion(
            &provider_name_clone,
            &model_name_clone,
            &messages,
            Some(&api_key_clone),
            api_base_clone.as_deref(),
        )
    })
    .await;

    match result {
        Ok(Ok(completion)) => {
            // Convert ChatCompletion to JSON
            let body = serde_json::to_string(&completion).unwrap_or_else(|_| "{}".to_string());
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(SseBody::from_string(body))
                .unwrap();
            Ok(resp)
        }
        Ok(Err(e)) => {
            // Map PyBridgeError to HTTP status
            let status = match &e {
                py_bridge::PyBridgeError::ProviderError(_) => StatusCode::BAD_GATEWAY,
                py_bridge::PyBridgeError::UnsupportedProvider(_) => StatusCode::BAD_REQUEST,
                py_bridge::PyBridgeError::PyError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };

            let error_body = serde_json::json!({
                "error": {
                    "message": e.to_string(),
                    "type": "provider_error",
                }
            });

            let resp = Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(SseBody::from_string(error_body.to_string()))
                .unwrap();
            Ok(resp)
        }
        Err(e) => {
            let resp = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(SseBody::from_error(format!("Task join error: {}", e)))
                .unwrap();
            Ok(resp)
        }
    }
}

#[cfg(not(any(feature = "any-llm-mode", feature = "full")))]
#[allow(dead_code)]
async fn handle_request_anyllm(
    _body_str: &str,
    _provider: &Provider,
    _api_key: &str,
    _dispatch_api_base: Option<&str>,
) -> Result<Response<SseBody>, Infallible> {
    let resp = Response::builder()
        .status(StatusCode::NOT_IMPLEMENTED)
        .body(SseBody::from_error("any-llm-mode not enabled".to_string()))
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

// ============================================================================
// Models endpoint (RFC-0917)
// ============================================================================

/// Handle /v1/models and /v1/models/{model_id} endpoints
fn handle_models_endpoint(
    dispatch_map: &HashMap<String, DispatchInfo>,
    model_id: &str,
) -> Response<SseBody> {
    if model_id.is_empty() {
        // List all models
        let models: Vec<serde_json::Value> = dispatch_map
            .values()
            .map(|d| {
                serde_json::json!({
                    "id": d.model,
                    "object": "model",
                    "created": 0,
                    "owned_by": d.provider,
                    "permission": [],
                    "root": d.model,
                    "parent": null,
                })
            })
            .collect();

        let body = serde_json::json!({
            "object": "list",
            "data": models,
        });

        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(SseBody::from_string(body.to_string()))
            .unwrap()
    } else {
        // Get specific model
        let dispatch = dispatch_map.values().find(|d| {
            d.model == model_id
                || d.model_group.as_deref() == Some(model_id)
                || d.deployment_id == model_id
        });

        match dispatch {
            Some(d) => {
                let body = serde_json::json!({
                    "id": d.model,
                    "object": "model",
                    "created": 0,
                    "owned_by": d.provider,
                    "permission": [],
                    "root": d.model,
                    "parent": null,
                });

                Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .body(SseBody::from_string(body.to_string()))
                    .unwrap()
            }
            None => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(SseBody::from_error(format!(
                    "Model '{}' not found",
                    model_id
                )))
                .unwrap(),
        }
    }
}

// ============================================================================
// Embeddings endpoint (RFC-0917)
// ============================================================================

// ============================================================================
// Completions endpoint (RFC-0942)
// ============================================================================

/// Handle /v1/completions endpoint — legacy text completions
#[cfg(any(feature = "litellm-mode", feature = "full"))]
async fn handle_completions_endpoint(
    body_str: &str,
    provider: &Provider,
    dispatch_map: &HashMap<String, DispatchInfo>,
) -> Result<Response<SseBody>, Infallible> {
    let json: serde_json::Value = match serde_json::from_str(body_str) {
        Ok(v) => v,
        Err(_) => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error("Invalid JSON".to_string()))
                .unwrap();
            return Ok(resp);
        }
    };

    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-3.5-turbo-instruct");

    let prompt = json.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    // Convert to chat completion format
    let chat_body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": json.get("max_tokens"),
        "temperature": json.get("temperature"),
        "top_p": json.get("top_p"),
        "stop": json.get("stop"),
        "stream": json.get("stream"),
    });

    // Look up provider from dispatch map
    let dispatch = dispatch_map.values().find(|d| {
        d.model == model || d.model_group.as_deref() == Some(model) || d.deployment_id == model
    });

    let config_key = dispatch.and_then(|d| d.api_key.as_deref());
    let api_key = match resolve_api_key(provider, config_key) {
        Some(key) => key,
        None => {
            let resp = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(SseBody::from_error("API key not set".to_string()))
                .unwrap();
            return Ok(resp);
        }
    };

    let dispatch_api_base = dispatch.and_then(|d| d.api_base.clone());

    // Forward to chat completions handler
    let chat_body_str = chat_body.to_string();

    #[cfg(any(feature = "litellm-mode", feature = "full"))]
    {
        handle_request_litellm(
            &chat_body_str,
            provider,
            &api_key,
            dispatch_api_base.as_deref(),
            None, // TODO: wire prompt_registry
        )
        .await
    }

    #[cfg(not(any(feature = "litellm-mode", feature = "full")))]
    {
        handle_request_anyllm(
            &chat_body_str,
            provider,
            &api_key,
            dispatch_api_base.as_deref(),
        )
        .await
    }
}

#[cfg(not(any(feature = "litellm-mode", feature = "full")))]
async fn handle_completions_endpoint(
    _body_str: &str,
    _provider: &Provider,
    _dispatch_map: &HashMap<String, DispatchInfo>,
) -> Result<Response<SseBody>, Infallible> {
    let resp = Response::builder()
        .status(StatusCode::NOT_IMPLEMENTED)
        .body(SseBody::from_error(
            "Completions not supported in this mode".to_string(),
        ))
        .unwrap();
    Ok(resp)
}

/// Handle /v1/embeddings endpoint
#[cfg(any(feature = "litellm-mode", feature = "full"))]
async fn handle_embedding_request(
    body_str: &str,
    provider: &Provider,
    api_key: &str,
    dispatch_api_base: Option<&str>,
) -> Result<Response<SseBody>, Infallible> {
    // Parse embedding request
    let request: serde_json::Value = match serde_json::from_str(body_str) {
        Ok(req) => req,
        Err(_) => {
            let resp = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(SseBody::from_error("Invalid request body".to_string()))
                .unwrap();
            return Ok(resp);
        }
    };

    let model = request
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("text-embedding-ada-002");

    let input = request
        .get("input")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    // Build embedding request for provider
    let embedding_req = crate::native_http::HttpEmbeddingRequest {
        input: input.to_string(),
        model: model.to_string(),
        api_base: dispatch_api_base.map(String::from),
    };

    // Get provider and call embedding
    let http_provider = match crate::native_http::HttpProviderFactory::create(&provider.name) {
        Some(p) => p,
        None => {
            let resp = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(SseBody::from_error(format!(
                    "Provider '{}' not found",
                    provider.name
                )))
                .unwrap();
            return Ok(resp);
        }
    };
    match http_provider.embedding(&embedding_req, api_key).await {
        Ok(response) => {
            let body = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(SseBody::from_string(body))
                .unwrap();
            Ok(resp)
        }
        Err(e) => {
            let resp = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(SseBody::from_error(format!("Embedding error: {}", e)))
                .unwrap();
            Ok(resp)
        }
    }
}

#[cfg(not(any(feature = "litellm-mode", feature = "full")))]
async fn handle_embedding_request(
    _body_str: &str,
    _provider: &Provider,
    _api_key: &str,
    _dispatch_api_base: Option<&str>,
) -> Result<Response<SseBody>, Infallible> {
    let resp = Response::builder()
        .status(StatusCode::NOT_IMPLEMENTED)
        .body(SseBody::from_error(
            "Embeddings not supported in this mode".to_string(),
        ))
        .unwrap();
    Ok(resp)
}

// ============================================================================
// Fallback helpers (RFC-0902)
// ============================================================================

/// Classify HTTP status code into RouterError for fallback lookup
fn classify_http_error(status: StatusCode) -> crate::fallback::RouterError {
    match status.as_u16() {
        429 => crate::fallback::RouterError::RateLimit,
        503 => crate::fallback::RouterError::ProviderUnavailable,
        401 | 403 => crate::fallback::RouterError::AuthError,
        408 | 504 => crate::fallback::RouterError::Timeout,
        _ => crate::fallback::RouterError::Unknown,
    }
}

/// Try fallback models in order, returning the first successful response
#[cfg(any(feature = "litellm-mode", feature = "full"))]
async fn try_fallback_models(
    fallback_models: &[String],
    dispatch_map: &HashMap<String, DispatchInfo>,
    original_provider: &Provider,
    body_str: &str,
    max_retries: u32,
    retry_delay_ms: u64,
) -> Option<Result<Response<SseBody>, Infallible>> {
    for (attempt, model) in fallback_models.iter().enumerate() {
        if attempt >= max_retries as usize {
            break;
        }

        // Look up fallback model's DispatchInfo
        let fallback_dispatch = dispatch_map.values().find(|d| {
            d.model == *model
                || d.model_group.as_deref() == Some(model.as_str())
                || d.deployment_id == *model
        });

        // Get API key for fallback model
        let fallback_api_key = fallback_dispatch
            .and_then(|d| d.api_key.as_deref())
            .and_then(|key| {
                if key.is_empty() {
                    None
                } else {
                    Some(key.to_string())
                }
            })
            .or_else(|| {
                std::env::var(format!("{}_API_KEY", original_provider.name.to_uppercase())).ok()
            });

        let api_key = match fallback_api_key {
            Some(key) => key,
            None => continue, // Skip this fallback if no API key
        };

        let fallback_api_base = fallback_dispatch.and_then(|d| d.api_base.as_deref());

        // Apply retry delay
        if attempt > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(
                retry_delay_ms * 2u64.pow(attempt as u32 - 1),
            ))
            .await;
        }

        // Try the fallback provider
        let result = handle_request_litellm(
            body_str,
            original_provider,
            &api_key,
            fallback_api_base,
            None,
        )
        .await;

        // Return if successful
        match &result {
            Ok(resp) if resp.status().is_success() => return Some(result),
            _ => continue,
        }
    }

    None // All fallbacks failed
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
