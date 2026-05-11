// SDK management functions per RFC-0917 Phase 3
//
// These delegate to core storage via python_sdk_entry module.
// Per RFC-0917: pyo3 should be thin wrapper only.

use pyo3::prelude::*;
use quota_router_core::python_sdk_entry;

/// set_api_key - Validates and stores an API key for a provider
///
/// Delegates to core storage via quota-router-core's python_sdk_entry module.
#[pyfunction]
#[pyo3(
    name = "set_api_key",
    text_signature = "(provider, api_key, label=None)"
)]
pub fn set_api_key(provider: String, api_key: String, label: Option<String>) -> PyResult<String> {
    // Delegate to core python_sdk_entry set_api_key
    python_sdk_entry::set_api_key(provider, api_key, label)
}

/// get_budget_status - Returns current budget status for all providers
///
/// Delegates to core storage via quota-router-core's python_sdk_entry module.
#[pyfunction]
#[pyo3(name = "get_budget_status", text_signature = "()")]
pub fn get_budget_status() -> PyResult<Py<PyAny>> {
    // Call core function and convert result
    let result = python_sdk_entry::get_budget_status()?;
    // Result is Py<PyDict> from core, return as Py<PyAny>
    Ok(result.into())
}

/// get_metrics - Returns Prometheus-format metrics
///
/// Delegates to core storage via quota-router-core's python_sdk_entry module.
#[pyfunction]
#[pyo3(name = "get_metrics", text_signature = "()")]
pub fn get_metrics() -> PyResult<Py<PyAny>> {
    // Call core function and convert result
    let result = python_sdk_entry::get_metrics()?;
    // Result is Py<PyDict> from core, return as Py<PyAny>
    Ok(result.into())
}
