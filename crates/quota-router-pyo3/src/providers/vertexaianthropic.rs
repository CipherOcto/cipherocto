// vertexaianthropic provider implementation
// Calls VertexAI Anthropic API via PyO3 using AsyncAnthropicVertex

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// vertexaianthropic provider implementation
pub struct VERTEXAIANTHROPICProvider {
    metadata: ProviderMetadata,
    project_id: Mutex<Option<String>>,
    region: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl VERTEXAIANTHROPICProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "vertexaianthropic".to_string(),
                documentation_url: "https://cloud.google.com/vertex-ai/generative-ai/docs/partner-models/use-claude".to_string(),
                env_api_key: "".to_string(), // Uses GCP ADC, not API key
                env_api_base: Some("VERTEXAI_ANTHROPIC_API_BASE".to_string()),
                api_base: None,
                features: ProviderFeatures {
                    supports_completion: true,
                    supports_completion_streaming: true,
                    supports_embedding: false,
                    supports_responses: false,
                    supports_list_models: false,
                    supports_batch: false,
                    supports_messages: true,
                },
            },
            project_id: Mutex::new(None),
            region: Mutex::new(None),
            client: Mutex::new(None),
        }
    }

    fn ensure_client(&self) -> Result<Py<PyAny>, ProviderError> {
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "vertexaianthropic"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let project_id = self.project_id.lock().unwrap();
        let region = self.region.lock().unwrap();

        // Get project_id from stored value or env var
        let proj_id: String = project_id
            .clone()
            .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
            .ok_or_else(|| {
                ProviderError::new(
                    "GOOGLE_CLOUD_PROJECT env var or project_id required for VertexAI",
                    "vertexaianthropic",
                )
            })?;

        let region_str: String = region
            .clone()
            .or_else(|| std::env::var("GOOGLE_CLOUD_LOCATION").ok())
            .unwrap_or_else(|| "us-central1".to_string());

        Python::with_gil(|py| {
            // Import anthropic package
            let anthropic = PyModule::import(py, "anthropic").map_err(|e| {
                ProviderError::new(
                    format!("Failed to import anthropic: {}", e),
                    "vertexaianthropic",
                )
            })?;

            // Get AsyncAnthropicVertex class
            let vertex_class = anthropic.getattr("AsyncAnthropicVertex").map_err(|e| {
                ProviderError::new(
                    format!("Failed to get AsyncAnthropicVertex: {}", e),
                    "vertexaianthropic",
                )
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("project_id", proj_id.as_str()).unwrap();
            kwargs.set_item("region", region_str.as_str()).unwrap();

            let client = vertex_class.call((), Some(kwargs)).map_err(|e| {
                ProviderError::new(
                    format!("Failed to create client: {}", e),
                    "vertexaianthropic",
                )
            })?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for VERTEXAIANTHROPICProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        // For vertexai, api_key parameter contains project_id
        *self.project_id.lock().unwrap() = Some(api_key.to_string());
        *self.region.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "anthropic") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("anthropic package not installed: {}", e)),
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
                "vertexaianthropic",
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

            let messages_attr = client_obj.getattr("messages").map_err(|e| {
                ProviderError::new(
                    format!("Failed to get messages: {}", e),
                    "vertexaianthropic",
                )
            })?;
            let create = messages_attr.getattr("create").map_err(|e| {
                ProviderError::new(format!("Failed to get create: {}", e), "vertexaianthropic")
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("messages", &py_messages).unwrap();
            kwargs.set_item("max_tokens", 1024).unwrap(); // Required param for Anthropic

            create
                .call((), Some(kwargs))
                .map_err(|e| {
                    ProviderError::new(format!("SDK call failed: {}", e), "vertexaianthropic")
                })
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_anthropic_response(py_result.as_ref(py), model))
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
    ) -> Result<crate::types::EmbeddingsResponse, ProviderError> {
        Err(ProviderError::new(
            "vertexaianthropic does not support embeddings",
            "vertexaianthropic",
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

fn convert_py_anthropic_response(
    py_obj: &PyAny,
    model: &str,
) -> Result<ChatCompletion, ProviderError> {
    let id: String = py_obj
        .get_item("id")
        .map_err(|e| ProviderError::new(format!("Failed to get id: {}", e), "vertexaianthropic"))?
        .extract()
        .unwrap_or_else(|_| format!("chatcmpl-{}", uuid::Uuid::new_v4()));

    let model_str: String = py_obj
        .get_item("model")
        .map_err(|e| {
            ProviderError::new(format!("Failed to get model: {}", e), "vertexaianthropic")
        })?
        .extract()
        .unwrap_or_else(|_| model.to_string());

    let content_blocks = py_obj.get_item("content").map_err(|e| {
        ProviderError::new(format!("Failed to get content: {}", e), "vertexaianthropic")
    })?;

    let choices: Vec<Choice> = if let Ok(list) = content_blocks.downcast::<pyo3::types::PyList>() {
        let mut result = Vec::new();
        for (i, block) in list.iter().enumerate() {
            let block_type: String = block
                .get_item("type")
                .map_err(|e| {
                    ProviderError::new(
                        format!("Failed to get block type: {}", e),
                        "vertexaianthropic",
                    )
                })?
                .extract()
                .unwrap_or_default();

            if block_type == "text" {
                let text: String = block
                    .get_item("text")
                    .map_err(|e| {
                        ProviderError::new(
                            format!("Failed to get text: {}", e),
                            "vertexaianthropic",
                        )
                    })?
                    .extract()
                    .unwrap_or_default();

                result.push(Choice::new(
                    i as u32,
                    Message::new("assistant".to_string(), text),
                    "stop".to_string(),
                ));
            }
        }
        if result.is_empty() {
            result.push(Choice::new(
                0,
                Message::new("assistant".to_string(), "".to_string()),
                "stop".to_string(),
            ));
        }
        result
    } else {
        return Err(ProviderError::new(
            "content is not a list",
            "vertexaianthropic",
        ));
    };

    let usage_obj = py_obj.get_item("usage").map_err(|e| {
        ProviderError::new(format!("Failed to get usage: {}", e), "vertexaianthropic")
    })?;

    let input_tokens: u32 = usage_obj
        .get_item("input_tokens")
        .map_err(|e| {
            ProviderError::new(
                format!("Failed to get input_tokens: {}", e),
                "vertexaianthropic",
            )
        })?
        .extract()
        .unwrap_or(0);
    let output_tokens: u32 = usage_obj
        .get_item("output_tokens")
        .map_err(|e| {
            ProviderError::new(
                format!("Failed to get output_tokens: {}", e),
                "vertexaianthropic",
            )
        })?
        .extract()
        .unwrap_or(0);

    Ok(ChatCompletion {
        id,
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model_str,
        choices,
        usage: crate::types::Usage::new(input_tokens, output_tokens, input_tokens + output_tokens),
    })
}

impl Default for VERTEXAIANTHROPICProvider {
    fn default() -> Self {
        Self::new()
    }
}
