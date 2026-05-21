//! Unified mode routing for provider calls.
//!
//! Per RFC-0917, the mode gate controls HOW providers are called:
//! - `litellm` mode: reqwest (native HTTP) — direct REST API calls
//! - `any-llm` mode: PyO3 (Python SDK delegation) — calls official Python SDKs
//!
//! This module provides the mode enum and default selection logic.
//! The actual provider calling is done by the caller (Python SDK or proxy)
//! using the appropriate factory for the selected mode.

/// Provider call mode — selects which backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderMode {
    /// litellm mode: reqwest → provider REST APIs
    LiteLLM,
    /// any-llm mode: PyO3 → official Python SDKs
    AnyLlm,
}

impl ProviderMode {
    /// Parse from string: "litellm" or "any-llm"
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "litellm" | "litellm-mode" | "litellm_mode" => Some(ProviderMode::LiteLLM),
            "any-llm" | "any-llm-mode" | "any_llm" | "any_llm_mode" => Some(ProviderMode::AnyLlm),
            _ => None,
        }
    }

    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderMode::LiteLLM => "litellm",
            ProviderMode::AnyLlm => "any-llm",
        }
    }
}

/// Get the default mode based on compiled features.
///
/// When both features are compiled (full mode), defaults to LiteLLM (reqwest)
/// because it's faster and has no Python dependency.
pub fn default_mode() -> ProviderMode {
    #[cfg(all(feature = "litellm-mode", not(feature = "any-llm-mode")))]
    {
        return ProviderMode::LiteLLM;
    }

    #[cfg(all(feature = "any-llm-mode", not(feature = "litellm-mode")))]
    {
        return ProviderMode::AnyLlm;
    }

    #[cfg(feature = "full")]
    {
        return ProviderMode::LiteLLM;
    }

    #[cfg(not(any(feature = "litellm-mode", feature = "any-llm-mode", feature = "full")))]
    {
        compile_error!("At least one of 'litellm-mode', 'any-llm-mode', or 'full' must be enabled")
    }
}

/// Check if litellm-mode is available.
pub fn has_litellm_mode() -> bool {
    cfg!(any(feature = "litellm-mode", feature = "full"))
}

/// Check if any-llm-mode is available.
pub fn has_any_llm_mode() -> bool {
    cfg!(any(feature = "any-llm-mode", feature = "full"))
}

/// Parse "provider/model" or "provider:model" into (provider, model).
/// If no separator, returns ("openai", model_str) as default.
pub fn parse_model_provider(model_str: &str) -> (&str, &str) {
    if let Some(pos) = model_str.find('/') {
        (&model_str[..pos], &model_str[pos + 1..])
    } else if let Some(pos) = model_str.find(':') {
        (&model_str[..pos], &model_str[pos + 1..])
    } else {
        ("openai", model_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_model_provider_slash() {
        let (provider, model) = parse_model_provider("openai/gpt-4");
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4");
    }

    #[test]
    fn test_parse_model_provider_colon() {
        let (provider, model) = parse_model_provider("anthropic:claude-3");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-3");
    }

    #[test]
    fn test_parse_model_provider_no_prefix() {
        let (provider, model) = parse_model_provider("gpt-4");
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4");
    }

    #[test]
    fn test_default_mode() {
        let mode = default_mode();
        assert_eq!(mode, ProviderMode::LiteLLM);
    }

    #[test]
    fn test_mode_from_str() {
        assert_eq!(
            ProviderMode::from_str("litellm"),
            Some(ProviderMode::LiteLLM)
        );
        assert_eq!(
            ProviderMode::from_str("any-llm"),
            Some(ProviderMode::AnyLlm)
        );
        assert_eq!(
            ProviderMode::from_str("LITELLM"),
            Some(ProviderMode::LiteLLM)
        );
        assert_eq!(ProviderMode::from_str("invalid"), None);
    }
}
