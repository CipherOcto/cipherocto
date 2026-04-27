// Model string parsing per RFC-0917 §B5
// Handles provider:model and provider/model formats

use pyo3::prelude::*;
use pyo3::types::PyTuple;

/// Known providers per RFC-0917 Phase 3
const KNOWN_PROVIDERS: &[&str] = &[
    "anthropic",
    "azure",
    "azureanthropic",
    "azureopenai",
    "bedrock",
    "cerebras",
    "cohere",
    "dashscope",
    "databricks",
    "deepseek",
    "fireworks",
    "gateway",
    "gemini",
    "groq",
    "huggingface",
    "inception",
    "llama",
    "llamacpp",
    "llamafile",
    "lmstudio",
    "minimax",
    "mistral",
    "moonshot",
    "mzai",
    "nebius",
    "ollama",
    "openai",
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
];

/// Default provider when no provider prefix is specified
const DEFAULT_PROVIDER: &str = "openai";

/// Parsed model string result
#[derive(Debug, Clone)]
pub struct ParsedModel {
    pub provider: String,
    pub model: String,
}

impl ParsedModel {
    /// Parse a model string with provider prefix
    ///
    /// Priority (RFC-0917 B5):
    /// 1. If `:` present and text before first `:` is known provider → `provider:model`
    /// 2. If `/` present and text before first `/` is known provider → `provider/model`
    /// 3. Otherwise → `default_provider:model` (warn logged)
    pub fn parse(model_str: &str) -> Result<Self, String> {
        let trimmed = model_str.trim();

        // Priority 1: provider:model format
        if let Some(colon_idx) = trimmed.find(':') {
            let potential_provider = trimmed[..colon_idx].to_lowercase();
            if KNOWN_PROVIDERS.contains(&potential_provider.as_str()) {
                let model = trimmed[colon_idx + 1..].to_string();
                return Ok(Self {
                    provider: potential_provider,
                    model,
                });
            }
        }

        // Priority 2: provider/model format
        if let Some(slash_idx) = trimmed.find('/') {
            let potential_provider = trimmed[..slash_idx].to_lowercase();
            if KNOWN_PROVIDERS.contains(&potential_provider.as_str()) {
                let model = trimmed[slash_idx + 1..].to_string();
                return Ok(Self {
                    provider: potential_provider,
                    model,
                });
            }
        }

        // Priority 3: bare model name → use default provider
        Ok(Self {
            provider: DEFAULT_PROVIDER.to_string(),
            model: trimmed.to_string(),
        })
    }
}

/// Parse a model string and return (provider, model) tuple
///
/// # Arguments
/// * `model_str` - Model string in provider:model, provider/model, or bare format
///
/// # Returns
/// Tuple of (provider, model) strings
///
/// # Example
/// ```
/// let (provider, model) = parse_model("openai:gpt-4o").unwrap();
/// assert_eq!(provider, "openai");
/// assert_eq!(model, "gpt-4o");
/// ```
#[pyfunction]
#[pyo3(name = "parse_model")]
pub fn parse_model(model_str: String) -> PyResult<Py<PyAny>> {
    let parsed = ParsedModel::parse(&model_str).map_err(pyo3::exceptions::PyValueError::new_err)?;

    Python::with_gil(|py| {
        let tuple = PyTuple::new(py, vec![parsed.provider, parsed.model]);
        Ok(tuple.into())
    })
}

/// Parse a model string and validate the provider is known
///
/// # Arguments
/// * `model_str` - Model string in provider:model, provider/model, or bare format
///
/// # Returns
/// Tuple of (provider, model) strings, or raises `UnsupportedProviderError`
///
/// # Errors
/// Returns `UnsupportedProviderError` if provider prefix is not in known providers list
#[pyfunction]
#[pyo3(name = "parse_model_strict")]
pub fn parse_model_strict(model_str: String) -> PyResult<Py<PyAny>> {
    let trimmed = model_str.trim();

    // Check if it's a provider:model format with unknown provider
    if let Some(colon_idx) = trimmed.find(':') {
        let potential_provider = trimmed[..colon_idx].to_lowercase();
        if !KNOWN_PROVIDERS.contains(&potential_provider.as_str()) {
            let err_msg = format!("Unknown provider: {}", potential_provider);
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(err_msg));
        }
    }

    // Check if it's a provider/model format with unknown provider
    if let Some(slash_idx) = trimmed.find('/') {
        let potential_provider = trimmed[..slash_idx].to_lowercase();
        if !KNOWN_PROVIDERS.contains(&potential_provider.as_str()) {
            let err_msg = format!("Unknown provider: {}", potential_provider);
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(err_msg));
        }
    }

    let parsed = ParsedModel::parse(&model_str).map_err(pyo3::exceptions::PyValueError::new_err)?;

    Python::with_gil(|py| {
        let tuple = PyTuple::new(py, vec![parsed.provider, parsed.model]);
        Ok(tuple.into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colon_format() {
        let result = ParsedModel::parse("openai:gpt-4o").unwrap();
        assert_eq!(result.provider, "openai");
        assert_eq!(result.model, "gpt-4o");
    }

    #[test]
    fn test_slash_format() {
        let result = ParsedModel::parse("anthropic/claude-3").unwrap();
        assert_eq!(result.provider, "anthropic");
        assert_eq!(result.model, "claude-3");
    }

    #[test]
    fn test_bare_model() {
        let result = ParsedModel::parse("gpt-4o").unwrap();
        assert_eq!(result.provider, "openai");
        assert_eq!(result.model, "gpt-4o");
    }

    #[test]
    fn test_case_insensitive_provider() {
        let result = ParsedModel::parse("OPENAI:gpt-4o").unwrap();
        assert_eq!(result.provider, "openai");
        assert_eq!(result.model, "gpt-4o");
    }

    #[test]
    fn test_model_with_colon_in_name() {
        // When a model name itself contains colon (like llama3.1:8b)
        // The provider should still parse correctly
        let result = ParsedModel::parse("ollama:llama3.1:8b").unwrap();
        assert_eq!(result.provider, "ollama");
        assert_eq!(result.model, "llama3.1:8b");
    }
}
