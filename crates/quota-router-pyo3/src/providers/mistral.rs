// Mistral provider implementation
// Calls Mistral SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, Embedding, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::PyErr;
use std::sync::Mutex;

/// Mistral provider implementation
pub struct MistralProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    // Python client
    client: Mutex<Option<Py<PyAny>>>,
}

impl MistralProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "mistral".to_string(),
                documentation_url: "https://docs.mistral.com/api/".to_string(),
                env_api_key: "MISTRAL_API_KEY".to_string(),
                env_api_base: Some("MISTRAL_BASE_URL".to_string()),
                api_base: Some("https://api.mistral.ai/v1".to_string()),
                features: ProviderFeatures {
                    supports_completion: true,
                    supports_completion_streaming: true,
                    supports_embedding: true,
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

    /// Initialize the Mistral client using PyO3
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
            let mistral = PyModule::import(py, "mistralai").map_err(|e| {
                ProviderError::new_err(format!("Failed to import mistralai: {}", e))
            })?;

            let mistral_class = mistral
                .getattr("Mistral")
                .map_err(|e| ProviderError::new_err(format!("Failed to get Mistral: {}", e)))?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new_err("No API key set"))?;

            // Create client with api_key and optional base_url
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();
            if let Some(base) = api_base.as_ref() {
                kwargs.set_item("base_url", base.as_str()).unwrap();
            }

            let client = mistral_class
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("Failed to create client: {}", e)))?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for MistralProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), PyErr> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "mistralai") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Mistral package not installed: {}", e)),
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

        // Build messages list for Python SDK (OpenAI-compatible format)
        let py_messages: Vec<Py<PyDict>> = Python::with_gil(|py| {
            messages
                .iter()
                .map(|msg| {
                    let dict = PyDict::new(py);
                    dict.set_item("role", &msg.role).unwrap();
                    dict.set_item("content", &msg.content).unwrap();
                    dict.into()
                })
                .collect()
        });

        // Call the Python SDK
        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            // Navigate: client.chat.complete(model=model, messages=messages)
            let chat = client_obj
                .getattr("chat")
                .map_err(|e| ProviderError::new_err(format!("Failed to get chat: {}", e)))?;
            let complete = chat
                .getattr("complete")
                .map_err(|e| ProviderError::new_err(format!("Failed to get complete: {}", e)))?;

            // Call with keyword args
            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();

            complete
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("SDK call failed: {}", e)))
                .map(|obj| obj.into())
        })?;

        // Convert Python response to Rust ChatCompletion
        Python::with_gil(|py| convert_py_mistral_response(py_result.as_ref(py), model))
    }

    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        self.completion(model, messages, stream)
    }

    fn embedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, PyErr> {
        // Get or create client
        let client = self.ensure_client()?;

        // Call the Python SDK: client.embeddings.create(model=model, input=input)
        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let embeddings = client_obj
                .getattr("embeddings")
                .map_err(|e| ProviderError::new_err(format!("Failed to get embeddings: {}", e)))?;
            let create = embeddings
                .getattr("create")
                .map_err(|e| ProviderError::new_err(format!("Failed to get create: {}", e)))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("input", input).unwrap();

            create
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("SDK call failed: {}", e)))
                .map(|obj| obj.into())
        })?;

        // Convert Python response to Rust EmbeddingsResponse
        Python::with_gil(|py| {
            let obj = py_result.as_ref(py);

            let py_data = obj
                .get_item("data")
                .map_err(|e| ProviderError::new_err(format!("Failed to get data: {}", e)))?;

            let data_list = py_data
                .downcast::<pyo3::types::PyList>()
                .map_err(|e| ProviderError::new_err(format!("data is not a list: {}", e)))?;

            let mut embeddings = Vec::new();
            for i in 0..data_list.len() {
                let item = data_list.get_item(i).unwrap();
                let index: u32 = item
                    .get_item("index")
                    .and_then(|v| v.extract())
                    .unwrap_or(i as u32);

                let py_embedding = item.get_item("embedding").map_err(|e| {
                    ProviderError::new_err(format!("Failed to get embedding: {}", e))
                })?;
                let embedding_list =
                    py_embedding
                        .downcast::<pyo3::types::PyList>()
                        .map_err(|e| {
                            ProviderError::new_err(format!("embedding is not a list: {}", e))
                        })?;
                let embedding_vec: Vec<f32> = (0..embedding_list.len())
                    .map(|j| {
                        embedding_list
                            .get_item(j)
                            .and_then(|v| v.extract::<f32>())
                            .unwrap_or(0.0)
                    })
                    .collect();

                embeddings.push(Embedding::new(index, embedding_vec));
            }

            let model_str: String = obj
                .get_item("model")
                .and_then(|v| v.extract())
                .unwrap_or_else(|_| model.to_string());

            let usage_obj = obj
                .get_item("usage")
                .map_err(|e| ProviderError::new_err(format!("Failed to get usage: {}", e)))?;
            let prompt_tokens: u32 = usage_obj
                .get_item("prompt_tokens")
                .and_then(|v| v.extract())
                .unwrap_or(0);
            let total_tokens: u32 = usage_obj
                .get_item("total_tokens")
                .and_then(|v| v.extract())
                .unwrap_or(0);

            let mut response = EmbeddingsResponse::new(model_str, embeddings);
            response.usage = crate::types::Usage::new(prompt_tokens, 0, total_tokens);
            Ok(response)
        })
    }

    async fn aembedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, PyErr> {
        self.embedding(input, model)
    }
}

/// Convert Mistral response to Rust ChatCompletion
fn convert_py_mistral_response(py_obj: &PyAny, model: &str) -> Result<ChatCompletion, PyErr> {
    // Mistral returns: { id, model, choices: [{message: {role, content}, finish_reason, index}], usage }
    // Similar to OpenAI format

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

    let py_choices = py_obj
        .get_item("choices")
        .map_err(|e| ProviderError::new_err(format!("Failed to get choices: {}", e)))?;

    let choices: Vec<Choice> = if let Ok(list) = py_choices.downcast::<pyo3::types::PyList>() {
        let mut result = Vec::new();
        for i in 0..list.len() {
            let choice_obj = list.get_item(i).unwrap();
            let index = i as u32;

            let message_obj = choice_obj
                .get_item("message")
                .map_err(|e| ProviderError::new_err(format!("Failed to get message: {}", e)))?;
            let role: String = message_obj
                .get_item("role")
                .map_err(|e| ProviderError::new_err(format!("Failed to get role: {}", e)))?
                .extract()
                .unwrap_or_else(|_| "assistant".to_string());
            let content: String = message_obj
                .get_item("content")
                .map_err(|e| ProviderError::new_err(format!("Failed to get content: {}", e)))?
                .extract()
                .unwrap_or_default();

            let finish_reason: String = choice_obj
                .get_item("finish_reason")
                .map_err(|e| ProviderError::new_err(format!("Failed to get finish_reason: {}", e)))?
                .extract()
                .unwrap_or_else(|_| "stop".to_string());

            result.push(Choice::new(
                index,
                Message::new(role, content),
                finish_reason,
            ));
        }
        result
    } else {
        return Err(ProviderError::new_err("choices is not a list"));
    };

    let usage_obj = py_obj
        .get_item("usage")
        .map_err(|e| ProviderError::new_err(format!("Failed to get usage: {}", e)))?;

    let prompt_tokens: u32 = usage_obj
        .get_item("prompt_tokens")
        .map_err(|e| ProviderError::new_err(format!("Failed to get prompt_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let completion_tokens: u32 = usage_obj
        .get_item("completion_tokens")
        .map_err(|e| ProviderError::new_err(format!("Failed to get completion_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let total_tokens: u32 = usage_obj
        .get_item("total_tokens")
        .map_err(|e| ProviderError::new_err(format!("Failed to get total_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);

    Ok(ChatCompletion {
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

impl Default for MistralProvider {
    fn default() -> Self {
        Self::new()
    }
}
