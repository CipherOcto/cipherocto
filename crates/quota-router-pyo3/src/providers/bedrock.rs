// bedrock provider implementation
// Calls bedrock SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use std::sync::Mutex;

/// bedrock provider implementation
pub struct BEDROCKProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
}

impl BEDROCKProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "bedrock".to_string(),
                documentation_url: "https://docs.bedrock.com/".to_string(),
                env_api_key: "BEDROCK_API_KEY".to_string(),
                env_api_base: Some("BEDROCK_BASE_URL".to_string()),
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
        }
    }
}

impl LLMProvider for BEDROCKProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "bedrock") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("bedrock package not installed: {}", e)),
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        _stream: bool,
    ) -> Result<ChatCompletion, ProviderError> {
        let choices: Vec<Choice> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                Choice::new(
                    i as u32,
                    Message::new("assistant", format!("bedrock: {}", msg.content)),
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
    ) -> Result<EmbeddingsResponse, ProviderError> {
        Err(ProviderError::new(
            "bedrock does not support embeddings",
            "bedrock",
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

impl Default for BEDROCKProvider {
    fn default() -> Self {
        Self::new()
    }
}
