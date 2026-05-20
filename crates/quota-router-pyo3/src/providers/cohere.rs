// cohere provider implementation
// Calls Cohere SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, Embedding, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::PyErr;
use std::sync::Mutex;

/// cohere provider implementation
pub struct COHEREProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl COHEREProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "cohere".to_string(),
                documentation_url: "https://docs.cohere.com/".to_string(),
                env_api_key: "COHERE_API_KEY".to_string(),
                env_api_base: Some("COHERE_BASE_URL".to_string()),
                api_base: Some("https://api.cohere.ai/v1".to_string()),
                features: ProviderFeatures {
                    supports_completion: true,
                    supports_completion_streaming: true,
                    supports_embedding: true,
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

        Python::with_gil(|py| {
            let cohere = PyModule::import(py, "cohere")
                .map_err(|e| ProviderError::new_err(format!("Failed to import cohere: {}", e)))?;

            let cohere_class = cohere
                .getattr("Client")
                .map_err(|e| ProviderError::new_err(format!("Failed to get Client: {}", e)))?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new_err("No API key set"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();

            let client = cohere_class
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("Failed to create client: {}", e)))?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for COHEREProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), PyErr> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "cohere") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("cohere package not installed: {}", e)),
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

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();

            chat.call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("SDK call failed: {}", e)))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_cohere_response(py_result.as_ref(py), model))
    }

    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        self.completion(model, messages, stream)
    }

    fn embedding(&self, input: &[String], _model: &str) -> Result<EmbeddingsResponse, PyErr> {
        let client = self.ensure_client()?;

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let embed = client_obj
                .getattr("embed")
                .map_err(|e| ProviderError::new_err(format!("Failed to get embed: {}", e)))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("texts", input).unwrap();

            embed
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("SDK call failed: {}", e)))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_cohere_embedding_response(py_result.as_ref(py)))
    }

    async fn aembedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, PyErr> {
        self.embedding(input, model)
    }
}

fn convert_py_cohere_response(py_obj: &PyAny, model: &str) -> Result<ChatCompletion, PyErr> {
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

    let text: String = py_obj
        .get_item("text")
        .map_err(|e| ProviderError::new_err(format!("Failed to get text: {}", e)))?
        .extract()
        .unwrap_or_default();

    let finish_reason: String = py_obj
        .get_item("finish_reason")
        .map_err(|e| ProviderError::new_err(format!("Failed to get finish_reason: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "stop".to_string());

    let choice = Choice::new(0, Message::new("assistant", text), finish_reason);

    let usage_obj = py_obj.get_item("usage");
    let (prompt_tokens, completion_tokens, total_tokens) = match usage_obj {
        Ok(u) => (
            u.get_item("prompt_tokens")
                .ok()
                .and_then(|v| v.extract::<u32>().ok())
                .unwrap_or(0),
            u.get_item("completion_tokens")
                .ok()
                .and_then(|v| v.extract::<u32>().ok())
                .unwrap_or(0),
            u.get_item("total_tokens")
                .ok()
                .and_then(|v| v.extract::<u32>().ok())
                .unwrap_or(0),
        ),
        Err(_) => (0, 0, 0),
    };

    Ok(ChatCompletion {
        id,
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model_str,
        choices: vec![choice],
        usage: crate::types::Usage::new(prompt_tokens, completion_tokens, total_tokens),
    })
}

fn convert_py_cohere_embedding_response(py_obj: &PyAny) -> Result<EmbeddingsResponse, PyErr> {
    let embeddings: Vec<Embedding> = py_obj
        .get_item("embeddings")
        .map_err(|e| ProviderError::new_err(format!("Failed to get embeddings: {}", e)))?
        .extract::<Vec<Vec<f32>>>()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, emb)| Embedding::new(i as u32, emb))
        .collect();

    let model: String = py_obj
        .get_item("model")
        .ok()
        .and_then(|m| m.extract().ok())
        .unwrap_or_else(|| "embed-english-v3.0".to_string());

    Ok(EmbeddingsResponse::new(&model, embeddings))
}

impl Default for COHEREProvider {
    fn default() -> Self {
        Self::new()
    }
}
