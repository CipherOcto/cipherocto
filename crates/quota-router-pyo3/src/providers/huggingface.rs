// huggingface provider implementation
// Calls HuggingFace SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::sync::Mutex;

/// huggingface provider implementation
pub struct HUGGINGFACEProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl HUGGINGFACEProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "huggingface".to_string(),
                documentation_url: "https://huggingface.co/docs/huggingface/index".to_string(),
                env_api_key: "HUGGINGFACE_API_KEY".to_string(),
                env_api_base: Some("HUGGINGFACE_BASE_URL".to_string()),
                api_base: Some("https://api-inference.huggingface.co/v1".to_string()),
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
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "huggingface"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_key = self.api_key.lock().unwrap();
        let api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            let hf = PyModule::import(py, "huggingface_hub").map_err(|e| {
                ProviderError::new(format!("Failed to import huggingface_hub: {}", e), "huggingface")
            })?;

            let inference_class = hf.getattr("InferenceClient").map_err(|e| {
                ProviderError::new(format!("Failed to get InferenceClient: {}", e), "huggingface")
            })?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new("No API key set", "huggingface"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();
            if let Some(base) = api_base.as_ref() {
                kwargs.set_item("base_url", base.as_str()).unwrap();
            }

            let client = inference_class
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("Failed to create client: {}", e), "huggingface"))?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for HUGGINGFACEProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "huggingface_hub") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("huggingface_hub package not installed: {}", e)),
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
                "huggingface",
            ));
        }

        let client = self.ensure_client()?;

        let prompt: String = messages
            .iter()
            .map(|msg| format!("{}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n");

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let chat = client_obj
                .getattr("chat")
                .map_err(|e| ProviderError::new(format!("Failed to get chat: {}", e), "huggingface"))?;

            // Build messages list
            let msg_dict = PyDict::new(py);
            msg_dict.set_item("role", "user").unwrap();
            msg_dict.set_item("content", &prompt).unwrap();
            let messages_list = PyList::new(py, vec![msg_dict.to_object(py)]);

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", messages_list).unwrap();

            chat.call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "huggingface"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_hf_response(py_result.as_ref(py), model))
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
                .map_err(|e| ProviderError::new(format!("Failed to get embed: {}", e), "huggingface"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("inputs", input).unwrap();
            kwargs.set_item("model", model).unwrap();

            embed.call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "huggingface"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_hf_embedding_response(py_result.as_ref(py)))
    }

    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

fn convert_py_hf_response(py_obj: &PyAny, model: &str) -> Result<ChatCompletion, ProviderError> {
    let content: String = py_obj
        .get_item("choices")
        .map_err(|e| ProviderError::new(format!("Failed to get choices: {}", e), "huggingface"))?
        .get_item(0)
        .map_err(|e| ProviderError::new(format!("Failed to get first choice: {}", e), "huggingface"))?
        .get_item("message")
        .map_err(|e| ProviderError::new(format!("Failed to get message: {}", e), "huggingface"))?
        .get_item("content")
        .map_err(|e| ProviderError::new(format!("Failed to get content: {}", e), "huggingface"))?
        .extract()
        .unwrap_or_default();

    let choice = Choice::new(0, Message::new("assistant", content), "stop");

    Ok(ChatCompletion {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model.to_string(),
        choices: vec![choice],
        usage: crate::types::Usage::new(0, 0, 0),
    })
}

fn convert_py_hf_embedding_response(py_obj: &PyAny) -> Result<EmbeddingsResponse, ProviderError> {
    let embeddings: Vec<crate::types::Embedding> = if let Ok(list) = py_obj.downcast::<pyo3::types::PyList>() {
        list.into_iter()
            .enumerate()
            .map(|(i, item)| {
                let emb = item.extract::<Vec<f32>>().unwrap_or_default();
                crate::types::Embedding::new(i as u32, emb)
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(crate::types::EmbeddingsResponse::new("embedding-model", embeddings))
}

impl Default for HUGGINGFACEProvider {
    fn default() -> Self {
        Self::new()
    }
}