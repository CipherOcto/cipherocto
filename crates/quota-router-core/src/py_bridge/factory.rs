// py_bridge factory — creates and dispatches to providers
//
// Provides a unified interface for calling any Python SDK provider.
// This is the INTERNAL boundary #1 (core → Python SDKs).

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use crate::types::Message;

// Re-export PyBridgeError from openai for consistency across all providers
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub use crate::py_bridge::openai::PyBridgeError;

/// Dispatch completion call to the appropriate provider
///
/// Per RFC-0929 REQUIRED changes:
/// - api_base: Option<&str> — per-deployment API base URL for custom endpoints
///   Security: api_base is NOT logged — it's forwarded to provider without logging
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub fn completion(
    provider: &str,
    model: &str,
    messages: &[Message],
    api_key: Option<&str>,
    api_base: Option<&str>, // per-deployment api_base (RFC-0929)
) -> Result<crate::types::ChatCompletion, PyBridgeError> {
    match provider {
        "openai" => {
            let mut p = crate::py_bridge::openai::OpenAIProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "anthropic" => {
            let mut p = crate::py_bridge::anthropic::AnthropicProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "mistral" => {
            let mut p = crate::py_bridge::mistral::MistralProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "gemini" => {
            let mut p = crate::py_bridge::gemini::GeminiProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "azure" => {
            let mut p = crate::py_bridge::azure::AzureProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "huggingface" => {
            let mut p = crate::py_bridge::huggingface::HuggingFaceProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "voyage" => {
            let mut p = crate::py_bridge::voyage::VoyageProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "cohere" => {
            let mut p = crate::py_bridge::cohere::CohereProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "deepseek" => {
            let mut p = crate::py_bridge::deepseek::DeepSeekProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "groq" => {
            let mut p = crate::py_bridge::groq::GroqProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "together" => {
            let mut p = crate::py_bridge::together::TogetherProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "openrouter" => {
            let mut p = crate::py_bridge::openrouter::OpenRouterProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "fireworks" => {
            let mut p = crate::py_bridge::fireworks::FireworksProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "cerebras" => {
            let mut p = crate::py_bridge::cerebras::CerebrasProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "deepinfra" => {
            let mut p = crate::py_bridge::deepinfra::DeepInfraProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "nebius" => {
            let mut p = crate::py_bridge::nebius::NebiusProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "moonshot" => {
            let mut p = crate::py_bridge::moonshot::MoonshotProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "minimax" => {
            let mut p = crate::py_bridge::minimax::MiniMaxProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "dashscope" => {
            let mut p = crate::py_bridge::dashscope::DashScopeProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "llamacpp" => {
            let mut p = crate::py_bridge::llamacpp::LlamaCppProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "llamafile" => {
            let mut p = crate::py_bridge::llamafile::LlamaFileProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "lmstudio" => {
            let mut p = crate::py_bridge::lmstudio::LMStudioProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "ollama" => {
            let mut p = crate::py_bridge::ollama::OllamaProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "portkey" => {
            let mut p = crate::py_bridge::portkey::PortkeyProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "xai" => {
            let mut p = crate::py_bridge::xai::XaiProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "vertexai" => {
            let mut p = crate::py_bridge::vertexai::VertexAIProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "sambanova" => {
            let mut p = crate::py_bridge::sambanova::SambaNovaProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "inception" => {
            let mut p = crate::py_bridge::inception::InceptionProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "watsonx" => {
            let mut p = crate::py_bridge::watsonx::WatsonxProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "bedrock" => {
            let mut p = crate::py_bridge::bedrock::BedrockProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "sagemaker" => {
            let mut p = crate::py_bridge::sagemaker::SageMakerProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "ai21" => {
            let mut p = crate::py_bridge::ai21::AI21Provider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "replicate" => {
            let mut p = crate::py_bridge::replicate::ReplicateProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "nvidia" => {
            let mut p = crate::py_bridge::nvidia::NvidiaProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "aleph_alpha" => {
            let mut p = crate::py_bridge::aleph_alpha::AlephAlphaProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "conjure" => {
            let mut p = crate::py_bridge::conjure::ConjureProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "infere" => {
            let mut p = crate::py_bridge::infere::InfereProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "level_ai" => {
            let mut p = crate::py_bridge::level_ai::LevelAiProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "ai_foundry" => {
            let mut p = crate::py_bridge::ai_foundry::AiFoundryProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "mistral_large" => {
            let mut p = crate::py_bridge::mistral_large::MistralLargeProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "cloudflareai" => {
            let mut p = crate::py_bridge::cloudflareai::CloudflareProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        "workersai" => {
            let mut p = crate::py_bridge::workersai::WorkersProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            p.completion(model, messages)
        }
        _ => Err(PyBridgeError::UnsupportedProvider(format!(
            "Provider '{}' not yet implemented in py_bridge",
            provider
        ))),
    }
}

/// Dispatch streaming completion call to the appropriate provider
///
/// Returns a receiver for SSE chunks. Only OpenAI provider supports streaming currently.
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub fn streaming_completion(
    provider: &str,
    model: &str,
    messages: &[Message],
    api_key: Option<&str>,
    api_base: Option<&str>,
) -> Result<
    tokio::sync::mpsc::Receiver<Result<crate::py_bridge::openai::PyBridgeChunk, PyBridgeError>>,
    PyBridgeError,
> {
    match provider {
        "openai" => {
            let mut p = crate::py_bridge::openai::OpenAIProvider::new();
            if let Some(key) = api_key {
                p = p.with_api_key(key.to_string());
            }
            if let Some(base) = api_base {
                p = p.with_api_base(base.to_string());
            }
            use crate::py_bridge::openai::PyBridgeProvider;
            p.streaming_completion(model, messages)
        }
        _ => Err(PyBridgeError::UnsupportedProvider(format!(
            "Streaming not supported for provider '{}' in py_bridge",
            provider
        ))),
    }
}
