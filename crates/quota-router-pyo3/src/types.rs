// Type definitions for PyO3 bindings

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde::{Deserialize, Serialize};

/// Message for chat completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }
}

/// Usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    #[serde(rename = "prompt_tokens")]
    pub prompt_tokens: u32,
    #[serde(rename = "completion_tokens")]
    pub completion_tokens: u32,
    #[serde(rename = "total_tokens")]
    pub total_tokens: u32,
}

impl Usage {
    pub fn new(prompt_tokens: u32, completion_tokens: u32, total_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens,
        }
    }

    pub fn default() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        }
    }
}

/// Choice in chat completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    #[serde(rename = "finish_reason")]
    pub finish_reason: String,
}

impl Choice {
    pub fn new(index: u32, message: Message, finish_reason: impl Into<String>) -> Self {
        Self {
            index,
            message,
            finish_reason: finish_reason.into(),
        }
    }
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletion {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

impl ChatCompletion {
    pub fn new(
        id: impl Into<String>,
        model: impl Into<String>,
        choices: Vec<Choice>,
    ) -> Self {
        let id = id.into();
        let model = model.into();
        let total_tokens: u32 = choices
            .iter()
            .map(|c| c.message.content.len() as u32)
            .sum();

        Self {
            id,
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model,
            choices,
            usage: Usage::new(0, 0, total_tokens),
        }
    }

    pub fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);

        dict.set_item("id", &self.id)?;
        dict.set_item("object", &self.object)?;
        dict.set_item("created", self.created)?;
        dict.set_item("model", &self.model)?;

        // Convert choices to list of dicts
        let choices_list = PyList::new(
            py,
            self.choices.iter().map(|c| {
                let choice_dict = PyDict::new(py);
                choice_dict.set_item("index", c.index).unwrap();

                let message_dict = PyDict::new(py);
                message_dict.set_item("role", &c.message.role).unwrap();
                message_dict.set_item("content", &c.message.content).unwrap();
                choice_dict.set_item("message", message_dict).unwrap();

                choice_dict.set_item("finish_reason", &c.finish_reason).unwrap();
                choice_dict.to_object(py)
            }),
        );
        for (i, choice) in self.choices.iter().enumerate() {
            let choice_dict = PyDict::new(py);
            choice_dict.set_item("index", choice.index)?;

            let message_dict = PyDict::new(py);
            message_dict.set_item("role", &choice.message.role)?;
            message_dict.set_item("content", &choice.message.content)?;
            choice_dict.set_item("message", message_dict)?;

            choice_dict.set_item("finish_reason", &choice.finish_reason)?;
            choices_list.set_item(i, choice_dict)?;
        }
        dict.set_item("choices", choices_list)?;

        // Usage dict
        let usage_dict = PyDict::new(py);
        usage_dict.set_item("prompt_tokens", self.usage.prompt_tokens)?;
        usage_dict.set_item("completion_tokens", self.usage.completion_tokens)?;
        usage_dict.set_item("total_tokens", self.usage.total_tokens)?;
        dict.set_item("usage", usage_dict)?;

        Ok(dict.into())
    }
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: u32,
}

impl Embedding {
    pub fn new(index: u32, embedding: Vec<f32>) -> Self {
        Self {
            object: "embedding".to_string(),
            embedding,
            index,
        }
    }
}

/// Embeddings response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsResponse {
    pub object: String,
    pub data: Vec<Embedding>,
    pub model: String,
    pub usage: Usage,
}

impl EmbeddingsResponse {
    pub fn new(model: impl Into<String>, embeddings: Vec<Embedding>) -> Self {
        Self {
            object: "list".to_string(),
            data: embeddings,
            model: model.into(),
            usage: Usage::default(),
        }
    }
}

// PyO3 conversions for Message
impl<'source> FromPyObject<'source> for Message {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        let dict = ob.downcast::<PyDict>()?;

        let role: String = dict
            .get_item("role")
            .ok()
            .flatten()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Missing 'role' field"))?
            .extract()?;

        let content: String = dict
            .get_item("content")
            .ok()
            .flatten()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Missing 'content' field"))?
            .extract()?;

        Ok(Message { role, content })
    }
}
