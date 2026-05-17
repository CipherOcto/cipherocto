// anthropic — Anthropic Python SDK via PyO3 (INTERNAL boundary #1 per RFC-0917)
//
// This module calls the official Anthropic Python SDK via PyO3.
// It is called by python_sdk_entry (EXTERNAL boundary #2).
//
// Per RFC-0917 lines 220-221:
// "Anthropic | `anthropic` Python SDK | Official Anthropic SDK"

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use super::PyBridgeProvider;
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::PyDict;

use crate::py_bridge::PyBridgeError;

/// Anthropic provider via official Python SDK
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct AnthropicProvider {
    api_key: Option<String>,
    api_base: Option<String>,
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicProvider {
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

    /// Call Anthropic completion via Python SDK
    pub fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| PyBridgeError::ProviderError("No API key set".to_string()))?;
        let api_base = self
            .api_base
            .as_deref()
            .unwrap_or("https://api.anthropic.com");

        Python::with_gil(|py| {
            // Import Anthropic SDK
            let anthropic = PyModule::import(py, "anthropic").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to import anthropic: {}", e))
            })?;

            let anthropic_class = anthropic.getattr("Anthropic").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to get Anthropic class: {}", e))
            })?;

            // Create client
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", api_key).unwrap();
            kwargs.set_item("base_url", api_base).unwrap();

            let client = anthropic_class
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;

            // Build messages list for Python SDK
            // Anthropic uses {"role": "user"|"assistant", "content": "..."}
            let py_messages: Vec<Py<PyDict>> = messages
                .iter()
                .map(|msg| {
                    let dict = PyDict::new(py);
                    // Map "assistant" to "assistant", others to "user"
                    let role = if msg.role == "assistant" {
                        "assistant"
                    } else {
                        "user"
                    };
                    dict.set_item("role", role).unwrap();
                    dict.set_item("content", &msg.content).unwrap();
                    dict.into()
                })
                .collect();

            // Call client.messages.create(model, messages, max_tokens=1024)
            let messages_attr = client
                .getattr("messages")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get messages: {}", e)))?;
            let create = messages_attr
                .getattr("create")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get create: {}", e)))?;

            let call_kwargs = PyDict::new(py);
            call_kwargs.set_item("model", model).unwrap();
            call_kwargs.set_item("messages", &py_messages).unwrap();
            call_kwargs.set_item("max_tokens", 1024).unwrap();

            let result = create
                .call((), Some(call_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

            // Convert to Rust type
            convert_response(result, py)
        })
    }
}

/// Convert Python Anthropic response to Rust ChatCompletion
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
fn convert_response(
    py_obj: &PyAny,
    _py: Python<'_>,
) -> Result<crate::types::ChatCompletion, PyBridgeError> {
    // Anthropic returns: { id, type, model, role, content: [{type, text}], stop_reason, stop_sequence, usage }
    // Convert to OpenAI-style ChatCompletion

    let id: String = py_obj
        .get_item("id")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get id: {}", e)))?
        .extract()
        .map_err(|e| PyBridgeError::PyError(format!("Failed to extract id: {}", e)))?;

    let model_str: String = py_obj
        .get_item("model")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get model: {}", e)))?
        .extract()
        .map_err(|e| PyBridgeError::PyError(format!("Failed to extract model: {}", e)))?;

    // Extract content from Anthropic response
    let content: String = py_obj
        .get_item("content")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get content: {}", e)))?
        .get_item(0) // First content block
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get first content block: {}", e)))?
        .get_item("text")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get text: {}", e)))?
        .extract()
        .map_err(|e| PyBridgeError::PyError(format!("Failed to extract text: {}", e)))?;

    let stop_reason: String = py_obj
        .get_item("stop_reason")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get stop_reason: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "stop".to_string());

    let usage_obj = py_obj
        .get_item("usage")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get usage: {}", e)))?;
    let input_tokens: u32 = usage_obj
        .get_item("input_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get input_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let output_tokens: u32 = usage_obj
        .get_item("output_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get output_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);

    let choice = crate::types::Choice::new(
        0,
        crate::types::Message::new("assistant", content),
        stop_reason,
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
        usage: crate::types::Usage::new(input_tokens, output_tokens, input_tokens + output_tokens),
    })
}

/// Re-export as PyBridgeProvider trait for generic use
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl PyBridgeProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
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
