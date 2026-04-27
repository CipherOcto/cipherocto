// OpenAI provider implementation
// Calls OpenAI SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{ProviderFeatures, ProviderMetadata, LLMProvider};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Embedding, Message};
use pyo3::prelude::*;
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

    /// Initialize the OpenAI client using PyO3
    fn ensure_client(&self) -> Result<Py<PyAny>, ProviderError> {
        let mut client_guard = self.client.lock().map_err(|e| {
            ProviderError::new(format!("Lock error: {}", e), "openai")
        })?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        // Create OpenAI client via PyO3
        let api_key = self.api_key.lock().unwrap();
        let api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            let openai = PyModule::import(py, "openai")
                .map_err(|e| ProviderError::new(format!("Failed to import openai: {}", e), "openai"))?;

            let async_openai_class = openai.getattr("AsyncOpenAI")
                .map_err(|e| ProviderError::new(format!("Failed to get AsyncOpenAI: {}", e), "openai"))?;

            let key = api_key.as_ref().ok_or_else(|| {
                ProviderError::new("No API key set", "openai")
            })?;
            let base = api_base.as_ref().map(|s| s.as_str()).unwrap_or("https://api.openai.com/v1");

            let client = async_openai_class.call1((key, base))
                .map_err(|e| ProviderError::new(format!("Failed to create client: {}", e), "openai"))?;

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
        Python::with_gil(|py| {
            match PyModule::import(py, "openai") {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("OpenAI package not installed: {}", e)),
            }
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        _stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        // Mock implementation - in production, this calls the OpenAI SDK
        let choices: Vec<Choice> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                Choice::new(
                    i as u32,
                    Message::new("assistant", format!("OpenAI Echo: {}", msg.content)),
                    "stop",
                )
            })
            .collect();

        Ok(ChatCompletion::new(
            format!("chatcmpl-{}", uuid::Uuid::new_v4()),
            model,
            choices,
        ))
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

    fn embedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, ProviderError> {
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

    async fn aembedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}
