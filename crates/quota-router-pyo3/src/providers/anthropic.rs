// Anthropic provider implementation
// Calls Anthropic SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, Message};
use pyo3::prelude::*;
use std::sync::Mutex;

/// Anthropic provider implementation
#[allow(dead_code)]
pub struct AnthropicProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "anthropic".to_string(),
                documentation_url: "https://docs.anthropic.com/en/api/reference".to_string(),
                env_api_key: "ANTHROPIC_API_KEY".to_string(),
                env_api_base: Some("ANTHROPIC_BASE_URL".to_string()),
                api_base: Some("https://api.anthropic.com".to_string()),
                features: ProviderFeatures {
                    supports_completion: true,
                    supports_completion_streaming: true,
                    supports_embedding: false,
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

impl LLMProvider for AnthropicProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "anthropic") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Anthropic package not installed: {}", e)),
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        _stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        // Mock implementation - in production, calls Anthropic SDK
        let choices: Vec<Choice> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                Choice::new(
                    i as u32,
                    Message::new("assistant", format!("Anthropic Echo: {}", msg.content)),
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

    fn embedding(
        &self,
        _input: &[String],
        _model: &str,
    ) -> Result<crate::types::EmbeddingsResponse, ProviderError> {
        Err(ProviderError::new(
            "Anthropic does not support embeddings",
            "anthropic",
        ))
    }

    async fn aembedding(
        &self,
        input: &[String],
        model: &str,
    ) -> Result<crate::types::EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}
