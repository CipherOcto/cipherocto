// Providers module for quota-router-pyo3
// Per RFC-0917 Phase 3: any-llm-mode replaces any-llm SDK
//
// ⚠️ CRITICAL INVARIANT (RFC-0917):
// Mode gate controls PROVIDER STRATEGY (reqwest vs PyO3), NOT interface availability.
// BOTH HTTP proxy AND Python SDK exist in ALL modes.

pub mod base;
pub mod factory;

// Provider implementations (scaffolded — used only for provider metadata via factory.rs)
pub mod anthropic;
pub mod azure;
pub mod azureanthropic;
pub mod azureopenai;
pub mod bedrock;
pub mod cerebras;
pub mod cohere;
pub mod dashscope;
pub mod databricks;
pub mod deepinfra;
pub mod deepseek;
pub mod fireworks;
pub mod gateway;
pub mod gemini;
pub mod groq;
pub mod huggingface;
pub mod inception;
pub mod llama;
pub mod llamacpp;
pub mod llamafile;
pub mod lmstudio;
pub mod minimax;
pub mod mistral;
pub mod moonshot;
pub mod mzai;
pub mod nebius;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod perplexity;
pub mod platform;
pub mod portkey;
pub mod sagemaker;
pub mod sambanova;
pub mod together;
pub mod vertexai;
pub mod vertexaianthropic;
pub mod vllm;
pub mod voyage;
pub mod watsonx;
pub mod xai;
pub mod zai;
