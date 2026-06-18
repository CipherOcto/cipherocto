// completion — Python SDK entry point (EXTERNAL boundary #2 per RFC-0917)
//
// This module provides #[pyfunction] decorated functions that are called by pyo3.
// Heavy lifting (provider dispatch, routing) stays in core py_bridge or router modules.
//
// Per RFC-0917 lines 296-297:
// "pub mod python_sdk_entry;  // PyO3 entry point — EXTERNAL boundary #2"

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use crate::types::Message;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;

/// completion — Sync completion call via Python SDK
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[pyfunction]
#[pyo3(name = "completion", text_signature = "(model, messages, **kwargs)")]
#[allow(clippy::too_many_arguments)]
pub fn completion(
    model: String,
    messages: Vec<Message>,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _n: Option<i32>,
    _stream: Option<bool>,
    _stop: Option<String>,
    _presence_penalty: Option<f64>,
    _frequency_penalty: Option<f64>,
    _user: Option<String>,
    _seed: Option<i32>,
    _timeout: Option<f64>,
    _extra_headers: Option<String>,
    _base_url: Option<String>,
    _api_version: Option<String>,
    api_key: Option<String>,
    _service_tier: Option<String>,
    _background: Option<bool>,
    _prompt_cache_key: Option<String>,
    _prompt_cache_retention: Option<String>,
    _conversation: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Parse model string to determine provider
    let parsed = crate::model::ParsedModel::parse(&model)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    // Dispatch to appropriate py_bridge provider via factory
    match crate::py_bridge::factory::completion(
        &parsed.provider,
        &parsed.model,
        &messages,
        api_key.as_deref(),
        _base_url.as_deref(),
    ) {
        Ok(response) => Python::with_gil(|py| response.to_dict(py)),
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "{} error: {}",
            parsed.provider, e
        ))),
    }
}
