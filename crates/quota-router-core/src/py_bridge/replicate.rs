// replicate — Replicate Python SDK via PyO3 (INTERNAL boundary #1 per RFC-0917)
//
// This module calls the Replicate Python SDK via PyO3.
//
// Note: Replicate SDK does NOT support custom base_url - it uses Replicate's default API endpoint.
// The api_base field exists for interface consistency but is IGNORED by completion().
// This ensures all py_bridge providers have a uniform interface without special-casing Replicate.

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use super::PyBridgeProvider;
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::PyDict;

use crate::py_bridge::PyBridgeError;

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct ReplicateProvider {
    api_key: Option<String>,
    // Note: api_base is intentionally IGNORED — Replicate SDK doesn't support custom endpoints.
    // Field exists for interface consistency with other providers.
    #[allow(dead_code)]
    api_base: Option<String>,
}

#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for ReplicateProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplicateProvider {
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
            let replicate = PyModule::import(py, "replicate").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to import replicate: {}", e))
            })?;
            let client_class = replicate.getattr("Client").map_err(|e| {
                PyBridgeError::PyError(format!("Failed to get Client class: {}", e))
            })?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", api_key).unwrap();
            let client = client_class
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;
            let last_msg = messages
                .last()
                .ok_or_else(|| PyBridgeError::ProviderError("No messages provided".to_string()))?;
            let call_kwargs = PyDict::new(py);
            call_kwargs.set_item("model", model).unwrap();
            let input_dict = PyDict::new(py);
            input_dict.set_item("prompt", &last_msg.content).unwrap();
            call_kwargs.set_item("input", input_dict).unwrap();
            let result = client
                .getattr("run")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get run: {}", e)))?
                .call((), Some(call_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;
            let output: String = result.extract().unwrap_or_else(|_| "".to_string());
            let id = format!("replicate-{}", uuid::Uuid::new_v4());
            let choice = crate::types::Choice::new(
                0,
                crate::types::Message::new("assistant", output),
                "stop".to_string(),
            );
            Ok(crate::types::ChatCompletion {
                id,
                object: "chat.completion".to_string(),
                created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                model: model.to_string(),
                choices: vec![choice],
                usage: crate::types::Usage::new(0, 0, 0),
            })
        })
    }
}
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl PyBridgeProvider for ReplicateProvider {
    fn name(&self) -> &str {
        "replicate"
    }

    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }

    fn with_api_key(mut self: Box<Self>, key: String) -> Box<dyn PyBridgeProvider> {
        self.api_key = Some(key);
        self
    }

    fn with_api_base(mut self: Box<Self>, base: String) -> Box<dyn PyBridgeProvider> {
        self.api_base = Some(base);
        self
    }
}
