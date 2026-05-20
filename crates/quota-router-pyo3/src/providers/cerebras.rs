// cerebras provider implementation
// Calls Cerebras SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::PyErr;
use std::sync::Mutex;

/// cerebras provider implementation
pub struct CEREBRASProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl CEREBRASProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "cerebras".to_string(),
                documentation_url: "https://docs.cerebras.ai/".to_string(),
                env_api_key: "CEREBRAS_API_KEY".to_string(),
                env_api_base: Some("CEREBRAS_BASE_URL".to_string()),
                api_base: Some("https://api.cerebras.ai/v1".to_string()),
                features: ProviderFeatures {
                    supports_completion: true,
                    supports_completion_streaming: true,
                    supports_embedding: false,
                    supports_responses: false,
                    supports_list_models: true,
                    supports_batch: false,
                    supports_messages: true,
                },
            },
            api_key: Mutex::new(None),
            api_base: Mutex::new(None),
            client: Mutex::new(None),
        }
    }

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
            let openai = PyModule::import(py, "openai")
                .map_err(|e| ProviderError::new_err(format!("Failed to import openai: {}", e)))?;

            let openai_class = openai
                .getattr("OpenAI")
                .map_err(|e| ProviderError::new_err(format!("Failed to get OpenAI: {}", e)))?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new_err("No API key set"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();
            if let Some(base) = api_base.as_ref() {
                kwargs.set_item("base_url", base.as_str()).unwrap();
            }

            let client = openai_class
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("Failed to create client: {}", e)))?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for CEREBRASProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), PyErr> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "openai") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("openai package not installed: {}", e)),
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        if stream {
            return Err(ProviderError::new_err(
                "Streaming not supported in sync completion. Use acompletion() instead.",
            ));
        }

        let client = self.ensure_client()?;

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

            let chat = client_obj
                .getattr("chat")
                .map_err(|e| ProviderError::new_err(format!("Failed to get chat: {}", e)))?;
            let completions = chat
                .getattr("completions")
                .map_err(|e| ProviderError::new_err(format!("Failed to get completions: {}", e)))?;
            let create = completions
                .getattr("create")
                .map_err(|e| ProviderError::new_err(format!("Failed to get create: {}", e)))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();

            create
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("SDK call failed: {}", e)))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_openai_response(py_result.as_ref(py), model))
    }

    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        self.completion(model, messages, stream)
    }

    fn embedding(&self, _input: &[String], _model: &str) -> Result<EmbeddingsResponse, PyErr> {
        Err(ProviderError::new_err(
            "cerebras does not support embeddings",
        ))
    }

    async fn aembedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, PyErr> {
        self.embedding(input, model)
    }
}

fn convert_py_openai_response(py_obj: &PyAny, model: &str) -> Result<ChatCompletion, PyErr> {
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

impl Default for CEREBRASProvider {
    fn default() -> Self {
        Self::new()
    }
}
