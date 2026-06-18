// QuotaRouter exceptions for PyO3 bindings
// Exception hierarchy per RFC-0917 Phase 3 and RFC-0920
//
// Uses create_exception! macro so exceptions are proper Python exceptions
// catchable with `except SomeError` in Python.

use pyo3::create_exception;
use pyo3::prelude::*;

// =============================================================================
// Base Exception (QuotaRouterError) — per RFC-0917 §Exception Mapping and RFC-0920
// =============================================================================

create_exception!(
    quota_router_native,
    QuotaRouterError,
    pyo3::exceptions::PyException,
    "Base exception"
);

// =============================================================================
// AuthenticationError — 401
// =============================================================================

create_exception!(
    quota_router_native,
    AuthenticationError,
    QuotaRouterError,
    "Auth failed"
);

// =============================================================================
// RateLimitError — 429
// =============================================================================

create_exception!(
    quota_router_native,
    RateLimitError,
    QuotaRouterError,
    "Rate limited"
);

// =============================================================================
// InvalidRequestError — 400
// =============================================================================

create_exception!(
    quota_router_native,
    InvalidRequestError,
    QuotaRouterError,
    "Invalid request"
);

// =============================================================================
// ProviderError — 500
// =============================================================================

create_exception!(
    quota_router_native,
    ProviderError,
    QuotaRouterError,
    "Provider error"
);

// =============================================================================
// ContentFilterError
// =============================================================================

create_exception!(
    quota_router_native,
    ContentFilterError,
    QuotaRouterError,
    "Content filter"
);

// =============================================================================
// ModelNotFoundError — 404
// =============================================================================

create_exception!(
    quota_router_native,
    ModelNotFoundError,
    QuotaRouterError,
    "Model not found"
);

// =============================================================================
// ContextLengthExceededError
// =============================================================================

create_exception!(
    quota_router_native,
    ContextLengthExceededError,
    QuotaRouterError,
    "Context exceeded"
);

// =============================================================================
// MissingApiKeyError
// =============================================================================

create_exception!(
    quota_router_native,
    MissingApiKeyError,
    QuotaRouterError,
    "Missing API key"
);

// =============================================================================
// UnsupportedProviderError
// =============================================================================

create_exception!(
    quota_router_native,
    UnsupportedProviderError,
    QuotaRouterError,
    "Unsupported provider"
);

// =============================================================================
// UnsupportedParameterError
// =============================================================================

create_exception!(
    quota_router_native,
    UnsupportedParameterError,
    QuotaRouterError,
    "Unsupported parameter"
);

// =============================================================================
// InsufficientFundsError (BudgetExceededError in LiteLLM)
// =============================================================================

create_exception!(
    quota_router_native,
    InsufficientFundsError,
    QuotaRouterError,
    "Budget exceeded"
);

// =============================================================================
// UpstreamProviderError (ServiceUnavailableError in LiteLLM)
// =============================================================================

create_exception!(
    quota_router_native,
    UpstreamProviderError,
    QuotaRouterError,
    "Upstream error"
);

// =============================================================================
// GatewayTimeoutError (Timeout/APIConnectionError in LiteLLM)
// =============================================================================

create_exception!(
    quota_router_native,
    GatewayTimeoutError,
    QuotaRouterError,
    "Gateway timeout"
);

// =============================================================================
// LengthFinishReasonError
// =============================================================================

create_exception!(
    quota_router_native,
    LengthFinishReasonError,
    QuotaRouterError,
    "Length finish"
);

// =============================================================================
// ContentFilterFinishReasonError
// =============================================================================

create_exception!(
    quota_router_native,
    ContentFilterFinishReasonError,
    QuotaRouterError,
    "Content filter finish"
);

// =============================================================================
// BatchNotCompleteError
// =============================================================================

create_exception!(
    quota_router_native,
    BatchNotCompleteError,
    QuotaRouterError,
    "Batch not complete"
);

// =============================================================================
// AllModelsFailedError
// =============================================================================

create_exception!(
    quota_router_native,
    AllModelsFailedError,
    QuotaRouterError,
    "All models failed"
);

// =============================================================================
// BatchPartialFailureError
// =============================================================================

create_exception!(
    quota_router_native,
    BatchPartialFailureError,
    QuotaRouterError,
    "Batch partial failure"
);

// =============================================================================
// Registration
// =============================================================================

/// Register all exceptions on the module
pub fn register_exceptions(m: &PyModule) -> PyResult<()> {
    let py = m.py();
    m.add("QuotaRouterError", py.get_type::<QuotaRouterError>())?;

    // Alias for any-llm compatibility
    m.add("AnyLLMError", py.get_type::<QuotaRouterError>())?;

    m.add("AuthenticationError", py.get_type::<AuthenticationError>())?;
    m.add("RateLimitError", py.get_type::<RateLimitError>())?;
    m.add("InvalidRequestError", py.get_type::<InvalidRequestError>())?;
    m.add("ProviderError", py.get_type::<ProviderError>())?;
    m.add("ContentFilterError", py.get_type::<ContentFilterError>())?;
    m.add("ModelNotFoundError", py.get_type::<ModelNotFoundError>())?;
    m.add(
        "ContextLengthExceededError",
        py.get_type::<ContextLengthExceededError>(),
    )?;
    m.add("MissingApiKeyError", py.get_type::<MissingApiKeyError>())?;
    m.add(
        "UnsupportedProviderError",
        py.get_type::<UnsupportedProviderError>(),
    )?;
    m.add(
        "UnsupportedParameterError",
        py.get_type::<UnsupportedParameterError>(),
    )?;
    m.add(
        "InsufficientFundsError",
        py.get_type::<InsufficientFundsError>(),
    )?;
    m.add(
        "UpstreamProviderError",
        py.get_type::<UpstreamProviderError>(),
    )?;
    m.add("GatewayTimeoutError", py.get_type::<GatewayTimeoutError>())?;
    m.add(
        "LengthFinishReasonError",
        py.get_type::<LengthFinishReasonError>(),
    )?;
    m.add(
        "ContentFilterFinishReasonError",
        py.get_type::<ContentFilterFinishReasonError>(),
    )?;
    m.add(
        "BatchNotCompleteError",
        py.get_type::<BatchNotCompleteError>(),
    )?;
    m.add(
        "AllModelsFailedError",
        py.get_type::<AllModelsFailedError>(),
    )?;
    m.add(
        "BatchPartialFailureError",
        py.get_type::<BatchPartialFailureError>(),
    )?;

    // LiteLLM-compatible aliases
    m.add(
        "BudgetExceededError",
        py.get_type::<InsufficientFundsError>(),
    )?;
    m.add(
        "ServiceUnavailableError",
        py.get_type::<UpstreamProviderError>(),
    )?;
    m.add("APIConnectionError", py.get_type::<GatewayTimeoutError>())?;
    m.add("APIError", py.get_type::<QuotaRouterError>())?;
    m.add("NotFoundError", py.get_type::<ModelNotFoundError>())?;
    m.add(
        "ContextWindowExceededError",
        py.get_type::<ContextLengthExceededError>(),
    )?;
    m.add(
        "ContentPolicyViolationError",
        py.get_type::<ContentFilterError>(),
    )?;
    m.add("Timeout", py.get_type::<GatewayTimeoutError>())?;

    Ok(())
}
