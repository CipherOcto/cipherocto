// aleph_alpha — via OpenAI SDK via PyO3 (INTERNAL boundary #1 per RFC-0917)
use crate::py_bridge::PyBridgeError;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::prelude::*;
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
use pyo3::types::{PyDict, PyList};
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub struct AlephAlphaProvider {
    api_key: Option<String>,
    api_base: Option<String>,
}
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl Default for AlephAlphaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AlephAlphaProvider {
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
            let openai = PyModule::import(py, "openai")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to import: {}", e)))?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("api_key", api_key).unwrap();
            if let Some(base) = &self.api_base {
                kwargs.set_item("base_url", base).unwrap();
            }
            let client = openai
                .getattr("OpenAI")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get class: {}", e)))?
                .call((), Some(kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("Failed to create client: {}", e)))?;
            let py_messages: Vec<Py<PyDict>> = messages
                .iter()
                .map(|msg| {
                    let dict = PyDict::new(py);
                    dict.set_item("role", &msg.role).unwrap();
                    dict.set_item("content", &msg.content).unwrap();
                    dict.into()
                })
                .collect();
            let call_kwargs = PyDict::new(py);
            call_kwargs.set_item("model", model).unwrap();
            call_kwargs.set_item("messages", &py_messages).unwrap();
            let result = client
                .getattr("chat")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get chat: {}", e)))?
                .getattr("completions")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get completions: {}", e)))?
                .getattr("create")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get create: {}", e)))?
                .call((), Some(call_kwargs))
                .map_err(|e| PyBridgeError::PyError(format!("SDK call failed: {}", e)))?;
            let id: String = result
                .get_item("id")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get id: {}", e)))?
                .extract()
                .map_err(|e| PyBridgeError::PyError(format!("Failed to extract id: {}", e)))?;
            let model_str: String = result
                .get_item("model")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get model: {}", e)))?
                .extract()
                .unwrap_or_else(|_| model.to_string());
            let py_choices = result
                .get_item("choices")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get choices: {}", e)))?;
            let choices: Vec<crate::types::Choice> = if let Ok(list) =
                py_choices.downcast::<PyList>()
            {
                let mut r = Vec::new();
                for i in 0..list.len() {
                    let c = list.get_item(i).unwrap();
                    let m = c.get_item("message").map_err(|e| {
                        PyBridgeError::PyError(format!("Failed to get message: {}", e))
                    })?;
                    let role: String = m
                        .get_item("role")
                        .map_err(|e| PyBridgeError::PyError(format!("Failed to get role: {}", e)))?
                        .extract()
                        .map_err(|e| {
                            PyBridgeError::PyError(format!("Failed to extract role: {}", e))
                        })?;
                    let content: String = m
                        .get_item("content")
                        .map_err(|e| {
                            PyBridgeError::PyError(format!("Failed to get content: {}", e))
                        })?
                        .extract()
                        .map_err(|e| {
                            PyBridgeError::PyError(format!("Failed to extract content: {}", e))
                        })?;
                    let fr: String = c
                        .get_item("finish_reason")
                        .map_err(|e| {
                            PyBridgeError::PyError(format!("Failed to get finish_reason: {}", e))
                        })?
                        .extract()
                        .unwrap_or_else(|_| "stop".to_string());
                    r.push(crate::types::Choice::new(
                        i as u32,
                        crate::types::Message::new(role, content),
                        fr,
                    ));
                }
                r
            } else {
                return Err(PyBridgeError::PyError("choices is not a list".to_string()));
            };
            let usage_obj = result
                .get_item("usage")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get usage: {}", e)))?;
            let pt: u32 = usage_obj
                .get_item("prompt_tokens")
                .map_err(|e| PyBridgeError::PyError(format!("Failed to get prompt_tokens: {}", e)))?
                .extract()
                .unwrap_or(0);
            let ct: u32 = usage_obj
                .get_item("completion_tokens")
                .map_err(|e| {
                    PyBridgeError::PyError(format!("Failed to get completion_tokens: {}", e))
                })?
                .extract()
                .unwrap_or(0);
            let tt: u32 = usage_obj
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
                usage: crate::types::Usage::new(pt, ct, tt),
            })
        })
    }
}
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
impl crate::py_bridge::openai::PyBridgeProvider for AlephAlphaProvider {
    fn name(&self) -> &str {
        "aleph_alpha"
    }
    fn completion(
        &self,
        model: &str,
        messages: &[crate::types::Message],
    ) -> Result<crate::types::ChatCompletion, PyBridgeError> {
        self.completion(model, messages)
    }
}
