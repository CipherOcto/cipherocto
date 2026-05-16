// sagemaker — AWS SageMaker via boto3 Python SDK (INTERNAL boundary #1 per RFC-0917)
//
// This module calls AWS SageMaker endpoints via boto3 Python SDK through PyO3.
// It is called by python_sdk_entry (EXTERNAL boundary #2).

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::{PyDict, PyList};

use crate::py_bridge::PyBridgeError;

/// AWS SageMaker provider via boto3 SDK
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct SageMakerProvider {
    api_key: Option<String>,
    api_base: Option<String>,
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for SageMakerProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SageMakerProvider {
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

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = Some(api_base);
        self
    }

    /// Call SageMaker endpoint via boto3 SDK
    pub fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        let _api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| PyBridgeError::ProviderError("No credentials set".to_string()))?;
        let api_base = self.api_base.as_deref().unwrap_or("");

        Python::with_gil(|py| {
            // Import boto3 and json
            let boto3 = PyModule::import(py, "boto3")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to import boto3: {}", e)))?;
            let json = PyModule::import(py, "json")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to import json: {}", e)))?;

            // Create SageMaker runtime client
            let kwargs = PyDict::new(py);
            kwargs
                .set_item("service_name", "sagemaker-runtime")
                .unwrap();
            kwargs.set_item("region_name", "us-east-1").unwrap();
            if !api_base.is_empty() {
                kwargs.set_item("endpoint_url", api_base).unwrap();
            }

            let client = boto3
                .getattr("client")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get client: {}", e)))?
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;

            // Build messages for SageMaker - convert to OpenAI-compatible format
            let py_messages: Vec<Py<PyDict>> = messages
                .iter()
                .map(|msg| {
                    let dict = PyDict::new(py);
                    dict.set_item("role", &msg.role).unwrap();
                    dict.set_item("content", &msg.content).unwrap();
                    dict.into()
                })
                .collect();

            // Format request body similar to OpenAI Chat Completions
            let body_dict = PyDict::new(py);
            body_dict.set_item("model", model).unwrap();
            body_dict.set_item("messages", &py_messages).unwrap();

            let body_str = json
                .getattr("dumps")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get dumps: {}", e)))?
                .call1((body_dict,))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to serialize request: {}", e)))?
                .extract::<String>()
                .map_err(|e| PyBridgeError::PyError(format!("Failed to extract body: {}", e)))?;

            // Call invoke_endpoint(EndpointName=..., Body=body, ContentType='application/json')
            let call_kwargs = PyDict::new(py);
            call_kwargs.set_item("EndpointName", model).unwrap();
            call_kwargs.set_item("Body", body_str).unwrap();
            call_kwargs
                .set_item("ContentType", "application/json")
                .unwrap();

            let result = client
                .getattr("invoke_endpoint")
                .map_err(|e| {
                    PyBridgeError::PyError(format!("Failed to get invoke_endpoint: {}", e))
                })?
                .call((), Some(call_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

            // Convert to Rust type
            convert_response(result, py)
        })
    }
}

/// Convert Python SageMaker response to Rust ChatCompletion
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
fn convert_response(
    py_obj: &PyAny,
    _py: Python<'_>,
) -> Result<crate::types::ChatCompletion, PyBridgeError> {
    // SageMaker returns: { Body: b'{"choices": [...], "usage": {...}}', ... }

    let body = py_obj
        .get_item("Body")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get Body: {}", e)))?;

    // Body is bytes, need to decode
    let body_bytes = body
        .str()
        .map_err(|e| PyBridgeError::PyError(format!("Failed to convert body to str: {}", e)))?
        .extract::<String>()
        .map_err(|e| PyBridgeError::PyError(format!("Failed to extract body: {}", e)))?;

    // Parse JSON
    let json = PyModule::import(_py, "json")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to import json: {}", e)))?;
    let parsed = json
        .getattr("loads")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get loads: {}", e)))?
        .call1((body_bytes,))
        .map_err(|e| PyBridgeError::PyError(format!("Failed to parse JSON: {}", e)))?;

    let id = format!("sagemaker-{}", uuid::Uuid::new_v4());

    let model_str: String = parsed
        .get_item("model")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get model: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "sagemaker".to_string());

    let py_choices = parsed
        .get_item("choices")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get choices: {}", e)))?;

    let choices: Vec<crate::types::Choice> = if let Ok(list) = py_choices.downcast::<PyList>() {
        let mut result = Vec::new();
        for i in 0..list.len() {
            let choice_obj = list.get_item(i).unwrap();
            let index = i as u32;

            let message_obj = choice_obj
                .get_item("message")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get message: {}", e)))?;
            let role: String = message_obj
                .get_item("role")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get role: {}", e)))?
                .extract()
                .map_err(|e| PyBridgeError::PyError(format!("Failed to extract role: {}", e)))?;
            let content: String = message_obj
                .get_item("content")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get content: {}", e)))?
                .extract()
                .map_err(|e| PyBridgeError::PyError(format!("Failed to extract content: {}", e)))?;

            let finish_reason: String = choice_obj
                .get_item("finish_reason")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get finish_reason: {}", e)))?
                .extract()
                .unwrap_or_else(|_| "stop".to_string());

            result.push(crate::types::Choice::new(
                index,
                crate::types::Message::new(role, content),
                finish_reason,
            ));
        }
        result
    } else {
        return Err(PyBridgeError::PyError("choices is not a list".to_string()));
    };

    let usage_obj = parsed
        .get_item("usage")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get usage: {}", e)))?;
    let prompt_tokens: u32 = usage_obj
        .get_item("prompt_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get prompt_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let completion_tokens: u32 = usage_obj
        .get_item("completion_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get completion_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let total_tokens: u32 = usage_obj
        .get_item("total_tokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get total_tokens: {}", e)))?
        .extract()
        .unwrap_or(0);

    Ok(crate::types::ChatCompletion {
        id,
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model_str,
        choices,
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
impl PyBridgeProvider for SageMakerProvider {
    fn name(&self) -> &str {
        "sagemaker"
    }

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }
}
