// voyage provider implementation
// Calls Voyage AI SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// voyage provider implementation
pub struct VOYAGEProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl VOYAGEProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "voyage".to_string(),
                documentation_url: "https://docs.voyageai.com/".to_string(),
                env_api_key: "VOYAGE_API_KEY".to_string(),
                env_api_base: Some("VOYAGE_BASE_URL".to_string()),
                api_base: Some("https://api.voyageai.com/v1".to_string()),
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

    fn ensure_client(&self) -> Result<Py<PyAny>, ProviderError> {
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "voyage"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_key = self.api_key.lock().unwrap();
        let _api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            let voyage = PyModule::import(py, "voyageai").map_err(|e| {
                ProviderError::new(format!("Failed to import voyageai: {}", e), "voyage")
            })?;

            let voyage_class = voyage.getattr("VoyageAI").map_err(|e| {
                ProviderError::new(format!("Failed to get VoyageAI: {}", e), "voyage")
            })?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new("No API key set", "voyage"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();

            let client = voyage_class
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("Failed to create client: {}", e), "voyage"))?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for VOYAGEProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "voyageai") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("voyageai package not installed: {}", e)),
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
                "voyage",
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
                .map_err(|e| ProviderError::new(format!("Failed to get chat: {}", e), "voyage"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();

            chat.call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "voyage"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_voyage_response(py_result.as_ref(py), model))
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
        let client = self.ensure_client()?;

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let embed = client_obj
                .getattr("embed")
                .map_err(|e| ProviderError::new(format!("Failed to get embed: {}", e), "voyage"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("input", input).unwrap();
            kwargs.set_item("model", model).unwrap();

            embed.call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "voyage"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_voyage_embedding_response(py_result.as_ref(py), model))
    }

    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

fn convert_py_voyage_response(py_obj: &PyAny, model: &str) -> Result<ChatCompletion, ProviderError> {
    let id: String = py_obj
        .get_item("id")
        .map_err(|e| ProviderError::new(format!("Failed to get id: {}", e), "voyage"))?
        .extract()
        .unwrap_or_else(|_| format!("chatcmpl-{}", uuid::Uuid::new_v4()));

    let model_str: String = py_obj
        .get_item("model")
        .map_err(|e| ProviderError::new(format!("Failed to get model: {}", e), "voyage"))?
        .extract()
        .unwrap_or_else(|_| model.to_string());

    let text: String = py_obj
        .get_item("text")
        .map_err(|e| ProviderError::new(format!("Failed to get text: {}", e), "voyage"))?
        .extract()
        .unwrap_or_default();

    let choice = Choice::new(0, Message::new("assistant", text), "stop");

    Ok(ChatCompletion {
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

fn convert_py_voyage_embedding_response(py_obj: &PyAny, model: &str) -> Result<EmbeddingsResponse, ProviderError> {
    let data = py_obj
        .get_item("data")
        .map_err(|e| ProviderError::new(format!("Failed to get data: {}", e), "voyage"))?
        .downcast::<pyo3::types::PyList>()
        .map_err(|_| ProviderError::new("data is not a list", "voyage"))?;

    let mut embeddings = Vec::new();
    for i in 0..data.len() {
        let item = data.get_item(i).unwrap();
        let embedding_vec = item
            .get_item("embedding")
            .map_err(|e| ProviderError::new(format!("Failed to get embedding: {}", e), "voyage"))?
            .extract::<Vec<f32>>()
            .unwrap_or_default();
        embeddings.push(crate::types::Embedding::new(i as u32, embedding_vec));
    }

    Ok(crate::types::EmbeddingsResponse::new(model, embeddings))
}

impl Default for VOYAGEProvider {
    fn default() -> Self {
        Self::new()
    }
}