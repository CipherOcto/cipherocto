// ollama provider implementation
// Calls Ollama via PyO3 (ollama Python package)

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// ollama provider implementation
pub struct OLLAMAProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl OLLAMAProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "ollama".to_string(),
                documentation_url: "https://docs.ollama.com/".to_string(),
                env_api_key: "OLLAMA_API_KEY".to_string(),
                env_api_base: Some("OLLAMA_BASE_URL".to_string()),
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
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "ollama"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            let ollama = PyModule::import(py, "ollama").map_err(|e| {
                ProviderError::new(format!("Failed to import ollama: {}", e), "ollama")
            })?;

            let ollama_class = ollama.getattr("Client").map_err(|e| {
                ProviderError::new(format!("Failed to get Client: {}", e), "ollama")
            })?;

            let kwargs = PyDict::new(py);
            if let Some(base) = api_base.as_ref() {
                kwargs.set_item("host", base.as_str()).unwrap();
            } else {
                kwargs.set_item("host", "http://localhost:11434").unwrap();
            }

            let client = ollama_class.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(format!("Failed to create client: {}", e), "ollama")
            })?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for OLLAMAProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "ollama") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("ollama package not installed: {}", e)),
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
                "ollama",
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
                .map_err(|e| ProviderError::new(format!("Failed to get chat: {}", e), "ollama"))?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();

            chat.call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "ollama"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_ollama_response(py_result.as_ref(py), model))
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
            "ollama does not support embeddings",
            "ollama",
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

fn convert_py_ollama_response(
    py_obj: &PyAny,
    model: &str,
) -> Result<ChatCompletion, ProviderError> {
    let message_obj = py_obj
        .get_item("message")
        .map_err(|e| ProviderError::new(format!("Failed to get message: {}", e), "ollama"))?;

    let role: String = message_obj
        .get_item("role")
        .map_err(|e| ProviderError::new(format!("Failed to get role: {}", e), "ollama"))?
        .extract()
        .unwrap_or_else(|_| "assistant".to_string());

    let content: String = message_obj
        .get_item("content")
        .map_err(|e| ProviderError::new(format!("Failed to get content: {}", e), "ollama"))?
        .extract()
        .unwrap_or_default();

    let choice = Choice::new(0, Message::new(role, content), "stop");

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

impl Default for OLLAMAProvider {
    fn default() -> Self {
        Self::new()
    }
}
