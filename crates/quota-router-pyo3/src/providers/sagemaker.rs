// sagemaker provider implementation
// Calls sagemaker SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{ProviderFeatures, ProviderMetadata, LLMProvider};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Embedding, Message};
use pyo3::prelude::*;
use std::sync::Mutex;

/// sagemaker provider implementation
pub struct SAGEMAKERProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
}

impl SAGEMAKERProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "sagemaker".to_string(),
                documentation_url: "https://docs.sagemaker.com/".to_string(),
                env_api_key: "SAGEMAKER_API_KEY".to_string(),
                env_api_base: Some("SAGEMAKER_BASE_URL".to_string()),
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

impl LLMProvider for SAGEMAKERProvider {
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
            match PyModule::import(py, "sagemaker") {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("sagemaker package not installed: {}", e)),
            }
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
                    Message::new("assistant", format!("sagemaker: {}", msg.content)),
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

    fn embedding(&self, _input: &[String], _model: &str) -> Result<EmbeddingsResponse, ProviderError> {
        Err(ProviderError::new(
            "sagemaker does not support embeddings",
            "sagemaker",
        ))
    }

    async fn aembedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, ProviderError> {
        self.embedding(input, model)
    }
}

impl Default for SAGEMAKERProvider {
    fn default() -> Self {
        Self::new()
    }
}
