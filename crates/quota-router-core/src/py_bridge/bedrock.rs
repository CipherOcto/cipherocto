// bedrock — AWS Bedrock via boto3 Python SDK (INTERNAL boundary #1 per RFC-0917)
//
// This module calls AWS Bedrock via boto3 Python SDK through PyO3.
// It is called by python_sdk_entry (EXTERNAL boundary #2).

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::{PyDict, PyList};

use crate::py_bridge::PyBridgeError;

/// AWS Bedrock provider via boto3 SDK
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct BedrockProvider {
    api_key: Option<String>,
    api_base: Option<String>,
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for BedrockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl BedrockProvider {
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

    /// Call Bedrock completion via boto3 SDK
    pub fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| PyBridgeError::ProviderError("No API key set".to_string()))?;
        let api_base = self
            .api_base
            .as_deref()
            .unwrap_or("https://bedrock-runtime.us-east-1.amazonaws.com");

        Python::with_gil(|py| {
            // Import boto3
            let boto3 = PyModule::import(py, "boto3")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to import boto3: {}", e)))?;

            // Create bedrock-runtime client
            let kwargs = PyDict::new(py);
            kwargs.set_item("service_name", "bedrock-runtime").unwrap();
            kwargs.set_item("aws_access_key_id", api_key).unwrap();
            kwargs.set_item("region_name", "us-east-1").unwrap();
            kwargs.set_item("endpoint_url", api_base).unwrap();

            let client = boto3
                .getattr("client")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get client: {}", e)))?
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;

            // Build messages for Bedrock Converse API
            // Bedrock uses {"role": "user"|"assistant", "content": [{"text": "..."}]}
            let py_messages: Vec<Py<PyDict>> = messages
                .iter()
                .map(|msg| {
                    let dict = PyDict::new(py);
                    let role = if msg.role == "assistant" {
                        "assistant"
                    } else {
                        "user"
                    };
                    dict.set_item("role", role).unwrap();

                    // Content as list with text block
                    let content_list = PyList::new(py, [PyDict::new(py)]);
                    let content_dict = content_list
                        .get_item(0)
                        .unwrap()
                        .downcast::<PyDict>()
                        .unwrap();
                    content_dict.set_item("text", &msg.content).unwrap();
                    dict.set_item("content", content_list).unwrap();

                    dict.into()
                })
                .collect();

            // Call client.converse(model, messages)
            let call_kwargs = PyDict::new(py);
            call_kwargs.set_item("modelId", model).unwrap();
            call_kwargs.set_item("messages", &py_messages).unwrap();

            let result = client
                .getattr("converse")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get converse: {}", e)))?
                .call((), Some(call_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;

            // Convert to Rust type
            convert_response(result, py)
        })
    }
}

/// Convert Python Bedrock response to Rust ChatCompletion
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
fn convert_response(
    py_obj: &PyAny,
    _py: Python<'_>,
) -> Result<crate::types::ChatCompletion, PyBridgeError> {
    // Bedrock returns: { output, metrics, stopReason, ... }
    // We need to extract from output.message.content[0].text

    let output = py_obj
        .get_item("output")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get output: {}", e)))?;

    let message = output
        .get_item("message")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get message: {}", e)))?;

    let content = message
        .get_item("content")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get content: {}", e)))?;

    // Extract text from content block, defaulting to empty string if not found
    let content_text: String = if let Ok(content_block) = content.get_item(0) {
        if let Ok(text_obj) = content_block.get_item("text") {
            text_obj.extract().unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let stop_reason: String = py_obj
        .get_item("stopReason")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get stopReason: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "stop".to_string());

    let model_str = py_obj
        .get_item("modelId")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get modelId: {}", e)))?
        .extract()
        .unwrap_or_else(|_| "bedrock".to_string());

    // Generate a unique id
    let id = format!("bedrock-{}", uuid::Uuid::new_v4());

    let usage_obj = py_obj
        .get_item("usage")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get usage: {}", e)))?;
    let input_tokens: u32 = usage_obj
        .get_item("inputTokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get inputTokens: {}", e)))?
        .extract()
        .unwrap_or(0);
    let output_tokens: u32 = usage_obj
        .get_item("outputTokens")
        .map_err(|e| PyBridgeError::PyError(format!("Failed to get outputTokens: {}", e)))?
        .extract()
        .unwrap_or(0);

    let choice = crate::types::Choice::new(
        0,
        crate::types::Message::new("assistant", content_text),
        stop_reason,
    );

    Ok(crate::types::ChatCompletion {
        id,
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        model: model_str,
        choices: vec![choice],
        usage: crate::types::Usage::new(input_tokens, output_tokens, input_tokens + output_tokens),
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
impl PyBridgeProvider for BedrockProvider {
    fn name(&self) -> &str {
        "bedrock"
    }

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }
}
