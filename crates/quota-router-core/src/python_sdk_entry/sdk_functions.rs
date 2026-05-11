// sdk_functions — Core functions for Python SDK entry point (EXTERNAL boundary #2 per RFC-0917)
//
// These functions delegate to core storage (KeyStorage trait) rather than
// maintaining in-memory state in the pyo3 binding layer.
//
// Per RFC-0917 lines 296-297:
// "pub mod python_sdk_entry;  // PyO3 entry point — EXTERNAL boundary #2"

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use crate::storage::{KeyStorage, ProviderKeyInfo};
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::{PyDict, PyList};

/// set_api_key — Register a provider API key with core storage
///
/// # Arguments
/// * `provider` - Provider name (e.g., "openai", "anthropic", "groq")
/// * `api_key` - The API key to store
/// * `label` - Optional label for this key
///
/// # Returns
/// * `Ok(id)` - The unique ID assigned to this key
/// * `Err` - Storage error
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[pyfunction]
#[pyo3(
    name = "set_api_key",
    text_signature = "(provider, api_key, label=None)"
)]
pub fn set_api_key(provider: String, api_key: String, label: Option<String>) -> PyResult<String> {
    use crate::storage::STORAGE;

    STORAGE
        .create_provider_key(&provider, &api_key, label.as_deref())
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Storage error: {}", e)))
}

/// get_budget_status — Get current budget status for all provider keys
///
/// # Returns
/// * Dictionary with provider key info (id, provider, prefix, label, created_at, is_active)
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[pyfunction]
#[pyo3(name = "get_budget_status", text_signature = "()")]
pub fn get_budget_status() -> PyResult<Py<PyDict>> {
    use crate::storage::STORAGE;

    let keys = STORAGE
        .list_provider_keys(None)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Storage error: {}", e)))?;

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        let providers_dict = PyDict::new(py);

        // Group by provider
        let mut provider_groups: std::collections::HashMap<String, Vec<&ProviderKeyInfo>> =
            std::collections::HashMap::new();
        for key in &keys {
            provider_groups
                .entry(key.provider.clone())
                .or_default()
                .push(key);
        }

        // Build provider dict with lists
        for (provider, key_list) in provider_groups {
            let list = PyList::new(py, Vec::<&PyAny>::with_capacity(key_list.len()));
            for k in key_list.iter() {
                let item = PyDict::new(py);
                item.set_item("id", &k.id)?;
                item.set_item("provider", &k.provider)?;
                item.set_item("api_key_prefix", &k.api_key_prefix)?;
                if let Some(ref label) = k.label {
                    item.set_item("label", label)?;
                }
                item.set_item("created_at", k.created_at)?;
                item.set_item("is_active", k.is_active)?;
                list.append(item)?;
            }
            providers_dict.set_item(&provider, list)?;
        }

        dict.set_item("providers", providers_dict)?;
        dict.set_item("total_keys", keys.len())?;
        Ok(dict.into())
    })
}

/// get_metrics — Get Prometheus-format metrics for all provider keys
///
/// # Returns
/// * Dictionary with metrics in Prometheus text format key
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[pyfunction]
#[pyo3(name = "get_metrics", text_signature = "()")]
pub fn get_metrics() -> PyResult<Py<PyDict>> {
    use crate::storage::STORAGE;

    let keys = STORAGE
        .list_provider_keys(None)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Storage error: {}", e)))?;

    Python::with_gil(|py| {
        let dict = PyDict::new(py);

        // Provider key counts
        let mut provider_counts = std::collections::HashMap::new();
        for key in keys {
            *provider_counts.entry(key.provider.clone()).or_insert(0) += 1;
        }

        // Convert to Prometheus format
        let mut metrics_text = String::new();
        metrics_text.push_str(
            "# HELP quota_router_provider_keys_total Total number of provider API keys\n",
        );
        metrics_text.push_str("# TYPE quota_router_provider_keys_total gauge\n");

        for (provider, count) in &provider_counts {
            metrics_text.push_str(&format!(
                "quota_router_provider_keys_total{{provider=\"{}\"}} {}\n",
                provider, count
            ));
        }

        metrics_text.push_str("# HELP quota_router_total_keys Total number of provider API keys\n");
        metrics_text.push_str("# TYPE quota_router_total_keys gauge\n");
        metrics_text.push_str(&format!(
            "quota_router_total_keys {}\n",
            provider_counts.values().sum::<i32>()
        ));

        dict.set_item("text", metrics_text)?;
        dict.set_item("provider_count", provider_counts.len() as i32)?;
        dict.set_item("total_keys", provider_counts.values().sum::<i32>())?;
        Ok(dict.into())
    })
}
