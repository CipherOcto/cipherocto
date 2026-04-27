// SDK management functions per RFC-0917 Phase 3
// set_api_key, get_budget_status, get_metrics

use crate::model::ParsedModel;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory API key storage (per-provider)
/// In production, this would use quota-router-core's KeyStorage trait
static API_KEYS: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// In-memory spend tracking
static SPEND_TRACKER: Lazy<Mutex<HashMap<String, SpendRecord>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Spend record for tracking
#[derive(Debug, Clone)]
struct SpendRecord {
    total_spend: f64,
    budget_limit: f64,
    requests: u64,
}

/// API key format validation per RFC-0917 A8
fn validate_api_key_format(provider: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    match provider.to_lowercase().as_str() {
        "openai" => {
            if !key.starts_with("sk-") || key.len() < 48 {
                return Err(
                    "OpenAI API keys must start with 'sk-' and be at least 48 characters"
                        .to_string(),
                );
            }
        }
        "anthropic" => {
            if !key.starts_with("sk-ant-") || key.len() < 48 {
                return Err(
                    "Anthropic API keys must start with 'sk-ant-' and be at least 48 characters"
                        .to_string(),
                );
            }
        }
        "mistral" => {
            if !key.starts_with("mistral-") && !key.contains('-') {
                return Err("Mistral API keys must start with 'mistral-'".to_string());
            }
        }
        "gemini" => {
            if !key.starts_with("AIza") || key.len() < 39 {
                return Err(
                    "Gemini API keys must start with 'AIza' and be at least 39 characters"
                        .to_string(),
                );
            }
        }
        "azure" | "azureopenai" | "azureanthropic" => {
            // Azure keys are typically UUIDs or connection strings
            if key.len() < 32 {
                return Err("Azure API keys must be at least 32 characters".to_string());
            }
        }
        _ => {
            // For unknown providers, just check minimum length
            if key.len() < 20 {
                return Err(format!(
                    "API key for {} must be at least 20 characters",
                    provider
                ));
            }
        }
    }
    Ok(())
}

/// set_api_key - Validates and registers an API key for a provider
///
/// # Arguments
/// * `provider` - Provider name (e.g., "openai", "anthropic")
/// * `api_key` - The API key to store
///
/// # Returns
/// True on success
///
/// # Errors
/// Raises InvalidRequestError if key format is invalid
#[pyfunction]
#[pyo3(name = "set_api_key")]
pub fn set_api_key(provider: String, api_key: String) -> PyResult<Py<PyAny>> {
    // Validate provider is known
    let parsed = ParsedModel::parse(&format!("{}:dummy", provider))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    // Validate key format
    validate_api_key_format(&parsed.provider, &api_key)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    // Store the key
    let mut keys = API_KEYS
        .lock()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Lock error: {}", e)))?;
    keys.insert(parsed.provider.clone(), api_key);

    // Initialize spend record with default budget
    let provider_name = parsed.provider.clone();
    let mut spend = SPEND_TRACKER
        .lock()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Lock error: {}", e)))?;
    spend.entry(parsed.provider).or_insert(SpendRecord {
        total_spend: 0.0,
        budget_limit: 100.0, // Default budget limit
        requests: 0,
    });

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("success", true)?;
        dict.set_item("provider", provider_name)?;
        dict.set_item("message", "API key stored successfully")?;
        Ok(dict.into())
    })
}

/// get_budget_status - Returns current spend vs budget limit for all providers
///
/// # Returns
/// Dict with provider budget information
#[pyfunction]
#[pyo3(name = "get_budget_status")]
pub fn get_budget_status() -> PyResult<Py<PyAny>> {
    let spend = SPEND_TRACKER
        .lock()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Lock error: {}", e)))?;

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("object", "list")?;

        let data_list = PyList::new(py, Vec::<&PyDict>::new());

        for (provider, record) in spend.iter() {
            let provider_dict = PyDict::new(py);
            provider_dict.set_item("provider", provider)?;
            provider_dict.set_item("total_spend", record.total_spend)?;
            provider_dict.set_item("budget_limit", record.budget_limit)?;
            provider_dict.set_item("remaining", record.budget_limit - record.total_spend)?;
            provider_dict.set_item("requests", record.requests)?;
            provider_dict.set_item(
                "percent_used",
                if record.budget_limit > 0.0 {
                    record.total_spend / record.budget_limit * 100.0
                } else {
                    0.0
                },
            )?;
            data_list.append(provider_dict)?;
        }

        dict.set_item("data", data_list)?;
        Ok(dict.into())
    })
}

/// get_metrics - Returns Prometheus metrics as a dict
///
/// # Returns
/// Dict with metric names and values
#[pyfunction]
#[pyo3(name = "get_metrics")]
pub fn get_metrics() -> PyResult<Py<PyAny>> {
    let spend = SPEND_TRACKER
        .lock()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Lock error: {}", e)))?;

    Python::with_gil(|py| {
        let dict = PyDict::new(py);

        // Request counts by provider
        let request_counts = PyDict::new(py);
        for (provider, record) in spend.iter() {
            request_counts.set_item(format!("{}_requests_total", provider), record.requests)?;
        }
        dict.set_item("request_counts", request_counts)?;

        // Spend by provider
        let spend_by_provider = PyDict::new(py);
        for (provider, record) in spend.iter() {
            spend_by_provider.set_item(
                format!("{}_spend_total_dollars", provider),
                record.total_spend,
            )?;
        }
        dict.set_item("spend_by_provider", spend_by_provider)?;

        // Total requests
        let total_requests: u64 = spend.values().map(|r| r.requests).sum();
        dict.set_item("total_requests", total_requests)?;

        // Total spend
        let total_spend: f64 = spend.values().map(|r| r.total_spend).sum();
        dict.set_item("total_spend_dollars", total_spend)?;

        // Active providers count
        dict.set_item("active_providers", spend.len() as u64)?;

        // Quota router SDK version
        dict.set_item("sdk_version", env!("CARGO_PKG_VERSION"))?;

        Ok(dict.into())
    })
}

/// Internal function to record spend (called by completion functions)
#[allow(dead_code)]
pub fn record_spend_internal(provider: &str, amount: f64) -> Result<(), String> {
    let mut spend = SPEND_TRACKER
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    if let Some(record) = spend.get_mut(provider) {
        record.total_spend += amount;
        record.requests += 1;
    } else {
        spend.insert(
            provider.to_string(),
            SpendRecord {
                total_spend: amount,
                budget_limit: 100.0,
                requests: 1,
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_openai_key() {
        // OpenAI keys must be at least 48 characters total
        let result = validate_api_key_format(
            "openai",
            "sk-1234567890abcdefghijklmnopqrstuvwxyz1234567890abcdef",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_anthropic_key() {
        // Anthropic keys must be at least 48 characters total
        let result = validate_api_key_format(
            "anthropic",
            "sk-ant-1234567890abcdefghijklmnopqrstuvwxyz12345678",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_invalid_key() {
        let result = validate_api_key_format("openai", "too-short");
        assert!(result.is_err());
    }

    #[test]
    fn test_record_spend_internal() {
        let result = record_spend_internal("openai", 0.05);
        assert!(result.is_ok());
    }
}
