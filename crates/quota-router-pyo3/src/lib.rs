// quota-router-pyo3 - Python bindings for quota-router
// Enables drop-in replacement for LiteLLM

#![allow(deprecated)]

mod completion;
mod exceptions;
mod model;
mod providers;
mod sdk;
mod streaming;
mod types;

use pyo3::prelude::*;

/// Quota Router Python SDK
///
/// This module provides Python bindings for the Rust quota-router,
/// enabling drop-in replacement for LiteLLM users.
///
/// Example:
/// ```python
/// import quota_router as litellm
///
/// response = litellm.completion(
///     model="gpt-4",
///     messages=[{"role": "user", "content": "Hello!"}]
/// )
/// print(response["choices"][0]["message"]["content"])
/// ```
#[pymodule]
fn quota_router(m: &PyModule) -> PyResult<()> {
    // Register exception classes
    exceptions::register_exceptions(m)?;

    // Add version
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Register sync completion functions
    m.add_function(wrap_pyfunction!(completion::completion, m)?)?;
    m.add_function(wrap_pyfunction!(completion::embedding, m)?)?;
    m.add_function(wrap_pyfunction!(completion::messages, m)?)?;
    m.add_function(wrap_pyfunction!(completion::responses, m)?)?;
    m.add_function(wrap_pyfunction!(completion::list_models, m)?)?;
    m.add_function(wrap_pyfunction!(completion::create_batch, m)?)?;
    m.add_function(wrap_pyfunction!(completion::retrieve_batch, m)?)?;
    m.add_function(wrap_pyfunction!(completion::cancel_batch, m)?)?;
    m.add_function(wrap_pyfunction!(completion::list_batches, m)?)?;
    m.add_function(wrap_pyfunction!(completion::retrieve_batch_results, m)?)?;

    // Register async completion functions (using pyo3 experimental-async)
    m.add_function(wrap_pyfunction!(completion::acompletion, m)?)?;
    m.add_function(wrap_pyfunction!(completion::aembedding, m)?)?;
    m.add_function(wrap_pyfunction!(completion::amessages, m)?)?;
    m.add_function(wrap_pyfunction!(completion::aresponses, m)?)?;
    m.add_function(wrap_pyfunction!(completion::alist_models, m)?)?;
    m.add_function(wrap_pyfunction!(completion::acreate_batch, m)?)?;
    m.add_function(wrap_pyfunction!(completion::aretrieve_batch, m)?)?;
    m.add_function(wrap_pyfunction!(completion::acancel_batch, m)?)?;
    m.add_function(wrap_pyfunction!(completion::alist_batches, m)?)?;
    m.add_function(wrap_pyfunction!(completion::aretrieve_batch_results, m)?)?;

    // Register model parsing functions
    m.add_function(wrap_pyfunction!(model::parse_model, m)?)?;
    m.add_function(wrap_pyfunction!(model::parse_model_strict, m)?)?;

    // Register SDK management functions
    m.add_function(wrap_pyfunction!(sdk::set_api_key, m)?)?;
    m.add_function(wrap_pyfunction!(sdk::get_budget_status, m)?)?;
    m.add_function(wrap_pyfunction!(sdk::get_metrics, m)?)?;

    // Register provider functions
    m.add_function(wrap_pyfunction!(
        providers::factory::get_supported_providers,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(
        providers::factory::is_provider_supported,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(providers::factory::get_provider_info, m)?)?;

    Ok(())
}
