// Providers module for quota-router-pyo3
// Per RFC-0917 Phase 3: any-llm-mode replaces any-llm SDK
//
// ⚠️ CRITICAL INVARIANT (RFC-0917):
// Mode gate controls PROVIDER STRATEGY (reqwest vs PyO3), NOT interface availability.
// BOTH HTTP proxy AND Python SDK exist in ALL modes.

#[allow(dead_code)]
pub mod base;
pub mod factory;

// Provider implementations
// All marked #[allow(dead_code)] because they are scaffolded providers
// that will be implemented in phases (Phase 1 = OpenAI, Anthropic first)
#[allow(dead_code)]
pub mod anthropic;
#[allow(dead_code)]
pub mod azure;
#[allow(dead_code)]
pub mod azureanthropic;
#[allow(dead_code)]
pub mod azureopenai;
#[allow(dead_code)]
pub mod bedrock;
#[allow(dead_code)]
pub mod cerebras;
#[allow(dead_code)]
pub mod cohere;
#[allow(dead_code)]
pub mod dashscope;
#[allow(dead_code)]
pub mod databricks;
#[allow(dead_code)]
pub mod deepseek;
#[allow(dead_code)]
pub mod fireworks;
#[allow(dead_code)]
pub mod gateway;
#[allow(dead_code)]
pub mod gemini;
#[allow(dead_code)]
pub mod groq;
#[allow(dead_code)]
pub mod huggingface;
#[allow(dead_code)]
pub mod inception;
#[allow(dead_code)]
pub mod llama;
#[allow(dead_code)]
pub mod llamacpp;
#[allow(dead_code)]
pub mod llamafile;
#[allow(dead_code)]
pub mod lmstudio;
#[allow(dead_code)]
pub mod minimax;
#[allow(dead_code)]
pub mod mistral;
#[allow(dead_code)]
pub mod moonshot;
#[allow(dead_code)]
pub mod mzai;
#[allow(dead_code)]
pub mod nebius;
#[allow(dead_code)]
pub mod ollama;
#[allow(dead_code)]
pub mod openai;
#[allow(dead_code)]
pub mod openrouter;
#[allow(dead_code)]
pub mod perplexity;
#[allow(dead_code)]
pub mod platform;
#[allow(dead_code)]
pub mod portkey;
#[allow(dead_code)]
pub mod sagemaker;
#[allow(dead_code)]
pub mod sambanova;
#[allow(dead_code)]
pub mod together;
#[allow(dead_code)]
pub mod vertexai;
#[allow(dead_code)]
pub mod vertexaianthropic;
#[allow(dead_code)]
pub mod vllm;
#[allow(dead_code)]
pub mod voyage;
#[allow(dead_code)]
pub mod watsonx;
#[allow(dead_code)]
pub mod xai;
#[allow(dead_code)]
pub mod zai;
