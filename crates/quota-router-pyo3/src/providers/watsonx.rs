// watsonx provider implementation
// Calls watsonx.ai SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// watsonx provider implementation
#[allow(clippy::upper_case_acronyms)]
pub struct WATSONXProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    client: Mutex<Option<Py<PyAny>>>,
}

impl WATSONXProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "watsonx".to_string(),
                documentation_url: "https://docs.watsonx.com/".to_string(),
                env_api_key: "WATSONX_API_KEY".to_string(),
                env_api_base: Some("WATSONX_BASE_URL".to_string()),
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
            .map_err(|e| ProviderError::new(format!("Lock error: {}", e), "watsonx"))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_key = self.api_key.lock().unwrap();
        let api_base = self.api_base.lock().unwrap();

        Python::with_gil(|py| {
            let ibm_cloud =
                PyModule::import(py, "ibm_cloud_sdk_core.authenticators").map_err(|e| {
                    ProviderError::new(
                        format!("Failed to import ibm_cloud_sdk_core: {}", e),
                        "watsonx",
                    )
                })?;

            let authenticator = ibm_cloud.getattr("IAMAuthenticator").map_err(|e| {
                ProviderError::new(format!("Failed to get IAMAuthenticator: {}", e), "watsonx")
            })?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new("No API key set", "watsonx"))?;

            let auth = authenticator.call1((key,)).map_err(|e| {
                ProviderError::new(format!("Failed to create authenticator: {}", e), "watsonx")
            })?;

            let watsonx = PyModule::import(py, "watsonx.language_model").map_err(|e| {
                ProviderError::new(
                    format!("Failed to import watsonx.language_model: {}", e),
                    "watsonx",
                )
            })?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("authenticator", auth).unwrap();

            if let Some(base) = api_base.as_ref() {
                kwargs.set_item("service_url", base.as_str()).unwrap();
            } else {
                kwargs
                    .set_item("service_url", "https://us-south.ml.cloud.ibm.com")
                    .unwrap();
            }

            let client = watsonx
                .getattr("WatsonxLLM")
                .map_err(|e| {
                    ProviderError::new(format!("Failed to get WatsonxLLM: {}", e), "watsonx")
                })?
                .call((), Some(kwargs))
                .map_err(|e| {
                    ProviderError::new(format!("Failed to create client: {}", e), "watsonx")
                })?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for WATSONXProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), ProviderError> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "watsonx.language_model") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "watsonx.language_model package not installed: {}",
                e
            )),
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
                "watsonx",
            ));
        }

        let client = self.ensure_client()?;

        // Build prompt from messages
        let prompt: String = messages
            .iter()
            .map(|msg| format!("{}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n");

        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            let generate = client_obj.getattr("generate").map_err(|e| {
                ProviderError::new(format!("Failed to get generate: {}", e), "watsonx")
            })?;

            let params = PyDict::new(py);
            params.set_item("prompt", &prompt).unwrap();
            params.set_item("model_id", model).unwrap();

            generate
                .call1((params,))
                .map_err(|e| ProviderError::new(format!("SDK call failed: {}", e), "watsonx"))
                .map(|obj| obj.into())
        })?;

        Python::with_gil(|py| convert_py_watsonx_response(py_result.as_ref(py), model))
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
            "watsonx does not support embeddings",
            "watsonx",
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

fn convert_py_watsonx_response(
    py_obj: &PyAny,
    model: &str,
) -> Result<ChatCompletion, ProviderError> {
    let results = py_obj
        .get_item("results")
        .map_err(|e| ProviderError::new(format!("Failed to get results: {}", e), "watsonx"))?;

    let first_result = results
        .get_item(0)
        .map_err(|e| ProviderError::new(format!("Failed to get first result: {}", e), "watsonx"))?;

    let text: String = first_result
        .get_item("generated_text")
        .map_err(|e| ProviderError::new(format!("Failed to get generated_text: {}", e), "watsonx"))?
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

impl Default for WATSONXProvider {
    fn default() -> Self {
        Self::new()
    }
}
