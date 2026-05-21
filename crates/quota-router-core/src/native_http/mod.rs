// native_http — LiteLLM mode providers via reqwest (INTERNAL boundary #1 per RFC-0917)
//
// This module provides LiteLLM mode via direct HTTP calls to provider REST APIs.
// It is called by proxy.rs / router.rs (EXTERNAL boundary #2).
//
// Per RFC-0917 lines 291, 340-369, 1833, 1855, 1861:
// "native_http: reqwest → provider REST APIs"

#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod anthropic;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod azure;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod bedrock;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod databricks;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod gemini;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod groq;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod mistral;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod ollama;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod openai;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod perplexity;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod replicate;
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod together;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::mpsc;

/// Provider error types
#[derive(Debug, Clone)]
pub enum ProviderError {
    Network(String),
    InvalidResponse(String),
    AuthError(String),
    RateLimit(String),
    UnsupportedModel(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Network(s) => write!(f, "Network error: {}", s),
            ProviderError::InvalidResponse(s) => write!(f, "Invalid response: {}", s),
            ProviderError::AuthError(s) => write!(f, "Auth error: {}", s),
            ProviderError::RateLimit(s) => write!(f, "Rate limit: {}", s),
            ProviderError::UnsupportedModel(s) => write!(f, "Unsupported model: {}", s),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Completion request — OpenAI-compatible format per RFC-0917
/// Extended with function calling fields per RFC-0939
#[derive(Debug, Clone)]
pub struct HttpCompletionRequest {
    pub model: String,
    pub messages: Vec<crate::shared_types::Message>,
    pub stream: Option<bool>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub stop: Option<Vec<String>>,
    pub n: Option<u32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub user: Option<String>,
    /// Per-deployment API base URL (optional).
    /// If Some, the provider should use this instead of its default api_base.
    /// This enables litellm-mode per-deployment api_base forwarding (RFC-0929).
    pub api_base: Option<String>,
    // Function calling fields (RFC-0939)
    pub tools: Option<Vec<crate::shared_types::Tool>>,
    pub tool_choice: Option<crate::shared_types::ToolChoice>,
    pub response_format: Option<crate::shared_types::ResponseFormat>,
    pub seed: Option<i64>,
    pub logprobs: Option<bool>,
    pub top_logprobs: Option<usize>,
    pub parallel_tool_calls: Option<bool>,
    // Prompt management fields (RFC-0948)
    pub prompt_id: Option<String>,
    pub prompt_variables: Option<std::collections::HashMap<String, String>>,
    /// Provider-specific parameters (e.g., Perplexity return_citations, search_domain_filter).
    /// Passed through as arbitrary JSON to the provider API.
    pub provider_params: Option<serde_json::Value>,
}

impl HttpCompletionRequest {
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// Embedding request
#[derive(Debug, Clone)]
pub struct HttpEmbeddingRequest {
    pub input: String,
    pub model: String,
    /// Per-deployment API base URL (optional).
    /// If Some, the provider should use this instead of its default api_base.
    pub api_base: Option<String>,
}

/// Embedding response
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpEmbeddingResponse {
    pub object: String,
    pub data: Vec<crate::shared_types::Embedding>,
    pub model: String,
    pub usage: crate::shared_types::Usage,
}

/// Completion response — OpenAI-compatible format per RFC-0917
#[derive(Debug, Clone)]
pub struct HttpCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<crate::shared_types::Choice>,
    pub usage: crate::shared_types::Usage,
    /// Provider-specific metadata (e.g., Perplexity citations)
    pub metadata: Option<serde_json::Value>,
}

#[async_trait]
pub trait HttpProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supported_models(&self) -> Vec<&str>;
    fn supports_model(&self, model: &str) -> bool {
        self.supported_models().contains(&model)
    }
    /// Returns true if this provider supports streaming completions
    fn supports_streaming(&self) -> bool {
        false
    }
    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError>;
    /// Streaming completion — returns SSE chunks as async iterator
    /// Default implementation returns error for providers that don't support streaming
    async fn streaming_completion(
        &self,
        _request: &HttpCompletionRequest,
        _api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support streaming",
            self.name()
        )))
    }
    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError>;
    /// Retrieve a response by ID (OpenAI Responses API).
    /// Default returns UnsupportedModel for providers that don't support it.
    async fn get_response(
        &self,
        _response_id: &str,
        _api_key: Option<&str>,
        _api_base: Option<&str>,
    ) -> Result<HttpResponseObject, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Responses API",
            self.name()
        )))
    }
    /// Delete a response by ID (OpenAI Responses API).
    /// Default returns UnsupportedModel for providers that don't support it.
    async fn delete_response(
        &self,
        _response_id: &str,
        _api_key: Option<&str>,
        _api_base: Option<&str>,
    ) -> Result<HttpDeletedObject, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Responses API",
            self.name()
        )))
    }
    fn routing_weight(&self) -> u32 {
        1
    }
}

/// Response object from OpenAI Responses API
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpResponseObject {
    pub id: String,
    pub object: String,
    pub model: String,
    pub status: String,
    pub output: Vec<serde_json::Value>,
    pub usage: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Deleted object response from OpenAI Responses API
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpDeletedObject {
    pub id: String,
    pub object: String,
    pub deleted: bool,
}

/// Streaming response — channel-based SSE chunk delivery
/// Provider sends chunks via sender, proxy receives via receiver
pub struct StreamingResponse {
    /// Receiver for SSE chunks from provider
    pub receiver: mpsc::Receiver<Result<StreamingChunk, ProviderError>>,
    /// Content type for this streaming response
    pub content_type: &'static str,
}

/// A streaming chunk — either raw SSE bytes or structured chunk
pub enum StreamingChunk {
    /// Raw SSE bytes to forward directly (for OpenAI passthrough)
    RawSSE(Vec<u8>),
    /// Structured chunk for conversion (for Anthropic → OpenAI SSE)
    Structured(crate::shared_types::ChatCompletionChunk),
}

/// Provider factory function type
type ProviderFactory = fn() -> Box<dyn HttpProvider>;

/// Provider registry — static factory pattern
static PROVIDER_REGISTRY: LazyLock<std::sync::RwLock<HashMap<&'static str, ProviderFactory>>> =
    LazyLock::new(|| std::sync::RwLock::new(HashMap::new()));

pub struct HttpProviderFactory;

impl HttpProviderFactory {
    pub fn register(name: &'static str, factory: fn() -> Box<dyn HttpProvider>) {
        PROVIDER_REGISTRY.write().unwrap().insert(name, factory);
    }

    /// Create a provider by name.
    /// Note: api_base is NOT passed to factory — it comes from HttpCompletionRequest.api_base
    /// at call time. This allows per-request api_base override without rebuilding the provider.
    pub fn create(name: &str) -> Option<Box<dyn HttpProvider>> {
        PROVIDER_REGISTRY.read().unwrap().get(name).map(|f| f())
    }

    /// Create a provider with optional api_base.
    /// Currently unused — api_base forwarding happens via HttpCompletionRequest.api_base.
    /// Kept for RFC-0929 AC compliance: HttpProviderFactory::create() accepts api_base parameter.
    pub fn create_with_api_base(
        name: &str,
        _api_base: Option<&str>,
    ) -> Option<Box<dyn HttpProvider>> {
        // api_base is forwarded via HttpCompletionRequest.api_base at call time,
        // not at provider creation time. This allows per-request override.
        Self::create(name)
    }

    pub fn list_providers() -> Vec<&'static str> {
        PROVIDER_REGISTRY.read().unwrap().keys().copied().collect()
    }
}

/// Initialize all native_http providers — call at startup
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub fn init_providers() {
    HttpProviderFactory::register("openai", || Box::new(openai::OpenAIProvider::new()));
    HttpProviderFactory::register(
        "anthropic",
        || Box::new(anthropic::AnthropicProvider::new()),
    );
    HttpProviderFactory::register("mistral", || Box::new(mistral::MistralProvider::new()));
    HttpProviderFactory::register("gemini", || Box::new(gemini::GeminiProvider::new()));
    HttpProviderFactory::register("azure", || Box::new(azure::AzureProvider::new()));
    HttpProviderFactory::register("bedrock", || Box::new(bedrock::BedrockProvider::new()));
    HttpProviderFactory::register("ollama", || Box::new(ollama::OllamaProvider::new()));
    HttpProviderFactory::register("groq", || Box::new(groq::GroqProvider::new()));
    HttpProviderFactory::register("together", || Box::new(together::TogetherProvider::new()));
    HttpProviderFactory::register(
        "replicate",
        || Box::new(replicate::ReplicateProvider::new()),
    );
    HttpProviderFactory::register("databricks", || {
        Box::new(databricks::DatabricksProvider::new())
    });
    HttpProviderFactory::register("perplexity", || {
        Box::new(perplexity::PerplexityProvider::new())
    });
}

/// Build an OpenAI-compatible request body from [`HttpCompletionRequest`].
///
/// `model` is passed separately because each provider strips its own prefix
/// (e.g. "databricks/", "perplexity/") before sending.
///
/// All optional fields (temperature, max\_tokens, tools, etc.) are included
/// only when present, matching the wire format expected by OpenAI-compatible APIs.
pub fn build_openai_compatible_body(
    request: &HttpCompletionRequest,
    model: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": request.messages.iter().map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content
            })
        }).collect::<Vec<_>>()
    });

    if let Some(stream) = request.stream {
        body["stream"] = serde_json::json!(stream);
    }
    if let Some(temp) = request.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }
    if let Some(top_p) = request.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(stop) = &request.stop {
        body["stop"] = serde_json::json!(stop);
    }
    if let Some(n) = request.n {
        body["n"] = serde_json::json!(n);
    }
    if let Some(p) = request.presence_penalty {
        body["presence_penalty"] = serde_json::json!(p);
    }
    if let Some(p) = request.frequency_penalty {
        body["frequency_penalty"] = serde_json::json!(p);
    }
    if let Some(user) = &request.user {
        body["user"] = serde_json::json!(user);
    }
    if let Some(seed) = request.seed {
        body["seed"] = serde_json::json!(seed);
    }
    // Function calling fields (RFC-0939)
    if let Some(tools) = &request.tools {
        body["tools"] = serde_json::to_value(tools).unwrap_or_default();
    }
    if let Some(tool_choice) = &request.tool_choice {
        body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
    }
    if let Some(fmt) = &request.response_format {
        body["response_format"] = serde_json::to_value(fmt).unwrap_or_default();
    }
    // Logprobs fields (OpenAI-compatible)
    if let Some(logprobs) = request.logprobs {
        body["logprobs"] = serde_json::json!(logprobs);
    }
    if let Some(top_logprobs) = request.top_logprobs {
        body["top_logprobs"] = serde_json::json!(top_logprobs);
    }
    // Parallel tool calls (OpenAI-compatible)
    if let Some(parallel_tool_calls) = request.parallel_tool_calls {
        body["parallel_tool_calls"] = serde_json::json!(parallel_tool_calls);
    }
    // Prompt management fields (RFC-0948)
    if let Some(prompt_id) = &request.prompt_id {
        body["prompt_id"] = serde_json::json!(prompt_id);
    }
    if let Some(prompt_variables) = &request.prompt_variables {
        body["prompt_variables"] = serde_json::to_value(prompt_variables).unwrap_or_default();
    }

    body
}

/// Shared streaming helper for OpenAI-compatible providers (RFC-0941).
///
/// This function handles the common SSE parsing logic for providers that use
/// OpenAI-compatible streaming format: Groq, Together, Ollama, Mistral, Azure.
pub async fn stream_openai_compatible(
    client: &reqwest::Client,
    url: &str,
    api_key: Option<&str>,
    body: serde_json::Value,
) -> Result<StreamingResponse, ProviderError> {
    let mut req_builder = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body);
    if let Some(key) = api_key {
        req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
    }
    let resp = req_builder
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        if status == 401 || status == 403 {
            return Err(ProviderError::AuthError(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }
        if status == 429 {
            return Err(ProviderError::RateLimit(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }
        return Err(ProviderError::InvalidResponse(format!(
            "HTTP {}: {}",
            status, err_body
        )));
    }

    let (tx, rx) = tokio::sync::mpsc::channel(100);

    // Spawn task to read SSE bytes and forward them
    tokio::spawn(async move {
        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    if tx
                        .send(Ok(StreamingChunk::RawSSE(bytes.to_vec())))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(ProviderError::Network(e.to_string()))).await;
                    break;
                }
            }
        }
    });

    Ok(StreamingResponse {
        receiver: rx,
        content_type: "text/event-stream",
    })
}
