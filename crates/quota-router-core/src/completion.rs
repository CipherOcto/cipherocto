// Completion functions for quota-router-core
// Provides sync and async completion/embedding functions

#![allow(clippy::should_implement_trait)]

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
    pub fn new(id: impl Into<String>, model: impl Into<String>, choices: Vec<Choice>) -> Self {
        let id = id.into();
        let model = model.into();
        let total_tokens: u32 = choices.iter().map(|c| c.message.content.len() as u32).sum();

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
}

/// Embedding data
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

/// Completion error
#[derive(Debug, Clone)]
pub struct CompletionError {
    pub message: String,
}

impl std::fmt::Display for CompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompletionError {}

/// Synchronous completion call
pub fn completion(
    model: String,
    messages: Vec<Message>,
) -> Result<ChatCompletion, CompletionError> {
    // In a real implementation, this would route to a provider
    // For now, return a mock response
    let choices: Vec<Choice> = messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            Choice::new(
                i as u32,
                Message::new("assistant", format!("Echo: {}", msg.content)),
                "stop",
            )
        })
        .collect();

    Ok(ChatCompletion::new(
        format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        model,
        choices,
    ))
}

/// Asynchronous completion call
pub async fn acompletion(
    model: String,
    messages: Vec<Message>,
) -> Result<ChatCompletion, CompletionError> {
    // For async, we just use tokio::task::spawn_blocking or similar
    // In real implementation, this would make HTTP calls
    let choices: Vec<Choice> = messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            Choice::new(
                i as u32,
                Message::new("assistant", format!("Async Echo: {}", msg.content)),
                "stop",
            )
        })
        .collect();

    Ok(ChatCompletion::new(
        format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        model,
        choices,
    ))
}

/// Synchronous embedding call
pub fn embedding(input: Vec<String>, model: String) -> Result<EmbeddingsResponse, CompletionError> {
    let embeddings: Vec<Embedding> = input
        .iter()
        .enumerate()
        .map(|(i, _)| {
            // Generate a simple mock embedding (in production, call the model)
            let embedding: Vec<f32> = (0..384).map(|_| 0.1).collect();
            Embedding::new(i as u32, embedding)
        })
        .collect();

    Ok(EmbeddingsResponse::new(model, embeddings))
}

/// Asynchronous embedding call
pub async fn aembedding(
    input: Vec<String>,
    model: String,
) -> Result<EmbeddingsResponse, CompletionError> {
    let embeddings: Vec<Embedding> = input
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let embedding: Vec<f32> = (0..384).map(|_| 0.1).collect();
            Embedding::new(i as u32, embedding)
        })
        .collect();

    Ok(EmbeddingsResponse::new(model, embeddings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion() {
        let messages = vec![Message::new("user", "Hello")];
        let result = completion("gpt-4".to_string(), messages);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Echo: Hello");
    }

    #[test]
    fn test_embedding() {
        let input = vec!["hello world".to_string()];
        let result = embedding(input, "text-embedding-3-small".to_string());

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].embedding.len(), 384);
    }

    #[tokio::test]
    async fn test_acompletion() {
        let messages = vec![Message::new("user", "Hello")];
        let result = acompletion("gpt-4".to_string(), messages).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.choices.len(), 1);
    }

    #[tokio::test]
    async fn test_aembedding() {
        let input = vec!["hello world".to_string()];
        let result = aembedding(input, "text-embedding-3-small".to_string()).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.data.len(), 1);
    }
}
