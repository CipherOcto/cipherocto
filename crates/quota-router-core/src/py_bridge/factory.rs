// py_bridge factory — creates and dispatches to providers
//
// Provides a unified interface for calling any Python SDK provider.
// This is the INTERNAL boundary #1 (core → Python SDKs).

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use crate::types::Message;

// Re-export PyBridgeError from openai for consistency across all providers
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub use crate::py_bridge::openai::PyBridgeError;

/// Dispatch completion call to the appropriate provider via registry
///
/// Per RFC-0929 REQUIRED changes:
/// - api_base: Option<&&str> — per-deployment API base URL for custom endpoints
///   Security: api_base is NOT logged — it's forwarded to provider without logging
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub fn completion(
    provider: &str,
    model: &str,
    messages: &[Message],
    api_key: Option<&str>,
    api_base: Option<&str>,
) -> Result<crate::types::ChatCompletion, PyBridgeError> {
    let mut p = crate::py_bridge::PyBridgeProviderFactory::create(provider).ok_or_else(|| {
        PyBridgeError::UnsupportedProvider(format!(
            "Provider '{}' not yet implemented in py_bridge",
            provider
        ))
    })?;
    if let Some(key) = api_key {
        p = p.with_api_key(key.to_string());
    }
    if let Some(base) = api_base {
        p = p.with_api_base(base.to_string());
    }
    p.completion(model, messages)
}

/// Dispatch streaming completion call to the appropriate provider via registry
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
    let mut p = crate::py_bridge::PyBridgeProviderFactory::create(provider).ok_or_else(|| {
        PyBridgeError::UnsupportedProvider(format!(
            "Provider '{}' not yet implemented in py_bridge",
            provider
        ))
    })?;
    if let Some(key) = api_key {
        p = p.with_api_key(key.to_string());
    }
    if let Some(base) = api_base {
        p = p.with_api_base(base.to_string());
    }
    p.streaming_completion(model, messages)
}
