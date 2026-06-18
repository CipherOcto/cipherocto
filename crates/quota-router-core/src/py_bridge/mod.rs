// py_bridge — PyO3 → official Python SDKs (INTERNAL boundary #1 per RFC-0917)
//
// This module is the INTERNAL boundary between Rust core and official Python SDKs.
// It is called by python_sdk_entry (EXTERNAL boundary #2).

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod factory;
//
// Per RFC-0917 lines 293-294:
// "pub mod py_bridge;    // PyO3 → official Python SDKs"

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod ai21;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod ai_foundry;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod aleph_alpha;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod anthropic;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod azure;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod bedrock;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod cerebras;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod cloudflareai;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod cohere;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod conjure;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod dashscope;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod deepinfra;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod deepseek;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod fireworks;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod gemini;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod groq;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod huggingface;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod inception;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod infere;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod level_ai;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod llamacpp;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod llamafile;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod lmstudio;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod minimax;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod mistral;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod mistral_large;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod moonshot;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod nebius;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod nvidia;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod ollama;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod openai;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod openrouter;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod portkey;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod replicate;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod sagemaker;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod sambanova;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod together;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod vertexai;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod voyage;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod watsonx;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod workersai;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod xai;

// Re-export provider trait and error type for py_bridge
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub use openai::PyBridgeError;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub use openai::PyBridgeProvider;

/// Provider factory function type
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
type PyBridgeFactory = fn() -> Box<dyn PyBridgeProvider>;

/// Provider registry — static factory pattern
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
static PY_BRIDGE_REGISTRY: std::sync::LazyLock<
    std::sync::RwLock<std::collections::HashMap<&'static str, PyBridgeFactory>>,
> = std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));

/// PyBridge provider factory — registry-based dispatch
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct PyBridgeProviderFactory;

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl PyBridgeProviderFactory {
    pub fn register(name: &'static str, factory: PyBridgeFactory) {
        PY_BRIDGE_REGISTRY.write().unwrap().insert(name, factory);
    }

    pub fn create(name: &str) -> Option<Box<dyn PyBridgeProvider>> {
        PY_BRIDGE_REGISTRY.read().unwrap().get(name).map(|f| f())
    }

    pub fn list_providers() -> Vec<&'static str> {
        PY_BRIDGE_REGISTRY.read().unwrap().keys().copied().collect()
    }
}

/// Initialize all py_bridge providers — call at startup
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub fn init_providers() {
    PyBridgeProviderFactory::register("openai", || Box::new(openai::OpenAIProvider::new()));
    PyBridgeProviderFactory::register(
        "anthropic",
        || Box::new(anthropic::AnthropicProvider::new()),
    );
    PyBridgeProviderFactory::register("mistral", || Box::new(mistral::MistralProvider::new()));
    PyBridgeProviderFactory::register("gemini", || Box::new(gemini::GeminiProvider::new()));
    PyBridgeProviderFactory::register("azure", || Box::new(azure::AzureProvider::new()));
    PyBridgeProviderFactory::register("huggingface", || {
        Box::new(huggingface::HuggingFaceProvider::new())
    });
    PyBridgeProviderFactory::register("voyage", || Box::new(voyage::VoyageProvider::new()));
    PyBridgeProviderFactory::register("cohere", || Box::new(cohere::CohereProvider::new()));
    PyBridgeProviderFactory::register("deepseek", || Box::new(deepseek::DeepSeekProvider::new()));
    PyBridgeProviderFactory::register("groq", || Box::new(groq::GroqProvider::new()));
    PyBridgeProviderFactory::register("together", || Box::new(together::TogetherProvider::new()));
    PyBridgeProviderFactory::register("openrouter", || {
        Box::new(openrouter::OpenRouterProvider::new())
    });
    PyBridgeProviderFactory::register(
        "fireworks",
        || Box::new(fireworks::FireworksProvider::new()),
    );
    PyBridgeProviderFactory::register("cerebras", || Box::new(cerebras::CerebrasProvider::new()));
    PyBridgeProviderFactory::register(
        "deepinfra",
        || Box::new(deepinfra::DeepInfraProvider::new()),
    );
    PyBridgeProviderFactory::register("nebius", || Box::new(nebius::NebiusProvider::new()));
    PyBridgeProviderFactory::register("moonshot", || Box::new(moonshot::MoonshotProvider::new()));
    PyBridgeProviderFactory::register("minimax", || Box::new(minimax::MiniMaxProvider::new()));
    PyBridgeProviderFactory::register(
        "dashscope",
        || Box::new(dashscope::DashScopeProvider::new()),
    );
    PyBridgeProviderFactory::register("llamacpp", || Box::new(llamacpp::LlamaCppProvider::new()));
    PyBridgeProviderFactory::register(
        "llamafile",
        || Box::new(llamafile::LlamaFileProvider::new()),
    );
    PyBridgeProviderFactory::register("lmstudio", || Box::new(lmstudio::LMStudioProvider::new()));
    PyBridgeProviderFactory::register("ollama", || Box::new(ollama::OllamaProvider::new()));
    PyBridgeProviderFactory::register("portkey", || Box::new(portkey::PortkeyProvider::new()));
    PyBridgeProviderFactory::register("xai", || Box::new(xai::XaiProvider::new()));
    PyBridgeProviderFactory::register("vertexai", || Box::new(vertexai::VertexAIProvider::new()));
    PyBridgeProviderFactory::register(
        "sambanova",
        || Box::new(sambanova::SambaNovaProvider::new()),
    );
    PyBridgeProviderFactory::register(
        "inception",
        || Box::new(inception::InceptionProvider::new()),
    );
    PyBridgeProviderFactory::register("watsonx", || Box::new(watsonx::WatsonxProvider::new()));
    PyBridgeProviderFactory::register("bedrock", || Box::new(bedrock::BedrockProvider::new()));
    PyBridgeProviderFactory::register(
        "sagemaker",
        || Box::new(sagemaker::SageMakerProvider::new()),
    );
    PyBridgeProviderFactory::register("ai21", || Box::new(ai21::AI21Provider::new()));
    PyBridgeProviderFactory::register(
        "replicate",
        || Box::new(replicate::ReplicateProvider::new()),
    );
    PyBridgeProviderFactory::register("nvidia", || Box::new(nvidia::NvidiaProvider::new()));
    PyBridgeProviderFactory::register("aleph_alpha", || {
        Box::new(aleph_alpha::AlephAlphaProvider::new())
    });
    PyBridgeProviderFactory::register("conjure", || Box::new(conjure::ConjureProvider::new()));
    PyBridgeProviderFactory::register("infere", || Box::new(infere::InfereProvider::new()));
    PyBridgeProviderFactory::register("level_ai", || Box::new(level_ai::LevelAiProvider::new()));
    PyBridgeProviderFactory::register("ai_foundry", || {
        Box::new(ai_foundry::AiFoundryProvider::new())
    });
    PyBridgeProviderFactory::register("mistral_large", || {
        Box::new(mistral_large::MistralLargeProvider::new())
    });
    PyBridgeProviderFactory::register("cloudflareai", || {
        Box::new(cloudflareai::CloudflareProvider::new())
    });
    PyBridgeProviderFactory::register("workersai", || Box::new(workersai::WorkersProvider::new()));
}
