// Mistral provider implementation
// Calls Mistral SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{ProviderFeatures, ProviderMetadata, LLMProvider};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Embedding, Message};
use pyo3::prelude::*;
use std::sync::Mutex;

/// Mistral provider implementation
pub struct MistralProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
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
        }
    }
}

impl LLMProvider for MistralProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| {
            match PyModule::import(py, "mistralai") {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Mistral package not installed: {}", e)),
            }
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        _stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        // Mock implementation - in production, calls Mistral SDK
        let choices: Vec<Choice> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                Choice::new(
                    i as u32,
                    Message::new("assistant", format!("Mistral Echo: {}", msg.content)),
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
        self.completion(model, messages, false)
    }

    fn embedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, ProviderError> {
        // Mock implementation
        let embeddings: Vec<Embedding> = input
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let embedding: Vec<f32> = (0..1024).map(|_| 0.02).collect();
                Embedding::new(i as u32, embedding)
            })
            .collect();

        Ok(EmbeddingsResponse::new(model, embeddings))
    }

    async fn aembedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

impl Default for MistralProvider {
    fn default() -> Self {
        Self::new()
    }
}
