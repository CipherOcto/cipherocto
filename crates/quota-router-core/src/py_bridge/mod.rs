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
