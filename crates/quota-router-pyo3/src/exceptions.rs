// QuotaRouter exceptions for PyO3 bindings
// Exception hierarchy per RFC-0917 Phase 3 and RFC-0920

#![allow(dead_code)]
#![allow(unused_variables)]

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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
        details: Option<std::collections::HashMap<String, String>>,
    ) -> Self {
        Self {
            message,
            llm_provider: provider,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("QuotaRouterError({})", self.message)
    }

    #[getter]
    fn code(&self) -> String {
        "internal_error".to_string()
    }

    #[getter]
    fn status(&self) -> i32 {
        0
    }

    #[getter]
    fn provider(&self) -> Option<String> {
        self.llm_provider.clone()
    }

    #[getter]
    fn details(&self) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::new()
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self {
            message,
            llm_provider: provider,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("AuthenticationError({})", self.message)
    }

    #[getter]
    fn code(&self) -> String {
        "auth_error".to_string()
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        retry_after: Option<f64>,
        status: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self {
            message,
            llm_provider: provider,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("RateLimitError({})", self.message)
    }

    #[getter]
    fn retry_after(&self) -> Option<f64> {
        None
    }

    #[getter]
    fn code(&self) -> String {
        "rate_limit_exceeded".to_string()
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        param: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self {
            message,
            llm_provider: provider,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("InvalidRequestError({})", self.message)
    }

    #[getter]
    fn param(&self) -> Option<String> {
        None
    }

    #[getter]
    fn code(&self) -> String {
        "invalid_request".to_string()
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        upstream_code: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self {
            message,
            llm_provider: provider.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ProviderError({})", self.message)
    }

    #[getter]
    fn upstream_code(&self) -> Option<String> {
        None
    }

    #[getter]
    fn code(&self) -> String {
        "provider_error".to_string()
    }
}

impl ProviderError {
    pub fn new(message: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            llm_provider: provider.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self {
            message,
            llm_provider: provider,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ContentFilterError({})", self.message)
    }

    #[getter]
    fn code(&self) -> String {
        "content_filter".to_string()
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        model: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self {
            message,
            model: model.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ModelNotFoundError({})", self.message)
    }

    #[getter]
    fn model(&self) -> String {
        self.model.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "model_not_found".to_string()
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
    received_tokens: Option<u32>,
}

#[pymethods]
impl ContextLengthExceededError {
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        max_tokens: Option<u32>,
        received_tokens: Option<u32>,
        status: Option<i32>,
        provider: Option<String>,
        model: Option<String>,
    ) -> Self {
        Self {
            message,
            model: model.unwrap_or_default(),
            max_tokens,
            received_tokens,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ContextLengthExceededError({})", self.message)
    }

    #[getter]
    fn model(&self) -> String {
        self.model.clone()
    }

    #[getter]
    fn max_tokens(&self) -> Option<u32> {
        self.max_tokens
    }

    #[getter]
    fn received_tokens(&self) -> Option<u32> {
        self.received_tokens
    }

    #[getter]
    fn code(&self) -> String {
        "context_length_exceeded".to_string()
    }
}

impl ContextLengthExceededError {
    pub fn new(
        message: impl Into<String>,
        model: impl Into<String>,
        max_tokens: Option<u32>,
        received_tokens: Option<u32>,
    ) -> Self {
        Self {
            message: message.into(),
            model: model.into(),
            max_tokens,
            received_tokens,
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
    env_var_name: String,
}

#[pymethods]
impl MissingApiKeyError {
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        provider: Option<String>,
        env_var_name: Option<String>,
        status: Option<i32>,
    ) -> Self {
        Self {
            message,
            provider: provider.unwrap_or_default(),
            env_var_name: env_var_name.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("MissingApiKeyError({})", self.message)
    }

    #[getter]
    fn provider(&self) -> String {
        self.provider.clone()
    }

    #[getter]
    fn env_var_name(&self) -> String {
        self.env_var_name.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "missing_api_key".to_string()
    }
}

impl MissingApiKeyError {
    pub fn new(
        message: impl Into<String>,
        provider: impl Into<String>,
        env_var_name: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            provider: provider.into(),
            env_var_name: env_var_name.into(),
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
    provider_key: String,
    supported_providers: Vec<String>,
}

#[pymethods]
impl UnsupportedProviderError {
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        provider_key: Option<String>,
        supported_providers: Option<Vec<String>>,
        status: Option<i32>,
    ) -> Self {
        Self {
            message,
            provider_key: provider_key.unwrap_or_default(),
            supported_providers: supported_providers.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("UnsupportedProviderError({})", self.message)
    }

    #[getter]
    fn provider_key(&self) -> String {
        self.provider_key.clone()
    }

    #[getter]
    fn supported_providers(&self) -> Vec<String> {
        self.supported_providers.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "unsupported_provider".to_string()
    }
}

impl UnsupportedProviderError {
    pub fn new(
        message: impl Into<String>,
        provider_key: impl Into<String>,
        supported_providers: Vec<String>,
    ) -> Self {
        Self {
            message: message.into(),
            provider_key: provider_key.into(),
            supported_providers,
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
    param: String,
    provider: String,
}

#[pymethods]
impl UnsupportedParameterError {
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        param: Option<String>,
        provider: Option<String>,
        status: Option<i32>,
    ) -> Self {
        Self {
            message,
            param: param.unwrap_or_default(),
            provider: provider.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("UnsupportedParameterError({})", self.message)
    }

    #[getter]
    fn param(&self) -> String {
        self.param.clone()
    }

    #[getter]
    fn provider(&self) -> String {
        self.provider.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "unsupported_parameter".to_string()
    }
}

impl UnsupportedParameterError {
    pub fn new(
        message: impl Into<String>,
        param: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            param: param.into(),
            provider: provider.into(),
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        current_balance: Option<i64>,
        required: Option<i64>,
        status: Option<i32>,
    ) -> Self {
        Self {
            message,
            current_balance: current_balance.unwrap_or(0),
            required: required.unwrap_or(0),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("InsufficientFundsError({})", self.message)
    }

    #[getter]
    fn current_balance(&self) -> i64 {
        self.current_balance
    }

    #[getter]
    fn required(&self) -> i64 {
        self.required
    }

    #[getter]
    fn code(&self) -> String {
        "insufficient_funds".to_string()
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
    status_code: Option<i32>,
}

#[pymethods]
impl UpstreamProviderError {
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        upstream_code: Option<String>,
        status_code: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self {
            message,
            provider: provider.unwrap_or_default(),
            upstream_code,
            status_code,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("UpstreamProviderError({})", self.message)
    }

    #[getter]
    fn provider(&self) -> String {
        self.provider.clone()
    }

    #[getter]
    fn upstream_code(&self) -> Option<String> {
        self.upstream_code.clone()
    }

    #[getter]
    fn status_code(&self) -> Option<i32> {
        self.status_code
    }

    #[getter]
    fn code(&self) -> String {
        "upstream_provider_error".to_string()
    }
}

impl UpstreamProviderError {
    pub fn new(
        message: impl Into<String>,
        provider: impl Into<String>,
        upstream_code: Option<String>,
        status_code: Option<i32>,
    ) -> Self {
        Self {
            message: message.into(),
            provider: provider.into(),
            upstream_code,
            status_code,
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
    ) -> Self {
        Self { message, provider }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("GatewayTimeoutError({})", self.message)
    }

    #[getter]
    fn code(&self) -> String {
        "gateway_timeout".to_string()
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        finish_reason: Option<String>,
        status: Option<i32>,
        provider: Option<String>,
        model: Option<String>,
    ) -> Self {
        Self {
            message,
            model: model.unwrap_or_default(),
            finish_reason: finish_reason.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("LengthFinishReasonError({})", self.message)
    }

    #[getter]
    fn model(&self) -> String {
        self.model.clone()
    }

    #[getter]
    fn finish_reason(&self) -> String {
        self.finish_reason.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "length_finish_reason".to_string()
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
    finish_reason: String,
}

#[pymethods]
impl ContentFilterFinishReasonError {
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        finish_reason: Option<String>,
        status: Option<i32>,
    ) -> Self {
        Self {
            message,
            finish_reason: finish_reason.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("ContentFilterFinishReasonError({})", self.message)
    }

    #[getter]
    fn finish_reason(&self) -> String {
        self.finish_reason.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "content_filter_finish_reason".to_string()
    }
}

impl ContentFilterFinishReasonError {
    pub fn new(message: impl Into<String>, finish_reason: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            finish_reason: finish_reason.into(),
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
    status_code: Option<i32>,
}

#[pymethods]
impl BatchNotCompleteError {
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        batch_id: Option<String>,
        status: Option<String>,
        status_code: Option<i32>,
    ) -> Self {
        Self {
            message,
            batch_id: batch_id.unwrap_or_default(),
            status: status.unwrap_or_default(),
            status_code,
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("BatchNotCompleteError({})", self.message)
    }

    #[getter]
    fn batch_id(&self) -> String {
        self.batch_id.clone()
    }

    #[getter]
    fn status(&self) -> String {
        self.status.clone()
    }

    #[getter]
    fn status_code(&self) -> Option<i32> {
        self.status_code
    }

    #[getter]
    fn code(&self) -> String {
        "batch_not_complete".to_string()
    }
}

impl BatchNotCompleteError {
    pub fn new(
        message: impl Into<String>,
        batch_id: impl Into<String>,
        status: impl Into<String>,
        status_code: Option<i32>,
    ) -> Self {
        Self {
            message: message.into(),
            batch_id: batch_id.into(),
            status: status.into(),
            status_code,
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        models: Option<Vec<String>>,
        status: Option<i32>,
    ) -> Self {
        Self {
            message,
            models: models.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("AllModelsFailedError({})", self.message)
    }

    #[getter]
    fn models(&self) -> Vec<String> {
        self.models.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "all_models_failed".to_string()
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
    #[new]
    fn py_new(
        message: String,
        code: Option<String>,
        successful: Option<Vec<String>>,
        failed: Option<Vec<String>>,
        status: Option<i32>,
    ) -> Self {
        Self {
            message,
            successful: successful.unwrap_or_default(),
            failed: failed.unwrap_or_default(),
        }
    }

    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("BatchPartialFailureError({})", self.message)
    }

    #[getter]
    fn successful(&self) -> Vec<String> {
        self.successful.clone()
    }

    #[getter]
    fn failed(&self) -> Vec<String> {
        self.failed.clone()
    }

    #[getter]
    fn code(&self) -> String {
        "batch_partial_failure".to_string()
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
    // Alias for any-llm compatibility
    let quota_router_error = m.getattr("QuotaRouterError")?;
    m.add("AnyLLMError", quota_router_error)?;
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
