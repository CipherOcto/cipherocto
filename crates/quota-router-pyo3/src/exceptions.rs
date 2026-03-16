// LiteLLM-compatible exceptions for PyO3 bindings

#![allow(dead_code)]

use pyo3::prelude::*;

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

#[pyclass]
#[derive(Debug)]
pub struct BudgetExceededError {
    message: String,
    budget: f64,
}

#[pymethods]
impl BudgetExceededError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("BudgetExceededError({})", self.message)
    }

    #[getter]
    fn get_budget(&self) -> f64 {
        self.budget
    }
}

impl BudgetExceededError {
    pub fn new(message: impl Into<String>, budget: f64) -> Self {
        Self {
            message: message.into(),
            budget,
        }
    }
}

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

#[pyclass]
#[derive(Debug)]
pub struct TimeoutError {
    message: String,
}

#[pymethods]
impl TimeoutError {
    fn __str__(&self) -> String {
        self.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("TimeoutError({})", self.message)
    }
}

impl TimeoutError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

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

/// Register all exceptions in a Python module
pub fn register_exceptions(m: &PyModule) -> PyResult<()> {
    m.add_class::<AuthenticationError>()?;
    m.add_class::<RateLimitError>()?;
    m.add_class::<BudgetExceededError>()?;
    m.add_class::<ProviderError>()?;
    m.add_class::<TimeoutError>()?;
    m.add_class::<InvalidRequestError>()?;
    Ok(())
}
