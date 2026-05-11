// Shared types without PyO3 dependencies — used by native_http and py_bridge
//
// This module contains core types (Message, Usage, Choice, Embedding) that are
// used by both native_http (reqwest-based providers) and py_bridge (PyO3-based providers).
// It has NO PyO3 dependencies so it's available in all feature configurations.

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
#[derive(Default)]
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

/// Chat completion chunk for streaming responses (OpenAI SSE format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

impl ChatCompletionChunk {
    pub fn new(id: impl Into<String>, model: impl Into<String>, choice: ChunkChoice) -> Self {
        Self {
            id: id.into(),
            object: "chat.completion.chunk".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: model.into(),
            choices: vec![choice],
        }
    }
}

/// Choice within a streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Message,
    #[serde(rename = "finish_reason", skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

impl ChunkChoice {
    pub fn new(index: u32, delta: Message) -> Self {
        Self {
            index,
            delta,
            finish_reason: None,
        }
    }

    pub fn with_finish_reason(index: u32, delta: Message, finish_reason: impl Into<String>) -> Self {
        Self {
            index,
            delta,
            finish_reason: Some(finish_reason.into()),
        }
    }
}
