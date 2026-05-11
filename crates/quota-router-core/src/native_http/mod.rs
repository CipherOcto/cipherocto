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
}

/// Embedding response
#[derive(Debug, Clone)]
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
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError>;
    /// Streaming completion — returns SSE chunks as async iterator
    /// Default implementation returns error for providers that don't support streaming
    async fn streaming_completion(
        &self,
        _request: &HttpCompletionRequest,
        _api_key: &str,
    ) -> Result<StreamingResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(format!(
            "{} does not support streaming",
            self.name()
        )))
    }
    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError>;
    fn routing_weight(&self) -> u32 {
        1
    }
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

    pub fn create(name: &str) -> Option<Box<dyn HttpProvider>> {
        PROVIDER_REGISTRY.read().unwrap().get(name).map(|f| f())
    }

    pub fn list_providers() -> Vec<&'static str> {
        PROVIDER_REGISTRY.read().unwrap().keys().copied().collect()
    }
}

/// Initialize all native_http providers — call at startup
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub fn init_providers() {
    HttpProviderFactory::register("openai", || Box::new(openai::OpenAIProvider::new()));
    HttpProviderFactory::register("anthropic", || Box::new(anthropic::AnthropicProvider::new()));
    HttpProviderFactory::register("mistral", || Box::new(mistral::MistralProvider::new()));
    HttpProviderFactory::register("gemini", || Box::new(gemini::GeminiProvider::new()));
    HttpProviderFactory::register("azure", || Box::new(azure::AzureProvider::new()));
    HttpProviderFactory::register("bedrock", || Box::new(bedrock::BedrockProvider::new()));
    HttpProviderFactory::register("ollama", || Box::new(ollama::OllamaProvider::new()));
    HttpProviderFactory::register("groq", || Box::new(groq::GroqProvider::new()));
    HttpProviderFactory::register("together", || Box::new(together::TogetherProvider::new()));
    HttpProviderFactory::register("replicate", || Box::new(replicate::ReplicateProvider::new()));
}
