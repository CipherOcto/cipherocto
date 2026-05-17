// cohere — Cohere Python SDK via PyO3 (INTERNAL boundary #1 per RFC-0917)
//
// This module calls the official Cohere Python SDK via PyO3.
// It is called by python_sdk_entry (EXTERNAL boundary #2).

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use super::PyBridgeProvider;
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::PyDict;

use crate::py_bridge::PyBridgeError;

/// Cohere provider via official Python SDK
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct CohereProvider {
    api_key: Option<String>,
    api_base: Option<String>,
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for CohereProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CohereProvider {
    pub fn new() -> Self {
        Self {
            api_key: None,
            api_base: None,
        }
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = Some(api_base);
        self
    }

    pub fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| PyBridgeError::ProviderError("No API key set".to_string()))?;
        let api_base = self.api_base.as_deref().unwrap_or("https://api.cohere.ai");

        Python::with_gil(|py| {
            let cohere = PyModule::import(py, "cohere")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to import cohere: {}", e)))?;

            let client_class = cohere.getattr("Client").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to get Client class: {}", e))
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", api_key).unwrap();
            kwargs.set_item("base_url", api_base).unwrap();

            let client = client_class
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;

            // Build chat messages
            let chat_kwargs = PyDict::new(py);
            chat_kwargs.set_item("model", model).unwrap();

            // Convert messages to chat format
            let last_msg = messages
                .last()
                .ok_or_else(|| PyBridgeError::ProviderError("No messages provided".to_string()))?;
            chat_kwargs.set_item("message", &last_msg.content).unwrap();

            let result = client
                .getattr("chat")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get chat: {}", e)))?
                .call((), Some(chat_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

            convert_response(result, py)
        })
    }
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
fn convert_response(
    py_obj: &PyAny,
    _py: Python<'_>,
) -> Result<crate::types::ChatCompletion, PyBridgeError> {
    let id: String = py_obj
        .get_item("id")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get id: {}", e)))?
        .extract()
        .map_err(|e| PyBridgeError::PyError(format!("Failed to extract id: {}", e)))?;

    let model_str: String = py_obj
        .get_item("model")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get model: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "cohere".to_string());

    let content: String = py_obj
        .get_item("text")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get text: {}", e)))?
        .extract()
        .map_err(|e| PyBridgeError::PyError(format!("Failed to extract text: {}", e)))?;

    let finish_reason = py_obj
        .get_item("finish_reason")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get finish_reason: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "stop".to_string());

    let choice = crate::types::Choice::new(
        0,
        crate::types::Message::new("assistant", content),
        finish_reason,
    );

    Ok(crate::types::ChatCompletion {
        id,
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model_str,
        choices: vec![choice],
        usage: crate::types::Usage::new(0, 0, 0),
    })
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl PyBridgeProvider for CohereProvider {
    fn name(&self) -> &str {
        "cohere"
    }

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }

    fn with_api_key(mut self: Box<Self>, key: String) -> Box<dyn PyBridgeProvider> {
        self.api_key = Some(key);
        self
    }

    fn with_api_base(mut self: Box<Self>, base: String) -> Box<dyn PyBridgeProvider> {
        self.api_base = Some(base);
        self
    }
}
