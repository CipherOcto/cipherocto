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
    /// Request timeout in seconds (None = provider default, typically 600s)
    pub timeout: Option<f64>,
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
    /// Request timeout in seconds (None = provider default)
    pub timeout: Option<f64>,
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
        _timeout: Option<f64>,
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
        _timeout: Option<f64>,
    ) -> Result<HttpDeletedObject, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Responses API",
            self.name()
        )))
    }
    /// Create a response via OpenAI Responses API.
    async fn create_response(
        &self,
        _request: &HttpResponsesRequest,
        _api_key: Option<&str>,
    ) -> Result<HttpResponseObject, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Responses API",
            self.name()
        )))
    }
    /// Create a batch job.
    async fn batch_create(
        &self,
        _request: &HttpBatchCreateRequest,
        _api_key: Option<&str>,
    ) -> Result<HttpBatchObject, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Batch API",
            self.name()
        )))
    }
    /// Retrieve a batch job.
    async fn batch_retrieve(
        &self,
        _batch_id: &str,
        _api_key: Option<&str>,
        _api_base: Option<&str>,
        _timeout: Option<f64>,
    ) -> Result<HttpBatchObject, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Batch API",
            self.name()
        )))
    }
    /// Cancel a batch job.
    async fn batch_cancel(
        &self,
        _batch_id: &str,
        _api_key: Option<&str>,
        _api_base: Option<&str>,
        _timeout: Option<f64>,
    ) -> Result<HttpBatchObject, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Batch API",
            self.name()
        )))
    }
    /// List batch jobs.
    async fn batch_list(
        &self,
        _api_key: Option<&str>,
        _api_base: Option<&str>,
        _limit: Option<u32>,
        _timeout: Option<f64>,
    ) -> Result<HttpBatchListResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Batch API",
            self.name()
        )))
    }
    /// Get batch results.
    async fn batch_results(
        &self,
        _batch_id: &str,
        _api_key: Option<&str>,
        _api_base: Option<&str>,
        _timeout: Option<f64>,
    ) -> Result<HttpBatchResultsResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support the Batch API",
            self.name()
        )))
    }
    /// List available models for this provider.
    async fn list_models(
        &self,
        _api_key: Option<&str>,
        _api_base: Option<&str>,
        _timeout: Option<f64>,
    ) -> Result<HttpListModelsResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support list_models",
            self.name()
        )))
    }
    fn routing_weight(&self) -> u32 {
        1
    }
}

/// Model object from list_models
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpModelObject {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Batch create request
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpBatchCreateRequest {
    pub input_file: String,
    pub endpoint: String,
    pub completion_window: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    /// Request timeout in seconds (None = provider default)
    #[serde(skip)]
    pub timeout: Option<f64>,
}

/// Batch object from OpenAI Batch API
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpBatchObject {
    pub id: String,
    pub object: String,
    pub endpoint: String,
    pub status: String,
    #[serde(default)]
    pub input_file_id: String,
    #[serde(default)]
    pub output_file_id: Option<String>,
    #[serde(default)]
    pub error_file_id: Option<String>,
    #[serde(default)]
    pub errors: Option<serde_json::Value>,
    #[serde(default)]
    pub completion_window: Option<String>,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub in_progress_at: Option<u64>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub finalizing_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub failed_at: Option<u64>,
    #[serde(default)]
    pub expired_at: Option<u64>,
    #[serde(default)]
    pub cancelling_at: Option<u64>,
    #[serde(default)]
    pub cancelled_at: Option<u64>,
    #[serde(default)]
    pub request_counts: Option<serde_json::Value>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Batch list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpBatchListResponse {
    pub object: String,
    pub data: Vec<HttpBatchObject>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub first_id: Option<String>,
    #[serde(default)]
    pub last_id: Option<String>,
}

/// Batch results response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpBatchResultsResponse {
    pub results: Vec<serde_json::Value>,
}

/// List models response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpListModelsResponse {
    pub object: String,
    pub data: Vec<HttpModelObject>,
}

/// Request for OpenAI Responses API
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpResponsesRequest {
    pub model: String,
    pub input: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    /// Request timeout in seconds (None = provider default)
    #[serde(skip)]
    pub timeout: Option<f64>,
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

impl std::fmt::Debug for StreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingResponse")
            .field("content_type", &self.content_type)
            .finish()
    }
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
        let bearer = crate::egress::key_swap::attach_bearer(&key)
                .expect("provider-boundary key-swap: api_key MUST be provider-shaped; if this fires, the upstream source path leaked a CipherOcto key");
        req_builder = req_builder.header("Authorization", bearer);
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

#[cfg(test)]
#[cfg(any(feature = "litellm-mode", feature = "full"))]
mod tests {
    use super::*;
    use crate::native_http::openai::OpenAIProvider;

    // =====================================================================
    // ProviderError tests
    // =====================================================================

    #[test]
    fn test_provider_error_display() {
        assert_eq!(
            ProviderError::Network("timeout".into()).to_string(),
            "Network error: timeout"
        );
        assert_eq!(
            ProviderError::InvalidResponse("bad json".into()).to_string(),
            "Invalid response: bad json"
        );
        assert_eq!(
            ProviderError::AuthError("unauthorized".into()).to_string(),
            "Auth error: unauthorized"
        );
        assert_eq!(
            ProviderError::RateLimit("too many".into()).to_string(),
            "Rate limit: too many"
        );
        assert_eq!(
            ProviderError::UnsupportedModel("gpt-5".into()).to_string(),
            "Unsupported model: gpt-5"
        );
    }

    #[test]
    fn test_provider_error_is_error() {
        let err = ProviderError::Network("test".into());
        let _: &dyn std::error::Error = &err;
    }

    // =====================================================================
    // HttpCompletionRequest tests
    // =====================================================================

    #[test]
    fn test_http_completion_request_model() {
        let req = HttpCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            parallel_tool_calls: None,
            prompt_id: None,
            prompt_variables: None,
            provider_params: None,
            timeout: None,
        };
        assert_eq!(req.model(), "gpt-4o");
    }

    // =====================================================================
    // HttpProviderFactory tests
    // =====================================================================

    #[test]
    fn test_provider_factory_register_and_create() {
        HttpProviderFactory::register("test_provider", || Box::new(OpenAIProvider::new()));
        let provider = HttpProviderFactory::create("test_provider");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "openai");
    }

    #[test]
    fn test_provider_factory_create_nonexistent() {
        let provider = HttpProviderFactory::create("nonexistent_provider_xyz");
        assert!(provider.is_none());
    }

    #[test]
    fn test_provider_factory_list_providers() {
        HttpProviderFactory::register("test_list_1", || Box::new(OpenAIProvider::new()));
        HttpProviderFactory::register("test_list_2", || Box::new(OpenAIProvider::new()));
        let providers = HttpProviderFactory::list_providers();
        assert!(providers.contains(&"test_list_1"));
        assert!(providers.contains(&"test_list_2"));
    }

    #[test]
    fn test_provider_factory_create_with_api_base() {
        HttpProviderFactory::register("test_api_base", || Box::new(OpenAIProvider::new()));
        let provider =
            HttpProviderFactory::create_with_api_base("test_api_base", Some("http://custom"));
        assert!(provider.is_some());
    }

    // =====================================================================
    // build_openai_compatible_body tests
    // =====================================================================

    #[test]
    fn test_build_openai_compatible_body_minimal() {
        let req = HttpCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![crate::shared_types::Message {
                role: "user".into(),
                content: Some("hello".into()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                function_call: None,
            }],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            parallel_tool_calls: None,
            prompt_id: None,
            prompt_variables: None,
            provider_params: None,
            timeout: None,
        };
        let body = build_openai_compatible_body(&req, "gpt-4o");
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert!(body.get("stream").is_none());
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn test_build_openai_compatible_body_all_fields() {
        let req = HttpCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![crate::shared_types::Message {
                role: "user".into(),
                content: Some("hi".into()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                function_call: None,
            }],
            stream: Some(true),
            temperature: Some(0.7),
            max_tokens: Some(1024),
            top_p: Some(0.9),
            stop: Some(vec!["END".into()]),
            n: Some(2),
            presence_penalty: Some(0.5),
            frequency_penalty: Some(0.3),
            user: Some("u-123".into()),
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: Some(42),
            logprobs: Some(true),
            top_logprobs: Some(5),
            parallel_tool_calls: Some(false),
            prompt_id: Some("my-prompt".into()),
            prompt_variables: Some([("name".into(), "Alice".into())].into_iter().collect()),
            provider_params: None,
            timeout: None,
        };
        let body = build_openai_compatible_body(&req, "gpt-4o");
        assert_eq!(body["stream"], true);
        assert!((body["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
        assert_eq!(body["max_tokens"], 1024);
        assert!((body["top_p"].as_f64().unwrap() - 0.9).abs() < 0.01);
        assert_eq!(body["stop"], serde_json::json!(["END"]));
        assert_eq!(body["n"], 2);
        assert!((body["presence_penalty"].as_f64().unwrap() - 0.5).abs() < 0.01);
        assert!((body["frequency_penalty"].as_f64().unwrap() - 0.3).abs() < 0.01);
        assert_eq!(body["user"], "u-123");
        assert_eq!(body["seed"], 42);
        assert_eq!(body["logprobs"], true);
        assert_eq!(body["top_logprobs"], 5);
        assert_eq!(body["parallel_tool_calls"], false);
        assert_eq!(body["prompt_id"], "my-prompt");
    }

    #[test]
    fn test_build_openai_compatible_body_model_override() {
        let req = HttpCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![crate::shared_types::Message {
                role: "user".into(),
                content: Some("hi".into()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                function_call: None,
            }],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            parallel_tool_calls: None,
            prompt_id: None,
            prompt_variables: None,
            provider_params: None,
            timeout: None,
        };
        let body = build_openai_compatible_body(&req, "gpt-3.5-turbo");
        assert_eq!(body["model"], "gpt-3.5-turbo");
    }

    // =====================================================================
    // init_providers tests
    // =====================================================================

    #[test]
    fn test_init_providers_registers_all() {
        init_providers();
        let providers = HttpProviderFactory::list_providers();
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"anthropic"));
        assert!(providers.contains(&"mistral"));
        assert!(providers.contains(&"gemini"));
        assert!(providers.contains(&"azure"));
        assert!(providers.contains(&"bedrock"));
        assert!(providers.contains(&"ollama"));
        assert!(providers.contains(&"groq"));
        assert!(providers.contains(&"together"));
        assert!(providers.contains(&"replicate"));
        assert!(providers.contains(&"databricks"));
        assert!(providers.contains(&"perplexity"));
    }

    // =====================================================================
    // OpenAI provider tests
    // =====================================================================

    #[test]
    fn test_openai_provider_new() {
        let provider = openai::OpenAIProvider::new();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_provider_supported_models() {
        let provider = openai::OpenAIProvider::new();
        let models = provider.supported_models();
        assert!(models.contains(&"gpt-4"));
        assert!(models.contains(&"gpt-4o"));
        assert!(models.contains(&"gpt-3.5-turbo"));
    }

    #[test]
    fn test_openai_provider_supports_streaming() {
        let provider = openai::OpenAIProvider::new();
        assert!(provider.supports_streaming());
    }

    #[test]
    fn test_openai_provider_with_api_base() {
        let provider =
            openai::OpenAIProvider::new().with_api_base("https://custom.api.com/v1".into());
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_provider_default() {
        let provider = openai::OpenAIProvider::default();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_provider_supports_model() {
        let provider = openai::OpenAIProvider::new();
        assert!(provider.supports_model("gpt-4o"));
        assert!(!provider.supports_model("claude-3-opus"));
    }

    #[test]
    fn test_openai_provider_routing_weight() {
        let provider = openai::OpenAIProvider::new();
        assert_eq!(provider.routing_weight(), 10);
    }

    #[test]
    fn test_openai_provider_default_trait_methods() {
        let provider = openai::OpenAIProvider::new();
        let rt = tokio::runtime::Runtime::new().unwrap();

        // OpenAI implements these methods, so they should succeed or fail gracefully
        let result = rt.block_on(provider.get_response("resp_123", None, None, None));
        // get_response makes an HTTP call - will fail with network error since no real API
        assert!(result.is_err());

        let result = rt.block_on(provider.delete_response("resp_123", None, None, None));
        assert!(result.is_err());
    }

    #[test]
    fn test_openai_provider_embedding_unsupported() {
        let provider = openai::OpenAIProvider::new();
        let req = HttpEmbeddingRequest {
            input: "test".into(),
            model: "text-embedding-ada-002".into(),
            api_base: None,
            timeout: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(provider.embedding(&req, None));
        assert!(result.is_err());
    }
}
