// portkey — Portkey via Python SDK or OpenAI-compatible API (INTERNAL boundary #1 per RFC-0917)
//
// This module calls Portkey via the `portkey` Python SDK or OpenAI-compatible interface.
// It is called by python_sdk_entry (EXTERNAL boundary #2).
//
// Per RFC-0917 lines 220-221:
// "Portkey | `portkey` Python SDK | Official Portkey SDK"

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::{PyDict, PyList};

use crate::py_bridge::PyBridgeError;

/// Portkey provider via official Python SDK or OpenAI-compatible interface
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct PortkeyProvider {
    api_key: Option<String>,
    api_base: Option<String>,
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for PortkeyProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PortkeyProvider {
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

    /// Call Portkey completion via Python SDK or OpenAI-compatible API
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
            .unwrap_or("https://api.portkey.ai/v1");

        Python::with_gil(|py| {
            // Try importing the official portkey SDK first, fall back to openai with base_url
            let result =
                PyModule::import(py, "portkey").and_then(|portkey| portkey.getattr("Portkey"));

            match result {
                Ok(portkey_class) => {
                    // Use official Portkey SDK
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("api_key", api_key).unwrap();
                    kwargs.set_item("base_url", api_base).unwrap();

                    let client = portkey_class.call((), Some(kwargs)).map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to create client: {}", e))
                    })?;

                    // Build messages list for Python SDK
                    let py_messages: Vec<Py<PyDict>> = messages
                        .iter()
                        .map(|msg| {
                            let dict = PyDict::new(py);
                            dict.set_item("role", &msg.role).unwrap();
                            dict.set_item("content", &msg.content).unwrap();
                            dict.into()
                        })
                        .collect();

                    // Call client.chat.completions.create
                    let chat = client.getattr("chat").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get chat: {}", e))
                    })?;
                    let completions = chat.getattr("completions").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get completions: {}", e))
                    })?;
                    let create = completions.getattr("create").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get create: {}", e))
                    })?;

                    let call_kwargs = PyDict::new(py);
                    call_kwargs.set_item("model", model).unwrap();
                    call_kwargs.set_item("messages", &py_messages).unwrap();

                    let result = create
                        .call((), Some(call_kwargs))
                        .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

                    convert_response(result, py)
                }
                Err(_) => {
                    // Fall back to OpenAI SDK with custom base_url for Portkey's OpenAI-compatible endpoint
                    let openai = PyModule::import(py, "openai").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to import openai: {}", e))
                    })?;

                    let openai_class = openai.getattr("OpenAI").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get OpenAI class: {}", e))
                    })?;

                    // Create client with custom base_url for Portkey
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("api_key", api_key).unwrap();
                    kwargs.set_item("base_url", api_base).unwrap();

                    let client = openai_class.call((), Some(kwargs)).map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to create client: {}", e))
                    })?;

                    // Build messages list for Python SDK
                    let py_messages: Vec<Py<PyDict>> = messages
                        .iter()
                        .map(|msg| {
                            let dict = PyDict::new(py);
                            dict.set_item("role", &msg.role).unwrap();
                            dict.set_item("content", &msg.content).unwrap();
                            dict.into()
                        })
                        .collect();

                    // Call client.chat.completions.create
                    let chat = client.getattr("chat").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get chat: {}", e))
                    })?;
                    let completions = chat.getattr("completions").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get completions: {}", e))
                    })?;
                    let create = completions.getattr("create").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get create: {}", e))
                    })?;

                    let call_kwargs = PyDict::new(py);
                    call_kwargs.set_item("model", model).unwrap();
                    call_kwargs.set_item("messages", &py_messages).unwrap();

                    let result = create
                        .call((), Some(call_kwargs))
                        .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

                    convert_response(result, py)
                }
            }
        })
    }
}

/// Convert Python ChatCompletion response to Rust ChatCompletion
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
        .map_err(|e| PyBridgeError::PyError(format!("Failed to extract model: {}", e)))?;

    let py_choices = py_obj
        .get_item("choices")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get choices: {}", e)))?;

    let choices: Vec<crate::types::Choice> = if let Ok(list) = py_choices.downcast::<PyList>() {
        let mut result = Vec::new();
        for i in 0..list.len() {
            let choice_obj = list.get_item(i).unwrap();
            let index = i as u32;

            let message_obj = choice_obj
                .get_item("message")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get message: {}", e)))?;
            let role: String = message_obj
                .get_item("role")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get role: {}", e)))?
                .extract()
                .map_err(|e| PyBridgeError::PyError(format!("Failed to extract role: {}", e)))?;
            let content: String = message_obj
                .get_item("content")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get content: {}", e)))?
                .extract()
                .map_err(|e| PyBridgeError::PyError(format!("Failed to extract content: {}", e)))?;

            let finish_reason: String = choice_obj
                .get_item("finish_reason")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get finish_reason: {}", e)))?
                .extract()
                .unwrap_or_else(|_| "stop".to_string());

            result.push(crate::types::Choice::new(
                index,
                crate::types::Message::new(role, content),
                finish_reason,
            ));
        }
        result
    } else {
        return Err(PyBridgeError::PyError("choices is not a list".to_string()));
    };

    let usage_obj = py_obj
        .get_item("usage")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get usage: {}", e)))?;
    let prompt_tokens: u32 = usage_obj
        .get_item("prompt_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get prompt_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let completion_tokens: u32 = usage_obj
        .get_item("completion_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get completion_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let total_tokens: u32 = usage_obj
        .get_item("total_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get total_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);

    Ok(crate::types::ChatCompletion {
        id,
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model_str,
        choices,
        usage: crate::types::Usage::new(prompt_tokens, completion_tokens, total_tokens),
    })
}

/// Re-export as PyBridgeProvider trait for generic use
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub trait PyBridgeProvider: Send + Sync {
    fn name(&self) -> &str;
    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError>;
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl PyBridgeProvider for PortkeyProvider {
    fn name(&self) -> &str {
        "portkey"
    }

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }
}
