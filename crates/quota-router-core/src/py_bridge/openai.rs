// openai — OpenAI Python SDK via PyO3 (INTERNAL boundary #1 per RFC-0917)
//
// This module calls the official OpenAI Python SDK via PyO3.
// It is called by python_sdk_entry (EXTERNAL boundary #2).
//
// Per RFC-0917 lines 220-221:
// "OpenAI | `openai` Python SDK | Official OpenAI SDK"

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::{PyDict, PyList};

/// Provider error type for py_bridge
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[derive(Debug, thiserror::Error)]
pub enum PyBridgeError {
    #[error("PyO3 error: {0}")]
    PyError(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),
}

/// OpenAI provider via official Python SDK
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct OpenAIProvider {
    api_key: Option<String>,
    api_base: Option<String>,
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAIProvider {
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

    /// Call OpenAI completion via Python SDK
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
            .unwrap_or("https://api.openai.com/v1");

        Python::with_gil(|py| {
            // Import OpenAI SDK
            let openai = PyModule::import(py, "openai")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to import openai: {}", e)))?;

            let openai_class = openai.getattr("OpenAI").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to get OpenAI class: {}", e))
            })?;

            // Create client
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", api_key).unwrap();
            kwargs.set_item("base_url", api_base).unwrap();

            let client = openai_class
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;

            // Build messages list for Python SDK - use Vec of owned Py<PyDict>
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
            let chat = client
                .getattr("chat")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get chat: {}", e)))?;
            let completions = chat
                .getattr("completions")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get completions: {}", e)))?;
            let create = completions
                .getattr("create")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get create: {}", e)))?;

            let call_kwargs = PyDict::new(py);
            call_kwargs.set_item("model", model).unwrap();
            call_kwargs.set_item("messages", &py_messages).unwrap();

            let result = create
                .call((), Some(call_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

            // Convert to Rust type
            convert_response(result, py)
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
/// Streaming chunk from py_bridge
#[derive(Debug, Clone)]
pub enum PyBridgeChunk {
    /// Raw SSE bytes to forward
    RawSSE(Vec<u8>),
    /// Structured chunk for conversion
    Structured(crate::shared_types::ChatCompletionChunk),
}

pub trait PyBridgeProvider: Send + Sync {
    fn name(&self) -> &str;
    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError>;

    /// Streaming completion — returns a receiver for SSE chunks
    /// Default implementation returns an error (streaming not supported)
    fn streaming_completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<tokio::sync::mpsc::Receiver<Result<PyBridgeChunk, PyBridgeError>>, PyBridgeError>
    {
        Err(PyBridgeError::ProviderError(format!(
            "Streaming not supported for provider '{}'",
            self.name()
        )))
    }
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl PyBridgeProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }

    fn streaming_completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<tokio::sync::mpsc::Receiver<Result<PyBridgeChunk, PyBridgeError>>, PyBridgeError>
    {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| PyBridgeError::ProviderError("No API key set".to_string()))?
            .clone();
        let api_base = self
            .api_base
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let model = model.to_string();
        let messages = messages.to_vec();

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Spawn a background thread to call Python SDK with streaming
        std::thread::spawn(move || {
            let result = Python::with_gil(|py| {
                // Import OpenAI SDK
                let openai = PyModule::import(py, "openai").map_err(|e| {
                    PyBridgeError::PyError(format!("Failed to import openai: {}", e))
                })?;

                let openai_class = openai.getattr("OpenAI").map_err(|e| {
                    PyBridgeError::PyError(format!("Failed to get OpenAI class: {}", e))
                })?;

                // Create client
                let kwargs = PyDict::new(py);
                kwargs.set_item("api_key", &api_key).unwrap();
                kwargs.set_item("base_url", &api_base).unwrap();

                let client = openai_class.call((), Some(kwargs)).map_err(|e| {
                    PyBridgeError::PyError(format!("Failed to create client: {}", e))
                })?;

                // Build messages list
                let py_messages: Vec<Py<PyDict>> = messages
                    .iter()
                    .map(|msg| {
                        let dict = PyDict::new(py);
                        dict.set_item("role", &msg.role).unwrap();
                        dict.set_item("content", &msg.content).unwrap();
                        dict.into()
                    })
                    .collect();

                // Call client.chat.completions.create with stream=True
                let chat = client
                    .getattr("chat")
                    .map_err(|e| PyBridgeError::PyError(format!("Failed to get chat: {}", e)))?;
                let completions = chat.getattr("completions").map_err(|e| {
                    PyBridgeError::PyError(format!("Failed to get completions: {}", e))
                })?;
                let create = completions
                    .getattr("create")
                    .map_err(|e| PyBridgeError::PyError(format!("Failed to get create: {}", e)))?;

                let call_kwargs = PyDict::new(py);
                call_kwargs.set_item("model", &model).unwrap();
                call_kwargs.set_item("messages", &py_messages).unwrap();
                call_kwargs.set_item("stream", true).unwrap();

                let stream = create
                    .call((), Some(call_kwargs))
                    .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

                // Iterate over stream and send chunks
                for chunk_result in stream.iter().map_err(|e| {
                    PyBridgeError::PyError(format!("Stream iteration failed: {}", e))
                })? {
                    let chunk = chunk_result
                        .map_err(|e| PyBridgeError::PyError(format!("Chunk error: {}", e)))?;

                    // Convert Python chunk to JSON bytes
                    let json_str: String = py
                        .import("json")
                        .map_err(|e| {
                            PyBridgeError::PyError(format!("Failed to import json: {}", e))
                        })?
                        .getattr("dumps")
                        .map_err(|e| PyBridgeError::PyError(format!("Failed to get dumps: {}", e)))?
                        .call1((chunk,))
                        .map_err(|e| {
                            PyBridgeError::PyError(format!("Failed to serialize chunk: {}", e))
                        })?
                        .extract()
                        .map_err(|e| {
                            PyBridgeError::PyError(format!("Failed to extract json: {}", e))
                        })?;

                    let sse_bytes = format!("data: {}\n\n", json_str).into_bytes();
                    let _ = tx.blocking_send(Ok(PyBridgeChunk::RawSSE(sse_bytes)));
                }

                // Send DONE marker
                let _ = tx.blocking_send(Ok(PyBridgeChunk::RawSSE(b"data: [DONE]\n\n".to_vec())));

                Ok::<(), PyBridgeError>(())
            });

            if let Err(e) = result {
                let _ = tx.blocking_send(Err(e));
            }
        });

        Ok(rx)
    }
}
