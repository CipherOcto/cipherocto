// QuotaRouter exceptions for PyO3 bindings
// Exception hierarchy per RFC-0917 Phase 3 and RFC-0920

#![allow(dead_code)]

use pyo3::prelude::*;

// =============================================================================
// Base Exception (QuotaRouterError) — per RFC-0917 §Exception Mapping and RFC-0920
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct QuotaRouterError {
    message: String,
    llm_provider: Option<String>,
}

#[pymethods]
impl QuotaRouterError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("QuotaRouterError({})", self.message)
    }
}

impl QuotaRouterError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: None,
        }
    }

    pub fn with_provider(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: Some(provider.into()),
        }
    }
}

// =============================================================================
// AuthenticationError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct AuthenticationError {
    message: String,
    llm_provider: Option<String>,
}

#[pymethods]
impl AuthenticationError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("AuthenticationError({})", self.message)
    }
}

impl AuthenticationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: None,
        }
    }

    pub fn with_provider(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: Some(provider.into()),
        }
    }
}

// =============================================================================
// RateLimitError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct RateLimitError {
    message: String,
    llm_provider: Option<String>,
}

#[pymethods]
impl RateLimitError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("RateLimitError({})", self.message)
    }
}

impl RateLimitError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: None,
        }
    }

    pub fn with_provider(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: Some(provider.into()),
        }
    }
}

// =============================================================================
// InvalidRequestError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct InvalidRequestError {
    message: String,
    llm_provider: Option<String>,
}

#[pymethods]
impl InvalidRequestError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("InvalidRequestError({})", self.message)
    }
}

impl InvalidRequestError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: None,
        }
    }
}

// =============================================================================
// ProviderError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct ProviderError {
    message: String,
    llm_provider: String,
}

#[pymethods]
impl ProviderError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ProviderError({})", self.message)
    }
}

impl ProviderError {
    pub fn new(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: provider.into(),
        }
    }
}

// =============================================================================
// ContentFilterError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct ContentFilterError {
    message: String,
    llm_provider: Option<String>,
}

#[pymethods]
impl ContentFilterError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ContentFilterError({})", self.message)
    }
}

impl ContentFilterError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: None,
        }
    }
}

// =============================================================================
// ModelNotFoundError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct ModelNotFoundError {
    message: String,
    model: String,
}

#[pymethods]
impl ModelNotFoundError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ModelNotFoundError({})", self.message)
    }

    #[getter]
    fn get_model(&self) -> String {
        self.model.clone()
    }
}

impl ModelNotFoundError {
    pub fn new(message: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            model: model.into(),
        }
    }
}

// =============================================================================
// ContextLengthExceededError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct ContextLengthExceededError {
    message: String,
    model: String,
    max_tokens: Option<u32>,
}

#[pymethods]
impl ContextLengthExceededError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ContextLengthExceededError({})", self.message)
    }

    #[getter]
    fn get_model(&self) -> String {
        self.model.clone()
    }

    #[getter]
    fn get_max_tokens(&self) -> Option<u32> {
        self.max_tokens
    }
}

impl ContextLengthExceededError {
    pub fn new(
        message: impl Into<String>,
        model: impl Into<String>,
        max_tokens: Option<u32>,
    ) -> Self {
        Self {
            message: message.into(),
            model: model.into(),
            max_tokens,
        }
    }
}

// =============================================================================
// MissingApiKeyError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct MissingApiKeyError {
    message: String,
    provider: String,
}

#[pymethods]
impl MissingApiKeyError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("MissingApiKeyError({})", self.message)
    }

    #[getter]
    fn get_provider(&self) -> String {
        self.provider.clone()
    }
}

impl MissingApiKeyError {
    pub fn new(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            provider: provider.into(),
        }
    }
}

// =============================================================================
// UnsupportedProviderError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct UnsupportedProviderError {
    message: String,
    provider: String,
}

#[pymethods]
impl UnsupportedProviderError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("UnsupportedProviderError({})", self.message)
    }

    #[getter]
    fn get_provider(&self) -> String {
        self.provider.clone()
    }
}

impl UnsupportedProviderError {
    pub fn new(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            provider: provider.into(),
        }
    }
}

// =============================================================================
// UnsupportedParameterError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct UnsupportedParameterError {
    message: String,
    parameter: String,
}

#[pymethods]
impl UnsupportedParameterError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("UnsupportedParameterError({})", self.message)
    }

    #[getter]
    fn get_parameter(&self) -> String {
        self.parameter.clone()
    }
}

impl UnsupportedParameterError {
    pub fn new(message: impl Into<String>, parameter: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            parameter: parameter.into(),
        }
    }
}

// =============================================================================
// InsufficientFundsError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct InsufficientFundsError {
    message: String,
    current_balance: i64, // µunits (microdollars) per RFC-0920 line 660
    required: i64,        // µunits (microdollars) per RFC-0920 line 660
}

#[pymethods]
impl InsufficientFundsError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("InsufficientFundsError({})", self.message)
    }

    #[getter]
    fn get_current_balance(&self) -> i64 {
        self.current_balance
    }

    #[getter]
    fn get_required(&self) -> i64 {
        self.required
    }
}

impl InsufficientFundsError {
    pub fn new(message: impl Into<String>, current_balance: i64, required: i64) -> Self {
        Self {
            message: message.into(),
            current_balance,
            required,
        }
    }
}

// =============================================================================
// UpstreamProviderError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct UpstreamProviderError {
    message: String,
    provider: String,
    upstream_code: Option<String>,
}

#[pymethods]
impl UpstreamProviderError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("UpstreamProviderError({})", self.message)
    }

    #[getter]
    fn get_provider(&self) -> String {
        self.provider.clone()
    }

    #[getter]
    fn get_upstream_code(&self) -> Option<String> {
        self.upstream_code.clone()
    }
}

impl UpstreamProviderError {
    pub fn new(
        message: impl Into<String>,
        provider: impl Into<String>,
        upstream_code: Option<String>,
    ) -> Self {
        Self {
            message: message.into(),
            provider: provider.into(),
            upstream_code,
        }
    }
}

// =============================================================================
// GatewayTimeoutError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct GatewayTimeoutError {
    message: String,
    provider: Option<String>,
}

#[pymethods]
impl GatewayTimeoutError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("GatewayTimeoutError({})", self.message)
    }
}

impl GatewayTimeoutError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            provider: None,
        }
    }

    pub fn with_provider(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            provider: Some(provider.into()),
        }
    }
}

// =============================================================================
// LengthFinishReasonError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct LengthFinishReasonError {
    message: String,
    model: String,
    finish_reason: String,
}

#[pymethods]
impl LengthFinishReasonError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("LengthFinishReasonError({})", self.message)
    }

    #[getter]
    fn get_model(&self) -> String {
        self.model.clone()
    }

    #[getter]
    fn get_finish_reason(&self) -> String {
        self.finish_reason.clone()
    }
}

impl LengthFinishReasonError {
    pub fn new(
        message: impl Into<String>,
        model: impl Into<String>,
        finish_reason: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            model: model.into(),
            finish_reason: finish_reason.into(),
        }
    }
}

// =============================================================================
// ContentFilterFinishReasonError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct ContentFilterFinishReasonError {
    message: String,
    model: String,
}

#[pymethods]
impl ContentFilterFinishReasonError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ContentFilterFinishReasonError({})", self.message)
    }

    #[getter]
    fn get_model(&self) -> String {
        self.model.clone()
    }
}

impl ContentFilterFinishReasonError {
    pub fn new(message: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            model: model.into(),
        }
    }
}

// =============================================================================
// BatchNotCompleteError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct BatchNotCompleteError {
    message: String,
    batch_id: String,
    status: String,
}

#[pymethods]
impl BatchNotCompleteError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("BatchNotCompleteError({})", self.message)
    }

    #[getter]
    fn get_batch_id(&self) -> String {
        self.batch_id.clone()
    }

    #[getter]
    fn get_status(&self) -> String {
        self.status.clone()
    }
}

impl BatchNotCompleteError {
    pub fn new(
        message: impl Into<String>,
        batch_id: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            batch_id: batch_id.into(),
            status: status.into(),
        }
    }
}

// =============================================================================
// AllModelsFailedError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct AllModelsFailedError {
    message: String,
    models: Vec<String>,
}

#[pymethods]
impl AllModelsFailedError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("AllModelsFailedError({})", self.message)
    }

    #[getter]
    fn get_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

impl AllModelsFailedError {
    pub fn new(message: impl Into<String>, models: Vec<String>) -> Self {
        Self {
            message: message.into(),
            models,
        }
    }
}

// =============================================================================
// BatchPartialFailureError
// =============================================================================

#[pyclass]
#[derive(Debug)]
pub struct BatchPartialFailureError {
    message: String,
    successful: Vec<String>,
    failed: Vec<String>,
}

#[pymethods]
impl BatchPartialFailureError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("BatchPartialFailureError({})", self.message)
    }

    #[getter]
    fn get_successful(&self) -> Vec<String> {
        self.successful.clone()
    }

    #[getter]
    fn get_failed(&self) -> Vec<String> {
        self.failed.clone()
    }
}

impl BatchPartialFailureError {
    pub fn new(message: impl Into<String>, successful: Vec<String>, failed: Vec<String>) -> Self {
        Self {
            message: message.into(),
            successful,
            failed,
        }
    }
}

// =============================================================================
// Register all exceptions
// =============================================================================

/// Register all exceptions in a Python module
pub fn register_exceptions(m: &PyModule) -> PyResult<()> {
    m.add_class::<QuotaRouterError>()?;
    m.add_class::<AuthenticationError>()?;
    m.add_class::<RateLimitError>()?;
    m.add_class::<InvalidRequestError>()?;
    m.add_class::<ProviderError>()?;
    m.add_class::<ContentFilterError>()?;
    m.add_class::<ModelNotFoundError>()?;
    m.add_class::<ContextLengthExceededError>()?;
    m.add_class::<MissingApiKeyError>()?;
    m.add_class::<UnsupportedProviderError>()?;
    m.add_class::<UnsupportedParameterError>()?;
    m.add_class::<InsufficientFundsError>()?;
    m.add_class::<UpstreamProviderError>()?;
    m.add_class::<GatewayTimeoutError>()?;
    m.add_class::<LengthFinishReasonError>()?;
    m.add_class::<ContentFilterFinishReasonError>()?;
    m.add_class::<BatchNotCompleteError>()?;
    m.add_class::<AllModelsFailedError>()?;
    m.add_class::<BatchPartialFailureError>()?;
    Ok(())
}
