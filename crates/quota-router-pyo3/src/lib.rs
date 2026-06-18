// quota-router-pyo3 - Python bindings for quota-router
// Enables drop-in replacement for LiteLLM
//
// ⚠️ CRITICAL INVARIANT (RFC-0917):
// This Python SDK EXISTS in ALL modes (litellm-mode, any-llm-mode, full).
// Mode gate controls PROVIDER STRATEGY (reqwest vs PyO3), NOT interface availability.
// BOTH HTTP proxy AND Python SDK exist in ALL modes:
//   - litellm-mode:  reqwest → provider REST APIs.    HTTP proxy ✅  Python SDK ✅
//   - any-llm-mode:  PyO3   → official Python SDKs.  HTTP proxy ✅  Python SDK ✅
//   - full:          Both reqwest AND PyO3.          HTTP proxy ✅  Python SDK ✅
//
// NEVER think "litellm-mode = proxy only" or "any-llm-mode = SDK only".
// See RFC-0917 lines 175-176: "HTTP Proxy Server | (always)" and "Python SDK Interface | (always)"

#![allow(deprecated)]

mod batch;
mod completion;
mod exceptions;
mod model;
mod providers;
mod router;
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
fn quota_router_native(m: &PyModule) -> PyResult<()> {
    // Initialize Tokio runtime and permanently enter its context on this thread.
    // This ensures the reactor is available when async functions (litellm-mode)
    // use reqwest for HTTP calls. Without this, asyncio.run() has no Tokio context.
    // mem::forget prevents the guard from calling exit() on drop.
    #[cfg(feature = "full")]
    {
        let guard = pyo3_asyncio_0_21::tokio::get_runtime().enter();
        std::mem::forget(guard);
    }

    // Register exception classes
    exceptions::register_exceptions(m)?;

    // Add version
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // LiteLLM-compatible module-level settings
    let py = m.py();
    m.add("drop_params", false)?;
    m.add("set_verbose", false)?;
    m.add("api_key", py.None())?;
    m.add("api_base", py.None())?;
    m.add("num_retries", 3)?;
    m.add("request_timeout", 30)?;
    m.add("cache", false)?;

    // Register sync completion functions
    m.add_function(wrap_pyfunction!(completion::completion, m)?)?;
    m.add_function(wrap_pyfunction!(completion::text_completion, m)?)?;
    m.add_function(wrap_pyfunction!(completion::embedding, m)?)?;
    m.add_function(wrap_pyfunction!(completion::messages, m)?)?;
    m.add_function(wrap_pyfunction!(completion::responses, m)?)?;
    m.add_function(wrap_pyfunction!(completion::list_models, m)?)?;
    m.add_function(wrap_pyfunction!(completion::batch_create, m)?)?;
    m.add_function(wrap_pyfunction!(completion::batch_retrieve, m)?)?;
    m.add_function(wrap_pyfunction!(completion::batch_cancel, m)?)?;
    m.add_function(wrap_pyfunction!(completion::batch_list, m)?)?;
    m.add_function(wrap_pyfunction!(completion::batch_results, m)?)?;
    m.add_function(wrap_pyfunction!(completion::get_response, m)?)?;
    m.add_function(wrap_pyfunction!(completion::delete_response, m)?)?;

    // Register async completion functions (using pyo3 experimental-async)
    m.add_function(wrap_pyfunction!(completion::acompletion, m)?)?;
    m.add_function(wrap_pyfunction!(completion::atext_completion, m)?)?;
    m.add_function(wrap_pyfunction!(completion::aembedding, m)?)?;
    m.add_function(wrap_pyfunction!(completion::amessages, m)?)?;
    m.add_function(wrap_pyfunction!(completion::aresponses, m)?)?;
    m.add_function(wrap_pyfunction!(completion::alist_models, m)?)?;
    m.add_function(wrap_pyfunction!(completion::abatch_create, m)?)?;
    m.add_function(wrap_pyfunction!(completion::abatch_retrieve, m)?)?;
    m.add_function(wrap_pyfunction!(completion::abatch_cancel, m)?)?;
    m.add_function(wrap_pyfunction!(completion::abatch_list, m)?)?;
    m.add_function(wrap_pyfunction!(completion::abatch_results, m)?)?;
    m.add_function(wrap_pyfunction!(completion::aget_response, m)?)?;
    m.add_function(wrap_pyfunction!(completion::adelete_response, m)?)?;

    // Register model parsing functions
    m.add_function(wrap_pyfunction!(model::parse_model, m)?)?;
    m.add_function(wrap_pyfunction!(model::parse_model_strict, m)?)?;

    // Register SDK management functions (delegate to core python_sdk_entry)
    // Note: These use in-memory storage in pyo3 for now; core storage will be wired in Phase 4
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

    // Register batch completion functions
    m.add_function(wrap_pyfunction!(batch::batch_completion, m)?)?;
    m.add_function(wrap_pyfunction!(batch::batch_completion_models, m)?)?;
    m.add_function(wrap_pyfunction!(
        batch::batch_completion_models_all_responses,
        m
    )?)?;

    // Register Router class
    m.add_class::<router::Router>()?;

    Ok(())
}
