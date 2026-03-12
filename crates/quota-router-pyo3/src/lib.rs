// quota-router-pyo3 - Python bindings for quota-router
// Enables drop-in replacement for LiteLLM

mod completion;
mod exceptions;
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
fn quota_router(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    // Register exception classes
    exceptions::register_exceptions(m)?;

    // Add version
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Register completion functions
    m.add_function(wrap_pyfunction!(completion::completion, m)?)?;
    m.add_function(wrap_pyfunction!(completion::embedding, m)?)?;

    Ok(())
}
