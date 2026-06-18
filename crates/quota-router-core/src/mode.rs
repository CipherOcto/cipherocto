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
    pub fn parse(s: &str) -> Option<Self> {
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
    // Use cfg!() runtime checks to avoid conflicting #[cfg] blocks when
    // multiple features are active simultaneously (e.g. "full" + default "litellm-mode").
    if cfg!(feature = "any-llm-mode") && !cfg!(feature = "litellm-mode") {
        ProviderMode::AnyLlm
    } else {
        // Default: litellm-mode (also covers full and combined modes)
        ProviderMode::LiteLLM
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
    fn test_mode_parse() {
        assert_eq!(ProviderMode::parse("litellm"), Some(ProviderMode::LiteLLM));
        assert_eq!(ProviderMode::parse("any-llm"), Some(ProviderMode::AnyLlm));
        assert_eq!(ProviderMode::parse("LITELLM"), Some(ProviderMode::LiteLLM));
        assert_eq!(ProviderMode::parse("invalid"), None);
    }
}
