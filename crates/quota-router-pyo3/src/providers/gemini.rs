// Gemini provider implementation
// Calls Google Gemini SDK via PyO3

use crate::exceptions::ProviderError;
use crate::providers::base::{LLMProvider, ProviderFeatures, ProviderMetadata};
use crate::types::{ChatCompletion, Choice, EmbeddingsResponse, Message};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3::PyErr;
use std::sync::Mutex;

/// Gemini provider implementation
pub struct GeminiProvider {
    metadata: ProviderMetadata,
    api_key: Mutex<Option<String>>,
    api_base: Mutex<Option<String>>,
    // Python client
    client: Mutex<Option<Py<PyAny>>>,
}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                name: "gemini".to_string(),
                documentation_url: "https://ai.google.dev/api/rest".to_string(),
                env_api_key: "GOOGLE_API_KEY".to_string(),
                env_api_base: Some("GOOGLE_BASE_URL".to_string()),
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

    /// Initialize the Gemini client using PyO3
    fn ensure_client(&self) -> Result<Py<PyAny>, PyErr> {
        let mut client_guard = self
            .client
            .lock()
            .map_err(|e| ProviderError::new_err(format!("Lock error: {}", e)))?;

        if client_guard.is_some() {
            return Ok(client_guard.clone().unwrap());
        }

        let api_key = self.api_key.lock().unwrap();

        Python::with_gil(|py| {
            // Import the Google GenAI SDK
            let genai = PyModule::import(py, "google.genai").map_err(|e| {
                ProviderError::new_err(format!("Failed to import google.genai: {}", e))
            })?;

            let client_class = genai
                .getattr("Client")
                .map_err(|e| ProviderError::new_err(format!("Failed to get Client: {}", e)))?;

            let key = api_key
                .as_ref()
                .ok_or_else(|| ProviderError::new_err("No API key set"))?;

            // Create client with api_key
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", key).unwrap();

            let client = client_class
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("Failed to create client: {}", e)))?;

            let client_py: Py<PyAny> = client.into();
            *client_guard = Some(client_py.clone());
            Ok(client_py)
        })
    }
}

impl LLMProvider for GeminiProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn init_client(&self, api_key: &str, api_base: Option<&str>) -> Result<(), PyErr> {
        *self.api_key.lock().unwrap() = Some(api_key.to_string());
        *self.api_base.lock().unwrap() = api_base.map(String::from);
        Ok(())
    }

    fn check_packages(&self) -> Result<(), String> {
        Python::with_gil(|py| match PyModule::import(py, "google.genai") {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("google.genai package not installed: {}", e)),
        })
    }

    fn completion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        // Don't support streaming in sync version
        if stream {
            return Err(ProviderError::new_err(
                "Streaming not supported in sync completion. Use acompletion() instead.",
            ));
        }

        // Get or create client
        let client = self.ensure_client()?;

        // Build content for Gemini - combine messages into a single text prompt
        // Gemini uses a different format: it takes a text prompt directly
        let prompt = {
            messages
                .iter()
                .map(|msg| format!("{}: {}", msg.role, msg.content))
                .collect::<Vec<_>>()
                .join("\n")
        };

        // Call the Python SDK
        let py_result: Py<PyAny> = Python::with_gil(|py| {
            let client_obj = client.as_ref(py);

            // Navigate: client.models.generate_content(model=model, contents=[prompt])
            let models = client_obj
                .getattr("models")
                .map_err(|e| ProviderError::new_err(format!("Failed to get models: {}", e)))?;
            let generate_content = models.getattr("generate_content").map_err(|e| {
                ProviderError::new_err(format!("Failed to get generate_content: {}", e))
            })?;

            // Build contents list
            let contents = PyList::new(py, vec![PyDict::new(py)]);
            let parts_dict = PyDict::new(py);
            parts_dict.set_item("text", &prompt).unwrap();
            let part_list = PyList::new(py, vec![parts_dict.to_object(py)]);
            let content_dict = PyDict::new(py);
            content_dict.set_item("parts", part_list).unwrap();
            content_dict.set_item("role", "user").unwrap();
            contents.set_item(0, content_dict).unwrap();

            // Call with keyword args
            let kwargs = PyDict::new(py);
            kwargs.set_item("model", model).unwrap();
            kwargs.set_item("contents", contents).unwrap();

            generate_content
                .call((), Some(kwargs))
                .map_err(|e| ProviderError::new_err(format!("SDK call failed: {}", e)))
                .map(|obj| obj.into())
        })?;

        // Convert Python response to Rust ChatCompletion
        Python::with_gil(|py| convert_py_gemini_response(py_result.as_ref(py), model))
    }

    async fn acompletion(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
    ) -> Result<ChatCompletion, PyErr> {
        self.completion(model, messages, stream)
    }

    fn embedding(&self, _input: &[String], _model: &str) -> Result<EmbeddingsResponse, PyErr> {
        Err(ProviderError::new_err("Gemini does not support embeddings"))
    }

    async fn aembedding(&self, input: &[String], model: &str) -> Result<EmbeddingsResponse, PyErr> {
        self.embedding(input, model)
    }
}

/// Convert Gemini response to Rust ChatCompletion
fn convert_py_gemini_response(py_obj: &PyAny, model: &str) -> Result<ChatCompletion, PyErr> {
    // Gemini returns: { candidates: [{content: {parts: [{text}], role}, finish_reason}], usage_metadata: {...} }

    // Get candidates list
    let candidates = py_obj
        .get_item("candidates")
        .map_err(|e| ProviderError::new_err(format!("Failed to get candidates: {}", e)))?
        .downcast::<pyo3::types::PyList>()
        .map_err(|_| ProviderError::new_err("candidates is not a list"))?;

    if candidates.is_empty() {
        return Err(ProviderError::new_err("No candidates in response"));
    }

    let candidate = candidates
        .get_item(0)
        .map_err(|e| ProviderError::new_err(format!("Failed to get candidate: {}", e)))?;
    let candidate = candidate
        .downcast::<pyo3::types::PyDict>()
        .map_err(|_| ProviderError::new_err("Candidate is not a dict"))?;

    let content = candidate
        .get_item("content")
        .map_err(|e| ProviderError::new_err(format!("Failed to get content: {}", e)))?
        .ok_or_else(|| ProviderError::new_err("content is None"))?;
    let parts = content
        .get_item("parts")
        .map_err(|e| ProviderError::new_err(format!("Failed to get parts: {}", e)))?
        .downcast::<pyo3::types::PyList>()
        .map_err(|_| ProviderError::new_err("Parts is not a list"))?;

    let text = if !parts.is_empty() {
        let part = parts
            .get_item(0)
            .map_err(|e| ProviderError::new_err(format!("Failed to get part: {}", e)))?;
        let part_dict = part
            .downcast::<pyo3::types::PyDict>()
            .map_err(|_| ProviderError::new_err("Part is not a dict"))?;
        match part_dict.get_item("text") {
            Ok(Some(text_obj)) => text_obj.extract::<String>().unwrap_or_default(),
            Ok(None) | Err(_) => String::new(),
        }
    } else {
        String::new()
    };

    let finish_reason = match candidate.get_item("finish_reason") {
        Ok(Some(fr_obj)) => fr_obj
            .extract::<String>()
            .unwrap_or_else(|_| "stop".to_string()),
        Ok(None) | Err(_) => "stop".to_string(),
    };

    // Get usage metadata
    let (prompt_tokens, completion_tokens, total_tokens) = match py_obj.get_item("usage_metadata") {
        Ok(usage) => (
            usage
                .get_item("prompt_token_count")
                .ok()
                .and_then(|v| v.extract::<u32>().ok())
                .unwrap_or(0),
            usage
                .get_item("candidates_token_count")
                .ok()
                .and_then(|v| v.extract::<u32>().ok())
                .unwrap_or(0),
            usage
                .get_item("total_token_count")
                .ok()
                .and_then(|v| v.extract::<u32>().ok())
                .unwrap_or(0),
        ),
        Err(_) => (0, 0, 0),
    };

    let choice = Choice::new(0, Message::new("assistant", text), finish_reason);

    Ok(ChatCompletion {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model.to_string(),
        choices: vec![choice],
        usage: crate::types::Usage::new(prompt_tokens, completion_tokens, total_tokens),
    })
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}
