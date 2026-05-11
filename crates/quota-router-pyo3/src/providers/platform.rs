// platform provider implementation
// Calls any-llm platform API via PyO3 using any_llm_platform_client

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// platform provider implementation
pub struct PLATFORMProvider {
    metadata: ProviderMetadata,
    any_llm_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
    platform_client: Mutex<Option<Py<PyAny>>>,
}

impl PLATFORMProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "platform".to_string(),
                documentation_url: "https://github.com/mozilla-ai/any-llm".to_string(),
                env_api_key: "ANY_LLM_KEY".to_string(),
                env_api_base: Some("ANY_LLM_PLATFORM_URL".to_string()),
                api_base: None,
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
            any_llm_key: Mutex::new(None),
            api_base: Mutex::new(None),
            client: Mutex::new(None),
            platform_client: Mutex::new(None),
        }
    }

    fn ensure_platform_client(&self) -> Result<Py<PyAny>, ProviderError> {
        let mut client_guard = self
            .platform_client
            .lock()
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "platform"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        Python::with_gil(|py| {
            let platform_client_module =
                PyModule::import(py, "any_llm_platform_client").map_err(|e| {
                    ProviderError::new(
                        format!("Failed to import any_llm_platform_client: {}", e),
                        "platform",
                    )
                })?;

            let client_class = platform_client_module
                .getattr("AnyLLMPlatformClient")
                .map_err(|e| {
                    ProviderError::new(
                        format!("Failed to get AnyLLMPlatformClient: {}", e),
                        "platform",
                    )
                })?;

            // Get platform URL from env or use default
            let api_base = self.api_base.lock().unwrap();
            let platform_url: Option<String> = api_base
                .clone()
                .or_else(|| std::env::var("ANY_LLM_PLATFORM_URL").ok());
            let platform_url_str = platform_url.as_deref().unwrap_or("https://api.anyllm.ai");

            let kwargs = PyDict::new(py);
            kwargs
                .set_item("any_llm_platform_url", platform_url_str)
                .unwrap();

            let client = client_class.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(
                    format!("Failed to create platform client: {}", e),
                    "platform",
                )
            })?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }

    fn ensure_wrapped_client(&self, provider_name: &str) -> Result<Py<PyAny>, ProviderError> {
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "platform"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let any_llm_key = self.any_llm_key.lock().unwrap();
        let key = any_llm_key.as_ref().ok_or_else(|| {
            ProviderError::new("ANY_LLM_KEY required for platform provider", "platform")
        })?;

        let platform_client = self.ensure_platform_client()?;

        // Get decrypted provider key via platform client
        let provider_key_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = platform_client.as_ref(py);
            let method = client_obj
                .getattr("aget_decrypted_provider_key")
                .map_err(|e| {
                    ProviderError::new(
                        format!("Failed to get aget_decrypted_provider_key: {}", e),
                        "platform",
                    )
                })?;

            // Call async method - for now we use sync wrapper pattern
            // In real implementation, this would need async runtime
            let kwargs = PyDict::new(py);
            kwargs.set_item("any_llm_key", key).unwrap();
            kwargs.set_item("provider", provider_name).unwrap();

            // Use sync equivalent or blocking call
            let pyo3_coroutine = method.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(
                    format!("Failed to call aget_decrypted_provider_key: {}", e),
                    "platform",
                )
            })?;

            Ok(pyo3_coroutine.into())
        })?;

        // For simplicity, create an OpenAI client with the retrieved key
        // In real implementation, would create the appropriate provider type
        Python::with_gil(|py| {
            let openai = PyModule::import(py, "openai").map_err(|e| {
                ProviderError::new(format!("Failed to import openai: {}", e), "platform")
            })?;

            let openai_class = openai.getattr("OpenAI").map_err(|e| {
                ProviderError::new(format!("Failed to get OpenAI: {}", e), "platform")
            })?;

            let api_key: String = provider_key_result
                .as_ref(py)
                .get_item("api_key")
                .map_err(|e| {
                    ProviderError::new(format!("Failed to get api_key: {}", e), "platform")
                })?
                .extract()
                .unwrap_or_default();

            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", api_key).unwrap();

            // Set base URL if api_base is configured
            if let Some(base) = self.api_base.lock().unwrap().as_ref() {
                kwargs.set_item("base_url", base.as_str()).unwrap();
            }

            let client = openai_class.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(format!("Failed to create client: {}", e), "platform")
            })?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for PLATFORMProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.any_llm_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "any_llm_platform_client") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "any_llm_platform_client package not installed: {}",
                e
            )),
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        if stream {
            return Err(ProviderError::new(
                "Streaming not supported in sync completion. Use acompletion() instead.",
                "platform",
            ));
        }

        // Platform provider delegates to wrapped provider
        // For now, use OpenAI-compatible client with any_llm_key
        let client = self.ensure_wrapped_client("openai")?;

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

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let chat = client_obj.getattr("chat").map_err(|e| {
                ProviderError::new(format!("Failed to get chat: {}", e), "platform")
            })?;
            let completions = chat.getattr("completions").map_err(|e| {
                ProviderError::new(format!("Failed to get completions: {}", e), "platform")
            })?;
            let create = completions.getattr("create").map_err(|e| {
                ProviderError::new(format!("Failed to get create: {}", e), "platform")
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();

            create
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "platform"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_openai_response(py_result.as_ref(py), model))
    }

    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        self.completion(model, messages, stream)
    }

    fn embedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        let client = self.ensure_wrapped_client("openai")?;

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let embed = client_obj.getattr("embeddings").map_err(|e| {
                ProviderError::new(format!("Failed to get embeddings: {}", e), "platform")
            })?;
            let create = embed.getattr("create").map_err(|e| {
                ProviderError::new(format!("Failed to get create: {}", e), "platform")
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("input", input).unwrap();
            kwargs.set_item("model", model).unwrap();

            create
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "platform"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_embedding_response(py_result.as_ref(py), model))
    }

    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

fn convert_py_openai_response(
    py_obj: &PyAny,
    model: &str,
) -> Result<ChatCompletion, ProviderError> {
    let id: String = py_obj
        .get_item("id")
        .map_err(|e| ProviderError::new(format!("Failed to get id: {}", e), "platform"))?
        .extract()
        .unwrap_or_else(|_| format!("chatcmpl-{}", uuid::Uuid::new_v4()));

    let model_str: String = py_obj
        .get_item("model")
        .map_err(|e| ProviderError::new(format!("Failed to get model: {}", e), "platform"))?
        .extract()
        .unwrap_or_else(|_| model.to_string());

    let py_choices = py_obj
        .get_item("choices")
        .map_err(|e| ProviderError::new(format!("Failed to get choices: {}", e), "platform"))?;

    let choices: Vec<Choice> = if let Ok(list) = py_choices.downcast::<pyo3::types::PyList>() {
        let mut result = Vec::new();
        for i in 0..list.len() {
            let choice_obj = list.get_item(i).unwrap();
            let index = i as u32;

            let message_obj = choice_obj.get_item("message").map_err(|e| {
                ProviderError::new(format!("Failed to get message: {}", e), "platform")
            })?;
            let role: String = message_obj
                .get_item("role")
                .map_err(|e| ProviderError::new(format!("Failed to get role: {}", e), "platform"))?
                .extract()
                .unwrap_or_else(|_| "assistant".to_string());
            let content: String = message_obj
                .get_item("content")
                .map_err(|e| {
                    ProviderError::new(format!("Failed to get content: {}", e), "platform")
                })?
                .extract()
                .unwrap_or_default();

            let finish_reason: String = choice_obj
                .get_item("finish_reason")
                .map_err(|e| {
                    ProviderError::new(format!("Failed to get finish_reason: {}", e), "platform")
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
        return Err(ProviderError::new("choices is not a list", "platform"));
    };

    let usage_obj = py_obj
        .get_item("usage")
        .map_err(|e| ProviderError::new(format!("Failed to get usage: {}", e), "platform"))?;

    let prompt_tokens: u32 = usage_obj
        .get_item("prompt_tokens")
        .map_err(|e| ProviderError::new(format!("Failed to get prompt_tokens: {}", e), "platform"))?
        .extract()
        .unwrap_or(0);
    let completion_tokens: u32 = usage_obj
        .get_item("completion_tokens")
        .map_err(|e| {
            ProviderError::new(
                format!("Failed to get completion_tokens: {}", e),
                "platform",
            )
        })?
        .extract()
        .unwrap_or(0);
    let total_tokens: u32 = usage_obj
        .get_item("total_tokens")
        .map_err(|e| ProviderError::new(format!("Failed to get total_tokens: {}", e), "platform"))?
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

fn convert_py_embedding_response(
    py_obj: &PyAny,
    model: &str,
) -> Result<EmbeddingsResponse, ProviderError> {
    let data = py_obj
        .get_item("data")
        .map_err(|e| ProviderError::new(format!("Failed to get data: {}", e), "platform"))?
        .downcast::<pyo3::types::PyList>()
        .map_err(|_| ProviderError::new("data is not a list", "platform"))?;

    let mut embeddings = Vec::new();
    for i in 0..data.len() {
        let item = data.get_item(i).unwrap();
        let embedding_vec = item
            .get_item("embedding")
            .map_err(|e| ProviderError::new(format!("Failed to get embedding: {}", e), "platform"))?
            .extract::<Vec<f32>>()
            .unwrap_or_default();
        embeddings.push(crate::types::Embedding::new(i as u32, embedding_vec));
    }

    Ok(crate::types::EmbeddingsResponse::new(model, embeddings))
}

impl Default for PLATFORMProvider {
    fn default() -> Self {
        Self::new()
    }
}
