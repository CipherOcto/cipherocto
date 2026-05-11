// mistral — Mistral via reqwest (native_http, LiteLLM mode)

use super::{HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse, ProviderError};
use async_trait::async_trait;
use reqwest::Client;

pub struct MistralProvider {
    client: Client,
    api_base: String,
}

impl MistralProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.mistral.ai/v1".to_string(),
        }
    }
}

impl Default for MistralProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for MistralProvider {
    fn name(&self) -> &str {
        "mistral"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "mistral-large-latest", "mistral-medium-latest", "mistral-small-latest",
            "mistral-tiny", "mistral-nemo",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.api_base);

        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>()
        });

        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let data: MistralResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: data.id,
            object: data.object,
            created: data.created,
            model: data.model,
            choices: data.choices.into_iter().map(|c| {
                crate::shared_types::Choice::new(
                    c.index,
                    crate::shared_types::Message::new(c.message.role, c.message.content),
                    c.finish_reason,
                )
            }).collect(),
            usage: crate::shared_types::Usage::new(data.usage.prompt_tokens, data.usage.completion_tokens, data.usage.total_tokens),
        })
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let url = format!("{}/embeddings", self.api_base);

        let body = serde_json::json!({
            "input": request.input,
            "model": request.model
        });

        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let data: MistralEmbeddingsResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpEmbeddingResponse {
            object: "list".to_string(),
            data: data.data.into_iter().map(|e| crate::shared_types::Embedding {
                object: e.object,
                embedding: e.embedding,
                index: e.index,
            }).collect(),
            model: data.model,
            usage: crate::shared_types::Usage::new(data.usage.prompt_tokens, 0, data.usage.total_tokens),
        })
    }

    fn routing_weight(&self) -> u32 {
        6
    }
}

#[derive(serde::Deserialize)]
struct MistralResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<MistralChoice>,
    usage: MistralUsage,
}

#[derive(serde::Deserialize)]
struct MistralChoice {
    index: u32,
    message: MistralMessage,
    finish_reason: String,
}

#[derive(serde::Deserialize)]
struct MistralMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct MistralUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct MistralEmbeddingsResponse {
    object: String,
    data: Vec<MistralEmbedding>,
    model: String,
    usage: MistralUsage,
}

#[derive(serde::Deserialize)]
struct MistralEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}
