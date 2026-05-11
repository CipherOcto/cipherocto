// bedrock provider implementation
// Calls AWS Bedrock via boto3 PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// bedrock provider implementation
pub struct BEDROCKProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl BEDROCKProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "bedrock".to_string(),
                documentation_url: "https://docs.bedrock.com/".to_string(),
                env_api_key: "AWS_ACCESS_KEY_ID".to_string(),
                env_api_base: Some("AWS_REGION".to_string()),
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
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "bedrock"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_key = self.api_key.lock().unwrap();
        let api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            let boto3 = PyModule::import(py, "boto3").map_err(|e| {
                ProviderError::new(format!("Failed to import boto3: {}", e), "bedrock")
            })?;

            let client_fn = boto3.getattr("client").map_err(|e| {
                ProviderError::new(format!("Failed to get client: {}", e), "bedrock")
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("service_name", "bedrock-runtime").unwrap();

            if let Some(key) = api_key.as_ref() {
                kwargs.set_item("aws_access_key_id", key).unwrap();
            }

            if let Some(region) = api_base.as_ref() {
                kwargs.set_item("region_name", region.as_str()).unwrap();
            }

            let client = client_fn.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(format!("Failed to create client: {}", e), "bedrock")
            })?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
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
        Python::with_gil(|py| match PyModule::import(py, "boto3") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("boto3 package not installed: {}", e)),
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
                "bedrock",
            ));
        }

        let client = self.ensure_client()?;

        // Build the prompt from messages
        let prompt: String = messages
            .iter()
            .map(|msg| format!("{}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Bedrock uses model ID format like "anthropic.claude-3-5-sonnet-20241022"
        let body = format!(
            r#"{{"anthropic_version": "vertex-2023-10-07", "max_tokens": 1024, "messages": [{{"role": "user", "content": "{}"}}]}}"#,
            prompt.replace("\"", "\\\"")
        );

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let invoke = client_obj.getattr("invoke_model").map_err(|e| {
                ProviderError::new(format!("Failed to get invoke_model: {}", e), "bedrock")
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("modelId", model).unwrap();
            kwargs.set_item("body", body.as_str()).unwrap();
            kwargs.set_item("contentType", "application/json").unwrap();
            kwargs.set_item("accept", "application/json").unwrap();

            invoke
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "bedrock"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_bedrock_response(py_result.as_ref(py), model))
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

fn convert_py_bedrock_response(
    py_obj: &PyAny,
    model: &str,
) -> Result<ChatCompletion, ProviderError> {
    let body_str: String = py_obj
        .get_item("body")
        .map_err(|e| ProviderError::new(format!("Failed to get body: {}", e), "bedrock"))?
        .extract()
        .unwrap_or_default();

    // Parse JSON response - Bedrock returns streaming or non-streaming format
    // Non-streaming: {"type": "message", "content": [...]}
    let content = if body_str.contains("\"content\"") {
        body_str
            .split("\"content\"")
            .nth(1)
            .and_then(|s| s.split('[').nth(1))
            .and_then(|s| s.split(']').next())
            .map(|s| s.trim().trim_matches('"').to_string())
            .unwrap_or_else(|| body_str.clone())
    } else {
        body_str
    };

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

impl Default for BEDROCKProvider {
    fn default() -> Self {
        Self::new()
    }
}
