// Anthropic provider implementation
// Calls Anthropic SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::PyErr;
use std::sync::Mutex;

/// Anthropic provider implementation
pub struct AnthropicProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    // Python client
    client: Mutex<Option<Py<PyAny>>>,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "anthropic".to_string(),
                documentation_url: "https://docs.anthropic.com/en/api/reference".to_string(),
                env_api_key: "ANTHROPIC_API_KEY".to_string(),
                env_api_base: Some("ANTHROPIC_BASE_URL".to_string()),
                api_base: Some("https://api.anthropic.com".to_string()),
                features: ProviderFeatures {
                    supports_completion: true,
                    supports_completion_streaming: true,
                    supports_embedding: false,
                    supports_responses: false,
                    supports_list_models: true,
                    supports_batch: true,
                    supports_messages: true,
                },
            },
            api_key: Mutex::new(None),
            api_base: Mutex::new(None),
            client: Mutex::new(None),
        }
    }

    /// Initialize the Anthropic client using PyO3
    fn ensure_client(&self) -> Result<Py<PyAny>, PyErr> {
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| ProviderError::new_err(format!("Lock error: {}", e)))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_key = self.api_key.lock().unwrap();
        let api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            let anthropic = PyModule::import(py, "anthropic").map_err(|e| {
                ProviderError::new_err(format!("Failed to import anthropic: {}", e))
            })?;

            let anthropic_class = anthropic
                .getattr("Anthropic")
                .map_err(|e| ProviderError::new_err(format!("Failed to get Anthropic: {}", e)))?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new_err("No API key set"))?;

            // Create client with api_key and optional base_url
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();
            if let Some(base) = api_base.as_ref() {
                kwargs.set_item("base_url", base.as_str()).unwrap();
            }

            let client = anthropic_class
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("Failed to create client: {}", e)))?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for AnthropicProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), PyErr> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "anthropic") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Anthropic package not installed: {}", e)),
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        // Don't support streaming in sync version
        if stream {
            return Err(ProviderError::new_err(
                "Streaming not supported in sync completion. Use acompletion() instead.",
            ));
        }

        // Get or create client
        let client = self.ensure_client()?;

        // Build messages list for Python SDK
        // Anthropic uses a different format: {"role": "user", "content": "..."}
        let py_messages: Vec<Py<PyDict>> = Python::with_gil(|py| {
            messages
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
                .collect()
        });

        // Call the Python SDK
        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            // Navigate: client.messages.create(model=model, messages=messages, max_tokens=1024)
            let messages_attr = client_obj
                .getattr("messages")
                .map_err(|e| ProviderError::new_err(format!("Failed to get messages: {}", e)))?;
            let create = messages_attr
                .getattr("create")
                .map_err(|e| ProviderError::new_err(format!("Failed to get create: {}", e)))?;

            // Call with keyword args
            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();
            kwargs.set_item("max_tokens", 1024).unwrap();

            create
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("SDK call failed: {}", e)))
                .map(|obj| obj.into())
        })?;

        // Convert Python response to Rust ChatCompletion
        Python::with_gil(|py| convert_py_anthropic_response(py_result.as_ref(py), model))
    }

    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        // For now, delegate to sync implementation
        self.completion(model, messages, stream)
    }

    fn embedding(
        &self,
        _input: &[String],
        _model: &str,
    ) -> Result<crate::types::EmbeddingsResponse, PyErr> {
        Err(ProviderError::new_err(
            "Anthropic does not support embeddings",
        ))
    }

    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<crate::types::EmbeddingsResponse, PyErr> {
        self.embedding(input, model)
    }
}

/// Convert Anthropic response to Rust ChatCompletion
fn convert_py_anthropic_response(py_obj: &PyAny, model: &str) -> Result<ChatCompletion, PyErr> {
    // Anthropic returns: { id, type, model, role, content: [{type, text}], stop_reason, stop_sequence, usage }
    // We need to convert to OpenAI-style ChatCompletion

    let id: String = py_obj
        .get_item("id")
        .map_err(|e| ProviderError::new_err(format!("Failed to get id: {}", e)))?
        .extract()
        .unwrap_or_else(|_| format!("chatcmpl-{}", uuid::Uuid::new_v4()));

    let model_str: String = py_obj
        .get_item("model")
        .map_err(|e| ProviderError::new_err(format!("Failed to get model: {}", e)))?
        .extract()
        .unwrap_or_else(|_| model.to_string());

    // Extract content from Anthropic response
    let content: String = py_obj
        .get_item("content")
        .map_err(|e| ProviderError::new_err(format!("Failed to get content: {}", e)))?
        .get_item(0) // First content block
        .map_err(|e| ProviderError::new_err(format!("Failed to get first content block: {}", e)))?
        .get_item("text")
        .map_err(|e| ProviderError::new_err(format!("Failed to get text: {}", e)))?
        .extract()
        .unwrap_or_default();

    let stop_reason: String = py_obj
        .get_item("stop_reason")
        .map_err(|e| ProviderError::new_err(format!("Failed to get stop_reason: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "stop".to_string());

    let usage_obj = py_obj
        .get_item("usage")
        .map_err(|e| ProviderError::new_err(format!("Failed to get usage: {}", e)))?;

    let input_tokens: u32 = usage_obj
        .get_item("input_tokens")
        .map_err(|e| ProviderError::new_err(format!("Failed to get input_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);

    let output_tokens: u32 = usage_obj
        .get_item("output_tokens")
        .map_err(|e| ProviderError::new_err(format!("Failed to get output_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);

    let choice = Choice::new(0, Message::new("assistant", content), stop_reason);

    Ok(ChatCompletion {
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

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}
