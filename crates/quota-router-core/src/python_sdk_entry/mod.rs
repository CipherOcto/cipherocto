// python_sdk_entry — PyO3 entry point (EXTERNAL boundary #2 per RFC-0917)
//
// This module is the EXTERNAL boundary between pyo3 bindings and Rust core.
// It is called by quota-router-pyo3 (Python SDK).
//
// Per RFC-0917 lines 296-297:
// "pub mod python_sdk_entry;  // PyO3 entry point — EXTERNAL boundary #2"
//
// This module provides the entry point that pyo3 calls.
// Heavy lifting (provider dispatch, routing, state management) stays in core.

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod completion;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod sdk_functions;

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub use sdk_functions::{get_budget_status, get_metrics, set_api_key};
