// vertexai provider implementation
// Calls Vertex AI SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// vertexai provider implementation
pub struct VERTEXAIProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl VERTEXAIProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "vertexai".to_string(),
                documentation_url: "https://docs.vertexai.com/".to_string(),
                env_api_key: "VERTEXAI_API_KEY".to_string(),
                env_api_base: Some("VERTEXAI_BASE_URL".to_string()),
                api_base: None,
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

    fn ensure_client(&self) -> Result<Py<PyAny>, ProviderError> {
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "vertexai"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_key = self.api_key.lock().unwrap();
        let project_id = api_key
            .as_ref()
            .ok_or_else(|| ProviderError::new("No API key set", "vertexai"))?;

        Python::with_gil(|py| {
            let vertexai = PyModule::import(py, "vertexai").map_err(|e| {
                ProviderError::new(format!("Failed to import vertexai: {}", e), "vertexai")
            })?;

            let init_fn = vertexai.getattr("init").map_err(|e| {
                ProviderError::new(format!("Failed to get init: {}", e), "vertexai")
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("project", project_id.as_str()).unwrap();
            kwargs.set_item("location", "us-central1").unwrap();

            init_fn.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(format!("Failed to init vertexai: {}", e), "vertexai")
            })?;

            let client_py: Py<PyAny> = py.None();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for VERTEXAIProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "vertexai") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("vertexai package not installed: {}", e)),
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
                "vertexai",
            ));
        }

        self.ensure_client()?;

        // Build prompt from messages
        let prompt: String = messages
            .iter()
            .map(|msg| format!("{}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n");

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let vertexai = PyModule::import(py, "vertexai").map_err(|e| {
                ProviderError::new(format!("Failed to import vertexai: {}", e), "vertexai")
            })?;

            let generative_model = vertexai.getattr("GenerativeModel").map_err(|e| {
                ProviderError::new(format!("Failed to get GenerativeModel: {}", e), "vertexai")
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("model_name", model).unwrap();

            let model_obj = generative_model.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(format!("Failed to create model: {}", e), "vertexai")
            })?;

            let generate_content = model_obj.getattr("generate_content").map_err(|e| {
                ProviderError::new(format!("Failed to get generate_content: {}", e), "vertexai")
            })?;

            let content_kwarg = PyDict::new(py);
            content_kwarg.set_item("text", &prompt).unwrap();

            let content_obj = PyDict::new(py);
            content_obj.set_item("role", "user").unwrap();
            content_obj.set_item("parts", vec![content_kwarg]).unwrap();

            generate_content
                .call1((vec![content_obj],))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "vertexai"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_vertex_response(py_result.as_ref(py), model))
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
        _input: &[String],
        _model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        Err(ProviderError::new(
            "vertexai does not support embeddings",
            "vertexai",
        ))
    }

    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

fn convert_py_vertex_response(
    py_obj: &PyAny,
    model: &str,
) -> Result<ChatCompletion, ProviderError> {
    // Vertex AI returns a GenerateContentResponse object
    // We need to parse candidates[0].content.parts[0].text
    let candidates = py_obj
        .get_item("candidates")
        .map_err(|e| ProviderError::new(format!("Failed to get candidates: {}", e), "vertexai"))?;

    let first_candidate = candidates.get_item(0).map_err(|e| {
        ProviderError::new(format!("Failed to get first candidate: {}", e), "vertexai")
    })?;

    let content = first_candidate
        .get_item("content")
        .map_err(|e| ProviderError::new(format!("Failed to get content: {}", e), "vertexai"))?;

    let parts = content
        .get_item("parts")
        .map_err(|e| ProviderError::new(format!("Failed to get parts: {}", e), "vertexai"))?;

    let first_part = parts
        .get_item(0)
        .map_err(|e| ProviderError::new(format!("Failed to get first part: {}", e), "vertexai"))?;

    let text: String = first_part
        .get_item("text")
        .map_err(|e| ProviderError::new(format!("Failed to get text: {}", e), "vertexai"))?
        .extract()
        .unwrap_or_default();

    let choice = Choice::new(0, Message::new("assistant", text), "stop");

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

impl Default for VERTEXAIProvider {
    fn default() -> Self {
        Self::new()
    }
}
