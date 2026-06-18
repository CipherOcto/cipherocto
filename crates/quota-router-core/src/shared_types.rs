// Shared types without PyO3 dependencies — used by native_http and py_bridge
//
// This module contains core types (Message, Usage, Choice, Embedding) that are
// used by both native_http (reqwest-based providers) and py_bridge (PyO3-based providers).
// It has NO PyO3 dependencies so it's available in all feature configurations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Function Calling Types (RFC-0939)
// ============================================================================

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub r#type: String, // "function"
    pub function: FunctionDefinition,
}

/// Function definition within a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>, // JSON Schema
}

/// Tool call in assistant response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String, // "function"
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String, // JSON string
}

/// Tool choice for request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    String(String), // "none", "auto", "required"
    Specific(SpecificToolChoice),
}

/// Specific tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificToolChoice {
    pub r#type: String, // "function"
    pub function: FunctionName,
}

/// Function name for specific tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionName {
    pub name: String,
}

/// Response format specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    pub r#type: String, // "text", "json_object", "json_schema"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
}

/// Log probabilities for a single token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLogProb {
    pub token: String,
    pub logprob: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<HashMap<String, f64>>,
}

/// Log probabilities for a choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogProbs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<TokenLogProb>>,
}

// ============================================================================
// Core Types
// ============================================================================

/// Message for chat completion (RFC-0939: extended with function calling fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>, // Legacy format
}

impl Message {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            function_call: None,
        }
    }

    /// Create a message with tool calls (content may be null)
    pub fn with_tool_calls(role: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: role.into(),
            content: None,
            name: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            function_call: None,
        }
    }

    /// Create a tool response message
    pub fn tool_response(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            name: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            function_call: None,
        }
    }
}

/// Usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Choice in chat completion (RFC-0939: extended with logprobs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    #[serde(rename = "finish_reason")]
    pub finish_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<LogProbs>,
}

impl Choice {
    pub fn new(index: u32, message: Message, finish_reason: impl Into<String>) -> Self {
        Self {
            index,
            message,
            finish_reason: finish_reason.into(),
            logprobs: None,
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

/// Choice within a streaming chunk (RFC-0939: extended with tool_calls)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Message,
    #[serde(rename = "finish_reason", skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<LogProbs>,
}

impl ChunkChoice {
    pub fn new(index: u32, delta: Message) -> Self {
        Self {
            index,
            delta,
            finish_reason: None,
            logprobs: None,
        }
    }

    pub fn with_finish_reason(
        index: u32,
        delta: Message,
        finish_reason: impl Into<String>,
    ) -> Self {
        Self {
            index,
            delta,
            finish_reason: Some(finish_reason.into()),
            logprobs: None,
        }
    }
}
