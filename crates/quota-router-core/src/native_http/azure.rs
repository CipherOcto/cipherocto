// azure — Azure OpenAI via reqwest (native_http, LiteLLM mode)

use super::{HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse, ProviderError};
use async_trait::async_trait;
use reqwest::Client;

pub struct AzureProvider {
    client: Client,
    api_base: String,
}

impl AzureProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: std::env::var("AZURE_OPENAI_BASE")
                .unwrap_or_else(|_| "https://YOUR_RESOURCE.openai.azure.com".to_string()),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }
}

impl Default for AzureProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for AzureProvider {
    fn name(&self) -> &str {
        "azure"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "gpt-4", "gpt-4-turbo", "gpt-4o",
            "gpt-35-turbo", "gpt-35-turbo-16k",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let deployment = request.model.clone();
        let url = format!("{}/openai/deployments/{}/chat/completions?api-version=2024-02-01", self.api_base, deployment);

        let body = serde_json::json!({
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>()
        });

        let resp = self.client
            .post(&url)
            .header("api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let data: AzureResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

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
        let deployment = request.model.clone();
        let url = format!("{}/openai/deployments/{}/embeddings?api-version=2024-02-01", self.api_base, deployment);

        let body = serde_json::json!({
            "input": request.input
        });

        let resp = self.client
            .post(&url)
            .header("api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let data: AzureEmbeddingsResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

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
        5
    }
}

#[derive(serde::Deserialize)]
struct AzureResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<AzureChoice>,
    usage: AzureUsage,
}

#[derive(serde::Deserialize)]
struct AzureChoice {
    index: u32,
    message: AzureMessage,
    finish_reason: String,
}

#[derive(serde::Deserialize)]
struct AzureMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct AzureUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct AzureEmbeddingsResponse {
    object: String,
    data: Vec<AzureEmbedding>,
    model: String,
    usage: AzureUsage,
}

#[derive(serde::Deserialize)]
struct AzureEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}
