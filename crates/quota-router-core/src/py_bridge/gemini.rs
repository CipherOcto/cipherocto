// gemini — Google Gemini Python SDK via PyO3 (INTERNAL boundary #1 per RFC-0917)
//
// This module calls the official Google Gemini Python SDK via PyO3.
// It is called by python_sdk_entry (EXTERNAL boundary #2).
//
// Per RFC-0917 lines 220-221:
// "Gemini | `google.genai` Python SDK | Official Google Gemini SDK"

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::{PyDict, PyList};

use crate::py_bridge::PyBridgeError;

/// Gemini provider via official Python SDK
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct GeminiProvider {
    api_key: Option<String>,
    #[allow(dead_code)]
    api_base: Option<String>, // exists for API consistency; Gemini SDK uses default endpoint
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {
            api_key: None,
            api_base: None,
        }
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Call Gemini completion via Python SDK
    pub fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| PyBridgeError::ProviderError("No API key set".to_string()))?;

        Python::with_gil(|py| {
            // Import Google GenAI SDK
            let genai = PyModule::import(py, "google.genai").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to import google.genai: {}", e))
            })?;

            let client_class = genai.getattr("Client").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to get Client class: {}", e))
            })?;

            // Create client
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", api_key).unwrap();

            let client = client_class
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;

            // Build content for Gemini - combine messages into a single text prompt
            // Gemini takes a text prompt, not a messages array
            let prompt = messages
                .iter()
                .map(|msg| format!("{}: {}", msg.role, msg.content))
                .collect::<Vec<_>>()
                .join("\n");

            // Call client.models.generate_content(model, contents=[prompt])
            let models = client
                .getattr("models")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get models: {}", e)))?;
            let generate_content = models.getattr("generate_content").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to get generate_content: {}", e))
            })?;

            // Build contents list: [{"parts": [{"text": prompt}], "role": "user"}]
            let parts_dict = PyDict::new(py);
            parts_dict.set_item("text", &prompt).unwrap();
            let part_list = PyList::new(py, vec![parts_dict.to_object(py)]);
            let content_dict = PyDict::new(py);
            content_dict.set_item("parts", part_list).unwrap();
            content_dict.set_item("role", "user").unwrap();
            let contents = PyList::new(py, vec![content_dict.to_object(py)]);

            let call_kwargs = PyDict::new(py);
            call_kwargs.set_item("model", model).unwrap();
            call_kwargs.set_item("contents", contents).unwrap();

            let result = generate_content
                .call((), Some(call_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

            // Convert to Rust type
            convert_response(result, model, py)
        })
    }
}

/// Convert Python Gemini response to Rust ChatCompletion
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
fn convert_response(
    py_obj: &PyAny,
    model: &str,
    _py: Python<'_>,
) -> Result<crate::types::ChatCompletion, PyBridgeError> {
    // Gemini returns: { candidates: [{content: {parts: [{text}], role}, finish_reason}], usage_metadata: {...} }

    // Get candidates list
    let candidates = py_obj
        .get_item("candidates")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get candidates: {}", e)))?
        .downcast::<PyList>()
        .map_err(|_| PyBridgeError::PyError("candidates is not a list".to_string()))?;

    if candidates.is_empty() {
        return Err(PyBridgeError::PyError(
            "No candidates in response".to_string(),
        ));
    }

    let candidate = candidates
        .get_item(0)
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get candidate: {}", e)))?
        .downcast::<PyDict>()
        .map_err(|_| PyBridgeError::PyError("Candidate is not a dict".to_string()))?;

    let content = candidate
        .get_item("content")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get content: {}", e)))?
        .ok_or_else(|| PyBridgeError::PyError("content is None".to_string()))?;
    let parts = content
        .get_item("parts")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get parts: {}", e)))?
        .downcast::<PyList>()
        .map_err(|_| PyBridgeError::PyError("Parts is not a list".to_string()))?;

    let text = if !parts.is_empty() {
        let part = parts
            .get_item(0)
            .map_err(|e| PyBridgeError::PyError(format!("Failed to get part: {}", e)))?
            .downcast::<PyDict>()
            .map_err(|_| PyBridgeError::PyError("Part is not a dict".to_string()))?;
        match part.get_item("text") {
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

    let choice = crate::types::Choice::new(
        0,
        crate::types::Message::new("assistant", text),
        finish_reason,
    );

    Ok(crate::types::ChatCompletion {
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

/// Re-export as PyBridgeProvider trait for generic use
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub trait PyBridgeProvider: Send + Sync {
    fn name(&self) -> &str;
    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError>;
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl PyBridgeProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }
}
