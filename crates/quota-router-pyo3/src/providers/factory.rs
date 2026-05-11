// Provider factory for creating and dispatching to providers
// Handles dynamic provider lookup and instantiation

use crate::exceptions::{ProviderError, UnsupportedProviderError};
use crate::providers::base::{ProviderInfo, Providers};
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// Global provider registry - stores initialized provider instances
#[allow(dead_code)]
static PROVIDER_REGISTRY: Lazy<Mutex<HashMap<String, ProviderInstance>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Provider instance wrapper
#[allow(dead_code)]
struct ProviderInstance {
    api_key: String,
    api_base: Option<String>,
}

/// Get the list of supported provider names
#[pyfunction]
#[pyo3(name = "get_supported_providers")]
pub fn get_supported_providers() -> Vec<String> {
    Providers::list_names()
        .into_iter()
        .map(String::from)
        .collect()
}

/// Check if a provider is supported
#[pyfunction]
#[pyo3(name = "is_provider_supported")]
pub fn is_provider_supported(provider: &str) -> bool {
    Providers::get(provider).is_some()
}

/// Get provider info as a dict
#[pyfunction]
#[pyo3(name = "get_provider_info")]
pub fn get_provider_info(provider: &str) -> PyResult<Py<PyAny>> {
    let info = Providers::get(provider).ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Unknown provider: {}", provider))
    })?;

    Python::with_gil(|py| {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("name", info.name)?;
        dict.set_item("documentation_url", info.doc_url)?;
        dict.set_item("env_api_key", info.env_api_key)?;
        dict.set_item("env_api_base", info.env_api_base.unwrap_or(""))?;
        dict.set_item("api_base", info.api_base.unwrap_or(""))?;

        let features = pyo3::types::PyDict::new(py);
        features.set_item("supports_completion", info.features.supports_completion)?;
        features.set_item(
            "supports_completion_streaming",
            info.features.supports_completion_streaming,
        )?;
        features.set_item("supports_embedding", info.features.supports_embedding)?;
        features.set_item("supports_responses", info.features.supports_responses)?;
        features.set_item("supports_list_models", info.features.supports_list_models)?;
        features.set_item("supports_batch", info.features.supports_batch)?;
        features.set_item("supports_messages", info.features.supports_messages)?;
        dict.set_item("features", features)?;

        Ok(dict.into())
    })
}

/// Validate that a provider is supported
#[allow(dead_code)]
pub fn validate_provider(
    provider: &str,
) -> Result<&'static ProviderInfo, UnsupportedProviderError> {
    Providers::get(provider).ok_or_else(|| {
        UnsupportedProviderError::new(format!("Unknown provider: {}", provider), provider, vec![])
    })
}

/// Resolve API key for a provider
/// Priority: explicit key > environment variable
#[allow(dead_code)]
pub fn resolve_api_key(
    provider_info: &ProviderInfo,
    explicit_key: Option<&str>,
) -> Result<String, ProviderError> {
    let key = if let Some(k) = explicit_key {
        k.to_string()
    } else if let Ok(env_val) = std::env::var(provider_info.env_api_key) {
        env_val
    } else {
        return Err(ProviderError::new(
            format!(
                "Missing API key for {}. Set {} environment variable or pass api_key parameter.",
                provider_info.name, provider_info.env_api_key
            ),
            provider_info.name,
        ));
    };

    if key.is_empty() {
        return Err(ProviderError::new(
            format!("API key for {} is empty", provider_info.name),
            provider_info.name,
        ));
    }

    Ok(key)
}

/// Resolve API base URL for a provider
#[allow(dead_code)]
pub fn resolve_api_base(
    provider_info: &ProviderInfo,
    explicit_base: Option<&str>,
) -> Option<String> {
    if let Some(base) = explicit_base {
        if !base.is_empty() {
            return Some(base.to_string());
        }
    }
    if let Some(env_var) = provider_info.env_api_base {
        if let Ok(env_val) = std::env::var(env_var) {
            if !env_val.is_empty() {
                return Some(env_val);
            }
        }
    }
    provider_info.api_base.map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_supported_providers() {
        let providers = Providers::list_names();
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"anthropic"));
        assert!(providers.len() >= 41);
    }

    #[test]
    fn test_provider_info() {
        let info = Providers::get("openai").unwrap();
        assert_eq!(info.name, "openai");
        assert_eq!(info.env_api_key, "OPENAI_API_KEY");
    }

    #[test]
    fn test_validate_provider() {
        assert!(validate_provider("openai").is_ok());
        assert!(validate_provider("unknown").is_err());
    }
}
