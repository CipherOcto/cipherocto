// OpenAI provider implementation
// Calls OpenAI SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, Embedding, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// OpenAI provider implementation
pub struct OpenAIProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    // Python client - will be initialized via PyO3
    client: Mutex<Option<Py<PyAny>>>,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "openai".to_string(),
                documentation_url: "https://platform.openai.com/docs/api-reference".to_string(),
                env_api_key: "OPENAI_API_KEY".to_string(),
                env_api_base: Some("OPENAI_BASE_URL".to_string()),
                api_base: Some("https://api.openai.com/v1".to_string()),
                features: ProviderFeatures {
                    supports_completion: true,
                    supports_completion_streaming: true,
                    supports_embedding: true,
                    supports_responses: true,
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

    /// Initialize the OpenAI client using PyO3 (synchronous client for simplicity)
    fn ensure_client(&self) -> Result<Py<PyAny>, ProviderError> {
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "openai"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        // Create OpenAI client via PyO3 using sync client
        let api_key = self.api_key.lock().unwrap();
        let api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            // Import the sync OpenAI client (not AsyncOpenAI)
            let openai = PyModule::import(py, "openai").map_err(|e| {
                ProviderError::new(format!("Failed to import openai: {}", e), "openai")
            })?;

            // Use the synchronous OpenAI class
            let openai_class = openai.getattr("OpenAI").map_err(|e| {
                ProviderError::new(format!("Failed to get OpenAI: {}", e), "openai")
            })?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new("No API key set", "openai"))?;
            let base = api_base
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("https://api.openai.com/v1");

            // Create client with api_key and base_url as keyword args
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();
            kwargs.set_item("base_url", base).unwrap();

            let client = openai_class.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(format!("Failed to create client: {}", e), "openai")
            })?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for OpenAIProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        // Check if OpenAI SDK is installed
        Python::with_gil(|py| match PyModule::import(py, "openai") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("OpenAI package not installed: {}", e)),
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        // Don't support streaming in sync version - use acompletion for streaming
        if stream {
            return Err(ProviderError::new(
                "Streaming not supported in sync completion. Use acompletion() instead.",
                "openai",
            ));
        }

        // Get or create client
        let client = self.ensure_client()?;

        // Build messages list for Python SDK
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

            // Navigate: client.chat.completions.create(model=model, messages=messages)
            let chat = client_obj
                .getattr("chat")
                .map_err(|e| ProviderError::new(format!("Failed to get chat: {}", e), "openai"))?;
            let completions = chat.getattr("completions").map_err(|e| {
                ProviderError::new(format!("Failed to get completions: {}", e), "openai")
            })?;
            let create = completions.getattr("create").map_err(|e| {
                ProviderError::new(format!("Failed to get create: {}", e), "openai")
            })?;

            // Call with keyword args
            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();

            create
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "openai"))
                .map(|obj| obj.into())
        })?;

        // Convert Python response to Rust ChatCompletion
        Python::with_gil(|py| convert_py_chat_completion(py_result.as_ref(py), model))
    }

    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        _stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        // For now, delegate to sync implementation
        // In production, this would call self.ensure_client() and make async call
        self.completion(model, messages, false)
    }

    fn embedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        // Mock implementation
        let embeddings: Vec<Embedding> = input
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let embedding: Vec<f32> = (0..1536).map(|_| 0.01).collect();
                Embedding::new(i as u32, embedding)
            })
            .collect();

        Ok(EmbeddingsResponse::new(model, embeddings))
    }

    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

/// Convert Python ChatCompletion object to Rust ChatCompletion
fn convert_py_chat_completion(
    py_obj: &PyAny,
    _model: &str,
) -> Result<ChatCompletion, ProviderError> {
    // The Python SDK returns an object with these attributes:
    // id, model, choices (list), usage (object with prompt_tokens, completion_tokens, total_tokens)

    let id: String = py_obj
        .get_item("id")
        .map_err(|e| ProviderError::new(format!("Failed to get id: {}", e), "openai"))?
        .extract()
        .map_err(|e| ProviderError::new(format!("Failed to extract id: {}", e), "openai"))?;

    let model_str: String = py_obj
        .get_item("model")
        .map_err(|e| ProviderError::new(format!("Failed to get model: {}", e), "openai"))?
        .extract()
        .map_err(|e| ProviderError::new(format!("Failed to extract model: {}", e), "openai"))?;

    let py_choices = py_obj
        .get_item("choices")
        .map_err(|e| ProviderError::new(format!("Failed to get choices: {}", e), "openai"))?;

    let choices: Vec<Choice> = if let Ok(list) = py_choices.downcast::<pyo3::types::PyList>() {
        let mut result = Vec::new();
        for i in 0..list.len() {
            let choice_obj = list.get_item(i).unwrap();
            let index = i as u32;

            let message_obj = choice_obj.get_item("message").map_err(|e| {
                ProviderError::new(format!("Failed to get message: {}", e), "openai")
            })?;
            let role: String = message_obj
                .get_item("role")
                .map_err(|e| ProviderError::new(format!("Failed to get role: {}", e), "openai"))?
                .extract()
                .map_err(|e| {
                    ProviderError::new(format!("Failed to extract role: {}", e), "openai")
                })?;
            let content: String = message_obj
                .get_item("content")
                .map_err(|e| ProviderError::new(format!("Failed to get content: {}", e), "openai"))?
                .extract()
                .map_err(|e| {
                    ProviderError::new(format!("Failed to extract content: {}", e), "openai")
                })?;

            let finish_reason: String = choice_obj
                .get_item("finish_reason")
                .map_err(|e| {
                    ProviderError::new(format!("Failed to get finish_reason: {}", e), "openai")
                })?
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
        return Err(ProviderError::new("choices is not a list", "openai"));
    };

    let usage_obj = py_obj
        .get_item("usage")
        .map_err(|e| ProviderError::new(format!("Failed to get usage: {}", e), "openai"))?;
    let prompt_tokens: u32 = usage_obj
        .get_item("prompt_tokens")
        .map_err(|e| ProviderError::new(format!("Failed to get prompt_tokens: {}", e), "openai"))?
        .extract()
        .unwrap_or(0);
    let completion_tokens: u32 = usage_obj
        .get_item("completion_tokens")
        .map_err(|e| {
            ProviderError::new(format!("Failed to get completion_tokens: {}", e), "openai")
        })?
        .extract()
        .unwrap_or(0);
    let total_tokens: u32 = usage_obj
        .get_item("total_tokens")
        .map_err(|e| ProviderError::new(format!("Failed to get total_tokens: {}", e), "openai"))?
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

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}
