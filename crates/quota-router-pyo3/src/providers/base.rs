// Base provider trait and types for quota-router-pyo3
// Inspired by any-llm's AnyLLM abstract base class

use crate::exceptions::ProviderError;
use crate::types::{ChatCompletion, Message};

/// Provider feature flags
#[derive(Debug, Clone)]
pub struct ProviderFeatures {
    pub supports_completion: bool,
    pub supports_completion_streaming: bool,
    pub supports_embedding: bool,
    pub supports_responses: bool,
    pub supports_list_models: bool,
    pub supports_batch: bool,
    pub supports_messages: bool,
}

/// Provider metadata
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ProviderMetadata {
    pub name: String,
    pub documentation_url: String,
    pub env_api_key: String,
    pub env_api_base: Option<String>,
    pub api_base: Option<String>,
    pub features: ProviderFeatures,
}

/// Trait for LLM providers
/// Each provider implements this trait to handle API calls to its SDK
#[allow(dead_code)]
pub trait LLMProvider: Send + Sync {
    /// Get provider metadata
    fn metadata(&self) -> &ProviderMetadata;

    /// Initialize the provider client with API key and optional base URL
    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError>;

    /// Check if required packages are available
    fn check_packages(&self) -> Result<(), String>;

    /// Make a completion call
    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, ProviderError>;

    /// Make an async completion call
    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, ProviderError>;

    /// Make an embedding call
    fn embedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<crate::types::EmbeddingsResponse, ProviderError>;

    /// Make an async embedding call
    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<crate::types::EmbeddingsResponse, ProviderError>;
}

/// Static provider info - shared across all instances of a provider
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: &'static str,
    pub doc_url: &'static str,
    pub env_api_key: &'static str,
    pub env_api_base: Option<&'static str>,
    pub api_base: Option<&'static str>,
    pub features: ProviderFeatures,
}

impl ProviderInfo {
    pub const fn new(
        name: &'static str,
        doc_url: &'static str,
        env_api_key: &'static str,
        env_api_base: Option<&'static str>,
        api_base: Option<&'static str>,
        features: ProviderFeatures,
    ) -> Self {
        Self {
            name,
            doc_url,
            env_api_key,
            env_api_base,
            api_base,
            features,
        }
    }
}

/// All supported providers
#[allow(clippy::upper_case_acronyms)]
pub struct Providers;

impl Providers {
    /// Get provider info by name
    pub fn get(name: &str) -> Option<&'static ProviderInfo> {
        match name.to_lowercase().as_str() {
            "openai" => Some(&OPENAI_INFO),
            "anthropic" => Some(&ANTHROPIC_INFO),
            "mistral" => Some(&MISTRAL_INFO),
            "ollama" => Some(&OLLAMA_INFO),
            "gemini" => Some(&GEMINI_INFO),
            _ => None,
        }
    }

    /// List all supported provider names
    pub fn list_names() -> Vec<&'static str> {
        vec![
            "openai",
            "anthropic",
            "mistral",
            "ollama",
            "gemini",
            "azure",
            "azureopenai",
            "azureanthropic",
            "bedrock",
            "cerebras",
            "cohere",
            "dashscope",
            "databricks",
            "deepseek",
            "fireworks",
            "gateway",
            "groq",
            "huggingface",
            "inception",
            "llama",
            "llamacpp",
            "llamafile",
            "lmstudio",
            "minimax",
            "moonshot",
            "mzai",
            "nebius",
            "openrouter",
            "perplexity",
            "platform",
            "portkey",
            "sagemaker",
            "sambanova",
            "together",
            "vertexai",
            "vertexaianthropic",
            "vllm",
            "voyage",
            "watsonx",
            "xai",
            "zai",
        ]
    }
}

// Provider metadata constants
pub const OPENAI_INFO: ProviderInfo = ProviderInfo::new(
    "openai",
    "https://platform.openai.com/docs/api-reference",
    "OPENAI_API_KEY",
    Some("OPENAI_BASE_URL"),
    Some("https://api.openai.com/v1"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: true,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const ANTHROPIC_INFO: ProviderInfo = ProviderInfo::new(
    "anthropic",
    "https://docs.anthropic.com/en/api/reference",
    "ANTHROPIC_API_KEY",
    Some("ANTHROPIC_BASE_URL"),
    Some("https://api.anthropic.com"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const MISTRAL_INFO: ProviderInfo = ProviderInfo::new(
    "mistral",
    "https://docs.mistral.com/api/",
    "MISTRAL_API_KEY",
    Some("MISTRAL_BASE_URL"),
    Some("https://api.mistral.ai/v1"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const OLLAMA_INFO: ProviderInfo = ProviderInfo::new(
    "ollama",
    "https://github.com/ollama/ollama",
    "OLLAMA_API_KEY", // No standard env var for ollama, often uses no auth
    Some("OLLAMA_BASE_URL"),
    Some("http://localhost:11434"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const GEMINI_INFO: ProviderInfo = ProviderInfo::new(
    "gemini",
    "https://ai.google.dev/api/rest",
    "GOOGLE_API_KEY",
    Some("GOOGLE_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);
