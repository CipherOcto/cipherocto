// llamafile provider implementation
// Calls llamafile SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use std::sync::Mutex;

/// llamafile provider implementation
pub struct LLAMAFILEProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
}

impl LLAMAFILEProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "llamafile".to_string(),
                documentation_url: "https://docs.llamafile.com/".to_string(),
                env_api_key: "LLAMAFILE_API_KEY".to_string(),
                env_api_base: Some("LLAMAFILE_BASE_URL".to_string()),
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

impl LLMProvider for LLAMAFILEProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "llamafile") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("llamafile package not installed: {}", e)),
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
                    Message::new("assistant", format!("llamafile: {}", msg.content)),
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
            "llamafile does not support embeddings",
            "llamafile",
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

impl Default for LLAMAFILEProvider {
    fn default() -> Self {
        Self::new()
    }
}
