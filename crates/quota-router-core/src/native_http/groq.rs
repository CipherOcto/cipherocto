// groq — Groq via reqwest (native_http, LiteLLM mode)

use super::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use reqwest::Client;

pub struct GroqProvider {
    client: Client,
    api_base: String,
}

impl GroqProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.groq.com/openai/v1".to_string(),
        }
    }
}

impl Default for GroqProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for GroqProvider {
    fn name(&self) -> &str {
        "groq"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "llama-3.1-70b-versatile",
            "llama-3.1-8b-instant",
            "mixtral-8x7b-32768",
            " llama-3.2-1b-preview",
            " llama-3.2-3b-preview",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.api_base);

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>()
        });

        // Function calling fields (RFC-0939)
        if let Some(tools) = &request.tools {
            body["tools"] = serde_json::to_value(tools).unwrap_or_default();
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
        }
        if let Some(response_format) = &request.response_format {
            body["response_format"] = serde_json::to_value(response_format).unwrap_or_default();
        }
        if let Some(seed) = request.seed {
            body["seed"] = serde_json::json!(seed);
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if resp.status() == 401 || resp.status() == 403 {
            return Err(ProviderError::AuthError(format!("HTTP {}", resp.status())));
        }
        if resp.status() == 429 {
            return Err(ProviderError::RateLimit("Rate limited".to_string()));
        }
        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}",
                resp.status()
            )));
        }

        let data: GroqResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: data.id,
            object: data.object,
            created: data.created,
            model: data.model,
            choices: data
                .choices
                .into_iter()
                .map(|c| {
                    crate::shared_types::Choice::new(
                        c.index,
                        crate::shared_types::Message::new(c.message.role, c.message.content),
                        c.finish_reason,
                    )
                })
                .collect(),
            usage: crate::shared_types::Usage::new(
                data.usage.prompt_tokens,
                data.usage.completion_tokens,
                data.usage.total_tokens,
            ),
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

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}",
                resp.status()
            )));
        }

        let data: GroqEmbeddingsResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpEmbeddingResponse {
            object: "list".to_string(),
            data: data
                .data
                .into_iter()
                .map(|e| crate::shared_types::Embedding {
                    object: e.object,
                    embedding: e.embedding,
                    index: e.index,
                })
                .collect(),
            model: data.model,
            usage: crate::shared_types::Usage::new(
                data.usage.prompt_tokens,
                0,
                data.usage.total_tokens,
            ),
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<StreamingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/chat/completions", base_url);

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>(),
            "stream": true
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &request.stop {
            body["stop"] = serde_json::json!(stop);
        }

        super::stream_openai_compatible(&self.client, &url, api_key, body).await
    }

    fn routing_weight(&self) -> u32 {
        7
    }
}

#[derive(serde::Deserialize)]
struct GroqResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<GroqChoice>,
    usage: GroqUsage,
}

#[derive(serde::Deserialize)]
struct GroqChoice {
    index: u32,
    message: GroqMessage,
    finish_reason: String,
}

#[derive(serde::Deserialize)]
struct GroqMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct GroqUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct GroqEmbeddingsResponse {
    object: String,
    data: Vec<GroqEmbedding>,
    model: String,
    usage: GroqUsage,
}

#[derive(serde::Deserialize)]
struct GroqEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}
