// Provider factory for creating and dispatching to providers
// Handles dynamic provider lookup and instantiation

use crate::providers::base::{ProviderInfo, Providers};
use pyo3::prelude::*;
use pyo3::PyErr;

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
}
